use super::{Result, Session};
use crate::{controls::Control, render::shading::ShadingMode};
use egui::{Event, Pos2};
use std::time::Duration;

fn open(s: &mut Session<'_>, anchor: Pos2) -> Result<()> {
    s.frame(vec![Event::PointerMoved(anchor)], Duration::ZERO)?;
    s.shortcut_down("shading.pie")?;
    s.require(
        s.state.shading_pie_active() && s.shortcut_is_down("shading.pie")?,
        "Holding the shading shortcut with the Cursor tool opens the shading pie",
    )?;
    s.witness(Control::ShadingPie)
}

fn choose(s: &mut Session<'_>, control: Control, mode: ShadingMode) -> Result<()> {
    let before = s.state.shading;
    s.hover(control)?;
    s.require(
        s.state.shading == before
            && s.trace.get(control)?.enabled
            && s.trace.get(control)?.parents == [Control::ShadingPie],
        "Hovering an enabled shading choice waits for key release before applying it",
    )?;
    s.shortcut_up("shading.pie")?;
    s.require(
        !s.state.shading_pie_active() && s.state.shading == mode,
        "Releasing the shading shortcut applies the pointed-at mode and closes the pie",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("cube-quads.obj")?;
    // Keep ordinary object edges off to demonstrate that Wireframe owns its
    // boundary display independently of the existing Solid edge preference.
    s.state.show_edges = false;
    s.settle()?;
    s.witness(Control::Viewport)?;
    s.click_at(s.state.viewport.center())?;
    s.shortcut("tool.cursor")?;
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let selected = s.state.editor.selected_objects.clone();
    let pose = s.state.camera.view_projection(s.state.aspect());
    let dirty = s.state.is_dirty();
    s.require(
        s.state.shading == ShadingMode::Solid && selected.len() == 1,
        "The shading example begins with one selected cube in the default Solid view",
    )?;
    let topology = &s
        .state
        .mesh
        .as_ref()
        .ok_or("Missing shading example mesh")?
        .edit_topology[0];
    s.require(
        topology.edge_vertices.len() == 12
            && topology.faces.len() == 6
            && topology.faces.iter().all(|face| face.vertices.len() == 4),
        "The cube has twelve authored boundaries and six quads, not triangulation diagonals",
    )?;
    let anchor = s.state.viewport.center();
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("shading-solid")?;

    open(s, anchor)?;
    s.witness(Control::PieSolid)?;
    s.hover(Control::PieWireframe)?;
    s.require(
        s.trace.get(Control::PieWireframe)?.rect.center().x < anchor.x
            && s.trace.get(Control::PieSolid)?.rect.center().x > anchor.x
            && s.state.shading == ShadingMode::Solid,
        "Wireframe is the left choice, Solid is the right choice, and hovering keeps the current shading",
    )?;
    s.capture_tutorial("shading-pie")?;
    choose(s, Control::PieWireframe, ShadingMode::Wireframe)?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("shading-wireframe")?;
    s.require(
        !s.state.show_edges
            && s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.selected_objects == selected
            && s.state.camera.view_projection(s.state.aspect()) == pose
            && s.state.is_dirty() == dirty,
        "Wireframe changes viewport shading without changing the Solid edge preference, geometry, selection, camera, or dirty state",
    )?;

    open(s, anchor)?;
    s.hover(Control::PieSolid)?;
    s.frame(vec![Event::PointerMoved(anchor)], Duration::ZERO)?;
    s.shortcut_up("shading.pie")?;
    s.require(
        !s.state.shading_pie_active() && s.state.shading == ShadingMode::Wireframe,
        "Returning to the opening point before release cancels the shading choice",
    )?;
    open(s, anchor)?;
    s.hover(Control::PieSolid)?;
    s.shortcut("cancel")?;
    s.shortcut_up("shading.pie")?;
    s.require(
        !s.state.shading_pie_active()
            && s.state.shading == ShadingMode::Wireframe
            && s.state.editor.selected_objects == selected,
        "Cancel closes the shading pie without applying the hovered choice or clearing selection",
    )?;
    open(s, anchor)?;
    choose(s, Control::PieSolid, ShadingMode::Solid)?;
    s.require(
        !s.state.show_edges,
        "Returning to Solid restores its unchanged ordinary edge-display preference",
    )?;
    for tool in ["tool.move", "tool.rotate", "tool.scale"] {
        s.shortcut(tool)?;
        s.shortcut_down("transform.axis-z")?;
        s.require(
            !s.state.shading_pie_active() && s.state.editor.transform_axis == Some(2),
            "In a transform tool the shading key belongs only to the Z-axis lock",
        )?;
        s.shortcut_up("transform.axis-z")?;
    }
    s.shortcut("tool.cursor")?;

    // Wireframe is a display mode, not a new component selection policy.
    s.shortcut("view.front")?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.shortcut("edit.confirm")?;
    let visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    s.require(
        s.state.editor.edit_mode && visible.len() == 4,
        "The front-facing cube exposes four selectable front vertices in Solid mode",
    )?;
    open(s, anchor)?;
    choose(s, Control::PieWireframe, ShadingMode::Wireframe)?;
    let wire_visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    s.require(
        wire_visible == visible,
        "Wireframe still limits vertex picking to the same visible front vertices",
    )?;
    for _ in 0..5 {
        s.shortcut("selection.next")?;
        s.require(
            s.state.editor.selected_vertices.len() == 1
                && visible
                    .iter()
                    .any(|(id, _)| s.state.editor.selected_vertices.contains(id)),
            "Cycling vertices in Wireframe never selects an occluded rear corner",
        )?;
    }
    s.shortcut("edit.leave")?;
    s.undo()?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.is_dirty() == dirty,
        "Shading changes and selection feedback add no document history entry",
    )
}
