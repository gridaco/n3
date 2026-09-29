use super::{Result, Session, pointer};
use crate::{controls::Control, units::LengthUnit};
use std::time::Duration;

fn x(s: &Session<'_>) -> f64 {
    s.state.editor.document.objects[0].transform.translation[0]
}

fn step(s: &mut Session<'_>) -> Result<f64> {
    s.state
        .editor
        .movement_snap_step(s.state.viewport, &s.state.camera, s.state.z_up)
        .ok_or_else(|| "The selected Move tool must expose its resolved snap step".into())
}

fn on_grid(value: f64, step: f64) -> bool {
    (value / step - (value / step).round()).abs() < 1e-8
}

fn locked_drag(s: &mut Session<'_>) -> Result<egui::Pos2> {
    s.shortcut("transform.axis-x")?;
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(38.0, -105.0);
    let end = start + egui::vec2(63.0, 0.0);
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(vec![pointer(start, true)], Duration::ZERO)?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    Ok(end)
}

fn cancel_drag(s: &mut Session<'_>, end: egui::Pos2) -> Result<()> {
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.shortcut("cancel").map(|_| ())
}

fn view(s: &mut Session<'_>, control: Control) -> Result<()> {
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(control)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()
}

fn type_value(s: &mut Session<'_>, control: Control, value: &str) -> Result<()> {
    s.click(control)?;
    // These are the text widget's native editing keys, not viewport commands.
    s.key(egui::Key::A, true, egui::Modifiers::COMMAND)?;
    s.key(egui::Key::A, false, egui::Modifiers::NONE)?;
    s.frame(vec![egui::Event::Text(value.into())], Duration::ZERO)?;
    s.key(egui::Key::Enter, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Enter, false, egui::Modifiers::NONE)?;
    Ok(())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("suzanne.obj")?;
    let document = s.state.editor.document.clone();
    let id = document.objects[0].id;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    view(s, Control::ViewFront)?;
    let row = s
        .state
        .layer_row_rect(id)
        .ok_or("Suzanne has no Layers row")?;
    s.click_at(row.center())?;
    s.click(Control::ToolMove)?;
    s.click_at(s.state.viewport.center())?;
    s.require(
        s.state.editor.selected_object == Some(id),
        "Suzanne is selected in Front view without changing its authored centimeter dimensions",
    )?;

    let overview_step = step(s)?;
    let end = locked_drag(s)?;
    let snapped = s.state.editor.document.clone();
    let overview_x = x(s);
    s.require(
        x(s) > 0.0 && on_grid(x(s), overview_step) && overview_step < 1.0
            && snapped.objects[0].transform.translation[1..] == [0.0, 0.0]
            && snapped.objects[0].geometry == document.objects[0].geometry
            && s.state.editor.is_transforming(),
        "Default Auto resolves a clean sub-centimeter viewport step for Suzanne and moves only the locked axis without changing its mesh",
    )?;
    let captured_step = step(s)?;
    s.require(
        captured_step == overview_step,
        "The active drag reports the interval captured at its beginning",
    )?;
    s.value("overview-step", overview_step);
    s.witness(Control::PositionX)?;
    s.witness(Control::MoveAxisLock)?;
    s.witness(Control::SnapFeedback)?;
    let snap_feedback = s.trace.get(Control::SnapFeedback)?.rect;
    let stats = s.trace.get(Control::SceneInfo)?.rect;
    s.require(
        (snap_feedback.left() - stats.left()).abs() < 1.0
            && (stats.top() - snap_feedback.bottom() - crate::theme::space::LG).abs() < 1.0,
        "Snap feedback stacks above the scene stats at the viewport's bottom left",
    )?;
    s.capture_tutorial("snapping-grid-move")?;
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.document == snapped && !s.state.editor.has_transform_session(),
        "Confirm accepts the Auto-snapped preview as the usual Move session",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == document,
        "One Undo restores the complete Auto drag",
    )?;
    s.redo()?;
    s.require(
        s.state.editor.document == snapped,
        "Redo restores the same snapped coordinates",
    )?;
    s.undo()?;

    // Exercise the same native pinch adapter as the navigation guide. This is
    // a software mapping check, not a claim about physical trackpad feel.
    s.hover(Control::Viewport)?;
    s.pinch(0.6)?;
    s.settle()?;
    let detail_step = step(s)?;
    let end = locked_drag(s)?;
    s.require(
        detail_step < overview_step && x(s) > 0.0 && x(s) < overview_x
            && on_grid(x(s), detail_step),
        "Zooming in selects a finer Auto step for the next drag and the same pointer travel moves a smaller physical distance",
    )?;
    s.value("detail-step", detail_step);
    s.capture_tutorial("snapping-detail-move")?;
    cancel_drag(s, end)?;
    s.require(
        s.state.editor.document == document,
        "Cancel restores the entire close-up preview",
    )?;

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    view(s, Control::ViewPerspective)?;
    let perspective_step = step(s)?;
    let end = locked_drag(s)?;
    s.require(
        perspective_step > 0.0 && perspective_step < 1.0 && x(s) > 0.0
            && on_grid(x(s), perspective_step)
            && s.trace.get(Control::SnapFeedback).is_ok(),
        "Perspective movement uses a clean depth-local Auto step and shows its increment without rulers",
    )?;
    cancel_drag(s, end)?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    view(s, Control::ViewFront)?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.witness(Control::SnapGrid)?;
    s.click_path(&[
        Control::N3Menu,
        Control::Preferences,
        Control::SnapSpacingMenu,
    ])?;
    s.witness(Control::SnapAdaptive)?;
    s.click_path(&[
        Control::N3Menu,
        Control::Preferences,
        Control::SnapSpacingMenu,
        Control::SnapFixed,
    ])?;
    type_value(s, Control::SnapStep, "1")?;
    let window = s.trace.get(Control::PreferencesWindow)?.rect;
    s.drag_at(
        egui::pos2(window.left() + 80.0, window.top() + 30.0),
        egui::vec2(0.0, 70.0),
    )?;
    s.hover(Control::SnapStep)?;
    s.frame(Vec::new(), Duration::from_millis(350))?;
    s.capture_tutorial("snapping-preferences")?;
    s.close_preferences()?;
    let fixed_step = step(s)?;
    s.require(
        fixed_step == 1.0,
        "Fixed spacing exposes the explicitly configured one-centimeter interval",
    )?;
    let end = locked_drag(s)?;
    s.require(
        on_grid(x(s), 1.0),
        "Fixed spacing quantizes the same viewport gesture to the requested measurement grid",
    )?;
    cancel_drag(s, end)?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    // Coarse pointer spacing must never swallow an explicit keyboard nudge.
    type_value(s, Control::SnapStep, "20")?;
    s.require(
        s.state.editor.snapping.step_cm == 20.0,
        &format!(
            "The visible Fixed-spacing field accepted 20 cm (step={}, field={:?}, window={:?})",
            s.state.editor.snapping.step_cm,
            s.trace.get(Control::SnapStep)?.rect,
            s.trace.get(Control::PreferencesWindow)?.rect
        ),
    )?;
    s.close_preferences()?;
    s.click_at(s.state.viewport.center())?;
    s.shortcut("nudge.right")?;
    s.require(
        x(s) == 1.0,
        &format!(
            "An explicit arrow nudge remains one centimeter even when Fixed pointer spacing is twenty centimeters (x={}, step={}, preferences={}, selected={:?})",
            x(s),
            s.state.editor.snapping.step_cm,
            s.state.show_preferences,
            s.state.editor.selected_object
        ),
    )?;
    s.undo()?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click_path(&[
        Control::N3Menu,
        Control::Preferences,
        Control::SnapSpacingMenu,
    ])?;
    s.click_path(&[
        Control::N3Menu,
        Control::Preferences,
        Control::SnapSpacingMenu,
        Control::SnapAdaptive,
    ])?;
    s.click_path(&[Control::N3Menu, Control::Preferences, Control::SnapGrid])?;
    s.close_preferences()?;
    let end = locked_drag(s)?;
    s.require(
        x(s) > 0.0 && !on_grid(x(s), overview_step),
        "Disabling snapping retains the fractional pointer result instead of rounding to the Auto grid",
    )?;
    cancel_drag(s, end)?;
    s.require(
        s.state.editor.document == document,
        "Cancelling disabled snapping restores the baseline without a document edit",
    )?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click_path(&[Control::N3Menu, Control::Preferences, Control::SnapGrid])?;
    s.close_preferences()?;

    let field = s.target(Control::PositionX)?;
    s.frame(vec![egui::Event::PointerMoved(field)], Duration::ZERO)?;
    s.frame(vec![pointer(field, true)], Duration::ZERO)?;
    for pixels in [10.0, 12.0, 14.0, 16.0, 20.0, 24.0, 30.0, 25.0, 20.0, 15.0] {
        s.frame(
            vec![egui::Event::PointerMoved(field + egui::vec2(pixels, 0.0))],
            Duration::ZERO,
        )?;
        let expected = ((f64::from(pixels) * 0.02) / 0.05).round() * 0.05;
        s.require(
            (x(s) - expected).abs() < 1e-8 && s.state.editor.has_property_edit(),
            "Small Position-scrub updates retain accumulated pointer motion, cross fine Auto thresholds, and reverse within one live property session",
        )?;
        let scrub_step = step(s)?;
        s.require(scrub_step == 0.05, "Position scrubbing exposes its sensitivity-derived 0.05 cm interval rather than viewport interval")?;
    }
    s.frame(
        vec![pointer(field + egui::vec2(15.0, 0.0), false)],
        Duration::ZERO,
    )?;
    s.settle()?;
    s.require(
        (x(s) - 0.3).abs() < 1e-8 && !s.state.editor.has_property_edit(),
        "Releasing the Position scrub commits its clean fractional preview",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == document,
        "The entire forward-and-reverse Position scrub is one Undo step",
    )?;

    type_value(s, Control::PositionX, "0.125 cm")?;
    s.require(
        x(s) == 0.125,
        "Typed Position retains its exact fractional value while Auto snapping is enabled",
    )?;
    let away = s.state.viewport_ui_rect.left_bottom() + egui::vec2(35.0, -85.0);
    s.frame(
        vec![egui::Event::PointerMoved(away)],
        Duration::from_millis(350),
    )?;
    s.capture_tutorial("snapping-precise-input")?;
    s.undo()?;
    s.require(
        s.state.editor.document == document,
        "One Undo restores the exact typed edit",
    )?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click(Control::LengthUnitMenu)?;
    s.click(Control::LengthMillimeters)?;
    s.close_preferences()?;
    s.require(
        s.state.display_unit == LengthUnit::Millimeters,
        "The snapping example switches its display to millimeters",
    )?;
    s.drag(Control::PositionX, egui::vec2(30.0, 0.0))?;
    s.require((x(s) - 0.6).abs() < 1e-8 && on_grid(x(s), 0.05), "Millimeter display preserves the Position field's canonical Auto sensitivity and 0.05 cm interval")?;
    s.undo()?;
    s.require(
        s.state.editor.document == document,
        "Display changes and snapping preferences add no document edits",
    )?;
    Ok(())
}
