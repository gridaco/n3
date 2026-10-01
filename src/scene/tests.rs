use super::test_support::{animated_asset, triangle_data};
use super::*;

#[test]
fn directly_constructed_scene_uses_centimeters_and_validated_immutable_cache() {
    let asset = SceneAsset::new(triangle_data()).unwrap();
    let frame = asset.evaluate(0, None).unwrap();
    assert_eq!(frame.draws[0].vertices[1].position, [2., 0., 0.]);
    assert_eq!(frame.draws[0].vertices[2].position, [0., 3., 0.]);
    assert!(Arc::ptr_eq(&frame, &asset.evaluate(0, None).unwrap()));
    let animation = animated_asset();
    let pose = animation
        .evaluate(0, Some(AnimationSample { clip: 0, time: 1. }))
        .unwrap();
    assert_eq!(pose.draws[0].vertices[0].position, [50., 0., 0.]);
    assert_eq!(animation.nodes[0].transform.matrix_for_test().w_axis.x, 0.);
}

impl Transform {
    fn matrix_for_test(&self) -> DMat4 {
        match *self {
            Self::Matrix(matrix) => matrix,
            Self::Trs {
                translation,
                rotation,
                scale,
            } => DMat4::from_scale_rotation_translation(scale, rotation, translation),
        }
    }
}

#[test]
fn direct_skin_and_morph_evaluation_stays_in_centimeters() {
    let mut data = triangle_data();
    let mesh = &mut data.meshes[0];
    mesh.weights = vec![0.5];
    let primitive = &mut mesh.primitives[0];
    primitive.morphs = vec![Morph {
        positions: vec![[0., 0., 2.]; 3],
        normals: vec![],
        tangents: vec![],
    }];
    primitive.influences = vec![vec![([0; 4], [1., 0., 0., 0.])]; 3];
    data.nodes[0].skin = Some(0);
    let mut joint = data.nodes[0].clone();
    joint.mesh = None;
    joint.skin = None;
    joint.transform = Transform::Trs {
        translation: DVec3::new(10., 0., 0.),
        rotation: DQuat::IDENTITY,
        scale: DVec3::ONE,
    };
    data.nodes.push(joint);
    data.scenes[0].roots.push(1);
    data.skins = vec![Skin {
        name: "Test skin".into(),
        joints: vec![1],
        inverse_bind: vec![DMat4::from_translation(DVec3::new(-3., 0., 0.))],
    }];
    let asset = SceneAsset::new(data).unwrap();
    let frame = asset.evaluate(0, None).unwrap();
    assert_eq!(frame.draws[0].vertices[0].position, [7., 0., 1.]);
    assert_eq!(frame.draws[0].vertices[1].position, [9., 0., 1.]);
}

#[test]
fn constructor_rejects_invalid_references_shapes_and_nonfinite_values_without_importer() {
    type Change = fn(&mut SceneData);
    let changes: [Change; 13] = [
        |data| data.default_scene = 99,
        |data| data.scenes[0].roots.push(99),
        |data| data.nodes[0].children.push(0),
        |data| data.nodes[0].mesh = Some(99),
        |data| data.nodes[0].camera = Some(99),
        |data| data.nodes[0].weights = Some(vec![1.]),
        |data| data.meshes[0].primitives[0].material = 99,
        |data| data.meshes[0].primitives[0].indices = vec![0, 1, 99].into(),
        |data| data.meshes[0].primitives[0].indices = vec![0, 1].into(),
        |data| data.meshes[0].primitives[0].vertices[0].position[0] = f64::NAN,
        |data| data.meshes[0].weights = vec![1.],
        |data| data.materials[0].base_color[0] = f32::NAN,
        |data| {
            data.materials[0].base_color_texture = Some(TextureInfo {
                texture: 0,
                tex_coord: 0,
                transform: TextureTransform::default(),
            })
        },
    ];
    for change in changes {
        let mut data = triangle_data();
        change(&mut data);
        assert!(SceneAsset::new(data).is_err());
    }
    let valid = animated_asset();
    for change in [
        (|data: &mut SceneData| data.animations[0].channels[0].node = 99) as Change,
        |data| data.animations[0].channels[0].values.clear(),
        |data| data.animations[0].channels[0].times[1] = 0.,
        |data| data.animations[0].channels[0].interpolation = Interpolation::CubicHermite,
    ] {
        let mut data = (*valid).clone();
        change(&mut data);
        assert!(SceneAsset::new(data).is_err());
    }
}

#[test]
fn runtime_hermite_curves_have_separate_centimeter_per_second_derivatives() {
    let asset = animated_asset();
    let mut data = (*asset).clone();
    let channel = &mut data.animations[0].channels[0];
    channel.interpolation = Interpolation::CubicHermite;
    channel.in_tangents = vec![0.; 6];
    channel.out_tangents = vec![100., 0., 0., 0., 0., 0.];
    let asset = SceneAsset::new(data).unwrap();
    let pose = asset
        .evaluate(0, Some(AnimationSample { clip: 0, time: 1. }))
        .unwrap();
    assert_eq!(pose.draws[0].vertices[0].position, [75., 0., 0.]);
}

#[test]
fn singular_animation_scales_remain_supported_without_source_format() {
    let asset = animated_asset();
    let mut data = (*asset).clone();
    let channel = &mut data.animations[0].channels[0];
    channel.property = Property::Scale;
    channel.values = vec![1., 1., 1., -1., 2., 1.];
    let asset = SceneAsset::new(data).unwrap();
    let pose = asset
        .evaluate(0, Some(AnimationSample { clip: 0, time: 1. }))
        .unwrap();
    assert!(
        pose.draws[0]
            .vertices
            .iter()
            .flat_map(|v| v.normal)
            .all(f32::is_finite)
    );
}

#[test]
fn authored_animation_metadata_and_extreme_hermite_rotations_fail_explicitly() {
    let asset = animated_asset();
    let mut data = (*asset).clone();
    data.animations[0].duration = 1.;
    assert!(SceneAsset::new(data).unwrap_err().contains("range"));
    let mut data = (*asset).clone();
    let channel = &mut data.animations[0].channels[0];
    channel.property = Property::Rotation;
    channel.components = 4;
    channel.values = vec![0., 0., 0., 1., 0., 0., 0., 1.];
    channel.interpolation = Interpolation::CubicHermite;
    channel.in_tangents = vec![0.; 8];
    channel.out_tangents = vec![1e200, 0., 0., 0., 0., 0., 0., 0.];
    let asset = SceneAsset::new(data).unwrap();
    assert!(
        asset
            .evaluate(0, Some(AnimationSample { clip: 0, time: 1. }))
            .unwrap_err()
            .contains("invalid quaternion")
    );
}

#[test]
fn runtime_deformation_budget_counts_instance_amplification_before_work() {
    let mut data = triangle_data();
    let primitive = &mut data.meshes[0].primitives[0];
    primitive.vertices.resize(999, primitive.vertices[0]);
    primitive.morphs = (0..64)
        .map(|_| Morph {
            positions: vec![[0.; 3]; 999],
            normals: vec![[0.; 3]; 999],
            tangents: vec![[0.; 3]; 999],
        })
        .collect();
    data.meshes[0].weights = vec![0.; 64];
    data.nodes = vec![data.nodes[0].clone(); 256];
    data.scenes[0].roots = (0..256).collect();
    assert!(SceneAsset::new(data).unwrap_err().contains("work budget"));
}
