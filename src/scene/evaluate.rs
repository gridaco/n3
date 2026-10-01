//! CPU deformation is deliberately bounded for the first read-only scene host.
//! Immutable assets cache their static evaluations; animated frames recompute
//! from authored values, never from the preceding frame's transformed vertices.
use super::*;

impl SceneAsset {
    /// `time` is absolute clip time in seconds, clamped per channel. The
    /// playback host owns looping; evaluation itself never wraps or advances.
    pub(crate) fn evaluate(
        &self,
        scene: usize,
        sample: Option<AnimationSample>,
    ) -> Result<Arc<EvaluatedScene>> {
        if scene >= self.scenes.len() {
            return Err("The requested scene does not exist.".into());
        }
        if sample.is_none() {
            let mut cached = self
                .static_scene
                .lock()
                .map_err(|_| "Scene cache lock was poisoned.")?;
            if let Some((index, frame)) = &*cached
                && *index == scene
            {
                return Ok(frame.clone());
            }
            let frame = Arc::new(evaluate(self, scene, None)?);
            *cached = Some((scene, frame.clone()));
            return Ok(frame);
        }
        Ok(Arc::new(evaluate(self, scene, sample)?))
    }
}

fn evaluate(
    asset: &SceneAsset,
    scene: usize,
    sample: Option<AnimationSample>,
) -> Result<EvaluatedScene> {
    let mut transforms: Vec<_> = asset
        .nodes
        .iter()
        .map(|node| node.transform.clone())
        .collect();
    let mut weights: Vec<_> = asset
        .nodes
        .iter()
        .map(|node| {
            node.weights
                .clone()
                .unwrap_or_else(|| {
                    node.mesh
                        .map_or_else(Vec::new, |mesh| asset.meshes[mesh].weights.clone())
                })
                .into_iter()
                .map(f64::from)
                .collect::<Vec<_>>()
        })
        .collect();
    if let Some(sample) = sample {
        if !sample.time.is_finite() {
            return Err("Animation time must be finite.".into());
        }
        let clip = asset
            .animations
            .get(sample.clip)
            .ok_or("The requested animation does not exist.")?;
        for channel in &clip.channels {
            let value = sample_channel(channel, sample.time)?;
            if channel.property == Property::Weights {
                weights[channel.node] = value;
                continue;
            }
            let Transform::Trs {
                translation,
                rotation,
                scale,
            } = &mut transforms[channel.node]
            else {
                return Err("TRS animation cannot target a matrix node.".into());
            };
            match channel.property {
                Property::Translation => *translation = DVec3::new(value[0], value[1], value[2]),
                Property::Scale => *scale = DVec3::new(value[0], value[1], value[2]),
                Property::Rotation => {
                    *rotation =
                        DQuat::from_array([value[0], value[1], value[2], value[3]]).normalize()
                }
                Property::Weights => unreachable!(),
            }
        }
    }
    let mut world = vec![DMat4::IDENTITY; asset.nodes.len()];
    let mut stack: Vec<_> = validate::hierarchy_roots(&asset.nodes)?
        .into_iter()
        .map(|node| (node, DMat4::IDENTITY))
        .collect();
    while let Some((node, parent)) = stack.pop() {
        let local = match transforms[node] {
            Transform::Matrix(matrix) => matrix,
            Transform::Trs {
                translation,
                rotation,
                scale,
            } => DMat4::from_scale_rotation_translation(scale, rotation, translation),
        };
        world[node] = parent * local;
        if !world[node].is_finite() {
            return Err("The node hierarchy produces a nonfinite transform.".into());
        }
        stack.extend(
            asset.nodes[node]
                .children
                .iter()
                .map(|child| (*child, world[node])),
        );
    }
    let mut active = vec![false; asset.nodes.len()];
    let mut stack = asset.scenes[scene].roots.clone();
    while let Some(node) = stack.pop() {
        active[node] = true;
        stack.extend(&asset.nodes[node].children);
    }
    budgets::evaluated(asset, &active)?;
    let mut frame = EvaluatedScene::default();
    for (node_id, node) in asset
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| active[*index])
    {
        if let Some(camera) = node.camera {
            frame.cameras.push(EvaluatedCamera {
                node: node_id,
                camera,
                world: world[node_id],
            });
        }
        if let Some(light) = node.light {
            frame.lights.push(EvaluatedLight {
                node: node_id,
                light,
                position_cm: world[node_id].transform_point3(DVec3::ZERO),
                direction: world[node_id]
                    .transform_vector3(DVec3::NEG_Z)
                    .normalize_or(DVec3::NEG_Z),
            });
        }
        let Some(mesh_id) = node.mesh else {
            continue;
        };
        let mesh = &asset.meshes[mesh_id];
        let joints = if let Some(skin_id) = node.skin {
            let skin = &asset.skins[skin_id];
            if skin.joints.iter().any(|joint| !active[*joint]) {
                return Err(
                    "Every skin joint must belong to the scene containing its skinned mesh.".into(),
                );
            }
            // Inverse-bind matrices map local mesh centimeters into joint
            // bind space; joint world matrices then produce world centimeters.
            // Mesh-node transform is not applied a second time.
            Some(
                skin.joints
                    .iter()
                    .zip(&skin.inverse_bind)
                    .map(|(joint, inverse_bind)| world[*joint] * *inverse_bind)
                    .collect::<Vec<_>>(),
            )
        } else {
            None
        };
        for (primitive_id, primitive) in mesh.primitives.iter().enumerate() {
            let mut vertices = Vec::with_capacity(primitive.vertices.len());
            for (index, vertex) in primitive.vertices.iter().enumerate() {
                let mut position = DVec3::from_array(vertex.position);
                let mut normal = DVec3::from_array(vertex.normal.map(f64::from));
                let mut tangent = DVec3::new(
                    vertex.tangent[0] as f64,
                    vertex.tangent[1] as f64,
                    vertex.tangent[2] as f64,
                );
                for (morph, weight) in primitive.morphs.iter().zip(&weights[node_id]) {
                    if *weight == 0. {
                        continue;
                    }
                    if let Some(delta) = morph.positions.get(index) {
                        position += DVec3::from_array(*delta) * *weight;
                    }
                    if let Some(delta) = morph.normals.get(index) {
                        normal += DVec3::from_array(delta.map(f64::from)) * *weight;
                    }
                    if let Some(delta) = morph.tangents.get(index) {
                        tangent += DVec3::from_array(delta.map(f64::from)) * *weight;
                    }
                }
                if !position.is_finite() || !normal.is_finite() || !tangent.is_finite() {
                    return Err("Morph evaluation produced nonfinite attributes.".into());
                }
                let transform = if let Some(joints) = &joints {
                    let mut transform = DMat4::ZERO;
                    for (indices, weights) in &primitive.influences[index] {
                        for i in 0..4 {
                            if weights[i] != 0. {
                                transform += joints[indices[i] as usize] * weights[i] as f64;
                            }
                        }
                    }
                    transform
                } else {
                    world[node_id]
                };
                position = transform.transform_point3(position);
                normal = transform_normal(transform, normal);
                let transformed_tangent = transform.transform_vector3(tangent);
                tangent = (transformed_tangent - normal * normal.dot(transformed_tangent))
                    .normalize_or_zero();
                let handedness = vertex.tangent[3]
                    * if transform.determinant() < 0. {
                        -1.
                    } else {
                        1.
                    };
                if !position.is_finite() || !normal.is_finite() || !tangent.is_finite() {
                    return Err("Scene evaluation produced nonfinite geometry.".into());
                }
                if let Some(bounds) = &mut frame.bounds {
                    bounds.min = bounds.min.min(position);
                    bounds.max = bounds.max.max(position);
                } else {
                    frame.bounds = Some(Bounds {
                        min: position,
                        max: position,
                    });
                }
                vertices.push(SceneVertex {
                    position: position.to_array(),
                    normal: normal.as_vec3().to_array(),
                    tangent: [
                        tangent.x as f32,
                        tangent.y as f32,
                        tangent.z as f32,
                        handedness,
                    ],
                    ..*vertex
                });
            }
            // Runtime mesh instances define front-face winding by the mesh
            // node's global determinant, including skinned instances. Joint
            // deformation can invert individual triangles; do not guess a
            // per-vertex winding repair from blended skin matrices.
            let mirrored = world[node_id].determinant() < 0.;
            let indices = if primitive.flat_normals && primitive.topology == Topology::Triangles {
                let mut flat = Vec::with_capacity(primitive.indices.len());
                for triangle in primitive.indices.as_chunks::<3>().0 {
                    let a = vertices[triangle[0] as usize];
                    let b = vertices[triangle[1] as usize];
                    let c = vertices[triangle[2] as usize];
                    let normal = (DVec3::from_array(b.position) - DVec3::from_array(a.position))
                        .cross(DVec3::from_array(c.position) - DVec3::from_array(a.position))
                        .normalize_or(DVec3::Z)
                        * if mirrored { -1. } else { 1. };
                    flat.extend([a, b, c].map(|mut vertex| {
                        vertex.normal = normal.as_vec3().to_array();
                        vertex
                    }));
                }
                vertices = flat;
                (0..vertices.len() as u32).collect::<Vec<_>>().into()
            } else {
                primitive.indices.clone()
            };
            frame.draws.push(EvaluatedDraw {
                node: node_id,
                mesh: mesh_id,
                primitive: primitive_id,
                material: primitive.material,
                topology: primitive.topology,
                vertices,
                indices,
                mirrored,
            });
        }
    }
    frame.node_world = world;
    Ok(frame)
}

fn transform_normal(transform: DMat4, normal: DVec3) -> DVec3 {
    let x = transform.x_axis.truncate();
    let y = transform.y_axis.truncate();
    let z = transform.z_axis.truncate();
    let cofactor = glam::DMat3::from_cols(y.cross(z), z.cross(x), x.cross(y));
    let result = cofactor
        * normal
        * if transform.determinant() < 0. {
            -1.
        } else {
            1.
        };
    // Animation scales may be singular. Collapsed triangles have no
    // defined normal; a finite fallback avoids NaNs while their zero area hides
    // them naturally. Nonsingular matrices use the exact inverse-transpose direction.
    result.normalize_or(DVec3::Z)
}

fn sample_channel(channel: &Channel, time: f32) -> Result<Vec<f64>> {
    let width = channel.components;
    let value = |key: usize| &channel.values[key * width..(key + 1) * width];
    let next = channel.times.partition_point(|key| *key <= time);
    if next == 0 {
        return Ok(value(0).to_vec());
    }
    if next == channel.times.len() {
        return Ok(value(next - 1).to_vec());
    }
    let previous = next - 1;
    if channel.interpolation == Interpolation::Step {
        return Ok(value(previous).to_vec());
    }
    let dt = f64::from(channel.times[next]) - f64::from(channel.times[previous]);
    let t = ((f64::from(time) - f64::from(channel.times[previous])) / dt).clamp(0., 1.);
    let before = value(previous);
    let after = value(next);
    if channel.property == Property::Rotation && channel.interpolation == Interpolation::Linear {
        let a = DQuat::from_array(std::array::from_fn(|i| before[i])).normalize();
        let b = DQuat::from_array(std::array::from_fn(|i| after[i])).normalize();
        return Ok(a.slerp(b, t).normalize().to_array().to_vec());
    }
    let mut result: Vec<f64> = (0..width)
        .map(|i| {
            if channel.interpolation == Interpolation::CubicHermite {
                let t2 = t * t;
                let t3 = t2 * t;
                (2. * t3 - 3. * t2 + 1.) * before[i]
                    + (t3 - 2. * t2 + t) * dt * channel.out_tangents[previous * width + i]
                    + (-2. * t3 + 3. * t2) * after[i]
                    + (t3 - t2) * dt * channel.in_tangents[next * width + i]
            } else {
                before[i] * (1. - t) + after[i] * t
            }
        })
        .collect();
    if result.iter().any(|value| !value.is_finite()) {
        return Err("Animation interpolation produced a nonfinite value.".into());
    }
    if channel.property == Property::Rotation {
        let rotation = DQuat::from_array(std::array::from_fn(|i| result[i]));
        if !rotation.length_squared().is_finite() || rotation.length_squared() < 1e-20 {
            return Err("Cubic rotation interpolation produced an invalid quaternion.".into());
        }
        result = rotation.normalize().to_array().to_vec();
    }
    Ok(result)
}
