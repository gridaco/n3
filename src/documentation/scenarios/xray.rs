//! X-ray expands selection through surfaces without changing its object scope.
use super::{ClipSpec, Result, Session};
use crate::{
    controls::Control,
    document::{Document, Geometry, PrimitiveKind},
    editor::Editor,
    render::shading::ShadingMode,
};
use egui::{PointerButton, Rect};
use std::{collections::BTreeSet, time::Duration};

fn candidates(s: &mut Session<'_>) -> Result<Vec<(u64, egui::Pos2)>> {
    s.state
        .editor
        .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)
}

fn front(s: &mut Session<'_>) -> Result<()> {
    s.shortcut("view.front")?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.require(
        s.state.is_planar_navigation() && s.state.ruler_2d_model.is_some(),
        "Front view enters 2D navigation with rulers before selecting",
    )
}

fn box_drag(s: &mut Session<'_>, bounds: Rect, expected: usize) -> Result<()> {
    let before = s.state.editor.selected_vertices.clone();
    s.move_pointer(bounds.min, Duration::from_millis(250))?;
    s.pointer_button(PointerButton::Primary, true)?;
    s.move_pointer(bounds.max, Duration::from_millis(750))?;
    s.require(
        s.state.editor.is_interacting() && s.state.editor.selected_vertices == before,
        "The selection box keeps the old vertex selection until pointer release",
    )?;
    s.wait(Duration::from_millis(200))?;
    s.pointer_button(PointerButton::Primary, false)?;
    s.require(
        !s.state.editor.is_interacting() && s.state.editor.selected_vertices.len() == expected,
        &format!("Releasing the same selection box selects exactly {expected} cube vertices"),
    )
}

fn orbit_to_reveal(s: &mut Session<'_>, start: egui::Pos2) -> Result<()> {
    let selected = s.state.editor.selected_vertices.clone();
    let orientation = s.state.camera.orientation();
    s.require(
        s.state.is_planar_navigation(),
        "Each selection reveal starts from the same 2D view",
    )?;
    s.move_pointer(start, Duration::from_millis(250))?;
    s.pointer_button(PointerButton::Secondary, true)?;
    let delta = egui::vec2(-75.0, 45.0);
    s.move_pointer(start + delta * 0.5, Duration::from_millis(450))?;
    s.require(
        s.input.is_pressed(PointerButton::Secondary)
            && !s.state.is_planar_navigation()
            && s.state.ruler_2d_model.is_none()
            && s.state.camera.orientation() != orientation
            && s.state.editor.selected_vertices == selected,
        "The held right drag exits 2D, hides rulers and orbits without changing the selection",
    )?;
    s.move_pointer(start + delta, Duration::from_millis(450))?;
    s.pointer_button(PointerButton::Secondary, false)?;
    s.require(
        !s.state.is_planar_navigation()
            && s.state.camera.is_orthographic()
            && s.state.editor.selected_vertices == selected,
        "Releasing the orbit keeps the original selection and orthographic projection in the revealed 3D view",
    )
}

fn close_view_menu(s: &mut Session<'_>) -> Result<()> {
    if s.trace.get(Control::Xray).is_ok() {
        s.shortcut("cancel")?;
    }
    s.require(
        s.trace.get(Control::Xray).is_err(),
        "The View menu has closed before viewport input resumes",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    // Prepare an intact procedural cube. All illustrated toggles, selection,
    // orbit, and shading changes below use the production input path.
    let mut document = Document::default();
    let cube = document.insert_primitive(PrimitiveKind::Cube)?;
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click_at(s.state.viewport.center())?;
    s.shortcut("tool.cursor")?;
    front(s)?;
    s.hover(Control::Viewport)?;
    s.pinch(-0.2)?;
    s.shortcut("edit.confirm")?;
    s.witness(Control::Viewport)?;
    s.witness(Control::ToolView)?;
    let revision = s.state.editor.revision;
    let render_revision = s.state.mesh_revision;
    let dirty = s.state.is_dirty();
    let front_vertices = candidates(s)?;
    s.require(
        s.state.editor.edit_mode
            && !s.state.editor.xray_enabled()
            && s.state.shading == ShadingMode::Solid
            && front_vertices.len() == 4
            && s.state.editor.selected_vertices.is_empty(),
        "X-ray starts off, exposing only the four front vertices of the unmodified primitive cube",
    )?;
    let mut bounds = Rect::NOTHING;
    for (_, point) in &front_vertices {
        bounds.extend_with(*point);
    }
    let bounds = bounds.expand(18.0);
    s.require(
        s.state.viewport_ui_rect.contains_rect(bounds)
            && !crate::axis_gizmo::bounds(s.state.viewport).intersects(bounds),
        "The tutorial's box surrounds the cube clear of viewport controls",
    )?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_clip("xray-box-selection", ClipSpec::default(), |s| {
        s.callout(
            Control::ToolView,
            "X-ray off: select the four front vertices.",
        )?;
        s.wait(Duration::from_millis(600))?;
        box_drag(s, bounds, 4)?;
        let front_selection = s.state.editor.selected_vertices.clone();
        s.wait(Duration::from_millis(450))?;
        s.callout(
            Control::ToolView,
            "Right-drag to orbit. Only the four front vertices are selected.",
        )?;
        orbit_to_reveal(s, bounds.max)?;
        let revealed = candidates(s)?;
        s.require(
            revealed.iter().any(|(id, _)| !front_selection.contains(id)),
            "Orbiting with X-ray off reveals unselected rear vertices beside the selected front face",
        )?;
        s.wait(Duration::from_millis(1500))?;
        s.callout(
            Control::ToolView,
            &format!("{} returns to Front. Try the same box with X-ray on.", s.shortcut_label("view.front")?),
        )?;
        front(s)?;
        let restored = candidates(s)?;
        s.require(
            restored.len() == front_vertices.len()
                && front_vertices.iter().all(|(id, point)| {
                    restored.iter().any(|(other, restored_point)| {
                        other == id && restored_point.distance(*point) < 0.01
                    })
                }),
            "Returning to Front restores the original framing for the same selection box",
        )?;
        s.wait(Duration::from_millis(450))?;
        s.shortcut("cancel")?;
        s.require(
            s.state.editor.edit_mode && s.state.editor.selected_vertices.is_empty(),
            "Deselecting the front vertices leaves vertex edit mode active",
        )?;
        s.shortcut("view.xray")?;
        let candidate_count = candidates(s)?.len();
        s.require(
            s.state.editor.xray_enabled() && candidate_count == 8,
            "The X-ray shortcut exposes all eight vertices before selecting them",
        )?;
        s.callout(
            Control::ToolView,
            &format!(
                "{} enables X-ray. The same box now reaches all eight vertices.",
                s.shortcut_label("view.xray")?
            ),
        )?;
        s.wait(Duration::from_millis(450))?;
        box_drag(s, bounds, 8)?;
        s.wait(Duration::from_millis(450))?;
        s.callout(
            Control::ToolView,
            "Orbit again: all eight vertices are selected, including the rear corners.",
        )?;
        orbit_to_reveal(s, bounds.max)?;
        let revealed = candidates(s)?;
        s.require(
            s.state.editor.xray_enabled()
                && revealed.len() == 8
                && revealed.iter().enumerate().all(|(index, (id, point))| {
                    s.state.editor.selected_vertices.contains(id)
                        && revealed[index + 1..].iter().all(|(_, other)| point.distance(*other) > 12.0)
                }),
            "Orbiting with X-ray exposes eight selected, visibly separated front and rear corners",
        )?;
        s.wait(Duration::from_millis(1500))?;
        s.clear_callout()
    })?;
    s.value("front-count", 4);
    s.value("through-count", 8);
    let all_vertices = s.state.editor.selected_vertices.clone();

    // Reuse the clip's final oblique view for the still rather than orbiting twice.
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("xray-solid")?;

    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.witness(Control::Xray)?;
    s.require(
        s.trace.get(Control::Xray)?.parents == [Control::N3Menu, Control::ViewMenu],
        "The X-ray checkbox lives in the actual View menu",
    )?;
    s.wait(Duration::from_millis(250))?;
    s.capture_tutorial("xray-menu")?;
    s.click(Control::Xray)?;
    close_view_menu(s)?;
    s.require(
        !s.state.editor.xray_enabled() && s.state.editor.selected_vertices == all_vertices,
        "Disabling X-ray through its menu preserves every selected rear vertex",
    )?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("xray-selection-retained")?;

    s.hover(Control::Viewport)?;
    s.shortcut("view.xray")?;
    s.require(
        s.state.editor.xray_enabled(),
        &format!(
            "After closing the View menu, the viewport X-ray shortcut enables X-ray (focus={:?}, pointer={:?}, interacting={})",
            s.ctx.memory(|memory| memory.focused()),
            s.input.position(),
            s.state.editor.is_interacting(),
        ),
    )?;
    s.shortcut_down("shading.pie")?;
    s.hover(Control::PieWireframe)?;
    s.shortcut_up("shading.pie")?;
    let candidate_count = candidates(s)?.len();
    s.require(
        s.state.shading == ShadingMode::Wireframe
            && s.state.editor.xray_enabled()
            && candidate_count == 8
            && s.state.editor.selected_vertices == all_vertices,
        &format!(
            "Wireframe retains the independent X-ray state and through-selection policy (shading={:?}, xray={}, candidates={candidate_count}, selected={:?}, expected={all_vertices:?})",
            s.state.shading,
            s.state.editor.xray_enabled(),
            s.state.editor.selected_vertices,
        ),
    )?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("xray-wireframe")?;
    s.shortcut("view.xray")?;
    let candidate_count = candidates(s)?.len();
    s.require(
        !s.state.editor.xray_enabled()
            && s.state.shading == ShadingMode::Wireframe
            && s.state.editor.selected_vertices == all_vertices
            && candidate_count < 8,
        "Disabling X-ray in Wireframe restores occlusion for picking without changing shading or selection",
    )?;
    s.shortcut_down("shading.pie")?;
    s.hover(Control::PieSolid)?;
    s.shortcut_up("shading.pie")?;
    front(s)?;
    s.shortcut("view.xray")?;
    s.wait(Duration::from_millis(400))?;
    s.click_at(front_vertices[0].1)?;
    s.require(
        s.state.editor.selected_vertices == BTreeSet::from([front_vertices[0].0]),
        "Clicking overlapping front and back corners selects only the nearer vertex",
    )?;
    s.shortcut("selection.all")?;
    s.require(
        s.state.editor.selected_vertices == all_vertices,
        "Select All reaches every vertex of the edited object while X-ray is enabled",
    )?;
    let mut visited = BTreeSet::new();
    for _ in 0..8 {
        s.shortcut("selection.next")?;
        s.require(
            s.state.editor.selected_vertices.len() == 1,
            "Each vertex cycle chooses exactly one vertex with X-ray enabled",
        )?;
        visited.extend(&s.state.editor.selected_vertices);
    }
    s.require(
        visited == all_vertices,
        "Vertex cycling reaches all eight vertices, including the occluded rear corners",
    )?;
    s.shortcut("edit.leave")?;
    s.undo()?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.mesh_revision == render_revision
            && s.state.is_dirty() == dirty
            && matches!(s.state.editor.document.objects[0].geometry, Geometry::Primitive(_)),
        "X-ray, shading, selection and leaving edit mode preserve the primitive and create no geometry upload or undo entry",
    )?;
    s.require(
        s.state.editor.selected_object == Some(cube),
        "Leaving vertex editing keeps the same object selected",
    )?;
    verify_object_scope(s)
}

fn verify_object_scope(s: &mut Session<'_>) -> Result<()> {
    // Two overlapping cubes test occlusion separately from Local View's scope.
    let mut document = Document::default();
    let near = document.insert_primitive(PrimitiveKind::Cube)?;
    let far = document.insert_primitive(PrimitiveKind::Cube)?;
    for (object, z) in document.objects.iter_mut().zip([2.0, -2.0]) {
        object.transform.translation[2] = z;
    }
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    front(s)?;
    s.hover(Control::Viewport)?;
    let bounds =
        s.state
            .editor
            .object_selection_bounds(s.state.viewport, &s.state.camera, s.state.z_up)?;
    s.require(
        bounds.len() == 1 && bounds[0].0 == near,
        "With X-ray off the rear cube is occluded and excluded from object box selection",
    )?;
    let selection_box = bounds[0].1.expand(15.0);
    s.drag_at(selection_box.min, selection_box.size())?;
    s.require(
        s.state.editor.selected_objects == BTreeSet::from([near]),
        "A box with X-ray off selects only the front object",
    )?;
    s.shortcut("view.xray")?;
    s.wait(Duration::from_millis(400))?;
    s.drag_at(selection_box.min, selection_box.size())?;
    s.require(
        s.state.editor.selected_objects == BTreeSet::from([near, far]),
        "The same object box with X-ray on reaches the fully occluded rear object",
    )?;
    let row = s
        .state
        .layer_row_rect(near)
        .ok_or("The near cube has no Layers row")?;
    s.click_at(row.center())?;
    s.hover(Control::Viewport)?;
    s.shortcut("view.local")?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.require(
        s.state.is_local_view()
            && s.state.editor.xray_enabled()
            && !s.state.editor.is_object_visible(far),
        "Local View excludes the other object even when X-ray is enabled",
    )?;
    s.shortcut("selection.all")?;
    s.require(
        s.state.editor.selected_objects == BTreeSet::from([near]),
        "Select All with X-ray respects Local View's object scope",
    )?;
    s.shortcut("edit.confirm")?;
    s.shortcut("selection.all")?;
    let candidate_count = candidates(s)?.len();
    s.require(
        s.state.editor.edit_mode
            && s.state.editor.selected_object == Some(near)
            && s.state.editor.selected_vertices.len() == 8
            && candidate_count == 8
            && s.state.editor.document == document,
        "X-ray vertex selection stays inside the one edited primitive and changes no geometry",
    )
}
