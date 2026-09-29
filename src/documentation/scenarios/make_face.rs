//! One authored face is an ordinary validated edit, including primitive recovery.
use super::{ClipSpec, Result, Session};
use crate::{
    controls::Control,
    document::{Document, EditableMesh, Geometry, MeshVertex, PrimitiveKind},
    editor::Editor,
};
use egui::PointerButton;
use std::{collections::BTreeSet, time::Duration};

fn evaluated(s: &Session<'_>, id: u64) -> Result<EditableMesh> {
    s.state.editor.document.eval_object(id)
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.click_path(&[Control::N3Menu, Control::FileMenu, Control::New])?;
    s.require(
        s.state.editor.document.objects.is_empty(),
        "Make Face begins in an empty document through the real New command",
    )?;
    s.hover(Control::Viewport)?;
    s.shortcut("view.front")?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.click_path(&[Control::InsertMenu, Control::InsertCircle])?;
    let id = s
        .state
        .editor
        .selected_object
        .ok_or("Insert Circle did not select its object")?;
    let before = s.state.editor.document.clone();
    let original = evaluated(s, id)?;
    let all_ids: BTreeSet<_> = original.vertices.iter().map(|vertex| vertex.id).collect();
    s.require(
        s.state.is_planar_navigation()
            && matches!(
                &before.objects[0].geometry,
                Geometry::Primitive(primitive)
                    if primitive.kind == PrimitiveKind::Circle && !primitive.fill
            )
            && original.faces.is_empty()
            && original.edges.len() == original.vertices.len(),
        "Inserting Circle in Front view creates a view-facing parametric outline with no faces",
    )?;
    s.shortcut("tool.cursor")?;
    s.shortcut("edit.confirm")?;
    s.witness(Control::ToolView)?;
    s.require(
        s.state.editor.edit_mode
            && s.state.editor.selected_vertices.is_empty()
            && s.state.editor.document == before,
        "Entering vertex editing keeps the circle recipe and leaves its vertices unselected",
    )?;
    s.hover(Control::Viewport)?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_clip("make-face-circle", ClipSpec::default(), |s| {
        s.callout(
            Control::ToolView,
            &format!(
                "{} selects the complete outline.",
                s.shortcut_label("selection.all")?
            ),
        )?;
        s.wait(Duration::from_millis(750))?;
        s.shortcut("selection.all")?;
        s.require(
            s.state.editor.selected_vertices == all_ids && s.state.editor.document == before,
            "Select All selects every circle vertex without materializing the primitive",
        )?;
        s.wait(Duration::from_millis(850))?;
        s.callout(
            Control::ToolView,
            &format!(
                "{} makes one face from the selected outline.",
                s.shortcut_label("mesh.make-face")?
            ),
        )?;
        s.shortcut("mesh.make-face")?;
        let after = s.state.editor.document.clone();
        let filled = evaluated(s, id)?;
        s.require(
            matches!(after.objects[0].geometry, Geometry::Mesh(_))
                && filled.vertices == original.vertices
                && filled.faces.len() == 1
                && filled.faces[0].vertices.iter().copied().collect::<BTreeSet<_>>() == all_ids
                && filled.edges.is_empty()
                && s.state.editor.selected_vertices == all_ids
                && s.state.editor.edit_mode,
            "Make Face creates one authored polygon, absorbs its loose boundary edges, and preserves positions, IDs and selection",
        )?;
        s.wait(Duration::from_millis(1100))?;
        s.callout(
            Control::ToolView,
            "Right-drag to inspect the new surface. Its selected vertices stay in place.",
        )?;
        let start = s.state.viewport.center() + egui::vec2(140.0, 90.0);
        s.move_pointer(start, Duration::from_millis(300))?;
        s.pointer_button(PointerButton::Secondary, true)?;
        s.move_pointer(start + egui::vec2(-80.0, 45.0), Duration::from_millis(850))?;
        s.require(
            !s.state.is_planar_navigation()
                && s.state.editor.document == after
                && s.state.editor.selected_vertices == all_ids,
            "Orbiting out of 2D reveals the filled surface without changing the face or its selection",
        )?;
        s.pointer_button(PointerButton::Secondary, false)?;
        s.wait(Duration::from_millis(800))?;
        s.callout(
            Control::ToolView,
            &format!(
                "{} restores the original parametric circle in one step.",
                s.shortcut_label("history.undo")?
            ),
        )?;
        s.undo()?;
        s.require(
            s.state.editor.document == before
                && s.state.editor.edit_mode
                && s.state.editor.selected_vertices == all_ids,
            "One Undo removes the face and exactly restores the original primitive while retaining edit mode and selected vertices",
        )?;
        s.wait(Duration::from_millis(1100))?;
        s.callout(
            Control::ToolView,
            &format!(
                "{} brings the face back.",
                s.shortcut_label("history.redo")?
            ),
        )?;
        s.redo()?;
        s.require(
            s.state.editor.document == after,
            "Redo restores the same face, IDs, geometry and object transform",
        )?;
        s.wait(Duration::from_millis(1100))?;
        s.clear_callout()
    })?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("make-face-result")?;

    // Exercise the discoverable route independently from the shortcut. Opening
    // and clicking the real context menu must own the same single edit.
    s.undo()?;
    s.right_click(Control::Viewport)?;
    s.witness(Control::MakeFace)?;
    s.require(
        s.trace.get(Control::MakeFace)?.parents == [Control::ViewportMenu]
            && s.trace.get(Control::MakeFace)?.enabled
            && s.state.editor.document == before,
        "The viewport context menu offers Make Face for the selected circle without editing on open",
    )?;
    s.hover(Control::MakeFace)?;
    s.wait(Duration::from_millis(300))?;
    s.capture_tutorial("make-face-menu")?;
    s.click(Control::MakeFace)?;
    s.require(
        evaluated(s, id)?.faces.len() == 1
            && matches!(
                s.state.editor.document.objects[0].geometry,
                Geometry::Mesh(_)
            ),
        "Clicking Make Face uses the same primitive-materializing edit as the keyboard action",
    )?;
    let after = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    s.hover(Control::Viewport)?;
    s.shortcut("mesh.make-face")?;
    s.require(
        s.state.editor.document == after && s.state.editor.revision == revision,
        "Making the same face again is a no-op rather than a duplicate polygon or geometry update",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == before,
        "A duplicate-face no-op creates no undo entry: one Undo restores the circle recipe",
    )?;
    s.redo()?;
    s.require(
        s.state.editor.document == after,
        "Redo restores the menu-created face in one step",
    )?;

    // Nonplanar fixture setup is intentionally separate from the illustrated
    // Circle workflow. Selection and rejection still use production input.
    let mut invalid = Document::default();
    let invalid_id = invalid.insert_primitive(PrimitiveKind::Plane)?;
    invalid.objects[0].geometry = Geometry::Mesh(EditableMesh {
        vertices: [
            [-1.0, -1.0, 0.0],
            [1.0, -1.0, 0.0],
            [1.0, 1.0, 0.6],
            [-1.0, 1.0, 0.0],
        ]
        .into_iter()
        .enumerate()
        .map(|(index, position)| MeshVertex {
            id: index as u64 + 1,
            position,
        })
        .collect(),
        faces: Vec::new(),
        edges: vec![[1, 2], [2, 3], [3, 4], [4, 1]],
    });
    s.state.new_document();
    s.state.editor = Editor::new(invalid.clone())?;
    s.settle()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.state.editor.select_object(invalid_id)?;
    s.settle()?;
    s.hover(Control::Viewport)?;
    s.shortcut("edit.confirm")?;
    s.shortcut("selection.all")?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_vertices.len() == 4,
        "The rejection example selects all four corners of the nonplanar closed outline",
    )?;
    let revision = s.state.editor.revision;
    s.shortcut("mesh.make-face")?;
    s.require(
        s.state.error.is_some()
            && s.state.editor.document == invalid
            && s.state.editor.revision == revision
            && s.state.editor.selected_vertices.len() == 4,
        "A nonplanar outline reports an error and preserves the exact geometry, revision and selection",
    )
}
