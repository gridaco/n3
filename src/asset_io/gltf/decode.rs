use std::{collections::BTreeSet, io::Cursor, path::Path};

use ::gltf::accessor::Dimensions;

use super::units::length_cm;
use super::*;
use crate::asset_io::resources::{MAX_FILE_BYTES, MAX_RESOURCE_BYTES, MAX_TOTAL_BYTES, read_file};
use crate::scene::validate::hierarchy_roots;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "KHR_lights_punctual",
    "KHR_materials_unlit",
    "KHR_texture_transform",
];

pub(crate) fn load_path(path: &Path) -> Result<SceneAsset> {
    let bytes = read_file(path, MAX_FILE_BYTES)?;
    let resolver = FileResolver::new(
        path.parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )?;
    super::load(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("glTF scene"),
        &bytes,
        &resolver,
    )
}

pub(crate) fn load(
    name: &str,
    bytes: &[u8],
    resolver: &dyn ResourceResolver,
) -> Result<SceneAsset> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err("glTF/GLB input exceeds the 64 MiB file budget.".into());
    }
    let gltf = ::gltf::Gltf::from_slice(bytes)
        .map_err(|error| format!("Invalid glTF 2.0 asset: {error}"))?;
    let document = &gltf.document;
    for extension in document.extensions_required() {
        if !SUPPORTED_EXTENSIONS.contains(&extension) {
            return Err(format!(
                "Required glTF extension is not supported: {extension}"
            ));
        }
    }
    let mut warnings: Vec<_> = document
        .extensions_used()
        .filter(|extension| !SUPPORTED_EXTENSIONS.contains(extension))
        .map(|extension| {
            format!(
                "Optional extension {extension} is ignored; its appearance or behavior may differ."
            )
        })
        .collect();
    if document.nodes().len() > 4096
        || document.meshes().len() > 2048
        || document.materials().len() >= 4096
        || document.images().len() > 512
        || document.textures().len() > 4096
        || document.scenes().len() > 128
        || document.animations().len() > 128
        || document.skins().len() > 1024
    {
        return Err("glTF scene exceeds an object/resource count budget.".into());
    }
    budgets::decoded(document)?;
    let mut total = bytes.len();
    let mut buffers = Vec::new();
    for buffer in document.buffers() {
        if buffer.length() > MAX_RESOURCE_BYTES {
            return Err("Declared glTF buffer exceeds the resource byte budget.".into());
        }
        let mut data = match buffer.source() {
            ::gltf::buffer::Source::Bin => {
                if buffer.index() != 0 {
                    return Err("Only the first GLB buffer may use the binary chunk.".into());
                }
                gltf.blob
                    .as_ref()
                    .ok_or("A buffer references missing GLB binary data.")?
                    .clone()
            }
            ::gltf::buffer::Source::Uri(uri) => resources::uri_bytes(uri, resolver)?,
        };
        if data.len() < buffer.length() {
            return Err("glTF buffer is shorter than its declared byteLength.".into());
        }
        total = total
            .checked_add(data.len())
            .filter(|total| *total <= MAX_TOTAL_BYTES)
            .ok_or("Asset exceeds the total resource byte budget.")?;
        data.truncate(buffer.length());
        buffers.push(data);
    }
    for view in document.views() {
        accessors::checked_view(&view, &buffers)?;
    }
    let mut decoded_values = 0usize;
    for accessor in document.accessors() {
        decoded_values = decoded_values
            .checked_add(
                accessor
                    .count()
                    .checked_mul(accessor.dimensions().multiplicity())
                    .ok_or("Accessor count overflow.")?,
            )
            .filter(|total| *total <= 32_000_000)
            .ok_or("Scene exceeds its decoded accessor budget.")?;
        // Validate every declared accessor, not only attributes we use. This
        // prevents malformed hidden payloads from becoming future panics.
        accessors::read(accessor, &buffers)?;
    }
    let images = images(document, &buffers, resolver, &mut total)?;
    let materials = materials::materials(document)?;
    let textures = materials::textures(document);
    let meshes = geometry::meshes(document, &buffers, &materials)?;
    let mut nodes = Vec::new();
    for node in document.nodes() {
        let transform = match node.transform() {
            ::gltf::scene::Transform::Matrix { matrix } => {
                let matrix = DMat4::from_cols_array_2d(&matrix.map(|column| column.map(f64::from)));
                if !matrix.is_finite()
                    || matrix.x_axis.w != 0.
                    || matrix.y_axis.w != 0.
                    || matrix.z_axis.w != 0.
                    || matrix.w_axis.w != 1.
                {
                    return Err("Node matrices must be finite affine transforms.".into());
                }
                Transform::Matrix(units::affine_cm(matrix))
            }
            ::gltf::scene::Transform::Decomposed {
                translation,
                rotation,
                scale,
            } => {
                let translation = DVec3::from_array(units::position_cm(translation));
                let rotation = DQuat::from_array(rotation.map(f64::from));
                let scale = DVec3::from_array(scale.map(f64::from));
                if !translation.is_finite()
                    || !scale.is_finite()
                    || !rotation.is_finite()
                    || (rotation.length_squared() - 1.).abs() > 1e-3
                {
                    return Err(
                        "Node TRS values must be finite, with a unit rotation quaternion.".into(),
                    );
                }
                Transform::Trs {
                    translation,
                    rotation: rotation.normalize(),
                    scale,
                }
            }
        };
        let mesh = node.mesh().map(|mesh| mesh.index());
        let weights = node.weights().map(<[f32]>::to_vec);
        if let Some(weights) = &weights {
            let Some(mesh) = mesh else {
                return Err("Node morph weights require a mesh.".into());
            };
            if weights.len() != meshes[mesh].weights.len() || weights.iter().any(|v| !v.is_finite())
            {
                return Err("Node morph weights must match its mesh and be finite.".into());
            }
        }
        nodes.push(Node {
            name: node
                .name()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Node {}", node.index() + 1)),
            children: node.children().map(|child| child.index()).collect(),
            mesh,
            skin: node.skin().map(|skin| skin.index()),
            camera: node.camera().map(|camera| camera.index()),
            light: node.light().map(|light| light.index()),
            weights,
            transform,
        });
    }
    let roots = hierarchy_roots(&nodes)?;
    let mut scenes: Vec<_> = document
        .scenes()
        .map(|scene| SceneDefinition {
            name: scene
                .name()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Scene {}", scene.index() + 1)),
            roots: scene.nodes().map(|node| node.index()).collect(),
        })
        .collect();
    if scenes.is_empty() {
        warnings.push("No scene is declared; showing the parentless node hierarchy.".into());
        scenes.push(SceneDefinition {
            name: "Scene".into(),
            roots: roots.clone(),
        });
    }
    for scene in &scenes {
        let unique: BTreeSet<_> = scene.roots.iter().copied().collect();
        if unique.len() != scene.roots.len() || scene.roots.iter().any(|root| !roots.contains(root))
        {
            return Err("Scene roots must be unique nodes without parents.".into());
        }
    }
    let skins = skins(document, &buffers)?;
    let mut tree = vec![0; nodes.len()];
    for root in &roots {
        let mut pending = vec![*root];
        while let Some(node) = pending.pop() {
            tree[node] = *root;
            pending.extend(&nodes[node].children);
        }
    }
    for skin in &skins {
        if skin
            .joints
            .iter()
            .any(|joint| tree[*joint] != tree[skin.joints[0]])
        {
            return Err("Skin joints must share a common ancestor in the node hierarchy.".into());
        }
    }
    for node in &nodes {
        if let Some(skin) = node.skin {
            let mesh = node.mesh.ok_or("A skinned node must reference a mesh.")?;
            for primitive in &meshes[mesh].primitives {
                if primitive.influences.is_empty() {
                    return Err("A skinned primitive is missing JOINTS_0/WEIGHTS_0.".into());
                }
                if primitive.influences.iter().flatten().any(|(joints, _)| {
                    joints
                        .iter()
                        .any(|joint| *joint as usize >= skins[skin].joints.len())
                }) {
                    return Err("JOINTS index exceeds the referenced skin's joint count.".into());
                }
            }
        }
    }
    let cameras = cameras(document)?;
    let lights = lights(document)?;
    let animations = animations(document, &buffers, &nodes, &meshes)?;
    let default_scene = document.default_scene().map_or(0, |scene| scene.index());
    SceneAsset::new(SceneData {
        name: name.into(),
        scenes,
        default_scene,
        nodes,
        materials,
        images,
        textures,
        cameras,
        lights,
        animations,
        warnings,
        meshes,
        skins,
    })
}

fn images(
    document: &::gltf::Document,
    buffers: &[Vec<u8>],
    resolver: &dyn ResourceResolver,
    total: &mut usize,
) -> Result<Vec<Image>> {
    let mut decoded = 0usize;
    document
        .images()
        .map(|image| {
            let (bytes, mime) = match image.source() {
                ::gltf::image::Source::View { view, mime_type } => (
                    accessors::checked_view(&view, buffers)?.to_vec(),
                    Some(mime_type),
                ),
                ::gltf::image::Source::Uri { uri, mime_type } => {
                    (resources::uri_bytes(uri, resolver)?, mime_type)
                }
            };
            *total = total
                .checked_add(bytes.len())
                .filter(|total| *total <= MAX_TOTAL_BYTES)
                .ok_or("Asset exceeds its total resource byte budget.")?;
            let format = image::guess_format(&bytes)
                .map_err(|error| format!("Invalid glTF image: {error}"))?;
            if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg)
                || mime.is_some_and(|mime| {
                    !matches!(
                        (mime, format),
                        ("image/png", image::ImageFormat::Png)
                            | ("image/jpeg", image::ImageFormat::Jpeg)
                    )
                })
            {
                return Err("Core glTF images must be PNG or JPEG with matching MIME type.".into());
            }
            let dimensions = image::ImageReader::with_format(Cursor::new(&bytes), format)
                .into_dimensions()
                .map_err(|error| error.to_string())?;
            let size = (dimensions.0 as usize)
                .checked_mul(dimensions.1 as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or("Decoded image size overflow.")?;
            if dimensions.0 == 0 || dimensions.1 == 0 || dimensions.0 > 8192 || dimensions.1 > 8192
            {
                return Err("Scene images must be at most 8192 by 8192 pixels.".into());
            }
            decoded = decoded
                .checked_add(size)
                .filter(|size| *size <= 256 * 1024 * 1024)
                .ok_or("Scene exceeds its decoded image budget.")?;
            let mut reader = image::ImageReader::with_format(Cursor::new(&bytes), format);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(8192);
            limits.max_image_height = Some(8192);
            limits.max_alloc = Some(256 * 1024 * 1024);
            reader.limits(limits);
            let rgba = reader
                .decode()
                .map_err(|error| format!("Cannot decode glTF image: {error}"))?
                .into_rgba8();
            Ok(Image {
                name: image.name().unwrap_or("Image").into(),
                width: dimensions.0,
                height: dimensions.1,
                rgba8: rgba.into_raw(),
            })
        })
        .collect()
}

fn skins(document: &::gltf::Document, buffers: &[Vec<u8>]) -> Result<Vec<Skin>> {
    document
        .skins()
        .map(|skin| {
            let joints: Vec<_> = skin.joints().map(|joint| joint.index()).collect();
            if joints.is_empty()
                || joints.len() > 1024
                || joints.iter().copied().collect::<BTreeSet<_>>().len() != joints.len()
            {
                return Err("Skin joints must be unique and number between one and 1024.".into());
            }
            let inverse_bind = if let Some(accessor) = skin.inverse_bind_matrices() {
                geometry::require_float(&accessor, Dimensions::Mat4)?;
                if accessor.count() < joints.len() {
                    return Err(
                        "Inverse bind matrix count must cover the skin's joint count.".into(),
                    );
                }
                let values = accessors::read(accessor, buffers)?;
                if values.as_chunks::<16>().0.iter().any(|matrix| {
                    matrix[3] != 0. || matrix[7] != 0. || matrix[11] != 0. || matrix[15] != 1.
                }) {
                    return Err(
                        "Inverse bind matrices must be affine with fourth row [0, 0, 0, 1].".into(),
                    );
                }
                values
                    .as_chunks::<16>()
                    .0
                    .iter()
                    .take(joints.len())
                    .map(|values| units::affine_cm(DMat4::from_cols_array(values)))
                    .collect()
            } else {
                vec![DMat4::IDENTITY; joints.len()]
            };
            Ok(Skin {
                name: skin.name().unwrap_or("Skin").into(),
                joints,
                inverse_bind,
            })
        })
        .collect()
}

fn cameras(document: &::gltf::Document) -> Result<Vec<Camera>> {
    document
        .cameras()
        .map(|camera| {
            let projection = match camera.projection() {
                ::gltf::camera::Projection::Perspective(value) => {
                    if !(0.0..std::f32::consts::PI).contains(&value.yfov())
                        || value.yfov() == 0.
                        || !value.znear().is_finite()
                        || value.znear() <= 0.
                        || value
                            .zfar()
                            .is_some_and(|far| !far.is_finite() || far <= value.znear())
                        || value
                            .aspect_ratio()
                            .is_some_and(|aspect| !aspect.is_finite() || aspect <= 0.)
                    {
                        return Err("Invalid perspective camera projection.".into());
                    }
                    Projection::Perspective {
                        yfov: value.yfov(),
                        aspect: value.aspect_ratio(),
                        near_cm: length_cm(value.znear())?,
                        far_cm: value.zfar().map(length_cm).transpose()?,
                    }
                }
                ::gltf::camera::Projection::Orthographic(value) => {
                    if [value.xmag(), value.ymag(), value.znear(), value.zfar()]
                        .iter()
                        .any(|value| !value.is_finite())
                        || value.xmag() <= 0.
                        || value.ymag() <= 0.
                        || value.znear() < 0.
                        || value.zfar() <= value.znear()
                    {
                        return Err("Invalid orthographic camera projection.".into());
                    }
                    Projection::Orthographic {
                        xmag_cm: length_cm(value.xmag())?,
                        ymag_cm: length_cm(value.ymag())?,
                        near_cm: length_cm(value.znear())?,
                        far_cm: length_cm(value.zfar())?,
                    }
                }
            };
            Ok(Camera {
                name: camera.name().unwrap_or("Camera").into(),
                projection,
            })
        })
        .collect()
}

fn lights(document: &::gltf::Document) -> Result<Vec<Light>> {
    document
        .lights()
        .into_iter()
        .flatten()
        .map(|light| {
            let color = light.color();
            if color
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
                || !light.intensity().is_finite()
                || light.intensity() < 0.
                || light
                    .range()
                    .is_some_and(|range| !range.is_finite() || range <= 0.)
            {
                return Err("Invalid punctual light color, intensity, or range.".into());
            }
            let kind = match light.kind() {
                ::gltf::khr_lights_punctual::Kind::Directional => LightKind::Directional,
                ::gltf::khr_lights_punctual::Kind::Point => LightKind::Point,
                ::gltf::khr_lights_punctual::Kind::Spot {
                    inner_cone_angle,
                    outer_cone_angle,
                } => {
                    if !inner_cone_angle.is_finite()
                        || !outer_cone_angle.is_finite()
                        || inner_cone_angle < 0.
                        || inner_cone_angle >= outer_cone_angle
                        || outer_cone_angle > std::f32::consts::FRAC_PI_2
                    {
                        return Err("Invalid spot light cone angles.".into());
                    }
                    LightKind::Spot {
                        inner: inner_cone_angle,
                        outer: outer_cone_angle,
                    }
                }
            };
            Ok(Light {
                name: light.name().unwrap_or("Light").into(),
                kind,
                color,
                intensity: light.intensity(),
                range_cm: light.range().map(length_cm).transpose()?,
            })
        })
        .collect()
}

fn animations(
    document: &::gltf::Document,
    buffers: &[Vec<u8>],
    nodes: &[Node],
    meshes: &[Mesh],
) -> Result<Vec<Animation>> {
    document
        .animations()
        .map(|animation| {
            let mut channels = Vec::new();
            let mut targets = BTreeSet::new();
            let mut start = f32::INFINITY;
            let mut end = 0_f32;
            for channel in animation.channels() {
                let node = channel.target().node().index();
                let (property, dimensions, components) = match channel.target().property() {
                    ::gltf::animation::Property::Translation => {
                        (Property::Translation, Dimensions::Vec3, 3)
                    }
                    ::gltf::animation::Property::Rotation => {
                        (Property::Rotation, Dimensions::Vec4, 4)
                    }
                    ::gltf::animation::Property::Scale => (Property::Scale, Dimensions::Vec3, 3),
                    ::gltf::animation::Property::MorphTargetWeights => {
                        let mesh = nodes[node]
                            .mesh
                            .ok_or("Morph animation targets a node without a mesh.")?;
                        if meshes[mesh].weights.is_empty() {
                            return Err(
                                "Morph animation targets a mesh without morph targets.".into()
                            );
                        }
                        (
                            Property::Weights,
                            Dimensions::Scalar,
                            meshes[mesh].weights.len(),
                        )
                    }
                };
                if property != Property::Weights
                    && matches!(nodes[node].transform, Transform::Matrix(_))
                {
                    return Err("TRS animation cannot target a node authored with a matrix.".into());
                }
                if !targets.insert((node, property as u8)) {
                    return Err(
                        "Animation channels cannot target the same node property twice.".into(),
                    );
                }
                let sampler = channel.sampler();
                geometry::require_float(&sampler.input(), Dimensions::Scalar)?;
                geometry::require_float(&sampler.output(), dimensions)?;
                let times: Vec<f32> = accessors::read(sampler.input(), buffers)?
                    .into_iter()
                    .map(|value| value as f32)
                    .collect();
                if times.first().is_none_or(|value| *value < 0.)
                    || times.windows(2).any(|pair| pair[0] >= pair[1])
                {
                    return Err(
                        "Animation key times must be nonnegative and strictly increasing.".into(),
                    );
                }
                let interpolation = match sampler.interpolation() {
                    ::gltf::animation::Interpolation::Step => Interpolation::Step,
                    ::gltf::animation::Interpolation::Linear => Interpolation::Linear,
                    ::gltf::animation::Interpolation::CubicSpline => Interpolation::CubicHermite,
                };
                let mut packed_values = accessors::read(sampler.output(), buffers)?;
                let factor = if interpolation == Interpolation::CubicHermite {
                    3
                } else {
                    1
                };
                if packed_values.len() != times.len() * components * factor {
                    return Err(
                        "Animation output count does not match its keys/interpolation/target."
                            .into(),
                    );
                }
                if property == Property::Rotation {
                    for key in 0..times.len() {
                        let offset = (key * factor + usize::from(factor == 3)) * 4;
                        let q =
                            DQuat::from_array(std::array::from_fn(|i| packed_values[offset + i]));
                        if (q.length_squared() - 1.).abs() > 1e-3 {
                            return Err("Rotation keyframes must contain unit quaternions.".into());
                        }
                    }
                }
                // Runtime curves store values and derivatives independently of
                // the source accessor's interleaved [in, value, out] records.
                if property == Property::Translation {
                    for value in &mut packed_values {
                        *value *= 100.;
                    }
                }
                let (values, in_tangents, out_tangents) = if factor == 3 {
                    let mut values = Vec::with_capacity(times.len() * components);
                    let mut incoming = Vec::with_capacity(values.capacity());
                    let mut outgoing = Vec::with_capacity(values.capacity());
                    for key in packed_values.chunks_exact(components * 3) {
                        incoming.extend_from_slice(&key[..components]);
                        values.extend_from_slice(&key[components..components * 2]);
                        outgoing.extend_from_slice(&key[components * 2..]);
                    }
                    (values, incoming, outgoing)
                } else {
                    (packed_values, Vec::new(), Vec::new())
                };
                start = start.min(times[0]);
                end = end.max(*times.last().unwrap());
                channels.push(Channel {
                    node,
                    property,
                    interpolation,
                    times,
                    values,
                    in_tangents,
                    out_tangents,
                    components,
                });
            }
            if channels.is_empty() {
                return Err("Animations must contain channels.".into());
            }
            Ok(Animation {
                name: animation
                    .name()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Animation {}", animation.index() + 1)),
                start,
                duration: end - start,
                channels,
            })
        })
        .collect()
}
