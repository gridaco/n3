use super::{Result, Session, pointer};
use crate::{
    controls::Control,
    document::{Document, Geometry, PrimitiveKind},
    editor::{Editor, Tool},
};
use glam::{DQuat, DVec3};
use std::{collections::BTreeSet, time::Duration};

fn view(s: &mut Session<'_>, binding: &str) -> Result<()> {
    s.shortcut(binding)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.require(
        !s.state.camera.is_transitioning(),
        "The axis-lock scenario waits for each requested view to finish",
    )
}

fn translated(before: &Document, after: &Document, selected: &BTreeSet<u64>, delta: DVec3) -> bool {
    before.version == after.version
        && before.length_unit == after.length_unit
        && before.objects.len() == after.objects.len()
        && before
            .objects
            .iter()
            .zip(&after.objects)
            .all(|(before, after)| {
                let expected = DVec3::from_array(before.transform.translation)
                    + if selected.contains(&before.id) {
                        delta
                    } else {
                        DVec3::ZERO
                    };
                before.id == after.id
                    && before.name == after.name
                    && before.geometry == after.geometry
                    && before.transform.rotation == after.transform.rotation
                    && before.transform.scale == after.transform.scale
                    && DVec3::from_array(after.transform.translation).abs_diff_eq(expected, 1e-9)
            })
}

fn drag_start(s: &mut Session<'_>) -> Result<(egui::Pos2, egui::Pos2)> {
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(45.0, -75.0);
    // Cross the Auto snapping interval on visible and end-on axes.
    let end = start + egui::vec2(48.0, -60.0);
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(vec![pointer(start, true)], Duration::ZERO)?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    Ok((start, end))
}

fn release_drag(s: &mut Session<'_>, end: egui::Pos2) -> Result<()> {
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.settle()
}

fn double_click(s: &mut Session<'_>) -> Result<()> {
    let position = s.empty_viewport_point()?;
    s.frame(
        vec![egui::Event::PointerMoved(position)],
        Duration::from_millis(500),
    )?;
    s.frame(vec![pointer(position, true)], Duration::ZERO)?;
    s.frame(vec![pointer(position, false)], Duration::from_millis(20))?;
    s.frame(Vec::new(), Duration::from_millis(80))?;
    s.frame(vec![pointer(position, true)], Duration::ZERO)?;
    s.frame(vec![pointer(position, false)], Duration::from_millis(20))?;
    s.settle()
}

fn navigation_between_drags(s: &mut Session<'_>) -> Result<()> {
    s.shortcut("edit.leave")?;
    s.shortcut("selection.all")?;
    s.shortcut("tool.move")?;
    view(s, "view.front")?;
    let baseline = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    s.shortcut("transform.axis-x")?;
    let (_, end) = drag_start(s)?;
    release_drag(s, end)?;
    let preview = s.state.editor.document.clone();
    s.require(
        preview != baseline
            && s.state.editor.has_transform_session()
            && !s.state.editor.is_pointer_interacting(),
        "Camera inspection starts with a released locked drag and an actual pending preview",
    )?;

    for (binding, delta) in [
        ("navigation.orbit", egui::vec2(42.0, -24.0)),
        ("navigation.pan", egui::vec2(32.0, 20.0)),
    ] {
        let start = s.state.viewport_ui_rect.center();
        s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
        let camera = s.state.camera.view_projection(s.state.aspect());
        s.shortcut_down(binding)?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.move_pointer(start + delta, Duration::from_millis(200))?;
        s.require(
            s.state.mouse_navigation_active()
                && !s
                    .state
                    .camera
                    .view_projection(s.state.aspect())
                    .abs_diff_eq(camera, 1e-5)
                && s.state.editor.document == preview
                && s.state.editor.transform_axis == Some(0)
                && s.state.editor.has_transform_session()
                && !s.state.editor.is_pointer_interacting()
                && s.state.editor.selected_objects == selected,
            "A held orbit or pan drag changes the camera while preserving the released transform preview, lock, and selection",
        )?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.shortcut_up(binding)?;
        s.require(
            !s.state.mouse_navigation_active()
                && s.state.editor.document == preview
                && s.state.editor.transform_axis == Some(0)
                && s.state.editor.has_transform_session(),
            "Releasing camera navigation leaves the transform session ready to continue",
        )?;
    }
    let camera = s.state.camera.view_projection(s.state.aspect());
    s.pinch(0.15)?;
    s.require(
        !s.state
            .camera
            .view_projection(s.state.aspect())
            .abs_diff_eq(camera, 1e-5)
            && s.state.editor.document == preview
            && s.state.editor.transform_axis == Some(0)
            && s.state.editor.has_transform_session(),
        "Pinch zoom inspects the pending preview without applying it or changing its axis",
    )?;

    let (_, end) = drag_start(s)?;
    release_drag(s, end)?;
    let applied = s.state.editor.document.clone();
    let inspected_camera = s.state.camera.view_projection(s.state.aspect());
    s.require(
        applied != preview
            && s.state.editor.has_transform_session()
            && s.state.editor.selected_objects == selected,
        "A second locked drag after camera inspection continues the pending transform",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.document == applied
            && !s.state.editor.has_transform_session()
            && s.state.editor.transform_axis.is_none(),
        "Confirm applies the complete transform after camera inspection",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == baseline
            && s.state.editor.selected_objects == selected
            && s.state.camera.view_projection(s.state.aspect()) == inspected_camera,
        "One Undo restores both locked drags across camera navigation while keeping the inspected view",
    )?;

    s.shortcut("transform.axis-x")?;
    let (_, end) = drag_start(s)?;
    release_drag(s, end)?;
    let cancelled_preview = s.state.editor.document.clone();
    s.hover(Control::Viewport)?;
    s.pinch(0.1)?;
    let inspected_camera = s.state.camera.view_projection(s.state.aspect());
    s.require(
        cancelled_preview != baseline
            && s.state.editor.document == cancelled_preview
            && s.state.editor.has_transform_session(),
        "The cancellation check retains a changed preview through another camera inspection",
    )?;
    s.shortcut("cancel")?;
    s.require(
        s.state.editor.document == baseline
            && !s.state.editor.has_transform_session()
            && s.state.editor.transform_axis.is_none()
            && s.state.editor.selected_objects == selected
            && s.state.camera.view_projection(s.state.aspect()) == inspected_camera,
        "Cancel restores the original transform baseline and selection without undoing camera navigation",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    let mut document = Document::default();
    let first = document.insert_primitive(PrimitiveKind::Cube)?;
    let second = document.insert_primitive(PrimitiveKind::Cube)?;
    for (id, name, x) in [(first, "Left cube", -3.0), (second, "Rotated cube", 3.0)] {
        let object = document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap();
        object.name = name.into();
        object.transform.translation[0] = x;
        if id == second {
            object.transform.rotation = DQuat::from_rotation_y(0.35).to_array();
        }
    }
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.witness(Control::Viewport)?;
    s.witness(Control::ToolMove)?;
    let empty = s.empty_viewport_point()?;
    s.click_at(empty)?;
    s.shortcut("selection.all")?;
    s.shortcut("tool.move")?;
    let selected = BTreeSet::from([first, second]);
    s.require(
        s.state.editor.selected_objects == selected
            && s.state.editor.tool == Tool::Move
            && s.state.editor.transform_axis.is_none(),
        "The Move example starts with both live primitives selected and no axis lock",
    )?;

    s.shortcut("nudge.right")?;
    s.require(
        s.state.editor.document == document,
        "Without a lock, an oblique perspective view gives arrow keys no movement axis",
    )?;
    for (binding, axis) in [
        ("transform.axis-x", 0),
        ("transform.axis-y", 1),
        ("transform.axis-z", 2),
    ] {
        s.shortcut(binding)?;
        s.require(
            s.state.editor.transform_axis == Some(axis) && !s.state.editor.is_pointer_interacting(),
            "The axis-lock shortcuts arm a Move session on the matching document axes",
        )?;
        s.shortcut(binding)?;
        s.require(
            s.state.editor.transform_axis.is_none() && s.state.editor.document == document,
            "Pressing the active axis key again removes the lock without editing geometry",
        )?;
        if s.state.editor.has_transform_session() {
            s.shortcut("cancel")?;
        }
        s.require(
            !s.state.editor.has_transform_session()
                && s.state.editor.selected_objects == selected
                && s.state.editor.document == document,
            "An unchanged arm-and-unlock sequence leaves selection and geometry intact",
        )?;
    }

    view(s, "view.front")?;
    s.shortcut("transform.axis-x")?;
    s.shortcut("nudge.right")?;
    s.require(
        translated(&document, &s.state.editor.document, &selected, DVec3::X),
        "A locked Right arrow moves every selected object exactly one centimeter along X, preserving primitive parameters and object rotations",
    )?;
    s.shortcut("nudge.fast-right")?;
    s.require(
        translated(
            &document,
            &s.state.editor.document,
            &selected,
            DVec3::X * 11.0,
        ),
        "The coarse right nudge adds exactly ten centimeters on the locked axis",
    )?;
    s.shortcut("nudge.up")?;
    s.require(
        translated(
            &document,
            &s.state.editor.document,
            &selected,
            DVec3::X * 12.0,
        ),
        "An upward nudge perpendicular to the visible X axis uses the positive-coordinate fallback",
    )?;
    s.value("nudge-step", 1);
    s.value("large-nudge-step", 10);

    s.shortcut_down("nudge.right")?;
    for _ in 0..2 {
        let mut event = s.shortcut_event("nudge.right", true)?;
        if let egui::Event::Key {
            repeat,
            physical_key,
            ..
        } = &mut event
        {
            *repeat = true;
            *physical_key = None;
        }
        s.frame(vec![event], Duration::from_millis(80))?;
    }
    s.shortcut_up("nudge.right")?;
    s.require(
        translated(
            &document,
            &s.state.editor.document,
            &selected,
            DVec3::X * 15.0,
        ) && s.state.editor.has_transform_session()
            && !s.state.editor.is_pointer_interacting(),
        "Separate arrow presses and held repeats accumulate in the same Move preview after every key release",
    )?;
    s.shortcut("transform.axis-y")?;
    s.shortcut("nudge.up")?;
    let applied = s.state.editor.document.clone();
    s.require(
        translated(&document, &applied, &selected, DVec3::new(15.0, 1.0, 0.0))
            && s.state.editor.has_transform_session(),
        "Changing the paused session to Y adds its movement without replacing the original X-and-Y baseline",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        !s.state.editor.has_transform_session()
            && !s.state.editor.is_interacting()
            && s.state.editor.transform_axis.is_none()
            && s.state.editor.document == applied
            && !s.state.editor.edit_mode
            && s.state.editor.selected_objects == selected,
        "Confirm applies the entire Move session, clears the lock and keeps the editing context",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document,
        "One Undo restores all arrow presses, repeats and axis changes in the confirmed session",
    )?;
    s.shortcut("history.redo")?;
    s.require(
        s.state.editor.document == applied,
        "Redo restores the complete confirmed Move session",
    )?;
    s.shortcut("history.undo")?;

    s.shortcut("transform.axis-x")?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    let (_, end) = drag_start(s)?;
    let preview = s.state.editor.document.clone();
    let dx =
        preview.objects[0].transform.translation[0] - document.objects[0].transform.translation[0];
    s.require(
        s.state.editor.is_transforming()
            && s.state.editor.transform_axis == Some(0)
            && dx > 0.0
            && translated(&document, &preview, &selected, DVec3::X * dx)
            && s.state.editor.selected_objects == selected,
        "With X locked, a primary drag from empty viewport space moves the entire selection along X instead of drawing a box",
    )?;
    s.witness(Control::MoveAxisLock)?;
    s.capture_tutorial("axis-locks-drag")?;
    release_drag(s, end)?;
    s.require(
        s.state.editor.has_transform_session()
            && s.state.editor.is_interacting()
            && !s.state.editor.is_pointer_interacting()
            && s.state.editor.document == preview,
        "Releasing a locked drag pauses the visible preview without confirming the Move session",
    )?;
    let active_x = preview
        .objects
        .iter()
        .find(|object| Some(object.id) == s.state.editor.selected_object)
        .ok_or("The Move session must retain its active object")?
        .transform
        .translation[0];
    let nudge_dx = (active_x + 1.0).round() - active_x;
    s.shortcut("nudge.right")?;
    s.require(
        translated(&preview, &s.state.editor.document, &selected, DVec3::X * nudge_dx),
        "An arrow nudge continues the same preview after a drag release and aligns its Auto-snapped fractional anchor to the one-centimeter keyboard grid",
    )?;
    let before_second_drag = s.state.editor.document.clone();
    let (_, end) = drag_start(s)?;
    let applied = s.state.editor.document.clone();
    release_drag(s, end)?;
    s.require(
        applied != before_second_drag
            && s.state.editor.has_transform_session()
            && !s.state.editor.is_pointer_interacting(),
        "A second drag extends the pending Move preview and its release still does not confirm",
    )?;
    double_click(s)?;
    s.require(
        !s.state.editor.has_transform_session()
            && s.state.editor.transform_axis.is_none()
            && s.state.editor.document == applied
            && s.state.editor.selected_objects == selected
            && !s.state.editor.edit_mode,
        "Two actual viewport clicks confirm the whole Move session without selecting background or changing editing mode",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document,
        "One Undo restores both released drags and their intervening nudge together",
    )?;

    s.shortcut("transform.axis-x")?;
    s.shortcut("nudge.right")?;
    let (_, end) = drag_start(s)?;
    release_drag(s, end)?;
    s.shortcut("transform.axis-y")?;
    s.shortcut("nudge.up")?;
    let (_, end) = drag_start(s)?;
    s.shortcut("cancel")?;
    s.require(
        !s.state.editor.is_interacting()
            && s.state.editor.document == document
            && !s.state.editor.has_transform_session()
            && s.state.editor.transform_axis.is_none()
            && s.state.editor.selected_objects == selected,
        "Cancel during a drag cancels every preview since the session began and clears the axis while retaining selection",
    )?;
    release_drag(s, end)?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document,
        "Cancelling a Move session adds no undo entry",
    )?;

    s.shortcut("transform.axis-x")?;
    s.shortcut("nudge.right")?;
    s.shortcut("transform.axis-x")?;
    let pending = s.state.editor.document.clone();
    let (_, end) = drag_start(s)?;
    release_drag(s, end)?;
    s.require(
        s.state.editor.transform_axis.is_none()
            && s.state.editor.has_transform_session()
            && s.state.editor.document == pending
            && s.state.editor.selected_objects == selected,
        "An axis-free pending session blocks background box selection and keeps its movement preview",
    )?;
    s.shortcut("cancel")?;
    s.require(
        !s.state.editor.has_transform_session()
            && s.state.editor.document == document
            && s.state.editor.selected_objects == selected,
        "Cancel also restores the original baseline after the active axis was toggled off",
    )?;

    let (_, end) = drag_start(s)?;
    s.require(
        s.state.editor.is_interacting()
            && !s.state.editor.is_transforming()
            && s.state.editor.document == document,
        "With the lock cleared, the same empty-space drag draws a selection box",
    )?;
    s.shortcut("cancel")?;
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.shortcut("transform.axis-z")?;
    s.shortcut("nudge.up")?;
    s.require(
        s.state.editor.has_transform_session() && s.state.editor.document != document,
        "The tool-change check begins with an actual pending Move preview",
    )?;
    s.shortcut("tool.rotate")?;
    s.shortcut("nudge.right")?;
    s.require(
        s.state.editor.tool == Tool::Rotate
            && s.state.editor.transform_axis.is_none()
            && !s.state.editor.has_transform_session()
            && s.state.editor.document == document,
        "Leaving Move cancels its pending session and clears the lock; arrow nudges do not run in Rotate",
    )?;
    s.shortcut("tool.move")?;

    for (view_binding, nudge_binding, delta) in [
        ("view.front", "nudge.right", DVec3::X),
        ("view.front", "nudge.up", DVec3::Y),
        ("view.right", "nudge.right", DVec3::NEG_Z),
        ("view.right", "nudge.up", DVec3::Y),
        ("view.top", "nudge.right", DVec3::X),
        ("view.top", "nudge.up", DVec3::NEG_Z),
    ] {
        view(s, view_binding)?;
        s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
        s.shortcut_down(nudge_binding)?;
        s.require(
            s.state.editor.transform_axis.is_none()
                && translated(&document, &s.state.editor.document, &selected, delta),
            "Without a lock, arrow nudges follow the document axes shown by Front, Right and Top views",
        )?;
        if view_binding == "view.front" && nudge_binding == "nudge.up" {
            s.capture_tutorial("axis-locks-aligned-nudge")?;
        }
        s.shortcut_up(nudge_binding)?;
        s.shortcut("history.undo")?;
        s.require(
            s.state.editor.document == document,
            "Undo restores the aligned-view nudge before the next example",
        )?;
    }

    view(s, "view.front")?;
    let row = s
        .state
        .layer_row_rect(first)
        .ok_or("The first cube has no Layers row")?;
    s.click_at(row.center())?;
    s.shortcut("edit.confirm")?;
    let empty = s.empty_viewport_point()?;
    s.click_at(empty)?;
    s.shortcut("selection.next")?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_vertices.len() == 1,
        "The vertex example selects one visible component without converting the cube",
    )?;
    let before_vertices = s.state.editor.document.clone();
    let vertices = s.state.editor.selected_vertices.clone();
    s.shortcut("transform.axis-z")?;
    s.shortcut("nudge.up")?;
    let mut mesh = before_vertices.eval_object(first)?;
    for vertex in &mut mesh.vertices {
        if vertices.contains(&vertex.id) {
            vertex.position[2] += 1.0;
        }
    }
    let mut expected = before_vertices.clone();
    let object = expected
        .objects
        .iter_mut()
        .find(|object| object.id == first)
        .unwrap();
    object.geometry = Geometry::Mesh(mesh);
    s.require(
        s.state.editor.document == expected
            && s.state.editor.transform_axis == Some(2)
            && s.state.editor.has_transform_session()
            && s.state.editor.selected_vertices == vertices,
        "With Z facing the camera, locked Up previews one positive document-Z unit on the selected vertex, retaining topology and the other live primitive",
    )?;
    let (_, end) = drag_start(s)?;
    release_drag(s, end)?;
    let applied_vertices = s.state.editor.document.clone();
    let object = applied_vertices
        .objects
        .iter()
        .find(|object| object.id == first)
        .unwrap();
    let Geometry::Mesh(moved_mesh) = &object.geometry else {
        return Err("The preview lost its editable mesh".into());
    };
    let expected_object = expected
        .objects
        .iter_mut()
        .find(|object| object.id == first)
        .unwrap();
    let Geometry::Mesh(mesh) = &mut expected_object.geometry else {
        return Err("The expected vertex preview is not a mesh".into());
    };
    let extra = moved_mesh
        .vertices
        .iter()
        .find(|vertex| vertices.contains(&vertex.id))
        .unwrap()
        .position[2]
        - mesh
            .vertices
            .iter()
            .find(|vertex| vertices.contains(&vertex.id))
            .unwrap()
            .position[2];
    for vertex in &mut mesh.vertices {
        if vertices.contains(&vertex.id) {
            vertex.position[2] += extra;
        }
    }
    s.require(
        extra > 0.0 && applied_vertices == expected
            && s.state.editor.has_transform_session()
            && !s.state.editor.is_pointer_interacting()
            && s.state.editor.selected_vertices == vertices,
        "A released depth drag adds to the vertex nudge in the same pending session, changing only selected coordinates",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        !s.state.editor.has_transform_session()
            && s.state.editor.transform_axis.is_none()
            && s.state.editor.edit_mode
            && s.state.editor.selected_vertices == vertices
            && s.state.editor.document == applied_vertices,
        "Confirm confirms the vertex Move session while staying in vertex mode with the same selection",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == before_vertices,
        "One Undo restores both the vertex nudge and the released drag",
    )?;
    s.shortcut("transform.axis-z")?;
    s.shortcut("nudge.up")?;
    s.shortcut("nudge.fast-up")?;
    s.shortcut("cancel")?;
    s.require(
        !s.state.editor.has_transform_session()
            && s.state.editor.transform_axis.is_none()
            && s.state.editor.edit_mode
            && s.state.editor.selected_vertices == vertices
            && s.state.editor.document == before_vertices,
        "Cancel cancels all vertex-session nudges while retaining vertex mode and selection",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document,
        "After cancellation the next Undo reaches conversion, proving the cancelled vertex session added no history entry",
    )?;
    navigation_between_drags(s)
}
