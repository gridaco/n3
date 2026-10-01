//! A built-in neutral studio, integrated once on the CPU and cached per process.
//! This is actual split-sum IBL: cosine-convolved diffuse irradiance, GGX-filtered
//! specular cubemap mips, and an integrated BRDF LUT. External HDR import is a
//! separate future feature; no network resource or runtime asset is required.
use glam::{Vec2, Vec3};
use std::sync::OnceLock;

const SPECULAR_SIZE: u32 = 128;
const DIFFUSE_SIZE: u32 = 16;
const LUT_SIZE: u32 = 64;
const ENV_SAMPLES: u32 = 64;
const LUT_SAMPLES: u32 = 128;
pub(super) const SPECULAR_MAX_LOD: f32 = 7.0;

struct Baked {
    specular: Vec<Vec<u16>>,
    diffuse: Vec<u16>,
    brdf: Vec<u16>,
}
static STUDIO: OnceLock<Baked> = OnceLock::new();

#[derive(Clone)]
pub(super) struct Environment {
    pub group: wgpu::BindGroup,
}
impl Environment {
    pub fn layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        let mut entries: Vec<_> = (0..3)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: if binding < 2 {
                        wgpu::TextureViewDimension::Cube
                    } else {
                        wgpu::TextureViewDimension::D2
                    },
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("n3 studio IBL layout"),
            entries: &entries,
        })
    }
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout) -> Self {
        let baked = STUDIO.get_or_init(bake);
        let diffuse = upload(
            device,
            queue,
            "n3 diffuse studio irradiance",
            DIFFUSE_SIZE,
            6,
            std::slice::from_ref(&baked.diffuse),
        );
        let specular = upload(
            device,
            queue,
            "n3 GGX filtered studio",
            SPECULAR_SIZE,
            6,
            &baked.specular,
        );
        let brdf = upload(
            device,
            queue,
            "n3 integrated split sum BRDF",
            LUT_SIZE,
            1,
            std::slice::from_ref(&baked.brdf),
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("n3 studio sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("n3 built in studio IBL"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&diffuse),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&specular),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&brdf),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self { group }
    }
}

fn radiance(direction: Vec3) -> Vec3 {
    let sky = 0.08 + 0.52 * (direction.y * 0.5 + 0.5);
    let key = direction
        .dot(Vec3::new(-0.6, 0.8, 0.7).normalize())
        .max(0.0)
        .powi(48);
    let fill = direction
        .dot(Vec3::new(0.8, 0.4, -0.4).normalize())
        .max(0.0)
        .powi(24);
    Vec3::splat(sky + key * 5.0 + fill * 1.8)
}
fn direction(face: u32, x: u32, y: u32, size: u32) -> Vec3 {
    let u = (x as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    let v = (y as f32 + 0.5) / size as f32 * 2.0 - 1.0;
    match face {
        0 => Vec3::new(1.0, -v, -u),
        1 => Vec3::new(-1.0, -v, u),
        2 => Vec3::new(u, 1.0, v),
        3 => Vec3::new(u, -1.0, -v),
        4 => Vec3::new(u, -v, 1.0),
        _ => Vec3::new(-u, -v, -1.0),
    }
    .normalize()
}
fn hammersley(index: u32, count: u32) -> Vec2 {
    Vec2::new(
        index as f32 / count as f32,
        index.reverse_bits() as f32 * 2.328_306_4e-10,
    )
}
fn hemisphere(normal: Vec3, sample: Vec3) -> Vec3 {
    let up = if normal.z.abs() < 0.999 {
        Vec3::Z
    } else {
        Vec3::X
    };
    let tangent = up.cross(normal).normalize();
    tangent * sample.x + normal.cross(tangent) * sample.y + normal * sample.z
}
fn ggx_sample(xi: Vec2, roughness: f32) -> Vec3 {
    let alpha = roughness * roughness;
    let cosine = ((1.0 - xi.y) / (1.0 + (alpha * alpha - 1.0) * xi.y)).sqrt();
    let sine = (1.0 - cosine * cosine).max(0.0).sqrt();
    let (s, c) = (std::f32::consts::TAU * xi.x).sin_cos();
    Vec3::new(sine * c, sine * s, cosine)
}
fn diffuse(normal: Vec3) -> Vec3 {
    let mut sum = Vec3::ZERO;
    for sample in 0..ENV_SAMPLES {
        let xi = hammersley(sample, ENV_SAMPLES);
        let (s, c) = (std::f32::consts::TAU * xi.x).sin_cos();
        let tangent = Vec3::new(xi.y.sqrt() * c, xi.y.sqrt() * s, (1.0 - xi.y).sqrt());
        sum += radiance(hemisphere(normal, tangent));
    }
    // Cosine-weighted PDF cancels N.L/pi: this stores irradiance divided by pi.
    sum / ENV_SAMPLES as f32
}
fn specular(normal: Vec3, roughness: f32) -> Vec3 {
    if roughness == 0.0 {
        return radiance(normal);
    }
    let mut sum = Vec3::ZERO;
    let mut weight = 0.0;
    for sample in 0..ENV_SAMPLES {
        let half = hemisphere(
            normal,
            ggx_sample(hammersley(sample, ENV_SAMPLES), roughness),
        );
        let light = 2.0 * normal.dot(half) * half - normal;
        let nl = normal.dot(light).max(0.0);
        sum += radiance(light) * nl;
        weight += nl;
    }
    sum / weight.max(1e-6)
}
fn integrated_brdf(nv: f32, roughness: f32) -> Vec2 {
    let view = Vec3::new((1.0 - nv * nv).sqrt(), 0.0, nv);
    let a2 = roughness.powi(4);
    let mut result = Vec2::ZERO;
    for sample in 0..LUT_SAMPLES {
        let half = ggx_sample(hammersley(sample, LUT_SAMPLES), roughness);
        let vh = view.dot(half).max(0.0);
        let light = 2.0 * vh * half - view;
        let nl = light.z.max(0.0);
        let nh = half.z.max(0.0);
        if nl > 0.0 && nh > 0.0 {
            let gv = nl * (nv * nv * (1.0 - a2) + a2).sqrt();
            let gl = nv * (nl * nl * (1.0 - a2) + a2).sqrt();
            let visibility = 0.5 / (gv + gl).max(1e-6);
            let weight = 4.0 * visibility * nl * vh / nh;
            let fresnel = (1.0 - vh).powi(5);
            result += Vec2::new(1.0 - fresnel, fresnel) * weight;
        }
    }
    result / LUT_SAMPLES as f32
}
fn cube(size: u32, sample: impl Fn(Vec3) -> Vec3) -> Vec<u16> {
    let mut pixels = Vec::with_capacity(size as usize * size as usize * 6 * 4);
    for face in 0..6 {
        for y in 0..size {
            for x in 0..size {
                let value = sample(direction(face, x, y, size));
                pixels.extend(value.extend(1.0).to_array().map(half));
            }
        }
    }
    pixels
}
fn bake() -> Baked {
    let levels = SPECULAR_SIZE.ilog2() + 1;
    let specular = (0..levels)
        .map(|level| {
            cube((SPECULAR_SIZE >> level).max(1), |normal| {
                specular(normal, level as f32 / (levels - 1) as f32)
            })
        })
        .collect();
    let diffuse = cube(DIFFUSE_SIZE, diffuse);
    let mut brdf = Vec::with_capacity(LUT_SIZE as usize * LUT_SIZE as usize * 4);
    for y in 0..LUT_SIZE {
        for x in 0..LUT_SIZE {
            let value = integrated_brdf(
                (x as f32 + 0.5) / LUT_SIZE as f32,
                (y as f32 + 0.5) / LUT_SIZE as f32,
            );
            brdf.extend([value.x, value.y, 0.0, 1.0].map(half));
        }
    }
    Baked {
        specular,
        diffuse,
        brdf,
    }
}
fn half(value: f32) -> u16 {
    // All generated channels are nonnegative finite radiance/integrals. Round
    // to nearest-even without another dependency solely for an upload format.
    let value = value.clamp(0.0, 65504.0);
    if value < 1.0 / 16384.0 {
        return (value * 16_777_216.0).round_ties_even() as u16;
    }
    let bits = value.to_bits();
    let rounded = bits + 0x0fff + ((bits >> 13) & 1);
    ((rounded >> 13) - (112 << 10)) as u16
}
fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    size: u32,
    layers: u32,
    mips: &[Vec<u16>],
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: layers,
        },
        mip_level_count: mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, pixels) in mips.iter().enumerate() {
        let width = (size >> level).max(1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(pixels),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 8),
                rows_per_image: Some(width),
            },
            wgpu::Extent3d {
                width,
                height: width,
                depth_or_array_layers: layers,
            },
        );
    }
    texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(if layers == 6 {
            wgpu::TextureViewDimension::Cube
        } else {
            wgpu::TextureViewDimension::D2
        }),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_render_fingerprints_environment_uploads_are_finite_and_complete() {
        use sha2::{Digest, Sha256};
        fn fingerprint(name: &str, size: u32, layers: u32, pixels: &[u16]) {
            assert_eq!(
                pixels.len(),
                size as usize * size as usize * layers as usize * 4
            );
            assert!(
                pixels
                    .iter()
                    .all(|value| value & 0x8000 == 0 && value & 0x7c00 != 0x7c00)
            );
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|pixel| pixel[3] == 0x3c00)
            );
            assert!(
                pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[..3].iter().any(|value| *value != 0))
            );
            let mut digest = Sha256::new();
            digest.update(b"n3 studio environment upload v1\0");
            digest.update(size.to_le_bytes());
            digest.update(layers.to_le_bytes());
            for value in pixels {
                digest.update(value.to_le_bytes());
            }
            println!(
                "imported-render-fingerprint environment={name} size={size} layers={layers} bytes={} sha256={:x}",
                pixels.len() * 2,
                digest.finalize()
            );
        }
        let baked = STUDIO.get_or_init(bake);
        assert!(std::ptr::eq(baked, STUDIO.get_or_init(bake)));
        assert_eq!(baked.specular.len(), SPECULAR_SIZE.ilog2() as usize + 1);
        for (level, pixels) in baked.specular.iter().enumerate() {
            fingerprint(
                &format!("specular-mip{level}"),
                (SPECULAR_SIZE >> level).max(1),
                6,
                pixels,
            );
        }
        fingerprint("diffuse", DIFFUSE_SIZE, 6, &baked.diffuse);
        fingerprint("brdf", LUT_SIZE, 1, &baked.brdf);
        assert!(
            baked
                .brdf
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[2] == 0)
        );
    }

    #[test]
    fn studio_integrals_are_finite_nonnegative_and_roughness_broadens_reflections() {
        for roughness in [0.0, 0.05, 0.5, 1.0] {
            for nv in [0.001, 0.1, 0.5, 1.0] {
                let value = integrated_brdf(nv, roughness);
                assert!(value.is_finite() && value.min_element() >= 0.0);
            }
        }
        let key = Vec3::new(-0.6, 0.8, 0.7).normalize();
        assert!(specular(key, 0.0).x > specular(key, 1.0).x * 2.0);
        assert!(diffuse(Vec3::Y).x > diffuse(Vec3::NEG_Y).x);
        assert_eq!(half(1.0), 0x3c00);
        assert_eq!(half(0.0), 0);
        assert_eq!(half(65504.0), 0x7bff);
        assert_eq!(half(1.0 / 16_777_216.0), 1);
    }
}
