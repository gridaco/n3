use std::collections::BTreeSet;

use ::gltf::{
    Semantic,
    accessor::{DataType, Dimensions},
    mesh::Mode,
};

use super::*;

const MAX_VERTICES: usize = 2_000_000;
pub(super) const MAX_INDICES: usize = 12_000_000;

pub(super) fn meshes(
    document: &::gltf::Document,
    buffers: &[Vec<u8>],
    materials: &[Material],
) -> Result<Vec<Mesh>> {
    let mut total_vertices = 0usize;
    let mut total_indices = 0usize;
    let mut result = Vec::new();
    for mesh in document.meshes() {
        let mut primitives = Vec::new();
        for primitive in mesh.primitives() {
            let positions = primitive
                .get(&Semantic::Positions)
                .ok_or("A mesh primitive is missing POSITION.")?;
            require_float(&positions, Dimensions::Vec3)?;
            let count = positions.count();
            total_vertices = total_vertices
                .checked_add(count)
                .filter(|total| *total <= MAX_VERTICES)
                .ok_or("Scene exceeds the decoded vertex budget.")?;
            for (_, accessor) in primitive.attributes() {
                if accessor.count() != count {
                    return Err("Primitive attributes must have matching vertex counts.".into());
                }
            }
            let positions = accessors::vectors::<3>(positions, buffers)?;
            let mut vertices: Vec<_> = positions
                .into_iter()
                .map(|position| SceneVertex {
                    position: units::position_cm(position),
                    normal: [0., 0., 1.],
                    tangent: [0., 0., 0., 1.],
                    uv0: [0.; 2],
                    uv1: [0.; 2],
                    color: [1.; 4],
                })
                .collect();
            let flat_normals = primitive.get(&Semantic::Normals).is_none();
            if let Some(accessor) = primitive.get(&Semantic::Normals) {
                require_float(&accessor, Dimensions::Vec3)?;
                for (vertex, normal) in vertices
                    .iter_mut()
                    .zip(accessors::vectors::<3>(accessor, buffers)?)
                {
                    let normal = glam::Vec3::from_array(normal);
                    if normal.length_squared() < 1e-20 {
                        return Err("Vertex normals must have nonzero length.".into());
                    }
                    vertex.normal = normal.normalize().to_array();
                }
            }
            if let Some(accessor) = primitive.get(&Semantic::Tangents) {
                require_float(&accessor, Dimensions::Vec4)?;
                for (vertex, tangent) in vertices
                    .iter_mut()
                    .zip(accessors::vectors::<4>(accessor, buffers)?)
                {
                    if tangent[3] != 1. && tangent[3] != -1. {
                        return Err("Tangent handedness must be +1 or -1.".into());
                    }
                    let direction = glam::Vec3::from_slice(&tangent[..3]);
                    if direction.length_squared() < 1e-20 {
                        return Err("Vertex tangents must have nonzero length.".into());
                    }
                    let direction = direction.normalize();
                    vertex.tangent = [direction.x, direction.y, direction.z, tangent[3]];
                }
            }
            for set in 0..=1 {
                if let Some(accessor) = primitive.get(&Semantic::TexCoords(set)) {
                    require_attribute(&accessor, Dimensions::Vec2, true)?;
                    for (vertex, uv) in vertices
                        .iter_mut()
                        .zip(accessors::vectors::<2>(accessor, buffers)?)
                    {
                        if set == 0 {
                            vertex.uv0 = uv;
                        } else {
                            vertex.uv1 = uv;
                        }
                    }
                }
            }
            if let Some(accessor) = primitive.get(&Semantic::Colors(0)) {
                let dimensions = accessor.dimensions();
                if !matches!(dimensions, Dimensions::Vec3 | Dimensions::Vec4) {
                    return Err("Vertex colors require three or four components.".into());
                }
                require_attribute(&accessor, dimensions, true)?;
                let components = dimensions.multiplicity();
                for (vertex, color) in vertices
                    .iter_mut()
                    .zip(accessors::read(accessor, buffers)?.chunks_exact(components))
                {
                    if color.iter().any(|value| !(0.0..=1.0).contains(value)) {
                        return Err("Vertex colors must be between zero and one.".into());
                    }
                    vertex.color[..components].copy_from_slice(
                        &color.iter().map(|value| *value as f32).collect::<Vec<_>>(),
                    );
                }
            }
            let indices: Vec<u32> = if let Some(accessor) = primitive.indices() {
                if accessor.dimensions() != Dimensions::Scalar
                    || accessor.normalized()
                    || !matches!(
                        accessor.data_type(),
                        DataType::U8 | DataType::U16 | DataType::U32
                    )
                {
                    return Err("Primitive indices must be unsigned scalar integers.".into());
                }
                accessors::read(accessor, buffers)?
                    .into_iter()
                    .map(|value| value as u32)
                    .collect()
            } else {
                (0..count as u32).collect()
            };
            if indices.iter().any(|index| *index as usize >= count) {
                return Err("Primitive index exceeds the vertex count.".into());
            }
            let (topology, indices) = expand_indices(primitive.mode(), indices)?;
            total_indices = total_indices
                .checked_add(indices.len())
                .filter(|total| *total <= MAX_INDICES)
                .ok_or("Scene exceeds the index budget.")?;
            let material = primitive.material().index().unwrap_or(materials.len() - 1);
            for texture in [
                materials[material].base_color_texture,
                materials[material].metallic_roughness_texture,
                materials[material].normal_texture,
                materials[material].occlusion_texture,
                materials[material].emissive_texture,
            ]
            .into_iter()
            .flatten()
            {
                if primitive
                    .get(&Semantic::TexCoords(texture.tex_coord))
                    .is_none()
                {
                    return Err(format!(
                        "A material uses missing TEXCOORD_{}.",
                        texture.tex_coord
                    ));
                }
            }
            let mut morphs = Vec::new();
            for target in primitive.morph_targets() {
                if morphs.len() >= 64 {
                    return Err("A primitive supports at most 64 morph targets.".into());
                }
                let delta = |accessor: Option<::gltf::Accessor<'_>>| -> Result<Vec<[f32; 3]>> {
                    if let Some(accessor) = accessor {
                        require_float(&accessor, Dimensions::Vec3)?;
                        if accessor.count() != count {
                            return Err("Morph target vertex count does not match POSITION.".into());
                        }
                        accessors::vectors::<3>(accessor, buffers)
                    } else {
                        Ok(Vec::new())
                    }
                };
                let positions = delta(target.positions())?
                    .into_iter()
                    .map(units::position_cm)
                    .collect();
                let normals = delta(target.normals())?;
                let tangents = delta(target.tangents())?;
                if (!normals.is_empty() && flat_normals)
                    || (!tangents.is_empty() && primitive.get(&Semantic::Tangents).is_none())
                {
                    return Err(
                        "Morph normal/tangent deltas require the corresponding base attribute."
                            .into(),
                    );
                }
                morphs.push(Morph {
                    positions,
                    normals,
                    tangents,
                });
            }
            let joint_sets: BTreeSet<_> = primitive
                .attributes()
                .filter_map(|(semantic, _)| match semantic {
                    Semantic::Joints(set) | Semantic::Weights(set) => Some(set),
                    _ => None,
                })
                .collect();
            if joint_sets.len() > 8 {
                return Err("A primitive supports at most eight joint influence sets.".into());
            }
            let mut influences = if joint_sets.is_empty() {
                Vec::new()
            } else {
                // Exact capacity keeps retained allocation within the preflight budget.
                (0..count)
                    .map(|_| Vec::with_capacity(joint_sets.len()))
                    .collect()
            };
            for (expected, set) in joint_sets.into_iter().enumerate() {
                if set as usize != expected {
                    return Err("Joint influence sets must be consecutive from JOINTS_0.".into());
                }
                let joints = primitive
                    .get(&Semantic::Joints(set))
                    .ok_or("WEIGHTS attribute has no JOINTS counterpart.")?;
                let weights = primitive
                    .get(&Semantic::Weights(set))
                    .ok_or("JOINTS attribute has no WEIGHTS counterpart.")?;
                if joints.dimensions() != Dimensions::Vec4
                    || joints.normalized()
                    || !matches!(joints.data_type(), DataType::U8 | DataType::U16)
                {
                    return Err("JOINTS must contain four unsigned byte/short indices.".into());
                }
                require_attribute(&weights, Dimensions::Vec4, true)?;
                let joint_values = accessors::read(joints, buffers)?;
                let weight_values = accessors::vectors::<4>(weights, buffers)?;
                for (index, values) in joint_values.as_chunks::<4>().0.iter().enumerate() {
                    let weights = weight_values[index];
                    if weights.iter().any(|value| *value < 0.) {
                        return Err("Skin weights must be nonnegative.".into());
                    }
                    influences[index].push((std::array::from_fn(|i| values[i] as u16), weights));
                }
            }
            for sets in &mut influences {
                let sum: f64 = sets
                    .iter()
                    .flat_map(|(_, weights)| weights)
                    .map(|value| *value as f64)
                    .sum();
                if sum <= 0. || !sum.is_finite() {
                    return Err("A skinned vertex must have positive total joint weight.".into());
                }
                for (_, weights) in sets {
                    for weight in weights {
                        *weight = (*weight as f64 / sum) as f32;
                    }
                }
            }
            primitives.push(Primitive {
                topology,
                material,
                vertices,
                indices: indices.into(),
                morphs,
                influences,
                flat_normals,
            });
        }
        let targets = primitives
            .first()
            .map_or(0, |primitive| primitive.morphs.len());
        if primitives
            .iter()
            .any(|primitive| primitive.morphs.len() != targets)
        {
            return Err("Every primitive in a mesh must have the same morph target count.".into());
        }
        let weights = mesh
            .weights()
            .map_or_else(|| vec![0.; targets], <[f32]>::to_vec);
        if weights.len() != targets || weights.iter().any(|value| !value.is_finite()) {
            return Err("Mesh morph weights must match its target count and be finite.".into());
        }
        result.push(Mesh {
            name: mesh
                .name()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Mesh {}", mesh.index() + 1)),
            primitives,
            weights,
        });
    }
    Ok(result)
}

pub(super) fn require_float(accessor: &::gltf::Accessor<'_>, dimensions: Dimensions) -> Result<()> {
    if accessor.data_type() != DataType::F32
        || accessor.dimensions() != dimensions
        || accessor.normalized()
    {
        return Err(format!(
            "Expected a non-normalized FLOAT {dimensions:?} accessor."
        ));
    }
    Ok(())
}

fn require_attribute(
    accessor: &::gltf::Accessor<'_>,
    dimensions: Dimensions,
    normalized_integer: bool,
) -> Result<()> {
    if accessor.dimensions() != dimensions
        || !matches!(
            accessor.data_type(),
            DataType::F32 | DataType::U8 | DataType::U16
        )
        || (accessor.data_type() != DataType::F32 && normalized_integer && !accessor.normalized())
        || (accessor.data_type() == DataType::F32 && accessor.normalized())
    {
        return Err(format!(
            "Invalid {dimensions:?} attribute component type or normalization."
        ));
    }
    Ok(())
}

fn expand_indices(mode: Mode, source: Vec<u32>) -> Result<(Topology, Vec<u32>)> {
    let count = source.len();
    let _ = topology_count(mode, count)?;
    let (topology, indices) = match mode {
        Mode::Points if count >= 1 => (Topology::Points, source),
        Mode::Lines if count >= 2 && count.is_multiple_of(2) => (Topology::Lines, source),
        Mode::Triangles if count >= 3 && count.is_multiple_of(3) => (Topology::Triangles, source),
        Mode::LineStrip | Mode::LineLoop if count >= 2 => {
            let mut indices: Vec<_> = source
                .windows(2)
                .flat_map(|pair| pair.iter().copied())
                .collect();
            if mode == Mode::LineLoop {
                indices.extend([source[count - 1], source[0]]);
            }
            (Topology::Lines, indices)
        }
        Mode::TriangleStrip if count >= 3 => (
            Topology::Triangles,
            (0..count - 2)
                .flat_map(|i| {
                    if i % 2 == 0 {
                        [source[i], source[i + 1], source[i + 2]]
                    } else {
                        [source[i + 1], source[i], source[i + 2]]
                    }
                })
                .collect(),
        ),
        Mode::TriangleFan if count >= 3 => (
            Topology::Triangles,
            (1..count - 1)
                .flat_map(|i| [source[0], source[i], source[i + 1]])
                .collect(),
        ),
        _ => return Err("Primitive index count does not form its declared topology.".into()),
    };
    if indices.len() > MAX_INDICES {
        return Err("Expanded primitive exceeds the index budget.".into());
    }
    Ok((topology, indices))
}

pub(super) fn topology_count(mode: Mode, count: usize) -> Result<(Topology, usize)> {
    let (topology, expanded) = match mode {
        Mode::Points if count >= 1 => (Topology::Points, Some(count)),
        Mode::Lines if count >= 2 && count.is_multiple_of(2) => (Topology::Lines, Some(count)),
        Mode::Triangles if count >= 3 && count.is_multiple_of(3) => {
            (Topology::Triangles, Some(count))
        }
        Mode::LineStrip if count >= 2 => (Topology::Lines, (count - 1).checked_mul(2)),
        Mode::LineLoop if count >= 2 => (Topology::Lines, count.checked_mul(2)),
        Mode::TriangleStrip | Mode::TriangleFan if count >= 3 => {
            (Topology::Triangles, (count - 2).checked_mul(3))
        }
        _ => return Err("Primitive index count does not form its declared topology.".into()),
    };
    let expanded = expanded
        .filter(|count| *count <= MAX_INDICES)
        .ok_or("Expanded primitive exceeds the index budget.")?;
    Ok((topology, expanded))
}
