//! Behavioral integration checks for the shared translation boundary.

use super::*;
use crate::document::{EditableMesh, MeshVertex, Object, Transform};
use crate::snapping::GridReference;

fn object_editor(origin: DVec3, size: f64) -> Editor {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[0].transform.translation = origin.to_array();
    let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
        unreachable!()
    };
    primitive.size = [size; 3];
    let mut editor = Editor::new(document).unwrap();
    editor.set_tool(Tool::Move);
    editor.select_object(id).unwrap();
    editor
}

fn position(editor: &Editor) -> DVec3 {
    DVec3::from_array(editor.document.objects[0].transform.translation)
}

fn projection(z_up: bool) -> Projection {
    Projection::new(
        Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0)),
        &Camera::default(),
        z_up,
    )
    .unwrap()
}

fn start_handle(editor: &mut Editor, projection: &Projection, kind: HandleKind) -> Pos2 {
    editor.prepare(projection).unwrap();
    let handle = editor
        .handles(projection)
        .into_iter()
        .find(|handle| handle.kind == kind)
        .expect("The fixture's transform handle must be reachable");
    let pointer = handle.target.center();
    editor
        .begin_transform(projection, &handle, pointer)
        .unwrap();
    pointer
}

/// Project an intended displacement to a real pointer ray; the editor must
/// recover it through the same camera/constraint path used by native drags.
fn preview_displacement(editor: &mut Editor, delta_cm: DVec3) {
    let Some(Gesture::Transform(drag)) = &editor.gesture else {
        panic!("A transform must be active")
    };
    let pointer = drag
        .projection
        .screen(drag.start + delta_cm * editor.frame.scale)
        .unwrap();
    editor.preview_transform(pointer).unwrap();
}

fn vertex_editor() -> Editor {
    let document = Document {
        objects: vec![Object {
            id: 1,
            name: "Transformed points".into(),
            transform: Transform {
                translation: [0.17, -0.23, 0.31],
                rotation: DQuat::from_euler(glam::EulerRot::XYZ, 0.21, 0.47, -0.32).to_array(),
                scale: [2.0, 0.5, 3.0],
            },
            geometry: Geometry::Mesh(EditableMesh {
                vertices: vec![
                    MeshVertex {
                        id: 10,
                        position: [-0.3, 0.1, -0.2],
                    },
                    MeshVertex {
                        id: 20,
                        position: [0.7, 0.5, 0.4],
                    },
                    MeshVertex {
                        id: 30,
                        position: [2.0, 3.0, 4.0],
                    },
                ],
                edges: vec![],
                faces: vec![],
            }),
        }],
        ..Default::default()
    };
    let mut editor = Editor::new(document).unwrap();
    editor.set_tool(Tool::Move);
    editor.select_object(1).unwrap();
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([10, 20]);
    editor
}

fn world_points(editor: &Editor) -> Vec<DVec3> {
    let object = &editor.document.objects[0];
    let Geometry::Mesh(mesh) = &object.geometry else {
        panic!("The fixture must contain explicit points")
    };
    mesh.vertices
        .iter()
        .map(|vertex| {
            object
                .transform
                .matrix()
                .transform_point3(DVec3::from_array(vertex.position))
        })
        .collect()
}

#[test]
fn default_nudge_cleans_only_the_moved_axis_and_repeats_form_one_history_entry() {
    let origin = DVec3::new(0.3, -0.27, 4.19);
    let mut editor = object_editor(origin, 2.0);
    let baseline = editor.document.clone();
    assert!(editor.snapping.enabled);
    assert_eq!(editor.snapping.step_cm, 1.0);
    assert_eq!(editor.snapping.reference, GridReference::WorldGrid);
    assert!(editor.nudge(DVec3::X, false).unwrap());
    assert_eq!(position(&editor), DVec3::new(1.0, origin.y, origin.z));
    assert!(editor.nudge(DVec3::X, true).unwrap());
    assert_eq!(position(&editor).x, 2.0);
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, baseline);
    assert!(editor.redo());
    assert_eq!(position(&editor).x, 2.0);
}

#[test]
fn group_nudge_uses_active_origin_and_preserves_every_relative_offset() {
    let mut editor = object_editor(DVec3::new(0.17, 0.23, 0.39), 2.0);
    let second = editor
        .document
        .insert_primitive(PrimitiveKind::Cube)
        .unwrap();
    editor.document.objects[1].transform.translation = [4.3, -1.47, 2.19];
    editor.select_object_with_modifier(second, true).unwrap();
    let before = editor.document.clone();
    let anchor = DVec3::from_array(before.objects[1].transform.translation);
    let offset = position(&editor) - anchor;
    editor.nudge(DVec3::new(1.0, -1.0, 0.0), false).unwrap();
    let moved_anchor = DVec3::from_array(editor.document.objects[1].transform.translation);
    assert_eq!(moved_anchor, DVec3::new(5.0, -2.0, 2.19));
    assert!((position(&editor) - moved_anchor).abs_diff_eq(offset, 1e-14));
    for (original, moved) in before.objects.iter().zip(&editor.document.objects) {
        assert_eq!(original.geometry, moved.geometry);
        assert_eq!(original.transform.rotation, moved.transform.rotation);
        assert_eq!(original.transform.scale, moved.transform.scale);
        assert_eq!(
            original.transform.translation[2],
            moved.transform.translation[2]
        );
    }
}

#[test]
fn transformed_vertices_snap_shared_world_center_without_distorting_selection() {
    let mut editor = vertex_editor();
    let original = editor.document.clone();
    let before = world_points(&editor);
    let center = (before[0] + before[1]) * 0.5;
    editor.nudge(DVec3::new(0.8, -1.3, 0.0), false).unwrap();
    let after = world_points(&editor);
    let moved_center = (after[0] + after[1]) * 0.5;
    assert!((moved_center.x - (center.x + 0.8).round()).abs() < 1e-13);
    assert!((moved_center.y - (center.y - 1.3).round()).abs() < 1e-13);
    assert!((moved_center.z - center.z).abs() < 1e-13);
    assert!((after[0] - before[0]).abs_diff_eq(after[1] - before[1], 1e-13));
    assert!((after[1] - after[0]).abs_diff_eq(before[1] - before[0], 1e-13));
    assert_eq!(after[2], before[2]);
    assert_eq!(
        editor.document.objects[0].transform,
        original.objects[0].transform
    );
    let Geometry::Mesh(old_mesh) = &original.objects[0].geometry else {
        unreachable!()
    };
    let Geometry::Mesh(new_mesh) = &editor.document.objects[0].geometry else {
        unreachable!()
    };
    assert_eq!(new_mesh.vertices[2], old_mesh.vertices[2]);
    assert_eq!(new_mesh.faces, old_mesh.faces);
    assert!(editor.undo());
    assert_eq!(editor.document, original);
}

#[test]
fn axis_drag_snaps_canonical_centimeters_across_model_scales_and_source_up_axes() {
    for size in [2.0, 20.0] {
        for z_up in [false, true] {
            let mut editor = object_editor(DVec3::new(0.17, -0.23, 0.31), size);
            editor.snap_policy = StepPolicy::Fixed;
            let original = editor.document.clone();
            let projection = projection(z_up);
            start_handle(&mut editor, &projection, HandleKind::Axis(0));
            preview_displacement(&mut editor, DVec3::X * 0.61);
            assert_eq!(position(&editor), DVec3::new(1.0, -0.23, 0.31));
            assert_eq!(editor.history.undo_len(), 0);
            editor.finish_gesture().unwrap();
            assert_eq!(editor.history.undo_len(), 1);
            assert!(editor.undo());
            assert_eq!(editor.document, original);
        }
    }
}

#[test]
fn plane_drag_snaps_both_allowed_axes_and_keeps_perpendicular_coordinate_exact() {
    let mut editor = object_editor(DVec3::new(0.17, -0.23, 0.31), 20.0);
    editor.snap_policy = StepPolicy::Fixed;
    let projection = projection(false);
    start_handle(&mut editor, &projection, HandleKind::Plane(0, 1));
    preview_displacement(&mut editor, DVec3::new(0.61, -1.1, 0.0));
    assert_eq!(position(&editor), DVec3::new(1.0, -1.0, 0.31));
    editor.finish_gesture().unwrap();
    assert_eq!(editor.history.undo_len(), 1);
}

#[test]
fn drag_policy_is_captured_until_release_and_next_drag_uses_new_policy() {
    let mut editor = object_editor(DVec3::ZERO, 2.0);
    editor.snap_policy = StepPolicy::Fixed;
    let projection = projection(false);
    start_handle(&mut editor, &projection, HandleKind::Axis(0));
    editor.snapping.enabled = false;
    editor.snapping.step_cm = 10.0;
    preview_displacement(&mut editor, DVec3::X * 0.61);
    assert_eq!(position(&editor).x, 1.0);
    editor.finish_gesture().unwrap();
    start_handle(&mut editor, &projection, HandleKind::Axis(0));
    preview_displacement(&mut editor, DVec3::X * 0.13);
    assert!((position(&editor).x - 1.13).abs() < 1e-5);
    editor.finish_gesture().unwrap();
    assert_eq!(editor.history.undo_len(), 2);
}

#[test]
fn no_motion_and_subthreshold_grid_drag_do_not_dirty_document_or_history() {
    for origin in [DVec3::ZERO, DVec3::new(0.3, 0.2, 0.1)] {
        let mut editor = object_editor(origin, 2.0);
        editor.snap_policy = StepPolicy::Fixed;
        let baseline = editor.document.clone();
        let projection = projection(false);
        let start = start_handle(&mut editor, &projection, HandleKind::Axis(0));
        editor.preview_transform(start).unwrap();
        editor.finish_gesture().unwrap();
        assert_eq!(editor.document, baseline);
        assert_eq!(editor.history.undo_len(), 0);
    }
    let mut editor = object_editor(DVec3::ZERO, 2.0);
    editor.snap_policy = StepPolicy::Fixed;
    let baseline = editor.document.clone();
    let projection = projection(false);
    start_handle(&mut editor, &projection, HandleKind::Axis(0));
    preview_displacement(&mut editor, DVec3::X * 0.2);
    editor.finish_gesture().unwrap();
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn snapped_move_session_can_cancel_or_commit_drags_and_nudges_as_one_edit() {
    for accept in [false, true] {
        let mut editor = object_editor(DVec3::new(0.3, -0.27, 0.19), 2.0);
        let original = editor.document.clone();
        editor.toggle_transform_axis(0).unwrap();
        let projection = projection(false);
        start_handle(&mut editor, &projection, HandleKind::Axis(0));
        preview_displacement(&mut editor, DVec3::X * 0.7);
        editor.finish_gesture().unwrap();
        assert!(editor.has_transform_session());
        editor.nudge(DVec3::X, false).unwrap();
        assert_eq!(position(&editor), DVec3::new(2.0, -0.27, 0.19));
        assert_eq!(editor.history.undo_len(), 0);
        if accept {
            assert_eq!(
                editor.confirm().unwrap(),
                ConfirmOutcome::InteractionFinished
            );
            assert_eq!(editor.history.undo_len(), 1);
            assert!(editor.undo());
        } else {
            assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
            assert_eq!(editor.history.undo_len(), 0);
        }
        assert_eq!(editor.document, original);
        assert!(!editor.has_transform_session());
        assert_eq!(editor.transform_axis, None);
    }
}

#[test]
fn property_scrub_snaps_live_from_baseline_captures_settings_and_records_once() {
    let mut editor = object_editor(DVec3::new(0.3, 0.2, 0.1), 2.0);
    editor.snap_policy = StepPolicy::Fixed;
    let baseline = editor.document.clone();
    assert!(editor.begin_property_translation(0.02).unwrap());
    editor.snapping.enabled = false;
    editor.snapping.step_cm = 10.0;
    for (raw, expected) in [(0.8, 1.0), (2.4, 2.0), (1.6, 2.0), (0.7, 1.0)] {
        editor
            .preview_property_translation(0, raw, TranslationSource::Interactive)
            .unwrap();
        assert_eq!(position(&editor), DVec3::new(expected, 0.2, 0.1));
        assert_eq!(editor.history.undo_len(), 0);
    }
    editor.finish_property_edit(true);
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, baseline);
    assert!(
        !editor.snapping.enabled,
        "Preferences do not belong to undo snapshots"
    );
    assert!(editor.begin_property_translation(0.02).unwrap());
    editor
        .preview_property_translation(0, 0.47, TranslationSource::Interactive)
        .unwrap();
    assert!((position(&editor).x - 0.47).abs() < 1e-15);
    editor.finish_property_edit(false);
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.history.redo_len(), 1);
}

#[test]
fn exact_property_input_retains_fractions_and_tiny_destinations_without_roundtrip_loss() {
    for value in [0.125, 1e-200, -1e-200] {
        let mut editor = object_editor(DVec3::new(1.25, 0.2, 0.1), 2.0);
        let baseline = editor.document.clone();
        assert!(editor.begin_property_translation(0.02).unwrap());
        editor
            .preview_property_translation(0, value, TranslationSource::Exact)
            .unwrap();
        assert_eq!(position(&editor).x.to_bits(), value.to_bits());
        assert_eq!(position(&editor).y, 0.2);
        editor.finish_property_edit(true);
        assert_eq!(editor.history.undo_len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.document, baseline);
        assert!(editor.redo());
        assert_eq!(position(&editor).x.to_bits(), value.to_bits());
    }
}

#[test]
fn vertex_property_offsets_share_snapping_and_exact_input_policy() {
    let mut editor = vertex_editor();
    editor.snap_policy = StepPolicy::Fixed;
    let before = world_points(&editor);
    let center = (before[0] + before[1]) * 0.5;
    assert!(editor.begin_property_translation(0.02).unwrap());
    editor
        .preview_property_translation(0, 0.8, TranslationSource::Interactive)
        .unwrap();
    let after = world_points(&editor);
    assert!((((after[0] + after[1]) * 0.5).x - (center.x + 0.8).round()).abs() < 1e-13);
    assert!((after[0] - before[0]).abs_diff_eq(after[1] - before[1], 1e-13));
    editor
        .preview_property_translation(0, 0.125, TranslationSource::Exact)
        .unwrap();
    let after = world_points(&editor);
    for index in [0, 1] {
        assert!((after[index] - before[index]).abs_diff_eq(DVec3::X * 0.125, 1e-13));
    }
    assert_eq!(after[2], before[2]);
    editor.finish_property_edit(false);
    assert_eq!(world_points(&editor), before);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn custom_fixed_step_and_relative_policy_are_shared_by_pointer_and_property_scrub() {
    let mut editor = object_editor(DVec3::new(0.3, 0.2, 0.1), 2.0);
    editor.snap_policy = StepPolicy::Fixed;
    editor.snapping.step_cm = 0.25;
    editor.snapping.reference = GridReference::Relative;
    let projection = projection(false);
    start_handle(&mut editor, &projection, HandleKind::Axis(0));
    preview_displacement(&mut editor, DVec3::X * 0.3);
    editor.finish_gesture().unwrap();
    assert_eq!(position(&editor).x, 0.55);
    assert!(editor.begin_property_translation(0.02).unwrap());
    editor
        .preview_property_translation(0, 0.86, TranslationSource::Interactive)
        .unwrap();
    assert_eq!(position(&editor).x, 0.8);
    editor.finish_property_edit(true);
    assert!(editor.undo());
    assert_eq!(position(&editor).x, 0.55);
    assert!(editor.undo());
    assert_eq!(position(&editor).x, 0.3);
}

fn captured_step(editor: &Editor) -> f64 {
    let Some(Gesture::Transform(drag)) = &editor.gesture else {
        panic!("A pointer transform must be active")
    };
    drag.snapping.step_cm
}

#[test]
fn adaptive_default_preserves_apparent_precision_across_physical_scales() {
    let mut ratios = Vec::new();
    for size in [0.02, 2.0, 200.0, 20_000.0] {
        for z_up in [false, true] {
            let mut editor = object_editor(DVec3::ZERO, size);
            assert_eq!(editor.snap_policy, StepPolicy::default());
            let original = editor.document.clone();
            let projection = projection(z_up);
            start_handle(&mut editor, &projection, HandleKind::Axis(0));
            let step = captured_step(&editor);
            let Some(Gesture::Transform(drag)) = &editor.gesture else {
                unreachable!()
            };
            let cm_per_point = projection.pixel_size(drag.pivot).unwrap() / editor.frame.scale;
            assert!(step / cm_per_point <= 4.0 + 1e-8);
            assert!(step / cm_per_point >= 1.6 - 1e-8);
            ratios.push(step / size);
            preview_displacement(&mut editor, DVec3::X * step * 3.2);
            assert!((position(&editor).x / step - 3.0).abs() < 1e-9);
            assert_eq!(position(&editor).y, 0.0);
            assert_eq!(
                editor.document.objects[0].geometry,
                original.objects[0].geometry
            );
            editor.finish_gesture().unwrap();
            assert_eq!(editor.history.undo_len(), 1);
            editor.undo();
            assert_eq!(editor.document, original);
        }
    }
    assert!(ratios.iter().all(|ratio| (ratio - ratios[0]).abs() < 1e-12));
}

#[test]
fn zoom_changes_next_adaptive_drag_but_never_a_captured_drag_or_undo_result() {
    for aligned in [false, true] {
        let mut editor = object_editor(DVec3::ZERO, 2.0);
        let baseline = editor.document.clone();
        let mut camera = Camera::default();
        if aligned {
            camera.look_from(Vec3::Z);
        }
        let viewport = projection(false).viewport;
        let initial = editor.movement_snap_step(viewport, &camera, false).unwrap();
        let old_projection = Projection::new(viewport, &camera, false).unwrap();
        start_handle(&mut editor, &old_projection, HandleKind::Axis(0));
        assert_eq!(captured_step(&editor), initial);
        camera.zoom(1.5);
        editor.snap_policy = StepPolicy::Fixed;
        editor.snapping.step_cm = 100.0;
        assert_eq!(
            editor.movement_snap_step(viewport, &camera, false),
            Some(initial)
        );
        preview_displacement(&mut editor, DVec3::X * initial * 3.2);
        let outward = editor.document.clone();
        preview_displacement(&mut editor, DVec3::X * initial * 7.2);
        preview_displacement(&mut editor, DVec3::X * initial * 3.2);
        assert_eq!(
            editor.document, outward,
            "Reversing cannot change the drag's measuring scale"
        );
        editor.finish_gesture().unwrap();
        editor.snap_policy = StepPolicy::default();
        let closer = editor.movement_snap_step(viewport, &camera, false).unwrap();
        assert!(closer < initial);
        assert!(editor.undo());
        assert_eq!(editor.document, baseline);
        assert!(editor.redo());
        assert_eq!(
            editor.document, outward,
            "Undo/redo stores geometry, not camera-dependent recomputation"
        );
    }
}

#[test]
fn adaptive_small_drag_and_return_to_start_do_not_leave_history() {
    let mut editor = object_editor(DVec3::ZERO, 2.0);
    let baseline = editor.document.clone();
    let projection = projection(false);
    let start = start_handle(&mut editor, &projection, HandleKind::Axis(0));
    let step = captured_step(&editor);
    preview_displacement(&mut editor, DVec3::X * step * 0.2);
    assert_eq!(editor.document, baseline);
    preview_displacement(&mut editor, DVec3::X * step * 4.0);
    assert_ne!(editor.document, baseline);
    editor.preview_transform(start).unwrap();
    editor.finish_gesture().unwrap();
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn adaptive_numeric_scrub_uses_field_sensitivity_and_captures_precision() {
    let mut editor = object_editor(DVec3::ZERO, 2.0);
    assert!(editor.begin_property_translation(0.02).unwrap());
    assert_eq!(editor.property_snapping.step_cm, 0.05);
    editor.snap_policy = StepPolicy::Fixed;
    editor.snapping.step_cm = 10.0;
    for (raw, expected) in [(0.071, 0.05), (0.28, 0.3), (0.18, 0.2)] {
        editor
            .preview_property_translation(0, raw, TranslationSource::Interactive)
            .unwrap();
        assert_eq!(position(&editor).x, expected);
    }
    editor.finish_property_edit(false);
    assert_eq!(position(&editor), DVec3::ZERO);
    assert_eq!(editor.history.undo_len(), 0);
    editor.snap_policy = StepPolicy::default();
    assert!(editor.begin_property_translation(0.01).unwrap());
    assert_eq!(editor.property_snapping.step_cm, 0.02);
    editor
        .preview_property_translation(0, 0.1234567, TranslationSource::Exact)
        .unwrap();
    assert_eq!(position(&editor).x, 0.1234567);
    editor.finish_property_edit(true);
    assert_eq!(editor.history.undo_len(), 1);
}

#[test]
fn coarse_pointer_grid_cannot_swallow_centimeter_keyboard_nudges() {
    let mut editor = object_editor(DVec3::ZERO, 20_000.0);
    for policy in [StepPolicy::Fixed, StepPolicy::default()] {
        editor.snap_policy = policy;
        editor.snapping.step_cm = 100.0;
        let baseline = editor.document.clone();
        let start = position(&editor).x;
        editor.nudge(DVec3::X, false).unwrap();
        assert_eq!(position(&editor).x, start + 1.0);
        editor.nudge(DVec3::X * 10.0, true).unwrap();
        assert_eq!(position(&editor).x, start + 11.0);
        editor.undo();
        assert_eq!(editor.document, baseline);
    }
}

#[test]
fn adaptive_plane_and_transformed_vertex_movement_share_one_resolved_step() {
    let mut editor = vertex_editor();
    let baseline = editor.document.clone();
    let before = world_points(&editor);
    let center = (before[0] + before[1]) * 0.5;
    let projection = projection(false);
    start_handle(&mut editor, &projection, HandleKind::Plane(0, 1));
    let step = captured_step(&editor);
    preview_displacement(&mut editor, DVec3::new(3.3 * step, -4.3 * step, 0.0));
    let after = world_points(&editor);
    let moved_center = (after[0] + after[1]) * 0.5;
    for axis in [0, 1] {
        assert!((moved_center[axis] / step - (moved_center[axis] / step).round()).abs() < 1e-10);
    }
    assert!((moved_center.z - center.z).abs() < 1e-13);
    assert!((after[1] - after[0]).abs_diff_eq(before[1] - before[0], 1e-13));
    assert_eq!(after[2], before[2]);
    editor.cancel();
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.history.undo_len(), 0);
}
