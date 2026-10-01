use super::{Result, Session, pointer};
use crate::{controls::Control, doc_harness::ClipSpec, editor::Tool};
use glam::{Vec2, Vec3};
use std::time::Duration;

fn geometry(s: &Session<'_>) -> (Vec2, f32, Vec3) {
    let camera = &s.state.camera;
    let projection = camera.view_projection(s.state.aspect());
    let project = |point: Vec3| {
        let clip = projection * point.extend(1.0);
        clip.truncate().truncate() / clip.w
    };
    let right = Vec3::new(
        camera.direction_in_view(Vec3::X).x,
        camera.direction_in_view(Vec3::Y).x,
        camera.direction_in_view(Vec3::Z).x,
    );
    let center = project(Vec3::ZERO);
    (
        center,
        project(right).distance(center),
        camera.direction_in_view(Vec3::Z),
    )
}

fn prepare_selection(s: &mut Session<'_>, edit_mode: bool) -> Result<()> {
    let empty = s.empty_viewport_point()?;
    s.click_at(empty)?;
    if edit_mode {
        s.shortcut("selection.all")?;
        s.require(
            !s.state.editor.selected_vertices.is_empty(),
            "The hand test starts with a real visible-vertex selection",
        )?;
    } else {
        s.shortcut("selection.next")?;
        s.require(
            s.state.editor.selected_object.is_some(),
            "The hand test starts with a real object selection",
        )?;
    }
    Ok(())
}

fn existing_gesture_keeps_priority(
    s: &mut Session<'_>,
    start: egui::Pos2,
    delta: egui::Vec2,
    transform: bool,
) -> Result<()> {
    let document = s.state.editor.document.clone();
    let objects = s.state.editor.selected_objects.clone();
    let vertices = s.state.editor.selected_vertices.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(vec![pointer(start, true)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerMoved(start + delta)],
        Duration::ZERO,
    )?;
    s.require(
        s.state.editor.is_interacting() && s.state.editor.is_transforming() == transform,
        "The existing editor gesture begins before the hand-tool key is pressed",
    )?;
    s.shortcut_down("navigation.pan")?;
    s.frame(
        vec![egui::Event::PointerMoved(start + delta * 1.5)],
        Duration::ZERO,
    )?;
    s.require(
        !s.state.hand_tool_active()
            && s.state.editor.is_interacting()
            && s.state.editor.is_transforming() == transform
            && s.state.camera.view_projection(s.state.aspect()) == camera,
        "The hand-tool key pressed after a marquee or transform began does not take over that gesture or pan camera",
    )?;
    s.shortcut_up("navigation.pan")?;
    s.shortcut("cancel")?;
    s.frame(vec![pointer(start + delta * 1.5, false)], Duration::ZERO)?;
    s.settle()?;
    s.require(
        !s.state.editor.is_interacting()
            && s.state.editor.document == document
            && s.state.editor.selected_objects == objects
            && s.state.editor.selected_vertices == vertices
            && s.state.camera.view_projection(s.state.aspect()) == camera,
        "Cancelling the original gesture restores its starting document and selection without a stray hand pan",
    )
}

fn animated_hand_pan(s: &mut Session<'_>) -> Result<()> {
    s.click(Control::ToolMove)?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    prepare_selection(s, false)?;
    let document = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    let before = geometry(s);
    let start = s.state.viewport.center() + egui::vec2(-45.0, 25.0);
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.capture_clip("hand-tool-pan-and-release", ClipSpec::default(), |s| {
        s.callout(Control::ToolMove, &format!("Hold {} to pan without changing tools.", s.shortcut_label("navigation.pan")?))?;
        s.wait(Duration::from_millis(500))?;
        s.shortcut_down("navigation.pan")?;
        s.wait(Duration::from_millis(350))?;
        s.require(s.state.hand_tool_active() && s.cursor == egui::CursorIcon::Grab
            && geometry(s) == before,
            "The animated hand tutorial shows the hand-tool key held before the pointer drag")?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.wait(Duration::from_millis(100))?;
        let end = start + egui::vec2(100.0, -50.0);
        s.move_pointer(end, Duration::from_millis(900))?;
        let after = geometry(s);
        s.require(s.state.mouse_navigation_active() && s.cursor == egui::CursorIcon::Grabbing
            && after.0.distance(before.0) > 1e-5 && (after.1-before.1).abs() < 1e-5
            && after.2.abs_diff_eq(before.2, 1e-5),
            "Sampled primary motion pans while the hand-tool key is held, without rotating or zooming")?;
        s.wait(Duration::from_millis(350))?;
        let held = s.state.camera.view_projection(s.state.aspect());
        s.shortcut_up("navigation.pan")?;
        s.callout(Control::ToolMove, &format!("Release {}: panning stops and Move stays selected.", s.shortcut_label("navigation.pan")?))?;
        s.wait(Duration::from_millis(300))?;
        s.move_pointer(end + egui::vec2(25.0, 12.0), Duration::from_millis(300))?;
        s.require(!s.state.hand_tool_active() && s.state.editor.tool == Tool::Move
            && s.state.camera.view_projection(s.state.aspect()) == held
            && s.state.editor.selected_objects == selected,
            "The animated pointer keeps moving after the hand-tool key release without moving the view or selecting")?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.wait(Duration::from_millis(500))?;
        s.clear_callout()
    })?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.selected_objects == selected
            && !s.state.editor.is_interacting()
            && !s.state.mouse_navigation_active(),
        "The completed hand clip preserves geometry and selection and releases every gesture",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    Ok(())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("cube-quads.obj")?;
    s.witness(Control::Viewport)?;
    animated_hand_pan(s)?;
    let document = s.state.editor.document.clone();
    let dirty = s.state.is_dirty();
    for edit_mode in [false, true] {
        if edit_mode {
            s.shortcut("edit.confirm")?;
        }
        s.require(
            s.state.editor.edit_mode == edit_mode,
            "The hand walkthrough runs in both object and vertex modes",
        )?;
        for (control, tool) in [
            (Control::ToolView, Tool::View),
            (Control::ToolMove, Tool::Move),
            (Control::ToolRotate, Tool::Rotate),
            (Control::ToolScale, Tool::Scale),
        ] {
            s.click(control)?;
            s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
            prepare_selection(s, edit_mode)?;
            let objects = s.state.editor.selected_objects.clone();
            let vertices = s.state.editor.selected_vertices.clone();
            let before = geometry(s);
            let start = s.state.viewport.center() + egui::vec2(-25.0, 15.0);
            s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
            s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
            s.shortcut_down("navigation.pan")?;
            s.require(
                s.state.hand_tool_active()
                    && s.cursor == egui::CursorIcon::Grab
                    && s.state.editor.tool == tool
                    && geometry(s) == before,
                "Holding the hand-tool key shows an open hand over the viewport without changing the persistent tool or camera",
            )?;
            if !edit_mode && tool == Tool::View {
                s.capture_tutorial("hand-tool-ready")?;
            }
            s.frame(vec![pointer(start, true)], Duration::ZERO)?;
            let end = start + egui::vec2(38.0, -24.0);
            s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
            let after = geometry(s);
            s.require(
                s.state.hand_tool_active()
                    && s.state.mouse_navigation_active()
                    && s.cursor == egui::CursorIcon::Grabbing
                    && s.input.is_pressed(egui::PointerButton::Primary)
                    && after.0.distance(before.0) > 1e-5
                    && (after.1 - before.1).abs() < 1e-5
                    && after.2.abs_diff_eq(before.2, 1e-5)
                    && s.state.editor.tool == tool
                    && s.state.editor.edit_mode == edit_mode
                    && s.state.editor.document == document
                    && s.state.editor.selected_objects == objects
                    && s.state.editor.selected_vertices == vertices,
                "The hand-tool key with a primary drag shows a closed hand and pans with every modeling tool and both modes without rotating, zooming, editing, or changing selection",
            )?;
            if !edit_mode && tool == Tool::View {
                s.capture_tutorial("hand-tool-drag")?;
            }
            let held_pose = s.state.camera.view_projection(s.state.aspect());
            s.shortcut_up("navigation.pan")?;
            let released_end = end + egui::vec2(26.0, 18.0);
            s.frame(
                vec![egui::Event::PointerMoved(released_end)],
                Duration::ZERO,
            )?;
            s.require(
                !s.state.hand_tool_active()
                    && s.state.editor.tool == tool
                    && s.state.camera.view_projection(s.state.aspect()) == held_pose
                    && s.state.editor.selected_objects == objects
                    && s.state.editor.selected_vertices == vertices,
                "Releasing the hand-tool key immediately stops hand panning even while the primary button remains held and retains the previous tool",
            )?;
            s.frame(vec![pointer(released_end, false)], Duration::ZERO)?;
            s.settle()?;
            s.require(
                !s.state.editor.is_interacting()
                    && s.state.editor.selected_objects == objects
                    && s.state.editor.selected_vertices == vertices
                    && s.state.editor.document == document
                    && s.state.is_dirty() == dirty,
                "The remaining primary release is consumed and cannot select or edit geometry",
            )?;
        }
    }

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    prepare_selection(s, true)?;
    let empty = s.empty_viewport_point()?;
    existing_gesture_keeps_priority(s, empty, egui::vec2(24.0, -20.0), false)?;
    s.click(Control::ToolMove)?;
    s.hover(Control::TransformX)?;
    let handle = s.trace.get(Control::TransformX)?.rect.center();
    existing_gesture_keeps_priority(s, handle, egui::vec2(10.0, 0.0), true)?;

    let objects = s.state.editor.selected_objects.clone();
    let vertices = s.state.editor.selected_vertices.clone();
    s.frame(vec![egui::Event::PointerMoved(empty)], Duration::ZERO)?;
    s.shortcut_down("navigation.pan")?;
    s.frame(vec![pointer(empty, true)], Duration::ZERO)?;
    let end = empty + egui::vec2(25.0, -20.0);
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    s.require(
        s.state.hand_tool_active() && s.state.mouse_navigation_active(),
        "The focus-loss check begins during an active hand pan",
    )?;
    let pose = s.state.camera.view_projection(s.state.aspect());
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)?;
    s.require(
        !s.state.hand_tool_active()
            && !s.state.mouse_navigation_active()
            && !s.shortcut_is_down("navigation.pan")?,
        "Losing window focus clears the held hand-tool state and active hand pan",
    )?;
    s.frame(vec![egui::Event::WindowFocused(true)], Duration::ZERO)?;
    s.frame(
        vec![
            egui::Event::PointerMoved(end + egui::vec2(15.0, 10.0)),
            pointer(end, false),
        ],
        Duration::ZERO,
    )?;
    s.settle()?;
    s.require(
        !s.state.hand_tool_active()
            && s.state.camera.view_projection(s.state.aspect()) == pose
            && s.state.editor.selected_objects == objects
            && s.state.editor.selected_vertices == vertices
            && s.state.editor.document == document,
        "Returning focus does not resume the hand pan or turn the old pointer release into selection",
    )?;

    Ok(())
}
