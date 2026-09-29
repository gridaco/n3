//! Lazy geometry editing must preserve authored primitives until a real edit,
//! and restore their parameters when that edit is cancelled or undone.
use super::*;
use crate::document::Transform;

fn primitive(kind: PrimitiveKind) -> Editor {
    let mut document = Document::default();
    let id = document.insert_primitive(kind).unwrap();
    let mut editor = Editor::new(document).unwrap();
    editor.set_tool(Tool::Move);
    editor.select_object(id).unwrap();
    editor.snapping.enabled = false;
    editor
}

fn select_vertices(editor: &mut Editor) {
    let id = editor.selected_object.unwrap();
    editor.selected_vertices = editor
        .document
        .eval_object(id)
        .unwrap()
        .vertices
        .iter()
        .map(|vertex| vertex.id)
        .collect();
}

#[test]
fn entering_and_leaving_each_primitive_does_not_mutate_or_record_conversion() {
    for kind in [
        PrimitiveKind::Cube,
        PrimitiveKind::Cylinder,
        PrimitiveKind::Cone,
        PrimitiveKind::Torus,
        PrimitiveKind::Circle,
    ] {
        let mut editor = primitive(kind);
        let original = editor.document.clone();
        let revision = editor.revision;
        assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::EditModeEntered);
        select_vertices(&mut editor);
        assert!(editor.edit_mode);
        assert_eq!(editor.document, original);
        assert_eq!(editor.revision, revision);
        assert_eq!(editor.history.undo_len(), 0);
        assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::EditModeLeft);
        assert_eq!(editor.document, original);
        assert_eq!(editor.revision, revision);
        assert!(!editor.undo());
    }
}

#[test]
fn first_effective_vertex_edit_and_conversion_form_one_undo_step_for_every_shape() {
    for kind in [
        PrimitiveKind::Cube,
        PrimitiveKind::Cylinder,
        PrimitiveKind::Cone,
        PrimitiveKind::Torus,
        PrimitiveKind::Circle,
    ] {
        let mut editor = primitive(kind);
        editor.enter_edit().unwrap();
        select_vertices(&mut editor);
        let original = editor.document.clone();
        let original_edges = original
            .eval_object(editor.selected_object.unwrap())
            .unwrap()
            .edges;
        let selection = editor.selected_vertices.clone();
        assert!(editor.nudge(DVec3::X * 0.25, false).unwrap());
        assert!(matches!(
            editor.document.objects[0].geometry,
            Geometry::Mesh(_)
        ));
        assert_eq!(
            editor
                .document
                .eval_object(editor.selected_object.unwrap())
                .unwrap()
                .edges,
            original_edges
        );
        assert!(editor.edit_mode);
        assert_eq!(editor.selected_vertices, selection);
        assert_eq!(editor.history.undo_len(), 1);
        let edited = editor.document.clone();
        assert!(editor.undo());
        assert_eq!(editor.document, original);
        assert!(editor.edit_mode);
        assert_eq!(editor.selected_vertices, selection);
        assert!(
            !editor.undo(),
            "Conversion must not be a separate undo step"
        );
        assert!(editor.redo());
        assert_eq!(editor.document, edited);
        assert!(editor.edit_mode);
        assert_eq!(editor.selected_vertices, selection);
        assert!(editor.undo());
        editor.leave_edit();
        assert_eq!(editor.document, original);
        assert!(!editor.edit_mode);
    }
}

#[test]
fn exact_return_to_evaluated_baseline_restores_original_primitive_during_editing() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 2, 3, 4]);
    let original = editor.document.clone();
    assert!(editor.nudge(DVec3::Z * 0.25, false).unwrap());
    let changed = editor.document.clone();
    assert!(editor.nudge(-DVec3::Z * 0.25, false).unwrap());
    assert_eq!(editor.document, original);
    assert!(editor.edit_mode);
    assert_eq!(editor.selected_vertices, BTreeSet::from([1, 2, 3, 4]));
    assert_eq!(editor.history.undo_len(), 2);
    assert!(editor.undo());
    assert_eq!(editor.document, changed);
    assert!(editor.redo());
    assert_eq!(editor.document, original);
    editor.leave_edit();
    assert_eq!(editor.document, original);
}

#[test]
fn retained_modification_stays_mesh_after_exiting_and_reentering_edit_mode() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 2, 3, 4]);
    assert!(editor.nudge(DVec3::Z * 0.25, false).unwrap());
    let edited = editor.document.clone();
    editor.leave_edit();
    assert!(matches!(edited.objects[0].geometry, Geometry::Mesh(_)));
    assert_eq!(editor.document, edited);
    assert!(editor.enter_edit().unwrap());
    assert_eq!(editor.document, edited);
    assert_eq!(editor.history.undo_len(), 1);
}

#[test]
fn cancelling_numeric_preview_restores_primitive_mode_and_selection() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 2, 3, 4]);
    let original = editor.document.clone();
    editor.toggle_transform_axis(2).unwrap();
    for character in "0.25".chars() {
        editor.numeric_input(character).unwrap();
    }
    assert!(matches!(
        editor.document.objects[0].geometry,
        Geometry::Mesh(_)
    ));
    assert!(editor.has_transform_session());
    assert_eq!(editor.history.undo_len(), 0);
    assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
    assert_eq!(editor.document, original);
    assert!(editor.edit_mode);
    assert_eq!(editor.selected_vertices, BTreeSet::from([1, 2, 3, 4]));
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn identity_numeric_previews_preserve_primitive_and_redo_without_history() {
    for (tool, value) in [(Tool::Move, '0'), (Tool::Rotate, '0'), (Tool::Scale, '1')] {
        let mut editor = primitive(PrimitiveKind::Cube);
        editor.enter_edit().unwrap();
        select_vertices(&mut editor);
        let original = editor.document.clone();
        editor.nudge(DVec3::X * 0.25, false).unwrap();
        assert!(editor.undo());
        let revision = editor.revision;
        editor.set_tool(tool);
        editor.toggle_transform_axis(0).unwrap();
        editor.numeric_input(value).unwrap();
        assert_eq!(editor.document, original);
        assert_eq!(editor.revision, revision);
        assert_eq!(
            editor.confirm().unwrap(),
            ConfirmOutcome::InteractionFinished
        );
        assert!(editor.edit_mode);
        assert_eq!(editor.document, original);
        assert_eq!(editor.history.undo_len(), 0);
        assert_eq!(editor.history.redo_len(), 1);
    }
}

#[test]
fn vertex_property_preview_uses_evaluated_primitive_baseline_and_can_cancel() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 2, 3, 4]);
    let original = editor.document.clone();
    assert!(editor.begin_property_translation(0.01).unwrap());
    for offset in [0.125, 0.25, 0.5] {
        assert!(
            editor
                .preview_property_translation(2, offset, TranslationSource::Exact)
                .unwrap()
        );
        assert_eq!(editor.property_translation_value(2), Some(offset));
        assert!(editor.edit_mode);
        assert_eq!(editor.history.undo_len(), 0);
    }
    assert!(editor.finish_property_edit(false));
    assert_eq!(editor.document, original);
    assert!(editor.edit_mode);
    assert_eq!(editor.selected_vertices, BTreeSet::from([1, 2, 3, 4]));
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn property_preview_returning_to_zero_keeps_primitive_without_history() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    select_vertices(&mut editor);
    let original = editor.document.clone();
    assert!(editor.begin_property_translation(0.01).unwrap());
    editor
        .preview_property_translation(0, 0.25, TranslationSource::Exact)
        .unwrap();
    assert!(matches!(
        editor.document.objects[0].geometry,
        Geometry::Mesh(_)
    ));
    editor
        .preview_property_translation(0, 0.0, TranslationSource::Exact)
        .unwrap();
    assert_eq!(editor.property_translation_value(0), Some(0.0));
    assert_eq!(editor.document, original);
    editor.finish_property_edit(true);
    assert_eq!(editor.history.undo_len(), 0);
    editor.leave_edit();
    assert_eq!(editor.document, original);
}

#[test]
fn returning_pointer_transform_to_start_restores_primitive_without_conversion_history() {
    for tool in [Tool::Move, Tool::Rotate, Tool::Scale] {
        let mut editor = primitive(PrimitiveKind::Cube);
        editor.enter_edit().unwrap();
        select_vertices(&mut editor);
        let original = editor.document.clone();
        editor.set_tool(tool);
        editor.toggle_transform_axis(0).unwrap();
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
        assert!(matches!(
            editor.document.objects[0].geometry,
            Geometry::Mesh(_)
        ));
        assert_eq!(editor.history.undo_len(), 0);
        editor.preview_transform(start).unwrap();
        assert_eq!(editor.document, original);
        editor.finish_gesture().unwrap();
        editor.confirm().unwrap();
        assert_eq!(editor.document, original);
        assert!(editor.edit_mode);
        assert_eq!(editor.history.undo_len(), 0);
    }
}

#[test]
fn deleting_primitive_vertex_and_incident_faces_is_one_atomic_edit() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1]);
    let original = editor.document.clone();
    let generated = original
        .eval_object(editor.selected_object.unwrap())
        .unwrap();
    assert!(editor.delete_selection().unwrap());
    let Geometry::Mesh(mesh) = &editor.document.objects[0].geometry else {
        panic!("Deleting a generated vertex must retain the edited mesh");
    };
    assert_eq!(
        mesh.vertices,
        generated
            .vertices
            .iter()
            .filter(|vertex| vertex.id != 1)
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        mesh.faces,
        generated
            .faces
            .iter()
            .filter(|face| !face.vertices.contains(&1))
            .cloned()
            .collect::<Vec<_>>()
    );
    assert!(editor.edit_mode);
    assert!(editor.selected_vertices.is_empty());
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, original);
    assert_eq!(editor.selected_vertices, BTreeSet::from([1]));
    assert!(editor.edit_mode);
}

#[test]
fn deleting_circle_vertex_removes_only_incident_edges_and_undo_restores_recipe() {
    let mut editor = primitive(PrimitiveKind::Circle);
    editor.enter_edit().unwrap();
    let id = editor.selected_object.unwrap();
    let baseline = editor.document.clone();
    let generated = baseline.eval_object(id).unwrap();
    let deleted = generated.vertices[0].id;
    editor.selected_vertices = BTreeSet::from([deleted]);
    assert!(editor.delete_selection().unwrap());
    let Geometry::Mesh(mesh) = &editor.document.objects[0].geometry else {
        panic!("Deleting a circle vertex materializes the remaining open path");
    };
    assert!(mesh.faces.is_empty());
    assert_eq!(mesh.vertices.len(), generated.vertices.len() - 1);
    assert_eq!(
        mesh.edges,
        generated
            .edges
            .iter()
            .filter(|edge| !edge.contains(&deleted))
            .copied()
            .collect::<Vec<_>>()
    );
    assert_eq!(mesh.edges.len(), generated.edges.len() - 2);
    assert!(editor.selected_vertices.is_empty());
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.selected_vertices, BTreeSet::from([deleted]));
    assert!(editor.edit_mode);
    assert!(editor.redo());
    editor.document.validate().unwrap();
    assert_eq!(
        editor.document.eval_object(id).unwrap().edges.len(),
        generated.edges.len() - 2
    );
}

#[test]
fn rejected_vertex_candidate_cannot_convert_primitive_or_record_history() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    select_vertices(&mut editor);
    let original = editor.document.clone();
    let revision = editor.revision;
    assert!(editor.nudge(DVec3::splat(f64::MAX), false).is_err());
    assert_eq!(editor.document, original);
    assert_eq!(editor.revision, revision);
    assert!(editor.edit_mode);
    assert_eq!(editor.history.undo_len(), 0);
    assert!(editor.begin_property_translation(0.01).unwrap());
    assert!(
        editor
            .preview_property_translation(0, f64::MAX, TranslationSource::Exact)
            .is_err()
    );
    assert_eq!(editor.document, original);
    editor.finish_property_edit(false);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn changing_primitive_parameters_during_vertex_editing_cannot_stale_the_edit_baseline() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 2, 3, 4]);
    let original = editor.document.clone();
    let revision = editor.revision;
    let change_parameters = |document: &mut Document| {
        let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
            panic!("The unchanged document must retain its authored primitive");
        };
        primitive.size[0] = 7.0;
        Ok(())
    };
    assert!(
        editor
            .commit("Change parameters", change_parameters)
            .is_err()
    );
    assert_eq!(editor.document, original);
    assert_eq!(editor.revision, revision);
    assert!(editor.edit_mode);
    assert_eq!(editor.selected_vertices, BTreeSet::from([1, 2, 3, 4]));
    assert_eq!(editor.history.undo_len(), 0);

    assert!(editor.begin_property_edit());
    assert!(editor.preview_property_edit(change_parameters).is_err());
    assert_eq!(editor.document, original);
    assert_eq!(editor.revision, revision);
    assert!(editor.has_property_edit());
    assert_eq!(editor.history.undo_len(), 0);
    assert!(editor.finish_property_edit(false));
    assert_eq!(editor.document, original);
    assert!(editor.edit_mode);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn transformed_primitive_numeric_edits_keep_object_transform_and_unselected_vertices() {
    for tool in [Tool::Move, Tool::Rotate, Tool::Scale] {
        let mut editor = primitive(PrimitiveKind::Cube);
        editor.document.objects[0].transform = Transform {
            translation: [0.25, -0.5, 0.75],
            rotation: DQuat::from_rotation_y(0.3).to_array(),
            scale: [2.0, 0.5, 3.0],
        };
        editor.reframe().unwrap();
        editor.enter_edit().unwrap();
        editor.selected_vertices = BTreeSet::from([1, 2, 3, 4]);
        let original = editor.document.clone();
        let id = editor.selected_object.unwrap();
        let generated = original.eval_object(id).unwrap();
        let model = original.objects[0].transform.matrix();
        let selected_world: Vec<_> = generated
            .vertices
            .iter()
            .filter(|vertex| editor.selected_vertices.contains(&vertex.id))
            .map(|vertex| model.transform_point3(DVec3::from_array(vertex.position)))
            .collect();
        let pivot = selected_world.iter().copied().sum::<DVec3>() / selected_world.len() as f64;
        editor.set_tool(tool);
        editor.toggle_transform_axis(2).unwrap();
        for character in if tool == Tool::Rotate { "15" } else { "1.25" }.chars() {
            editor.numeric_input(character).unwrap();
        }
        assert_eq!(editor.numeric_error(), None);
        let edited = editor.document.eval_object(id).unwrap();
        assert!(matches!(
            editor.document.objects[0].geometry,
            Geometry::Mesh(_)
        ));
        assert_eq!(
            editor.document.objects[0].transform,
            original.objects[0].transform
        );
        for vertex in &generated.vertices {
            let updated = edited
                .vertices
                .iter()
                .find(|item| item.id == vertex.id)
                .unwrap();
            if editor.selected_vertices.contains(&vertex.id) {
                let from = model.transform_point3(DVec3::from_array(vertex.position));
                let expected = match tool {
                    Tool::Move => from + DVec3::Z * 1.25,
                    Tool::Rotate => {
                        pivot + DQuat::from_rotation_z(15_f64.to_radians()) * (from - pivot)
                    }
                    Tool::Scale => from + DVec3::Z * (from.z - pivot.z) * 0.25,
                    Tool::View => unreachable!(),
                };
                assert!(
                    model
                        .transform_point3(DVec3::from_array(updated.position))
                        .abs_diff_eq(expected, 1e-12)
                );
            } else {
                assert_eq!(updated, vertex);
            }
        }
        editor.confirm().unwrap();
        assert_eq!(editor.history.undo_len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.document, original);
        assert!(editor.edit_mode);
    }
}

#[test]
fn explicitly_converted_mesh_is_not_reinterpreted_as_a_primitive() {
    let mut editor = primitive(PrimitiveKind::Cube);
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    select_vertices(&mut editor);
    let original = editor.document.clone();
    editor.nudge(DVec3::X * 0.25, false).unwrap();
    editor.nudge(-DVec3::X * 0.25, false).unwrap();
    assert_eq!(editor.document, original);
    assert!(matches!(
        editor.document.objects[0].geometry,
        Geometry::Mesh(_)
    ));
    editor.leave_edit();
    assert_eq!(editor.document, original);
}
