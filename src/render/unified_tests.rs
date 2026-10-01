//! Shared-target GPU evidence: formats coexist in world space and depth, and
//! transparent primitive ordering spans asset instances rather than documents.
use super::*;
use crate::{
    camera::View,
    doc_capture::Capture,
    document::{DisplayFrame, Transform},
    scene::{self, AlphaMode, SceneAsset},
};
use std::{
    sync::{Arc, mpsc},
    time::Duration,
};
const SIZE: u32 = 64;
fn display() -> DisplayFrame {
    DisplayFrame {
        center: [0.; 3],
        scale: 0.01,
    }
}
fn asset(color: [f32; 4], blend: bool) -> Arc<SceneAsset> {
    let mut data = scene::test_support::triangle_data();
    data.materials[0].base_color = color;
    data.materials[0].unlit = true;
    data.materials[0].alpha_mode = if blend {
        AlphaMode::Blend
    } else {
        AlphaMode::Opaque
    };
    let primitive = &mut data.meshes[0].primitives[0];
    let vertex = primitive.vertices[0];
    primitive.vertices = [
        [-80., -80., 0.],
        [80., -80., 0.],
        [80., 80., 0.],
        [-80., 80., 0.],
    ]
    .map(|position| scene::SceneVertex { position, ..vertex })
    .to_vec();
    primitive.indices = Arc::from([0, 1, 2, 0, 2, 3]);
    Arc::new(SceneAsset::new(data).unwrap())
}
fn placed(object: u64, asset: Arc<SceneAsset>, z: f64) -> PlacedScene {
    PlacedScene {
        exposure: 0.0,
        object,
        frame: asset.evaluate(0, None).unwrap(),
        asset,
        transform: Transform {
            translation: [0., 0., z],
            ..Default::default()
        },
    }
}
fn proxy_mesh(native: bool, sources: &[PlacedScene]) -> MeshData {
    let mut mesh = MeshData {
        vertices: Vec::new(),
        edges: Vec::new(),
        object_ranges: Vec::new(),
        edit_topology: Vec::new(),
        vertex_count: 0,
        face_count: 0,
        triangle_count: 0,
        object_count: 0,
        source_extent: [200.; 3],
        warnings: Vec::new(),
    };
    if native {
        let points = [
            [-0.8, -0.8, 0.],
            [0.8, -0.8, 0.],
            [0.8, 0.8, 0.],
            [-0.8, 0.8, 0.],
        ];
        mesh.vertices.extend([0, 1, 2, 0, 2, 3].map(|i| Vertex {
            position: points[i],
            normal: [0., 0., 1.],
        }));
        mesh.object_ranges.push(ObjectRange {
            object: 1,
            triangles: 0..6,
            edges: 0..0,
            loose_edges: 0..0,
        });
    }
    for source in sources {
        let start = mesh.vertices.len() as u32;
        for draw in &source.frame.draws {
            for &index in draw.indices.iter() {
                let vertex = draw.vertices[index as usize];
                let position = source
                    .transform
                    .matrix()
                    .transform_point3(glam::DVec3::from_array(vertex.position));
                mesh.vertices.push(Vertex {
                    position: display().world_to_display(position).as_vec3().to_array(),
                    normal: vertex.normal,
                });
            }
        }
        mesh.object_ranges.push(ObjectRange {
            object: source.object,
            triangles: start..mesh.vertices.len() as u32,
            edges: 0..0,
            loose_edges: 0..0,
        });
    }
    mesh
}
fn prepare(renderer: &mut SceneRenderer, capture: &Capture, native: bool, sources: &[PlacedScene]) {
    renderer
        .set_assets(&capture.device, &capture.queue, sources, &display())
        .unwrap();
    renderer
        .set_mesh(&capture.device, &proxy_mesh(native, sources))
        .unwrap();
}
fn pixels(renderer: &SceneRenderer, capture: &Capture) -> Vec<u8> {
    // These coexistence tests exercise imported material appearance explicitly.
    pixels_mode(renderer, capture, ShadingMode::MaterialPreview, false)
}
fn pixels_mode(
    renderer: &SceneRenderer,
    capture: &Capture,
    shading: ShadingMode,
    xray: bool,
) -> Vec<u8> {
    pixels_from(renderer, capture, shading, xray, View::Front, false)
}
fn pixels_from(
    renderer: &SceneRenderer,
    capture: &Capture,
    shading: ShadingMode,
    xray: bool,
    view: View,
    z_up: bool,
) -> Vec<u8> {
    let mut camera = Camera::default();
    camera.set_view(view);
    let mut encoder = capture.device.create_command_encoder(&Default::default());
    renderer
        .render(
            &capture.queue,
            &mut encoder,
            &camera,
            ViewportRenderOptions {
                shading,
                xray,
                show_edges: false,
                show_grid: false,
                z_up,
                background: egui::Color32::BLACK,
            },
        )
        .unwrap();
    let stride = SIZE * 4;
    let buffer = capture.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("unified viewport readback"),
        size: u64::from(stride * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        renderer.view.texture().as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(SIZE),
            },
        },
        renderer.view.texture().size(),
    );
    let submission = capture.queue.submit([encoder.finish()]);
    let (tx, rx) = mpsc::sync_channel(1);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    capture
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(1)).unwrap().unwrap();
    buffer.slice(..).get_mapped_range().unwrap().to_vec()
}
fn center(pixels: &[u8]) -> [u8; 4] {
    pixels[((SIZE / 2 * SIZE + SIZE / 2) * 4) as usize..][..4]
        .try_into()
        .unwrap()
}
#[test]
fn unified_gpu_native_and_imported_surfaces_share_occlusion_and_object_highlights() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    prepare(&mut renderer, &capture, true, &[]);
    let native = center(&pixels(&renderer, &capture));
    let red = asset([1., 0., 0., 1.], false);
    prepare(
        &mut renderer,
        &capture,
        true,
        &[placed(2, red.clone(), -20.)],
    );
    assert_eq!(
        center(&pixels(&renderer, &capture)),
        native,
        "native geometry occludes a farther PBR surface"
    );
    prepare(
        &mut renderer,
        &capture,
        true,
        &[placed(2, red.clone(), 20.)],
    );
    let front = pixels(&renderer, &capture);
    assert_eq!(
        center(&front),
        [255, 0, 0, 255],
        "PBR surface occludes native geometry in the same depth buffer"
    );
    renderer.set_highlights(ObjectHighlights {
        selected: BTreeSet::from([2]),
        hovered: None,
    });
    let selected = pixels(&renderer, &capture);
    assert_ne!(
        selected, front,
        "asset proxy contributes the same selected silhouette"
    );
    assert_eq!(
        center(&selected),
        [255, 0, 0, 255],
        "selection does not replace the imported material"
    );
    renderer.set_visible_objects(Some(&BTreeSet::from([1])));
    assert_eq!(
        center(&pixels(&renderer, &capture)),
        native,
        "Local View hides imported material and proxy together"
    );
    renderer.set_visible_objects(Some(&BTreeSet::from([2])));
    assert_eq!(center(&pixels(&renderer, &capture)), [255, 0, 0, 255]);
}
#[test]
fn unified_gpu_transparency_sorts_across_assets_and_respects_native_depth() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let red = asset([1., 0., 0., 0.5], true);
    let blue = asset([0., 0., 1., 0.5], true);
    let sources = [placed(2, red.clone(), 20.), placed(3, blue.clone(), -20.)];
    prepare(&mut renderer, &capture, false, &sources);
    let combined = center(&pixels(&renderer, &capture));
    assert!(
        combined[0].abs_diff(188) <= 2 && combined[2].abs_diff(137) <= 2,
        "linear blend across shared SRGB attachment: {combined:?}"
    );
    prepare(
        &mut renderer,
        &capture,
        false,
        &[placed(30, blue, -20.), placed(40, red.clone(), 20.)],
    );
    assert_eq!(
        center(&pixels(&renderer, &capture)),
        combined,
        "ordering is depth-based across objects, not their ids"
    );
    prepare(&mut renderer, &capture, true, &[]);
    let native = center(&pixels(&renderer, &capture));
    prepare(&mut renderer, &capture, true, &[placed(2, red, -20.)]);
    assert_eq!(
        center(&pixels(&renderer, &capture)),
        native,
        "native depth rejects background transparent primitives"
    );
}
#[test]
fn unified_gpu_failed_batch_keeps_old_scene_and_placement_keeps_material_resources() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let source = placed(2, asset([1., 0., 0., 1.], false), 0.);
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    let before = pixels(&renderer, &capture);
    let original_resources = renderer.assets.cache_identity(source.object);
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    assert_eq!(
        renderer.assets.cache_identity(source.object),
        original_resources,
        "an unchanged frame does not upload geometry"
    );
    let mut exposed = source.clone();
    exposed.exposure = 1.0;
    prepare(&mut renderer, &capture, false, &[exposed]);
    assert_eq!(
        renderer.assets.cache_identity(source.object),
        original_resources,
        "exposure changes only uniforms"
    );
    let mut moved = source.clone();
    moved.transform.translation[0] = 200.;
    for invalid in [f64::NAN, 1e17] {
        let mut bad = source.clone();
        bad.object = 3;
        bad.transform.translation[0] = invalid;
        assert!(
            renderer
                .set_assets(
                    &capture.device,
                    &capture.queue,
                    &[moved.clone(), bad],
                    &display()
                )
                .is_err(),
            "finite values beyond shader range fail as well as NaN"
        );
        assert_eq!(
            pixels(&renderer, &capture),
            before,
            "the first candidate cannot leak if a later asset fails"
        );
        assert_eq!(
            renderer.assets.cache_identity(source.object),
            original_resources
        );
    }
    prepare(&mut renderer, &capture, false, &[moved]);
    let moved_resources = renderer.assets.cache_identity(source.object);
    assert_eq!(
        moved_resources.0, original_resources.0,
        "placement retains material and texture bindings"
    );
    assert_eq!(
        moved_resources.1, original_resources.1,
        "placement reuses its existing vertex buffer"
    );
    assert_eq!(moved_resources.2, original_resources.2 + 1);
    assert_eq!(
        center(&pixels(&renderer, &capture)),
        [0, 0, 0, 255],
        "placement uses shared centimeters without recentering"
    );
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    assert_eq!(pixels(&renderer, &capture), before);
    let mut duplicate = source.clone();
    duplicate.object = 7;
    prepare(&mut renderer, &capture, false, &[source, duplicate]);
    let duplicated = renderer.assets.cache_identity(7);
    assert_eq!(
        duplicated.0, original_resources.0,
        "duplicates share immutable GPU materials"
    );
    assert_ne!(
        duplicated.1, original_resources.1,
        "duplicates own placement buffers"
    );
}

#[test]
fn unified_gpu_asset_lines_keep_object_feedback_without_editable_topology() {
    use crate::document::{AssetFrames, AssetInstance, Document};
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let mut data = scene::test_support::triangle_data();
    data.materials[0].base_color = [1., 0., 0., 1.];
    data.materials[0].unlit = true;
    let primitive = &mut data.meshes[0].primitives[0];
    primitive.topology = scene::Topology::Lines;
    primitive.vertices[0].position = [-80., 0., 0.];
    primitive.vertices[1].position = [80., 0., 0.];
    primitive.indices = Arc::from([0, 1]);
    let asset = Arc::new(SceneAsset::new(data).unwrap());
    let mut document = Document::default();
    let reference = AssetInstance {
        source: "line.glb".into(),
        scene: 0,
    };
    let id = document
        .insert_asset(reference.clone(), "Line".into())
        .unwrap();
    let source = placed(id, asset, 0.);
    let frames = AssetFrames::from([(reference, source.frame.clone())]);
    let mesh = document
        .render_mesh_with_assets(&display(), &frames)
        .unwrap();
    assert!(
        mesh.edit_topology.is_empty(),
        "render ranges do not invent editable vertices"
    );
    renderer
        .set_assets(&capture.device, &capture.queue, &[source], &display())
        .unwrap();
    renderer.set_mesh(&capture.device, &mesh).unwrap();
    let ordinary = pixels(&renderer, &capture);
    assert!(
        ordinary
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 100 && p[1] < 10)
    );
    renderer.set_highlights(ObjectHighlights {
        selected: BTreeSet::new(),
        hovered: Some(id),
    });
    let hovered = pixels(&renderer, &capture);
    assert_ne!(
        hovered, ordinary,
        "noneditable line geometry has shared hover feedback"
    );
    renderer.set_highlights(ObjectHighlights {
        selected: BTreeSet::from([id]),
        hovered: Some(id),
    });
    let selected = pixels(&renderer, &capture);
    assert_ne!(
        selected, hovered,
        "selected lines use the stronger shared state"
    );
    renderer.set_highlights(ObjectHighlights::default());
    let wire = pixels_mode(&renderer, &capture, ShadingMode::Wireframe, false);
    assert!(
        wire.as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[0] > 50 && p[0].abs_diff(p[1]) < 20 && p[1].abs_diff(p[2]) < 20)
    );
    renderer.set_visible_objects(Some(&BTreeSet::new()));
    assert!(
        pixels(&renderer, &capture)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
}

#[test]
fn unified_gpu_reflected_placement_and_exposure_preserve_shared_world() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let mut source = placed(2, asset([0.2, 0.2, 0.2, 1.], false), 20.);
    prepare(&mut renderer, &capture, true, std::slice::from_ref(&source));
    let ordinary = center(&pixels(&renderer, &capture));
    source.transform.scale[0] = -1.0;
    prepare(&mut renderer, &capture, true, std::slice::from_ref(&source));
    assert_eq!(
        center(&pixels(&renderer, &capture)),
        ordinary,
        "placement reflection preserves front-face winding"
    );
    let unchanged = renderer.assets.cache_identity(source.object);
    source.exposure = 1.0;
    prepare(&mut renderer, &capture, true, std::slice::from_ref(&source));
    let brighter = center(&pixels(&renderer, &capture));
    assert!(
        brighter[0] > ordinary[0] + 20,
        "exposure changes the imported material: {ordinary:?} -> {brighter:?}"
    );
    assert_eq!(renderer.assets.cache_identity(source.object), unchanged);
    renderer.set_visible_objects(Some(&BTreeSet::from([1])));
    let native = pixels(&renderer, &capture);
    source.exposure = -1.0;
    prepare(&mut renderer, &capture, true, &[source]);
    assert_eq!(
        pixels(&renderer, &capture),
        native,
        "per-asset exposure does not change native appearance"
    );
}

#[test]
fn unified_gpu_imported_points_remain_visible_in_solid_wire_and_xray() {
    use crate::document::{AssetFrames, AssetInstance, Document};
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let mut data = scene::test_support::triangle_data();
    data.materials[0].base_color = [1., 0., 0., 1.];
    data.materials[0].unlit = true;
    let primitive = &mut data.meshes[0].primitives[0];
    primitive.topology = scene::Topology::Points;
    primitive.vertices[0].position = [0., 0., 0.];
    primitive.indices = Arc::from([0]);
    let asset = Arc::new(SceneAsset::new(data).unwrap());
    let mut document = Document::default();
    let reference = AssetInstance {
        source: "point.glb".into(),
        scene: 0,
    };
    let id = document
        .insert_asset(reference.clone(), "Point".into())
        .unwrap();
    let source = placed(id, asset, 0.);
    let frames = AssetFrames::from([(reference, source.frame.clone())]);
    let mesh = document
        .render_mesh_with_assets(&display(), &frames)
        .unwrap();
    assert!(mesh.edges.is_empty() && mesh.vertices.is_empty());
    renderer
        .set_assets(&capture.device, &capture.queue, &[source], &display())
        .unwrap();
    renderer.set_mesh(&capture.device, &mesh).unwrap();
    for (shading, xray) in [
        (ShadingMode::Solid, false),
        (ShadingMode::MaterialPreview, false),
        (ShadingMode::Wireframe, false),
        (ShadingMode::Solid, true),
        (ShadingMode::MaterialPreview, true),
    ] {
        let image = pixels_mode(&renderer, &capture, shading, xray);
        assert!(
            image.as_chunks::<4>().0.iter().any(|p| p[0] > 20),
            "a point remains visible with {shading:?}, xray={xray}"
        );
    }
    renderer.set_visible_objects(Some(&BTreeSet::new()));
    assert!(
        pixels(&renderer, &capture)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255])
    );
}

#[test]
fn unified_gpu_source_up_rotation_preserves_native_imported_depth_alignment() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let image = |renderer: &SceneRenderer| {
        pixels_from(
            renderer,
            &capture,
            ShadingMode::MaterialPreview,
            false,
            View::Top,
            true,
        )
    };
    prepare(&mut renderer, &capture, true, &[]);
    let native = center(&image(&renderer));
    let blue = asset([0., 0., 1., 1.], false);
    prepare(
        &mut renderer,
        &capture,
        true,
        &[placed(2, blue.clone(), -20.)],
    );
    assert_eq!(
        center(&image(&renderer)),
        native,
        "the same source-up rotation applies to native and PBR positions"
    );
    prepare(&mut renderer, &capture, true, &[placed(2, blue, 20.)]);
    assert_eq!(center(&image(&renderer)), [0, 0, 255, 255]);
}

#[test]
fn unified_gpu_solid_ignores_material_light_and_exposure_without_changing_caches() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let mut source = placed(2, asset([0.2, 0.02, 0.02, 1.], false), 0.);
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    let solid = pixels_mode(&renderer, &capture, ShadingMode::Solid, false);
    let preview = pixels(&renderer, &capture);
    assert_ne!(solid, preview, "Solid has a fixed editor material");
    let cached = renderer.assets.cache_identity(2);
    source.exposure = 2.0;
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    assert_eq!(
        pixels_mode(&renderer, &capture, ShadingMode::Solid, false),
        solid
    );
    assert_ne!(
        pixels(&renderer, &capture),
        preview,
        "preview still honors exposure"
    );
    assert_eq!(
        renderer.assets.cache_identity(2),
        cached,
        "shading and exposure never upload pose or material resources"
    );

    let mut data = (**source.asset).clone();
    data.images.push(scene::Image {
        name: "Appearance probe".into(),
        width: 1,
        height: 1,
        rgba8: vec![255, 0, 128, 0],
    });
    data.textures.push(scene::Texture {
        image: 0,
        sampler: scene::Sampler {
            wrap_s: scene::Wrap::Clamp,
            wrap_t: scene::Wrap::Clamp,
            mag: scene::Filter::Nearest,
            min: scene::Filter::Nearest,
            mipmap: None,
        },
    });
    let texture = Some(scene::TextureInfo {
        texture: 0,
        tex_coord: 0,
        transform: Default::default(),
    });
    let material = &mut data.materials[0];
    material.unlit = false;
    material.base_color = [0.3, 1.0, 0.4, 0.0];
    material.alpha_mode = AlphaMode::Mask;
    material.emissive = [10., 0., 4.];
    material.metallic = 1.0;
    material.roughness = 0.05;
    material.base_color_texture = texture;
    material.normal_texture = texture;
    material.metallic_roughness_texture = texture;
    material.emissive_texture = texture;
    material.occlusion_texture = texture;
    for vertex in &mut data.meshes[0].primitives[0].vertices {
        vertex.color = [0.1, 0.9, 0.2, 0.0];
    }
    data.nodes[0].light = Some(0);
    data.lights.push(scene::Light {
        name: "Red light".into(),
        kind: scene::LightKind::Directional,
        color: [1., 0., 0.],
        intensity: 100.,
        range_cm: None,
    });
    source = placed(2, Arc::new(SceneAsset::new(data).unwrap()), 0.);
    prepare(&mut renderer, &capture, false, &[source]);
    assert_eq!(
        pixels_mode(&renderer, &capture, ShadingMode::Solid, false),
        solid,
        "all source appearance, including masked opacity, is irrelevant in Solid"
    );
    assert!(
        pixels(&renderer, &capture)
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [0, 0, 0, 255]),
        "Material Preview retains masked opacity"
    );
}

#[test]
fn unified_gpu_solid_preserves_evaluated_normals_pose_and_double_sided_inspection() {
    let capture = pollster::block_on(Capture::new(SIZE, SIZE)).unwrap();
    let mut renderer = SceneRenderer::new(&capture.device, SIZE, SIZE);
    let mut source = placed(2, asset([0.2, 0.2, 0.2, 1.], false), 0.);
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    let flat = pixels_mode(&renderer, &capture, ShadingMode::Solid, false);
    let cached = renderer.assets.cache_identity(2);
    let mut evaluated = (*source.frame).clone();
    for draw in &mut evaluated.draws {
        for vertex in &mut draw.vertices {
            vertex.normal = glam::Vec3::new(0.8, 0.3, 1.).normalize().to_array();
        }
    }
    source.frame = Arc::new(evaluated);
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    let mut flat_proxy = proxy_mesh(false, std::slice::from_ref(&source));
    for vertex in &mut flat_proxy.vertices {
        vertex.normal = [0., 0., 1.];
    }
    renderer.set_mesh(&capture.device, &flat_proxy).unwrap();
    let smooth = pixels_mode(&renderer, &capture, ShadingMode::Solid, false);
    assert_ne!(
        smooth, flat,
        "Solid uses evaluated smooth normals, never flat picking-proxy normals"
    );
    let mut evaluated = (*source.frame).clone();
    for draw in &mut evaluated.draws {
        for vertex in &mut draw.vertices {
            vertex.position[1] += 40.;
        }
    }
    source.frame = Arc::new(evaluated);
    prepare(
        &mut renderer,
        &capture,
        false,
        std::slice::from_ref(&source),
    );
    let moved = pixels_mode(&renderer, &capture, ShadingMode::Solid, false);
    assert_ne!(moved, smooth, "Solid displays the current evaluated pose");
    let coverage = |image: Vec<u8>| {
        image
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[..3] != [0, 0, 0])
            .collect::<Vec<_>>()
    };
    assert_eq!(
        coverage(moved),
        coverage(pixels(&renderer, &capture)),
        "both shading modes display the same evaluated silhouette"
    );
    let now = renderer.assets.cache_identity(2);
    assert_eq!(
        (now.0, now.1),
        (cached.0, cached.1),
        "pose updates reuse material bindings and vertex allocations"
    );
    let back = pixels_from(
        &renderer,
        &capture,
        ShadingMode::Solid,
        false,
        View::Back,
        false,
    );
    assert!(
        back.as_chunks::<4>().0.iter().any(|p| p[0] > 20),
        "Solid shows backfaces even for a single-sided source material"
    );
    assert!(
        pixels_from(
            &renderer,
            &capture,
            ShadingMode::MaterialPreview,
            false,
            View::Back,
            false
        )
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| *p == [0, 0, 0, 255]),
        "Material Preview preserves source backface culling"
    );
}
