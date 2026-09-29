use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{shortcuts::Command, ui::toast::Toast};
use egui::{Key, Modifiers};

fn assert_focused(s: &Session<'_>, control: Control) {
    let id = s.ctx.memory(|memory| memory.focused()).unwrap();
    assert_eq!(
        s.ctx.read_response(id).unwrap().rect,
        s.trace.get(control).unwrap().rect,
    );
}

fn unfamiliar_key(s: &mut Session<'_>) {
    s.key(Key::A, true, Modifiers::NONE).unwrap();
    s.key(Key::A, false, Modifiers::NONE).unwrap();
}

#[test]
fn shortcut_hints_wait_for_idle_and_dismiss_without_changing_editor_state() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.click(Control::InsertMenu).unwrap();
    s.click(Control::InsertCube).unwrap();
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    s.shortcut_down("navigation.pan").unwrap();
    unfamiliar_key(&mut s);
    assert!(s.trace.get(Control::ToastStack).is_err());
    s.shortcut_up("navigation.pan").unwrap();
    s.extra_layout_pass = true;
    unfamiliar_key(&mut s);
    assert!(s.trace.get(Control::ToastStack).is_ok());
    s.click(Control::ToastDismiss).unwrap();
    assert!(s.trace.get(Control::ToastStack).is_err());
    unfamiliar_key(&mut s);
    assert!(s.trace.get(Control::ToastStack).is_err());
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.selected_objects, selection);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
    s.undo().unwrap();
    assert!(s.state.editor.document.objects.is_empty());
}

#[test]
fn toast_actions_dispatch_once_after_layout_retries_and_preserve_host_effects() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.click(Control::InsertMenu).unwrap();
    s.click(Control::InsertCube).unwrap();
    let original = s.state.editor.document.clone();
    s.state.toasts.push(
        Toast::new("Duplicate the selected cube").action("Duplicate", Command::DuplicateSelection),
    );
    s.settle().unwrap();
    s.extra_layout_pass = true;
    s.click(Control::ToastAction).unwrap();
    assert_eq!(s.state.editor.document.objects.len(), 2);
    assert!(s.trace.get(Control::ToastStack).is_err());
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, original);

    s.state
        .toasts
        .push(Toast::new("Open a document").action("Open", Command::Open));
    s.settle().unwrap();
    assert!(
        s.click(Control::ToastAction).unwrap(),
        "Open reaches the host-effect boundary"
    );
    assert!(s.trace.get(Control::ToastStack).is_err());
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn toast_keyboard_access_preserves_ownership_selection_and_editor_history() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.click(Control::InsertMenu).unwrap();
    s.click(Control::InsertCube).unwrap();
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    s.ctx.memory_mut(|memory| {
        memory.request_focus(crate::shortcuts::viewport_focus_id());
    });
    s.settle().unwrap();
    let previous_focus = s.ctx.memory(|memory| memory.focused());
    s.shortcut("notifications.focus").unwrap();
    assert_eq!(s.ctx.memory(|memory| memory.focused()), previous_focus);

    s.state.toasts.push(
        Toast::new("Duplicate the selected cube").action("Duplicate", Command::DuplicateSelection),
    );
    s.settle().unwrap();
    // A held viewport gesture owns input; notification focus must not end it.
    s.shortcut_down("navigation.pan").unwrap();
    s.shortcut("notifications.focus").unwrap();
    assert_eq!(s.ctx.memory(|memory| memory.focused()), previous_focus);
    s.shortcut_up("navigation.pan").unwrap();
    s.extra_layout_pass = true;
    // Focus transfer owns the rest of this native batch, before the newly
    // focused button can receive input on the next frame.
    s.frame(
        vec![
            s.shortcut_event("notifications.focus", true).unwrap(),
            s.shortcut_event("selection.delete", true).unwrap(),
            s.shortcut_event("edit.confirm", true).unwrap(),
        ],
        std::time::Duration::ZERO,
    )
    .unwrap();
    s.settle().unwrap();
    s.shortcut_up("notifications.focus").unwrap();
    s.shortcut_up("selection.delete").unwrap();
    s.shortcut_up("edit.confirm").unwrap();
    assert_focused(&s, Control::ToastAction);
    assert_eq!(s.state.editor.document, document);
    assert!(!s.state.editor.edit_mode);
    s.key(Key::Escape, true, Modifiers::NONE).unwrap();
    s.key(Key::Escape, false, Modifiers::NONE).unwrap();
    assert_eq!(s.ctx.memory(|memory| memory.focused()), previous_focus);
    assert!(s.trace.get(Control::ToastStack).is_ok());
    assert_eq!(s.state.editor.selected_objects, selection);

    s.shortcut("notifications.focus").unwrap();
    s.key(Key::Tab, true, Modifiers::SHIFT).unwrap();
    s.key(Key::Tab, false, Modifiers::SHIFT).unwrap();
    assert_focused(&s, Control::ToastDismiss);
    s.key(Key::Enter, true, Modifiers::NONE).unwrap();
    s.key(Key::Enter, false, Modifiers::NONE).unwrap();
    assert!(s.trace.get(Control::ToastStack).is_err());
    assert_eq!(s.ctx.memory(|memory| memory.focused()), previous_focus);
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.selected_objects, selection);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
    assert!(!s.state.editor.edit_mode);
    s.undo().unwrap();
    assert!(s.state.editor.document.objects.is_empty());
}
