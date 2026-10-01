//! Placed imported assets share the editor; contents use the explicit clock.
use super::{ClipSpec, Result, Session};
use crate::{controls::Control, render::shading::ShadingMode, scene_view::SceneView};
use std::{sync::Arc, time::Duration};

fn scene<'a>(s: &'a Session<'_>) -> &'a SceneView {
    s.state
        .selected_asset_view()
        .expect("Selected asset fixture")
}

fn witness_controls(s: &mut Session<'_>) -> Result<()> {
    for control in [
        Control::SceneHierarchy,
        Control::ScenePlay,
        Control::SceneTime,
        Control::SceneExposure,
        Control::SceneDiagnostics,
    ] {
        s.witness(control)?;
    }
    Ok(())
}

fn show_edges(s: &mut Session<'_>, enabled: bool) -> Result<()> {
    if s.state.show_edges != enabled {
        s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
        s.click(Control::Edges)?;
        s.shortcut("edit.leave")?;
    }
    s.require(
        s.state.show_edges == enabled,
        "The real Edges control sets the preview overlay",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_scene_fixture("WaterBottle/glTF-Binary/WaterBottle.glb")?;
    super::shading::select_mode(s, ShadingMode::MaterialPreview)?;
    show_edges(s, false)?;
    s.require(
        !s.state.animation_panel_is_open() && s.trace.get(Control::ScenePlay).is_err(),
        "Opening a static imported asset leaves animation controls closed by default",
    )?;
    s.click(Control::AnimationPanelToggle)?;
    s.require(
        s.state.animation_panel_is_open(),
        "The Animation control explicitly opens the panel for the selected asset",
    )?;
    witness_controls(s)?;
    let asset = scene(s).asset.clone();
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let dirty = s.state.is_dirty();
    let draw = &scene(s).frame.draws[0];
    let material = &asset.materials[draw.material];
    s.require(asset.images.len() == 4 && asset.nodes.len() == 1
        && draw.vertices.len() == 2549 && draw.indices.len() == 13530
        && material.base_color_texture.is_some()
        && material.metallic_roughness_texture.is_some()
        && material.normal_texture.is_some(),
        "The GLB example renders the real Water Bottle geometry and its embedded PBR texture channels")?;
    s.require(
        !s.trace.get(Control::ScenePlay)?.enabled
            && !s.trace.get(Control::SceneTime)?.enabled
            && s.trace.get(Control::SceneExposure)?.enabled,
        "A static scene disables playback and time controls while Material Preview enables Exposure",
    )?;
    s.click(Control::ToolDockClose)?;
    s.require(
        !s.state.animation_panel_is_open()
            && s.trace.get(Control::ScenePlay).is_err()
            && s.state.editor.document == document
            && s.state.editor.revision == revision,
        "Closing the animation panel returns space to the viewport without editing the document",
    )?;

    // Inspect source hierarchy without changing the authored object selection.
    let selected = s.state.editor.selected_object;
    s.click(Control::SceneHierarchy)?;
    s.require(
        s.state.editor.selected_object == selected && selected.is_some(),
        "Expanding the source hierarchy preserves the selected asset placement object",
    )?;
    s.hover(Control::Viewport)?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("scene-viewer-materials")?;

    let frame = scene(s).frame.clone();
    let exposure = s.trace.get(Control::SceneExposure)?.rect;
    s.click_at(exposure.left_center() + egui::vec2(65.0, 0.0))?;
    s.require(
        scene(s).exposure != 0.0 && Arc::ptr_eq(&frame, &scene(s).frame),
        "Exposure adjusts presentation without reevaluating or modifying scene geometry",
    )?;
    s.click(Control::SceneDiagnostics)?;
    s.require(s.state.editor.document == document && s.state.editor.revision == revision
        && s.state.is_dirty() == dirty && Arc::ptr_eq(&asset, &scene(s).asset),
        "Scene inspection, exposure and diagnostics preserve the source and editable-document history")?;

    show_edges(s, true)?;
    mixed_document(s)
}

fn mixed_document(s: &mut Session<'_>) -> Result<()> {
    use crate::document::{Document, Geometry, PrimitiveKind};
    // A prepared native shape keeps the illustrated sequence focused on import
    // and placement, rather than repeating the primitive-sizing tutorial.
    s.state.new_document();
    let mut document = Document::default();
    document.insert_primitive(PrimitiveKind::Cube)?;
    document.objects[0].transform.translation = [-12., 0., 0.];
    if let Geometry::Primitive(primitive) = &mut document.objects[0].geometry {
        primitive.size = [8.; 3];
    }
    s.state.editor = crate::editor::Editor::new(document.clone())?;
    s.settle()?;
    show_edges(s, false)?;
    s.take_host_effects();
    s.click_path(&[Control::N3Menu, Control::FileMenu])?;
    s.click(Control::Import)?;
    let effects = s.take_host_effects();
    s.require(
        effects == [crate::shortcuts::HostEffect::Import],
        "The real Import menu requests the native file-choice boundary exactly once",
    )?;
    s.import_scene_fixture("WaterBottle/glTF-Binary/WaterBottle.glb")?;
    super::shading::select_mode(s, ShadingMode::MaterialPreview)?;
    s.require(
        !s.state.animation_panel_is_open() && s.trace.get(Control::ScenePlay).is_err(),
        "Importing the placement example does not reopen animation controls",
    )?;
    let imported = s.state.editor.document.clone();
    let object = s
        .state
        .editor
        .selected_object
        .ok_or("Import selects its object")?;
    s.require(
        imported.objects.len() == 2
            && matches!(imported.objects[0].geometry, Geometry::Primitive(_))
            && matches!(imported.objects[1].geometry, Geometry::Asset(_)),
        "Import appends a linked asset beside native geometry in one document",
    )?;
    let asset = scene(s).asset.clone();
    s.hover(Control::Viewport)?;
    s.shortcut("view.frame")?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_clip("scene-viewer-placement", ClipSpec::default(), |s| {
        s.callout(Control::ToolMove, "Move the imported object just like the native cube.")?;
            s.wait(Duration::from_millis(700))?;
        s.shortcut("tool.move")?;
        s.shortcut("transform.axis-x")?;
        s.key(egui::Key::Num6, true, egui::Modifiers::NONE)?;
        s.key(egui::Key::Num6, false, egui::Modifiers::NONE)?;
            s.wait(Duration::from_millis(700))?;
        s.shortcut("edit.confirm")?;
        let placed_object = s.state.editor.document.objects.iter().find(|entry| entry.id == object).unwrap();
        s.require(placed_object.transform.translation == [6., 0., 0.]
            && matches!(placed_object.geometry, Geometry::Asset(_))
            && Arc::ptr_eq(&asset, &scene(s).asset),
            "Moving the asset changes authored placement while retaining its linked source and decoded contents")?;
        let placed = s.state.editor.document.clone();
            s.wait(Duration::from_millis(700))?;
        s.clear_callout()?;
        s.undo()?;
        s.require(s.state.editor.document == imported,
            "Undo restores the asset placement in one history step")?;
            s.wait(Duration::from_millis(700))?;
        s.redo()?;
        s.require(s.state.editor.document == placed,
            "Redo restores placement without reimporting or altering the source")?;
            s.wait(Duration::from_millis(900))
    })?;
    s.shortcut("edit.confirm")?;
    s.require(
        !s.state.editor.edit_mode
            && matches!(
                s.state
                    .editor
                    .document
                    .objects
                    .iter()
                    .find(|entry| entry.id == object)
                    .unwrap()
                    .geometry,
                Geometry::Asset(_)
            ),
        "Entering vertex edit does not silently flatten or modify an imported asset's contents",
    )?;
    s.click_path(&[Control::N3Menu, Control::FileMenu])?;
    s.witness(Control::Save)?;
    s.witness(Control::SaveAs)?;
    s.require(
        s.trace.get(Control::Save)?.enabled && s.trace.get(Control::SaveAs)?.enabled,
        "The mixed authored document can be saved with its linked asset references",
    )?;
    s.shortcut("edit.leave")?;
    Ok(())
}
