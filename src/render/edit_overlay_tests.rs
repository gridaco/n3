use super::*;
use crate::{
    doc_capture::Capture,
    edit_feedback::EditSelection,
    mesh::{EditFace, EditObjectTopology, EditVertex, ObjectRange},
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

fn quad_mesh(occluder: bool) -> MeshData {
    let mut mesh = MeshData {
        vertices: Vec::new(),
        edges: Vec::new(),
        object_ranges: Vec::new(),
        edit_topology: Vec::new(),
        vertex_count: 0,
        face_count: 0,
        triangle_count: 0,
        object_count: 0,
        source_extent: [2.0, 2.0, 0.4],
        warnings: Vec::new(),
    };
    let mut objects = vec![(17, 0.8, 0.0)];
    if occluder {
        objects.push((99, 1.0, 0.4));
    }
    for (object, size, depth) in objects {
        let points = [
            [-size, -size, depth],
            [size, -size, depth],
            [size, size, depth],
            [-size, size, depth],
        ];
        let vertex = |index: usize| Vertex {
            position: points[index],
            normal: [0.0, 0.0, 1.0],
        };
        let triangle_start = mesh.vertices.len() as u32;
        mesh.vertices.extend([0, 1, 2, 0, 2, 3].map(vertex));
        let triangles = triangle_start..mesh.vertices.len() as u32;
        let edge_start = mesh.edges.len() as u32;
        mesh.edges.extend([0, 1, 1, 2, 2, 3, 3, 0].map(vertex));
        mesh.object_ranges.push(ObjectRange {
            object,
            triangles: triangles.clone(),
            edges: edge_start..edge_start + 8,
            loose_edges: edge_start..edge_start,
        });
        mesh.edit_topology.push(EditObjectTopology {
            object,
            edges: edge_start..mesh.edges.len() as u32,
            loose_edges: mesh.edges.len() as u32..mesh.edges.len() as u32,
            vertices: points
                .into_iter()
                .enumerate()
                .map(|(index, position)| EditVertex {
                    id: index as u64 + 1,
                    position,
                })
                .collect(),
            edge_vertices: vec![[1, 2], [2, 3], [3, 4], [4, 1]],
            faces: vec![EditFace {
                vertices: vec![1, 2, 3, 4],
                triangles,
            }],
        });
        mesh.vertex_count += 4;
        mesh.face_count += 1;
        mesh.triangle_count += 2;
        mesh.object_count += 1;
    }
    mesh
}

fn selection(ids: &[u64]) -> EditSelection {
    EditSelection {
        object: Some(17),
        vertices: ids.iter().copied().collect(),
    }
}

fn frame(
    capture: &mut Capture,
    ctx: &egui::Context,
    camera: &Camera,
    selection: EditSelection,
    edges: bool,
    z_up: bool,
) -> Vec<u8> {
    capture.scene.set_edit_selection(&capture.device, selection);
    let viewport =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(viewport),
            ..Default::default()
        },
        |ui| {
            ui.ctx().layer_painter(egui::LayerId::background()).image(
                capture.texture,
                viewport,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        },
    );
    capture
        .render(
            ctx,
            output,
            viewport,
            camera,
            crate::render::shading::ShadingMode::Solid,
            false,
            edges,
            false,
            z_up,
            crate::theme::Palette::new(
                crate::settings::ResolvedTheme::Dark,
                crate::settings::AccentColor::DEFAULT,
            )
            .workbench_viewport,
        )
        .unwrap();
    capture.read_frame().unwrap().rgba
}

fn projected(camera: &Camera, point: [f32; 3], z_up: bool) -> (usize, usize) {
    let position =
        crate::orientation::display_rotation(z_up).transform_point3(glam::Vec3::from_array(point));
    let p = camera
        .view_projection(WIDTH as f32 / HEIGHT as f32)
        .project_point3(position);
    (
        ((p.x + 1.0) * WIDTH as f32 * 0.5).round() as usize,
        ((1.0 - p.y) * HEIGHT as f32 * 0.5).round() as usize,
    )
}

fn pixel(rgba: &[u8], x: usize, y: usize) -> &[u8] {
    &rgba[(y * WIDTH as usize + x) * 4..(y * WIDTH as usize + x) * 4 + 4]
}

fn amber_near(rgba: &[u8], x: usize, y: usize) -> i16 {
    (y.saturating_sub(2)..=(y + 2).min(HEIGHT as usize - 1))
        .flat_map(|py| {
            (x.saturating_sub(2)..=(x + 2).min(WIDTH as usize - 1)).map(move |px| {
                let p = pixel(rgba, px, py);
                i16::from(p[0]) - i16::from(p[2])
            })
        })
        .max()
        .unwrap()
}

#[test]
fn edit_overlay_tints_whole_polygons_and_interpolates_original_edges_without_reupload() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    let mut camera = Camera::default();
    camera.set_view(crate::camera::View::Front);
    capture
        .scene
        .set_mesh(&capture.device, &quad_mesh(false))
        .unwrap();
    let none = frame(&mut capture, &ctx, &camera, selection(&[]), false, false);
    let partial = frame(
        &mut capture,
        &ctx,
        &camera,
        selection(&[1, 2, 3]),
        false,
        false,
    );
    let (x, y) = projected(&camera, [0.0, 0.0, 0.0], false);
    for py in y - 8..=y + 8 {
        for px in x - 8..=x + 8 {
            assert_eq!(
                pixel(&none, px, py),
                pixel(&partial, px, py),
                "One selected derived triangle cannot tint part of its original quad"
            );
        }
    }
    let full = frame(
        &mut capture,
        &ctx,
        &camera,
        selection(&[1, 2, 3, 4]),
        false,
        false,
    );
    let a = pixel(&none, x, y);
    let b = pixel(&full, x, y);
    assert!(
        b[0] > a[0] && b[2] < a[2],
        "Full face has a warm translucent tint: {a:?} -> {b:?}"
    );
    assert_ne!(
        b,
        &[255, 169, 64, 255],
        "The face retains its lit surface, not opaque selection paint"
    );
    assert_eq!(
        full,
        frame(
            &mut capture,
            &ctx,
            &camera,
            selection(&[1, 2, 3, 4]),
            true,
            false
        ),
        "Ordinary edges must not draw again underneath edited-object edge chrome"
    );
    let gradient = frame(&mut capture, &ctx, &camera, selection(&[1]), false, false);
    let left = projected(&camera, [-0.5, -0.8, 0.0], false);
    let right = projected(&camera, [0.5, -0.8, 0.0], false);
    assert!(
        amber_near(&gradient, left.0, left.1) > amber_near(&gradient, right.0, right.1) + 25,
        "One selected endpoint fades toward its unselected neighbor"
    );
    let revision = capture.scene.edit.upload_revision;
    capture
        .scene
        .set_edit_selection(&capture.device, selection(&[1]));
    camera.orbit(10.0, 4.0);
    frame(&mut capture, &ctx, &camera, selection(&[1]), false, false);
    assert_eq!(
        capture.scene.edit.upload_revision, revision,
        "Selection identity and camera-only changes reuse cached GPU edges"
    );
}

#[test]
fn edit_overlay_is_depth_tested_source_up_aware_and_clears_on_mesh_replacement() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    let mut camera = Camera::default();
    for z_up in [false, true] {
        camera.set_view(if z_up {
            crate::camera::View::Top
        } else {
            crate::camera::View::Front
        });
        capture
            .scene
            .set_mesh(&capture.device, &quad_mesh(false))
            .unwrap();
        let base = frame(
            &mut capture,
            &ctx,
            &camera,
            EditSelection::default(),
            false,
            z_up,
        );
        let selected = frame(
            &mut capture,
            &ctx,
            &camera,
            selection(&[1, 2, 3, 4]),
            false,
            z_up,
        );
        assert_ne!(
            base, selected,
            "Selection follows the same source-up transform as the mesh"
        );
        capture
            .scene
            .set_mesh(&capture.device, &quad_mesh(true))
            .unwrap();
        let hidden_base = frame(
            &mut capture,
            &ctx,
            &camera,
            EditSelection::default(),
            false,
            z_up,
        );
        assert_eq!(
            hidden_base,
            frame(
                &mut capture,
                &ctx,
                &camera,
                selection(&[1, 2, 3, 4]),
                false,
                z_up
            ),
            "Foreground scene surfaces occlude both face tint and selected edges"
        );
        capture.scene.clear_mesh();
        let empty = frame(
            &mut capture,
            &ctx,
            &camera,
            selection(&[1, 2, 3, 4]),
            false,
            z_up,
        );
        assert_eq!(
            empty,
            frame(
                &mut capture,
                &ctx,
                &camera,
                EditSelection::default(),
                false,
                z_up
            )
        );
        capture
            .scene
            .set_mesh(&capture.device, &quad_mesh(false))
            .unwrap();
        assert_eq!(
            base,
            frame(
                &mut capture,
                &ctx,
                &camera,
                EditSelection::default(),
                false,
                z_up
            ),
            "Reload after leaving edit mode does not retain stale topology feedback"
        );
    }
}

fn line_mesh(start: [f32; 3], end: [f32; 3]) -> MeshData {
    let mut mesh = quad_mesh(false);
    mesh.vertices.clear();
    mesh.object_ranges.clear();
    mesh.edges = [start, end]
        .map(|position| Vertex {
            position,
            normal: [0.0, 0.0, 1.0],
        })
        .to_vec();
    mesh.edit_topology = vec![EditObjectTopology {
        object: 17,
        edges: 0..2,
        loose_edges: 0..2,
        vertices: vec![
            EditVertex {
                id: 1,
                position: start,
            },
            EditVertex {
                id: 2,
                position: end,
            },
        ],
        edge_vertices: vec![[1, 2]],
        faces: Vec::new(),
    }];
    mesh.vertex_count = 2;
    mesh.face_count = 0;
    mesh.triangle_count = 0;
    mesh
}

#[test]
fn edit_edge_width_scales_for_retina_and_near_plane_clipping_does_not_flip_ribbons() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    let mut camera = Camera::default();
    camera.set_view(crate::camera::View::Front);
    let blank = frame(
        &mut capture,
        &ctx,
        &camera,
        EditSelection::default(),
        false,
        false,
    );
    capture
        .scene
        .set_mesh(
            &capture.device,
            &line_mesh([-0.8, 0.0, 0.0], [0.8, 0.0, 0.0]),
        )
        .unwrap();
    let normal = frame(
        &mut capture,
        &ctx,
        &camera,
        selection(&[1, 2]),
        false,
        false,
    );
    capture.scene.set_pixel_scale(2.0);
    let retina = frame(
        &mut capture,
        &ctx,
        &camera,
        selection(&[1, 2]),
        false,
        false,
    );
    let count = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(blank.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a != b)
            .count()
    };
    assert!(count(&normal) > 0);
    assert!(
        count(&retina) > count(&normal),
        "Logical line width increases in physical pixels on Retina"
    );
    assert!(
        count(&retina) < count(&normal) * 3,
        "Pixel scale changes stroke width, not whole-screen coverage"
    );
    capture.scene.set_pixel_scale(1.0);
    camera.toggle_projection();
    let eye = camera.eye();
    capture
        .scene
        .set_mesh(
            &capture.device,
            &line_mesh([0.0, 0.0, eye.z + 0.1], [0.2, 0.0, eye.z + 0.2]),
        )
        .unwrap();
    assert_eq!(
        blank,
        frame(
            &mut capture,
            &ctx,
            &camera,
            selection(&[1, 2]),
            false,
            false
        ),
        "Edges entirely behind the perspective eye never render"
    );
    capture
        .scene
        .set_mesh(
            &capture.device,
            &line_mesh([-0.02, 0.0, eye.z - 0.1], [0.2, 0.0, eye.z + 0.1]),
        )
        .unwrap();
    let crossing = frame(
        &mut capture,
        &ctx,
        &camera,
        selection(&[1, 2]),
        false,
        false,
    );
    let changed: Vec<_> = crossing
        .as_chunks::<4>()
        .0
        .iter()
        .zip(blank.as_chunks::<4>().0.iter())
        .enumerate()
        .filter_map(|(index, (a, b))| (a != b).then_some(index))
        .collect();
    assert!(
        !changed.is_empty(),
        "The visible part of a near-plane crossing edge remains rendered"
    );
    assert!(
        changed
            .iter()
            .all(|index| (index / WIDTH as usize).abs_diff(HEIGHT as usize / 2) < 4),
        "Near-plane clipping must preserve a thin line, not a flipped screen-sized quad"
    );
}
