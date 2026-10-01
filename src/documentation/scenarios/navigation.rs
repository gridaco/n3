use super::{Result, Session};
use crate::{controls::Control, pointer_policy::DRAG_THRESHOLD, scroll_input::ScrollPhase};
use glam::{Mat4, Vec2, Vec3};
use std::time::Duration;

fn matrix(s: &Session<'_>) -> Mat4 {
    s.state.camera.view_projection(s.state.aspect())
}

fn orientation(s: &Session<'_>) -> Vec3 {
    s.state.camera.direction_in_view(Vec3::Z)
}

fn button(position: egui::Pos2, button: egui::PointerButton, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: position,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

fn drag_mouse(
    s: &mut Session<'_>,
    mouse_button: egui::PointerButton,
    start: egui::Pos2,
    delta: egui::Vec2,
) -> Result<()> {
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(vec![button(start, mouse_button, true)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerMoved(start + delta * 0.5)],
        Duration::ZERO,
    )?;
    s.frame(
        vec![egui::Event::PointerMoved(start + delta)],
        Duration::ZERO,
    )?;
    s.frame(
        vec![button(start + delta, mouse_button, false)],
        Duration::ZERO,
    )?;
    s.settle()
}

/// Measure the image of the origin and of a unit segment parallel to the image
/// plane. Unlike an arbitrary world-axis segment, this segment has equal depth
/// at both ends, so perspective panning must preserve its projected length.
fn screen_geometry(s: &Session<'_>) -> (Vec2, f32) {
    let projection = matrix(s);
    let project = |point: Vec3| {
        let clip = projection * point.extend(1.0);
        clip.truncate().truncate() / clip.w
    };
    let camera = &s.state.camera;
    let screen_right = Vec3::new(
        camera.direction_in_view(Vec3::X).x,
        camera.direction_in_view(Vec3::Y).x,
        camera.direction_in_view(Vec3::Z).x,
    );
    let center = project(Vec3::ZERO);
    (center, project(screen_right).distance(center))
}

fn panned(before: (Vec2, f32), after: (Vec2, f32)) -> bool {
    before.0.distance(after.0) > 1e-5 && (before.1 - after.1).abs() < 1e-5
}

fn zoomed_in_from_center(before: (Vec2, f32), after: (Vec2, f32)) -> bool {
    before.0.length() < 1e-5 && before.0.abs_diff_eq(after.0, 1e-5) && after.1 > before.1 + 1e-5
}

fn complete_view(s: &mut Session<'_>, control: Control, direction: Option<Vec3>) -> Result<()> {
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(control)?;
    s.require(
        s.state.camera.is_transitioning(),
        "A different preset starts the configured camera transition",
    )?;
    let duration = Duration::from_millis(s.state.view_duration_ms.into());
    s.frame(Vec::new(), duration)?;
    s.settle()?;
    s.require(
        !s.state.camera.is_transitioning(),
        "The preset transition completes at its configured duration",
    )?;
    if let Some(direction) = direction {
        s.require(
            s.state.camera.is_orthographic()
                && s.state.is_planar_navigation()
                && s.state
                    .camera
                    .direction_in_view(direction)
                    .abs_diff_eq(Vec3::Z, 1e-5),
            "The requested preset reaches its exact orthographic axis",
        )?;
    } else {
        s.require(
            !s.state.camera.is_orthographic() && !s.state.is_planar_navigation(),
            "The perspective preset restores perspective projection",
        )?;
    }
    Ok(())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("bracket.obj")?;
    s.settle()?;
    s.witness(Control::Viewport)?;
    let framed = matrix(s);
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(55.0, -75.0);
    drag_mouse(
        s,
        egui::PointerButton::Secondary,
        start,
        egui::vec2(24.0, 16.0),
    )?;
    drag_mouse(
        s,
        egui::PointerButton::Middle,
        start,
        egui::vec2(20.0, -14.0),
    )?;
    s.hover(Control::Viewport)?;
    s.pinch(0.15)?;
    s.require(
        !matrix(s).abs_diff_eq(framed, 1e-5),
        "Navigation changes the camera",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.require(
        matrix(s).abs_diff_eq(framed, 1e-5),
        "Frame recenters, refits, and restores the default viewing angle",
    )?;

    s.click(Control::Projection)?;
    s.require(
        s.state.camera.is_orthographic(),
        "Projection switches to orthographic",
    )?;
    s.click(Control::Projection)?;
    s.require(
        !s.state.camera.is_orthographic(),
        "Projection switches back to perspective",
    )?;
    for (path, edges) in [
        (
            &[Control::N3Menu, Control::ViewMenu, Control::Edges][..],
            true,
        ),
        (
            &[Control::N3Menu, Control::Preferences, Control::Grid][..],
            false,
        ),
    ] {
        s.click_path(path)?;
        s.require(
            if edges {
                !s.state.show_edges
            } else {
                !s.state.show_grid
            },
            "The display checkbox hides its overlay",
        )?;
        s.click_path(path)?;
        s.require(
            if edges {
                s.state.show_edges
            } else {
                s.state.show_grid
            },
            "The display checkbox restores its overlay",
        )?;
    }
    s.close_preferences()?;
    s.require(
        !s.state.show_preferences,
        "Closing Preferences leaves the viewport free for mouse navigation",
    )?;
    for (control, direction) in [
        (Control::ViewTop, Some(Vec3::Y)),
        (Control::ViewFront, Some(Vec3::Z)),
        (Control::ViewRight, Some(Vec3::X)),
        (Control::ViewPerspective, None),
    ] {
        complete_view(s, control, direction)?;
    }

    // Mouse navigation uses the same real egui pointer events as the native UI.
    let document = s.state.editor.document.clone();
    for edit_mode in [false, true] {
        if edit_mode {
            let id = s.state.editor.document.objects[0].id;
            let row = s
                .state
                .layer_row_rect(id)
                .ok_or("The fixture has no Layers row")?;
            s.click_at(row.center())?;
            s.shortcut("edit.confirm")?;
        }
        s.require(
            s.state.editor.edit_mode == edit_mode,
            "The mouse checks run in their requested editing mode",
        )?;
        s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
        let before = screen_geometry(s);
        let facing = orientation(s);
        let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(55.0, -75.0);
        drag_mouse(
            s,
            egui::PointerButton::Middle,
            start,
            egui::vec2(24.0, -18.0),
        )?;
        let after = screen_geometry(s);
        let now_facing = orientation(s);
        s.require(
            panned(before, after) && now_facing.abs_diff_eq(facing, 1e-5)
                && s.state.editor.edit_mode == edit_mode,
            &format!("A real middle-button drag pans in object and vertex modes while preserving scale and viewing direction (panned={}, facing={}, mode={}, viewport={:?}, start={start:?})",
                panned(before, after), now_facing.abs_diff_eq(facing, 1e-5),
                s.state.editor.edit_mode == edit_mode, s.state.viewport),
        )?;
        s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
        let before = matrix(s);
        let facing = orientation(s);
        s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
        s.frame(
            vec![button(start, egui::PointerButton::Secondary, true)],
            Duration::ZERO,
        )?;
        let end = start + egui::vec2(36.0, 20.0);
        s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
        s.require(
            !matrix(s).abs_diff_eq(before, 1e-5)
                && !orientation(s).abs_diff_eq(facing, 1e-5)
                && s.input.is_pressed(egui::PointerButton::Secondary)
                && s.state.editor.edit_mode == edit_mode,
            "A real held right-button drag orbits in object and vertex modes",
        )?;
        if !edit_mode {
            s.capture_tutorial("navigation-mouse-orbit")?;
        }
        s.frame(
            vec![button(end, egui::PointerButton::Secondary, false)],
            Duration::ZERO,
        )?;
        s.settle()?;
        s.require(
            s.trace.get(Control::ViewportFrame).is_err() && s.state.editor.document == document,
            "Releasing an orbit drag does not open a context menu or change geometry",
        )?;
    }
    s.shortcut("edit.confirm")?;
    s.require(
        !s.state.editor.edit_mode,
        "Idle Confirm returns the navigation walkthrough to object mode",
    )?;

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = matrix(s);
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(70.0, -100.0);
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(
        vec![button(start, egui::PointerButton::Secondary, true)],
        Duration::ZERO,
    )?;
    let click_end = start + egui::vec2(DRAG_THRESHOLD, 0.0);
    s.frame(vec![egui::Event::PointerMoved(click_end)], Duration::ZERO)?;
    s.require(
        matrix(s) == before,
        "Right-button movement within four logical pixels does not move the camera",
    )?;
    s.frame(
        vec![button(click_end, egui::PointerButton::Secondary, false)],
        Duration::ZERO,
    )?;
    s.settle()?;
    s.require(
        matrix(s) == before && s.trace.get(Control::ViewportFrame)?.parents == [Control::ViewportMenu]
            && s.trace.get(Control::ViewportPreferences)?.parents == [Control::ViewportMenu],
        "A right-button release within four logical pixels opens the viewport context menu without camera movement",
    )?;
    s.witness(Control::ViewportMenu)?;
    s.require(
        s.trace.get(Control::ViewportFrame)?.rect.width() >= crate::theme::size::STEP_52,
        "Context-menu rows use the same minimum width as dropdown menus",
    )?;
    s.value("mouse-drag-threshold", DRAG_THRESHOLD);
    s.witness(Control::ViewportPreferences)?;
    s.hover(Control::ViewportFrame)?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("navigation-context-menu")?;
    s.click(Control::ViewportFrame)?;
    s.require(
        s.trace.get(Control::ViewportFrame).is_err() && matrix(s).abs_diff_eq(framed, 1e-5),
        "Frame all in the viewport context menu frames the document and closes the menu",
    )?;

    // Crossing the threshold commits to a drag even if the pointer comes back.
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(
        vec![button(start, egui::PointerButton::Secondary, true)],
        Duration::ZERO,
    )?;
    s.frame(
        vec![egui::Event::PointerMoved(
            start + egui::vec2(DRAG_THRESHOLD * 3.0, 0.0),
        )],
        Duration::ZERO,
    )?;
    s.require(
        !matrix(s).abs_diff_eq(framed, 1e-5),
        "Crossing the right-drag threshold starts orbiting",
    )?;
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(
        vec![button(start, egui::PointerButton::Secondary, false)],
        Duration::ZERO,
    )?;
    s.settle()?;
    s.require(
        s.trace.get(Control::ViewportFrame).is_err(),
        "A drag that crossed the threshold cannot become a context click by returning to its start",
    )?;

    // Scroll and trackpad adapters share production mappings; physical device delivery is
    // outside these deterministic UI checks.
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = matrix(s);
    let facing = orientation(s);
    s.hover(Control::Viewport)?;
    s.scroll(18.0, -12.0, true, egui::Modifiers::NONE)?;
    s.require(
        !matrix(s).abs_diff_eq(before, 1e-5) && !orientation(s).abs_diff_eq(facing, 1e-5),
        "Precise scrolling orbits by default in an oblique view",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = screen_geometry(s);
    let facing = orientation(s);
    s.hover(Control::Viewport)?;
    s.scroll(18.0, -12.0, true, egui::Modifiers::SHIFT)?;
    s.require(
        panned(before, screen_geometry(s)) && orientation(s).abs_diff_eq(facing, 1e-5),
        "Shift with precise scrolling translates the model center while preserving scale and direction",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = screen_geometry(s);
    let facing = orientation(s);
    s.hover(Control::Viewport)?;
    s.pinch(0.2)?;
    s.require(
        zoomed_in_from_center(before, screen_geometry(s))
            && orientation(s).abs_diff_eq(facing, 1e-5),
        "Positive pinch increases projected scale while preserving the framed center and direction",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = screen_geometry(s);
    let facing = orientation(s);
    s.hover(Control::Viewport)?;
    s.scroll(0.0, 2.0, false, egui::Modifiers::NONE)?;
    s.require(
        zoomed_in_from_center(before, screen_geometry(s))
            && orientation(s).abs_diff_eq(facing, 1e-5),
        "Positive mouse wheel scrolling increases scale while preserving the framed center and direction",
    )?;
    let before = matrix(s);
    let facing = orientation(s);
    s.hover(Control::Viewport)?;
    s.trackpad_rotate(15.0)?;
    s.require(
        !matrix(s).abs_diff_eq(before, 1e-5) && !orientation(s).abs_diff_eq(facing, 1e-5),
        "The native twist adapter rotates the viewing direction in Free navigation",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = matrix(s);
    s.hover(Control::Viewport)?;
    s.scroll(0.0, 0.0, true, egui::Modifiers::NONE)?;
    s.hover(Control::Viewport)?;
    s.scroll(0.0, 0.0, true, egui::Modifiers::SHIFT)?;
    s.hover(Control::Viewport)?;
    s.scroll(0.0, 0.0, false, egui::Modifiers::NONE)?;
    s.hover(Control::Viewport)?;
    s.pinch(0.0)?;
    s.hover(Control::Viewport)?;
    s.trackpad_rotate(0.0)?;
    s.require(
        matrix(s) == before,
        "Zero gesture deltas leave the camera exactly unchanged",
    )?;

    let dirty = s.state.is_dirty();
    for edit_mode in [false, true] {
        let id = s.state.editor.document.objects[0].id;
        let row = s
            .state
            .layer_row_rect(id)
            .ok_or("The fixture has no Layers row")?;
        s.click_at(row.center())?;
        if edit_mode {
            s.shortcut("edit.confirm")?;
        }
        let objects = s.state.editor.selected_objects.clone();
        let vertices = s.state.editor.selected_vertices.clone();
        for (control, direction) in [
            (Control::ViewFront, Vec3::Z),
            (Control::ViewBack, Vec3::NEG_Z),
            (Control::ViewRight, Vec3::X),
            (Control::ViewLeft, Vec3::NEG_X),
            (Control::ViewTop, Vec3::Y),
            (Control::ViewBottom, Vec3::NEG_Y),
        ] {
            s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
            complete_view(s, control, Some(direction))?;
            s.require(
                crate::ruler_2d::eligible(&s.state.camera, s.state.z_up)
                    && s.state.is_planar_navigation()
                    && s.state.ruler_2d_model.is_some(),
                "Each settled cardinal orthographic view has visible rulers before trackpad navigation",
            )?;
            let camera = s.state.camera.clone();
            let before = screen_geometry(s);
            let tutorial = !edit_mode && control == Control::ViewFront;
            if tutorial {
                s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
            }
            s.hover(Control::Viewport)?;
            s.trackpad_scroll(18.0, -12.0, egui::Modifiers::NONE, ScrollPhase::Started)?;
            if tutorial {
                s.require(
                    panned(before, screen_geometry(s)),
                    "The shared native scroll event pans the viewport and supplies its tutorial cue in the same replay step",
                )?;
                s.capture_tutorial("navigation-aligned-trackpad")?;
            }
            // Replay rotation deltas between native scroll phases. This checks
            // the adapter policy, not macOS recognition of physical gestures.
            let before_twist = matrix(s);
            for degrees in [0.01, -0.01, 15.0, -30.0, 180.0] {
                s.hover(Control::Viewport)?;
                s.trackpad_rotate(degrees)?;
            }
            s.require(
                matrix(s) == before_twist && s.state.is_planar_navigation(),
                "Synthetic native twist deltas leave Planar navigation and its camera unchanged while panning, in all six views and both editing modes",
            )?;
            s.hover(Control::Viewport)?;
            s.trackpad_scroll(0.0, 0.0, egui::Modifiers::NONE, ScrollPhase::Ended)?;
            s.settle()?;
            s.require(
                panned(before, screen_geometry(s))
                    && [Vec3::X, Vec3::Y, Vec3::Z].into_iter().all(|axis| {
                        s.state.camera.direction_in_view(axis)
                            .abs_diff_eq(camera.direction_in_view(axis), 1e-5)
                    })
                    && crate::ruler_2d::eligible(&s.state.camera, s.state.z_up)
                    && s.state.is_planar_navigation()
                    && s.state.ruler_2d_model.is_some()
                    && s.state.editor.edit_mode == edit_mode,
                "Unmodified precise scrolling pans all six aligned views in object and vertex modes, preserving orientation, scale and rulers",
            )?;
            let before = screen_geometry(s);
            s.hover(Control::Viewport)?;
            s.pinch(0.1)?;
            let before_twist = matrix(s);
            s.hover(Control::Viewport)?;
            s.trackpad_rotate(-12.0)?;
            s.require(
                matrix(s) == before_twist && s.state.is_planar_navigation(),
                "A synthetic twist after pinch leaves the Planar view and zoom unchanged",
            )?;
            s.settle()?;
            s.require(
                screen_geometry(s).1 > before.1 + 1e-5
                    && [Vec3::X, Vec3::Y, Vec3::Z].into_iter().all(|axis| {
                        s.state
                            .camera
                            .direction_in_view(axis)
                            .abs_diff_eq(camera.direction_in_view(axis), 1e-5)
                    })
                    && s.state.ruler_2d_model.is_some(),
                "Pinching zooms each aligned view without rotating it or hiding the rulers",
            )?;
            let before = screen_geometry(s);
            let facing = orientation(s);
            s.hover(Control::Viewport)?;
            s.trackpad_scroll(18.0, -12.0, egui::Modifiers::SHIFT, ScrollPhase::Started)?;
            s.hover(Control::Viewport)?;
            s.trackpad_scroll(16.0, 10.0, egui::Modifiers::SHIFT, ScrollPhase::Moved)?;
            s.hover(Control::Viewport)?;
            s.trackpad_scroll(8.0, 6.0, egui::Modifiers::SHIFT, ScrollPhase::Ended)?;
            s.settle()?;
            s.require(
                panned(before, screen_geometry(s))
                    && orientation(s).abs_diff_eq(facing, 1e-5)
                    && s.state.is_planar_navigation()
                    && s.state.ruler_2d_model.is_some(),
                "Shift scrolling pans through every native phase while preserving each aligned view and its rulers",
            )?;
            s.modifiers_changed(egui::Modifiers::NONE)?;
            let start = s.state.viewport.center() + egui::vec2(-35.0, 20.0);
            s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
            s.shortcut_down("navigation.orbit")?;
            s.pointer_button(egui::PointerButton::Primary, true)?;
            s.frame(
                vec![egui::Event::PointerMoved(start + egui::vec2(36.0, -24.0))],
                Duration::ZERO,
            )?;
            s.pointer_button(egui::PointerButton::Primary, false)?;
            s.shortcut_up("navigation.orbit")?;
            s.require(
                !orientation(s).abs_diff_eq(facing, 1e-5)
                    && !s.state.is_planar_navigation()
                    && s.state.camera.is_orthographic()
                    && s.state.ruler_2d_model.is_none(),
                "The orbit modifier with a primary drag orbits out of every aligned view, hiding the rulers while retaining orthographic projection",
            )?;
            let before = screen_geometry(s);
            let facing = orientation(s);
            s.hover(Control::Viewport)?;
            s.trackpad_scroll(18.0, -12.0, egui::Modifiers::SHIFT, ScrollPhase::Started)?;
            s.hover(Control::Viewport)?;
            s.trackpad_scroll(0.0, 0.0, egui::Modifiers::SHIFT, ScrollPhase::Ended)?;
            s.settle()?;
            s.require(
                panned(before, screen_geometry(s)) && orientation(s).abs_diff_eq(facing, 1e-5),
                "Shift scrolling also pans in 3D without depending on the previous navigation mode",
            )?;
            s.modifiers_changed(egui::Modifiers::NONE)?;
            let before = orientation(s);
            s.hover(Control::Viewport)?;
            s.scroll(16.0, 10.0, true, egui::Modifiers::NONE)?;
            s.settle()?;
            s.require(
                !orientation(s).abs_diff_eq(before, 1e-5)
                    && s.state.ruler_2d_model.is_none()
                    && s.state.editor.document == document
                    && s.state.is_dirty() == dirty
                    && s.state.editor.selected_objects == objects
                    && s.state.editor.selected_vertices == vertices,
                "Plain precise scrolling continues orbiting once oblique, while trackpad navigation preserves geometry, selection and document dirty state",
            )?;
        }
    }
    s.shortcut("edit.confirm")?;
    s.require(
        !s.state.editor.edit_mode,
        "Trackpad examples return to object mode",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;

    s.require(
        !s.state.show_preferences
            && s.trace.get(Control::PreciseScroll).is_err()
            && s.trace.get(Control::AnimateViews).is_err()
            && s.trace.get(Control::Duration).is_err(),
        "Settings remain hidden until Preferences is opened",
    )?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.require(
        s.state.show_preferences
            && s.trace.get(Control::PreciseScroll)?.parents == [Control::PreferencesWindow],
        "Preferences contains the precise-scroll setting",
    )?;
    let precise_before_scroll = s.trace.get(Control::PreciseScroll)?.rect;
    s.hover(Control::PreferencesWindow)?;
    s.scroll(0.0, -600.0, true, egui::Modifiers::NONE)?;
    s.wait(Duration::from_millis(350))?;
    let precise_after_scroll = s.trace.get(Control::PreciseScroll)?.rect;
    s.click(Control::PreciseScroll)?;
    s.require(
        s.state.precise_scroll_zoom,
        &format!("The precise-scroll zoom setting is enabled (before={precise_before_scroll:?}, after={precise_after_scroll:?}, window={:?}, popup={})", s.trace.get(Control::PreferencesWindow)?.rect, egui::Popup::is_any_open(&s.ctx)),
    )?;
    // Preferences covers the viewport center. Close it before replaying the
    // gesture so production pointer ownership permits camera navigation.
    s.close_preferences()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = screen_geometry(s);
    let facing = orientation(s);
    s.hover(Control::Viewport)?;
    s.scroll(0.0, 24.0, true, egui::Modifiers::NONE)?;
    s.require(
        zoomed_in_from_center(before, screen_geometry(s))
            && orientation(s).abs_diff_eq(facing, 1e-5),
        "Enabled positive precise-scroll zoom increases scale in an oblique view while preserving the framed center and direction",
    )?;
    complete_view(s, Control::ViewFront, Some(Vec3::Z))?;
    let before = screen_geometry(s);
    let facing = orientation(s);
    s.hover(Control::Viewport)?;
    s.scroll(18.0, -12.0, true, egui::Modifiers::NONE)?;
    s.settle()?;
    s.require(
        panned(before, screen_geometry(s))
            && orientation(s).abs_diff_eq(facing, 1e-5)
            && s.state.ruler_2d_model.is_some(),
        "Aligned-view panning takes precedence over the precise-scroll zoom preference",
    )?;
    let before = screen_geometry(s);
    s.hover(Control::Viewport)?;
    s.scroll(18.0, -12.0, true, egui::Modifiers::SHIFT)?;
    s.settle()?;
    s.require(
        panned(before, screen_geometry(s))
            && orientation(s).abs_diff_eq(facing, 1e-5)
            && s.state.is_planar_navigation()
            && s.state.ruler_2d_model.is_some(),
        "Shift still pans an aligned view when the precise-scroll zoom preference is enabled",
    )?;
    s.modifiers_changed(egui::Modifiers::NONE)?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.hover(Control::PreferencesWindow)?;
    s.scroll(0.0, -600.0, true, egui::Modifiers::NONE)?;
    s.wait(Duration::from_millis(350))?;
    s.click(Control::PreciseScroll)?;
    s.require(
        !s.state.precise_scroll_zoom,
        "The precise-scroll setting can be restored",
    )?;
    s.close_preferences()?;
    s.require(
        !s.state.show_preferences && s.trace.get(Control::PreciseScroll).is_err(),
        "Closing Preferences removes the precise-scroll control from the viewport",
    )?;
    complete_view(s, Control::ViewPerspective, None)?;

    // Release widget focus through real viewport pointer input, then deliver the
    // key through the shared UI/input path and capture it while still held.
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(55.0, -75.0);
    drag_mouse(
        s,
        egui::PointerButton::Secondary,
        start,
        egui::vec2(20.0, 10.0),
    )?;
    s.click(Control::Viewport)?;
    let before_face_command = matrix(s);
    let grid = s.state.show_grid;
    s.require(
        !before_face_command.abs_diff_eq(framed, 1e-5),
        "The Make Face navigation check starts from a view that differs from Frame",
    )?;
    s.shortcut("mesh.make-face")?;
    s.require(
        matrix(s) == before_face_command && s.state.show_grid == grid,
        "Make Face does not frame the camera or toggle Grid",
    )?;
    // Let the prior click cue expire; the next illustration teaches the key.
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    let open = s.shortcut_down("view.frame")?;
    s.require(
        !open && s.shortcut_is_down("view.frame")? && matrix(s).abs_diff_eq(framed, 1e-5),
        "Pressing the Frame all shortcut invokes Frame and remains observable while held",
    )?;
    s.capture_tutorial("navigation-frame-key")?;
    s.require(
        s.shortcut_is_down("view.frame")? && matrix(s).abs_diff_eq(framed, 1e-5),
        "The keyboard tutorial capture preserves the held Frame all shortcut and the framed view",
    )?;
    s.shortcut_up("view.frame")?;
    s.require(
        !s.shortcut_is_down("view.frame")?,
        "Releasing the Frame all shortcut clears its held key and modifier state",
    )?;

    // Exercise the same processed keyboard events used by the native host.
    for expected in [true, false] {
        s.shortcut("view.projection")?;
        s.require(
            s.state.camera.is_orthographic() == expected,
            "The top-row projection shortcut toggles projection",
        )?;
    }

    let before_unassigned_key = matrix(s);
    s.key(egui::Key::H, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::H, false, egui::Modifiers::NONE)?;
    s.require(
        matrix(s) == before_unassigned_key && !s.state.show_preferences,
        "H is unassigned and does not change the camera or open Preferences",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before_menu = matrix(s);
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.hover(Control::ViewTop)?;
    // Explicit tutorial time finishes the popup's actual fade and prior cues.
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.settle()?;
    let top = s.trace.get(Control::ViewTop)?.clone();
    s.require(
        top.parents == [Control::N3Menu, Control::ViewMenu]
            && s.input
                .position()
                .is_some_and(|position| top.rect.contains(position))
            && matrix(s).abs_diff_eq(before_menu, 1e-5),
        "Hovering Top keeps the real View popup open without changing the camera",
    )?;
    s.capture_tutorial("navigation-view-menu")?;
    s.click(Control::ViewPerspective)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.require(
        s.trace.get(Control::ViewTop).is_err()
            && !s.state.camera.is_transitioning()
            && !s.state.camera.is_orthographic(),
        "Selecting Perspective closes the View popup and finishes the requested view",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    complete_view(s, Control::ViewFront, Some(Vec3::Z))?;
    s.settle()?;
    s.witness(Control::Projection)?;
    s.capture_image("navigation")?;
    verify_planar_zoom(s)
}

/// Exercise pointer anchoring through the native/replay funnel without adding
/// another illustration. The point may be empty space, not a picked surface.
fn verify_planar_zoom(s: &mut Session<'_>) -> Result<()> {
    let command = egui::Modifiers {
        command: true,
        mac_cmd: true,
        ..Default::default()
    };
    for ruler_visible in [true, false] {
        if !ruler_visible {
            s.shortcut("view.2d-ruler")?;
        }
        let rect = s.state.viewport;
        let pointer = rect.center() + egui::vec2(rect.width() * 0.2, rect.height() * 0.15);
        s.frame(vec![egui::Event::PointerMoved(pointer)], Duration::ZERO)?;
        let anchor = Vec2::new(0.4, -0.3);
        for source in 0..3 {
            let point = matrix(s).inverse().project_point3(anchor.extend(0.5));
            let scale = screen_geometry(s).1;
            match source {
                0 => s.pinch(0.12)?,
                1 => s.scroll(0.0, 1.0, false, egui::Modifiers::NONE)?,
                _ => {
                    // A zero initial pan phase followed by Command must choose
                    // zoom; releasing Command must restore pan in this gesture.
                    s.trackpad_scroll(0.0, 0.0, egui::Modifiers::NONE, ScrollPhase::Started)?;
                    s.modifiers_changed(command)?;
                    s.trackpad_scroll(0.0, 15.0, command, ScrollPhase::Moved)?;
                }
            }
            s.require(
                matrix(s).project_point3(point).truncate().abs_diff_eq(anchor, 1e-4)
                    && screen_geometry(s).1 > scale + 1e-5
                    && s.state.is_planar_navigation(),
                "2D pinch, wheel and Command-scroll magnify around the cursor with the ruler visible or hidden",
            )?;
        }
        s.modifiers_changed(egui::Modifiers::NONE)?;
        let before = screen_geometry(s);
        s.trackpad_scroll(10.0, 12.0, egui::Modifiers::NONE, ScrollPhase::Moved)?;
        s.require(
            panned(before, screen_geometry(s)),
            "Releasing Command restores 2D panning during the scroll gesture",
        )?;
        s.trackpad_scroll(0.0, 0.0, egui::Modifiers::NONE, ScrollPhase::Ended)?;
    }
    Ok(())
}
