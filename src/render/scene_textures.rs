//! Immutable PBR material resources. A single decoded image may be used in both
//! color (SRGB) and data (linear) roles; never infer color space from its filename.
use super::{GpuMaterial, MaterialUniform, Result, UvUniform, finite_values};
use crate::scene::{self, AlphaMode, Filter, TextureInfo, Wrap};
use std::collections::BTreeMap;
use wgpu::util::DeviceExt;

pub(super) fn materials(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    definitions: &[scene::Material],
    source_images: &[scene::Image],
    source_textures: &[scene::Texture],
) -> Result<Vec<GpuMaterial>> {
    // Validate the complete immutable candidate before issuing GPU allocations.
    // The same image may legitimately need separate SRGB and linear storage.
    let mut image_roles = std::collections::BTreeSet::new();
    let mut texture_bytes = 0_u64;
    if definitions.len() > 4096 {
        return Err("Scene exceeds the 4096 GPU material budget.".into());
    }
    for material in definitions {
        let uniform = material_uniform(material);
        if !finite_values(bytemuck::cast_slice(std::slice::from_ref(&uniform))) {
            return Err("A material exceeds the renderer's numeric range.".into());
        }
        for (slot, info) in material_infos(material).into_iter().enumerate() {
            let Some(info) = info else { continue };
            if info.tex_coord > 1 {
                return Err("The viewport supports texture coordinate sets 0 and 1.".into());
            }
            let texture = source_textures
                .get(info.texture)
                .ok_or("A material references a missing texture.")?;
            let key = (texture.image, slot == 0 || slot == 4);
            if image_roles.insert(key) {
                let image = source_images
                    .get(texture.image)
                    .ok_or("A texture references a missing image.")?;
                texture_bytes = texture_bytes
                    .checked_add(image_budget(device, image)?)
                    .filter(|bytes| *bytes <= 512 * 1024 * 1024)
                    .ok_or("Scene exceeds the 512 MiB GPU texture budget.")?;
            }
        }
    }
    let fallback = scene::Image {
        name: "default white".into(),
        width: 1,
        height: 1,
        rgba8: vec![255; 4],
    };
    let white = upload(device, queue, &fallback, false)?;
    let default_sampler = scene::Sampler {
        wrap_s: Wrap::Repeat,
        wrap_t: Wrap::Repeat,
        mag: Filter::Linear,
        min: Filter::Linear,
        mipmap: Some(Filter::Linear),
    };
    let mut images = BTreeMap::new();
    let mut materials = Vec::with_capacity(definitions.len());
    for material in definitions {
        let infos = material_infos(material);
        let uniform = material_uniform(material);
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("n3 immutable scene material"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let mut views = Vec::with_capacity(5);
        let mut samplers = Vec::with_capacity(5);
        for (slot, info) in infos.iter().enumerate() {
            let texture = info.map(|info| &source_textures[info.texture]);
            let view = if let Some(texture) = texture {
                let srgb = slot == 0 || slot == 4;
                let key = (texture.image, srgb);
                if let std::collections::btree_map::Entry::Vacant(entry) = images.entry(key) {
                    entry.insert(upload(device, queue, &source_images[texture.image], srgb)?);
                }
                images[&key].clone()
            } else {
                white.clone()
            };
            views.push(view);
            samplers.push(sampler(
                device,
                texture.map_or(default_sampler, |texture| texture.sampler),
            ));
        }
        let mut entries = vec![wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }];
        for slot in 0..5 {
            entries.push(wgpu::BindGroupEntry {
                binding: 1 + slot as u32 * 2,
                resource: wgpu::BindingResource::TextureView(&views[slot]),
            });
            entries.push(wgpu::BindGroupEntry {
                binding: 2 + slot as u32 * 2,
                resource: wgpu::BindingResource::Sampler(&samplers[slot]),
            });
        }
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("n3 scene material textures"),
            layout,
            entries: &entries,
        });
        materials.push(GpuMaterial {
            group,
            blend: material.alpha_mode == AlphaMode::Blend,
            double_sided: material.double_sided,
        });
    }
    Ok(materials)
}

fn material_infos(material: &scene::Material) -> [Option<TextureInfo>; 5] {
    [
        material.base_color_texture,
        material.metallic_roughness_texture,
        material.normal_texture,
        material.occlusion_texture,
        material.emissive_texture,
    ]
}
fn material_uniform(material: &scene::Material) -> MaterialUniform {
    MaterialUniform {
        base_color: material.base_color,
        emissive: [
            material.emissive[0],
            material.emissive[1],
            material.emissive[2],
            0.0,
        ],
        pbr: [
            material.metallic,
            material.roughness,
            material.normal_scale,
            material.occlusion_strength,
        ],
        flags: [
            match material.alpha_mode {
                AlphaMode::Opaque => 0.0,
                AlphaMode::Mask => 1.0,
                AlphaMode::Blend => 2.0,
            },
            material.alpha_cutoff,
            u8::from(material.double_sided) as f32,
            u8::from(material.unlit) as f32,
        ],
        uv: material_infos(material).map(uv_transform),
    }
}

fn uv_transform(info: Option<TextureInfo>) -> UvUniform {
    let transform = info.map(|value| value.transform).unwrap_or_default();
    let (sine, cosine) = transform.rotation.sin_cos();
    UvUniform {
        x: [
            cosine * transform.scale[0],
            -sine * transform.scale[1],
            transform.offset[0],
            info.map_or(0.0, |value| value.tex_coord as f32),
        ],
        y: [
            sine * transform.scale[0],
            cosine * transform.scale[1],
            transform.offset[1],
            u8::from(info.is_some()) as f32,
        ],
    }
}
fn sampler(device: &wgpu::Device, source: scene::Sampler) -> wgpu::Sampler {
    let wrap = |mode| match mode {
        Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
        Wrap::Repeat => wgpu::AddressMode::Repeat,
        Wrap::Mirror => wgpu::AddressMode::MirrorRepeat,
    };
    let filter = |mode| match mode {
        Filter::Nearest => wgpu::FilterMode::Nearest,
        Filter::Linear => wgpu::FilterMode::Linear,
    };
    device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("n3 scene sampler"),
        address_mode_u: wrap(source.wrap_s),
        address_mode_v: wrap(source.wrap_t),
        mag_filter: filter(source.mag),
        min_filter: filter(source.min),
        mipmap_filter: match source.mipmap.unwrap_or(Filter::Nearest) {
            Filter::Nearest => wgpu::MipmapFilterMode::Nearest,
            Filter::Linear => wgpu::MipmapFilterMode::Linear,
        },
        lod_max_clamp: if source.mipmap.is_some() { 32.0 } else { 0.0 },
        ..Default::default()
    })
}
fn image_budget(device: &wgpu::Device, image: &scene::Image) -> Result<u64> {
    let limit = device.limits().max_texture_dimension_2d;
    if image.width == 0 || image.height == 0 || image.width > limit || image.height > limit {
        return Err(format!(
            "Texture '{}' dimensions exceed this GPU's {limit}-pixel limit.",
            image.name
        ));
    }
    let expected = u64::from(image.width) * u64::from(image.height) * 4;
    if expected != image.rgba8.len() as u64 {
        return Err(format!(
            "Texture '{}' has invalid RGBA storage.",
            image.name
        ));
    }
    let (mut width, mut height) = (image.width, image.height);
    let mut bytes = expected;
    while width > 1 || height > 1 {
        width = (width / 2).max(1);
        height = (height / 2).max(1);
        bytes += u64::from(width) * u64::from(height) * 4;
    }
    Ok(bytes)
}
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &scene::Image,
    srgb: bool,
) -> Result<wgpu::TextureView> {
    image_budget(device, image)?;
    let mip_count = image.width.max(image.height).ilog2() + 1;
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&image.name),
        size: wgpu::Extent3d {
            width: image.width,
            height: image.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: mip_count,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: if srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        },
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut pixels = image.rgba8.clone();
    let mut width = image.width;
    let mut height = image.height;
    for mip in 0..mip_count {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        if mip + 1 < mip_count {
            let next_width = (width / 2).max(1);
            let next_height = (height / 2).max(1);
            pixels = downsample(&pixels, width, height, next_width, next_height, srgb);
            width = next_width;
            height = next_height;
        }
    }
    Ok(texture.create_view(&Default::default()))
}

/// Average color in linear light; data maps and alpha remain linear byte values.
/// Integer area bins include odd dimensions rather than silently dropping edges.
fn downsample(
    pixels: &[u8],
    width: u32,
    height: u32,
    out_width: u32,
    out_height: u32,
    srgb: bool,
) -> Vec<u8> {
    let mut output = vec![0; out_width as usize * out_height as usize * 4];
    for y in 0..out_height {
        for x in 0..out_width {
            let mut channels = [0.0; 4];
            let mut count = 0.0;
            for sy in y * height / out_height..(y + 1) * height / out_height {
                for sx in x * width / out_width..(x + 1) * width / out_width {
                    let offset = (sy as usize * width as usize + sx as usize) * 4;
                    for channel in 0..4 {
                        let value = pixels[offset + channel] as f32 / 255.0;
                        channels[channel] += if srgb && channel < 3 {
                            srgb_to_linear(value)
                        } else {
                            value
                        };
                    }
                    count += 1.0;
                }
            }
            let offset = (y as usize * out_width as usize + x as usize) * 4;
            for channel in 0..4 {
                let value = channels[channel] / count;
                output[offset + channel] = (if srgb && channel < 3 {
                    linear_to_srgb(value)
                } else {
                    value
                } * 255.0)
                    .round() as u8;
            }
        }
    }
    output
}
pub(super) fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mip_colors_average_linear_light_without_gamma_correcting_data_or_alpha() {
        let pixels = [0, 0, 0, 0, 255, 255, 255, 255];
        assert_eq!(downsample(&pixels, 2, 1, 1, 1, true), [188, 188, 188, 128]);
        assert_eq!(downsample(&pixels, 2, 1, 1, 1, false), [128; 4]);
        assert_eq!(
            downsample(
                &[0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255],
                3,
                1,
                1,
                1,
                false
            ),
            [85, 85, 85, 255]
        );
    }
    #[test]
    fn uv_transform_keeps_khr_scale_then_rotation_then_offset_and_uv_set() {
        let uv = uv_transform(Some(TextureInfo {
            texture: 0,
            tex_coord: 1,
            transform: scene::TextureTransform {
                offset: [0.2, 0.3],
                scale: [2.0, 3.0],
                rotation: std::f32::consts::FRAC_PI_2,
            },
        }));
        assert!((uv.x[0]).abs() < 1e-6 && (uv.x[1] + 3.0).abs() < 1e-6);
        assert!((uv.y[0] - 2.0).abs() < 1e-6 && uv.y[1].abs() < 1e-6);
        assert_eq!(uv.x[2..], [0.2, 1.0]);
        assert_eq!(uv.y[2..], [0.3, 1.0]);
    }
}
