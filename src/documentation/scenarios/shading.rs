use super::{Result, Session};
use crate::{controls::Control, render::shading::ShadingMode};
use egui::{Event, Pos2};
use std::{sync::Arc, time::Duration};

fn open(s: &mut Session<'_>, anchor: Pos2) -> Result<()> {
    s.frame(vec![Event::PointerMoved(anchor)], Duration::ZERO)?;
    s.shortcut_down("shading.pie")?;
    s.require(
        s.state.shading_pie_active() && s.shortcut_is_down("shading.pie")?,
        "Holding the shading shortcut with the Cursor tool opens the shading pie",
    )?;
    s.witness(Control::ShadingPie)?;
    s.require(
        s.trace.get(Control::ShadingPie)?.label == "Shading",
        "The open pie identifies itself as Shading",
    )
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

/// Other guide features also select their display mode through the real pie.
/// Loading a fixture never silently opts a material illustration into PBR.
pub(super) fn select_mode(s: &mut Session<'_>, mode: ShadingMode) -> Result<()> {
    s.hover(Control::Viewport)?;
    s.shortcut("tool.cursor")?;
    open(s, s.state.viewport.center())?;
    let control = match mode {
        ShadingMode::Solid => Control::PieSolid,
        ShadingMode::Wireframe => Control::PieWireframe,
        ShadingMode::MaterialPreview => Control::PieMaterialPreview,
    };
    choose(s, control, mode)
}

fn comparison_pixels(s: &Session<'_>) -> Result<Vec<u8>> {
    // Compare only the scene's center, excluding the pie, toolbar, properties,
    // input cues, and cursor. This witnesses an actual rendering change, not
    // merely different menu text or a changed enum value.
    let frame = s.capture.read_frame()?;
    let region = s.state.viewport.shrink2(egui::vec2(180.0, 120.0));
    let mut pixels = Vec::new();
    for y in region.top() as u32..region.bottom() as u32 {
        let start = ((y * frame.width + region.left() as u32) * 4) as usize;
        let end = ((y * frame.width + region.right() as u32) * 4) as usize;
        pixels.extend_from_slice(&frame.rgba[start..end]);
    }
    Ok(pixels)
}

fn imported_material_comparison(s: &mut Session<'_>) -> Result<()> {
    s.load_scene_fixture("WaterBottle/glTF-Binary/WaterBottle.glb")?;
    select_mode(s, ShadingMode::Solid)?;
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let selected = s.state.editor.selected_objects.clone();
    let pose = s.state.camera.view_projection(s.state.aspect());
    let dirty = s.state.is_dirty();
    let view = s
        .state
        .selected_asset_view()
        .ok_or("Missing comparison asset")?;
    let asset = view.asset.clone();
    let frame = view.frame.clone();
    let exposure = view.exposure;
    s.require(
        asset.images.len() == 4
            && asset.materials[frame.draws[0].material]
                .base_color_texture
                .is_some(),
        "The comparison uses the actual Water Bottle asset and its imported texture maps",
    )?;
    s.require(
        !s.trace.get(Control::SceneExposure)?.enabled,
        "Solid inspection shading disables the material-preview Exposure control",
    )?;
    let away = s.state.viewport_ui_rect.left_bottom() + egui::vec2(40.0, -45.0);
    s.frame(
        vec![Event::PointerMoved(away)],
        crate::doc_input::CUE_LIFETIME,
    )?;
    s.capture_tutorial("shading-imported-solid")?;
    let solid = comparison_pixels(s)?;

    select_mode(s, ShadingMode::MaterialPreview)?;
    s.frame(
        vec![Event::PointerMoved(away)],
        crate::doc_input::CUE_LIFETIME,
    )?;
    s.capture_tutorial("shading-material-preview")?;
    let material_preview = comparison_pixels(s)?;
    s.require(
        material_preview != solid && s.trace.get(Control::SceneExposure)?.enabled,
        "Material Preview visibly changes the same imported geometry from Solid inspection shading to its materials",
    )?;
    let view = s
        .state
        .selected_asset_view()
        .ok_or("Comparison asset lost selection")?;
    s.require(
        Arc::ptr_eq(&asset, &view.asset)
            && Arc::ptr_eq(&frame, &view.frame)
            && view.exposure == exposure
            && s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.selected_objects == selected
            && s.state.camera.view_projection(s.state.aspect()) == pose
            && s.state.is_dirty() == dirty,
        "The material comparison changes only viewport shading while preserving the exact source resources, evaluated geometry, exposure, placement, selection, and camera",
    )?;
    select_mode(s, ShadingMode::Solid)?;
    s.frame(
        vec![Event::PointerMoved(away)],
        crate::doc_input::CUE_LIFETIME,
    )?;
    s.settle()?;
    s.require(
        comparison_pixels(s)? == solid,
        "Returning to Solid restores the same inspection pixels without modifying the imported material",
    )?;
    let no_undo = !s.state.editor.undo();
    s.require(
        no_undo && s.state.editor.document == document,
        "Switching imported assets between shading modes creates no undo entry",
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
    s.witness(Control::PieMaterialPreview)?;
    s.hover(Control::PieWireframe)?;
    s.require(
        s.trace.get(Control::PieWireframe)?.rect.center().x < anchor.x
            && s.trace.get(Control::PieSolid)?.rect.center().x > anchor.x
            && s.trace.get(Control::PieMaterialPreview)?.rect.center().y > anchor.y
            && s.state.shading == ShadingMode::Solid,
        "Wireframe is left, Solid is right, Material Preview is below, and hovering keeps the current shading",
    )?;
    s.capture_tutorial("shading-pie")?;
    let card = s.trace.get(Control::PieWireframe)?.rect;
    let sector = anchor - egui::vec2(60.0, 0.0);
    s.require(
        !card.contains(sector),
        "Shading help is demonstrated away from its button",
    )?;
    s.frame(vec![Event::PointerMoved(sector)], Duration::ZERO)?;
    let delay = s.ctx.global_style().interaction.tooltip_delay;
    s.wait(Duration::from_secs_f32(delay + 0.25))?;
    s.require(
        s.state.shading_pie_active() && s.state.shading == ShadingMode::Solid,
        "Waiting for sector help keeps the shading pie open without changing the mode",
    )?;
    s.capture_tutorial("shading-pie-help")?;
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
    let edge = s.state.viewport_ui_rect.left_center() + egui::vec2(32.0, 0.0);
    open(s, edge)?;
    s.require(
        s.trace.get(Control::PieWireframe)?.rect.right() < s.state.viewport_ui_rect.left(),
        "The shading pie can extend over the sidebar",
    )?;
    choose(s, Control::PieWireframe, ShadingMode::Wireframe)?;
    open(s, edge)?;
    choose(s, Control::PieSolid, ShadingMode::Solid)?;
    open(s, anchor)?;
    choose(s, Control::PieMaterialPreview, ShadingMode::MaterialPreview)?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.selected_objects == selected,
        "Material Preview is available for native meshes without adding or changing their material data",
    )?;
    open(s, anchor)?;
    s.frame(
        vec![Event::PointerMoved(anchor - egui::vec2(0.0, 140.0))],
        Duration::ZERO,
    )?;
    s.shortcut_up("shading.pie")?;
    s.require(
        s.state.shading == ShadingMode::MaterialPreview && !s.state.shading_pie_active(),
        "The empty upper direction cancels without choosing an unimplemented Rendered mode",
    )?;
    select_mode(s, ShadingMode::Solid)?;
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
    )?;
    imported_material_comparison(s)
}
