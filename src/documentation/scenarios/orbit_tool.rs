//! Modifier-held navigation uses the same production pointer route as native UI.
use super::{Result, Session};
use crate::{controls::Control, doc_harness::ClipSpec, editor::Tool};
use std::time::Duration;

fn move_to(s: &mut Session<'_>, point: egui::Pos2) -> Result<()> {
    s.frame(vec![egui::Event::PointerMoved(point)], Duration::ZERO)?;
    Ok(())
}

/// This ownership probe overlaps two holds. Preserve the already-held modifier
/// while resolving the pan key from the same binding as ordinary tutorial input.
fn pan_with_held_modifiers(s: &mut Session<'_>, pressed: bool) -> Result<()> {
    let mut event = s.shortcut_event("navigation.pan", pressed)?;
    let egui::Event::Key { modifiers, .. } = &mut event else {
        return Err("The combined-navigation probe expects a pan key binding".into());
    };
    *modifiers |= s.input.modifiers();
    s.frame(vec![event], Duration::ZERO)?;
    s.settle()
}

fn select(s: &mut Session<'_>, edit: bool) -> Result<()> {
    let empty = s.empty_viewport_point()?;
    s.click_at(empty)?;
    if edit {
        s.shortcut("selection.all")?;
        s.require(
            !s.state.editor.selected_vertices.is_empty(),
            "The orbit walkthrough selects visible vertices through Select all",
        )
    } else {
        s.shortcut("selection.next")?;
        s.require(
            s.state.editor.selected_object.is_some(),
            "The orbit walkthrough selects an object through Next selection",
        )
    }
}

fn animated_exit(s: &mut Session<'_>) -> Result<()> {
    s.click(Control::ToolMove)?;
    select(s, false)?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::ViewFront])?;
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.settle()?;
    let document = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    let before = s.state.camera.clone();
    let start = s.state.viewport.center() + egui::vec2(-45.0, 30.0);
    move_to(s, start)?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.shortcut_down("navigation.orbit")?;
    s.require(
        s.state.orbit_tool_active()
            && s.state.is_planar_navigation()
            && s.state.ruler_2d_model.is_some()
            && s.state.camera.view_projection(1.0) == before.view_projection(1.0),
        "Holding the orbit modifier arms orbit without leaving the aligned view or changing the selected tool",
    )?;
    s.capture_tutorial("orbit-tool-ready")?;
    s.shortcut_up("navigation.orbit")?;
    s.capture_clip("orbit-tool-leave-alignment", ClipSpec::default(), |s| {
        s.callout(Control::NavigationPlanar, &format!("Hold {}, then left-drag to orbit out of 2D.", s.shortcut_label("navigation.orbit")?))?;
        s.wait(Duration::from_millis(700))?;
        s.shortcut_down("navigation.orbit")?;
        s.wait(Duration::from_millis(300))?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.wait(Duration::from_millis(150))?;
        s.require(s.state.is_planar_navigation() && s.state.ruler_2d_model.is_some(),
            "Pressing the primary button without motion keeps the 2D view and its rulers")?;
        let delta = egui::vec2(110.0, -55.0);
        s.move_pointer(start + delta, Duration::from_millis(1000))?;
        let mut expected = before.clone();
        expected.orbit(delta.x, delta.y);
        s.require(s.state.orbit_tool_active() && s.state.mouse_navigation_active()
            && !s.state.is_planar_navigation() && s.state.ruler_2d_model.is_none()
            && s.state.camera.is_orthographic()
            && s.state.camera.view_projection(1.0).abs_diff_eq(expected.view_projection(1.0), 1e-5),
            "Sampled orbit-modified primary motion orbits from the visible aligned pose without recalling a previous view, panning or zooming")?;
        s.callout(Control::NavigationFree, "Orbiting enters 3D and keeps the current projection.")?;
        s.wait(Duration::from_millis(900))?;
        let pose = s.state.camera.view_projection(1.0);
        s.shortcut_up("navigation.orbit")?;
        s.callout(Control::ToolMove, &format!("Release {} to stop orbiting. Move stays selected.", s.shortcut_label("navigation.orbit")?))?;
        s.wait(Duration::from_millis(750))?;
        s.move_pointer(start + delta + egui::vec2(25.0, 12.0), Duration::from_millis(300))?;
        s.require(!s.state.orbit_tool_active()
            && s.state.camera.view_projection(1.0) == pose
            && s.state.editor.tool == Tool::Move
            && s.state.editor.selected_objects == selected,
            "Releasing the orbit modifier stops orbit immediately even while the left button remains held")?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.wait(Duration::from_millis(750))?;
        s.clear_callout()
    })?;
    s.require(s.state.editor.document == document
        && s.state.editor.selected_objects == selected
        && !s.state.editor.is_interacting()
        && !s.state.mouse_navigation_active(),
        "The orbit tutorial preserves geometry and selection and consumes the remaining primary release")
}

fn existing_gesture_keeps_priority(
    s: &mut Session<'_>,
    start: egui::Pos2,
    transform: bool,
) -> Result<()> {
    let document = s.state.editor.document.clone();
    let objects = s.state.editor.selected_objects.clone();
    let vertices = s.state.editor.selected_vertices.clone();
    let pose = s.state.camera.view_projection(1.0);
    move_to(s, start)?;
    s.pointer_button(egui::PointerButton::Primary, true)?;
    move_to(s, start + egui::vec2(20.0, -10.0))?;
    s.require(
        s.state.editor.is_interacting() && s.state.editor.is_transforming() == transform,
        "An editor gesture owns its primary press before the orbit modifier is held",
    )?;
    s.shortcut_down("navigation.orbit")?;
    move_to(s, start + egui::vec2(30.0, -15.0))?;
    s.require(
        !s.state.orbit_tool_active()
            && s.state.editor.is_interacting()
            && s.state.camera.view_projection(1.0) == pose,
        "Holding the orbit modifier during an existing selection or transform gesture cannot take it over",
    )?;
    s.shortcut_up("navigation.orbit")?;
    s.shortcut("cancel")?;
    s.pointer_button(egui::PointerButton::Primary, false)?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.selected_objects == objects
            && s.state.editor.selected_vertices == vertices
            && s.state.camera.view_projection(1.0) == pose,
        "Cancelling the original editor gesture restores its own baseline without a stray orbit",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("cube-quads.obj")?;
    s.witness(Control::Viewport)?;
    animated_exit(s)?;
    let document = s.state.editor.document.clone();
    let dirty = s.state.is_dirty();
    for edit in [false, true] {
        if edit {
            s.shortcut("edit.confirm")?;
        }
        for (control, tool) in [
            (Control::ToolView, Tool::View),
            (Control::ToolMove, Tool::Move),
            (Control::ToolRotate, Tool::Rotate),
            (Control::ToolScale, Tool::Scale),
        ] {
            s.click(control)?;
            s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
            select(s, edit)?;
            // Frame refreshes the derived display frame and its revision even
            // without changing document data. Measure only the orbit gesture.
            let revision = s.state.editor.revision;
            let objects = s.state.editor.selected_objects.clone();
            let vertices = s.state.editor.selected_vertices.clone();
            let start = s.state.viewport.center() + egui::vec2(-25.0, 15.0);
            move_to(s, start)?;
            let before = s.state.camera.clone();
            s.shortcut_down("navigation.orbit")?;
            s.pointer_button(egui::PointerButton::Primary, true)?;
            s.pointer_button(egui::PointerButton::Primary, false)?;
            s.require(s.state.camera.view_projection(1.0) == before.view_projection(1.0)
                && s.state.editor.selected_objects == objects
                && s.state.editor.selected_vertices == vertices,
                "An orbit-modified click without dragging neither changes the view nor selects or edits geometry")?;
            s.pointer_button(egui::PointerButton::Primary, true)?;
            let delta = egui::vec2(38.0, -24.0);
            move_to(s, start + delta)?;
            let mut expected = before;
            expected.orbit(delta.x, delta.y);
            s.require(s.state.orbit_tool_active() && s.state.mouse_navigation_active()
                && s.state.camera.view_projection(1.0).abs_diff_eq(expected.view_projection(1.0), 1e-5)
                && s.state.editor.tool == tool && s.state.editor.edit_mode == edit,
                "Orbit-modified primary dragging orbits in every tool and both editing modes without changing the persistent tool")?;
            s.shortcut_up("navigation.orbit")?;
            let pose = s.state.camera.view_projection(1.0);
            move_to(s, start + delta * 1.5)?;
            s.pointer_button(egui::PointerButton::Primary, false)?;
            s.require(!s.state.orbit_tool_active() && !s.state.editor.is_interacting()
                && s.state.camera.view_projection(1.0) == pose
                && s.state.editor.document == document
                && s.state.editor.revision == revision
                && s.state.is_dirty() == dirty
                && s.state.editor.selected_objects == objects
                && s.state.editor.selected_vertices == vertices,
                "The orbit modifier release and the remaining primary release preserve geometry, selection, revision and dirty state")?;
        }
    }
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    select(s, true)?;
    let empty = s.empty_viewport_point()?;
    existing_gesture_keeps_priority(s, empty, false)?;
    s.click(Control::ToolMove)?;
    s.hover(Control::TransformX)?;
    existing_gesture_keeps_priority(s, s.trace.get(Control::TransformX)?.rect.center(), true)?;

    // The precedence rule is only for a fresh press. Existing gestures never
    // switch owners when another modifier arrives or the original one leaves.
    move_to(s, empty)?;
    s.shortcut_down("navigation.orbit")?;
    pan_with_held_modifiers(s, true)?;
    let before = s.state.camera.clone();
    s.pointer_button(egui::PointerButton::Primary, true)?;
    move_to(s, empty + egui::vec2(30.0, -20.0))?;
    s.require(
        s.state.hand_tool_active()
            && !s.state.orbit_tool_active()
            && s.state.camera.orientation() == before.orientation()
            && s.state.camera.view_projection(1.0) != before.view_projection(1.0),
        "The hand-tool key takes precedence when the hand-tool key and the orbit modifier are both held before the primary press",
    )?;
    pan_with_held_modifiers(s, false)?;
    let pose = s.state.camera.view_projection(1.0);
    move_to(s, empty + egui::vec2(45.0, -25.0))?;
    s.require(
        s.state.camera.view_projection(1.0) == pose,
        "Releasing the hand-tool key while the orbit modifier remains held does not hand the existing pan over to orbit",
    )?;
    s.pointer_button(egui::PointerButton::Primary, false)?;
    s.shortcut_up("navigation.orbit")?;

    move_to(s, empty)?;
    s.shortcut_down("navigation.orbit")?;
    s.pointer_button(egui::PointerButton::Primary, true)?;
    move_to(s, empty + egui::vec2(20.0, -15.0))?;
    pan_with_held_modifiers(s, true)?;
    let mut expected = s.state.camera.clone();
    expected.orbit(10.0, -5.0);
    move_to(s, empty + egui::vec2(30.0, -20.0))?;
    s.require(
        s.state.orbit_tool_active()
            && !s.state.hand_tool_active()
            && s.state
                .camera
                .view_projection(1.0)
                .abs_diff_eq(expected.view_projection(1.0), 1e-5),
        "Pressing the hand-tool key during a temporary orbit preserves the original gesture owner",
    )?;
    s.shortcut_up("navigation.orbit")?;
    let pose = s.state.camera.view_projection(1.0);
    move_to(s, empty + egui::vec2(40.0, -25.0))?;
    s.require(
        s.state.camera.view_projection(1.0) == pose,
        "Releasing the orbit modifier while the hand-tool key remains held stops orbit without handing the press to panning",
    )?;
    s.pointer_button(egui::PointerButton::Primary, false)?;
    s.shortcut_up("navigation.pan")?;
    let unchanged = s.state.editor.document.clone();
    s.undo()?;
    s.require(
        s.state.editor.document == unchanged && unchanged == document,
        "Temporary navigation and cancelled editor gestures add no document undo entry",
    )?;

    Ok(())
}
