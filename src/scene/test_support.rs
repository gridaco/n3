//! Internal scene producers for evaluator and playback tests. No file adapter.
use super::*;

pub(crate) fn triangle_data() -> SceneData {
    SceneData {
        name: "Internal scene".into(),
        scenes: vec![SceneDefinition {
            name: "Scene".into(),
            roots: vec![0],
        }],
        nodes: vec![Node {
            name: "Triangle".into(),
            children: Vec::new(),
            mesh: Some(0),
            skin: None,
            camera: None,
            light: None,
            weights: None,
            transform: Transform::Trs {
                translation: DVec3::ZERO,
                rotation: DQuat::IDENTITY,
                scale: DVec3::ONE,
            },
        }],
        meshes: vec![Mesh {
            name: "Triangle mesh".into(),
            weights: Vec::new(),
            primitives: vec![Primitive {
                topology: Topology::Triangles,
                material: 0,
                vertices: [[0., 0., 0.], [2., 0., 0.], [0., 3., 0.]]
                    .map(|position| SceneVertex {
                        position,
                        normal: [0., 0., 1.],
                        tangent: [1., 0., 0., 1.],
                        uv0: [0.; 2],
                        uv1: [0.; 2],
                        color: [1.; 4],
                    })
                    .to_vec(),
                indices: Arc::from([0, 1, 2]),
                morphs: Vec::new(),
                influences: Vec::new(),
                flat_normals: false,
            }],
        }],
        materials: vec![Material {
            name: "Surface".into(),
            base_color: [1.; 4],
            metallic: 0.,
            roughness: 0.8,
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
        }],
        ..Default::default()
    }
}

pub(crate) fn animated_asset() -> SceneAsset {
    let mut data = triangle_data();
    data.animations.push(Animation {
        name: "Move".into(),
        start: 0.,
        duration: 2.,
        channels: vec![Channel {
            node: 0,
            property: Property::Translation,
            interpolation: Interpolation::Linear,
            times: vec![0., 2.],
            values: vec![0., 0., 0., 100., 0., 0.],
            in_tangents: Vec::new(),
            out_tangents: Vec::new(),
            components: 3,
        }],
    });
    SceneAsset::new(data).unwrap()
}
