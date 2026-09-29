use super::{ClipSpec, Result, Session};
use crate::{
    controls::Control,
    document::{Document, PrimitiveKind},
    editor::Editor,
};
use glam::Mat4;
use std::{collections::BTreeSet, time::Duration};

fn camera(s: &Session<'_>) -> Mat4 {
    s.state.camera.view_projection(s.state.aspect())
}

fn mesh_objects(s: &Session<'_>) -> BTreeSet<u64> {
    s.state
        .mesh
        .as_ref()
        .map(|mesh| {
            mesh.object_ranges
                .iter()
                .map(|range| range.object)
                .collect()
        })
        .unwrap_or_default()
}

fn press_slash(s: &mut Session<'_>) -> Result<()> {
    s.shortcut("view.local")?;
    Ok(())
}

fn slash(s: &mut Session<'_>) -> Result<()> {
    press_slash(s)?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.require(
        !s.state.camera.is_transitioning(),
        "The Local View transition finishes on the explicit guide clock",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    // Scene creation is setup. The illustrated selection and both Local View
    // transitions travel through the same input and UI routes as the editor.
    let mut document = Document::default();
    let mut ids = Vec::new();
    for (name, x) in [
        ("Left cube", -5.0),
        ("Middle cube", 0.0),
        ("Right cube", 5.0),
    ] {
        let id = document.insert_primitive(PrimitiveKind::Cube)?;
        let object = document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap();
        object.name = name.into();
        object.transform.translation[0] = x;
        ids.push(id);
    }
    document.convert_object(ids[1])?;
    let all = ids.iter().copied().collect::<BTreeSet<_>>();
    let middle = BTreeSet::from([ids[1]]);
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.witness(Control::Viewport)?;
    s.witness(Control::ObjectList)?;
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(18.0, -18.0);
    s.click_at(empty)?;
    let full_camera = camera(s);
    let revision = s.state.editor.revision;
    let dirty = s.state.is_dirty();
    s.require(
        s.state.editor.selected_objects.is_empty()
            && !s.state.is_local_view()
            && s.state.visible_objects().is_none()
            && mesh_objects(s) == all,
        "The Local View example starts with three cubes in the render mesh and no selection",
    )?;

    slash(s)?;
    s.require(
        !s.state.is_local_view()
            && s.state.visible_objects().is_none()
            && mesh_objects(s) == all
            && camera(s) == full_camera,
        "The Local View shortcut with no object selection leaves the full view and camera unchanged",
    )?;

    // The middle cube is centered by Frame all. Clicking its visible surface
    // selects it and gives the viewport keyboard focus for the shortcut.
    s.click_at(s.state.viewport.center())?;
    s.require(
        s.state.editor.selected_objects == middle,
        "A real viewport click selects the middle cube before isolation",
    )?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("local-view-all")?;

    slash(s)?;
    s.require(
        s.state.is_local_view()
            && s.state.visible_objects() == Some(&middle)
            && all
                .iter()
                .all(|id| s.state.editor.is_object_visible(*id) == middle.contains(id))
            && mesh_objects(s) == all
            && s.state.editor.selected_objects == middle
            && s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.is_dirty() == dirty,
        "The Local View shortcut restricts visibility and picking to the selected cube without rebuilding the mesh, editing the document, changing selection, or adding history",
    )?;
    s.capture_tutorial("local-view-isolated")?;
    let isolated_camera = camera(s);

    slash(s)?;
    s.require(
        !s.state.is_local_view()
            && s.state.visible_objects().is_none()
            && all.iter().all(|id| s.state.editor.is_object_visible(*id))
            && mesh_objects(s) == all
            && s.state.editor.selected_objects == middle
            && camera(s).abs_diff_eq(full_camera, 1e-5)
            && s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.is_dirty() == dirty,
        "Pressing the Local View shortcut again restores every cube and the previous camera without changing selection or document history",
    )?;
    s.capture_tutorial("local-view-restored")?;

    // Use the normal animation preference and real Slash events. The clock is
    // sampled through both transitions, so the clip shows the actual motion.
    let duration = Duration::from_millis(s.state.view_duration_ms.into());
    let half = duration / 2;
    s.require(
        s.state.animate_views && half >= Duration::from_millis(11),
        "The Local View tutorial uses the normal enabled animation preference",
    )?;
    s.wait(crate::doc_input::CUE_LIFETIME)?;
    s.capture_clip("local-view-transition", ClipSpec::default(), |s| {
        s.callout(Control::N3Menu, &format!("Press {} to focus on the selected object.", s.shortcut_label("view.local")?))?;
        s.wait(Duration::from_millis(550))?;
        let before = s.state.camera.clone();
        press_slash(s)?;
        s.require(
            s.state.is_local_view()
                && s.state.camera.is_transitioning()
                && camera(s) == before.view_projection(s.state.aspect())
                && s.state.camera.eye().abs_diff_eq(before.eye(), 1e-6)
                && s.state.camera.orientation() == before.orientation(),
            "Entering Local View starts an animation without jumping the visible camera pose",
        )?;
        s.wait(half)?;
        s.require(
            s.state.camera.is_transitioning()
                && !camera(s).abs_diff_eq(full_camera, 1e-5)
                && !camera(s).abs_diff_eq(isolated_camera, 1e-5),
            "The guide samples an intermediate camera pose while fitting the isolated object",
        )?;
        s.wait(duration - half)?;
        s.require(
            !s.state.camera.is_transitioning()
                && camera(s).abs_diff_eq(isolated_camera, 1e-5)
                && s.state.visible_objects() == Some(&middle),
            "The animated fit reaches the same isolated view as the still example",
        )?;
        s.wait(Duration::from_millis(650))?;
        s.callout(Control::N3Menu, &format!("Press {} again to return to the previous view.", s.shortcut_label("view.local")?))?;
        s.wait(Duration::from_millis(450))?;
        let before = s.state.camera.clone();
        press_slash(s)?;
        s.require(
            !s.state.is_local_view()
                && s.state.camera.is_transitioning()
                && camera(s) == before.view_projection(s.state.aspect())
                && s.state.camera.eye().abs_diff_eq(before.eye(), 1e-6)
                && s.state.camera.orientation() == before.orientation(),
            "Leaving Local View starts from the visible isolated camera pose",
        )?;
        s.wait(half)?;
        s.require(
            s.state.camera.is_transitioning()
                && !camera(s).abs_diff_eq(isolated_camera, 1e-5)
                && !camera(s).abs_diff_eq(full_camera, 1e-5),
            "The guide samples an intermediate camera pose while returning to the scene",
        )?;
        s.wait(duration - half)?;
        s.require(
            !s.state.camera.is_transitioning()
                && camera(s).abs_diff_eq(full_camera, 1e-5)
                && s.state.visible_objects().is_none()
                && mesh_objects(s) == all
                && s.state.editor.document == document
                && s.state.editor.revision == revision
                && s.state.editor.selected_objects == middle
                && s.state.is_dirty() == dirty,
            "The animated return restores the previous camera without changing geometry, selection, or history",
        )?;
        s.wait(Duration::from_millis(650))?;
        s.clear_callout()?;
        s.wait(Duration::from_millis(250))
    })?;

    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.witness(Control::LocalViewMenu)?;
    s.require(
        s.trace.get(Control::LocalViewMenu)?.parents == [Control::N3Menu, Control::ViewMenu],
        "Local View is available in the actual View menu",
    )?;
    s.click(Control::LocalViewMenu)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.require(
        s.state.is_local_view()
            && s.state.visible_objects() == Some(&middle)
            && all
                .iter()
                .all(|id| s.state.editor.is_object_visible(*id) == middle.contains(id)),
        "The View menu uses the same isolation behavior as the Local View shortcut",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::LocalViewMenu])?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.require(
        !s.state.is_local_view()
            && all.iter().all(|id| s.state.editor.is_object_visible(*id))
            && mesh_objects(s) == all
            && camera(s).abs_diff_eq(full_camera, 1e-5),
        "The same View menu item exits Local View and restores the full camera",
    )?;

    s.undo()?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.selected_objects == middle
            && !s.state.is_local_view(),
        "Local View toggles add no undo step",
    )?;

    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_vertices.is_empty(),
        "The selected mesh enters vertex edit mode with no component selected",
    )?;
    slash(s)?;
    s.require(
        s.state.is_local_view()
            && s.state.visible_objects() == Some(&middle)
            && s.state.editor.edit_mode
            && s.state.editor.selected_vertices.is_empty(),
        "In vertex edit mode Local View isolates the whole edited object without requiring a vertex selection",
    )?;
    slash(s)?;
    s.require(
        !s.state.is_local_view()
            && s.state.editor.edit_mode
            && s.state.editor.document == document
            && s.state.editor.revision == revision,
        "Leaving Local View in vertex edit mode restores the scene without editing the mesh",
    )?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.witness(Control::AnimateViews)?;
    s.witness(Control::Duration)?;
    s.click(Control::PreferencesClose)?;
    Ok(())
}
