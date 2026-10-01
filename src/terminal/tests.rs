use super::view::TerminalViewResponse;
use super::*;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use egui::{Event, Key, Modifiers, Pos2, Rect, Vec2};

#[test]
fn placeholder_seeds_after_initial_size_and_never_reseeds_existing_output() {
    let mut session = TerminalSession::placeholder();
    session.resize(43, 25);
    assert!(session.visible_text().starts_with("N3 Terminal"));
    assert_eq!(session.history_size(), 0);
    session.ingest(b"\r\nappended after layout");
    let output = session.visible_text();
    session.resize(43, 25);
    assert_eq!(session.visible_text(), output);
    assert_eq!(output.matches("N3 Terminal").count(), 1);

    // A host may feed output before the first view: preserve sample/data order.
    let mut early = TerminalSession::placeholder();
    early.ingest(b"\r\nearly output");
    assert!(early.visible_text().starts_with("N3 Terminal"));
    assert!(early.visible_text().contains("early output"));
}

#[test]
fn ingestion_keeps_parser_state_across_utf8_and_ansi_chunks() {
    let mut session = TerminalSession::empty(40, 4);
    session.ingest(b"Hello \x1b[3");
    session.ingest(b"1mred\x1b[0m ");
    let character = "é".as_bytes();
    session.ingest(&character[..1]);
    session.ingest(&character[1..]);
    assert!(session.visible_text().starts_with("Hello red é"));
    assert_eq!(
        session.term.lock().grid()[Point::new(Line(0), Column(6))].fg,
        Color::Named(NamedColor::Red)
    );
    assert_eq!(
        session.term.lock().grid()[Point::new(Line(0), Column(10))].fg,
        Color::Named(NamedColor::Foreground)
    );
}

#[test]
fn ansi_controls_update_the_real_grid_and_preserve_style() {
    let mut session = TerminalSession::empty(40, 4);
    session.ingest(b"discard\r\x1b[2K\x1b[38;2;12;34;56;48;5;21;4mX\x1b[0mY");
    assert!(session.visible_text().starts_with("XY\n"));
    let term = session.term.lock();
    let cell = &term.grid()[Point::new(Line(0), Column(0))];
    assert_eq!(
        cell.fg,
        Color::Spec(Rgb {
            r: 12,
            g: 34,
            b: 56
        })
    );
    assert_eq!(cell.bg, Color::Indexed(21));
    assert!(cell.flags.contains(Flags::UNDERLINE));
    let next = &term.grid()[Point::new(Line(0), Column(1))];
    assert_eq!(next.fg, Color::Named(NamedColor::Foreground));
    assert!(!next.flags.contains(Flags::UNDERLINE));
}

#[test]
fn resize_reflows_without_discarding_output_and_clamps_empty_sizes() {
    let mut session = TerminalSession::empty(20, 4);
    session.ingest(b"abcdefghijklmnop");
    session.resize(8, 4);
    assert_eq!(session.columns(), 8);
    // Alacritty keeps the bottom screen stable and puts the reflowed first
    // line into history; it must remain available, not be truncated.
    session.scroll_to(session.history_size());
    assert!(session.visible_text().starts_with("abcdefgh\nijklmnop"));
    session.resize(20, 4);
    assert!(session.visible_text().starts_with("abcdefghijklmnop"));
    let text = session.visible_text();
    session.resize(20, 4);
    assert_eq!(session.visible_text(), text);
    session.resize(0, 0);
    assert_eq!((session.columns(), session.rows()), (2, 1));
}

#[test]
fn scrollback_has_a_bounded_history_and_wide_cells_are_not_duplicated() {
    let mut session = TerminalSession::empty(20, 3);
    session.ingest("A界e\u{301}Z\r\n".as_bytes());
    assert!(session.visible_text().starts_with("A界e\u{301}Z"));
    for index in 0..1_100 {
        session.ingest(format!("row {index}\r\n").as_bytes());
    }
    assert_eq!(session.history_size(), 1_000);
    session.scroll_lines(i32::MAX);
    assert_eq!(session.display_offset(), 1_000);
    assert!(!session.visible_text().contains("row 1099"));
    session.scroll_to(0);
    assert!(session.visible_text().contains("row 1099"));
}

fn input(events: Vec<Event>) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 400.0))),
        events,
        ..Default::default()
    }
}

fn show(
    ctx: &egui::Context,
    view: &mut TerminalView,
    session: &mut TerminalSession,
    raw: egui::RawInput,
) -> TerminalViewResponse {
    let mut response = None;
    let mut output = ctx.run_ui(raw, |ui| {
        response = Some(view.show(ui, session));
    });
    output.textures_delta.clear();
    response.unwrap()
}

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn collapsed_first_layout_does_not_reflow_or_seed_the_terminal() {
    let ctx = egui::Context::default();
    let mut view = TerminalView::new(egui::Id::new("terminal"));
    let mut session = TerminalSession::placeholder();
    let mut raw = input(vec![]);
    raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(20.0)));
    show(&ctx, &mut view, &mut session, raw);
    assert_eq!(session.history_size(), 0);
    assert!(session.visible_text().trim().is_empty());
    show(&ctx, &mut view, &mut session, input(vec![]));
    assert!(session.visible_text().starts_with("N3 Terminal"));
    assert_eq!(session.history_size(), 0);
}

#[test]
fn focused_input_is_owned_without_echo_and_escape_releases_focus() {
    let ctx = egui::Context::default();
    let mut view = TerminalView::new(egui::Id::new("terminal"));
    let mut session = TerminalSession::placeholder();
    show(&ctx, &mut view, &mut session, input(vec![]));
    let initial = session.visible_text();
    view.request_focus(&ctx);
    let response = show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![
            key(Key::Space),
            key(Key::Num2),
            Event::Text("rm anything".into()),
        ]),
    );
    assert!(response.focused);
    assert!(ctx.egui_wants_keyboard_input());
    assert!(ctx.input(|i| i.events.is_empty()));
    assert_eq!(session.visible_text(), initial);
    show(&ctx, &mut view, &mut session, input(vec![key(Key::Escape)]));
    assert!(!view.is_focused(&ctx));
    assert!(!ctx.input(|i| i.key_pressed(Key::Escape)));
}

#[test]
fn focus_loss_and_other_native_input_owners_release_or_keep_their_input() {
    let ctx = egui::Context::default();
    let mut view = TerminalView::new(egui::Id::new("terminal"));
    let mut session = TerminalSession::placeholder();
    show(&ctx, &mut view, &mut session, input(vec![]));
    view.request_focus(&ctx);
    let mut raw = input(vec![]);
    raw.focused = false;
    show(&ctx, &mut view, &mut session, raw);
    assert!(!view.is_focused(&ctx));
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![Event::Text("other field".into())]),
    );
    assert!(ctx.input(|i| i.events.iter().any(|event| matches!(event, Event::Text(_)))));
}

#[test]
fn visible_bounds_are_clipped_and_instances_keep_independent_state() {
    let ctx = egui::Context::default();
    let mut first = TerminalView::new(egui::Id::new("first"));
    let mut second = TerminalView::new(egui::Id::new("second"));
    let mut a = TerminalSession::placeholder();
    let mut b = TerminalSession::placeholder();
    let clip = Rect::from_min_max(Pos2::new(20.0, 20.0), Pos2::new(320.0, 260.0));
    let mut output = ctx.run_ui(input(vec![]), |ui| {
        ui.set_clip_rect(clip);
        let response = first.show(ui, &mut a);
        assert!(clip.contains_rect(response.rect));
        assert!(clip.contains_rect(response.content_rect));
    });
    output.textures_delta.clear();
    first.request_focus(&ctx);
    assert!(first.is_focused(&ctx));
    assert!(!second.is_focused(&ctx));
    second.request_focus(&ctx);
    assert!(!first.is_focused(&ctx));
    assert!(second.is_focused(&ctx));
    a.ingest(b"\r\nonly first");
    assert!(!b.visible_text().contains("only first"));
    show(&ctx, &mut second, &mut b, input(vec![]));
}

#[test]
fn wheel_is_applied_once_across_layout_passes_and_does_not_scroll_parent() {
    let ctx = egui::Context::default();
    let mut view = TerminalView::new(egui::Id::new("terminal"));
    let mut session = TerminalSession::placeholder();
    let first = show(&ctx, &mut view, &mut session, input(vec![]));
    for index in 0..60 {
        session.ingest(format!("\r\nline {index}").as_bytes());
    }
    let pointer = first.content_rect.center();
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![Event::PointerMoved(pointer)]),
    );
    let raw = input(vec![
        Event::PointerMoved(pointer),
        Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            phase: egui::TouchPhase::Move,
            delta: Vec2::new(0.0, 60.0),
            modifiers: Modifiers::NONE,
        },
    ]);
    let mut offsets = Vec::new();
    let mut output = ctx.run_ui(raw, |ui| {
        let response = view.show(ui, &mut session);
        offsets.push(response.display_offset);
        assert_eq!(ui.input(|i| i.smooth_scroll_delta), Vec2::ZERO);
        if ui.ctx().current_pass_index() == 0 {
            ui.ctx()
                .request_discard("terminal repeated-pass regression");
        }
    });
    output.textures_delta.clear();
    assert!(
        offsets.len() >= 2,
        "regression must exercise repeated layout"
    );
    assert!(offsets[0] > 0);
    assert!(offsets.iter().all(|offset| *offset == offsets[0]));
}

#[test]
fn interactive_focus_reports_once_and_deactivation_releases_protocol_focus() {
    let ctx = egui::Context::default();
    let mut view = TerminalView::new(egui::Id::new("focus-protocol"));
    let mut session = TerminalSession::interactive_fixture();
    session.ingest(b"\x1b[?1004h");
    show(&ctx, &mut view, &mut session, input(vec![]));
    view.request_focus(&ctx);
    let mut output = ctx.run_ui(input(vec![]), |ui| {
        view.show(ui, &mut session);
        if ui.ctx().current_pass_index() == 0 {
            ui.ctx().request_discard("focus report repeated pass");
        }
    });
    output.textures_delta.clear();
    assert_eq!(session.take_fixture_input(), b"\x1b[I");
    view.deactivate(&ctx, &mut session);
    view.deactivate(&ctx, &mut session);
    assert_eq!(session.take_fixture_input(), b"\x1b[O");
    assert!(!view.is_focused(&ctx));
}

#[test]
fn interactive_view_exposes_ime_and_copies_selected_terminal_text() {
    use alacritty_terminal::{
        index::Side,
        selection::{Selection, SelectionType},
    };
    let ctx = egui::Context::default();
    ctx.set_os(egui::os::OperatingSystem::Mac);
    let mut view = TerminalView::new(egui::Id::new("ime-clipboard"));
    let mut session = TerminalSession::interactive_fixture();
    session.ingest("hello 界".as_bytes());
    show(&ctx, &mut view, &mut session, input(vec![]));
    view.request_focus(&ctx);
    let mut output = ctx.run_ui(input(vec![]), |ui| {
        view.show(ui, &mut session);
    });
    assert!(
        output.platform_output.ime.is_some(),
        "Native IME must be enabled for terminal text input"
    );
    output.textures_delta.clear();
    let mut selection = Selection::new(
        SelectionType::Simple,
        Point::new(Line(0), Column(0)),
        Side::Left,
    );
    selection.update(Point::new(Line(0), Column(4)), Side::Right);
    session.term.lock().selection = Some(selection);
    let mut output = ctx.run_ui(input(vec![Event::Copy]), |ui| {
        view.show(ui, &mut session);
    });
    assert!(
        output.platform_output.commands.iter().any(
            |command| matches!(command, egui::OutputCommand::CopyText(text) if text == "hello")
        )
    );
    output.textures_delta.clear();
    assert!(
        session.take_fixture_input().is_empty(),
        "Copy is a local action on macOS"
    );
    assert_eq!(session.visible_text().lines().next(), Some("hello 界"));
}

#[test]
fn application_mouse_release_outside_view_retains_cell_and_shift_selects_locally() {
    let ctx = egui::Context::default();
    let mut view = TerminalView::new(egui::Id::new("mouse-protocol"));
    let mut session = TerminalSession::interactive_fixture();
    session.ingest(b"\x1b[?1000h\x1b[?1006hhello");
    let response = show(&ctx, &mut view, &mut session, input(vec![]));
    let pointer = response.content_rect.left_top() + Vec2::splat(crate::theme::space::LG + 1.0);
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![Event::PointerMoved(pointer)]),
    );
    let button = |pos, pressed, modifiers| Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers,
    };
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![button(pointer, true, Modifiers::NONE)]),
    );
    assert_eq!(session.take_fixture_input(), b"\x1b[<0;1;1M");
    let outside = response.content_rect.right_bottom() + Vec2::splat(50.0);
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![
            Event::PointerMoved(outside),
            button(outside, false, Modifiers::SHIFT),
        ]),
    );
    assert_eq!(
        session.take_fixture_input(),
        format!("\x1b[<4;{};{}m", session.columns(), session.rows()).as_bytes()
    );
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![Event::PointerMoved(pointer)]),
    );
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![button(pointer, true, Modifiers::SHIFT)]),
    );
    assert!(session.take_fixture_input().is_empty());
    assert!(
        session.term.lock().selection.is_some(),
        "Shift bypasses application mouse reporting for local selection"
    );
    let end = pointer + Vec2::new(25.0, 0.0);
    show(
        &ctx,
        &mut view,
        &mut session,
        input(vec![
            Event::PointerMoved(end),
            button(end, false, Modifiers::NONE),
        ]),
    );
    assert!(
        session.take_fixture_input().is_empty(),
        "Changing Shift cannot transfer a local selection to the application"
    );
    assert!(
        session
            .term
            .lock()
            .selection_to_string()
            .is_some_and(|text| !text.is_empty())
    );
}
