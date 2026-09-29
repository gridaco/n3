use super::{Result, Session, pointer};
use crate::{
    controls::Control,
    document::{self, Geometry, Object, PolyhedronType, PrimitiveKind},
    document_io,
    editor::Tool,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "n3-editing-docs-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("Getting started.n3.json"));
        let _ = std::fs::remove_file(self.0.join("Edited mesh.n3.json"));
        let _ = std::fs::remove_dir(&self.0);
    }
}

fn object(s: &Session<'_>, id: u64) -> Result<Object> {
    s.state
        .editor
        .document
        .objects
        .iter()
        .find(|object| object.id == id)
        .cloned()
        .ok_or_else(|| format!("Document object {id} is missing"))
}

/// The scenario supplies a destination only after the real Save control emits
/// the request. This is the same persistence boundary used by the native host.
fn save_requested(s: &mut Session<'_>, path: PathBuf) -> Result<()> {
    s.require(
        s.state.request_save,
        "Saving requires a live UI save request",
    )?;
    let bytes = document_io::save(&path, &s.state.editor.document, None, false)?;
    s.state.mark_saved(path, bytes);
    s.state.request_save = false;
    s.state.request_save_as = false;
    s.require(
        !s.state.is_dirty(),
        "A successful save marks the current document clean",
    )
}

fn escape(s: &mut Session<'_>) -> Result<()> {
    s.shortcut("cancel")?;
    Ok(())
}

fn enter(s: &mut Session<'_>) -> Result<()> {
    s.shortcut("edit.confirm")?;
    Ok(())
}

fn double_click(s: &mut Session<'_>, position: egui::Pos2) -> Result<()> {
    // Separate this sequence from previous clicks, then send two complete
    // pointer clicks within egui's double-click interval on the scenario clock.
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

fn select_visible_vertex(s: &mut Session<'_>) -> Result<u64> {
    let visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    let (vertex_id, position) = visible
        .into_iter()
        .find(|(_, position)| {
            s.state.viewport_ui_rect.shrink(15.0).contains(*position)
                && !crate::axis_gizmo::bounds(s.state.viewport).contains(*position)
        })
        .ok_or("The mesh has no visible vertex suitable for selection")?;
    s.drag_at(position - egui::vec2(6.0, 6.0), egui::vec2(12.0, 12.0))?;
    s.require(
        s.state.editor.selected_vertices.len() == 1
            && s.state.editor.selected_vertices.contains(&vertex_id),
        "A real selection box selects exactly the visible vertex it encloses",
    )?;
    Ok(vertex_id)
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    let scratch = Scratch::new()?;
    s.click_path(&[Control::N3Menu, Control::FileMenu, Control::New])?;
    s.require(
        s.state.editor.document.objects.is_empty() && !s.state.is_dirty(),
        "New starts an empty, clean document",
    )?;
    s.click(Control::InsertMenu)?;
    s.click(Control::InsertCube)?;
    let id = s
        .state
        .editor
        .selected_object
        .ok_or("The inserted cube was not selected")?;
    let original = object(s, id)?;
    let Geometry::Primitive(original_primitive) = original.geometry.clone() else {
        return Err("Insert Cube did not create a parametric primitive".into());
    };
    s.require(
        original_primitive.kind == PrimitiveKind::Cube
            && s.state.editor.document.objects.len() == 1
            && s.trace.get(Control::PrimitiveX)?.parents == [Control::Inspector],
        "Insert Cube creates and selects a primitive with dimensions in Properties",
    )?;
    s.witness(Control::ObjectList)?;
    s.witness(Control::Inspector)?;
    s.drag(Control::PrimitiveX, egui::vec2(35.0, 0.0))?;
    let changed = object(s, id)?;
    let Geometry::Primitive(primitive) = &changed.geometry else {
        return Err("Changing dimensions discarded primitive parameters".into());
    };
    s.require(
        primitive.kind == PrimitiveKind::Cube
            && primitive.size[0] > original_primitive.size[0]
            && primitive.size[1..] == original_primitive.size[1..]
            && changed.transform == original.transform
            && s.state.is_dirty(),
        "Dragging Width previews the cube immediately and commits one change on release while retaining its other parameters and transform",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.hover(Control::PrimitiveX)?;
    s.capture_tutorial("editing-primitive")?;

    for (tool, name) in [
        (Control::ToolMove, "editing-move-handles"),
        (Control::ToolScale, "editing-scale-handles"),
    ] {
        s.click(tool)?;
        s.hover(Control::TransformX)?;
        s.wait(crate::doc_input::CUE_LIFETIME)?;
        s.capture_tutorial(name)?;
        s.shortcut("view.front")?;
        s.wait(Duration::from_millis(250))?;
        let before = s.state.editor.document.clone();
        let margin = if tool == Control::ToolMove { 10.5 } else { 9.0 };
        let start = s.trace.get(Control::TransformX)?.rect.center() + egui::vec2(margin, 0.0);
        s.drag_at(start, egui::vec2(30.0, 0.0))?;
        s.require(
            s.state.editor.document != before
                && matches!(object(s, id)?.geometry, Geometry::Primitive(_)),
            "Dragging just outside a solid transform endpoint uses its forgiving margin, edits the selected object, and retains its primitive parameters",
        )?;
        s.undo()?;
        s.require(
            s.state.editor.document == before,
            "One Undo restores the complete solid-handle drag",
        )?;
        s.shortcut("view.perspective")?;
        s.wait(Duration::from_millis(250))?;
    }
    s.click(Control::ToolMove)?;

    let primitive_document = s.state.editor.document.clone();
    s.click_path(&[Control::N3Menu, Control::FileMenu, Control::Save])?;
    let primitive_path = scratch.0.join("Getting started.n3.json");
    save_requested(s, primitive_path.clone())?;
    let reopened = document::load(&primitive_path)?;
    s.require(
        reopened == primitive_document,
        "Saving and reopening preserves the exact primitive parameters and object transform",
    )?;
    s.state.install_document(primitive_path, reopened)?;
    s.settle()?;
    s.click(Control::Viewport)?;
    s.require(
        s.state.editor.selected_object == Some(id) && object(s, id)? == changed,
        "A real viewport click selects the reopened primitive with its parameters intact",
    )?;
    let evaluated = s.state.editor.document.eval_object(id)?;
    let entry_revision = s.state.editor.revision;
    enter(s)?;
    s.require(
        s.state.editor.edit_mode
            && object(s, id)? == changed
            && s.state.editor.revision == entry_revision
            && !s.state.is_dirty()
            && (s.state.viewport.bottom()
                - s.trace.get(Control::EditToolbar)?.rect.bottom()
                - crate::theme::space::XL)
                .abs()
                < 1.0
            && s.trace.get(Control::LeaveEdit)?.parents == [Control::EditToolbar],
        "Confirm exposes primitive vertices without changing the geometry definition, revision, or saved state",
    )?;
    s.witness(Control::EditToolbar)?;
    select_visible_vertex(s)?;
    s.require(
        object(s, id)? == changed && s.state.editor.revision == entry_revision,
        "Selecting evaluated vertices does not convert the primitive or invalidate geometry",
    )?;
    s.callout(
        Control::LeaveEdit,
        "Inspect or select vertices freely. The cube stays parametric until you change its geometry.",
    )?;
    s.capture_tutorial("editing-primitive-vertices")?;
    s.clear_callout()?;
    enter(s)?;
    s.require(
        !s.state.editor.edit_mode
            && object(s, id)? == changed
            && s.state.editor.revision == entry_revision
            && !s.state.is_dirty()
            && s.trace.get(Control::PrimitiveX).is_ok(),
        "Entering, selecting, and leaving restores the original Shape controls without a document change",
    )?;
    s.undo()?;
    s.require(
        object(s, id)? == changed && s.state.editor.revision == entry_revision,
        "An unchanged primitive edit visit creates no undo entry",
    )?;

    s.click(Control::ToolView)?;
    double_click(s, s.state.viewport.center())?;
    s.require(
        s.state.editor.edit_mode && object(s, id)? == changed,
        "Double-clicking a primitive enters vertex editing without conversion",
    )?;
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(20.0, -20.0);
    double_click(s, empty)?;
    s.require(
        !s.state.editor.edit_mode
            && object(s, id)? == changed
            && s.state.editor.revision == entry_revision,
        "Double-clicking empty space leaves an unchanged primitive intact",
    )?;

    // An ordinary vertex operation owns conversion and history together. Edit
    // mode is not a pending transaction, so Undo can return to the live recipe.
    enter(s)?;
    select_visible_vertex(s)?;
    s.click(Control::ToolMove)?;
    s.drag(Control::TransformX, egui::vec2(28.0, 0.0))?;
    s.require(
        s.state.editor.edit_mode
            && matches!(object(s, id)?.geometry, Geometry::Mesh(_))
            && s.state.is_dirty(),
        "The first effective vertex drag materializes an editable mesh as part of that operation",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.edit_mode && object(s, id)? == changed && !s.state.is_dirty(),
        "One Undo restores the exact primitive recipe while keeping vertex edit mode available",
    )?;
    s.shortcut("transform.axis-x")?;
    // Exact numeric values avoid grid alignment moving an initially off-grid
    // vertex. Changing the total amount back to zero restores the baseline.
    s.key(egui::Key::Num1, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Num1, false, egui::Modifiers::NONE)?;
    s.require(
        s.state.editor.has_transform_session()
            && matches!(object(s, id)?.geometry, Geometry::Mesh(_)),
        "A numeric vertex move previews the edited representation through the same session",
    )?;
    s.key(egui::Key::Backspace, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Backspace, false, egui::Modifiers::NONE)?;
    s.key(egui::Key::Num0, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Num0, false, egui::Modifiers::NONE)?;
    enter(s)?;
    s.require(
        s.state.editor.edit_mode
            && !s.state.editor.has_transform_session()
            && object(s, id)? == changed
            && !s.state.is_dirty(),
        "Returning the numeric move to zero restores the source recipe without requiring Undo",
    )?;
    enter(s)?;
    s.require(
        !s.state.editor.edit_mode
            && object(s, id)? == changed
            && s.trace.get(Control::PrimitiveX).is_ok()
            && !s.state.is_dirty(),
        "Leaving after Undo retains the original primitive and restores its editable Shape parameters",
    )?;
    enter(s)?;
    s.require(
        s.state.editor.edit_mode && object(s, id)? == changed,
        "Confirm enters the same non-destructive primitive edit mode as double-click",
    )?;

    // Selection and transformation both use the same actual pointer path as
    // the editor. Geometry is inspected only to choose a visible tutorial target.
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(20.0, -20.0);
    s.click_at(empty)?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_vertices.is_empty(),
        "Clicking empty viewport space clears vertex selection without leaving edit mode",
    )?;
    let visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    let (vertex_id, position) = visible
        .into_iter()
        .find(|(_, position)| {
            s.state.viewport_ui_rect.shrink(15.0).contains(*position)
                && !crate::axis_gizmo::bounds(s.state.viewport).contains(*position)
        })
        .ok_or("The cube has no visible vertex suitable for the selection tutorial")?;
    s.drag_at(position - egui::vec2(6.0, 6.0), egui::vec2(12.0, 12.0))?;
    s.require(
        s.state.editor.selected_vertices.len() == 1
            && s.state.editor.selected_vertices.contains(&vertex_id),
        "A real selection box selects exactly the visible vertex it encloses",
    )?;
    s.click(Control::ToolMove)?;
    s.require(
        s.state.editor.tool == Tool::Move,
        "Move selects the translation handles",
    )?;
    let before_move = s.state.editor.document.clone();
    s.hover(Control::TransformX)?;
    let handle = s.trace.get(Control::TransformX)?.rect.center();
    s.frame(vec![pointer(handle, true)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerMoved(handle + egui::vec2(28.0, 0.0))],
        Duration::ZERO,
    )?;
    s.require(
        s.input.is_pressed(egui::PointerButton::Primary) && s.state.editor.document != before_move,
        "Dragging the live X handle previews a vertex translation while the pointer is held",
    )?;
    s.capture_tutorial("editing-move-vertices")?;
    let preview = s.state.editor.document.clone();
    enter(s)?;
    s.require(
        !s.state.editor.is_interacting()
            && s.state.editor.edit_mode
            && s.state.editor.document == preview
            && s.input.is_pressed(egui::PointerButton::Primary),
        "Confirm finishes the held transform at its visible preview without leaving vertex mode",
    )?;
    s.frame(
        vec![pointer(handle + egui::vec2(28.0, 0.0), false)],
        Duration::ZERO,
    )?;
    s.settle()?;
    s.require(
        s.state.editor.document == preview && !s.state.editor.is_interacting(),
        "Releasing the pointer after Confirm leaves the committed transform unchanged",
    )?;
    enter(s)?;
    s.require(
        !s.state.editor.edit_mode
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.tool == Tool::Move
            && s.state.editor.document == preview
            && matches!(object(s, id)?.geometry, Geometry::Mesh(_))
            && s.trace.get(Control::PrimitiveX).is_err(),
        "Leaving a changed primitive keeps the mesh, selection and tool; obsolete primitive parameters disappear",
    )?;
    enter(s)?;
    let after_move = s.state.editor.document.clone();
    let Geometry::Mesh(moved) = object(s, id)?.geometry else {
        return Err("Moving vertices changed the object's geometry kind".into());
    };
    let only_selected_moved = evaluated.vertices.iter().all(|before| {
        moved
            .vertices
            .iter()
            .find(|after| after.id == before.id)
            .is_some_and(|after| {
                if before.id == vertex_id {
                    (after.position[0] - before.position[0]).abs() > 1e-6
                        && (after.position[1] - before.position[1]).abs() < 1e-9
                        && (after.position[2] - before.position[2]).abs() < 1e-9
                } else {
                    after == before
                }
            })
    });
    s.require(
        only_selected_moved && moved.faces == evaluated.faces,
        "The X handle moves only the selected vertex along X and preserves polygon topology",
    )?;
    s.click(Control::LeaveEdit)?;
    s.require(
        !s.state.editor.edit_mode,
        "Object mode leaves vertex editing",
    )?;
    let dirty = s.state.is_dirty();
    enter(s)?;
    s.require(
        s.state.editor.edit_mode
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.tool == Tool::Move
            && s.state.editor.document == after_move
            && s.state.is_dirty() == dirty,
        "Idle Confirm enters a selected mesh without changing its document, tool, or saved state",
    )?;
    enter(s)?;
    s.require(
        !s.state.editor.edit_mode
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.tool == Tool::Move
            && s.state.editor.document == after_move
            && s.state.is_dirty() == dirty,
        "A second idle Confirm restores object mode with the same object selection and tool",
    )?;

    // The cursor tool hides handles so both clicks target the actual surface.
    s.click(Control::ToolView)?;
    let surface = s.state.viewport.center();
    let visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    s.require(
        !visible.is_empty()
            && visible
                .iter()
                .all(|(_, position)| position.distance(surface) > 15.0),
        "The surface double-click target is away from visible vertices",
    )?;
    double_click(s, surface)?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_object == Some(id),
        "Two timed viewport clicks on the mesh surface enter vertex mode",
    )?;
    double_click(s, surface)?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_object == Some(id),
        "Double-clicking mesh surface away from vertices stays in vertex mode",
    )?;
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(20.0, -20.0);
    double_click(s, empty)?;
    s.require(
        !s.state.editor.edit_mode
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.tool == Tool::View
            && s.state.editor.document == after_move
            && s.state.is_dirty() == dirty,
        "Double-clicking actual empty viewport leaves vertex mode and retains the object, tool, and document",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == before_move,
        "One Undo reverses the confirmed drag: its later release and mode switches add no history entries",
    )?;
    s.redo()?;
    s.require(
        s.state.editor.document == after_move,
        "Redo restores the committed vertex translation exactly",
    )?;

    let tool_selection = s.state.editor.selected_objects.clone();
    let tool_active = s.state.editor.selected_object;
    let tool_vertices = s.state.editor.selected_vertices.clone();
    let tool_edit_mode = s.state.editor.edit_mode;
    let tool_revision = s.state.editor.revision;
    let tool_camera = s.state.camera.view_projection(s.state.aspect());
    let tool_dirty = s.state.is_dirty();
    for (control, binding, tool) in [
        (Control::ToolView, "tool.cursor", Tool::View),
        (Control::ToolView, "tool.cursor-alternate", Tool::View),
        (Control::ToolMove, "tool.move", Tool::Move),
        (Control::ToolRotate, "tool.rotate", Tool::Rotate),
        (Control::ToolScale, "tool.scale", Tool::Scale),
    ] {
        s.click(control)?;
        s.require(
            s.state.editor.tool == tool,
            "The live tool button selects its editor tool",
        )?;
        s.click(if tool == Tool::Move {
            Control::ToolView
        } else {
            Control::ToolMove
        })?;
        let open = s.shortcut_down(binding)?;
        s.shortcut_up(binding)?;
        s.require(
            !open && s.state.editor.tool == tool,
            "The actual shortcut key selects the same editor tool",
        )?;
        s.require(
            s.state.editor.document == after_move
                && s.state.editor.revision == tool_revision
                && s.state.editor.selected_objects == tool_selection
                && s.state.editor.selected_object == tool_active
                && s.state.editor.selected_vertices == tool_vertices
                && s.state.editor.edit_mode == tool_edit_mode
                && s.state.camera.view_projection(s.state.aspect()) == tool_camera
                && s.state.is_dirty() == tool_dirty,
            "Tool buttons and keys preserve geometry, selection, editing mode, camera and saved state",
        )?;
        if tool == Tool::View {
            s.require(
                [
                    Control::TransformX,
                    Control::TransformY,
                    Control::TransformZ,
                    Control::TransformXY,
                    Control::TransformXZ,
                    Control::TransformYZ,
                    Control::TransformUniform,
                ]
                .iter()
                .all(|control| s.trace.get(*control).is_err()),
                "V and Q select the same cursor tool with transform handles hidden",
            )?;
        }
    }
    s.undo()?;
    s.require(
        s.state.editor.document == before_move,
        "Switching tools with V, Q, W, E or R creates no history entry before the previous vertex move",
    )?;
    s.redo()?;
    s.require(
        s.state.editor.document == after_move,
        "Redo restores the vertex move after the tool-alias history check",
    )?;
    s.click_path(&[Control::N3Menu, Control::FileMenu, Control::SaveAs])?;
    s.require(
        s.state.request_save && s.state.request_save_as && s.state.editor.document == after_move,
        "Save as requests a destination without changing the edited document",
    )?;
    let edited_path = scratch.0.join("Edited mesh.n3.json");
    save_requested(s, edited_path.clone())?;
    let reopened = document::load(&edited_path)?;
    s.require(
        reopened == after_move && document::load(&scratch.0.join("Getting started.n3.json"))? == primitive_document,
        "Save as preserves editable vertex IDs, positions, faces and transforms without replacing the earlier primitive document",
    )?;

    // Verify every documented primitive through its own live menu command.
    // Undo each insertion so the existing edited object remains the baseline.
    s.key(egui::Key::I, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::I, false, egui::Modifiers::NONE)?;
    s.require(
        s.trace.get(Control::InsertCube).is_err() && s.state.editor.document == after_move,
        "Plain I is unassigned and does not open Insert or change geometry",
    )?;
    for (control, kind) in [
        (Control::InsertPlane, PrimitiveKind::Plane),
        (Control::InsertCircle, PrimitiveKind::Circle),
        (Control::InsertCylinder, PrimitiveKind::Cylinder),
        (Control::InsertCone, PrimitiveKind::Cone),
        (Control::InsertTorus, PrimitiveKind::Torus),
        (Control::InsertSphere, PrimitiveKind::Sphere),
    ] {
        let selection = s.state.editor.selected_objects.clone();
        s.shortcut("insert.open")?;
        s.require(
            s.trace.get(control)?.parents == [Control::InsertMenu]
                && s.state.editor.document == after_move
                && s.state.editor.selected_objects == selection
                && !s.state.is_dirty(),
            "The Insert shortcut opens the existing Insert menu without changing geometry, selection or saved state",
        )?;
        s.click(control)?;
        let inserted = s
            .state
            .editor
            .selected_object
            .ok_or("The new primitive was not selected")?;
        s.require(
            s.state.editor.document.objects.len() == after_move.objects.len() + 1
                && matches!(object(s, inserted)?.geometry, Geometry::Primitive(ref primitive) if primitive.kind == kind)
                && !s.state.editor.document.eval_object(inserted)?.vertices.is_empty(),
            &format!("{} inserts and selects its own evaluated parametric shape", control.label()),
        )?;
        s.undo()?;
        s.require(
            s.state.editor.document == after_move,
            "Undo removes the inserted primitive without changing the existing editable mesh",
        )?;
    }
    s.shortcut("insert.open")?;
    s.require(
        s.trace.get(Control::InsertPolyhedron)?.parents == [Control::InsertMenu]
            && s.state.editor.document == after_move,
        "Polyhedron is available among Insert primitives without changing the document",
    )?;
    s.click(Control::InsertPolyhedron)?;
    let inserted = s
        .state
        .editor
        .selected_object
        .ok_or("The polyhedron was not selected")?;
    s.require(
        s.state.editor.document.objects.len() == after_move.objects.len() + 1
            && matches!(object(s, inserted)?.geometry, Geometry::Primitive(ref primitive)
                if primitive.kind == PrimitiveKind::Polyhedron
                    && primitive.polyhedron_type == Some(PolyhedronType::Icosahedron))
            && s.state.editor.document.eval_object(inserted)?.faces.len() == 20,
        "Insert creates one selected parametric Polyhedron, initially an icosahedron",
    )?;
    s.shortcut("view.local")?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.require(
        s.state.is_local_view(),
        "Local View isolates the new Polyhedron for its type preview",
    )?;
    s.click(Control::PolyhedronTypeMenu)?;
    let presets = [
        (
            Control::PolyhedronTetrahedron,
            PolyhedronType::Tetrahedron,
            4,
        ),
        (Control::PolyhedronCube, PolyhedronType::Cube, 6),
        (Control::PolyhedronOctahedron, PolyhedronType::Octahedron, 8),
        (
            Control::PolyhedronDodecahedron,
            PolyhedronType::Dodecahedron,
            12,
        ),
        (
            Control::PolyhedronIcosahedron,
            PolyhedronType::Icosahedron,
            20,
        ),
    ];
    for (control, _, faces) in presets {
        let item = s.trace.get(control)?;
        s.require(
            item.parents.contains(&Control::PolyhedronTypeMenu)
                && item.label.ends_with(&format!("({faces} faces)")),
            "Properties names each of the five regular solids with its face count",
        )?;
        s.witness(control)?;
    }
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_image("editing-polyhedron-type")?;
    escape(s)?;
    let original = object(s, inserted)?;
    let inspector = s.trace.get(Control::Inspector)?.rect;
    for (control, kind, faces) in presets {
        s.click(Control::PolyhedronTypeMenu)?;
        s.click(control)?;
        let current = object(s, inserted)?;
        s.require(
            current.id == original.id
                && current.name == original.name
                && current.transform == original.transform
                && matches!(current.geometry, Geometry::Primitive(ref primitive)
                    if primitive.kind == PrimitiveKind::Polyhedron
                        && primitive.polyhedron_type == Some(kind))
                && s.state.editor.document.eval_object(inserted)?.faces.len() == faces
                && s.trace.get(Control::Inspector)?.rect == inspector
                && inspector.contains_rect(s.trace.get(Control::PolyhedronTypeMenu)?.rect),
            "Choosing a regular-solid preset preserves the object and panel layout and produces its advertised face count",
        )?;
        if kind != PolyhedronType::Icosahedron {
            s.undo()?;
            s.require(
                object(s, inserted)? == original,
                "Undo restores the previous Polyhedron recipe in one step",
            )?;
        }
    }
    s.shortcut("view.local")?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.undo()?;
    s.require(
        s.state.editor.document == after_move,
        "Reselecting the current preset creates no undo step; Undo removes the inserted Polyhedron",
    )?;
    s.click(Control::NavigationPlanar)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.shortcut("insert.open")?;
    s.click(Control::InsertPlane)?;
    let plane_id = s
        .state
        .editor
        .selected_object
        .ok_or("The 2D plane was not selected")?;
    let plane = object(s, plane_id)?;
    let normal = glam::DQuat::from_array(plane.transform.rotation) * glam::DVec3::Z;
    let source_view = crate::orientation::display_rotation(s.state.z_up)
        .inverse()
        .transform_vector3(s.state.camera.nearest_axis_direction())
        .as_dvec3();
    s.require(
        s.state.is_planar_navigation()
            && s.state.editor.document.eval_object(plane_id)?.faces.len() == 1
            && normal.distance(source_view) < 1e-6,
        "Inserting Plane in 2D faces the active view and keeps planar navigation",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == after_move,
        "Undo removes the 2D plane",
    )?;
    s.click(Control::NavigationFree)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.state.frame_all();
    s.settle()?;
    let insert_selection = s.state.editor.selected_objects.clone();
    s.shortcut_down("insert.open")?;
    let mut repeat_event = s.shortcut_event("insert.open", true)?;
    if let egui::Event::Key {
        repeat,
        physical_key,
        ..
    } = &mut repeat_event
    {
        *repeat = true;
        *physical_key = None;
    }
    s.frame(vec![repeat_event], Duration::ZERO)?;
    s.settle()?;
    s.require(
        s.trace.get(Control::InsertTorus)?.parents == [Control::InsertMenu]
            && s.state.editor.document == after_move
            && s.state.editor.selected_objects == insert_selection,
        "Repeating the held Insert shortcut keeps the existing Insert menu open without inserting or toggling it",
    )?;
    // Keep every icon and its label visible in the menu capture. The decorative
    // glyph must not replace the name used by the guide and keyboard controls.
    for control in [
        Control::InsertCube,
        Control::InsertCylinder,
        Control::InsertCone,
        Control::InsertTorus,
        Control::InsertPlane,
        Control::InsertCircle,
        Control::InsertSphere,
        Control::InsertPolyhedron,
    ] {
        s.require(
            s.trace.get(control)?.label == control.label(),
            "Insert items retain their shape names alongside decorative icons",
        )?;
    }
    s.hover(Control::InsertTorus)?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("editing-insert-menu")?;
    s.require(
        s.shortcut_is_down("insert.open")?,
        "The Insert tutorial preserves the held Insert shortcut",
    )?;
    s.shortcut_up("insert.open")?;

    let saved = s.state.editor.document.clone();
    s.require(
        !s.state.is_dirty(),
        "The Cancel walkthrough starts from a saved document",
    )?;
    escape(s)?;
    s.require(
        s.trace.get(Control::InsertTorus).is_err()
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.selected_objects == insert_selection
            && s.state.editor.document == saved,
        "Cancel closes the Insert popup without deselecting the object or changing the document",
    )?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.witness(Control::LengthUnitMenu)?;
    escape(s)?;
    s.require(
        !s.state.show_preferences
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.document == saved,
        "Cancel closes unfocused Preferences before changing editor selection",
    )?;
    s.click(Control::ToolMove)?;
    enter(s)?;
    select_visible_vertex(s)?;
    s.shortcut("edit.leave")?;
    s.require(
        !s.state.editor.edit_mode
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.selected_vertices.is_empty(),
        "Leave edit exits vertex editing directly while retaining object selection",
    )?;
    enter(s)?;
    let selected_vertex = select_visible_vertex(s)?;
    s.hover(Control::TransformX)?;
    let start = s.trace.get(Control::TransformX)?.rect.center();
    // Move far enough to cross the default centimeter grid before cancelling.
    let end = start + egui::vec2(120.0, 0.0);
    s.frame(vec![pointer(start, true)], Duration::ZERO)?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    s.require(
        s.state.editor.is_transforming() && s.state.editor.document != saved,
        "The Cancel walkthrough starts a real unfinished transform",
    )?;
    escape(s)?;
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.settle()?;
    s.require(
        !s.state.editor.is_interacting() && s.state.editor.edit_mode
            && s.state.editor.selected_vertices.contains(&selected_vertex)
            && s.state.editor.document == saved && !s.state.is_dirty(),
        "Cancel discards the active transform and restores the saved geometry before clearing selection",
    )?;
    let marquee_start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(20.0, -20.0);
    let marquee_end = marquee_start + egui::vec2(25.0, -25.0);
    s.frame(
        vec![egui::Event::PointerMoved(marquee_start)],
        Duration::ZERO,
    )?;
    s.frame(vec![pointer(marquee_start, true)], Duration::ZERO)?;
    s.frame(vec![egui::Event::PointerMoved(marquee_end)], Duration::ZERO)?;
    s.require(
        s.state.editor.is_interacting()
            && !s.state.editor.is_transforming()
            && s.state.editor.selected_vertices.len() == 1
            && s.state.editor.selected_vertices.contains(&selected_vertex),
        "An unfinished empty-space marquee preserves the vertex selection until it is accepted",
    )?;
    escape(s)?;
    s.frame(vec![pointer(marquee_end, false)], Duration::ZERO)?;
    s.settle()?;
    s.require(
        !s.state.editor.is_interacting()
            && s.state.editor.edit_mode
            && s.state.editor.selected_vertices.len() == 1
            && s.state.editor.selected_vertices.contains(&selected_vertex)
            && s.state.editor.document == saved
            && !s.state.is_dirty(),
        "Cancel discards an active marquee and restores the previous selection before deselection",
    )?;
    escape(s)?;
    s.require(
        s.state.editor.edit_mode
            && s.state.editor.selected_vertices.is_empty()
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.document == saved
            && !s.state.is_dirty(),
        "The next Cancel clears selected vertices while staying in vertex edit mode",
    )?;
    escape(s)?;
    s.require(
        !s.state.editor.edit_mode
            && s.state.editor.selected_object == Some(id)
            && s.state.editor.document == saved
            && !s.state.is_dirty(),
        "The next Cancel leaves vertex edit mode while retaining the object selection",
    )?;
    s.shortcut_down("cancel")?;
    let no_object_properties = Control::ALL
        .iter()
        .filter_map(|control| s.trace.get(*control).ok())
        .all(|observed| !observed.parents.contains(&Control::Inspector));
    let no_handles = [
        Control::TransformX,
        Control::TransformY,
        Control::TransformZ,
        Control::TransformXY,
        Control::TransformXZ,
        Control::TransformYZ,
    ]
    .iter()
    .all(|control| s.trace.get(*control).is_err());
    s.require(
        s.state.editor.selected_object.is_none() && s.state.editor.selected_vertices.is_empty()
            && !s.state.editor.edit_mode && s.trace.get(Control::Inspector).is_ok()
            && no_object_properties && no_handles
            && s.trace.get(Control::LengthUnitMenu).is_err()
            && s.state.editor.document == saved && !s.state.is_dirty(),
        "The next Cancel deselects the object and hides object properties and transform handles without changing the document or display-unit preference",
    )?;

    s.capture_tutorial("editing-no-selection")?;
    s.shortcut_up("cancel")?;
    escape(s)?;
    s.require(
        s.state.editor.selected_object.is_none()
            && s.state.editor.selected_vertices.is_empty()
            && !s.state.editor.edit_mode
            && !s.state.editor.is_interacting()
            && s.state.editor.document == saved
            && !s.state.is_dirty(),
        "Cancel with no selection or active interaction leaves the document and session unchanged",
    )?;
    let tool = s.state.editor.tool;
    enter(s)?;
    s.require(
        s.state.editor.selected_object.is_none()
            && !s.state.editor.edit_mode
            && s.state.editor.tool == tool
            && s.state.editor.document == saved
            && !s.state.is_dirty(),
        "Idle Confirm with no selected object is a no-op",
    )?;

    Ok(())
}
