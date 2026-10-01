//! Operation-level numeric input, shared by native shortcuts and tutorials.
use super::*;
use crate::document::{EditableMesh, MeshVertex, Object, Transform};

fn object_editor(tool: Tool, axis: usize) -> Editor {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let mut editor = Editor::new(document).unwrap();
    editor.select_object(id).unwrap();
    editor.set_tool(tool);
    editor.toggle_transform_axis(axis).unwrap();
    editor
}

#[test]
fn numeric_and_pointer_pivots_share_normalized_centers_at_large_world_offsets() {
    for edit_vertices in [false, true] {
        let mut document = Document::default();
        let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
        let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
            unreachable!()
        };
        primitive.size = [1.0e307; 3];
        document.objects[0].transform.translation = [1.0e308, 0.0, 0.0];
        if edit_vertices {
            document.convert_object(id).unwrap();
        }
        let mut editor = Editor::new(document).unwrap();
        editor.select_object(id).unwrap();
        if edit_vertices {
            editor.enter_edit().unwrap();
            editor.selected_vertices = editor
                .document
                .eval_object(id)
                .unwrap()
                .vertices
                .iter()
                .map(|vertex| vertex.id)
                .collect();
        }
        editor.set_tool(Tool::Scale);
        editor.toggle_transform_axis(0).unwrap();
        let projection = Projection::new(
            Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0)),
            &Camera::default(),
            false,
        )
        .unwrap();
        editor.prepare(&projection).unwrap();
        let pointer_world = editor
            .frame
            .display_to_world(editor.selection_pivot().unwrap());
        let numeric_world =
            snapshot_pivot(&editor.snapshot(), &editor.frame, editor.asset_frames()).unwrap();
        assert_eq!(numeric_world, pointer_world);
        assert!(numeric_world.is_finite());
        assert_eq!(numeric_world.x, 1.0e308);
        let before = editor.document.clone();
        type_text(&mut editor, "1.1");
        assert!(
            editor.numeric_error().is_none(),
            "A finite normalized pivot must not overflow while averaging world positions"
        );
        assert_ne!(editor.document, before);
        editor.confirm().unwrap();
        assert_eq!(editor.history.undo_len(), 1);
        editor.undo();
        assert_eq!(editor.document, before);
    }
}

fn type_text(editor: &mut Editor, text: &str) {
    for character in text.chars() {
        editor.numeric_input(character).unwrap();
    }
}

fn position(editor: &Editor) -> DVec3 {
    DVec3::from_array(editor.document.objects[0].transform.translation)
}

#[test]
fn move_digits_replace_total_amount_without_compounding_and_commit_once() {
    let mut editor = object_editor(Tool::Move, 0);
    editor.document.objects[0].transform.translation = [0.3, 0.7, -0.2];
    let before = editor.document.clone();
    editor.numeric_input('1').unwrap();
    assert_eq!(position(&editor).x, 1.3);
    editor.numeric_input('2').unwrap();
    assert_eq!(position(&editor).x, 12.3);
    assert_eq!(editor.numeric_text(), Some("12"));
    assert_eq!(editor.history.undo_len(), 0);
    editor.confirm().unwrap();
    assert_eq!(editor.numeric_text(), None);
    assert_eq!(editor.transform_axis, None);
    assert_eq!(editor.history.undo_len(), 1);
    let after = editor.document.clone();
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(editor.redo());
    assert_eq!(editor.document, after);
}

#[test]
fn exact_signed_decimal_moves_bypass_coarse_snapping() {
    let mut editor = object_editor(Tool::Move, 2);
    editor.snapping.step_cm = 1000.0;
    editor.snap_policy = crate::snapping::StepPolicy::Fixed;
    type_text(&mut editor, "-.125");
    assert_eq!(position(&editor), DVec3::new(0.0, 0.0, -0.125));
    assert!(editor.numeric_error().is_none());
    assert!(
        editor
            .movement_snap_step(
                Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0)),
                &Camera::default(),
                false
            )
            .is_none()
    );
    editor.escape();
    assert_eq!(position(&editor), DVec3::ZERO);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn input_length_limit_cannot_commit_a_silently_truncated_value() {
    let mut editor = object_editor(Tool::Move, 0);
    let baseline = editor.document.clone();
    type_text(&mut editor, &"0".repeat(512));
    assert!(editor.numeric_error().is_none());
    assert!(editor.numeric_input('9').is_err());
    assert!(editor.confirm().is_err());
    assert_eq!(editor.document, baseline);
    assert!(editor.has_transform_session());
    editor.numeric_backspace().unwrap();
    assert!(editor.numeric_error().is_none());
    editor.confirm().unwrap();
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn incomplete_invalid_and_backspace_states_do_not_commit_last_valid_preview() {
    let mut editor = object_editor(Tool::Move, 0);
    let baseline = editor.document.clone();
    type_text(&mut editor, "12.");
    let valid = editor.document.clone();
    editor.numeric_input('.').unwrap();
    assert!(editor.numeric_error().is_some());
    assert!(editor.confirm().is_err());
    assert_eq!(editor.document, valid);
    assert!(editor.has_transform_session());
    for _ in 0..4 {
        editor.numeric_backspace().unwrap();
    }
    assert_eq!(editor.numeric_text(), Some(""));
    assert_eq!(editor.document, baseline);
    type_text(&mut editor, "-.");
    assert!(editor.numeric_error().is_some());
    assert!(editor.confirm().is_err());
    editor.escape();
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn numeric_entry_replaces_prior_session_nudges_and_holds_pointer_ownership() {
    let mut editor = object_editor(Tool::Move, 0);
    let baseline = editor.document.clone();
    editor.nudge(DVec3::X * 4.0, false).unwrap();
    type_text(&mut editor, "2");
    assert_eq!(position(&editor).x, 2.0);
    assert!(editor.nudge(DVec3::X, false).is_err());
    editor.preview_transform(egui::pos2(700.0, 900.0)).unwrap();
    assert_eq!(position(&editor).x, 2.0);
    assert!(!editor.finish_gesture().unwrap());
    editor.escape();
    assert_eq!(editor.document, baseline);
}

#[test]
fn changing_axis_reuses_number_against_original_operation_baseline() {
    let mut editor = object_editor(Tool::Move, 0);
    type_text(&mut editor, "2");
    editor.toggle_transform_axis(1).unwrap();
    assert_eq!(position(&editor), DVec3::Y * 2.0);
    assert_eq!(editor.numeric_text(), Some("2"));
    editor.toggle_transform_axis(1).unwrap();
    assert_eq!(position(&editor), DVec3::ZERO);
    assert_eq!(editor.numeric_text(), None);
    assert_eq!(editor.transform_axis, None);
    editor.confirm().unwrap();
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn numeric_rotation_degrees_use_world_axis_and_replace_digit_previews() {
    let mut editor = object_editor(Tool::Rotate, 2);
    let original = DQuat::from_rotation_x(0.3);
    editor.document.objects[0].transform.rotation = original.to_array();
    type_text(&mut editor, "90");
    let expected = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2) * original;
    let actual = DQuat::from_array(editor.document.objects[0].transform.rotation);
    assert!(actual.abs_diff_eq(expected, 1e-12));
    editor.confirm().unwrap();
    assert_eq!(editor.history.undo_len(), 1);
    editor.undo();
    assert_eq!(
        editor.document.objects[0].transform.rotation,
        original.to_array()
    );
}

#[test]
fn group_rotation_orbits_shared_pivot_and_axis_scale_is_rejected() {
    let mut editor = object_editor(Tool::Rotate, 2);
    editor.cancel();
    let second = editor
        .document
        .insert_primitive(PrimitiveKind::Cube)
        .unwrap();
    editor.document.objects[0].transform.translation = [-2.0, 0.0, 0.0];
    editor.document.objects[1].transform.translation = [2.0, 0.0, 0.0];
    editor.select_object_with_modifier(second, true).unwrap();
    editor.toggle_transform_axis(2).unwrap();
    type_text(&mut editor, "90");
    assert!(position(&editor).abs_diff_eq(DVec3::new(0.0, -2.0, 0.0), 1e-12));
    assert!(
        DVec3::from_array(editor.document.objects[1].transform.translation)
            .abs_diff_eq(DVec3::new(0.0, 2.0, 0.0), 1e-12)
    );
    editor.confirm().unwrap();
    editor.set_tool(Tool::Scale);
    assert!(editor.toggle_transform_axis(0).is_err());
    assert!(!editor.numeric_input_available());
}

#[test]
fn object_scale_is_positive_factor_on_local_axis_and_identity_is_noop() {
    let mut editor = object_editor(Tool::Scale, 0);
    let original = DQuat::from_rotation_z(0.7).to_array();
    editor.document.objects[0].transform.rotation = original;
    type_text(&mut editor, "2.5");
    assert_eq!(editor.document.objects[0].transform.scale, [2.5, 1.0, 1.0]);
    assert_eq!(editor.document.objects[0].transform.rotation, original);
    editor.escape();
    editor.toggle_transform_axis(0).unwrap();
    type_text(&mut editor, "0");
    assert!(editor.numeric_error().is_some());
    assert!(editor.confirm().is_err());
    editor.numeric_backspace().unwrap();
    type_text(&mut editor, "-1");
    assert!(editor.numeric_error().is_some());
    editor.escape();
    editor.toggle_transform_axis(0).unwrap();
    type_text(&mut editor, "1");
    editor.confirm().unwrap();
    assert_eq!(editor.history.undo_len(), 0);
}

fn vertex_editor(tool: Tool, axis: usize) -> Editor {
    let document = Document {
        objects: vec![Object {
            id: 1,
            name: "Points".into(),
            transform: Transform {
                translation: [0.3, -0.2, 0.4],
                rotation: DQuat::from_rotation_y(0.3).to_array(),
                scale: [2.0, 0.5, 3.0],
            },
            geometry: Geometry::Mesh(EditableMesh {
                vertices: vec![
                    MeshVertex {
                        id: 1,
                        position: [-1.0, 0.0, 0.0],
                    },
                    MeshVertex {
                        id: 2,
                        position: [1.0, 0.0, 0.0],
                    },
                    MeshVertex {
                        id: 3,
                        position: [0.0, 2.0, 0.0],
                    },
                ],
                edges: vec![],
                faces: vec![],
            }),
        }],
        ..Default::default()
    };
    let mut editor = Editor::new(document).unwrap();
    editor.select_object(1).unwrap();
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 2]);
    editor.set_tool(tool);
    editor.toggle_transform_axis(axis).unwrap();
    editor
}

#[test]
fn vertex_numeric_transforms_preserve_unselected_points_and_object_transform() {
    for tool in [Tool::Move, Tool::Rotate, Tool::Scale] {
        let mut editor = vertex_editor(tool, 2);
        let baseline = editor.document.clone();
        let original = baseline.eval_object(1).unwrap();
        let matrix = baseline.objects[0].transform.matrix();
        let from: Vec<_> = original
            .vertices
            .iter()
            .map(|v| matrix.transform_point3(DVec3::from_array(v.position)))
            .collect();
        let pivot = (from[0] + from[1]) * 0.5;
        type_text(&mut editor, if tool == Tool::Rotate { "90" } else { "2" });
        let after = editor.document.eval_object(1).unwrap();
        for (i, from) in from.iter().copied().enumerate().take(2) {
            let expected = match tool {
                Tool::Move => from + DVec3::Z * 2.0,
                Tool::Rotate => {
                    pivot + DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2) * (from - pivot)
                }
                Tool::Scale => from + DVec3::Z * (from.z - pivot.z),
                _ => unreachable!(),
            };
            assert!(
                matrix
                    .transform_point3(DVec3::from_array(after.vertices[i].position))
                    .abs_diff_eq(expected, 1e-12)
            );
        }
        assert_eq!(after.vertices[2], original.vertices[2]);
        assert_eq!(
            editor.document.objects[0].transform,
            baseline.objects[0].transform
        );
        editor.confirm().unwrap();
        assert_eq!(editor.history.undo_len(), 1);
        editor.undo();
        assert_eq!(editor.document, baseline);
    }
}

#[test]
fn invalid_or_unrenderable_numeric_values_cannot_commit_previous_preview() {
    for tool in [Tool::Move, Tool::Scale] {
        let mut editor = object_editor(tool, 0);
        editor.numeric_input('2').unwrap();
        for _ in 0..310 {
            editor.numeric_input('9').unwrap();
        }
        assert!(editor.numeric_error().is_some());
        assert!(editor.confirm().is_err());
        assert_eq!(editor.history.undo_len(), 0);
        assert!(editor.document.validate().is_ok());
        editor.escape();
        assert_eq!(position(&editor), DVec3::ZERO);
    }
}

#[test]
fn numeric_availability_tracks_selection_axis_tool_and_property_ownership() {
    let mut editor = object_editor(Tool::Move, 0);
    assert!(editor.numeric_input_available());
    editor.toggle_transform_axis(0).unwrap();
    assert!(!editor.numeric_input_available());
    assert!(editor.begin_property_edit());
    assert!(!editor.numeric_input_available());
    editor.finish_property_edit(false);
    editor.set_tool(Tool::View);
    assert!(!editor.numeric_input_available());
    editor.set_tool(Tool::Rotate);
    editor.toggle_transform_axis(1).unwrap();
    editor.deselect();
    assert!(!editor.numeric_input_available());
    let mut vertices = vertex_editor(Tool::Scale, 1);
    vertices.selected_vertices.clear();
    assert!(!vertices.numeric_input_available());
}

#[test]
fn armed_rotate_and_scale_drags_stay_pending_until_confirm_or_cancel() {
    for tool in [Tool::Rotate, Tool::Scale] {
        for accept in [true, false] {
            let mut editor = object_editor(tool, 0);
            let baseline = editor.document.clone();
            let projection = Projection::new(
                Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0)),
                &Camera::default(),
                false,
            )
            .unwrap();
            editor.prepare(&projection).unwrap();
            let handle = editor.locked_transform_handle(&projection).unwrap();
            let start = handle.target.center();
            editor.begin_transform(&projection, &handle, start).unwrap();
            editor
                .preview_transform(start + egui::vec2(35.0, -30.0))
                .unwrap();
            assert_ne!(editor.document, baseline);
            editor.finish_gesture().unwrap();
            assert!(editor.has_transform_session());
            assert_eq!(editor.history.undo_len(), 0);
            if accept {
                editor.confirm().unwrap();
                assert_eq!(editor.history.undo_len(), 1);
                editor.undo();
            } else {
                editor.escape();
            }
            assert_eq!(editor.document, baseline);
        }
    }
}
