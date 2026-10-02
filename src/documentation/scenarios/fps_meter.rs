//! Show the actual developer control and passive overlay. Guide replay has an
//! explicit scenario clock, not a host surface-presentation cadence, so it must
//! never manufacture a numeric FPS result for a screenshot.
use super::{HEIGHT, Result, Session, WIDTH};
use crate::controls::Control;
use std::time::Duration;

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.require(
        !s.state.fps_meter.enabled() && s.trace.get(Control::FpsMeter).is_err(),
        "The FPS meter starts disabled in a fresh workspace",
    )?;
    s.click_path(&[Control::InsertMenu, Control::InsertCube])?;
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let revision = s.state.editor.revision;
    let settings = s.state.user_settings();
    let camera = s.state.camera.clone();

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::DeveloperMenu])?;
    s.witness(Control::ShowFpsMeter)?;
    s.require(
        s.trace.get(Control::ShowFpsMeter)?.parents
            == [Control::N3Menu, Control::ViewMenu, Control::DeveloperMenu]
            && s.trace.get(Control::ShowFpsMeter)?.enabled,
        "Show FPS Meter is available in N3, View, Developer",
    )?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("fps-meter-menu")?;
    s.click(Control::ShowFpsMeter)?;
    s.hover(Control::Viewport)?;
    let meter = s.trace.get(Control::FpsMeter)?.rect;
    let window =
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(WIDTH as f32, HEIGHT as f32));
    s.require(
        s.state.fps_meter.enabled()
            && s.state.fps_meter.sample().is_none()
            && s.trace
                .get(Control::FpsMeter)?
                .label
                .contains("Waiting for frames")
            && s.trace
                .get(Control::FpsMeter)?
                .label
                .contains("last sample")
            && window.contains_rect(meter)
            && meter.right_bottom() == window.right_bottom()
            && !s.state.viewport.contains_rect(meter),
        "The menu enables the meter at the window's bottom-right corner, outside the viewport bounds and waiting for host frame samples",
    )?;
    // Advancing tutorial time renders the UI without pretending that replay
    // submissions are real native/browser surface presentation observations.
    s.wait(Duration::from_secs(1))?;
    s.require(
        s.state.fps_meter.sample().is_none(),
        "Tutorial clock advances do not invent a measured application FPS",
    )?;
    s.capture_image("fps-meter-waiting")?;

    s.shortcut("ui.toggle")?;
    s.require(
        !s.state.show_ui && s.state.fps_meter.enabled() && s.trace.get(Control::FpsMeter).is_err(),
        "Hide UI hides the meter without disabling its measurement",
    )?;
    s.shortcut("ui.toggle")?;
    s.require(
        s.state.show_ui
            && s.state.fps_meter.enabled()
            && s.trace.get(Control::FpsMeter)?.rect == meter,
        "Showing the UI restores the enabled FPS meter at the same window position",
    )?;
    s.click_path(&[
        Control::N3Menu,
        Control::ViewMenu,
        Control::DeveloperMenu,
        Control::ShowFpsMeter,
    ])?;
    s.require(
        !s.state.fps_meter.enabled()
            && s.state.fps_meter.sample().is_none()
            && s.trace.get(Control::FpsMeter).is_err()
            && s.state.editor.document == document
            && s.state.editor.selected_objects == selection
            && s.state.editor.revision == revision
            && s.state.user_settings() == settings
            && s.state.camera.eye() == camera.eye()
            && s.state.camera.orientation() == camera.orientation(),
        "The same menu disables the meter without changing document, selection, settings, or camera",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document.objects.is_empty(),
        "Meter toggles add no history: one Undo still removes the inserted cube",
    )?;
    Ok(())
}
