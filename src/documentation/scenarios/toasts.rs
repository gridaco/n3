use super::{Result, Session};
use crate::{controls::Control, editor::Tool};
use std::time::Duration;

// A is intentionally not an N3 binding: this exercises the advisory fallback.
fn unfamiliar_key(s: &mut Session<'_>) -> Result<()> {
    s.key(egui::Key::A, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::A, false, egui::Modifiers::NONE)?;
    Ok(())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.click(Control::InsertMenu)?;
    s.click(Control::InsertCube)?;
    s.click_at(s.state.viewport_ui_rect.left_bottom() + egui::vec2(20., -20.))?;
    let document = s.state.editor.document.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    s.require(
        s.state.editor.selected_objects.is_empty(),
        "The hint starts with no selection",
    )?;
    unfamiliar_key(s)?;
    s.require(
        s.trace.get(Control::ToastStack).is_ok()
            && s.trace.get(Control::ToastAction)?.label == Control::SelectAll.label()
            && s.state.editor.selected_objects.is_empty()
            && s.state.editor.document == document
            && s.state.editor.tool == Tool::View
            && s.state.camera.view_projection(s.state.aspect()) == camera,
        "Unassigned A offers Select all without selecting objects, changing tools, or changing the document or view",
    )?;
    s.witness(Control::ToastDismiss)?;
    s.witness(Control::ToastAction)?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("toasts-shortcut-hint")?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.require(
        s.trace.get(Control::ToastStack).is_err(),
        "Preferences owns its space while the toast waits",
    )?;
    s.wait(Duration::from_secs(8))?;
    s.click(Control::AppearanceThemeMenu)?;
    s.click(Control::ThemeDark)?;
    s.click(Control::PreferencesClose)?;
    s.require(
        s.trace.get(Control::ToastStack).is_ok(),
        "The toast resumes with its remaining lifetime after Preferences closes",
    )?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("toasts-shortcut-hint-dark")?;

    // Keyboard access starts from the viewport. Escape returns without taking
    // the editor's ordinary deselect/cancel path in this same input frame.
    s.click_at(s.state.viewport_ui_rect.left_bottom() + egui::vec2(20., -20.))?;
    let previous_focus = s.ctx.memory(|memory| memory.focused());
    s.shortcut("notifications.focus")?;
    let focused = s.ctx.memory(|memory| memory.focused());
    s.require(
        focused
            .and_then(|id| s.ctx.read_response(id))
            .is_some_and(|response| {
                response.rect == s.trace.get(Control::ToastAction).unwrap().rect
            }),
        "The notification shortcut focuses its actual action button",
    )?;
    s.wait(Duration::from_secs(8))?;
    s.require(
        s.trace.get(Control::ToastStack).is_ok(),
        "Keyboard focus preserves reading time",
    )?;
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)?;
    s.require(
        s.ctx.memory(|memory| memory.focused()) == previous_focus
            && s.trace.get(Control::ToastStack).is_ok()
            && s.state.editor.document == document
            && s.state.camera.view_projection(s.state.aspect()) == camera,
        "Escape returns keyboard focus while keeping the toast and editor unchanged",
    )?;

    s.hover(Control::ToastAction)?;
    s.wait(Duration::from_secs(8))?;
    s.require(
        s.trace.get(Control::ToastAction).is_ok(),
        "Hovering preserves reading and action time",
    )?;
    s.shortcut("notifications.focus")?;
    s.key(egui::Key::Enter, true, egui::Modifiers::NONE)?;
    s.key(egui::Key::Enter, false, egui::Modifiers::NONE)?;
    s.require(
        s.trace.get(Control::ToastStack).is_err()
            && s.state.editor.selected_objects.len() == 1
            && s.state.editor.document == document
            && s.state.camera.view_projection(s.state.aspect()) == camera,
        &format!("The toast action dispatches ordinary Select all once, dismisses itself, and changes only selection (toast={}, selected={}, document_unchanged={}, camera_unchanged={})",
            s.trace.get(Control::ToastStack).is_ok(), s.state.editor.selected_objects.len(),
            s.state.editor.document == document, s.state.camera.view_projection(s.state.aspect()) == camera),
    )?;
    unfamiliar_key(s)?;
    s.require(
        s.trace.get(Control::ToastStack).is_err(),
        "A shortcut hint is not repeated during the session",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document.objects.is_empty(),
        "The hint and its selection action add no undo steps; Undo removes the inserted cube",
    )?;
    Ok(())
}
