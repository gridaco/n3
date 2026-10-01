//! Real terminal input, layout retries, and independent emulator consumers.
use super::{
    Case, Workbench,
    replay::{Replay, wheel},
    terminal::Event as TerminalEvent,
};
use crate::{settings::ResolvedTheme, shortcuts};
use egui::{Event, Key, Modifiers, PointerButton};
use std::time::Duration;

fn frame(replay: &mut Replay, events: Vec<Event>) {
    replay.retry = true;
    let mut output = replay.frame(events, Duration::from_millis(16)).unwrap();
    output.textures_delta.clear();
}

fn settle(replay: &mut Replay) {
    for _ in 0..3 {
        frame(replay, Vec::new());
    }
}

fn fixture(case: Case) -> Replay {
    let mut replay = Replay::new(case, ResolvedTheme::Light);
    settle(&mut replay);
    replay
}

fn click(replay: &mut Replay, instance: usize, target: &str) {
    let point = replay.target(instance, target).unwrap().rect.center();
    frame(replay, vec![Event::PointerMoved(point)]);
    frame(
        replay,
        vec![Replay::button(point, PointerButton::Primary, true)],
    );
    frame(
        replay,
        vec![Replay::button(point, PointerButton::Primary, false)],
    );
    settle(replay);
}

#[test]
fn terminal_reset_restores_bytes_scroll_focus_and_stable_identity() {
    let mut replay = fixture(Case::TerminalPlaceholder);
    let identity = replay.target(0, "terminal-content").unwrap().id;
    let initial = replay.bench.terminal.as_ref().unwrap().sessions[0].visible_text();
    click(&mut replay, 0, "terminal-append");
    click(&mut replay, 0, "terminal-content");
    let point = replay.target(0, "terminal-content").unwrap().rect.center();
    frame(
        &mut replay,
        vec![
            Event::PointerMoved(point),
            wheel(egui::vec2(0.0, 90.0), Modifiers::NONE),
        ],
    );
    assert!(replay.bench.terminal.as_ref().unwrap().sessions[0].display_offset() > 0);
    replay.bench.appearance = ResolvedTheme::Dark;
    click(&mut replay, 0, "reset");
    let fixture = replay.bench.terminal.as_ref().unwrap();
    assert_eq!(fixture.sessions[0].visible_text(), initial);
    assert_eq!(fixture.sessions[0].display_offset(), 0);
    assert_eq!(fixture.batches, [0, 0]);
    assert!(fixture.events.is_empty());
    assert!(!fixture.views[0].is_focused(&replay.ctx));
    assert_eq!(replay.target(0, "terminal-content").unwrap().id, identity);
    assert_eq!(replay.bench.appearance, ResolvedTheme::Dark);
}

#[test]
fn terminal_instances_keep_ingestion_scrolling_and_focus_independent_across_layout_retries() {
    let mut replay = fixture(Case::TerminalIsolation);
    let first_id = replay.target(0, "terminal-content").unwrap().id;
    let second_id = replay.target(1, "terminal-content").unwrap().id;
    assert_ne!(first_id, second_id);
    let first_text = replay.bench.terminal.as_ref().unwrap().sessions[0].visible_text();
    click(&mut replay, 1, "terminal-append");
    assert_eq!(replay.bench.terminal.as_ref().unwrap().batches, [0, 1]);
    click(&mut replay, 1, "terminal-content");
    let point = replay.target(1, "terminal-content").unwrap().rect.center();
    frame(
        &mut replay,
        vec![
            Event::PointerMoved(point),
            wheel(egui::vec2(0.0, 90.0), Modifiers::NONE),
        ],
    );
    let fixture = replay.bench.terminal.as_ref().unwrap();
    assert_eq!(fixture.sessions[0].visible_text(), first_text);
    assert_eq!(fixture.sessions[0].display_offset(), 0);
    assert!(fixture.sessions[1].display_offset() > 0);
    assert!(!fixture.views[0].is_focused(&replay.ctx));
    assert!(fixture.views[1].is_focused(&replay.ctx));
    assert_eq!(
        fixture
            .events
            .iter()
            .filter(|event| matches!(event, TerminalEvent::SampleAppended { .. }))
            .count(),
        1
    );
    assert_eq!(
        replay.ctx.memory(|memory| memory.focused()),
        Some(second_id)
    );
    assert!(replay.bench.events.is_empty());
}

#[test]
fn terminal_owns_text_and_shortcut_keys_and_yields_to_text_fields_popups_and_focus_loss() {
    let mut replay = fixture(Case::TerminalPlaceholder);
    click(&mut replay, 0, "terminal-content");
    assert!(!shortcuts::viewport_keys_available(&replay.ctx));
    let original = replay.bench.terminal.as_ref().unwrap().sessions[0].visible_text();
    for key in [Key::Space, Key::Num2, Key::Enter] {
        frame(
            &mut replay,
            vec![
                Replay::literal(key, true),
                Event::Text("typed input".into()),
            ],
        );
        frame(&mut replay, vec![Replay::literal(key, false)]);
        assert!(!shortcuts::viewport_keys_available(&replay.ctx));
    }
    assert_eq!(
        replay.bench.terminal.as_ref().unwrap().sessions[0].visible_text(),
        original
    );
    assert!(replay.bench.events.is_empty());
    click(&mut replay, 0, "owner-field");
    frame(&mut replay, vec![Event::Text("fixture text".into())]);
    assert_eq!(replay.bench.text, "fixture text");
    assert!(!replay.bench.terminal.as_ref().unwrap().views[0].is_focused(&replay.ctx));
    click(&mut replay, 0, "owner-popup");
    assert!(egui::Popup::is_any_open(&replay.ctx));
    frame(&mut replay, vec![Replay::literal(Key::Num2, true)]);
    frame(&mut replay, vec![Replay::literal(Key::Num2, false)]);
    assert!(!shortcuts::viewport_keys_available(&replay.ctx));
    frame(&mut replay, vec![Replay::literal(Key::Escape, true)]);
    frame(&mut replay, vec![Replay::literal(Key::Escape, false)]);
    click(&mut replay, 0, "terminal-content");
    frame(&mut replay, vec![Event::WindowFocused(false)]);
    assert!(!replay.bench.terminal.as_ref().unwrap().views[0].is_focused(&replay.ctx));
    frame(&mut replay, vec![Event::WindowFocused(true)]);
    click(&mut replay, 0, "terminal-content");
    frame(&mut replay, vec![Replay::literal(Key::Escape, true)]);
    frame(&mut replay, vec![Replay::literal(Key::Escape, false)]);
    assert!(!replay.bench.terminal.as_ref().unwrap().views[0].is_focused(&replay.ctx));
}

#[test]
fn terminal_resizes_from_live_available_width_and_contains_parent_scrolling() {
    let mut replay = fixture(Case::TerminalNarrow);
    let initial_columns = replay.bench.terminal.as_ref().unwrap().sessions[0].columns();
    let start = replay.target(0, "available-width").unwrap().rect.center();
    let end = start - egui::vec2(400.0, 0.0);
    frame(&mut replay, vec![Event::PointerMoved(start)]);
    frame(
        &mut replay,
        vec![Replay::button(start, PointerButton::Primary, true)],
    );
    frame(&mut replay, vec![Event::PointerMoved(end)]);
    frame(
        &mut replay,
        vec![Replay::button(end, PointerButton::Primary, false)],
    );
    settle(&mut replay);
    assert!(replay.bench.terminal.as_ref().unwrap().sessions[0].columns() < initial_columns);
    // The terminal lives inside a scrollable, deliberately overflowing fixture.
    replay.bench.dimensions.y = 900.0;
    settle(&mut replay);
    click(&mut replay, 0, "terminal-append");
    click(&mut replay, 0, "terminal-append");
    let before = replay.target(0, "terminal-content").unwrap().rect;
    let point = before.center();
    frame(
        &mut replay,
        vec![
            Event::PointerMoved(point),
            wheel(egui::vec2(0.0, 90.0), Modifiers::NONE),
        ],
    );
    settle(&mut replay);
    assert!(replay.bench.terminal.as_ref().unwrap().sessions[0].display_offset() > 0);
    frame(
        &mut replay,
        vec![wheel(egui::vec2(0.0, -45.0), Modifiers::NONE)],
    );
    settle(&mut replay);
    assert_eq!(
        replay.target(0, "terminal-content").unwrap().rect,
        before,
        "Terminal scrolling must not move its enclosing workbench scroll area"
    );
    let view = replay.target(0, "terminal-view").unwrap().rect;
    assert!(view.contains_rect(before));
    assert!(view.contains_rect(replay.target(0, "terminal-notice").unwrap().rect));
    assert!(replay.ctx.content_rect().contains_rect(view));
}

#[test]
fn other_workbench_cases_do_not_allocate_terminal_sessions() {
    assert!(Workbench::default().terminal.is_none());
    assert!(fixture(Case::TimelineSmall).bench.terminal.is_none());
}

#[test]
fn interactive_terminal_records_exact_bytes_once_and_keeps_tab_escape_inside_terminal() {
    let mut replay = fixture(Case::TerminalInput);
    click(&mut replay, 0, "terminal-tui");
    click(&mut replay, 0, "terminal-content");
    let initial = replay.bench.terminal.as_ref().unwrap().sessions[0].visible_text();
    for key in [Key::ArrowUp, Key::Tab, Key::Escape] {
        frame(&mut replay, vec![Replay::literal(key, true)]);
        frame(&mut replay, vec![Replay::literal(key, false)]);
    }
    frame(
        &mut replay,
        vec![
            Event::Text("hello ✓".into()),
            Event::Paste("one\ntwo".into()),
        ],
    );
    let fixture = replay.bench.terminal.as_ref().unwrap();
    assert_eq!(
        fixture.input_bytes[0],
        "\x1bOA\t\x1bhello ✓\x1b[200~one\ntwo\x1b[201~".as_bytes()
    );
    assert!(fixture.input_bytes[1].is_empty());
    assert!(fixture.views[0].is_focused(&replay.ctx));
    assert_eq!(
        fixture.sessions[0].visible_text(),
        initial,
        "Outbound input is not fake local echo"
    );
    assert!(!shortcuts::viewport_keys_available(&replay.ctx));
    assert_eq!(
        fixture
            .events
            .iter()
            .filter(|event| matches!(event, TerminalEvent::InputSent { .. }))
            .count(),
        4
    );
    assert!(replay.bench.events.is_empty());
    click(&mut replay, 0, "reset");
    let fixture = replay.bench.terminal.as_ref().unwrap();
    assert!(fixture.input_bytes.iter().all(Vec::is_empty));
    assert!(fixture.events.is_empty());
    assert!(
        fixture.sessions[0]
            .visible_text()
            .contains("N3 interactive input fixture")
    );
}

#[test]
fn interactive_terminal_blocks_bytes_while_competing_text_or_popup_owns_input() {
    let mut replay = fixture(Case::TerminalInput);
    click(&mut replay, 0, "terminal-content");
    frame(&mut replay, vec![Event::Text("first".into())]);
    click(&mut replay, 0, "owner-field");
    frame(
        &mut replay,
        vec![
            Event::Text("field".into()),
            Replay::literal(Key::Enter, true),
        ],
    );
    frame(&mut replay, vec![Replay::literal(Key::Enter, false)]);
    assert_eq!(replay.bench.text, "field");
    click(&mut replay, 0, "owner-popup");
    frame(
        &mut replay,
        vec![
            Event::Text("popup".into()),
            Replay::literal(Key::Space, true),
        ],
    );
    frame(&mut replay, vec![Replay::literal(Key::Space, false)]);
    assert_eq!(
        replay.bench.terminal.as_ref().unwrap().input_bytes[0],
        b"first"
    );
    assert!(replay.bench.events.is_empty());
}
