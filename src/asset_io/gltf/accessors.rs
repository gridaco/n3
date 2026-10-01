//! Bounded accessor decoding, including sparse zero bases and matrix padding.
//! glTF's schema validator checks references; these checks own byte spans and
//! decoded finite values before any data reaches geometry or animation.
use ::gltf::accessor::{DataType, Dimensions};

use super::Result;

const MAX_VALUES: usize = 24_000_000;

pub(super) fn read(accessor: ::gltf::Accessor<'_>, buffers: &[Vec<u8>]) -> Result<Vec<f64>> {
    let count = accessor.count();
    let components = accessor.dimensions().multiplicity();
    let values = count
        .checked_mul(components)
        .filter(|size| *size <= MAX_VALUES)
        .ok_or("Accessor exceeds the decoded-value budget.")?;
    if count == 0 {
        return Err("Accessors must contain at least one element.".into());
    }
    let component_bytes = accessor.data_type().size();
    if accessor.normalized()
        && !matches!(
            accessor.data_type(),
            DataType::I8 | DataType::U8 | DataType::I16 | DataType::U16
        )
    {
        return Err("Only byte/short integer accessors may be normalized.".into());
    }
    let columns = match accessor.dimensions() {
        Dimensions::Mat2 => 2,
        Dimensions::Mat3 => 3,
        Dimensions::Mat4 => 4,
        _ => 1,
    };
    let rows = components / columns;
    let column_stride = if columns > 1 {
        (rows * component_bytes).div_ceil(4) * 4
    } else {
        rows * component_bytes
    };
    let element_bytes = columns * column_stride;
    // Columns and successive matrices retain four-byte alignment, but glTF
    // permits the final column's trailing padding to be absent at a view's end.
    let element_extent = (columns - 1) * column_stride + rows * component_bytes;
    let mut output = vec![0.; values];
    if let Some(view) = accessor.view() {
        if view
            .stride()
            .is_some_and(|stride| !(4..=252).contains(&stride) || !stride.is_multiple_of(4))
        {
            return Err(
                "Interleaved accessors require a 4–252 byte stride divisible by four.".into(),
            );
        }
        let alignment = if columns > 1 { 4 } else { component_bytes };
        if !view
            .offset()
            .checked_add(accessor.offset())
            .ok_or("Accessor offset overflow.")?
            .is_multiple_of(alignment)
        {
            return Err("Accessor offset is not aligned within its buffer.".into());
        }
        let stride = view.stride().unwrap_or(element_bytes);
        let bytes = checked_view(&view, buffers)?;
        validate_span(
            bytes,
            accessor.offset(),
            count,
            stride,
            element_extent,
            component_bytes,
        )?;
        for index in 0..count {
            decode_element(
                &mut output[index * components..(index + 1) * components],
                bytes,
                accessor.offset() + index * stride,
                rows,
                column_stride,
                accessor.data_type(),
                accessor.normalized(),
            )?;
        }
    } else if accessor.offset() != 0 {
        return Err("An accessor without a buffer view cannot have a byte offset.".into());
    }
    if let Some(sparse) = accessor.sparse() {
        if sparse.count() == 0 || sparse.count() > count {
            return Err("Invalid sparse accessor count.".into());
        }
        let indices = sparse.indices();
        let view = indices.view();
        if view.stride().is_some() {
            return Err("Sparse index views cannot be interleaved.".into());
        }
        let bytes = checked_view(&view, buffers)?;
        let size = indices.index_type().size();
        if !view
            .offset()
            .checked_add(indices.offset())
            .ok_or("Sparse index offset overflow.")?
            .is_multiple_of(size)
        {
            return Err("Sparse indices are not aligned within their buffer.".into());
        }
        validate_span(bytes, indices.offset(), sparse.count(), size, size, size)?;
        let sparse_values = sparse.values();
        let value_view = sparse_values.view();
        if value_view.stride().is_some() {
            return Err("Sparse value views cannot be interleaved.".into());
        }
        let replacement = checked_view(&value_view, buffers)?;
        if !value_view
            .offset()
            .checked_add(sparse_values.offset())
            .ok_or("Sparse value offset overflow.")?
            .is_multiple_of(if columns > 1 { 4 } else { component_bytes })
        {
            return Err("Sparse values are not aligned within their buffer.".into());
        }
        validate_span(
            replacement,
            sparse_values.offset(),
            sparse.count(),
            element_bytes,
            element_extent,
            component_bytes,
        )?;
        let mut previous = None;
        for index in 0..sparse.count() {
            let offset = indices.offset() + index * size;
            let selected = match size {
                1 => bytes[offset] as usize,
                2 => u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize,
                4 => u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize,
                _ => unreachable!(),
            };
            if selected >= count || previous.is_some_and(|last| selected <= last) {
                return Err(
                    "Sparse indices must be increasing, unique, and within the accessor.".into(),
                );
            }
            previous = Some(selected);
            decode_element(
                &mut output[selected * components..(selected + 1) * components],
                replacement,
                sparse_values.offset() + index * element_bytes,
                rows,
                column_stride,
                accessor.data_type(),
                accessor.normalized(),
            )?;
        }
    }
    Ok(output)
}

pub(super) fn vectors<const N: usize>(
    accessor: ::gltf::Accessor<'_>,
    buffers: &[Vec<u8>],
) -> Result<Vec<[f32; N]>> {
    if accessor.dimensions().multiplicity() != N {
        return Err(format!("Expected an accessor with {N} components."));
    }
    Ok(read(accessor, buffers)?
        .as_chunks::<N>()
        .0
        .iter()
        .map(|values| std::array::from_fn(|index| values[index] as f32))
        .collect())
}

pub(super) fn checked_view<'a>(
    view: &::gltf::buffer::View<'_>,
    buffers: &'a [Vec<u8>],
) -> Result<&'a [u8]> {
    let bytes = buffers
        .get(view.buffer().index())
        .ok_or("Missing buffer.")?;
    let end = view
        .offset()
        .checked_add(view.length())
        .ok_or("Buffer view span overflow.")?;
    bytes
        .get(view.offset()..end)
        .ok_or_else(|| "Buffer view exceeds its declared buffer.".into())
}

fn validate_span(
    bytes: &[u8],
    offset: usize,
    count: usize,
    stride: usize,
    size: usize,
    alignment: usize,
) -> Result<()> {
    if stride < size || !stride.is_multiple_of(alignment) || !offset.is_multiple_of(alignment) {
        return Err("Accessor offset/stride does not match its element size or alignment.".into());
    }
    let end = count
        .checked_sub(1)
        .and_then(|last| last.checked_mul(stride))
        .and_then(|end| end.checked_add(offset))
        .and_then(|end| end.checked_add(size))
        .ok_or("Accessor byte span overflow.")?;
    if end > bytes.len() {
        return Err("Accessor exceeds its buffer view.".into());
    }
    Ok(())
}

fn decode_element(
    output: &mut [f64],
    bytes: &[u8],
    start: usize,
    rows: usize,
    column_stride: usize,
    ty: DataType,
    normalized: bool,
) -> Result<()> {
    for (component, value) in output.iter_mut().enumerate() {
        let offset = start + component / rows * column_stride + component % rows * ty.size();
        let raw = &bytes[offset..offset + ty.size()];
        *value = match ty {
            DataType::F32 => f32::from_le_bytes(raw.try_into().unwrap()) as f64,
            DataType::U8 => {
                if normalized {
                    raw[0] as f64 / 255.
                } else {
                    raw[0] as f64
                }
            }
            DataType::I8 => {
                if normalized {
                    (raw[0] as i8 as f64 / 127.).max(-1.)
                } else {
                    raw[0] as i8 as f64
                }
            }
            DataType::U16 => {
                let v = u16::from_le_bytes(raw.try_into().unwrap()) as f64;
                if normalized { v / 65535. } else { v }
            }
            DataType::I16 => {
                let v = i16::from_le_bytes(raw.try_into().unwrap()) as f64;
                if normalized { (v / 32767.).max(-1.) } else { v }
            }
            DataType::U32 => u32::from_le_bytes(raw.try_into().unwrap()) as f64,
        };
        if !value.is_finite() {
            return Err("Accessor contains a nonfinite numeric value.".into());
        }
    }
    Ok(())
}
