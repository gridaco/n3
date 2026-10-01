use super::*;
use std::collections::BTreeSet;

pub(crate) fn hierarchy_roots(nodes: &[Node]) -> Result<Vec<usize>> {
    let mut parents = vec![None; nodes.len()];
    for (parent, node) in nodes.iter().enumerate() {
        for child in &node.children {
            if *child >= nodes.len() || parents[*child].replace(parent).is_some() {
                return Err(
                    "Nodes must form a forest without multiple parents or repeated children."
                        .into(),
                );
            }
        }
    }
    let roots: Vec<_> = parents
        .iter()
        .enumerate()
        .filter_map(|(index, parent)| parent.is_none().then_some(index))
        .collect();
    let mut stack = roots.clone();
    let mut visited = 0;
    while let Some(node) = stack.pop() {
        visited += 1;
        stack.extend(&nodes[node].children);
    }
    if visited != nodes.len() {
        return Err("Node hierarchy contains a cycle.".into());
    }
    Ok(roots)
}

/// Structural safety belongs to the runtime model, even for scenes produced
/// directly in code. Import adapters additionally validate their file formats.
pub(super) fn data(data: &SceneData) -> Result<()> {
    if data.scenes.is_empty() || data.default_scene >= data.scenes.len() {
        return Err("A scene asset must have a valid default scene.".into());
    }
    if data.nodes.len() > 4096
        || data.meshes.len() > 2048
        || data.materials.len() > 4096
        || data.images.len() > 512
        || data.textures.len() > 4096
        || data.scenes.len() > 128
        || data.animations.len() > 128
        || data.skins.len() > 1024
        || data.cameras.len() > 4096
        || data.lights.len() > 4096
    {
        return Err("Scene exceeds an object/resource count budget.".into());
    }
    let roots = hierarchy_roots(&data.nodes)?;
    for scene in &data.scenes {
        if scene.roots.iter().any(|root| !roots.contains(root))
            || scene.roots.iter().copied().collect::<BTreeSet<_>>().len() != scene.roots.len()
        {
            return Err("Scene roots must be unique parentless nodes.".into());
        }
    }
    let mut budget = budgets::Budget::default();
    let mut vertices = budgets::Budget::default();
    let mut indices = budgets::Budget::default();
    let mut primitive_count = 0;
    for mesh in &data.meshes {
        if mesh.weights.len() > 64 || mesh.weights.iter().any(|v| !v.is_finite()) {
            return Err("Mesh morph weights must be finite and within the target budget.".into());
        }
        for primitive in &mesh.primitives {
            primitive_count += 1;
            if primitive_count > budgets::MAX_PRIMITIVES {
                return Err("Scene exceeds the primitive count budget.".into());
            }
            let count = primitive.vertices.len();
            vertices.charge(count, 1, 2_000_000)?;
            indices.charge(primitive.indices.len(), 1, 12_000_000)?;
            budget.owned::<Primitive>(1)?;
            budget.owned::<SceneVertex>(count)?;
            budget.owned::<u32>(primitive.indices.len())?;
            if count == 0
                || primitive.indices.is_empty()
                || primitive.material >= data.materials.len()
                || primitive
                    .indices
                    .iter()
                    .any(|index| *index as usize >= count)
            {
                return Err(
                    "Primitive vertices, indices, and material references must be valid.".into(),
                );
            }
            let size = match primitive.topology {
                Topology::Points => 1,
                Topology::Lines => 2,
                Topology::Triangles => 3,
            };
            if !primitive.indices.len().is_multiple_of(size) {
                return Err("Primitive indices must match their topology.".into());
            }
            for vertex in &primitive.vertices {
                if vertex.position.iter().any(|v| !v.is_finite())
                    || vertex
                        .normal
                        .iter()
                        .chain(&vertex.tangent)
                        .chain(&vertex.uv0)
                        .chain(&vertex.uv1)
                        .chain(&vertex.color)
                        .any(|v| !v.is_finite())
                    || vertex.color.iter().any(|v| !(0. ..=1.).contains(v))
                    || ![-1., 1.].contains(&vertex.tangent[3])
                    || (!primitive.flat_normals && vertex.normal.iter().all(|v| *v == 0.))
                {
                    return Err("Primitive attributes must be finite with valid color, normal, and tangent values.".into());
                }
            }
            if primitive.morphs.len() != mesh.weights.len() {
                return Err("Primitive morph targets must match its mesh weights.".into());
            }
            for morph in &primitive.morphs {
                budget.owned::<Morph>(1)?;
                budget.owned::<[f64; 3]>(morph.positions.len())?;
                budget.owned::<[f32; 3]>(morph.normals.len())?;
                budget.owned::<[f32; 3]>(morph.tangents.len())?;
                if [
                    morph.positions.len(),
                    morph.normals.len(),
                    morph.tangents.len(),
                ]
                .iter()
                .any(|length| *length != 0 && *length != count)
                    || morph.positions.iter().flatten().any(|v| !v.is_finite())
                    || morph
                        .normals
                        .iter()
                        .chain(&morph.tangents)
                        .flatten()
                        .any(|v| !v.is_finite())
                {
                    return Err(
                        "Morph arrays must be finite and match the primitive's vertex count."
                            .into(),
                    );
                }
            }
            if !primitive.influences.is_empty() {
                if primitive.influences.len() != count {
                    return Err(
                        "Skin influence rows must match the primitive's vertex count.".into(),
                    );
                }
                budget.owned::<Vec<([u16; 4], [f32; 4])>>(count)?;
                let expected = primitive.influences[0].len();
                if expected == 0 || expected > 8 {
                    return Err("Skin influence set count is outside the supported budget.".into());
                }
                for sets in &primitive.influences {
                    budget.owned::<([u16; 4], [f32; 4])>(sets.len())?;
                    let weights = || sets.iter().flat_map(|(_, weights)| weights).copied();
                    let sum: f64 = weights().map(f64::from).sum();
                    if sets.len() != expected
                        || weights().any(|v| !v.is_finite() || v < 0.)
                        || (sum - 1.).abs() > 1e-5
                    {
                        return Err("Skin weights must be finite, nonnegative, normalized, and consistently shaped.".into());
                    }
                }
            }
        }
    }
    for skin in &data.skins {
        budget.owned::<DMat4>(skin.inverse_bind.len())?;
        budget.owned::<usize>(skin.joints.len())?;
        if skin.joints.is_empty()
            || skin.joints.len() > 1024
            || skin.inverse_bind.len() != skin.joints.len()
            || skin.joints.iter().any(|id| *id >= data.nodes.len())
            || skin.joints.iter().copied().collect::<BTreeSet<_>>().len() != skin.joints.len()
            || skin.inverse_bind.iter().any(|matrix| !affine(*matrix))
        {
            return Err(
                "Skin joints and affine inverse-bind matrices must form valid matching arrays."
                    .into(),
            );
        }
    }
    for node in &data.nodes {
        let valid_transform = match node.transform {
            Transform::Matrix(matrix) => affine(matrix),
            Transform::Trs {
                translation,
                rotation,
                scale,
            } => {
                translation.is_finite()
                    && scale.is_finite()
                    && rotation.is_finite()
                    && (rotation.length_squared() - 1.).abs() <= 1e-6
            }
        };
        if !valid_transform {
            return Err(
                "Node transforms must be finite affine matrices or TRS with unit rotations.".into(),
            );
        }
        for (id, count) in [
            (node.mesh, data.meshes.len()),
            (node.skin, data.skins.len()),
            (node.camera, data.cameras.len()),
            (node.light, data.lights.len()),
        ] {
            if id.is_some_and(|id| id >= count) {
                return Err("A node references an unavailable scene resource.".into());
            }
        }
        if let Some(weights) = &node.weights {
            let mesh = node.mesh.ok_or("Node morph weights require a mesh.")?;
            if weights.len() != data.meshes[mesh].weights.len()
                || weights.iter().any(|v| !v.is_finite())
            {
                return Err("Node morph weights must be finite and match its mesh.".into());
            }
        }
        if let Some(skin) = node.skin {
            let mesh = node.mesh.ok_or("Skinning requires a mesh instance.")?;
            for primitive in &data.meshes[mesh].primitives {
                if primitive.influences.len() != primitive.vertices.len()
                    || primitive.influences.iter().flatten().any(|(ids, _)| {
                        ids.iter()
                            .any(|id| usize::from(*id) >= data.skins[skin].joints.len())
                    })
                {
                    return Err(
                        "Skin influence indices must reference the instance's joint array.".into(),
                    );
                }
            }
        }
    }
    let mut image_bytes = budgets::Budget::default();
    for image in &data.images {
        let pixels = (image.width as usize)
            .checked_mul(image.height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or("Image byte count overflow.")?;
        if image.width == 0
            || image.height == 0
            || image.width > 8192
            || image.height > 8192
            || image.rgba8.len() != pixels
        {
            return Err("Scene images require matching RGBA8 pixels within 8192 by 8192.".into());
        }
        image_bytes.charge(pixels, 1, 256 * 1024 * 1024)?;
    }
    if data
        .textures
        .iter()
        .any(|texture| texture.image >= data.images.len())
    {
        return Err("A texture references an unavailable image.".into());
    }
    for material in &data.materials {
        if material
            .base_color
            .iter()
            .chain([
                &material.metallic,
                &material.roughness,
                &material.occlusion_strength,
            ])
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || material.emissive.iter().any(|v| !v.is_finite() || *v < 0.)
            || !material.normal_scale.is_finite()
            || !material.alpha_cutoff.is_finite()
            || material.alpha_cutoff < 0.
        {
            return Err("Material factors must be finite and within their physical ranges.".into());
        }
        for texture in [
            material.base_color_texture,
            material.metallic_roughness_texture,
            material.normal_texture,
            material.occlusion_texture,
            material.emissive_texture,
        ]
        .into_iter()
        .flatten()
        {
            if texture.texture >= data.textures.len()
                || texture.tex_coord > 1
                || texture
                    .transform
                    .offset
                    .iter()
                    .chain(&texture.transform.scale)
                    .chain([&texture.transform.rotation])
                    .any(|v| !v.is_finite())
            {
                return Err("Material texture references and UV transforms must be valid.".into());
            }
        }
    }
    for camera in &data.cameras {
        let valid = match camera.projection {
            Projection::Perspective {
                yfov,
                aspect,
                near_cm,
                far_cm,
            } => {
                yfov.is_finite()
                    && yfov > 0.
                    && yfov < std::f32::consts::PI
                    && aspect.is_none_or(|v| v.is_finite() && v > 0.)
                    && near_cm.is_finite()
                    && near_cm > 0.
                    && far_cm.is_none_or(|v| v.is_finite() && v > near_cm)
            }
            Projection::Orthographic {
                xmag_cm,
                ymag_cm,
                near_cm,
                far_cm,
            } => {
                [xmag_cm, ymag_cm, near_cm, far_cm]
                    .iter()
                    .all(|v| v.is_finite())
                    && xmag_cm > 0.
                    && ymag_cm > 0.
                    && near_cm >= 0.
                    && far_cm > near_cm
            }
        };
        if !valid {
            return Err(
                "Camera projection must have finite, ordered dimensions in centimeters.".into(),
            );
        }
    }
    for light in &data.lights {
        if light.color.iter().any(|v| !v.is_finite() || *v < 0.)
            || !light.intensity.is_finite()
            || light.intensity < 0.
            || light.range_cm.is_some_and(|v| !v.is_finite() || v <= 0.)
        {
            return Err("Light color, intensity, and range must be finite and nonnegative.".into());
        }
        if let LightKind::Spot { inner, outer } = light.kind
            && (!inner.is_finite()
                || !outer.is_finite()
                || inner < 0.
                || inner >= outer
                || outer > std::f32::consts::FRAC_PI_2)
        {
            return Err("Spot light cone angles must be finite and ordered.".into());
        }
    }
    for animation in &data.animations {
        if !animation.start.is_finite()
            || !animation.duration.is_finite()
            || animation.duration < 0.
            || !(animation.start + animation.duration).is_finite()
            || animation.channels.is_empty()
            || animation.channels.len() > 16384
        {
            return Err("Animation requires a finite duration and bounded channels.".into());
        }
        let mut targets = BTreeSet::new();
        let mut start = f32::INFINITY;
        let mut end = f32::NEG_INFINITY;
        for channel in &animation.channels {
            budget.owned::<Channel>(1)?;
            budget.owned::<f32>(channel.times.len())?;
            for values in [&channel.values, &channel.in_tangents, &channel.out_tangents] {
                budget.owned::<f64>(values.len())?;
            }
            if channel.node >= data.nodes.len()
                || !targets.insert((channel.node, channel.property as u8))
            {
                return Err("Animation targets must be valid, unique node properties.".into());
            }
            let node = &data.nodes[channel.node];
            let components = match channel.property {
                Property::Translation | Property::Scale => 3,
                Property::Rotation => 4,
                Property::Weights => node.mesh.map_or(0, |id| data.meshes[id].weights.len()),
            };
            let count = channel
                .times
                .len()
                .checked_mul(components)
                .ok_or("Animation value count overflow.")?;
            if components == 0
                || components != channel.components
                || channel.times.is_empty()
                || channel.values.len() != count
                || channel.times.iter().any(|v| !v.is_finite())
                || channel.times.windows(2).any(|pair| pair[0] >= pair[1])
                || channel
                    .values
                    .iter()
                    .chain(&channel.in_tangents)
                    .chain(&channel.out_tangents)
                    .any(|v| !v.is_finite())
            {
                return Err(
                    "Animation keys and component arrays must be finite, ordered, and matching."
                        .into(),
                );
            }
            start = start.min(channel.times[0]);
            end = end.max(*channel.times.last().unwrap());
            if (channel.interpolation == Interpolation::CubicHermite
                && (channel.in_tangents.len() != count || channel.out_tangents.len() != count))
                || (channel.interpolation != Interpolation::CubicHermite
                    && (!channel.in_tangents.is_empty() || !channel.out_tangents.is_empty()))
            {
                return Err("Hermite curves require a derivative vector at each key; other curves have no tangents.".into());
            }
            if channel.property != Property::Weights
                && matches!(node.transform, Transform::Matrix(_))
            {
                return Err("TRS animation requires a TRS node transform.".into());
            }
            if channel.property == Property::Rotation
                && channel
                    .values
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|q| (DQuat::from_array(*q).length_squared() - 1.).abs() > 1e-3)
            {
                return Err("Rotation keyframes require unit quaternions.".into());
            }
        }
        if animation.start != start || animation.duration != end - start {
            return Err("Animation range must match its channel key interval.".into());
        }
    }
    Ok(())
}

fn affine(matrix: DMat4) -> bool {
    matrix.is_finite()
        && matrix.x_axis.w == 0.
        && matrix.y_axis.w == 0.
        && matrix.z_axis.w == 0.
        && matrix.w_axis.w == 1.
}
