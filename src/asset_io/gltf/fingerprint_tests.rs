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

#[test]
fn chess_set_glb_preserves_external_scene_through_browser_byte_loading() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/benchmarks/chess-set");
    let source = load_path(&root.join("source/chess_set_1k.gltf")).unwrap();
    let bytes = std::fs::read(root.join("chess-set-1k.glb")).unwrap();
    let loaded = crate::asset_io::load_bytes("chess-set-1k.glb", &bytes).unwrap();
    assert!(loaded.diagnostics.is_empty());
    assert_eq!(loaded.document.objects.len(), 1);
    assert_eq!(loaded.assets.len(), 1);
    let embedded = loaded.assets.values().next().unwrap();

    for asset in [&source, embedded.as_ref()] {
        assert_eq!(asset.scenes.len(), 1);
        assert_eq!(asset.default_scene, 0);
        assert_eq!(asset.nodes.len(), 33);
        assert_eq!(
            asset
                .nodes
                .iter()
                .filter(|node| node.mesh.is_some())
                .count(),
            33
        );
        assert_eq!(asset.meshes.len(), 33);
        // The importer appends one fallback material after the three authored ones.
        assert_eq!(asset.materials.len(), 4);
        assert_eq!(asset.textures.len(), 9);
        assert_eq!(asset.images.len(), 9);
        assert!(asset.animations.is_empty());
        assert!(asset.skins.is_empty());
        let frame = asset.evaluate(asset.default_scene, None).unwrap();
        assert_eq!(frame.draws.len(), 37);
        assert_eq!(
            frame
                .draws
                .iter()
                .map(|draw| draw.vertices.len())
                .sum::<usize>(),
            49_150
        );
        assert_eq!(
            frame
                .draws
                .iter()
                .map(|draw| draw.indices.len() / 3)
                .sum::<usize>(),
            76_920
        );
        assert!(
            frame
                .draws
                .iter()
                .all(|draw| draw.topology == Topology::Triangles)
        );
    }
    let source_frame = source.evaluate(source.default_scene, None).unwrap();
    let embedded_frame = embedded.evaluate(embedded.default_scene, None).unwrap();
    assert_eq!(source_frame.bounds, embedded_frame.bounds);
    assert_eq!(
        frame_fingerprints(&source_frame),
        frame_fingerprints(&embedded_frame)
    );

    for (original, packed) in source.materials.iter().zip(&embedded.materials) {
        assert_eq!(original.name, packed.name);
        assert_eq!(original.base_color, packed.base_color);
        assert_eq!(original.metallic, packed.metallic);
        assert_eq!(original.roughness, packed.roughness);
        assert_eq!(original.emissive, packed.emissive);
        assert_eq!(original.normal_scale, packed.normal_scale);
        assert_eq!(original.occlusion_strength, packed.occlusion_strength);
        assert_eq!(original.alpha_mode, packed.alpha_mode);
        assert_eq!(original.alpha_cutoff, packed.alpha_cutoff);
        assert_eq!(original.double_sided, packed.double_sided);
        assert_eq!(original.unlit, packed.unlit);
        let slots = |material: &Material| {
            [
                material.base_color_texture,
                material.metallic_roughness_texture,
                material.normal_texture,
                material.occlusion_texture,
                material.emissive_texture,
            ]
            .map(|slot| slot.map(|info| (info.texture, info.tex_coord, info.transform)))
        };
        assert_eq!(slots(original), slots(packed));
    }
    for (original, packed) in source.textures.iter().zip(&embedded.textures) {
        assert_eq!(original.image, packed.image);
        assert_eq!(original.sampler.wrap_s, packed.sampler.wrap_s);
        assert_eq!(original.sampler.wrap_t, packed.sampler.wrap_t);
        assert_eq!(original.sampler.mag, packed.sampler.mag);
        assert_eq!(original.sampler.min, packed.sampler.min);
        assert_eq!(original.sampler.mipmap, packed.sampler.mipmap);
    }
    for (original, packed) in source.images.iter().zip(&embedded.images) {
        assert_eq!(original.name, packed.name);
        assert_eq!(original.width, packed.width);
        assert_eq!(original.height, packed.height);
        assert_eq!(original.rgba8.len(), packed.rgba8.len());
        assert_eq!(
            Sha256::digest(&original.rgba8),
            Sha256::digest(&packed.rgba8)
        );
    }
}
