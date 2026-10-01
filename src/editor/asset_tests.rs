//! Linked placement behavior shares ordinary selection, permissions and history.
use super::*;
use crate::{
    document::{AssetInstance, Object},
    scene::{SceneAsset, test_support},
};

fn linked() -> (Editor, AssetInstance, Arc<crate::scene::EvaluatedScene>) {
    let scene = SceneAsset::new(test_support::triangle_data()).unwrap();
    let frame = scene.evaluate(0, None).unwrap();
    let reference = AssetInstance {
        source: "assets/triangle.glb".into(),
        scene: 0,
    };
    let mut document = Document::default();
    document
        .insert_asset(reference.clone(), "Linked triangle".into())
        .unwrap();
    let mut editor = Editor::new(document).unwrap();
    editor
        .set_asset_frames(BTreeMap::from([(reference.clone(), frame.clone())]))
        .unwrap();
    editor.reframe().unwrap();
    editor.select_object(1).unwrap();
    (editor, reference, frame)
}

#[test]
fn linked_asset_schema_contains_only_reference_and_placement() {
    let (editor, reference, _) = linked();
    let text = editor.document.to_json().unwrap();
    assert!(text.contains("\"type\": \"asset\""));
    assert!(!text.contains("vertices"));
    assert!(!text.contains("materials"));
    let roundtrip = Document::from_json(&text).unwrap();
    assert_eq!(roundtrip, editor.document);
    assert_eq!(roundtrip.objects[0].geometry, Geometry::Asset(reference));
    assert!(roundtrip.eval_object(1).is_err());
    let mut copy = roundtrip.clone();
    assert!(copy.convert_object(1).is_err());
    assert_eq!(copy, roundtrip);
}

#[test]
fn linked_transform_session_undo_keeps_payload_identical() {
    let (mut editor, reference, frame) = linked();
    let before = editor.document.clone();
    let vertices = frame.draws[0].vertices.clone();
    editor.set_tool(Tool::Move);
    editor.toggle_transform_axis(0).unwrap();
    editor.numeric_input('9').unwrap();
    assert_eq!(
        editor.document.objects[0].transform.translation,
        [9., 0., 0.]
    );
    assert_eq!(editor.history.undo_len(), 0);
    assert_eq!(
        editor.confirm().unwrap(),
        ConfirmOutcome::InteractionFinished
    );
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));
    assert_eq!(frame.draws[0].vertices, vertices);
    assert!(editor.redo());
    assert_eq!(
        editor.document.objects[0].transform.translation,
        [9., 0., 0.]
    );
    assert_eq!(
        editor.document.objects[0].geometry,
        before.objects[0].geometry
    );
}

#[test]
fn linked_rotate_scale_and_property_preview_use_ordinary_transactions() {
    let (mut editor, reference, frame) = linked();
    for (tool, axis, value) in [(Tool::Rotate, 2, "90"), (Tool::Scale, 0, "2")] {
        let before = editor.document.clone();
        editor.set_tool(tool);
        editor.toggle_transform_axis(axis).unwrap();
        for character in value.chars() {
            editor.numeric_input(character).unwrap();
        }
        assert!(editor.numeric_error().is_none());
        editor.confirm().unwrap();
        assert_ne!(editor.document, before);
        assert!(editor.undo());
        assert_eq!(editor.document, before);
    }
    let before = editor.document.clone();
    assert!(editor.begin_property_edit());
    for value in [1., 2., 3.] {
        editor
            .preview_property_edit(|document| {
                document.objects[0].transform.translation[1] = value;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(editor.history.undo_len(), 0);
    assert!(editor.finish_property_edit(true));
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));
}

#[test]
fn linked_duplicate_rename_delete_reuse_cache_and_history() {
    let (mut editor, reference, frame) = linked();
    assert!(editor.duplicate_selection().unwrap());
    let copy = editor.selected_object.unwrap();
    assert_eq!(copy, 2);
    assert_eq!(editor.asset_frames().len(), 1);
    assert_eq!(editor.render_mesh().unwrap().triangle_count, 2);
    assert!(editor.rename_object(copy, "New placement".into()).unwrap());
    assert!(editor.delete_selection().unwrap());
    assert_eq!(editor.document.objects.len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document.objects[1].name, "New placement");
    assert!(editor.undo());
    assert_eq!(editor.document.objects[1].name, "Linked triangle copy");
    assert!(editor.undo());
    assert_eq!(editor.document.objects.len(), 1);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));
}

#[test]
fn asset_contents_do_not_enter_vertex_edit_or_gain_authored_topology() {
    let (mut editor, _, _) = linked();
    let baseline = editor.snapshot();
    assert!(!editor.enter_edit().unwrap());
    assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::NothingToDo);
    assert_eq!(editor.snapshot(), baseline);
    assert!(editor.render_mesh().unwrap().edit_topology.is_empty());
    assert!(!editor.can_make_face());
    assert!(editor.make_face().is_err());
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn mixed_objects_share_frame_picking_box_bounds_and_selection_transform() {
    let (mut editor, _, _) = linked();
    let native = editor.insert(PrimitiveKind::Cube).unwrap();
    editor
        .commit("Separate objects", |document| {
            document.objects[1].transform.translation = [5., 0., 0.];
            Ok(())
        })
        .unwrap();
    editor.reframe().unwrap();
    let rendered = editor.render_mesh().unwrap();
    assert_eq!(rendered.object_count, 2);
    assert_eq!(rendered.triangle_count, 13);
    assert_eq!(rendered.edit_topology.len(), 1);
    assert_eq!(rendered.edit_topology[0].object, native);
    assert_eq!(rendered.source_extent, [6., 4., 2.]);
    let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 800.));
    let mut camera = Camera::default();
    camera.look_from(Vec3::Z);
    let projection = Projection::new(viewport, &camera, false).unwrap();
    let inside = projection
        .screen(editor.frame.world_to_display(DVec3::new(0.5, 0.5, 0.)))
        .unwrap();
    assert_eq!(
        editor.object_at(inside, viewport, &camera, false).unwrap(),
        Some(1)
    );
    editor.prepare(&projection).unwrap();
    assert_eq!(editor.selection_bounds(&projection).len(), 2);
    editor.select_object(1).unwrap();
    editor.select_object_with_modifier(native, true).unwrap();
    let groups = editor.selection_point_groups(false).unwrap();
    assert_eq!(groups.len(), 2);
    editor.set_tool(Tool::Move);
    let before = editor.document.clone();
    editor.nudge(DVec3::Y, false).unwrap();
    for (old, new) in before.objects.iter().zip(&editor.document.objects) {
        assert_eq!(
            new.transform.translation[1],
            old.transform.translation[1] + 1.
        );
        assert_eq!(new.geometry, old.geometry);
    }
    assert!(editor.undo());
    assert_eq!(editor.document, before);
}

#[test]
fn mixed_group_rotation_includes_missing_and_empty_asset_placement_anchors() {
    for resolved in [false, true] {
        let reference = AssetInstance {
            source: "assets/empty.glb".into(),
            scene: 0,
        };
        let mut document = Document::default();
        let native = document.insert_primitive(PrimitiveKind::Cube).unwrap();
        let linked = document
            .insert_asset(reference.clone(), "Empty asset".into())
            .unwrap();
        document.objects[1].transform.translation = [100., 0., 0.];
        let mut editor = Editor::new(document).unwrap();
        if resolved {
            editor
                .set_asset_frames(BTreeMap::from([(
                    reference.clone(),
                    Arc::new(crate::scene::EvaluatedScene::default()),
                )]))
                .unwrap();
        }
        editor.select_object(native).unwrap();
        editor.select_object_with_modifier(linked, true).unwrap();
        let before = editor.document.clone();
        // The cube spans -1..1, and the empty placement contributes x=100.
        let pivot = DVec3::new(49.5, 0., 0.);
        assert_eq!(
            snapshot_pivot(&editor.snapshot(), &editor.frame, editor.asset_frames()).unwrap(),
            pivot
        );
        let projection = Projection::new(
            Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 800.)),
            &Camera::default(),
            false,
        )
        .unwrap();
        editor.prepare(&projection).unwrap();
        assert!(
            (editor
                .frame
                .display_to_world(editor.selection_pivot().unwrap())
                - pivot)
                .length()
                < 1e-12
        );
        assert!(
            editor
                .cache
                .as_ref()
                .unwrap()
                .vertices
                .iter()
                .all(|vertex| vertex.object == native)
        );

        editor.set_tool(Tool::Rotate);
        editor.toggle_transform_axis(2).unwrap();
        for character in "180".chars() {
            editor.numeric_input(character).unwrap();
        }
        assert!(editor.numeric_error().is_none());
        assert!(
            (DVec3::from_array(editor.document.objects[0].transform.translation)
                - DVec3::new(99., 0., 0.))
            .length()
                < 1e-12
        );
        assert!(
            (DVec3::from_array(editor.document.objects[1].transform.translation)
                - DVec3::new(-1., 0., 0.))
            .length()
                < 1e-12
        );
        assert_eq!(editor.history.undo_len(), 0);
        editor.confirm().unwrap();
        let rotated = editor.document.clone();
        assert_eq!(editor.history.undo_len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.document, before);
        assert!(editor.redo());
        assert_eq!(editor.document, rotated);
        assert_eq!(editor.asset_frames().contains_key(&reference), resolved);
    }
}

#[test]
fn read_only_blocks_all_mutation_sources_but_keeps_selection() {
    let (mut editor, reference, frame) = linked();
    editor.rename_object(1, "Committed".into()).unwrap();
    assert!(editor.undo());
    editor.set_access(EditorAccess::ReadOnly);
    let before = editor.document.clone();
    let revision = editor.revision;
    assert!(!editor.can_edit());
    assert!(!editor.can_duplicate_selection());
    assert!(!editor.begin_property_edit());
    assert!(!editor.undo());
    assert!(!editor.redo());
    assert!(
        editor
            .commit("Attempt", |document| {
                document.objects.clear();
                Ok(())
            })
            .is_err()
    );
    assert!(editor.rename_object(1, "No".into()).is_err());
    assert!(editor.delete_selection().is_err());
    assert!(editor.duplicate_selection().is_err());
    assert!(editor.insert(PrimitiveKind::Cube).is_err());
    assert!(editor.insert_asset(reference.clone(), "No".into()).is_err());
    assert!(
        editor
            .import_objects(before.objects.clone(), AssetFrames::new())
            .is_err()
    );
    assert!(editor.enter_edit().is_err());
    assert!(editor.nudge(DVec3::X, false).is_err());
    assert!(editor.toggle_transform_axis(0).is_err());
    editor.set_tool(Tool::Move);
    assert_eq!(editor.tool, Tool::View);
    assert!(!editor.numeric_input_available());
    assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::NothingToDo);
    assert_eq!(editor.document, before);
    assert_eq!(editor.revision, revision);
    editor.deselect();
    editor.select_object(1).unwrap();
    assert_eq!(editor.selected_object, Some(1));
    editor
        .set_asset_frames(BTreeMap::from([(reference, frame)]))
        .unwrap();
    assert_eq!(editor.document, before);
    editor.set_access(EditorAccess::ReadWrite);
    assert!(
        editor.redo(),
        "Read-only must preserve the undo/redo history"
    );
    assert_eq!(editor.document.objects[0].name, "Committed");
}

#[test]
fn enabling_read_only_cancels_unaccepted_preview() {
    let (mut editor, _, _) = linked();
    let before = editor.document.clone();
    assert!(editor.begin_property_edit());
    editor
        .preview_property_edit(|document| {
            document.objects[0].transform.translation[0] = 4.;
            Ok(())
        })
        .unwrap();
    editor.set_access(EditorAccess::ReadOnly);
    assert_eq!(editor.document, before);
    assert!(!editor.has_property_edit());
    assert_eq!(editor.history.undo_len(), 0);
    assert!(!editor.edit_mode);
}

#[test]
fn import_resources_and_object_ids_publish_atomically_and_undo_together() {
    let (mut editor, reference, frame) = linked();
    let before = editor.document.clone();
    let mut invalid = before.objects.clone();
    invalid[0].transform.scale = [0.; 3];
    assert!(editor.import_objects(invalid, AssetFrames::new()).is_err());
    assert_eq!(editor.document, before);
    assert_eq!(editor.history.undo_len(), 0);
    let ids = editor
        .import_objects(
            before.objects.clone(),
            BTreeMap::from([(reference.clone(), frame.clone())]),
        )
        .unwrap();
    assert_eq!(ids, vec![2]);
    assert_eq!(editor.selected_objects, BTreeSet::from([2]));
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(editor.redo());
    assert_eq!(editor.document.objects.len(), 2);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));
}

#[test]
fn unavailable_asset_remains_serializable_selectable_and_transformable() {
    let reference = AssetInstance {
        source: "missing.glb".into(),
        scene: 0,
    };
    let mut document = Document::default();
    let id = document
        .insert_asset(reference, "Unavailable".into())
        .unwrap();
    let mut editor = Editor::new(document).unwrap();
    let mesh = editor.render_mesh().unwrap();
    assert!(mesh.vertices.is_empty());
    assert!(
        mesh.warnings
            .iter()
            .any(|warning| warning.contains("unavailable"))
    );
    editor.select_object(id).unwrap();
    editor.set_tool(Tool::Move);
    editor.toggle_transform_axis(0).unwrap();
    editor.numeric_input('2').unwrap();
    editor.confirm().unwrap();
    assert_eq!(
        editor.document.objects[0].transform.translation,
        [2., 0., 0.]
    );
    assert!(editor.document.to_json().is_ok());
}

#[test]
fn invalid_frame_replacement_is_atomic_and_degenerate_triangles_remain_legal() {
    let (mut editor, reference, frame) = linked();
    let before = editor.document.clone();
    let revision = editor.revision;
    let mut invalid = (*frame).clone();
    invalid.draws[0].vertices[0].position = [f64::INFINITY; 3];
    assert!(
        editor
            .set_asset_frames(BTreeMap::from([(reference.clone(), Arc::new(invalid))]))
            .is_err()
    );
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.document, before);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));
    let mut degenerate = (*frame).clone();
    degenerate.draws[0].indices = Arc::from([0, 0, 0]);
    editor
        .set_asset_frames(BTreeMap::from([(reference, Arc::new(degenerate))]))
        .unwrap();
    assert_eq!(editor.render_mesh().unwrap().triangle_count, 1);
    assert_eq!(editor.document, before);
}

#[test]
fn empty_import_is_not_a_history_entry() {
    let mut editor = Editor::new(Document::default()).unwrap();
    assert!(
        editor
            .import_objects(Vec::<Object>::new(), AssetFrames::new())
            .unwrap()
            .is_empty()
    );
    assert_eq!(editor.history.undo_len(), 0);
    assert_eq!(editor.revision, 0);
}

#[test]
fn first_asset_import_normalizes_large_coordinates_before_gpu_conversion() {
    let (_, reference, frame) = linked();
    let mut huge = (*frame).clone();
    for vertex in &mut huge.draws[0].vertices {
        for coordinate in &mut vertex.position {
            *coordinate *= 1.0e13;
        }
    }
    let object = Object {
        id: 9,
        name: "Large coordinates".into(),
        transform: crate::document::Transform::default(),
        geometry: Geometry::Asset(reference.clone()),
    };
    let mut editor = Editor::new(Document::default()).unwrap();
    editor
        .import_objects(vec![object], BTreeMap::from([(reference, Arc::new(huge))]))
        .unwrap();
    assert!(editor.frame.scale < 1.0e-12);
    let mesh = editor.render_mesh().unwrap();
    assert_eq!(mesh.triangle_count, 1);
    assert!(
        mesh.vertices
            .iter()
            .all(|vertex| vertex.position.into_iter().all(f32::is_finite))
    );
}

#[test]
fn imported_lines_and_points_are_pickable_without_editable_topology() {
    for topology in [
        crate::scene::Topology::Lines,
        crate::scene::Topology::Points,
    ] {
        let (mut editor, reference, frame) = linked();
        let mut derived = (*frame).clone();
        derived.draws[0].topology = topology;
        derived.draws[0].indices = if topology == crate::scene::Topology::Lines {
            Arc::from([0, 1])
        } else {
            Arc::from([0])
        };
        editor
            .set_asset_frames(BTreeMap::from([(reference, Arc::new(derived))]))
            .unwrap();
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000., 800.));
        let mut camera = Camera::default();
        camera.look_from(Vec3::Z);
        let projection = Projection::new(viewport, &camera, false).unwrap();
        let point = if topology == crate::scene::Topology::Lines {
            DVec3::X
        } else {
            DVec3::ZERO
        };
        let screen = projection
            .screen(editor.frame.world_to_display(point))
            .unwrap();
        assert_eq!(
            editor.object_at(screen, viewport, &camera, false).unwrap(),
            Some(1)
        );
        editor.prepare(&projection).unwrap();
        assert_eq!(editor.selection_bounds(&projection).len(), 1);
        let mesh = editor.render_mesh().unwrap();
        assert!(mesh.edit_topology.is_empty());
        assert_eq!(
            mesh.object_ranges[0].loose_edges.len(),
            if topology == crate::scene::Topology::Lines {
                2
            } else {
                0
            }
        );
    }
}

#[test]
fn empty_evaluated_asset_has_a_placement_anchor_without_fake_geometry() {
    let (mut editor, reference, _) = linked();
    editor
        .set_asset_frames(BTreeMap::from([(
            reference,
            Arc::new(crate::scene::EvaluatedScene::default()),
        )]))
        .unwrap();
    editor
        .commit("Position empty asset", |document| {
            document.objects[0].transform.translation = [100.0, 200.0, -30.0];
            Ok(())
        })
        .unwrap();
    editor.reframe().unwrap();
    assert_eq!(editor.frame.center, [100.0, 200.0, -30.0]);
    assert_eq!(editor.selection_points(false).unwrap(), vec![Vec3::ZERO]);
    let rendered = editor.render_mesh().unwrap();
    assert!(rendered.vertices.is_empty());
    assert!(rendered.edit_topology.is_empty());
    assert_eq!(rendered.vertex_count, 0);
}

#[test]
fn extreme_asset_placement_rejection_preserves_numeric_property_and_history_baselines() {
    let (mut editor, reference, frame) = linked();
    editor.rename_object(1, "Keep this redo".into()).unwrap();
    editor.undo();
    let baseline = editor.document.clone();
    editor.set_tool(Tool::Move);
    editor.toggle_transform_axis(0).unwrap();
    let mut last_valid = baseline.clone();
    for character in "100000000000000000".chars() {
        editor.numeric_input(character).unwrap();
        if editor.numeric_error().is_none() {
            last_valid = editor.document.clone();
        } else {
            assert_eq!(editor.document, last_valid);
        }
    }
    assert_eq!(
        editor.numeric_error(),
        Some("Animated geometry exceeds the renderer's numeric range.")
    );
    assert!(editor.confirm().is_err());
    assert_eq!(editor.document, last_valid);
    assert_eq!(editor.history.undo_len(), 0);
    assert!(editor.cancel());
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.history.redo_len(), 1);

    assert!(editor.begin_property_edit());
    editor
        .preview_property_edit(|document| {
            document.objects[0].transform.translation[0] = 2.0;
            Ok(())
        })
        .unwrap();
    let last_valid = editor.document.clone();
    let revision = editor.revision;
    let error = editor
        .preview_property_edit(|document| {
            document.objects[0].transform.translation[0] = 1.0e17;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(
        error,
        "Animated geometry exceeds the renderer's numeric range."
    );
    assert_eq!(editor.document, last_valid);
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), 0);
    assert!(editor.finish_property_edit(false));
    assert_eq!(editor.document, baseline);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));
    assert!(editor.redo());
    assert_eq!(editor.document.objects[0].name, "Keep this redo");

    // Presentation limits never narrow the canonical authored JSON domain.
    let mut extreme = baseline;
    extreme.objects[0].transform.translation[0] = 1.0e17;
    let encoded = extreme.to_json().unwrap();
    assert_eq!(Document::from_json(&encoded).unwrap(), extreme);
}

#[test]
fn light_only_placement_is_rejected_before_document_or_frame_publication() {
    let (mut editor, reference, _) = linked();
    let mut lights = crate::scene::EvaluatedScene::default();
    lights.lights.push(crate::scene::EvaluatedLight {
        node: 0,
        light: 0,
        position_cm: DVec3::ZERO,
        direction: DVec3::NEG_Z,
    });
    let frame = Arc::new(lights);
    editor
        .set_asset_frames(BTreeMap::from([(reference.clone(), frame.clone())]))
        .unwrap();
    let baseline = editor.document.clone();
    let revision = editor.revision;
    let error = editor
        .commit("Invalid light placement", |document| {
            document.objects[0].transform.translation[0] = 1.0e17;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(
        error,
        "A punctual light exceeds the renderer's numeric range."
    );
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), 0);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));

    let mut invalid_pose = (*frame).clone();
    invalid_pose.lights[0].position_cm.x = 1.0e17;
    assert!(
        editor
            .set_asset_frames(BTreeMap::from([(
                reference.clone(),
                Arc::new(invalid_pose)
            )]))
            .is_err()
    );
    assert_eq!(editor.document, baseline);
    assert_eq!(editor.revision, revision);
    assert!(Arc::ptr_eq(&editor.asset_frames()[&reference], &frame));
}
