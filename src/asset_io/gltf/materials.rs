use super::*;

pub(super) fn materials(document: &::gltf::Document) -> Result<Vec<Material>> {
    let mut result = document
        .materials()
        .map(material)
        .collect::<Result<Vec<_>>>()?;
    result.push(Material {
        name: "Default material".into(),
        base_color: [1.; 4],
        metallic: 1.,
        roughness: 1.,
        emissive: [0.; 3],
        base_color_texture: None,
        metallic_roughness_texture: None,
        normal_texture: None,
        normal_scale: 1.,
        occlusion_texture: None,
        occlusion_strength: 1.,
        emissive_texture: None,
        alpha_mode: AlphaMode::Opaque,
        alpha_cutoff: 0.5,
        double_sided: false,
        unlit: false,
    });
    Ok(result)
}

fn material(material: ::gltf::Material<'_>) -> Result<Material> {
    let pbr = material.pbr_metallic_roughness();
    let base_color = pbr.base_color_factor();
    let metallic = pbr.metallic_factor();
    let roughness = pbr.roughness_factor();
    let emissive = material.emissive_factor();
    if base_color
        .iter()
        .chain(emissive.iter())
        .chain([&metallic, &roughness])
        .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
    {
        return Err("Material factors must be finite values between zero and one.".into());
    }
    let normal = material.normal_texture();
    let occlusion = material.occlusion_texture();
    let normal_scale = normal.as_ref().map_or(1., |normal| normal.scale());
    let occlusion_strength = occlusion.as_ref().map_or(1., |texture| texture.strength());
    let alpha_cutoff = material.alpha_cutoff().unwrap_or(0.5);
    if !normal_scale.is_finite()
        || !occlusion_strength.is_finite()
        || !(0.0..=1.0).contains(&occlusion_strength)
        || !alpha_cutoff.is_finite()
        || alpha_cutoff < 0.
    {
        return Err("Invalid normal scale, occlusion strength, or alpha cutoff.".into());
    }
    Ok(Material {
        name: material
            .name()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Material {}", material.index().unwrap_or(0) + 1)),
        base_color,
        metallic,
        roughness,
        emissive,
        base_color_texture: pbr.base_color_texture().map(info).transpose()?,
        metallic_roughness_texture: pbr.metallic_roughness_texture().map(info).transpose()?,
        normal_texture: normal
            .map(|texture| {
                special_info(
                    texture.texture().index(),
                    texture.tex_coord(),
                    texture.extension_value("KHR_texture_transform"),
                )
            })
            .transpose()?,
        normal_scale,
        occlusion_texture: occlusion
            .map(|texture| {
                special_info(
                    texture.texture().index(),
                    texture.tex_coord(),
                    texture.extension_value("KHR_texture_transform"),
                )
            })
            .transpose()?,
        occlusion_strength,
        emissive_texture: material.emissive_texture().map(info).transpose()?,
        alpha_mode: match material.alpha_mode() {
            ::gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
            ::gltf::material::AlphaMode::Mask => AlphaMode::Mask,
            ::gltf::material::AlphaMode::Blend => AlphaMode::Blend,
        },
        alpha_cutoff,
        double_sided: material.double_sided(),
        unlit: material.unlit(),
    })
}

fn info(texture: ::gltf::texture::Info<'_>) -> Result<TextureInfo> {
    let mut result = TextureInfo {
        texture: texture.texture().index(),
        tex_coord: texture.tex_coord(),
        transform: TextureTransform::default(),
    };
    if let Some(transform) = texture.texture_transform() {
        result.transform = TextureTransform {
            offset: transform.offset(),
            scale: transform.scale(),
            rotation: transform.rotation(),
        };
        result.tex_coord = transform.tex_coord().unwrap_or(result.tex_coord);
    }
    validate_info(result)
}

fn special_info(
    texture: usize,
    tex_coord: u32,
    extension: Option<&serde_json::Value>,
) -> Result<TextureInfo> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Extended {
        #[serde(default)]
        offset: [f32; 2],
        #[serde(default = "one")]
        scale: [f32; 2],
        #[serde(default)]
        rotation: f32,
        tex_coord: Option<u32>,
    }
    fn one() -> [f32; 2] {
        [1.; 2]
    }
    let mut result = TextureInfo {
        texture,
        tex_coord,
        transform: TextureTransform::default(),
    };
    if let Some(extension) = extension {
        let transform: Extended = serde_json::from_value(extension.clone())
            .map_err(|error| format!("Invalid texture transform: {error}"))?;
        result.transform = TextureTransform {
            offset: transform.offset,
            scale: transform.scale,
            rotation: transform.rotation,
        };
        result.tex_coord = transform.tex_coord.unwrap_or(tex_coord);
    }
    validate_info(result)
}

fn validate_info(info: TextureInfo) -> Result<TextureInfo> {
    if info.tex_coord > 1 {
        return Err(
            "This scene viewer supports texture coordinates TEXCOORD_0 and TEXCOORD_1 only.".into(),
        );
    }
    if info
        .transform
        .offset
        .into_iter()
        .chain(info.transform.scale)
        .chain([info.transform.rotation])
        .any(|value| !value.is_finite())
    {
        return Err("Texture transforms must be finite.".into());
    }
    Ok(info)
}

pub(super) fn textures(document: &::gltf::Document) -> Vec<Texture> {
    use ::gltf::texture::{MagFilter, MinFilter, WrappingMode};
    let wrap = |mode| match mode {
        WrappingMode::ClampToEdge => Wrap::Clamp,
        WrappingMode::MirroredRepeat => Wrap::Mirror,
        WrappingMode::Repeat => Wrap::Repeat,
    };
    document
        .textures()
        .map(|texture| {
            let sampler = texture.sampler();
            let (min, mipmap) = match sampler.min_filter() {
                Some(MinFilter::Nearest) => (Filter::Nearest, None),
                Some(MinFilter::Linear) => (Filter::Linear, None),
                Some(MinFilter::NearestMipmapNearest) => (Filter::Nearest, Some(Filter::Nearest)),
                Some(MinFilter::NearestMipmapLinear) => (Filter::Nearest, Some(Filter::Linear)),
                Some(MinFilter::LinearMipmapNearest) => (Filter::Linear, Some(Filter::Nearest)),
                None | Some(MinFilter::LinearMipmapLinear) => {
                    (Filter::Linear, Some(Filter::Linear))
                }
            };
            Texture {
                image: texture.source().index(),
                sampler: Sampler {
                    wrap_s: wrap(sampler.wrap_s()),
                    wrap_t: wrap(sampler.wrap_t()),
                    mag: if sampler.mag_filter() == Some(MagFilter::Nearest) {
                        Filter::Nearest
                    } else {
                        Filter::Linear
                    },
                    min,
                    mipmap,
                },
            }
        })
        .collect()
}
