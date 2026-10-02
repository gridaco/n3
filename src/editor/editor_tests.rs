use super::*;
use crate::document::{EditableMesh, Face, MeshVertex, Object, Transform};
use egui::{Event, Modifiers};

fn cube() -> Editor {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let mut editor = Editor::new(document).unwrap();
    editor.set_tool(Tool::Move);
    editor.select_object(id).unwrap();
    editor
}

#[test]
fn duplicate_objects_keep_geometry_order_and_active_selection_in_one_undo_step() {
    let mut document = Document::default();
    document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let mesh_id = document.insert_primitive(PrimitiveKind::Cylinder).unwrap();
    document.convert_object(mesh_id).unwrap();
    document.insert_primitive(PrimitiveKind::Cone).unwrap();
    for (object, id) in document.objects.iter_mut().zip([40, 3, 70]) {
        object.id = id;
    }
    document.objects[0].name = "Live shape".into();
    document.objects[1].name = "Editable shape".into();
    document.objects[0].transform.translation = [0.125, -2.0, 3.0];
    document.objects[0].transform.rotation = DQuat::from_rotation_y(0.4).to_array();
    document.objects[1].transform.scale = [2.0, 0.5, 1.5];
    let mut editor = Editor::new(document).unwrap();
    editor.select_object(3).unwrap();
    editor.select_object_with_modifier(40, true).unwrap();
    let baseline = editor.snapshot();
    let frame = editor.frame;
    assert!(editor.can_duplicate_selection());
    assert!(editor.duplicate_selection().unwrap());
    assert_eq!(editor.history.undo_len(), 1);
    assert_eq!(editor.document.objects.len(), 5);
    assert_eq!(&editor.document.objects[..3], &baseline.document.objects);
    for (source, copy) in baseline.document.objects[..2]
        .iter()
        .zip(&editor.document.objects[3..])
    {
        assert_eq!(copy.name, format!("{} copy", source.name));
        assert_eq!(copy.transform, source.transform);
        assert_eq!(copy.geometry, source.geometry);
    }
    assert_eq!(editor.document.objects[3].id, 71);
    assert_eq!(editor.document.objects[4].id, 72);
    assert_eq!(editor.selected_objects, BTreeSet::from([71, 72]));
    assert_eq!(
        editor.selected_object,
        Some(71),
        "Active source maps to its own copy"
    );
    assert_eq!(editor.frame, frame);
    let duplicated = editor.snapshot();
    assert!(editor.undo());
    assert!(editor.snapshot() == baseline);
    assert!(
        !editor.undo(),
        "Duplicate contributes exactly one history entry"
    );
    assert!(editor.redo());
    assert!(editor.snapshot() == duplicated);

    // Editing either copy cannot mutate its source through shared storage.
    editor
        .commit("Edit copies independently", |document| {
            let Geometry::Primitive(primitive) = &mut document.objects[3].geometry else {
                panic!("Duplicating must keep a primitive live")
            };
            primitive.size[0] = 7.0;
            let Geometry::Mesh(mesh) = &mut document.objects[4].geometry else {
                panic!("Duplicating must keep explicit mesh topology")
            };
            for vertex in &mut mesh.vertices {
                vertex.position[0] += 0.125;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(&editor.document.objects[..3], &baseline.document.objects);
    assert_ne!(
        editor.document.objects[3].geometry,
        editor.document.objects[0].geometry
    );
    assert_ne!(
        editor.document.objects[4].geometry,
        editor.document.objects[1].geometry
    );
}

#[test]
fn property_preview_changes_live_but_records_one_undo_step_on_acceptance() {
    let mut editor = cube();
    let original = editor.snapshot();
    assert!(editor.begin_property_edit());
    for width in [2.5, 3.0, 4.0] {
        assert!(
            editor
                .preview_property_edit(|document| {
                    let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
                        panic!("The fixture has a live primitive")
                    };
                    primitive.size[0] = width;
                    Ok(())
                })
                .unwrap()
        );
        let Geometry::Primitive(primitive) = &editor.document.objects[0].geometry else {
            panic!("Preview must keep a live primitive")
        };
        assert_eq!(primitive.size[0], width);
        assert_eq!(editor.history.undo_len(), 0);
    }
    assert!(!editor.has_transform_session());
    assert!(editor.finish_property_edit(true));
    assert_eq!(editor.history.undo_len(), 1);
    let accepted = editor.snapshot();
    assert!(editor.undo());
    assert_eq!(editor.snapshot(), original);
    assert!(editor.redo());
    assert_eq!(editor.snapshot(), accepted);
}

#[test]
fn property_cancellation_validation_and_noop_preserve_redo() {
    let mut editor = cube();
    editor
        .commit("First change", |document| {
            document.objects[0].name = "Changed".into();
            Ok(())
        })
        .unwrap();
    assert!(editor.undo());
    let baseline = editor.snapshot();
    assert_eq!(editor.history.redo_len(), 1);
    assert!(editor.begin_property_edit());
    assert!(editor.finish_property_edit(true));
    assert_eq!(editor.history.undo_len(), 0);
    assert_eq!(editor.history.redo_len(), 1);
    assert!(editor.begin_property_edit());
    assert!(
        editor
            .preview_property_edit(|document| {
                document.objects[0].transform.translation[0] = 1.0;
                Ok(())
            })
            .unwrap()
    );
    let valid_preview = editor.snapshot();
    assert!(
        editor
            .preview_property_edit(|document| {
                document.objects[0].transform.scale[0] = 0.0;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(editor.snapshot(), valid_preview);
    assert!(editor.finish_property_edit(false));
    assert_eq!(editor.snapshot(), baseline);
    assert_eq!(editor.history.redo_len(), 1);
    assert!(editor.redo());
    assert_eq!(editor.document.objects[0].name, "Changed");
}

#[test]
fn renaming_one_layer_is_one_undoable_document_change() {
    let mut editor = cube();
    let id = editor.selected_object.unwrap();
    let original = editor.snapshot();
    assert!(editor.rename_object(id, "  New name  ".into()).unwrap());
    assert_eq!(editor.document.objects[0].name, "New name");
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.snapshot(), original);
    assert!(editor.redo());
    assert_eq!(editor.document.objects[0].name, "New name");
    assert!(editor.rename_object(id, " ".into()).is_err());
    assert_eq!(editor.document.objects[0].name, "New name");
}

#[test]
fn duplicate_is_a_noop_for_empty_edit_and_interacting_selections() {
    for context in 0..6 {
        let mut editor = cube();
        match context {
            0 => editor.deselect(),
            1 => {
                editor.convert_selected().unwrap();
                editor.enter_edit().unwrap();
                editor.selected_vertices.insert(1);
            }
            2 => editor.toggle_transform_axis(0).unwrap(),
            3 => {
                editor.toggle_transform_axis(0).unwrap();
                editor.nudge(DVec3::X, false).unwrap();
            }
            4 => {
                let projection = Projection::new(viewport(), &front(), false).unwrap();
                editor.prepare(&projection).unwrap();
                let handle = editor
                    .handles(&projection)
                    .into_iter()
                    .find(|handle| handle.kind == HandleKind::Axis(0))
                    .unwrap();
                editor
                    .begin_transform(&projection, &handle, handle.target.center())
                    .unwrap();
            }
            _ => {
                let ctx = egui::Context::default();
                let camera = front();
                let start = egui::pos2(30.0, 500.0);
                frame(&ctx, &mut editor, &camera, vec![], true);
                frame(
                    &ctx,
                    &mut editor,
                    &camera,
                    vec![Event::PointerMoved(start), pointer(start, true)],
                    true,
                );
                assert!(editor.is_pointer_interacting());
            }
        }
        let before = editor.snapshot();
        let revision = editor.revision;
        let history = (editor.history.undo_len(), editor.history.redo_len());
        let session = editor.has_transform_session();
        let pointer = editor.is_pointer_interacting();
        let axis = editor.transform_axis;
        assert!(!editor.can_duplicate_selection(), "Context {context}");
        assert!(!editor.duplicate_selection().unwrap());
        assert!(editor.snapshot() == before);
        assert_eq!(editor.revision, revision);
        assert_eq!(
            (editor.history.undo_len(), editor.history.redo_len()),
            history
        );
        assert_eq!(editor.has_transform_session(), session);
        assert_eq!(editor.is_pointer_interacting(), pointer);
        assert_eq!(editor.transform_axis, axis);
    }
}

#[test]
fn duplicate_id_exhaustion_is_atomic_even_after_allocating_one_candidate_copy() {
    for last_id in [u64::MAX, u64::MAX - 1] {
        let mut document = pair().document;
        document.objects[1].id = last_id;
        let mut editor = Editor::new(document).unwrap();
        editor.set_tool(Tool::Move);
        editor.select_object(1).unwrap();
        editor.select_object_with_modifier(last_id, true).unwrap();
        editor.nudge(DVec3::Y, false).unwrap();
        let redo_document = editor.document.clone();
        assert!(editor.undo());
        let before = editor.snapshot();
        let revision = editor.revision;
        let frame = editor.frame;
        assert!(editor.can_duplicate_selection());
        assert!(
            editor
                .duplicate_selection()
                .unwrap_err()
                .contains("Object ID space exhausted")
        );
        assert!(editor.snapshot() == before);
        assert_eq!(editor.revision, revision);
        assert_eq!(editor.frame, frame);
        assert_eq!(
            (editor.history.undo_len(), editor.history.redo_len()),
            (0, 1)
        );
        assert!(editor.redo());
        assert_eq!(editor.document, redo_document);
    }
}

#[test]
fn object_hover_uses_nearest_surface_without_changing_selection_or_document() {
    let mut document = Document::default();
    let rear = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[0].transform.translation[2] = -2.0;
    let front_object = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[1].transform.translation[2] = 2.0;
    let mut editor = Editor::new(document).unwrap();
    editor.select_object(rear).unwrap();
    let original = editor.document.clone();
    let revision = editor.revision;
    let hit = editor
        .object_at(viewport().center(), viewport(), &front(), false)
        .unwrap();
    assert_eq!(
        hit,
        Some(front_object),
        "The selected rear object cannot win hover through the front surface"
    );
    assert!(
        editor.cache.as_ref().unwrap().projection.is_none(),
        "Object hover must not run per-vertex projection or visibility queries"
    );
    editor.hover_object(hit);
    assert_eq!(editor.hovered_object, Some(front_object));
    assert_eq!(editor.selected_object, Some(rear));
    assert_eq!(editor.document, original);
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), 0);
    assert_eq!(editor.history.redo_len(), 0);

    editor.tool = Tool::View;
    assert_eq!(
        editor
            .object_at(viewport().center(), viewport(), &front(), false)
            .unwrap(),
        hit
    );
    assert_eq!(
        editor
            .object_at(egui::pos2(-1.0, -1.0), viewport(), &front(), false)
            .unwrap(),
        None
    );
}

#[test]
fn loose_circle_picking_follows_the_visible_ring_in_both_projections() {
    let mut z_up_camera = Camera::default();
    z_up_camera.look_from(display_rotation(true).transform_vector3(Vec3::Z));
    for (camera, z_up) in [
        (front(), false),
        (Camera::default(), false),
        (z_up_camera, true),
    ] {
        let mut document = Document::default();
        let id = document.insert_primitive(PrimitiveKind::Circle).unwrap();
        let mut editor = Editor::new(document).unwrap();
        let generated = editor.document.eval_object(id).unwrap();
        let point = DVec3::from_array(generated.vertices[0].position);
        let projection = Projection::new(viewport(), &camera, z_up).unwrap();
        let screen = projection
            .screen(editor.frame.world_to_display(point))
            .unwrap();
        let center = projection
            .screen(editor.frame.world_to_display(DVec3::ZERO))
            .unwrap();
        let outward = (screen - center).normalized();
        for position in [screen, screen + outward * 4.0] {
            assert_eq!(
                editor
                    .object_at(position, viewport(), &camera, z_up)
                    .unwrap(),
                Some(id)
            );
        }
        for position in [center, screen + outward * 10.0] {
            assert_eq!(
                editor
                    .object_at(position, viewport(), &camera, z_up)
                    .unwrap(),
                None
            );
        }
        assert!(editor.cache.as_ref().unwrap().projection.is_none());
        assert_eq!(
            editor
                .object_selection_bounds(viewport(), &camera, z_up)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(editor.history.undo_len(), 0);
    }
}

#[test]
fn loose_edges_obey_surface_occlusion_and_local_view_without_becoming_faces() {
    for front_surface in [true, false] {
        let mut document = Document::default();
        let ring = document.insert_primitive(PrimitiveKind::Circle).unwrap();
        let cube = document.insert_primitive(PrimitiveKind::Cube).unwrap();
        document.objects[1].transform.scale = [4.0; 3];
        document.objects[1].transform.translation[2] = if front_surface { 4.0 } else { -4.0 };
        let mut editor = Editor::new(document).unwrap();
        let camera = front();
        let projection = Projection::new(viewport(), &camera, false).unwrap();
        let position =
            DVec3::from_array(editor.document.eval_object(ring).unwrap().vertices[0].position);
        let edge = projection
            .screen(editor.frame.world_to_display(position))
            .unwrap();
        let center = projection
            .screen(editor.frame.world_to_display(DVec3::ZERO))
            .unwrap();
        assert_eq!(
            editor.object_at(edge, viewport(), &camera, false).unwrap(),
            Some(if front_surface { cube } else { ring })
        );
        assert_eq!(
            editor
                .object_at(center, viewport(), &camera, false)
                .unwrap(),
            Some(cube)
        );
        editor.set_visible_objects(Some(BTreeSet::from([ring])));
        assert_eq!(
            editor.object_at(edge, viewport(), &camera, false).unwrap(),
            Some(ring)
        );
        assert_eq!(
            editor
                .object_at(center, viewport(), &camera, false)
                .unwrap(),
            None
        );
    }
}

#[test]
fn loose_circle_single_and_double_click_share_the_object_hit_policy() {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Circle).unwrap();
    let mut editor = Editor::new(document.clone()).unwrap();
    editor.set_tool(Tool::View);
    let camera = front();
    let projection = Projection::new(viewport(), &camera, false).unwrap();
    let position = DVec3::from_array(document.eval_object(id).unwrap().vertices[0].position);
    let edge = projection
        .screen(editor.frame.world_to_display(position))
        .unwrap();
    let center = projection
        .screen(editor.frame.world_to_display(DVec3::ZERO))
        .unwrap();
    let ctx = egui::Context::default();
    frame(&ctx, &mut editor, &camera, vec![], true);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(edge), pointer(edge, true)],
        true,
    );
    frame(&ctx, &mut editor, &camera, vec![pointer(edge, false)], true);
    assert_eq!(editor.selected_object, Some(id));
    assert!(!editor.edit_mode);
    // A fresh context gives the following gesture an independent double-click clock.
    let ctx = egui::Context::default();
    frame(&ctx, &mut editor, &camera, vec![], true);
    double_click(&ctx, &mut editor, &camera, edge);
    assert!(editor.edit_mode);
    let ctx = egui::Context::default();
    frame(&ctx, &mut editor, &camera, vec![], true);
    double_click(&ctx, &mut editor, &camera, center);
    assert!(!editor.edit_mode);
    assert_eq!(editor.document, document);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn isolated_objects_are_the_only_pick_and_box_targets_and_do_not_occlude_vertices() {
    let mut document = Document::default();
    let rear = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[0].transform.translation[2] = -2.0;
    document.convert_object(rear).unwrap();
    let front_id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[1].transform.translation[2] = 2.0;
    let mut editor = Editor::new(document).unwrap();
    let viewport = viewport();
    let camera = front();

    assert_eq!(
        editor
            .object_at(viewport.center(), viewport, &camera, false)
            .unwrap(),
        Some(front_id)
    );
    editor.set_visible_objects(Some(BTreeSet::from([rear])));
    assert_eq!(
        editor
            .object_at(viewport.center(), viewport, &camera, false)
            .unwrap(),
        Some(rear),
        "The hidden front surface must not intercept a local-view pick"
    );
    assert_eq!(
        editor
            .object_selection_bounds(viewport, &camera, false)
            .unwrap()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        vec![rear],
        "A box select must not reach hidden objects"
    );
    editor.select_object(rear).unwrap();
    editor.enter_edit().unwrap();
    assert!(
        !editor
            .selectable_vertices(viewport, &camera, false)
            .unwrap()
            .is_empty(),
        "Hidden geometry must not occlude vertices in the isolated mesh"
    );
    editor.leave_edit();
    editor.set_visible_objects(None);
    assert_eq!(
        editor
            .object_at(viewport.center(), viewport, &camera, false)
            .unwrap(),
        Some(front_id),
        "Leaving local view restores the complete pick scene"
    );
}

#[test]
fn local_view_filters_selection_commands_and_history_selection_without_editing_document() {
    let mut document = Document::default();
    let first = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let second = document.insert_primitive(PrimitiveKind::Cone).unwrap();
    let third = document.insert_primitive(PrimitiveKind::Cylinder).unwrap();
    let mut editor = Editor::new(document).unwrap();
    let original = editor.document.clone();
    let camera = front();
    editor.select_object(first).unwrap();
    editor
        .commit("Rename first", |document| {
            document.objects[0].name = "Earlier name".into();
            Ok(())
        })
        .unwrap();
    editor.select_object(second).unwrap();
    let history_len = editor.history.undo_len();
    let revision = editor.revision;

    editor.set_visible_objects(Some(BTreeSet::from([second, third])));
    assert_eq!(
        editor.revision, revision,
        "View isolation is not a model edit"
    );
    assert_eq!(editor.history.undo_len(), history_len);
    assert_eq!(
        editor.visible_objects(),
        Some(&BTreeSet::from([second, third]))
    );
    assert!(editor.select_object(first).is_err());
    editor.hover_object(Some(first));
    assert_eq!(editor.hovered_object, None);

    editor.deselect();
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_object, Some(second));
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_object, Some(third));
    editor.select_all(viewport(), &camera, false).unwrap();
    assert_eq!(editor.selected_objects, BTreeSet::from([second, third]));
    assert_eq!(editor.history.undo_len(), history_len);

    assert!(editor.undo());
    assert_eq!(editor.document, original);
    assert!(
        editor.selected_objects.is_empty(),
        "Undo cannot reselect hidden IDs"
    );
    assert_eq!(
        editor.visible_objects(),
        Some(&BTreeSet::from([second, third]))
    );
    editor.set_visible_objects(None);
    editor.select_all(viewport(), &camera, false).unwrap();
    assert_eq!(
        editor.selected_objects,
        BTreeSet::from([first, second, third])
    );
}

#[test]
fn objects_created_in_local_view_stay_visible_and_selected() {
    let mut editor = cube();
    let first = editor.selected_object.unwrap();
    editor.set_visible_objects(Some(BTreeSet::from([first])));

    let inserted = editor.insert(PrimitiveKind::Cone).unwrap();
    assert!(editor.is_object_visible(inserted));
    assert_eq!(editor.selected_objects, BTreeSet::from([inserted]));
    assert!(editor.duplicate_selection().unwrap());
    let duplicate = editor.selected_object.unwrap();
    assert_ne!(duplicate, inserted);
    assert!(editor.is_object_visible(duplicate));
    assert_eq!(editor.selected_objects, BTreeSet::from([duplicate]));

    assert!(editor.undo());
    assert_eq!(editor.selected_objects, BTreeSet::from([inserted]));
    assert!(editor.redo());
    assert_eq!(editor.selected_objects, BTreeSet::from([duplicate]));
}

#[test]
fn object_hover_is_ephemeral_across_edit_mode_history_and_removal() {
    let mut editor = cube();
    let id = editor.selected_object.unwrap();
    editor.hover_object(Some(id));
    editor.convert_selected().unwrap();
    assert_eq!(editor.hovered_object, None);
    editor.hover_object(Some(id));
    editor.enter_edit().unwrap();
    assert_eq!(editor.hovered_object, None);
    editor.hover_object(Some(id));
    assert_eq!(
        editor.hovered_object, None,
        "Vertex editing suppresses object hover"
    );
    editor.leave_edit();
    editor.hover_object(Some(id));
    editor.undo();
    assert_eq!(
        editor.hovered_object, None,
        "Undo must not restore a historical pointer target"
    );
    editor.hover_object(Some(id));
    editor
        .commit("Remove object", |document| {
            document.objects.clear();
            Ok(())
        })
        .unwrap();
    assert_eq!(editor.hovered_object, None);
    editor.hover_object(Some(id));
    assert_eq!(editor.hovered_object, None, "Removed IDs cannot be hovered");
}

fn viewport() -> Rect {
    Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))
}

fn front() -> Camera {
    let mut camera = Camera::default();
    camera.look_from(glam::Vec3::Z);
    camera
}

fn pointer(position: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn frame(
    ctx: &egui::Context,
    editor: &mut Editor,
    camera: &Camera,
    events: Vec<Event>,
    focused: bool,
) -> Option<String> {
    let mut error = None;
    let modifiers = events
        .iter()
        .rev()
        .find_map(|event| match event {
            Event::PointerButton { modifiers, .. } | Event::Key { modifiers, .. } => {
                Some(*modifiers)
            }
            _ => None,
        })
        .unwrap_or_else(|| ctx.input(|input| input.modifiers));
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(viewport()),
            events: std::iter::once(Event::ModifiersChanged(modifiers))
                .chain(events)
                .collect(),
            focused,
            ..Default::default()
        },
        |root_ui| {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(root_ui, |ui| {
                    let response = ui.allocate_rect(viewport(), egui::Sense::click_and_drag());
                    error = editor.ui(ui, &response, camera, false);
                });
        },
    )
    .textures_delta
    .clear();
    error
}

fn pair() -> Editor {
    let mut document = Document::default();
    document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[0].transform.translation[0] = -2.0;
    document.objects[1].transform.translation[0] = 2.0;
    let mut editor = Editor::new(document).unwrap();
    editor.set_tool(Tool::Move);
    editor
}

fn primary_with_modifiers(position: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn click_object(
    ctx: &egui::Context,
    editor: &mut Editor,
    camera: &Camera,
    point: Pos2,
    modifiers: Modifiers,
) {
    assert!(
        frame(
            ctx,
            editor,
            camera,
            vec![
                Event::PointerMoved(point),
                primary_with_modifiers(point, true, modifiers)
            ],
            true
        )
        .is_none()
    );
    assert!(
        frame(
            ctx,
            editor,
            camera,
            vec![primary_with_modifiers(point, false, modifiers)],
            true
        )
        .is_none()
    );
}

fn box_drag(
    ctx: &egui::Context,
    editor: &mut Editor,
    camera: &Camera,
    rect: Rect,
    modifiers: Modifiers,
    release: bool,
) {
    assert!(
        frame(
            ctx,
            editor,
            camera,
            vec![
                Event::PointerMoved(rect.min),
                primary_with_modifiers(rect.min, true, modifiers)
            ],
            true
        )
        .is_none()
    );
    assert!(
        frame(
            ctx,
            editor,
            camera,
            vec![Event::PointerMoved(rect.max)],
            true
        )
        .is_none()
    );
    if release {
        assert!(
            frame(
                ctx,
                editor,
                camera,
                vec![primary_with_modifiers(rect.max, false, modifiers)],
                true
            )
            .is_none()
        );
    }
}

#[test]
fn primary_object_box_selects_multiple_in_every_tool_without_history() {
    for tool in [Tool::View, Tool::Move, Tool::Rotate, Tool::Scale] {
        let ctx = egui::Context::default();
        let mut editor = pair();
        editor.tool = tool;
        let camera = front();
        let original = editor.document.clone();
        let bounds = editor
            .object_selection_bounds(viewport(), &camera, false)
            .unwrap();
        let selection_box = bounds
            .iter()
            .fold(Rect::NOTHING, |rect, (_, bounds)| rect.union(*bounds))
            .expand(12.0);
        frame(&ctx, &mut editor, &camera, vec![], true);
        box_drag(
            &ctx,
            &mut editor,
            &camera,
            selection_box,
            Modifiers::NONE,
            true,
        );
        assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]), "{tool:?}");
        assert!(
            editor
                .selected_objects
                .contains(&editor.selected_object.unwrap())
        );
        assert_eq!(editor.document, original);
        assert_eq!(editor.revision, 0);
        assert_eq!(editor.history.undo_len(), 0);
        assert_eq!(editor.history.redo_len(), 0);
        assert!(!editor.is_interacting());
    }
}

#[test]
fn shift_object_click_toggles_box_adds_and_cancel_restores_active_member() {
    let ctx = egui::Context::default();
    let mut editor = pair();
    editor.tool = Tool::View;
    let camera = front();
    let original = editor.document.clone();
    let bounds: BTreeMap<_, _> = editor
        .object_selection_bounds(viewport(), &camera, false)
        .unwrap()
        .into_iter()
        .collect();
    frame(&ctx, &mut editor, &camera, vec![], true);
    click_object(
        &ctx,
        &mut editor,
        &camera,
        bounds[&1].center(),
        Modifiers::NONE,
    );
    click_object(
        &ctx,
        &mut editor,
        &camera,
        bounds[&2].center(),
        Modifiers::SHIFT,
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    assert_eq!(editor.selected_object, Some(2));
    click_object(
        &ctx,
        &mut editor,
        &camera,
        bounds[&2].center(),
        Modifiers::SHIFT,
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));
    assert_eq!(editor.selected_object, Some(1));
    click_object(
        &ctx,
        &mut editor,
        &camera,
        egui::pos2(20.0, 300.0),
        Modifiers::SHIFT,
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));
    box_drag(
        &ctx,
        &mut editor,
        &camera,
        bounds[&2].expand(10.0),
        Modifiers::SHIFT,
        true,
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    let active = editor.selected_object;
    box_drag(
        &ctx,
        &mut editor,
        &camera,
        bounds[&2].expand(10.0),
        Modifiers::NONE,
        false,
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    assert_eq!(editor.selected_object, active);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![pointer(bounds[&2].expand(10.0).max, false)],
        true,
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    assert_eq!(editor.document, original);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn box_timing_and_hit_policies_are_independent_and_captured_on_press() {
    for timing in [SelectionTiming::OnRelease, SelectionTiming::Live] {
        for hit in [BoxHitPolicy::Intersects, BoxHitPolicy::Contains] {
            for additive in [false, true] {
                let ctx = egui::Context::default();
                let mut editor = pair();
                editor.tool = Tool::View;
                editor.box_selection = BoxSelectionPolicy { timing, hit };
                editor.select_object(2).unwrap();
                let camera = front();
                let original = editor.document.clone();
                let bounds = editor
                    .object_selection_bounds(viewport(), &camera, false)
                    .unwrap()[0]
                    .1;
                let partial = Rect::from_min_max(
                    bounds.min - egui::vec2(10.0, 10.0),
                    egui::pos2(bounds.center().x, bounds.max.y + 10.0),
                );
                let before = BTreeSet::from([2]);
                let mut expected = if additive {
                    before.clone()
                } else {
                    BTreeSet::new()
                };
                if hit == BoxHitPolicy::Intersects {
                    expected.insert(1);
                }
                let modifiers = if additive {
                    Modifiers::SHIFT
                } else {
                    Modifiers::NONE
                };
                frame(&ctx, &mut editor, &camera, vec![], true);
                box_drag(&ctx, &mut editor, &camera, partial, modifiers, false);
                assert_eq!(
                    editor.selected_objects,
                    if timing == SelectionTiming::Live {
                        expected.clone()
                    } else {
                        before.clone()
                    }
                );
                // The gesture keeps the settings it started with.
                editor.box_selection = BoxSelectionPolicy {
                    timing: if timing == SelectionTiming::Live {
                        SelectionTiming::OnRelease
                    } else {
                        SelectionTiming::Live
                    },
                    hit: if hit == BoxHitPolicy::Contains {
                        BoxHitPolicy::Intersects
                    } else {
                        BoxHitPolicy::Contains
                    },
                };
                frame(
                    &ctx,
                    &mut editor,
                    &camera,
                    vec![primary_with_modifiers(partial.max, false, modifiers)],
                    true,
                );
                assert_eq!(editor.selected_objects, expected);
                assert_eq!(editor.document, original);
                assert_eq!(editor.revision, 0);
                assert_eq!(
                    (editor.history.undo_len(), editor.history.redo_len()),
                    (0, 0)
                );
            }
        }
    }
}

#[test]
fn deferred_box_uses_release_position_and_enter_finishes_only_once() {
    let ctx = egui::Context::default();
    let mut editor = pair();
    editor.tool = Tool::View;
    let camera = front();
    let bounds = editor
        .object_selection_bounds(viewport(), &camera, false)
        .unwrap()[0]
        .1;
    let rect = bounds.expand(10.0);
    frame(&ctx, &mut editor, &camera, vec![], true);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(rect.min), pointer(rect.min, true)],
        true,
    );
    assert!(editor.selected_objects.is_empty());
    // No separate motion event: the release itself carries the endpoint.
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![pointer(rect.max, false)],
        true,
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));

    let empty = Rect::from_min_max(egui::pos2(20.0, 30.0), egui::pos2(55.0, 65.0));
    box_drag(&ctx, &mut editor, &camera, empty, Modifiers::NONE, false);
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));
    assert_eq!(
        editor.confirm().unwrap(),
        ConfirmOutcome::InteractionFinished
    );
    assert!(editor.selected_objects.is_empty());
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![pointer(rect.center(), false)],
        true,
    );
    assert!(
        editor.selected_objects.is_empty(),
        "The accepted gesture's release cannot click a new object"
    );
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn vertex_box_timing_matches_objects_and_focus_loss_discards_pending_selection() {
    for timing in [SelectionTiming::OnRelease, SelectionTiming::Live] {
        let ctx = egui::Context::default();
        let mut editor = cube();
        editor.convert_selected().unwrap();
        editor.enter_edit().unwrap();
        editor.tool = Tool::View;
        editor.box_selection.timing = timing;
        let camera = front();
        let visible = editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap();
        let before = BTreeSet::from([visible[0].0]);
        editor.set_vertex_selection(before.clone());
        let target = Rect::from_center_size(visible[1].1, egui::vec2(20.0, 20.0));
        let expected = BTreeSet::from([visible[1].0]);
        let original = editor.document.clone();
        let history = (editor.history.undo_len(), editor.history.redo_len());
        frame(&ctx, &mut editor, &camera, vec![], true);
        box_drag(&ctx, &mut editor, &camera, target, Modifiers::NONE, false);
        assert_eq!(
            editor.selected_vertices,
            if timing == SelectionTiming::Live {
                expected.clone()
            } else {
                before.clone()
            }
        );
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![Event::WindowFocused(false)],
            false,
        );
        assert_eq!(editor.selected_vertices, before);
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![pointer(target.max, false)],
            true,
        );
        assert_eq!(editor.selected_vertices, before);
        box_drag(&ctx, &mut editor, &camera, target, Modifiers::NONE, false);
        assert_eq!(
            editor.confirm().unwrap(),
            ConfirmOutcome::InteractionFinished
        );
        assert_eq!(editor.selected_vertices, expected);
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![pointer(target.max, false)],
            true,
        );
        assert_eq!(editor.selected_vertices, expected);
        assert_eq!(editor.document, original);
        assert_eq!(
            (editor.history.undo_len(), editor.history.redo_len()),
            history
        );
    }
}

#[test]
fn contained_object_box_requires_full_bounds_and_visible_surface_vertices() {
    let mut editor = pair();
    editor.box_selection.hit = BoxHitPolicy::Contains;
    let camera = front();
    let bounds = editor
        .object_selection_bounds(viewport(), &camera, false)
        .unwrap();
    let first = bounds[0].1;
    let ctx = egui::Context::default();
    editor.tool = Tool::View;
    frame(&ctx, &mut editor, &camera, vec![], true);
    let partial = Rect::from_min_max(
        first.min - egui::vec2(10., 10.),
        egui::pos2(first.center().x, first.max.y + 10.),
    );
    box_drag(&ctx, &mut editor, &camera, partial, Modifiers::NONE, true);
    assert!(
        editor.selected_objects.is_empty(),
        "Overlap alone must not select an object"
    );
    let mut document = Document::default();
    document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[0].transform.translation[2] = -2.0;
    document.objects[1].transform.translation[2] = 2.0;
    document.objects[1].transform.scale = [2.0, 2.0, 1.0];
    let mut occluded = Editor::new(document).unwrap();
    let visible = occluded
        .object_selection_bounds(viewport(), &camera, false)
        .unwrap();
    assert_eq!(
        visible.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![2],
        "Fully hidden rear geometry cannot qualify by bounds alone"
    );
    let ctx = egui::Context::default();
    frame(&ctx, &mut occluded, &camera, vec![], true);
    box_drag(
        &ctx,
        &mut occluded,
        &camera,
        visible[0].1.expand(10.),
        Modifiers::NONE,
        true,
    );
    assert_eq!(occluded.selected_objects, BTreeSet::from([2]));
}

#[test]
fn multiple_selection_guards_editing_and_reconciles_active_ids_with_history() {
    let mut editor = pair();
    editor.select_object(1).unwrap();
    editor.select_object_with_modifier(2, true).unwrap();
    let original = editor.document.clone();
    assert!(editor.enter_edit().unwrap_err().contains("one object"));
    assert!(
        editor
            .convert_selected()
            .unwrap_err()
            .contains("one object")
    );
    assert_eq!(editor.document, original);
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    editor.leave_edit();
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    editor
        .commit("Remove active", |document| {
            document.objects.retain(|object| object.id != 2);
            Ok(())
        })
        .unwrap();
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));
    assert_eq!(editor.selected_object, Some(1));
    assert!(editor.undo());
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    assert_eq!(editor.selected_object, Some(2));
    assert!(editor.redo());
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));
    let inserted = editor.insert(PrimitiveKind::Cone).unwrap();
    assert_eq!(editor.selected_objects, BTreeSet::from([inserted]));
    editor.undo();
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));
}

#[test]
fn group_move_rotation_and_uniform_scale_transform_all_members_atomically() {
    for tool in [Tool::Move, Tool::Rotate, Tool::Scale] {
        let mut editor = pair();
        editor.document.objects[0].transform.rotation = DQuat::from_rotation_z(0.3).to_array();
        editor.document.objects[1].transform.rotation = DQuat::from_rotation_y(-0.5).to_array();
        editor.document.objects[0].transform.scale = [1.2, 0.8, 1.0];
        editor.select_object(1).unwrap();
        editor.select_object_with_modifier(2, true).unwrap();
        editor.tool = tool;
        let original = editor.document.clone();
        let projection = Projection::new(viewport(), &front(), false).unwrap();
        editor.prepare(&projection).unwrap();
        let handles = editor.handles(&projection);
        let kind = match tool {
            Tool::Move => HandleKind::Axis(0),
            Tool::Rotate => HandleKind::Axis(2),
            _ => HandleKind::Uniform,
        };
        if tool == Tool::Scale {
            assert_eq!(handles.len(), 1);
            assert_eq!(handles[0].kind, HandleKind::Uniform);
        }
        let handle = handles
            .into_iter()
            .find(|handle| handle.kind == kind)
            .unwrap();
        let (start, end) = if tool == Tool::Rotate {
            (
                projection
                    .screen(handle.pivot + DVec3::X * handle.length)
                    .unwrap(),
                projection
                    .screen(handle.pivot + DVec3::Y * handle.length)
                    .unwrap(),
            )
        } else {
            (
                handle.target.center(),
                projection
                    .screen(handle.pivot + handle.axis * handle.length * 1.5)
                    .unwrap(),
            )
        };
        editor.begin_transform(&projection, &handle, start).unwrap();
        editor.preview_transform(start).unwrap();
        assert_eq!(editor.document, original);
        editor.preview_transform(end).unwrap();
        for (before, after) in original.objects.iter().zip(&editor.document.objects) {
            assert_eq!(
                before.geometry, after.geometry,
                "Group transforms must not bake geometry"
            );
            assert_ne!(
                before.transform, after.transform,
                "Every selected member must transform"
            );
            if tool == Tool::Scale {
                for axis in 0..3 {
                    assert!(
                        (after.transform.scale[axis] / before.transform.scale[axis] - 1.5).abs()
                            < 1e-5
                    );
                }
                assert_eq!(after.transform.rotation, before.transform.rotation);
            }
            if tool == Tool::Rotate {
                let delta = DQuat::from_array(after.transform.rotation)
                    * DQuat::from_array(before.transform.rotation).inverse();
                assert!((delta * DVec3::X).abs_diff_eq(DVec3::Y, 1e-5));
                assert_eq!(after.transform.scale, before.transform.scale);
            }
        }
        let moved = editor.document.clone();
        editor.finish_gesture().unwrap();
        assert_eq!(editor.history.undo_len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.document, original);
        assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
        assert_eq!(editor.selected_object, Some(2));
        assert!(editor.redo());
        assert_eq!(editor.document, moved);
    }
}

#[test]
fn object_select_all_and_cycle_use_layer_order_without_history() {
    let mut document = pair().document;
    document.insert_primitive(PrimitiveKind::Cone).unwrap();
    document.objects.rotate_left(1); // Layer order 2, 3, 1 differs from ID order.
    let mut editor = Editor::new(document).unwrap();
    let original = editor.document.clone();
    let camera = front();
    editor.select_all(viewport(), &camera, false).unwrap();
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2, 3]));
    assert_eq!(editor.selected_object, Some(2));
    editor.select_object(3).unwrap();
    editor.select_all(viewport(), &camera, false).unwrap();
    assert_eq!(
        editor.selected_object,
        Some(3),
        "Select all preserves a valid active object"
    );
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_objects, BTreeSet::from([1]));
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_object, Some(2));
    editor
        .cycle_selection(true, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_object, Some(1));
    editor.deselect();
    editor
        .cycle_selection(true, viewport(), &camera, false)
        .unwrap();
    assert_eq!(
        editor.selected_object,
        Some(1),
        "Reverse from none starts at the last layer"
    );
    editor.deselect();
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_object, Some(2));
    assert_eq!(editor.document, original);
    assert_eq!(editor.revision, 0);
    assert_eq!(editor.history.undo_len(), 0);
    assert_eq!(editor.history.redo_len(), 0);
}

#[test]
fn empty_selection_commands_are_valid_noops() {
    let mut editor = Editor::new(Document::default()).unwrap();
    let camera = front();
    editor.select_all(Rect::NOTHING, &camera, false).unwrap();
    for reverse in [false, true] {
        editor
            .cycle_selection(reverse, Rect::NOTHING, &camera, false)
            .unwrap();
    }
    assert!(!editor.delete_selection().unwrap());
    assert!(editor.selected_objects.is_empty());
    assert_eq!(editor.selected_object, None);
    assert_eq!(editor.revision, 0);
    assert_eq!(editor.history.undo_len(), 0);
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    let revision = editor.revision;
    let history = editor.history.undo_len();
    assert!(!editor.delete_selection().unwrap());
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), history);
    assert!(editor.edit_mode);
}

#[test]
fn deleting_multiple_objects_is_one_undo_step_with_full_selection_restore() {
    let mut document = pair().document;
    let survivor = document.insert_primitive(PrimitiveKind::Cone).unwrap();
    let mut editor = Editor::new(document).unwrap();
    editor.select_object(1).unwrap();
    editor.select_object_with_modifier(2, true).unwrap();
    let original = editor.document.clone();
    assert!(editor.delete_selection().unwrap());
    assert_eq!(editor.document.objects.len(), 1);
    assert_eq!(
        editor.document.objects[0],
        original
            .objects
            .iter()
            .find(|object| object.id == survivor)
            .unwrap()
            .clone()
    );
    assert!(editor.selected_objects.is_empty());
    assert_eq!(editor.selected_object, None);
    assert_eq!(editor.history.undo_len(), 1);
    let revision = editor.revision;
    assert!(!editor.delete_selection().unwrap());
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, original);
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    assert_eq!(editor.selected_object, Some(2));
    assert!(editor.redo());
    assert_eq!(editor.document.objects.len(), 1);
    assert_eq!(editor.document.objects[0].id, survivor);
    assert!(editor.selected_objects.is_empty());
}

#[test]
fn vertex_delete_removes_incident_imported_polygons_without_cleanup_or_renumbering() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/obj/cube-quads.obj");
    let document = crate::asset_io::document::load_path(&path).unwrap();
    let mut editor = Editor::new(document).unwrap();
    let id = editor.document.objects[0].id;
    editor.select_object(id).unwrap();
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 6]);
    let original = editor.document.clone();
    let before = original.eval_object(id).unwrap();
    assert!(editor.delete_selection().unwrap());
    let after = editor.document.eval_object(id).unwrap();
    assert_eq!(
        after.vertices,
        before
            .vertices
            .iter()
            .filter(|vertex| ![1, 6].contains(&vertex.id))
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        after.faces,
        before
            .faces
            .iter()
            .filter(|face| !face.vertices.iter().any(|id| [1, 6].contains(id)))
            .cloned()
            .collect::<Vec<_>>()
    );
    assert!(
        after.faces.iter().all(|face| face.vertices.len() == 4),
        "Surviving authored polygons are not triangulated or dissolved"
    );
    assert!(
        after.vertices.iter().any(|vertex| !after
            .faces
            .iter()
            .any(|face| face.vertices.contains(&vertex.id))),
        "Unselected loose vertices remain"
    );
    assert!(editor.edit_mode);
    assert_eq!(editor.selected_objects, BTreeSet::from([id]));
    assert!(editor.selected_vertices.is_empty());
    assert_eq!(editor.history.undo_len(), 1);
    let deleted = editor.document.clone();
    editor.undo();
    assert_eq!(editor.document, original);
    assert_eq!(editor.selected_vertices, BTreeSet::from([1, 6]));
    assert!(editor.edit_mode);
    editor.redo();
    assert_eq!(editor.document, deleted);
    assert!(editor.selected_vertices.is_empty());
    editor.selected_vertices = after.vertices.iter().map(|vertex| vertex.id).collect();
    editor.delete_selection().unwrap();
    let empty = editor.document.eval_object(id).unwrap();
    assert!(empty.vertices.is_empty() && empty.faces.is_empty());
    assert_eq!(editor.document.objects.len(), 1);
    assert!(
        editor.edit_mode,
        "Deleting every vertex keeps the empty object in edit mode"
    );
    assert_eq!(editor.selected_object, Some(id));
    editor.document.validate().unwrap();
    editor.undo();
    assert_eq!(editor.document, deleted);
}

#[test]
fn vertex_all_and_cycle_follow_visible_source_order_not_sorted_ids() {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.convert_object(id).unwrap();
    let Geometry::Mesh(mesh) = &mut document.objects[0].geometry else {
        unreachable!()
    };
    let order = [7, 5, 8, 6, 1, 2, 3, 4];
    mesh.vertices
        .sort_by_key(|vertex| order.iter().position(|id| *id == vertex.id).unwrap());
    let mut editor = Editor::new(document).unwrap();
    editor.select_object(id).unwrap();
    editor.enter_edit().unwrap();
    let original = editor.document.clone();
    let camera = front();
    editor.select_all(viewport(), &camera, false).unwrap();
    assert_eq!(
        editor.selected_vertices,
        BTreeSet::from([5, 6, 7, 8]),
        "Select all must not select the rear vertices"
    );
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(
        editor.selected_vertices,
        BTreeSet::from([7]),
        "Forward from multiple advances after last visible source vertex, then wraps"
    );
    editor
        .cycle_selection(true, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_vertices, BTreeSet::from([6]));
    editor.selected_vertices.clear();
    editor
        .cycle_selection(true, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_vertices, BTreeSet::from([6]));
    editor.selected_vertices = BTreeSet::from([5, 8]);
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_vertices, BTreeSet::from([6]));
    editor.selected_vertices = BTreeSet::from([5, 8]);
    editor
        .cycle_selection(true, viewport(), &camera, false)
        .unwrap();
    assert_eq!(editor.selected_vertices, BTreeSet::from([7]));
    editor.selected_vertices = BTreeSet::from([1]);
    editor
        .cycle_selection(false, viewport(), &camera, false)
        .unwrap();
    assert_eq!(
        editor.selected_vertices,
        BTreeSet::from([7]),
        "A hidden selected vertex is not a cycle target or anchor"
    );
    assert_eq!(editor.document, original);
    assert_eq!(editor.revision, 0);
    assert_eq!(editor.history.undo_len(), 0);
    assert!(editor.edit_mode);
}

#[test]
fn selection_commands_cancel_preview_before_applying_and_suppress_its_release() {
    for operation in 0..3 {
        let mut editor = cube();
        let original = editor.document.clone();
        let camera = front();
        let projection = Projection::new(viewport(), &camera, false).unwrap();
        editor.prepare(&projection).unwrap();
        let handle = editor
            .handles(&projection)
            .into_iter()
            .find(|handle| handle.kind == HandleKind::Axis(0))
            .unwrap();
        let start = handle.target.center();
        editor.begin_transform(&projection, &handle, start).unwrap();
        editor
            .preview_transform(start + egui::vec2(100., 0.))
            .unwrap();
        assert_ne!(editor.document, original);
        match operation {
            0 => editor.select_all(viewport(), &camera, false).unwrap(),
            1 => editor
                .cycle_selection(false, viewport(), &camera, false)
                .unwrap(),
            _ => {
                editor.delete_selection().unwrap();
            }
        }
        assert!(!editor.is_interacting());
        assert!(editor.suppress_release);
        if operation == 2 {
            assert!(editor.document.objects.is_empty());
            assert_eq!(editor.history.undo_len(), 1);
            editor.undo();
        } else {
            assert_eq!(editor.history.undo_len(), 0);
        }
        assert_eq!(
            editor.document, original,
            "A command must never publish the cancelled preview"
        );
    }
}

#[test]
fn atomic_changes_and_conversion_undo_preserve_source_and_display_frame() {
    let mut editor = cube();
    let before = editor.document.clone();
    let fixed_frame = editor.frame;
    assert!(editor.enter_edit().unwrap());
    assert_eq!(editor.document, before);
    editor.leave_edit();
    assert!(
        editor
            .commit("Invalid", |document| {
                document.objects[0].name = "Must not publish".into();
                document.objects[0].transform.scale[0] = 0.0;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(editor.document, before);
    assert_eq!(editor.revision, 0);
    assert!(!editor.undo());
    assert!(!editor.commit("No change", |_| Ok(())).unwrap());
    assert!(editor.convert_selected().unwrap());
    let converted = editor.document.clone();
    editor.enter_edit().unwrap();
    assert_eq!(editor.frame, fixed_frame);
    assert!(editor.undo());
    assert_eq!(editor.document, before);
    assert!(!editor.edit_mode);
    assert!(editor.redo());
    assert_eq!(editor.document, converted);
    assert_eq!(editor.frame, fixed_frame);
}

#[test]
fn repeated_escape_unwinds_selection_without_touching_document_history() {
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    let object = editor.selected_object.unwrap();
    editor.selected_vertices = editor
        .document
        .eval_object(object)
        .unwrap()
        .vertices
        .iter()
        .take(2)
        .map(|vertex| vertex.id)
        .collect();
    editor.tool = Tool::Rotate;
    let document = editor.document.clone();
    let display_frame = editor.frame;
    let revision = editor.revision;
    let history = (editor.history.undo_len(), editor.history.redo_len());

    assert_eq!(editor.escape(), EscapeOutcome::VerticesDeselected);
    assert!(editor.edit_mode);
    assert!(editor.selected_vertices.is_empty());
    assert_eq!(editor.selected_object, Some(object));
    assert_eq!(editor.escape(), EscapeOutcome::EditModeLeft);
    assert!(!editor.edit_mode);
    assert_eq!(editor.selected_object, Some(object));
    assert_eq!(editor.escape(), EscapeOutcome::ObjectDeselected);
    assert_eq!(editor.selected_object, None);
    for _ in 0..3 {
        assert_eq!(editor.escape(), EscapeOutcome::NothingToDo);
    }
    assert_eq!(editor.document, document);
    assert_eq!(editor.frame, display_frame);
    assert_eq!(editor.revision, revision);
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        history
    );
    assert_eq!(editor.tool, Tool::Rotate);
    assert!(editor.undo(), "the original conversion remains undoable");
    assert!(matches!(
        editor.document.objects[0].geometry,
        Geometry::Primitive(_)
    ));
}

#[test]
fn escape_rolls_back_transform_before_clearing_component_selection() {
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    let object = editor.selected_object.unwrap();
    editor.selected_vertices = editor
        .document
        .eval_object(object)
        .unwrap()
        .vertices
        .iter()
        .map(|vertex| vertex.id)
        .collect();
    let selection = editor.selected_vertices.clone();
    let original = editor.document.clone();
    let history = (editor.history.undo_len(), editor.history.redo_len());
    let projection = Projection::new(viewport(), &front(), false).unwrap();
    editor.prepare(&projection).unwrap();
    let handle = editor
        .handles(&projection)
        .into_iter()
        .find(|handle| handle.kind == HandleKind::Axis(0))
        .unwrap();
    let start = handle.target.center();
    editor.begin_transform(&projection, &handle, start).unwrap();
    editor
        .preview_transform(start + egui::vec2(100.0, 0.0))
        .unwrap();
    assert_ne!(editor.document, original);
    assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
    assert_eq!(editor.document, original);
    assert_eq!(editor.selected_vertices, selection);
    assert_eq!(editor.selected_object, Some(object));
    assert!(editor.edit_mode);
    assert!(!editor.is_interacting());
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        history
    );
    let revision_after_cancel = editor.revision;
    assert_eq!(editor.escape(), EscapeOutcome::VerticesDeselected);
    assert!(editor.edit_mode);
    assert_eq!(editor.document, original);
    assert_eq!(editor.revision, revision_after_cancel);

    editor.selected_vertices = selection.clone();
    editor.gesture = Some(Gesture::Marquee {
        start: egui::pos2(10.0, 10.0),
        current: egui::pos2(30.0, 30.0),
        additive: false,
        before: selection.clone(),
        pending: BTreeSet::new(),
        policy: BoxSelectionPolicy::default(),
        dragged: true,
    });
    editor.selected_vertices.clear();
    assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
    assert_eq!(editor.selected_vertices, selection);
    assert!(editor.edit_mode);
    assert_eq!(editor.revision, revision_after_cancel);
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        history
    );
}

#[test]
fn idle_selection_and_explicit_deselect_are_valid_without_history() {
    for document in [Document::default(), cube().document] {
        let mut editor = Editor::new(document.clone()).unwrap();
        assert_eq!(editor.selected_object, None);
        assert_eq!(editor.escape(), EscapeOutcome::NothingToDo);
        if let Some(object) = editor.document.objects.first() {
            editor.select_object(object.id).unwrap();
        }
        editor.deselect();
        editor.deselect();
        assert_eq!(editor.selected_object, None);
        assert!(editor.selected_vertices.is_empty());
        assert!(!editor.edit_mode);
        assert_eq!(editor.escape(), EscapeOutcome::NothingToDo);
        assert_eq!(editor.document, document);
        assert_eq!(editor.revision, 0);
        assert!(!editor.undo());
        assert!(!editor.redo());
    }
}

#[test]
fn idle_confirm_toggles_edit_mode_without_document_history() {
    let mut editor = cube();
    let primitive = editor.document.clone();
    assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::EditModeEntered);
    assert!(editor.edit_mode);
    assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::EditModeLeft);
    assert_eq!(editor.document, primitive);
    assert!(!editor.edit_mode);
    assert_eq!(editor.history.undo_len(), 0);
    editor.convert_selected().unwrap();
    let selected = editor.selected_object;
    let document = editor.document.clone();
    let history = (editor.history.undo_len(), editor.history.redo_len());
    let revision = editor.revision;
    for _ in 0..3 {
        assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::EditModeEntered);
        assert!(editor.edit_mode);
        assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::EditModeLeft);
        assert!(!editor.edit_mode);
        assert_eq!(editor.selected_object, selected);
    }
    editor.deselect();
    assert_eq!(editor.confirm().unwrap(), ConfirmOutcome::NothingToDo);
    assert_eq!(editor.document, document);
    assert_eq!(editor.revision, revision);
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        history
    );
}

#[test]
fn confirm_finishes_preview_once_while_escape_rolls_it_back() {
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    editor.selected_vertices = editor
        .document
        .eval_object(editor.selected_object.unwrap())
        .unwrap()
        .vertices
        .iter()
        .map(|vertex| vertex.id)
        .collect();
    let original = editor.document.clone();
    let initial_history = editor.history.undo_len();
    let projection = Projection::new(viewport(), &front(), false).unwrap();
    editor.prepare(&projection).unwrap();
    let handle = editor
        .handles(&projection)
        .into_iter()
        .find(|handle| handle.kind == HandleKind::Axis(0))
        .unwrap();
    let start = handle.target.center();
    editor.begin_transform(&projection, &handle, start).unwrap();
    editor
        .preview_transform(start + egui::vec2(100.0, 0.0))
        .unwrap();
    let preview = editor.document.clone();
    assert_ne!(preview, original);
    assert_eq!(
        editor.confirm().unwrap(),
        ConfirmOutcome::InteractionFinished
    );
    assert_eq!(editor.document, preview);
    assert!(editor.edit_mode);
    assert!(!editor.is_interacting());
    assert_eq!(editor.history.undo_len(), initial_history + 1);
    assert!(
        !editor.finish_gesture().unwrap(),
        "release cannot commit twice"
    );
    assert_eq!(editor.history.undo_len(), initial_history + 1);
    assert!(editor.undo());
    assert_eq!(editor.document, original);

    editor.prepare(&projection).unwrap();
    editor.begin_transform(&projection, &handle, start).unwrap();
    editor
        .preview_transform(start + egui::vec2(100.0, 0.0))
        .unwrap();
    assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
    assert_eq!(editor.document, original);
    assert_eq!(editor.history.undo_len(), initial_history);
    assert!(
        editor.redo(),
        "cancelling the second preview preserves redo"
    );
    assert_eq!(editor.document, preview);
}

#[test]
fn externally_finished_handle_press_does_not_select_on_release() {
    for confirm in [true, false] {
        let ctx = egui::Context::default();
        let camera = front();
        let mut editor = cube();
        editor.convert_selected().unwrap();
        editor.enter_edit().unwrap();
        editor.selected_vertices = editor
            .document
            .eval_object(editor.selected_object.unwrap())
            .unwrap()
            .vertices
            .iter()
            .map(|vertex| vertex.id)
            .collect();
        let selected = editor.selected_vertices.clone();
        let history = editor.history.undo_len();
        frame(&ctx, &mut editor, &camera, vec![], true);
        let start = editor
            .handle_rects(viewport(), &camera, false)
            .unwrap()
            .into_iter()
            .find(|(kind, _)| *kind == HandleKind::Axis(0))
            .unwrap()
            .1
            .center();
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![Event::PointerMoved(start), pointer(start, true)],
            true,
        );
        assert!(editor.is_transforming());
        if confirm {
            assert_eq!(
                editor.confirm().unwrap(),
                ConfirmOutcome::InteractionFinished
            );
        } else {
            assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
        }
        ctx.stop_dragging();
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![pointer(start, false)],
            true,
        );
        assert!(editor.edit_mode);
        assert_eq!(editor.selected_vertices, selected);
        assert_eq!(editor.history.undo_len(), history);
    }
}

fn double_click(ctx: &egui::Context, editor: &mut Editor, camera: &Camera, position: Pos2) {
    for _ in 0..2 {
        assert!(
            frame(
                ctx,
                editor,
                camera,
                vec![Event::PointerMoved(position), pointer(position, true)],
                true,
            )
            .is_none()
        );
        assert!(frame(ctx, editor, camera, vec![pointer(position, false)], true).is_none());
    }
}

#[test]
fn double_click_toggles_only_mesh_entry_and_geometric_background_exit() {
    let camera = front();
    let mut document = cube().document;
    let selected = document.objects[0].id;
    document.convert_object(selected).unwrap();
    let other = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document
        .objects
        .iter_mut()
        .find(|object| object.id == other)
        .unwrap()
        .transform
        .translation[0] = 3.0;

    // A face interior and a different object are both occupied geometry,
    // even when no selectable vertex is anywhere near the pointer.
    for source_position in [Some(DVec3::ZERO), Some(DVec3::new(3.0, 0.0, 0.0)), None] {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(document.clone()).unwrap();
        editor.select_object(selected).unwrap();
        editor.enter_edit().unwrap();
        editor.tool = Tool::View;
        frame(&ctx, &mut editor, &camera, vec![], true);
        let projection = Projection::new(viewport(), &camera, false).unwrap();
        let position = source_position
            .map(|point| {
                projection
                    .screen(editor.frame.world_to_display(point))
                    .unwrap()
            })
            .unwrap_or(egui::pos2(30.0, 540.0));
        double_click(&ctx, &mut editor, &camera, position);
        assert_eq!(editor.edit_mode, source_position.is_some());
        assert_eq!(editor.selected_object, Some(selected));
        assert_eq!(editor.document, document);
        assert_eq!(editor.revision, 0);
        assert_eq!(editor.history.undo_len(), 0);
    }

    for tool in [Tool::View, Tool::Move] {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(document.clone()).unwrap();
        editor.tool = tool;
        frame(&ctx, &mut editor, &camera, vec![], true);
        let projection = Projection::new(viewport(), &camera, false).unwrap();
        let position = projection
            .screen(editor.frame.world_to_display(DVec3::new(-0.35, 0.35, 0.0)))
            .unwrap();
        double_click(&ctx, &mut editor, &camera, position);
        assert!(editor.edit_mode, "entry must work with {tool:?}");
        assert_eq!(editor.selected_object, Some(selected));
        assert_eq!(editor.document, document);
        assert_eq!(editor.history.undo_len(), 0);
    }
}

#[test]
fn visible_selection_rejects_rear_vertices_and_occluded_objects() {
    let mut editor = cube();
    let id = editor.selected_object.unwrap();
    let camera = front();
    let visible = editor
        .selectable_vertices(viewport(), &camera, false)
        .unwrap();
    assert_eq!(
        visible.len(),
        4,
        "a front view exposes only the front four cube corners"
    );
    let source = editor.document.eval_object(id).unwrap();
    for (id, _) in visible {
        assert!(
            source
                .vertices
                .iter()
                .find(|vertex| vertex.id == id)
                .unwrap()
                .position[2]
                > 0.0
        );
    }
    let behind = editor.insert(PrimitiveKind::Cube).unwrap();
    editor
        .commit("Move behind", |document| {
            document
                .objects
                .iter_mut()
                .find(|object| object.id == behind)
                .unwrap()
                .transform
                .translation[2] = -3.0;
            Ok(())
        })
        .unwrap();
    assert!(
        editor
            .selectable_vertices(viewport(), &camera, false)
            .unwrap()
            .is_empty()
    );
    let projection = Projection::new(viewport(), &camera, false).unwrap();
    let hit = editor
        .cache
        .as_ref()
        .unwrap()
        .bvh
        .hit(projection.ray(viewport().center()).unwrap())
        .unwrap();
    assert_eq!(hit.object, id);
}

#[test]
fn silhouette_occlusion_survives_axis_mapping_and_viewport_offsets() {
    let mut editor = cube();
    let source = editor
        .document
        .eval_object(editor.selected_object.unwrap())
        .unwrap();
    for viewport in [
        viewport(),
        Rect::from_min_size(egui::pos2(137.0, 51.0), egui::vec2(600.0, 800.0)),
    ] {
        for z_up in [false, true] {
            for direction in [
                glam::Vec3::X,
                -glam::Vec3::X,
                glam::Vec3::Y,
                -glam::Vec3::Y,
                glam::Vec3::Z,
                -glam::Vec3::Z,
            ] {
                let mut camera = Camera::default();
                camera.frame(viewport.aspect_ratio());
                camera.look_from(display_rotation(z_up).transform_vector3(direction));
                let visible = editor.selectable_vertices(viewport, &camera, z_up).unwrap();
                assert_eq!(
                    visible.len(),
                    4,
                    "direction={direction:?}, z_up={z_up}, viewport={viewport:?}"
                );
                for (id, _) in visible {
                    let position = source
                        .vertices
                        .iter()
                        .find(|vertex| vertex.id == id)
                        .unwrap()
                        .position;
                    assert!(DVec3::from_array(position).dot(direction.as_dvec3()) > 0.0);
                }
            }
        }
    }
}

#[test]
fn explicit_axis_drag_is_one_transaction_and_focus_loss_cancels() {
    let ctx = egui::Context::default();
    let camera = front();
    let mut editor = cube();
    let original = editor.document.clone();
    let display_frame = editor.frame;
    assert!(frame(&ctx, &mut editor, &camera, vec![], true).is_none());
    let start = editor
        .handle_rects(viewport(), &camera, false)
        .unwrap()
        .into_iter()
        .find(|(kind, _)| *kind == HandleKind::Axis(0))
        .unwrap()
        .1
        .center();
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start), pointer(start, true)],
        true,
    );
    for dx in [10.0, 50.0, 100.0] {
        assert!(
            frame(
                &ctx,
                &mut editor,
                &camera,
                vec![Event::PointerMoved(start + egui::vec2(dx, 0.0))],
                true
            )
            .is_none()
        );
    }
    assert!(editor.is_transforming());
    assert_ne!(editor.document, original);
    assert_eq!(editor.history.undo_len(), 0);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![pointer(start + egui::vec2(100.0, 0.0), false)],
        true,
    );
    assert_eq!(editor.history.undo_len(), 1);
    assert_eq!(editor.frame, display_frame);
    assert_eq!(
        editor.document.objects[0].transform.translation[1..],
        [0.0, 0.0]
    );
    assert!(editor.undo());
    assert_eq!(editor.document, original);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start), pointer(start, true)],
        true,
    );
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start + egui::vec2(100.0, 0.0))],
        true,
    );
    assert_ne!(editor.document, original);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::WindowFocused(false)],
        false,
    );
    assert_eq!(editor.document, original);
    assert!(!editor.is_interacting());
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn edit_marquee_is_visible_only_even_with_option_and_adds_with_shift() {
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    let camera = front();
    let projection = Projection::new(viewport(), &camera, false).unwrap();
    let visible = editor
        .selectable_vertices(viewport(), &camera, false)
        .unwrap();
    let expected: BTreeSet<_> = visible.iter().map(|(id, _)| *id).collect();
    let ctx = egui::Context::default();
    frame(&ctx, &mut editor, &camera, vec![], true);
    let start = egui::pos2(180.0, 100.0);
    let end = egui::pos2(610.0, 520.0);
    let press = Event::PointerButton {
        pos: start,
        button: PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::ALT,
    };
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start), press],
        true,
    );
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(end)],
        true,
    );
    frame(&ctx, &mut editor, &camera, vec![pointer(end, false)], true);
    assert_eq!(editor.selected_vertices, expected);
    let source = editor.document.clone();
    editor.prepare(&projection).unwrap();
    let (id, position) = visible[0];
    editor.select_vertex(position, true);
    assert!(!editor.selected_vertices.contains(&id));
    editor.select_vertex(position, true);
    assert_eq!(editor.selected_vertices, expected);
    assert_eq!(
        editor.document, source,
        "selection never manipulates geometry"
    );
}

#[test]
fn rotate_and_scale_constraints_change_real_object_transforms_without_noop_drift() {
    let mut editor = cube();
    let camera = front();
    let projection = Projection::new(viewport(), &camera, false).unwrap();
    editor.tool = Tool::Rotate;
    editor.prepare(&projection).unwrap();
    let handle = editor
        .handles(&projection)
        .into_iter()
        .find(|handle| handle.kind == HandleKind::Axis(2))
        .unwrap();
    let start = projection
        .screen(handle.pivot + DVec3::X * handle.length)
        .unwrap();
    let end = projection
        .screen(handle.pivot + DVec3::Y * handle.length)
        .unwrap();
    let original = editor.document.clone();
    editor.begin_transform(&projection, &handle, start).unwrap();
    editor.preview_transform(start).unwrap();
    assert_eq!(editor.document, original);
    editor.preview_transform(end).unwrap();
    let rotation = DQuat::from_array(editor.document.objects[0].transform.rotation);
    assert!((rotation * DVec3::X).abs_diff_eq(DVec3::Y, 1e-5));
    editor.cancel();
    assert_eq!(editor.document, original);
    editor.tool = Tool::Scale;
    editor.prepare(&projection).unwrap();
    let handle = editor
        .handles(&projection)
        .into_iter()
        .find(|handle| handle.kind == HandleKind::Axis(0))
        .unwrap();
    let start = handle.target.center();
    editor.begin_transform(&projection, &handle, start).unwrap();
    editor
        .preview_transform(start + egui::vec2(39.0, 0.0))
        .unwrap();
    assert!(
        (editor.document.objects[0].transform.scale[0] - 1.5).abs() < 1e-5,
        "actual scale: {:?}",
        editor.document.objects[0].transform.scale
    );
    assert_eq!(editor.document.objects[0].transform.scale[1..], [1.0, 1.0]);
    editor.preview_transform(start).unwrap();
    assert_eq!(editor.document, original);
    editor.cancel();
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn invalid_vertex_drag_rolls_back_the_whole_transaction() {
    let document = Document {
        objects: vec![Object {
            id: 1,
            name: "Quad".into(),
            transform: Transform::default(),
            geometry: Geometry::Mesh(EditableMesh {
                vertices: vec![
                    MeshVertex {
                        id: 1,
                        position: [-1.0, -1.0, 0.0],
                    },
                    MeshVertex {
                        id: 2,
                        position: [1.0, -1.0, 0.0],
                    },
                    MeshVertex {
                        id: 3,
                        position: [1.0, 1.0, 0.0],
                    },
                    MeshVertex {
                        id: 4,
                        position: [-1.0, 1.0, 0.0],
                    },
                ],
                edges: vec![],
                faces: vec![Face {
                    id: 1,
                    vertices: vec![1, 2, 3, 4],
                }],
            }),
        }],
        ..Document::default()
    };
    let mut editor = Editor::new(document.clone()).unwrap();
    editor.set_tool(Tool::Move);
    editor.select_object(1).unwrap();
    editor.enter_edit().unwrap();
    editor.selected_vertices.insert(1);
    let camera = front();
    let projection = Projection::new(viewport(), &camera, false).unwrap();
    editor.prepare(&projection).unwrap();
    let handle = editor
        .handles(&projection)
        .into_iter()
        .find(|handle| handle.kind == HandleKind::Axis(0))
        .unwrap();
    let start = handle.target.center();
    let displacement = projection.screen(handle.pivot + DVec3::X * 3.0).unwrap()
        - projection.screen(handle.pivot).unwrap();
    let ctx = egui::Context::default();
    frame(&ctx, &mut editor, &camera, vec![], true);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start), pointer(start, true)],
        true,
    );
    let error = frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start + displacement)],
        true,
    );
    assert!(error.is_some());
    assert_eq!(editor.document, document);
    assert!(!editor.is_interacting());
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn degenerate_constraints_do_not_infer_another_axis() {
    let ray = Ray {
        origin: DVec3::Z * 4.0,
        direction: -DVec3::Z,
    };
    assert!(constraint_point(ray, DVec3::ZERO, DVec3::Z, None).is_none());
    assert!(constraint_point(ray, DVec3::ZERO, DVec3::X, Some(DVec3::X)).is_none());
    assert_eq!(
        constraint_point(ray, DVec3::ZERO, DVec3::X, Some(DVec3::Z)),
        Some(DVec3::ZERO)
    );
}

#[test]
fn selection_points_match_rendered_transforms_fixed_frame_and_source_up_axis() {
    let mut editor = pair();
    let second = editor.document.objects[1].id;
    editor.select_object(1).unwrap();
    editor.select_object_with_modifier(second, true).unwrap();
    editor.document.objects[0].transform.translation = [80.0, 12.0, -50.0];
    editor.document.objects[0].transform.rotation = DQuat::from_rotation_y(0.73).to_array();
    editor.document.objects[0].transform.scale = [1.5, 0.7, 2.0];
    let mut torus = Document::default();
    let torus_id = torus.insert_primitive(PrimitiveKind::Torus).unwrap();
    editor.document.objects[1].geometry = torus.objects[0].geometry.clone();
    let expected_count = editor.document.eval_object(1).unwrap().vertices.len()
        + torus.eval_object(torus_id).unwrap().vertices.len();
    let original = editor.document.clone();
    let original_frame = editor.frame;
    let revision = editor.revision;
    let rendered = editor.document.render_mesh(&editor.frame).unwrap();
    for z_up in [false, true] {
        let points = editor.selection_points(z_up).unwrap();
        assert_eq!(points.len(), expected_count);
        assert!(points.iter().any(|point| point.abs().max_element() > 1.0));
        for vertex in &rendered.vertices {
            let display =
                display_rotation(z_up).transform_point3(Vec3::from_array(vertex.position));
            assert!(
                points.contains(&display),
                "Fit must use the same f64 transform/normalization and f32 up-axis mapping as rendering"
            );
        }
    }
    assert_eq!(editor.document, original);
    assert_eq!(editor.frame, original_frame);
    assert_eq!(editor.revision, revision);
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        (0, 0)
    );
    assert!(editor.cache.is_none());
}

#[test]
fn edit_selection_points_include_selected_occluded_vertices_only() {
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    let mesh = editor.document.eval_object(1).unwrap();
    let visible: BTreeSet<_> = editor
        .selectable_vertices(viewport(), &front(), false)
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let hidden = mesh
        .vertices
        .iter()
        .find(|vertex| !visible.contains(&vertex.id))
        .unwrap();
    editor.selected_vertices.insert(hidden.id);
    let points = editor.selection_points(false).unwrap();
    assert_eq!(
        points,
        vec![
            editor
                .frame
                .world_to_display(DVec3::from_array(hidden.position))
                .as_vec3()
        ]
    );
    assert_eq!(editor.selected_vertices, BTreeSet::from([hidden.id]));
    editor.selected_vertices.clear();
    assert!(editor.selection_points(false).unwrap().is_empty());
    editor.leave_edit();
    editor.deselect();
    assert!(editor.selection_points(false).unwrap().is_empty());
}

#[test]
fn selection_points_read_transform_preview_without_cancelling_or_creating_history() {
    let mut editor = cube();
    let projection = Projection::new(viewport(), &front(), false).unwrap();
    editor.prepare(&projection).unwrap();
    let handle = editor
        .handles(&projection)
        .into_iter()
        .find(|handle| handle.kind == HandleKind::Axis(0))
        .unwrap();
    let start = handle.target.center();
    editor.begin_transform(&projection, &handle, start).unwrap();
    editor
        .preview_transform(start + egui::vec2(100.0, 0.0))
        .unwrap();
    let preview = editor.document.clone();
    let revision = editor.revision;
    let points = editor.selection_points(false).unwrap();
    assert_eq!(points.len(), 8);
    assert!(editor.is_interacting());
    assert_eq!(editor.document, preview);
    assert_eq!(editor.revision, revision);
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        (0, 0)
    );
    editor.cancel();
    assert_ne!(editor.selection_points(false).unwrap(), points);
}

#[test]
fn locked_nudges_preview_every_selected_object_until_one_confirmed_undo() {
    let mut editor = pair();
    editor.select_object(1).unwrap();
    editor.select_object_with_modifier(2, true).unwrap();
    editor.document.objects[0].transform.rotation = DQuat::from_rotation_y(0.8).to_array();
    editor.document.objects[0].transform.scale = [2.0, 0.5, 1.5];
    let original = editor.document.clone();
    editor.toggle_transform_axis(0).unwrap();
    assert!(editor.nudge(DVec3::X, false).unwrap());
    assert!(editor.nudge(DVec3::X, true).unwrap());
    assert!(editor.nudge(DVec3::X * 10.0, true).unwrap());
    assert!(editor.has_transform_session());
    assert!(!editor.is_pointer_interacting());
    assert_eq!(editor.history.undo_len(), 0);
    for (before, after) in original.objects.iter().zip(&editor.document.objects) {
        assert_eq!(
            after.transform.translation[0],
            before.transform.translation[0] + 12.0
        );
        assert_eq!(
            &after.transform.translation[1..],
            &before.transform.translation[1..]
        );
        assert_eq!(after.transform.rotation, before.transform.rotation);
        assert_eq!(after.transform.scale, before.transform.scale);
        assert_eq!(after.geometry, before.geometry);
    }
    let moved = editor.document.clone();
    assert_eq!(
        editor.confirm().unwrap(),
        ConfirmOutcome::InteractionFinished
    );
    assert_eq!(editor.history.undo_len(), 1);
    assert!(!editor.has_transform_session());
    assert_eq!(editor.transform_axis, None);
    assert!(editor.undo());
    assert_eq!(editor.document, original);
    assert!(editor.redo());
    assert_eq!(editor.document, moved);
    assert!(editor.nudge(DVec3::X, false).unwrap());
    assert_eq!(
        editor.history.undo_len(),
        2,
        "A new key press starts a separate entry"
    );
    assert!(editor.undo());
    assert_eq!(editor.document, moved);
}

#[test]
fn world_nudges_inverse_transform_selected_vertices_without_touching_other_data() {
    let transform = Transform {
        translation: [30.0, -20.0, 12.0],
        rotation: DQuat::from_euler(glam::EulerRot::XYZ, 0.2, 0.7, -0.4).to_array(),
        scale: [2.0, 0.5, 3.0],
    };
    let document = Document {
        objects: vec![Object {
            id: 1,
            name: "Editable points".into(),
            transform: transform.clone(),
            geometry: Geometry::Mesh(EditableMesh {
                vertices: vec![
                    MeshVertex {
                        id: 7,
                        position: [1.0, 2.0, -3.0],
                    },
                    MeshVertex {
                        id: 19,
                        position: [-2.0, 1.0, 4.0],
                    },
                ],
                edges: vec![],
                faces: vec![],
            }),
        }],
        ..Document::default()
    };
    let mut editor = Editor::new(document.clone()).unwrap();
    editor.set_tool(Tool::Move);
    // This test isolates exact inverse-transform math; grid behavior has its own tests.
    editor.snapping.enabled = false;
    editor.select_object(1).unwrap();
    editor.enter_edit().unwrap();
    editor.selected_vertices.insert(7);
    editor.toggle_transform_axis(2).unwrap();
    let before = editor.document.eval_object(1).unwrap();
    let delta = DVec3::new(1.0, 3.0, 10.0);
    assert!(editor.nudge(DVec3::new(1.0, 0.0, 10.0), false).unwrap());
    editor.toggle_transform_axis(1).unwrap();
    assert!(editor.nudge(DVec3::Y * 3.0, false).unwrap());
    let after = editor.document.eval_object(1).unwrap();
    let matrix = transform.matrix();
    let from = matrix.transform_point3(DVec3::from_array(before.vertices[0].position));
    let to = matrix.transform_point3(DVec3::from_array(after.vertices[0].position));
    assert!((to - from).abs_diff_eq(delta, 1e-12));
    assert_eq!(after.vertices[1], before.vertices[1]);
    assert_eq!(after.faces, before.faces);
    assert_eq!(editor.document.objects[0].transform, transform);
    assert_eq!(editor.selected_vertices, BTreeSet::from([7]));
    assert_eq!(editor.history.undo_len(), 0);
    editor.confirm().unwrap();
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, document);
    assert_eq!(editor.transform_axis, None);
}

#[test]
fn nudge_noops_failures_and_history_cap_preserve_transaction_boundaries() {
    let mut editor = cube();
    let original = editor.document.clone();
    assert!(!editor.nudge(DVec3::ZERO, false).unwrap());
    assert!(editor.nudge(DVec3::NAN, false).is_err());
    assert!(editor.nudge(DVec3::splat(f64::MAX), false).is_err());
    assert_eq!(editor.document, original);
    assert_eq!(editor.history.undo_len(), 0);
    for _ in 0..HISTORY_LIMIT {
        editor.nudge(DVec3::X, false).unwrap();
    }
    editor.nudge(DVec3::X, true).unwrap();
    assert_eq!(editor.history.undo_len(), HISTORY_LIMIT);
    for _ in 0..HISTORY_LIMIT {
        assert!(editor.undo());
    }
    assert_eq!(
        editor.document, original,
        "Coalescing at the history cap must not discard the oldest entry"
    );
    editor.deselect();
    assert!(!editor.nudge(DVec3::X, false).unwrap());
}

#[test]
fn axis_lock_lifecycle_is_ephemeral_and_context_specific() {
    let mut editor = pair();
    editor.select_object(1).unwrap();
    let revision = editor.revision;
    editor.toggle_transform_axis(0).unwrap();
    assert_eq!(editor.transform_axis, Some(0));
    editor.toggle_transform_axis(1).unwrap();
    assert_eq!(editor.transform_axis, Some(1));
    editor.toggle_transform_axis(1).unwrap();
    assert_eq!(editor.transform_axis, None);
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), 0);
    editor.toggle_transform_axis(2).unwrap();
    assert!(editor.toggle_transform_axis(3).is_err());
    assert_eq!(editor.transform_axis, Some(2));
    assert_eq!(editor.escape(), EscapeOutcome::AxisUnlocked);
    assert_eq!(editor.selected_object, Some(1));
    editor.toggle_transform_axis(0).unwrap();
    editor.set_tool(Tool::Rotate);
    assert_eq!(editor.transform_axis, None);
    editor.toggle_transform_axis(0).unwrap();
    assert_eq!(editor.transform_axis, Some(0));
    editor.set_tool(Tool::View);
    assert!(editor.toggle_transform_axis(0).is_err());
    editor.set_tool(Tool::Move);
    editor.toggle_transform_axis(0).unwrap();
    editor.select_object(2).unwrap();
    assert_eq!(editor.transform_axis, None);
    editor.convert_selected().unwrap();
    editor.toggle_transform_axis(0).unwrap();
    editor.enter_edit().unwrap();
    assert_eq!(editor.transform_axis, None);
    editor.toggle_transform_axis(0).unwrap();
    editor.set_vertex_selection(BTreeSet::from([1]));
    assert_eq!(editor.transform_axis, None);
    editor.toggle_transform_axis(0).unwrap();
    editor.leave_edit();
    assert_eq!(editor.transform_axis, None);
}

#[test]
fn locked_empty_space_drag_escape_cancels_session_and_unlocks_together() {
    for axis in [0, 2] {
        let mut editor = pair();
        editor.select_object(1).unwrap();
        editor.select_object_with_modifier(2, true).unwrap();
        editor.toggle_transform_axis(axis).unwrap();
        let original = editor.document.clone();
        let camera = front();
        let projection = Projection::new(viewport(), &camera, false).unwrap();
        editor.prepare(&projection).unwrap();
        let pivot = editor.selection_pivot().unwrap();
        let step = editor.pointer_snapping(&projection, pivot).unwrap().step_cm;
        let expected = (projection.pixel_size(pivot).unwrap() * 100.0 / editor.frame.scale / step)
            .round()
            * step;
        let ctx = egui::Context::default();
        let start = egui::pos2(100.0, 500.0);
        let end = start
            + if axis == 0 {
                egui::vec2(100.0, 20.0)
            } else {
                egui::vec2(20.0, -100.0)
            };
        assert_eq!(
            editor.object_at(start, viewport(), &camera, false).unwrap(),
            None
        );
        frame(&ctx, &mut editor, &camera, vec![], true);
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![Event::PointerMoved(start), pointer(start, true)],
            true,
        );
        assert!(editor.is_transforming());
        assert!(
            frame(
                &ctx,
                &mut editor,
                &camera,
                vec![Event::PointerMoved(end)],
                true
            )
            .is_none()
        );
        for (before, after) in original.objects.iter().zip(&editor.document.objects) {
            for component in 0..3 {
                let delta = after.transform.translation[component]
                    - before.transform.translation[component];
                assert!((delta - if component == axis { expected } else { 0.0 }).abs() < 1e-8);
            }
        }
        assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
        assert_eq!(editor.history.undo_len(), 0);
        assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
        assert_eq!(editor.document, original);
        assert_eq!(editor.transform_axis, None);
        assert!(!editor.has_transform_session());
        frame(&ctx, &mut editor, &camera, vec![pointer(end, false)], true);
        assert_eq!(editor.document, original);
        assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
    }
}

#[test]
fn locked_transform_commits_once_and_parallel_fallback_respects_source_up_axis() {
    for (axis, direction, z_up) in [
        (0, Vec3::X, false),
        (1, Vec3::Y, false),
        (2, Vec3::Z, false),
        (2, Vec3::Y, true),
    ] {
        let mut editor = cube();
        editor.document.objects[0].transform.rotation = DQuat::from_rotation_z(0.5).to_array();
        editor.toggle_transform_axis(axis).unwrap();
        let mut camera = Camera::default();
        camera.look_from(direction);
        let projection = Projection::new(viewport(), &camera, z_up).unwrap();
        editor.prepare(&projection).unwrap();
        let handle = editor.locked_transform_handle(&projection).unwrap();
        assert_eq!(editor.handles(&projection).len(), 1);
        assert_eq!(handle.kind, HandleKind::Axis(axis));
        let before = editor.document.clone();
        let start = egui::pos2(150.0, 500.0);
        editor.begin_transform(&projection, &handle, start).unwrap();
        let step = editor
            .pointer_snapping(&projection, handle.pivot)
            .unwrap()
            .step_cm;
        let expected =
            (projection.pixel_size(handle.pivot).unwrap() * 100.0 / editor.frame.scale / step)
                .round()
                * step;
        editor
            .preview_transform(start + egui::vec2(70.0, -100.0))
            .unwrap();
        let after = &editor.document.objects[0].transform;
        assert!((after.translation[axis] - expected).abs() < 1e-8);
        assert_eq!(after.rotation, before.objects[0].transform.rotation);
        assert!(editor.finish_gesture().unwrap());
        assert_eq!(editor.history.undo_len(), 0);
        assert!(editor.has_transform_session());
        assert!(!editor.is_pointer_interacting());
        assert_eq!(editor.transform_axis, Some(axis));
        editor.confirm().unwrap();
        assert_eq!(editor.history.undo_len(), 1);
        assert_eq!(editor.transform_axis, None);
        assert!(editor.undo());
        assert_eq!(editor.document, before);
    }
}

#[test]
fn locked_axis_line_is_clipped_and_empty_selection_cannot_box_select() {
    let horizontal =
        clipped_axis_line(viewport(), egui::pos2(400.0, 300.0), egui::Vec2::X).unwrap();
    assert_eq!(
        horizontal,
        [egui::pos2(0.0, 300.0), egui::pos2(800.0, 300.0)]
    );
    assert!(clipped_axis_line(viewport(), egui::pos2(400.0, -10.0), egui::Vec2::X).is_none());
    let mut editor = cube();
    editor.deselect();
    editor.toggle_transform_axis(0).unwrap();
    let ctx = egui::Context::default();
    let start = egui::pos2(50.0, 50.0);
    let end = egui::pos2(650.0, 550.0);
    frame(&ctx, &mut editor, &front(), vec![], true);
    frame(
        &ctx,
        &mut editor,
        &front(),
        vec![Event::PointerMoved(start), pointer(start, true)],
        true,
    );
    frame(
        &ctx,
        &mut editor,
        &front(),
        vec![Event::PointerMoved(end)],
        true,
    );
    frame(&ctx, &mut editor, &front(), vec![pointer(end, false)], true);
    assert!(editor.selected_objects.is_empty());
    assert!(!editor.is_interacting());
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn move_session_spans_multiple_drags_nudges_and_axis_changes() {
    let mut editor = pair();
    editor.select_object(1).unwrap();
    editor.select_object_with_modifier(2, true).unwrap();
    let before = editor.snapshot();
    let fixed_frame = editor.frame;
    let camera = front();
    let projection = Projection::new(viewport(), &camera, false).unwrap();
    let start = egui::pos2(100.0, 500.0);
    for (axis, displacement) in [(0, egui::vec2(30.0, 0.0)), (1, egui::vec2(0.0, -40.0))] {
        editor.toggle_transform_axis(axis).unwrap();
        editor.prepare(&projection).unwrap();
        let handle = editor.locked_transform_handle(&projection).unwrap();
        let drag_before = editor.document.clone();
        editor.begin_transform(&projection, &handle, start).unwrap();
        editor.preview_transform(start + displacement).unwrap();
        let moved = editor.document.clone();
        assert_ne!(moved, drag_before);
        editor.preview_transform(start).unwrap();
        assert_eq!(editor.document, drag_before, "Each drag has its own origin");
        editor.preview_transform(start + displacement).unwrap();
        assert_eq!(editor.document, moved);
        editor.finish_gesture().unwrap();
        assert!(editor.has_transform_session() && editor.is_transforming());
        assert!(!editor.is_pointer_interacting());
        assert_eq!(editor.history.undo_len(), 0);
        editor.nudge([DVec3::X, DVec3::Y][axis], false).unwrap();
    }
    editor.toggle_transform_axis(1).unwrap();
    assert_eq!(editor.transform_axis, None);
    assert!(editor.has_transform_session());
    editor.nudge(DVec3::Z, false).unwrap();
    let preview = editor.document.clone();
    let a = DVec3::from_array(preview.objects[0].transform.translation)
        - DVec3::from_array(before.document.objects[0].transform.translation);
    let b = DVec3::from_array(preview.objects[1].transform.translation)
        - DVec3::from_array(before.document.objects[1].transform.translation);
    assert!(a.abs_diff_eq(b, 1e-12));
    assert!(a.x > 1.0 && a.y > 1.0 && a.z == 1.0);
    let ctx = egui::Context::default();
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start)],
        true,
    );
    assert_eq!(
        editor.document, preview,
        "Idle pointer movement cannot transform a paused session"
    );
    assert_eq!(
        editor.confirm().unwrap(),
        ConfirmOutcome::InteractionFinished
    );
    assert_eq!(editor.history.undo_len(), 1);
    assert!(!editor.is_interacting());
    assert_eq!(editor.transform_axis, None);
    assert_eq!(editor.frame, fixed_frame);
    assert!(editor.undo());
    assert!(editor.snapshot() == before);
    assert!(editor.redo());
    assert_eq!(editor.document, preview);
    assert_eq!(editor.selected_objects, BTreeSet::from([1, 2]));
}

#[test]
fn cancelled_and_noop_sessions_preserve_redo_and_invalid_confirm_rolls_back() {
    for completion in 0..4 {
        let mut editor = cube();
        let baseline = editor.snapshot();
        editor.nudge(DVec3::Y, false).unwrap();
        let redo_document = editor.document.clone();
        editor.undo();
        let redo = editor.history.redo_len();
        editor.toggle_transform_axis(0).unwrap();
        editor.nudge(DVec3::X, false).unwrap();
        assert!(
            editor.history.redo_len() == redo,
            "A preview must not erase redo"
        );
        match completion {
            0 => {
                assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
            }
            1 => {
                editor.nudge(-DVec3::X, false).unwrap();
                editor.confirm().unwrap();
            }
            2 => {
                editor.document.objects[0].transform.scale[0] = 0.0;
                assert!(editor.confirm().is_err());
            }
            _ => {
                assert!(editor.undo(), "Undo first cancels the pending transaction");
            }
        }
        assert!(editor.snapshot() == baseline);
        assert_eq!(editor.history.undo_len(), 0);
        assert!(editor.history.redo_len() == redo);
        assert_eq!(editor.transform_axis, None);
        assert!(!editor.is_interacting());
        assert!(editor.redo());
        assert_eq!(editor.document, redo_document);
    }
}

#[test]
fn history_transaction_owns_the_baseline_and_selection_alone_is_not_an_edit() {
    let mut editor = pair();
    editor.select_object(1).unwrap();
    let baseline = editor.snapshot();
    assert!(editor.history.begin_transaction(baseline.clone()));
    editor.selected_objects = BTreeSet::from([2]);
    editor.selected_object = Some(2);
    assert!(!editor.history.begin_transaction(editor.snapshot()));
    assert!(editor.history.transaction_baseline() == Some(&baseline));
    assert_eq!(
        editor.confirm().unwrap(),
        ConfirmOutcome::InteractionFinished
    );
    assert_eq!(editor.selected_objects, BTreeSet::from([2]));
    assert_eq!(editor.history.undo_len(), 0);
    assert!(!editor.has_transform_session());
}

#[test]
fn invalid_nudge_keeps_the_valid_preview_and_original_cancellation_baseline() {
    let mut editor = cube();
    let baseline = editor.snapshot();
    editor.toggle_transform_axis(0).unwrap();
    editor.nudge(DVec3::X, false).unwrap();
    let preview = editor.snapshot();
    assert!(editor.nudge(DVec3::NAN, false).is_err());
    assert!(editor.nudge(DVec3::splat(f64::MAX), false).is_err());
    assert!(editor.snapshot() == preview);
    assert!(editor.history.transaction_baseline() == Some(&baseline));
    assert_eq!(editor.history.undo_len(), 0);
    assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
    assert!(editor.snapshot() == baseline);
}

#[test]
fn released_drag_cancel_restores_session_baseline_and_context_changes_cancel_first() {
    for context in 0..5 {
        let mut editor = cube();
        let before = editor.document.clone();
        editor.toggle_transform_axis(0).unwrap();
        editor.nudge(DVec3::X, false).unwrap();
        let projection = Projection::new(viewport(), &front(), false).unwrap();
        editor.prepare(&projection).unwrap();
        let handle = editor.locked_transform_handle(&projection).unwrap();
        let start = egui::pos2(100.0, 500.0);
        editor.begin_transform(&projection, &handle, start).unwrap();
        editor
            .preview_transform(start + egui::vec2(40.0, 0.0))
            .unwrap();
        editor.finish_gesture().unwrap();
        assert!(!editor.is_pointer_interacting());
        match context {
            0 => assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled),
            1 => editor.set_tool(Tool::Rotate),
            2 => editor.select_object(1).unwrap(),
            3 => {
                let ctx = egui::Context::default();
                frame(&ctx, &mut editor, &front(), vec![], false);
            }
            _ => {
                editor
                    .commit("Rename", |document| {
                        document.objects[0].name = "Renamed".into();
                        Ok(())
                    })
                    .unwrap();
                assert_eq!(
                    editor.document.objects[0].transform,
                    before.objects[0].transform
                );
                assert_eq!(editor.history.undo_len(), 1);
                editor.undo();
            }
        }
        assert_eq!(editor.document, before);
        assert!(!editor.is_interacting());
        assert_eq!(editor.transform_axis, None);
        assert_eq!(editor.history.undo_len(), 0);
    }
}

#[test]
fn session_confirm_while_pressed_suppresses_release_and_keeps_the_selection() {
    for finish in [true, false] {
        let mut editor = cube();
        let baseline = editor.document.clone();
        editor.toggle_transform_axis(0).unwrap();
        editor.nudge(DVec3::X, false).unwrap();
        let ctx = egui::Context::default();
        let camera = front();
        let start = egui::pos2(100.0, 500.0);
        let end = start + egui::vec2(30.0, 0.0);
        frame(&ctx, &mut editor, &camera, vec![], true);
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![Event::PointerMoved(start), pointer(start, true)],
            true,
        );
        frame(
            &ctx,
            &mut editor,
            &camera,
            vec![Event::PointerMoved(end)],
            true,
        );
        assert!(editor.is_pointer_interacting());
        let preview = editor.document.clone();
        if finish {
            assert_eq!(
                editor.confirm().unwrap(),
                ConfirmOutcome::InteractionFinished
            );
        } else {
            assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
        }
        assert!(editor.suppress_release);
        ctx.stop_dragging();
        frame(&ctx, &mut editor, &camera, vec![pointer(end, false)], true);
        assert_eq!(editor.selected_objects, BTreeSet::from([1]));
        assert!(!editor.edit_mode);
        assert!(!editor.is_interacting());
        assert_eq!(editor.transform_axis, None);
        assert_eq!(editor.document, if finish { preview } else { baseline });
        assert_eq!(editor.history.undo_len(), usize::from(finish));
    }
}

#[test]
fn double_click_confirms_armed_or_pending_move_without_jitter_or_edit_cascade() {
    for pending in [false, true] {
        for axis_off in [false, true] {
            if axis_off && !pending {
                continue;
            }
            for edit in [false, true] {
                let mut editor = cube();
                editor.convert_selected().unwrap();
                let history = editor.history.undo_len();
                if edit {
                    editor.enter_edit().unwrap();
                    editor.selected_vertices = editor
                        .document
                        .eval_object(1)
                        .unwrap()
                        .vertices
                        .iter()
                        .map(|vertex| vertex.id)
                        .collect();
                }
                editor.toggle_transform_axis(0).unwrap();
                if pending {
                    editor.nudge(DVec3::X, false).unwrap();
                }
                if axis_off {
                    editor.toggle_transform_axis(0).unwrap();
                }
                let preview = editor.snapshot();
                let ctx = egui::Context::default();
                let camera = front();
                let start = egui::pos2(80.0, 520.0);
                frame(&ctx, &mut editor, &camera, vec![], true);
                for _ in 0..2 {
                    frame(
                        &ctx,
                        &mut editor,
                        &camera,
                        vec![Event::PointerMoved(start), pointer(start, true)],
                        true,
                    );
                    let jitter = start + egui::vec2(1.0, -1.0);
                    frame(
                        &ctx,
                        &mut editor,
                        &camera,
                        vec![Event::PointerMoved(jitter), pointer(jitter, false)],
                        true,
                    );
                }
                assert!(editor.snapshot() == preview);
                assert_eq!(editor.transform_axis, None);
                assert!(!editor.is_interacting());
                assert_eq!(editor.history.undo_len(), history + usize::from(pending));
            }
        }
    }
}

#[test]
fn axis_off_pending_session_blocks_selection_and_armed_confirm_does_not_enter_edit() {
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.toggle_transform_axis(0).unwrap();
    assert_eq!(
        editor.confirm().unwrap(),
        ConfirmOutcome::InteractionFinished
    );
    assert!(!editor.edit_mode);
    assert_eq!(editor.transform_axis, None);
    editor.toggle_transform_axis(0).unwrap();
    editor.nudge(DVec3::X, false).unwrap();
    editor.toggle_transform_axis(0).unwrap();
    let preview = editor.snapshot();
    let history = editor.history.undo_len();
    let ctx = egui::Context::default();
    let camera = front();
    let start = egui::pos2(50.0, 450.0);
    let end = egui::pos2(700.0, 550.0);
    frame(&ctx, &mut editor, &camera, vec![], true);
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(start), pointer(start, true)],
        true,
    );
    frame(
        &ctx,
        &mut editor,
        &camera,
        vec![Event::PointerMoved(end)],
        true,
    );
    frame(&ctx, &mut editor, &camera, vec![pointer(end, false)], true);
    assert!(editor.snapshot() == preview);
    assert!(editor.has_transform_session());
    assert!(!editor.is_pointer_interacting());
    assert_eq!(editor.history.undo_len(), history);
    assert_eq!(editor.escape(), EscapeOutcome::InteractionCancelled);
    assert_eq!(editor.transform_axis, None);
}

#[test]
fn selection_groups_preserve_rotated_object_extents_and_the_gap_between_them() {
    let mut editor = pair();
    editor.select_object(2).unwrap();
    editor.select_object_with_modifier(1, true).unwrap();
    editor.document.objects[0].transform.translation = [-5.0, 0.0, 0.0];
    editor.document.objects[0].transform.rotation =
        DQuat::from_rotation_z(std::f64::consts::FRAC_PI_4).to_array();
    editor.document.objects[0].transform.scale = [2.0, 0.5, 1.0];
    editor.document.objects[1].transform.translation = [6.0, 0.0, 0.0];
    editor.document.objects[1].transform.rotation = DQuat::from_rotation_y(0.42).to_array();
    editor.document.objects[1].transform.scale = [0.8, 1.2, 2.3];
    let original = editor.document.clone();
    let original_frame = editor.frame;
    let revision = editor.revision;
    let x_bounds = |points: &[Vec3]| {
        points
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), point| {
                (min.min(point.x), max.max(point.x))
            })
    };
    for z_up in [false, true] {
        let groups = editor.selection_point_groups(z_up).unwrap();
        assert_eq!(groups.iter().map(Vec::len).collect::<Vec<_>>(), vec![8, 8]);
        let first = x_bounds(&groups[0]);
        let second = x_bounds(&groups[1]);
        let first_extent = (2.0 + 0.5) * std::f64::consts::FRAC_1_SQRT_2;
        let second_extent = 0.8 * 0.42_f64.cos() + 2.3 * 0.42_f64.sin();
        for (actual, expected) in [
            (first.0, (-5.0 - first_extent) * editor.frame.scale),
            (first.1, (-5.0 + first_extent) * editor.frame.scale),
            (second.0, (6.0 - second_extent) * editor.frame.scale),
            (second.1, (6.0 + second_extent) * editor.frame.scale),
        ] {
            assert!((f64::from(actual) - expected).abs() < 1e-6);
        }
        assert!(
            first.1 < second.0,
            "Separate ruler intervals must retain the empty gap"
        );
        let flat = groups.into_iter().flatten().collect::<Vec<_>>();
        assert_eq!(editor.selection_points(z_up).unwrap(), flat);
    }
    assert_eq!(editor.document, original);
    assert_eq!(editor.frame, original_frame);
    assert_eq!(editor.revision, revision);
    assert!(editor.cache.is_none());
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        (0, 0)
    );
}

#[test]
fn selection_groups_include_hidden_selected_vertices_and_omit_empty_selections() {
    let mut editor = cube();
    editor.convert_selected().unwrap();
    editor.enter_edit().unwrap();
    let mesh = editor.document.eval_object(1).unwrap();
    let visible: BTreeSet<_> = editor
        .selectable_vertices(viewport(), &front(), false)
        .unwrap()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let hidden = mesh
        .vertices
        .iter()
        .find(|vertex| !visible.contains(&vertex.id))
        .unwrap();
    let shown = mesh
        .vertices
        .iter()
        .find(|vertex| visible.contains(&vertex.id))
        .unwrap();
    editor.selected_vertices = BTreeSet::from([hidden.id, shown.id]);
    let before = editor.snapshot();
    let cached_projection = editor.cache.as_ref().unwrap().projection.clone();
    for z_up in [false, true] {
        let groups = editor.selection_point_groups(z_up).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        for vertex in [hidden, shown] {
            let expected = display_rotation(z_up).transform_point3(
                editor
                    .frame
                    .world_to_display(DVec3::from_array(vertex.position))
                    .as_vec3(),
            );
            assert!(groups[0].contains(&expected));
        }
        assert_eq!(groups[0], editor.selection_points(z_up).unwrap());
    }
    assert!(editor.snapshot() == before);
    assert!(editor.cache.as_ref().unwrap().projection == cached_projection);
    editor.selected_vertices.clear();
    assert!(editor.selection_point_groups(false).unwrap().is_empty());
    editor.leave_edit();
    editor.deselect();
    assert!(editor.selection_point_groups(false).unwrap().is_empty());
    editor.document.objects.push(Object {
        id: 2,
        name: "Empty mesh".into(),
        transform: Transform::default(),
        geometry: Geometry::Mesh(EditableMesh {
            vertices: vec![],
            edges: vec![],
            faces: vec![],
        }),
    });
    editor.select_object(2).unwrap();
    assert!(editor.selection_point_groups(false).unwrap().is_empty());
    editor.select_object_with_modifier(1, true).unwrap();
    assert_eq!(editor.selection_point_groups(false).unwrap().len(), 1);
}
