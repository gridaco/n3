//! X-ray changes projected target eligibility, never the document or transaction.

use super::*;
use crate::document::{EditableMesh, MeshVertex, Object, Transform};
use egui::{Event, Modifiers};

fn viewport() -> Rect {
    Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))
}

fn front() -> Camera {
    let mut camera = Camera::default();
    camera.look_from(Vec3::Z);
    camera
}

fn cube() -> Editor {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let mut editor = Editor::new(document).unwrap();
    editor.select_object(id).unwrap();
    editor
}

fn pointer(point: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: point,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn frame(ctx: &egui::Context, editor: &mut Editor, camera: &Camera, events: Vec<Event>) {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(viewport()),
            events,
            focused: true,
            ..Default::default()
        },
        |root_ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(root_ui, |ui| {
                    let response = ui.allocate_rect(viewport(), egui::Sense::click_and_drag());
                    assert!(editor.ui(ui, &response, camera, false).is_none());
                });
        },
    )
    .textures_delta
    .clear();
}

fn drag(ctx: &egui::Context, editor: &mut Editor, camera: &Camera, rect: Rect) {
    frame(ctx, editor, camera, vec![]);
    frame(
        ctx,
        editor,
        camera,
        vec![Event::PointerMoved(rect.min), pointer(rect.min, true)],
    );
    frame(ctx, editor, camera, vec![Event::PointerMoved(rect.max)]);
    assert!(!editor.set_xray(!editor.xray_enabled()));
    frame(ctx, editor, camera, vec![pointer(rect.max, false)]);
}

#[test]
fn xray_vertex_all_and_cycle_share_depth_policy_without_materializing_or_history() {
    let mut editor = cube();
    editor.enter_edit().unwrap();
    let camera = front();
    let original = editor.snapshot();
    let revision = editor.revision;
    assert!(!editor.xray_enabled());
    assert_eq!(
        editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap()
            .len(),
        4
    );
    let projected = editor.cache.as_ref().unwrap().projected.as_ptr();
    assert!(editor.set_xray(true));
    assert_eq!(
        editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap()
            .len(),
        8
    );
    assert_eq!(editor.cache.as_ref().unwrap().projected.as_ptr(), projected);
    editor.select_all(viewport(), &camera, false).unwrap();
    assert_eq!(editor.selected_vertices.len(), 8);
    let order: Vec<_> = editor
        .document
        .eval_object(editor.selected_object.unwrap())
        .unwrap()
        .vertices
        .into_iter()
        .map(|vertex| vertex.id)
        .collect();
    editor.selected_vertices.clear();
    for id in order.iter().chain(order.first()) {
        editor
            .cycle_selection(false, viewport(), &camera, false)
            .unwrap();
        assert_eq!(editor.selected_vertices, BTreeSet::from([*id]));
    }
    editor.select_all(viewport(), &camera, false).unwrap();
    assert!(editor.set_xray(false));
    assert_eq!(
        editor.selected_vertices.len(),
        8,
        "Turning X-ray off retains rear selections"
    );
    assert_eq!(editor.selection_points(false).unwrap().len(), 8);
    assert_eq!(
        editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap()
            .len(),
        4
    );
    assert_eq!(editor.document, original.document);
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), 0);
    editor.leave_edit();
    assert_eq!(
        editor.document, original.document,
        "Inspecting a primitive retains its recipe"
    );
}

#[test]
fn xray_click_resolves_overlapping_vertices_by_depth_before_id() {
    let mut editor = cube();
    editor.enter_edit().unwrap();
    editor.set_xray(true);
    let camera = front();
    let projection = Projection::new(viewport(), &camera, false).unwrap();
    editor.prepare(&projection).unwrap();
    let front: Vec<_> = editor
        .cache
        .as_ref()
        .unwrap()
        .projected
        .iter()
        .filter(|vertex| !vertex.occluded)
        .map(|vertex| (vertex.id, vertex.screen))
        .collect();
    assert_eq!(front.len(), 4);
    for (id, point) in front {
        editor.select_vertex(point, false);
        assert_eq!(editor.selected_vertices, BTreeSet::from([id]));
    }
    assert!(
        editor
            .cache
            .as_ref()
            .unwrap()
            .projected
            .iter()
            .any(|vertex| {
                vertex.occluded && vertex.id < *editor.selected_vertices.first().unwrap()
            }),
        "Rear vertices with lower IDs exercise depth rather than source-ID priority"
    );
}

#[test]
fn xray_marquee_selects_rear_vertices_only_on_release_and_retains_shift_contract() {
    for enabled in [false, true] {
        let mut editor = cube();
        editor.enter_edit().unwrap();
        editor.set_xray(enabled);
        let camera = front();
        let vertices = editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap();
        let rect = vertices
            .iter()
            .fold(Rect::NOTHING, |mut rect, (_, point)| {
                rect.extend_with(*point);
                rect
            })
            .expand(12.0);
        let ctx = egui::Context::default();
        frame(&ctx, &mut editor, &camera, vec![]);
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![Event::PointerMoved(rect.min), pointer(rect.min, true)],
        );
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![Event::PointerMoved(rect.max)],
        );
        assert!(
            editor.selected_vertices.is_empty(),
            "Selection is committed on release"
        );
        assert!(
            !editor.set_xray(!enabled),
            "A held marquee freezes its depth policy"
        );
        frame(&ctx, &mut editor, &camera, vec![pointer(rect.max, false)]);
        let expected: BTreeSet<_> = vertices.iter().map(|(id, _)| *id).collect();
        assert_eq!(editor.selected_vertices, expected);
        assert_eq!(expected.len(), if enabled { 8 } else { 4 });
        let (id, point) = vertices
            .iter()
            .find(|(id, _)| {
                editor
                    .cache
                    .as_ref()
                    .unwrap()
                    .projected
                    .iter()
                    .any(|v| v.id == *id && !v.occluded)
            })
            .copied()
            .unwrap();
        editor.select_vertex(point, true);
        assert!(!editor.selected_vertices.contains(&id));
        editor.select_vertex(point, true);
        assert_eq!(editor.selected_vertices, expected);
    }
}

#[test]
fn xray_object_boxes_reach_occluded_objects_but_clicks_and_isolation_keep_their_scope() {
    let mut document = Document::default();
    let rear = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[0].transform.translation[2] = -2.0;
    let foreground = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[1].transform.translation[2] = 2.0;
    document.objects[1].transform.scale = [2.0, 2.0, 1.0];
    let camera = front();
    let mut editor = Editor::new(document).unwrap();
    let ctx = egui::Context::default();
    let bounds = editor
        .object_selection_bounds(viewport(), &camera, false)
        .unwrap();
    assert_eq!(
        bounds.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![foreground]
    );
    let rect = bounds[0].1.expand(12.0);
    drag(&ctx, &mut editor, &camera, rect);
    assert_eq!(editor.selected_objects, BTreeSet::from([foreground]));
    assert!(editor.set_xray(true));
    drag(&ctx, &mut editor, &camera, rect);
    assert_eq!(editor.selected_objects, BTreeSet::from([rear, foreground]));
    assert_eq!(
        editor
            .object_at(viewport().center(), viewport(), &camera, false)
            .unwrap(),
        Some(foreground)
    );
    editor.select_object(rear).unwrap();
    editor.enter_edit().unwrap();
    assert_eq!(
        editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap()
            .len(),
        8
    );
    editor.set_xray(false);
    assert!(
        editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap()
            .is_empty()
    );
    editor.set_xray(true);
    editor.leave_edit();
    editor.set_visible_objects(Some(BTreeSet::from([rear])));
    drag(&ctx, &mut editor, &camera, rect);
    assert_eq!(editor.selected_objects, BTreeSet::from([rear]));
    assert_eq!(
        editor
            .object_at(viewport().center(), viewport(), &camera, false)
            .unwrap(),
        Some(rear)
    );
    editor.select_all(viewport(), &camera, false).unwrap();
    assert_eq!(editor.selected_objects, BTreeSet::from([rear]));
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn xray_keeps_projection_clipping_and_viewport_limits() {
    let document = Document {
        objects: vec![Object {
            id: 1,
            name: "Clip probes".into(),
            transform: Transform::default(),
            geometry: Geometry::Mesh(EditableMesh {
                vertices: [
                    [0.0, 0.0, 0.5],
                    [0.0, 0.0, -0.5],
                    [4.0, 0.0, 0.5],
                    [0.0, 0.0, 4.0],
                    [0.0, 0.0, -4.0],
                ]
                .into_iter()
                .enumerate()
                .map(|(index, position)| MeshVertex {
                    id: index as u64 + 1,
                    position,
                })
                .collect(),
                faces: vec![],
                edges: vec![],
            }),
        }],
        ..Default::default()
    };
    let mut cache = GeometryCache::new(
        &document,
        &DisplayFrame {
            center: [0.0; 3],
            scale: 1.0,
        },
        None,
    )
    .unwrap();
    let matrix = DMat4::IDENTITY;
    let projection = Projection {
        matrix,
        inverse: matrix,
        viewport: viewport(),
    };
    cache.project(&projection);
    assert_eq!(
        cache
            .eligible_vertices(SelectionDepth::Through)
            .map(|vertex| vertex.id)
            .collect::<Vec<_>>(),
        vec![1]
    );
}

#[test]
fn xray_preserves_released_transform_sessions_and_refuses_owned_input() {
    let mut editor = cube();
    editor.set_tool(Tool::Move);
    editor.toggle_transform_axis(0).unwrap();
    let original = editor.document.clone();
    editor.nudge(DVec3::X, false).unwrap();
    let preview = editor.document.clone();
    let baseline = editor.history.transaction_baseline().cloned();
    assert!(editor.set_xray(true));
    assert_eq!(editor.document, preview);
    assert_eq!(editor.history.transaction_baseline(), baseline.as_ref());
    assert_eq!(editor.history.undo_len(), 0);
    editor.cancel();
    assert_eq!(editor.document, original);
    assert!(
        editor.xray_enabled(),
        "Cancel restores the edit, not viewport state"
    );
    editor.toggle_transform_axis(0).unwrap();
    editor.numeric_input('2').unwrap();
    let numeric = editor.document.clone();
    assert!(!editor.set_xray(false));
    assert_eq!(editor.document, numeric);
    assert_eq!(editor.numeric_text(), Some("2"));
    editor.cancel();
    assert!(editor.begin_property_edit());
    assert!(!editor.set_xray(false));
    assert!(editor.has_property_edit());
    editor.finish_property_edit(false);
    assert!(editor.set_xray(false));
    assert!(!editor.set_xray(false), "An unchanged toggle is a no-op");
    assert_eq!(editor.history.undo_len(), 0);

    editor.nudge(DVec3::X, false).unwrap();
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.set_xray(true));
    assert!(editor.undo());
    assert_eq!(editor.document, original);
    assert!(
        editor.xray_enabled(),
        "Undo does not restore viewport state"
    );
    assert!(editor.redo());
    assert!(editor.xray_enabled());
}
