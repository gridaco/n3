//! Pixel assertions for shading policy, using the production GPU and compositor.

use super::*;
use crate::{
    camera::View,
    doc_capture::Capture,
    mesh::{EditFace, EditObjectTopology, EditVertex},
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 240;

fn quads(objects: &[u64]) -> MeshData {
    let mut mesh = MeshData {
        vertices: Vec::new(),
        edges: Vec::new(),
        object_ranges: Vec::new(),
        edit_topology: Vec::new(),
        vertex_count: 0,
        face_count: 0,
        triangle_count: 0,
        object_count: 0,
        source_extent: [1.6, 1.6, 0.4],
        warnings: Vec::new(),
    };
    for &object in objects {
        let (size, depth) = if object == 17 { (0.4, 0.0) } else { (0.8, 0.4) };
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
        let start = mesh.vertices.len() as u32;
        mesh.vertices.extend([0, 1, 2, 0, 2, 3].map(vertex));
        let triangles = start..mesh.vertices.len() as u32;
        let edges_start = mesh.edges.len() as u32;
        mesh.edges.extend([0, 1, 1, 2, 2, 3, 3, 0].map(vertex));
        mesh.object_ranges.push(ObjectRange {
            object,
            triangles: triangles.clone(),
            edges: edges_start..edges_start + 8,
            loose_edges: edges_start..edges_start,
        });
        mesh.edit_topology.push(EditObjectTopology {
            object,
            edges: edges_start..mesh.edges.len() as u32,
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

fn loose_loop(occluder: bool) -> MeshData {
    let mut mesh = quads(if occluder { &[17, 99] } else { &[17] });
    // Retain the authored loop while removing its face. The foreground quad,
    // when present, remains an ordinary solid occluder.
    mesh.vertices.drain(..6);
    for object in &mut mesh.object_ranges {
        if object.object == 17 {
            object.loose_edges = object.edges.clone();
        }
        object.triangles =
            object.triangles.start.saturating_sub(6)..object.triangles.end.saturating_sub(6);
    }
    for object in &mut mesh.edit_topology {
        if object.object == 17 {
            object.faces.clear();
            object.loose_edges = object.edges.clone();
        } else {
            for face in &mut object.faces {
                face.triangles = face.triangles.start - 6..face.triangles.end - 6;
            }
        }
    }
    mesh.face_count -= 1;
    mesh.triangle_count -= 2;
    mesh
}

fn frame(
    capture: &mut Capture,
    ctx: &egui::Context,
    shading: ShadingMode,
    show_edges: bool,
    background: egui::Color32,
) -> Vec<u8> {
    frame_with_xray(capture, ctx, shading, false, show_edges, background)
}

fn frame_with_xray(
    capture: &mut Capture,
    ctx: &egui::Context,
    shading: ShadingMode,
    xray: bool,
    show_edges: bool,
    background: egui::Color32,
) -> Vec<u8> {
    let viewport =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
    let mut camera = Camera::default();
    camera.set_view(View::Front);
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
            ctx, output, viewport, &camera, shading, xray, show_edges, false, false, background,
        )
        .unwrap();
    capture.read_frame().unwrap().rgba
}

fn center(pixels: &[u8]) -> &[u8] {
    let index = ((HEIGHT / 2 * WIDTH + WIDTH / 2) * 4) as usize;
    &pixels[index..index + 4]
}

fn strongest_color_change(before: &[u8], after: &[u8]) -> u16 {
    before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0)
        .map(|(a, b)| (0..3).map(|i| u16::from(a[i].abs_diff(b[i]))).sum())
        .max()
        .unwrap_or(0)
}

#[test]
fn unfilled_objects_remain_visible_and_highlighted_without_solid_edge_overlay() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    let background = egui::Color32::from_gray(240);
    let empty = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
    capture
        .scene
        .set_mesh(&capture.device, &loose_loop(false))
        .unwrap();
    assert!(capture.scene.triangle_buffer.is_none());
    let edge_buffer = capture.scene.edge_buffer.as_ref().unwrap() as *const _;
    let revision = capture.scene.edit.upload_revision;
    let normal = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
    assert_ne!(
        normal, empty,
        "Standalone edges are visible with face edges hidden"
    );
    assert_eq!(
        center(&normal),
        background.to_array(),
        "The loop stays unfilled"
    );
    assert_eq!(
        normal,
        frame(&mut capture, &ctx, ShadingMode::Solid, true, background),
        "The Solid edge preference only controls face boundaries"
    );

    for shading in [ShadingMode::Solid, ShadingMode::Wireframe] {
        capture.scene.set_highlights(ObjectHighlights::default());
        let normal = frame(&mut capture, &ctx, shading, false, background);
        capture.scene.set_highlights(ObjectHighlights {
            hovered: Some(17),
            ..Default::default()
        });
        let hovered = frame(&mut capture, &ctx, shading, false, background);
        assert_ne!(normal, hovered, "An unfilled object has hover feedback");
        capture.scene.set_highlights(ObjectHighlights {
            selected: BTreeSet::from([17]),
            hovered: Some(17),
        });
        let selected = frame(&mut capture, &ctx, shading, false, background);
        assert_ne!(hovered, selected, "Selection uses its own color and width");
        capture.scene.set_highlights(ObjectHighlights {
            selected: BTreeSet::from([17]),
            hovered: None,
        });
        assert_eq!(
            selected,
            frame(&mut capture, &ctx, shading, false, background)
        );
    }
    assert!(std::ptr::eq(
        edge_buffer,
        capture.scene.edge_buffer.as_ref().unwrap()
    ));
    assert_eq!(revision, capture.scene.edit.upload_revision);

    capture.scene.set_highlights(ObjectHighlights::default());
    capture.scene.set_edit_selection(
        &capture.device,
        EditSelection {
            object: Some(17),
            vertices: BTreeSet::from([1, 2]),
        },
    );
    let partial = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
    capture.scene.set_edit_selection(
        &capture.device,
        EditSelection {
            object: Some(17),
            vertices: BTreeSet::from([1, 2, 3, 4]),
        },
    );
    assert_ne!(
        partial,
        frame(&mut capture, &ctx, ShadingMode::Solid, false, background),
        "Loose edges retain endpoint selection gradients with face edges hidden"
    );
}

#[test]
fn loose_edge_feedback_respects_occlusion_and_local_view() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    let background = egui::Color32::from_gray(240);
    capture
        .scene
        .set_mesh(&capture.device, &quads(&[99]))
        .unwrap();
    let front = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
    capture
        .scene
        .set_mesh(&capture.device, &loose_loop(true))
        .unwrap();
    assert_eq!(
        front,
        frame(&mut capture, &ctx, ShadingMode::Solid, false, background)
    );
    for shading in [ShadingMode::Solid, ShadingMode::Wireframe] {
        capture.scene.set_highlights(ObjectHighlights::default());
        let neutral = frame(&mut capture, &ctx, shading, false, background);
        capture.scene.set_highlights(ObjectHighlights {
            selected: BTreeSet::from([17]),
            hovered: None,
        });
        assert_eq!(
            neutral,
            frame(&mut capture, &ctx, shading, false, background),
            "Hidden loop selection must not show through the foreground surface"
        );
    }
    capture
        .scene
        .set_visible_objects(Some(&BTreeSet::from([17])));
    let isolated = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
    assert_ne!(front, isolated);
    capture
        .scene
        .set_mesh(&capture.device, &loose_loop(false))
        .unwrap();
    assert_eq!(
        isolated,
        frame(&mut capture, &ctx, ShadingMode::Solid, false, background)
    );
    capture.scene.set_visible_objects(Some(&BTreeSet::new()));
    let hidden = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
    capture.scene.clear_mesh();
    assert_eq!(
        hidden,
        frame(&mut capture, &ctx, ShadingMode::Solid, false, background),
        "Local view also hides the loop and its object feedback"
    );
}

#[test]
fn wireframe_has_no_fill_or_triangle_diagonals_and_keeps_rear_authored_edges() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    for background in [egui::Color32::from_gray(240), egui::Color32::from_gray(25)] {
        capture
            .scene
            .set_mesh(&capture.device, &quads(&[17, 99]))
            .unwrap();
        let triangles = capture.scene.triangle_buffer.as_ref().unwrap() as *const _;
        let edges = capture.scene.edge_buffer.as_ref().unwrap() as *const _;
        let solid = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
        let wire = frame(
            &mut capture,
            &ctx,
            ShadingMode::Wireframe,
            false,
            background,
        );
        assert_eq!(
            center(&wire),
            background.to_array(),
            "Neither face fill nor a triangulation diagonal may cover the polygon center"
        );
        assert_ne!(center(&solid), center(&wire));
        assert_eq!(
            wire,
            frame(&mut capture, &ctx, ShadingMode::Wireframe, true, background),
            "The solid edge-overlay preference cannot hide the wireframe"
        );
        assert!(std::ptr::eq(
            triangles,
            capture.scene.triangle_buffer.as_ref().unwrap()
        ));
        assert!(std::ptr::eq(
            edges,
            capture.scene.edge_buffer.as_ref().unwrap()
        ));

        capture
            .scene
            .set_mesh(&capture.device, &quads(&[99]))
            .unwrap();
        assert_eq!(
            solid,
            frame(&mut capture, &ctx, ShadingMode::Solid, false, background)
        );
        let front_only = frame(
            &mut capture,
            &ctx,
            ShadingMode::Wireframe,
            false,
            background,
        );
        assert_ne!(
            wire, front_only,
            "Rear polygon edges remain visible through the front surface"
        );
        assert!(
            wire.as_chunks::<4>().0.iter().any(|pixel| {
                pixel[..3]
                    .iter()
                    .zip(background.to_array())
                    .any(|(a, b)| a.abs_diff(b) > 30)
            }),
            "Wire lines need readable contrast on both light and dark backgrounds"
        );
    }
}

#[test]
fn wireframe_isolation_and_edit_feedback_keep_visible_only_semantics() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    let background = egui::Color32::from_gray(240);
    capture
        .scene
        .set_mesh(&capture.device, &quads(&[17, 99]))
        .unwrap();
    let normal = frame(
        &mut capture,
        &ctx,
        ShadingMode::Wireframe,
        false,
        background,
    );
    capture.scene.set_edit_selection(
        &capture.device,
        EditSelection {
            object: Some(17),
            vertices: BTreeSet::from([1, 2, 3, 4]),
        },
    );
    assert_eq!(
        normal,
        frame(
            &mut capture,
            &ctx,
            ShadingMode::Wireframe,
            false,
            background
        ),
        "Rear edges are visible neutrals, but hidden edit feedback is not promoted to x-ray"
    );
    capture
        .scene
        .set_visible_objects(Some(&BTreeSet::from([17])));
    let isolated = frame(
        &mut capture,
        &ctx,
        ShadingMode::Wireframe,
        false,
        background,
    );
    assert_eq!(
        center(&isolated),
        background.to_array(),
        "Selected faces still have no fill"
    );
    capture
        .scene
        .set_mesh(&capture.device, &quads(&[17]))
        .unwrap();
    assert_eq!(
        isolated,
        frame(
            &mut capture,
            &ctx,
            ShadingMode::Wireframe,
            false,
            background
        )
    );
    capture
        .scene
        .set_edit_selection(&capture.device, EditSelection::default());
    assert_ne!(
        isolated,
        frame(
            &mut capture,
            &ctx,
            ShadingMode::Wireframe,
            false,
            background
        ),
        "Visible selected edges still show their selection colors"
    );
    capture.scene.set_visible_objects(Some(&BTreeSet::new()));
    let empty = frame(
        &mut capture,
        &ctx,
        ShadingMode::Wireframe,
        false,
        background,
    );
    capture.scene.clear_mesh();
    assert_eq!(
        empty,
        frame(
            &mut capture,
            &ctx,
            ShadingMode::Wireframe,
            false,
            background
        )
    );
}

#[test]
fn xray_preserves_a_faint_nearest_surface_independent_of_triangle_order() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    for background in [egui::Color32::from_gray(240), egui::Color32::from_gray(25)] {
        capture
            .scene
            .set_mesh(&capture.device, &quads(&[17, 99]))
            .unwrap();
        let triangles = capture.scene.triangle_buffer.as_ref().unwrap() as *const _;
        let edges = capture.scene.edge_buffer.as_ref().unwrap() as *const _;
        let revision = capture.scene.edit.upload_revision;
        let solid = frame(&mut capture, &ctx, ShadingMode::Solid, false, background);
        let xray = frame_with_xray(
            &mut capture,
            &ctx,
            ShadingMode::Solid,
            true,
            false,
            background,
        );
        for ((faint, opaque), backdrop) in center(&xray)[..3]
            .iter()
            .zip(&center(&solid)[..3])
            .zip(background.to_array())
        {
            assert!(
                faint.abs_diff(backdrop) < opaque.abs_diff(backdrop),
                "X-ray keeps a faint, theme-readable shape cue rather than an opaque face"
            );
        }
        assert_ne!(center(&xray), background.to_array());
        assert_eq!(
            xray,
            frame_with_xray(
                &mut capture,
                &ctx,
                ShadingMode::Solid,
                true,
                true,
                background
            ),
            "X-ray owns its topology display without changing the Solid edge preference"
        );
        assert_eq!(
            solid,
            frame(&mut capture, &ctx, ShadingMode::Solid, false, background)
        );
        assert!(std::ptr::eq(
            triangles,
            capture.scene.triangle_buffer.as_ref().unwrap()
        ));
        assert!(std::ptr::eq(
            edges,
            capture.scene.edge_buffer.as_ref().unwrap()
        ));
        assert_eq!(revision, capture.scene.edit.upload_revision);

        capture
            .scene
            .set_mesh(&capture.device, &quads(&[99, 17]))
            .unwrap();
        assert_eq!(
            xray,
            frame_with_xray(
                &mut capture,
                &ctx,
                ShadingMode::Solid,
                true,
                false,
                background
            ),
            "A depth prepass prevents hidden surfaces accumulating opacity by draw order"
        );
        capture
            .scene
            .set_mesh(&capture.device, &quads(&[99]))
            .unwrap();
        let front = frame_with_xray(
            &mut capture,
            &ctx,
            ShadingMode::Solid,
            true,
            false,
            background,
        );
        assert_eq!(center(&xray), center(&front));
        assert_ne!(
            xray, front,
            "Rear authored edges remain visible without rear face fill"
        );
    }
}

#[test]
fn xray_reveals_hidden_edit_and_object_feedback_but_respects_local_view() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let ctx = egui::Context::default();
    for background in [egui::Color32::from_gray(240), egui::Color32::from_gray(25)] {
        for shading in [ShadingMode::Solid, ShadingMode::Wireframe] {
            capture.scene.set_visible_objects(None);
            capture
                .scene
                .set_mesh(&capture.device, &quads(&[17, 99]))
                .unwrap();
            capture
                .scene
                .set_edit_selection(&capture.device, EditSelection::default());
            capture.scene.set_highlights(ObjectHighlights::default());
            let baseline = frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            if shading == ShadingMode::Wireframe {
                assert_eq!(center(&baseline), background.to_array());
            }
            capture.scene.set_highlights(ObjectHighlights {
                selected: BTreeSet::from([17]),
                hovered: None,
            });
            let selected = frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            assert_ne!(
                baseline, selected,
                "An entirely occluded selected object has feedback"
            );
            let revision = capture.scene.edit.upload_revision;
            let without_xray = frame(&mut capture, &ctx, shading, false, background);
            capture.scene.set_highlights(ObjectHighlights::default());
            assert_eq!(
                without_xray,
                frame(&mut capture, &ctx, shading, false, background)
            );
            assert_eq!(revision, capture.scene.edit.upload_revision);

            let select = |vertices| EditSelection {
                object: Some(17),
                vertices,
            };
            capture
                .scene
                .set_edit_selection(&capture.device, select(BTreeSet::new()));
            let edit_none = frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            capture
                .scene
                .set_edit_selection(&capture.device, select(BTreeSet::from([1])));
            let edit_one = frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            assert_ne!(
                edit_none, edit_one,
                "Hidden edges retain selected-endpoint gradients"
            );
            let hidden_change = strongest_color_change(&edit_none, &edit_one);
            assert!(
                hidden_change > 30,
                "Rear selection must remain readable in both themes"
            );
            capture
                .scene
                .set_visible_objects(Some(&BTreeSet::from([17])));
            let visible_one = frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            capture
                .scene
                .set_edit_selection(&capture.device, select(BTreeSet::new()));
            let visible_none =
                frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            assert!(
                hidden_change < strongest_color_change(&visible_none, &visible_one),
                "Occluded edit feedback must remain quieter than front feedback"
            );
            capture.scene.set_visible_objects(None);
            capture
                .scene
                .set_edit_selection(&capture.device, select(BTreeSet::from([1, 2, 3, 4])));
            let edit_all = frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            assert_ne!(edit_one, edit_all);
            if shading == ShadingMode::Solid {
                assert_ne!(
                    center(&edit_one),
                    center(&edit_all),
                    "Only the fully selected hidden polygon is tinted"
                );
            } else {
                assert_eq!(
                    center(&edit_all),
                    background.to_array(),
                    "Wireframe X-ray stays unfilled"
                );
            }
            capture.scene.set_highlights(ObjectHighlights {
                selected: BTreeSet::from([17]),
                hovered: None,
            });
            capture
                .scene
                .set_visible_objects(Some(&BTreeSet::from([99])));
            let isolated = frame_with_xray(&mut capture, &ctx, shading, true, false, background);
            capture
                .scene
                .set_mesh(&capture.device, &quads(&[99]))
                .unwrap();
            assert_eq!(
                isolated,
                frame_with_xray(&mut capture, &ctx, shading, true, false, background),
                "X-ray never reveals objects excluded by Local View, including their feedback"
            );
        }
    }
}
