//! Real UI input ownership regressions, shared with the native event dispatcher.
use super::{Capture, HEIGHT, Session, WIDTH, pointer};
use crate::{camera::View, document::PrimitiveKind, shortcuts::viewport_focus_id};
use egui::{Event, Key, Modifiers};
use std::time::Duration;

fn key(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn tap(s: &mut Session<'_>, key: Key) {
    s.frame(
        vec![self::key(key, true), self::key(key, false)],
        Duration::ZERO,
    )
    .unwrap();
}

fn type_native(s: &mut Session<'_>, input: &str) {
    let mut events = Vec::new();
    for c in input.chars() {
        let logical = match c {
            '0' => Key::Num0,
            '1' => Key::Num1,
            '2' => Key::Num2,
            '3' => Key::Num3,
            '4' => Key::Num4,
            '5' => Key::Num5,
            '6' => Key::Num6,
            '7' => Key::Num7,
            '8' => Key::Num8,
            '9' => Key::Num9,
            '.' => Key::Period,
            '-' => Key::Minus,
            _ => panic!("unsupported test input"),
        };
        events.extend([
            key(logical, true),
            Event::Text(c.into()),
            key(logical, false),
        ]);
    }
    s.extra_layout_pass = true;
    s.frame(events, Duration::ZERO).unwrap();
}

fn setup(s: &mut Session<'_>) {
    s.state.editor.insert(PrimitiveKind::Cube).unwrap();
    s.state.editor.set_tool(crate::editor::Tool::Move);
    s.state.camera.set_view(View::Front);
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.settle().unwrap();
}

#[test]
fn typing_during_locked_drag_takes_over_pointer_then_commits_one_undo() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let baseline = s.state.editor.document.clone();
    tap(&mut s, Key::X);
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(60.0, -85.0);
    s.frame(
        vec![Event::PointerMoved(start), pointer(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![Event::PointerMoved(start + egui::vec2(50.0, 0.0))],
        Duration::ZERO,
    )
    .unwrap();
    assert_ne!(s.state.editor.document, baseline);
    type_native(&mut s, "-0.125");
    assert_eq!(s.state.editor.numeric_text(), Some("-0.125"));
    assert_eq!(
        s.state.editor.document.objects[0].transform.translation[0],
        -0.125
    );
    let typed = s.state.editor.document.clone();
    let end = start + egui::vec2(100.0, 20.0);
    s.frame(
        vec![Event::PointerMoved(end), pointer(end, false)],
        Duration::ZERO,
    )
    .unwrap();
    s.settle().unwrap();
    assert_eq!(
        s.state.editor.document, typed,
        "pointer cannot overwrite typed precision"
    );
    tap(&mut s, Key::Enter);
    assert!(!s.state.editor.has_transform_session());
    assert!(!s.state.editor.edit_mode);
    s.undo().unwrap();
    assert_eq!(
        s.state.editor.document, baseline,
        "drag and typing form one undo step"
    );
    s.redo().unwrap();
    assert_eq!(s.state.editor.document, typed);
}

#[test]
fn numeric_decimal_and_backspace_never_toggle_view_or_delete_objects() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let baseline = s.state.editor.document.clone();
    let planar = s.state.is_planar_navigation();
    tap(&mut s, Key::X);
    type_native(&mut s, ".5");
    assert_eq!(
        s.state.editor.document.objects[0].transform.translation[0],
        0.5
    );
    assert_eq!(s.state.is_planar_navigation(), planar);
    tap(&mut s, Key::Backspace);
    assert_eq!(s.state.editor.numeric_text(), Some("."));
    tap(&mut s, Key::Enter);
    assert!(
        s.state.editor.has_transform_session(),
        "an incomplete decimal cannot commit"
    );
    assert!(s.state.error.is_some());
    tap(&mut s, Key::Backspace);
    assert_eq!(
        s.state.editor.document, baseline,
        "clearing input restores the operation baseline"
    );
    tap(&mut s, Key::Escape);
    assert_eq!(s.state.editor.document, baseline);
    assert_eq!(s.state.editor.selected_objects.len(), 1);
    assert!(!s.state.editor.edit_mode);
}

#[test]
fn numeric_input_focus_loss_cancels_preview_without_history_entry() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let baseline = s.state.editor.document.clone();
    tap(&mut s, Key::E);
    tap(&mut s, Key::Y);
    type_native(&mut s, "1.5");
    assert_eq!(s.state.editor.document.objects[0].transform.scale[1], 1.5);
    s.frame(vec![Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    assert_eq!(s.state.editor.document, baseline);
    assert!(s.state.editor.numeric_text().is_none());
    assert!(s.state.editor.transform_axis.is_none());
    s.frame(vec![Event::WindowFocused(true)], Duration::ZERO)
        .unwrap();
    s.undo().unwrap();
    assert!(
        s.state.editor.document.objects.is_empty(),
        "only original insertion is in history"
    );
}
