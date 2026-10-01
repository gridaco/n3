use super::{Result, Session};
use crate::{
    controls::Control,
    document::{Document, Geometry, PrimitiveKind},
    editor::Editor,
    object_feedback::SELECTED_COLOR,
    shortcuts::viewport_focus_id,
};
use std::collections::BTreeSet;

fn focus_viewport(s: &mut Session<'_>) -> Result<()> {
    let empty = s.empty_viewport_point()?;
    s.click_at(empty)?;
    s.require(
        s.ctx.memory(|memory| memory.focused()) == Some(viewport_focus_id()),
        "An actual viewport click gives the viewport keyboard focus before Next selection cycling",
    )
}

fn selected_object(s: &mut Session<'_>, id: u64, fact: &str) -> Result<()> {
    s.require(
        s.state.editor.selected_objects == BTreeSet::from([id])
            && s.state.editor.selected_object == Some(id),
        fact,
    )
}

fn duplicated_objects(s: &mut Session<'_>, before: &Document, active: u64) -> Result<Vec<u64>> {
    let after = &s.state.editor.document;
    let count = before.objects.len();
    s.require(
        after.objects.len() == count * 2,
        "Duplicate creates exactly one new object for each selected source",
    )?;
    let after = &s.state.editor.document;
    let copies = &after.objects[count..];
    let ids: Vec<_> = copies.iter().map(|object| object.id).collect();
    let selected: BTreeSet<_> = ids.iter().copied().collect();
    let source_ids: BTreeSet<_> = before.objects.iter().map(|object| object.id).collect();
    let active_index = before
        .objects
        .iter()
        .position(|object| object.id == active)
        .ok_or("The active duplicate source is missing")?;
    let correct = after.version == before.version
        && after.length_unit == before.length_unit
        && after.objects[..count] == before.objects
        && selected.len() == count
        && selected.is_disjoint(&source_ids)
        && copies.iter().zip(&before.objects).all(|(copy, source)| {
            copy.geometry == source.geometry
                && copy.transform == source.transform
                && copy.name == format!("{} copy", source.name)
        })
        && s.state.editor.selected_objects == selected
        && s.state.editor.selected_object == Some(ids[active_index])
        && ids
            .iter()
            .all(|id| s.state.layer_row_color(*id) == Some(SELECTED_COLOR));
    s.require(
        correct,
        "Duplicate appends independent in-place copies with fresh object IDs, intact mesh and primitive data, unchanged originals, and only the copies selected",
    )?;
    Ok(ids)
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    // A small existing scene is the starting point; every documented command
    // is delivered through real keys or its live context-menu control.
    let mut document = Document::default();
    let first = document.insert_primitive(PrimitiveKind::Cube)?;
    let second = document.insert_primitive(PrimitiveKind::Cube)?;
    for (id, name, x) in [(first, "Left cube", -2.3), (second, "Right cube", 2.3)] {
        let object = document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap();
        object.name = name.into();
        object.transform.translation[0] = x;
        if id == first {
            document.convert_object(id)?;
        }
    }
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.witness(Control::Viewport)?;
    s.witness(Control::ObjectList)?;
    let dirty = s.state.is_dirty();
    focus_viewport(s)?;
    s.require(
        s.state.editor.selected_objects.is_empty(),
        "Clicking empty viewport starts object cycling with no selection",
    )?;
    let revision = s.state.editor.revision;
    s.shortcut("selection.duplicate")?;
    s.right_click(Control::Viewport)?;
    s.require(
        !s.trace.get(Control::DuplicateSelection)?.enabled
            && s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.selected_objects.is_empty(),
        "Duplicate does nothing with no object selection and the context-menu Duplicate action is disabled",
    )?;
    s.shortcut("cancel")?;
    focus_viewport(s)?;
    for (binding, expected, fact) in [
        (
            "selection.next",
            first,
            "Next selection with no selection selects the first object in Layers order",
        ),
        (
            "selection.next",
            second,
            "Next selection advances to the next object",
        ),
        (
            "selection.next",
            first,
            "Next selection wraps from the last object to the first",
        ),
        (
            "selection.previous",
            second,
            "Previous selection cycles backwards and wraps to the last object",
        ),
    ] {
        s.shortcut(binding)?;
        selected_object(s, expected, fact)?;
    }
    s.shortcut("cancel")?;
    s.shortcut("selection.previous")?;
    selected_object(
        s,
        second,
        "Previous selection with no selection starts at the last object",
    )?;
    s.shortcut("selection.next")?;
    selected_object(s, first, "Forward cycling resumes from the active object")?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document
            && s.state.is_dirty() == dirty
            && s.state.editor.selected_objects == BTreeSet::from([first]),
        "Keyboard selection does not modify geometry, saved state, or undo history",
    )?;

    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.shortcut_down("selection.all")?;
    let both = BTreeSet::from([first, second]);
    s.require(
        s.state.editor.selected_objects == both
            && s.state.object_highlights().selected == both
            && s.state.layer_row_color(first) == Some(SELECTED_COLOR)
            && s.state.layer_row_color(second) == Some(SELECTED_COLOR)
            && s.state.editor.document == document,
        "Select all in object mode selects every document object and highlights both Layers rows and outlines",
    )?;
    s.capture_tutorial("selection-keys-all-objects")?;
    s.shortcut_up("selection.all")?;

    let active = s
        .state
        .editor
        .selected_object
        .ok_or("Select all has no active object")?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.shortcut_down("selection.duplicate")?;
    let copies = duplicated_objects(s, &document, active)?;
    let duplicated = s.state.editor.document.clone();
    s.require(
        !s.state.editor.is_interacting()
            && matches!(&duplicated.objects[3].geometry, Geometry::Primitive(_)),
        "Duplicating retains the live primitive and starts no automatic move or editing session",
    )?;
    s.capture_tutorial("selection-keys-duplicate")?;
    s.shortcut_up("selection.duplicate")?;

    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document && s.state.editor.selected_objects == both
            && s.state.editor.selected_object == Some(active),
        "One Undo removes the entire duplicate group and restores the original selection and active object",
    )?;
    s.shortcut("history.redo")?;
    s.require(
        s.state.editor.document == duplicated
            && s.state.editor.selected_objects == copies.iter().copied().collect(),
        "One Redo restores every copy with the same IDs, geometry and copy selection",
    )?;
    duplicated_objects(s, &document, active)?;

    let row = s
        .state
        .layer_row_rect(copies[1])
        .ok_or("The primitive copy has no Layers row")?;
    s.click_at(row.center())?;
    s.drag(Control::PrimitiveX, egui::vec2(25.0, 0.0))?;
    s.require(
        s.state.editor.document.objects[..document.objects.len()] == document.objects
            && s.state.editor.document.objects[3].geometry != duplicated.objects[3].geometry,
        "Editing the copied primitive's width leaves both original objects unchanged",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == duplicated,
        "Undo restores the independent copy edit",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document && s.state.editor.selected_objects == both,
        "One further Undo removes the entire duplicate group and restores the original selection",
    )?;
    s.right_click(Control::Viewport)?;
    s.require(
        s.trace.get(Control::DuplicateSelection)?.enabled
            && s.trace.get(Control::DuplicateSelection)?.parents == [Control::ViewportMenu],
        "The enabled Duplicate command belongs to the viewport context menu",
    )?;
    s.click(Control::DuplicateSelection)?;
    duplicated_objects(s, &document, active)?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document && s.state.editor.selected_objects == both,
        "Context-menu duplication has the same one-step undo boundary as Duplicate",
    )?;

    s.right_click(Control::Viewport)?;
    s.require(
        s.trace.get(Control::SelectAll)?.parents == [Control::ViewportMenu]
            && s.trace.get(Control::DeleteSelection)?.parents == [Control::ViewportMenu],
        "The viewport context menu exposes the same Select all and Delete selection commands",
    )?;
    s.witness(Control::SelectAll)?;
    s.click(Control::DeleteSelection)?;
    s.require(
        s.state.editor.document.objects.is_empty()
            && s.state.editor.selected_objects.is_empty()
            && s.state.editor.selected_object.is_none(),
        "Delete selection in the context menu removes the selected object group",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document && s.state.editor.selected_objects == both,
        "One Undo restores the complete deleted object group and its selection",
    )?;

    focus_viewport(s)?;
    s.right_click(Control::Viewport)?;
    s.click(Control::SelectAll)?;
    s.require(
        s.state.editor.selected_objects == both,
        "Select all in the menu selects the same complete object set as Select all",
    )?;
    s.shortcut("selection.backspace")?;
    s.require(
        s.state.editor.document.objects.is_empty(),
        "Backspace deletes the selected objects through the shared Delete selection command",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document,
        "One Undo restores a Backspace deletion exactly",
    )?;
    s.shortcut("history.redo")?;
    s.require(
        s.state.editor.document.objects.is_empty(),
        "Redo redoes the complete object deletion",
    )?;
    s.shortcut("history.undo")?;

    focus_viewport(s)?;
    s.shortcut("selection.next")?;
    selected_object(
        s,
        first,
        "Viewport-focused Next selection selects the first mesh for vertex editing",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.edit_mode,
        "Confirm changes the command context to vertex editing",
    )?;
    let revision = s.state.editor.revision;
    s.shortcut("selection.duplicate")?;
    s.right_click(Control::Viewport)?;
    s.require(
        !s.trace.get(Control::DuplicateSelection)?.enabled
            && s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.edit_mode,
        "Duplicate is inactive in vertex mode and its object-only menu action is disabled",
    )?;
    s.shortcut("cancel")?;
    let Geometry::Mesh(original_mesh) = &document.objects[0].geometry else {
        return Err("The keyboard tutorial requires an editable mesh".into());
    };
    let visible: BTreeSet<_> = s
        .state
        .editor
        .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let ordered: Vec<_> = original_mesh
        .vertices
        .iter()
        .filter(|vertex| visible.contains(&vertex.id))
        .map(|vertex| vertex.id)
        .collect();
    s.require(
        !ordered.is_empty() && ordered.len() < original_mesh.vertices.len(),
        "The tutorial mesh contains both visible and occluded vertices",
    )?;
    s.shortcut("selection.all")?;
    s.require(
        s.state.editor.selected_vertices == visible
            && s.state.editor.selected_objects == BTreeSet::from([first])
            && s.state.editor.document == document,
        "Select all in vertex mode selects currently visible vertices of the edited mesh and leaves other objects unchanged",
    )?;
    s.shortcut("cancel")?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_vertices.is_empty(),
        "Cancel clears vertex selection while keeping the viewport in vertex mode",
    )?;
    for id in &ordered {
        s.shortcut("selection.next")?;
        s.require(
            s.state.editor.selected_vertices == BTreeSet::from([*id]),
            "Next selection cycles visible vertices in their mesh order, selecting one at a time",
        )?;
    }
    s.shortcut("selection.next")?;
    s.require(
        s.state.editor.selected_vertices == BTreeSet::from([ordered[0]]),
        "Vertex Next selection wraps to the first visible vertex",
    )?;
    s.shortcut("selection.previous")?;
    s.require(
        s.state.editor.selected_vertices == BTreeSet::from([*ordered.last().unwrap()]),
        "Previous selection reverses vertex cycling and wraps to the last visible vertex",
    )?;
    s.shortcut("cancel")?;
    s.shortcut("selection.previous")?;
    s.require(
        s.state.editor.selected_vertices == BTreeSet::from([*ordered.last().unwrap()]),
        "Previous selection with no selected vertices starts at the last visible vertex",
    )?;
    s.shortcut("selection.next")?;
    let removed = ordered[0];
    let mut expected = document.clone();
    let Geometry::Mesh(expected_mesh) = &mut expected.objects[0].geometry else {
        unreachable!()
    };
    expected_mesh.vertices.retain(|vertex| vertex.id != removed);
    expected_mesh
        .faces
        .retain(|face| !face.vertices.contains(&removed));
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.shortcut_down("selection.delete")?;
    s.require(
        s.state.editor.document == expected
            && s.state.editor.edit_mode
            && s.state.editor.selected_vertices.is_empty()
            && s.state.editor.selected_object == Some(first),
        "Delete removes selected vertices and every incident face, retaining the object, unselected vertices, other objects and surviving polygon faces without repair",
    )?;
    s.capture_tutorial("selection-keys-delete-vertex")?;
    s.shortcut_up("selection.delete")?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.selected_vertices == BTreeSet::from([removed]),
        "One Undo restores deleted vertex IDs, positions, incident faces and component selection exactly",
    )?;
    s.shortcut("history.redo")?;
    s.require(
        s.state.editor.document == expected,
        "Redo restores the same vertex deletion without generating replacement faces",
    )?;
    s.shortcut("history.undo")?;

    Ok(())
}
