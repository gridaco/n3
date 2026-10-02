//! Portable input fingerprints isolate CPU preparation from renderer drift.
use super::*;
use crate::asset_io::native::load_scene_path as load_path;
use sha2::{Digest, Sha256};
use std::path::Path;

fn frame_fingerprints(frame: &EvaluatedScene) -> (String, String) {
    let bounds = frame.bounds.expect("Fixture geometry has world bounds");
    assert!(bounds.min.is_finite() && bounds.max.is_finite());
    let display = crate::render::scene_renderer::ViewTransform::from_bounds(frame.bounds);
    let mut geometry = Sha256::new();
    let mut positions = Sha256::new();
    geometry.update(b"n3 evaluated imported frame v1\0");
    positions.update(b"n3 imported display positions v1\0");
    for matrix in &frame.node_world {
        assert!(matrix.is_finite());
        for value in matrix.to_cols_array() {
            geometry.update(value.to_le_bytes());
        }
    }
    for draw in &frame.draws {
        for id in [draw.node, draw.mesh, draw.primitive, draw.material] {
            geometry.update((id as u64).to_le_bytes());
        }
        geometry.update([
            match draw.topology {
                Topology::Points => 0,
                Topology::Lines => 1,
                Topology::Triangles => 2,
            },
            u8::from(draw.mirrored),
        ]);
        geometry.update((draw.vertices.len() as u64).to_le_bytes());
        for vertex in &draw.vertices {
            let point = DVec3::from_array(vertex.position);
            assert!(point.is_finite());
            assert!(point.cmpge(bounds.min).all() && point.cmple(bounds.max).all());
            for value in vertex.position {
                geometry.update(value.to_le_bytes());
            }
            for value in vertex
                .normal
                .into_iter()
                .chain(vertex.tangent)
                .chain(vertex.uv0)
                .chain(vertex.uv1)
                .chain(vertex.color)
            {
                assert!(value.is_finite());
                geometry.update(value.to_le_bytes());
            }
            let normal = glam::Vec3::from_array(vertex.normal);
            assert!(normal == glam::Vec3::ZERO || (normal.length() - 1.0).abs() < 1e-5);
            let point = display.point(point);
            assert!(point.is_finite() && point.abs().max_element() <= 1.000_001);
            for value in point.to_array() {
                positions.update(value.to_le_bytes());
            }
        }
        geometry.update((draw.indices.len() as u64).to_le_bytes());
        for index in draw.indices.iter() {
            assert!((*index as usize) < draw.vertices.len());
            geometry.update(index.to_le_bytes());
        }
    }
    (
        format!("{:x}", geometry.finalize()),
        format!("{:x}", positions.finalize()),
    )
}

#[test]
fn imported_render_fingerprints_scene_inputs_are_finite_and_reproducible() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/gltf");
    for fixture in [
        "WaterBottle/glTF-Binary/WaterBottle.glb",
        "SimpleSkin/glTF/SimpleSkin.gltf",
        "AnimatedMorphCube/glTF/AnimatedMorphCube.gltf",
    ] {
        let asset = load_path(&root.join(fixture)).unwrap();
        let rest = asset.evaluate(asset.default_scene, None).unwrap();
        assert!(std::sync::Arc::ptr_eq(
            &rest,
            &asset.evaluate(asset.default_scene, None).unwrap()
        ));
        let original = frame_fingerprints(&rest);
        let reloaded = load_path(&root.join(fixture)).unwrap();
        assert_eq!(
            original,
            frame_fingerprints(&reloaded.evaluate(reloaded.default_scene, None).unwrap()),
            "Independent imports reproduce their evaluated geometry"
        );
        println!(
            "imported-render-fingerprint fixture={fixture} pose=rest geometry={} display={}",
            original.0, original.1
        );
        let mut images = Sha256::new();
        images.update(b"n3 decoded imported images v1\0");
        for image in &asset.images {
            assert_eq!(
                image.rgba8.len(),
                image.width as usize * image.height as usize * 4
            );
            images.update(image.width.to_le_bytes());
            images.update(image.height.to_le_bytes());
            images.update(&image.rgba8);
        }
        println!(
            "imported-render-fingerprint fixture={fixture} images={} sha256={:x}",
            asset.images.len(),
            images.finalize()
        );
        if !asset.animations.is_empty() {
            let pose = asset
                .evaluate(
                    asset.default_scene,
                    Some(AnimationSample {
                        clip: 0,
                        time: 1.25,
                    }),
                )
                .unwrap();
            let fingerprints = frame_fingerprints(&pose);
            println!(
                "imported-render-fingerprint fixture={fixture} pose=clip0@1.25 geometry={} display={}",
                fingerprints.0, fingerprints.1
            );
            assert_eq!(
                frame_fingerprints(&rest),
                original,
                "Playback leaves Rest intact"
            );
            assert!(std::sync::Arc::ptr_eq(
                &rest,
                &asset.evaluate(asset.default_scene, None).unwrap()
            ));
        }
    }
}
