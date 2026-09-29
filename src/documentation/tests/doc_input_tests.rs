use super::*;

impl VirtualInput {
    fn observe_without_numbers(&mut self, events: &[Event], modifiers: Modifiers, focused: bool) {
        self.observe(events, &[], modifiers, focused);
    }
}

fn pointer(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn key(key: Key, pressed: bool, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}

fn number(event_index: usize, key: NumberKey, pressed: bool) -> NumberKeyEvent {
    NumberKeyEvent {
        event_index,
        key,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn tutorial_drag_cues_use_the_editor_threshold_and_remember_crossing_it() {
    let mut input = VirtualInput::default();
    let start = egui::pos2(20.0, 30.0);
    input.observe(&[pointer(start, true)], &[], Modifiers::NONE, true);
    input.observe(
        &[Event::PointerMoved(
            start + egui::vec2(crate::pointer_policy::DRAG_THRESHOLD, 0.0),
        )],
        &[],
        Modifiers::NONE,
        true,
    );
    assert_eq!(input.mouse_labels(), ["Left down"]);
    input.observe(
        &[Event::PointerMoved(
            start + egui::vec2(crate::pointer_policy::DRAG_THRESHOLD + 0.5, 0.0),
        )],
        &[],
        Modifiers::NONE,
        true,
    );
    assert_eq!(input.mouse_labels(), ["Left drag"]);
    input.observe(
        &[Event::PointerMoved(start), pointer(start, false)],
        &[],
        Modifiers::NONE,
        true,
    );
    assert!(
        input
            .summary(CursorIcon::Default)
            .contains("Left drag released")
    );
}

#[test]
fn simultaneous_top_row_and_numpad_digits_keep_independent_cues_and_releases() {
    let mut input = VirtualInput::default();
    let top = NumberKey::TopRow(3);
    let pad = NumberKey::Numpad(3);
    let events = [
        key(Key::Num3, true, Modifiers::NONE),
        key(Key::Num3, true, Modifiers::NONE),
    ];
    input.observe(
        &events,
        &[number(0, top, true), number(1, pad, true)],
        Modifiers::NONE,
        true,
    );
    assert_eq!(
        chord(input.modifiers, input.keys.keys().copied()),
        ["3", "Numpad 3"]
    );
    assert_eq!(input.keys.len(), 2);
    input.observe(
        &[key(Key::Num3, false, Modifiers::NONE)],
        &[number(0, top, false)],
        Modifiers::NONE,
        true,
    );
    assert!(input.key_down(Key::Num3));
    assert_eq!(
        chord(input.modifiers, input.keys.keys().copied()),
        ["Numpad 3"]
    );
    input.observe(
        &[key(Key::Num3, true, Modifiers::NONE)],
        &[NumberKeyEvent {
            repeat: true,
            ..number(0, pad, true)
        }],
        Modifiers::NONE,
        true,
    );
    assert_eq!(input.keys.len(), 1);
    input.observe(
        &[key(Key::Num3, false, Modifiers::NONE)],
        &[number(0, pad, false)],
        Modifiers::NONE,
        true,
    );
    assert!(!input.key_down(Key::Num3));
    assert!(
        input
            .summary(CursorIcon::Default)
            .contains("released_keys=Numpad 3/600ms")
    );
    input.advance(CUE_LIFETIME);
    assert!(input.recent_keys.is_none());
}

#[test]
fn number_metadata_is_indexed_validated_and_cleared_on_focus_loss() {
    let mut input = VirtualInput::default();
    let events = [
        Event::PointerMoved(egui::pos2(1.0, 2.0)),
        key(Key::Num1, true, Modifiers::NONE),
        key(Key::Num2, true, Modifiers::NONE),
    ];
    input.observe(
        &events,
        &[
            number(0, NumberKey::Numpad(1), true),
            number(2, NumberKey::Numpad(2), true),
        ],
        Modifiers::NONE,
        true,
    );
    assert_eq!(
        chord(input.modifiers, input.keys.keys().copied()),
        ["1", "Numpad 2"]
    );
    input.observe(&[Event::WindowFocused(false)], &[], Modifiers::NONE, false);
    assert!(input.keys.is_empty());
    assert!(input.recent_keys.is_none());
    assert!(!input.key_down(Key::Num1));
    input.observe(
        &[
            Event::WindowFocused(true),
            key(Key::Num1, true, Modifiers::NONE),
        ],
        &[number(1, NumberKey::Numpad(1), true)],
        Modifiers::NONE,
        true,
    );
    assert_eq!(
        chord(input.modifiers, input.keys.keys().copied()),
        ["Numpad 1"]
    );
}

#[test]
fn logical_digit_fallback_matches_top_row_without_releasing_the_numpad() {
    let mut input = VirtualInput::default();
    input.observe(
        &[key(Key::Num1, true, Modifiers::NONE)],
        &[],
        Modifiers::NONE,
        true,
    );
    input.observe(
        &[key(Key::Num1, true, Modifiers::NONE)],
        &[number(0, NumberKey::Numpad(1), true)],
        Modifiers::NONE,
        true,
    );
    input.observe(
        &[key(Key::Num1, false, Modifiers::NONE)],
        &[number(0, NumberKey::TopRow(1), false)],
        Modifiers::NONE,
        true,
    );
    assert_eq!(
        chord(input.modifiers, input.keys.keys().copied()),
        ["Numpad 1"]
    );
}

#[test]
fn hand_cursors_have_distinct_valid_geometry_anchored_to_the_event_hotspot() {
    let cursor_mesh = |icon, position| {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |root_ui| {
            let context = root_ui.ctx().clone();
            let ctx = &context;
            paint_cursor(&ctx.debug_painter(), position, icon, false);
        });
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .find_map(|shape| match shape.shape {
                egui::Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .unwrap()
    };
    let hotspot = egui::pos2(100.0, 150.0);
    let open = cursor_mesh(CursorIcon::Grab, hotspot);
    let closed = cursor_mesh(CursorIcon::Grabbing, hotspot);
    let arrow = cursor_mesh(CursorIcon::Default, hotspot);
    assert!(open.is_valid() && closed.is_valid());
    assert_ne!(open.vertices.len(), closed.vertices.len());
    assert_ne!(open.vertices.len(), arrow.vertices.len());
    assert_ne!(closed.vertices.len(), arrow.vertices.len());
    assert!(open.calc_bounds().height() > closed.calc_bounds().height());
    assert!(open.calc_bounds().contains(hotspot));
    assert!(closed.calc_bounds().contains(hotspot));
    let offset = egui::vec2(31.0, -27.0);
    let moved = cursor_mesh(CursorIcon::Grab, hotspot + offset);
    assert_eq!(open.indices, moved.indices);
    for (before, after) in open.vertices.iter().zip(&moved.vertices) {
        assert_eq!(before.pos + offset, after.pos);
    }
}

#[test]
fn held_mouse_and_drag_persist_until_release_then_expire() {
    let mut input = VirtualInput::default();
    input.observe_without_numbers(
        &[pointer(egui::pos2(10.0, 20.0), true)],
        Modifiers::NONE,
        true,
    );
    input.advance(Duration::from_secs(30));
    assert!(input.is_pressed(PointerButton::Primary));
    for x in 0..200 {
        input.observe_without_numbers(
            &[Event::PointerMoved(egui::pos2(10.0 + x as f32 * 3.0, 20.0))],
            Modifiers::NONE,
            true,
        );
    }
    assert!(input.summary(CursorIcon::Grabbing).contains("Left drag"));
    assert_eq!(input.trail.len(), MAX_TRAIL_POINTS);
    let end = input.position().unwrap();
    input.observe_without_numbers(&[pointer(end, false)], Modifiers::NONE, true);
    assert!(!input.is_pressed(PointerButton::Primary));
    assert!(
        input
            .summary(CursorIcon::Grab)
            .contains("Left drag released")
    );
    input.advance(CUE_LIFETIME - Duration::from_millis(1));
    assert!(input.recent_pointer.is_some());
    input.advance(Duration::from_millis(1));
    assert!(input.recent_pointer.is_none());
    assert!(input.trail.is_empty());
    assert_eq!(input.position(), Some(end));
}

#[test]
fn combined_chord_has_one_command_and_retains_release_cue() {
    let mut input = VirtualInput::default();
    let modifiers = Modifiers {
        ctrl: true,
        alt: true,
        shift: true,
        mac_cmd: true,
        command: true,
    };
    input.observe_without_numbers(&[key(Key::F, true, modifiers)], modifiers, true);
    input.advance(Duration::from_secs(10));
    assert!(input.key_down(Key::F));
    assert_eq!(input.modifiers(), modifiers);
    let labels = chord(input.modifiers(), input.keys.keys().copied());
    assert_eq!(labels, ["Control", "Option", "Shift", "Command", "F"]);
    input.observe_without_numbers(
        &[key(Key::F, false, Modifiers::NONE)],
        Modifiers::NONE,
        true,
    );
    assert!(!input.key_down(Key::F));
    assert!(
        input
            .summary(CursorIcon::Default)
            .contains("Control+Option+Shift+Command+F/600ms")
    );
    input.advance(CUE_LIFETIME);
    assert!(input.recent_keys.is_none());
    assert_eq!(
        chord(Modifiers::CTRL | Modifiers::COMMAND, std::iter::empty()),
        ["Control"]
    );
}

#[test]
fn pointer_gone_hides_visuals_without_inventing_a_release() {
    let mut input = VirtualInput::default();
    input.observe_without_numbers(
        &[pointer(egui::pos2(2.0, 3.0), true), Event::PointerGone],
        Modifiers::NONE,
        true,
    );
    assert_eq!(input.position(), None);
    assert!(input.is_pressed(PointerButton::Primary));
    assert!(input.trail.is_empty());
    assert!(input.recent_pointer.is_none());
    input.observe_without_numbers(
        &[pointer(egui::pos2(4.0, 5.0), false)],
        Modifiers::NONE,
        true,
    );
    assert!(!input.is_pressed(PointerButton::Primary));
    assert_eq!(input.position(), Some(egui::pos2(4.0, 5.0)));
}

#[test]
fn focus_loss_clears_held_and_recent_input() {
    let mut input = VirtualInput::default();
    input.observe_without_numbers(
        &[
            key(Key::A, true, Modifiers::SHIFT),
            pointer(egui::pos2(10.0, 10.0), true),
            Event::Zoom(1.2),
            Event::WindowFocused(false),
            key(Key::B, true, Modifiers::SHIFT),
        ],
        Modifiers::SHIFT,
        true,
    );
    assert!(!input.key_down(Key::A));
    assert!(!input.key_down(Key::B));
    assert!(!input.is_pressed(PointerButton::Primary));
    assert_eq!(input.position(), None);
    assert_eq!(input.modifiers(), Modifiers::NONE);
    assert!(input.recent_gesture.is_none());
    input.observe_without_numbers(
        &[key(Key::C, true, Modifiers::NONE)],
        Modifiers::NONE,
        false,
    );
    assert!(!input.key_down(Key::C));
    input.observe_without_numbers(&[key(Key::C, true, Modifiers::NONE)], Modifiers::NONE, true);
    assert!(input.key_down(Key::C));
}

#[test]
fn cue_clock_is_partition_independent_and_zero_time_is_stable() {
    let make_input = || {
        let mut input = VirtualInput::default();
        input.observe_without_numbers(
            &[
                key(Key::Num1, true, Modifiers::NONE),
                key(Key::Num1, false, Modifiers::NONE),
                Event::Zoom(1.25),
            ],
            Modifiers::NONE,
            true,
        );
        input
    };
    let mut single = make_input();
    let mut split = make_input();
    let before = single.summary(CursorIcon::Default);
    single.advance(Duration::ZERO);
    assert_eq!(before, single.summary(CursorIcon::Default));
    single.advance(Duration::from_millis(400));
    for _ in 0..4 {
        split.advance(Duration::from_millis(100));
    }
    assert_eq!(
        single.summary(CursorIcon::Default),
        split.summary(CursorIcon::Default)
    );
    single.advance(Duration::MAX);
    assert!(single.recent_keys.is_none());
    assert!(single.recent_gesture.is_none());
}

#[test]
fn overlay_paint_does_not_change_hover_or_capture_input() {
    let ctx = egui::Context::default();
    let viewport = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(400.0, 300.0));
    let mut input = VirtualInput::default();
    let position = egui::pos2(40.0, 40.0);
    let mut target = egui::Rect::NOTHING;
    let mut cursor = CursorIcon::Default;
    for _ in 0..2 {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events: vec![Event::PointerMoved(position)],
                ..Default::default()
            },
            |root_ui| {
                let context = root_ui.ctx().clone();
                let ctx = &context;
                egui::CentralPanel::default().show(root_ui, |ui| {
                    let response = ui.put(
                        egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(100.0, 40.0)),
                        egui::Button::new("Real button"),
                    );
                    target = response.rect;
                    if response.hovered() {
                        ctx.set_cursor_icon(CursorIcon::PointingHand);
                    }
                });
                input.observe_without_numbers(
                    &[Event::PointerMoved(position)],
                    Modifiers::NONE,
                    true,
                );
                for icon in [
                    CursorIcon::PointingHand,
                    CursorIcon::Grab,
                    CursorIcon::Grabbing,
                ] {
                    input.paint(ctx, viewport, icon);
                }
            },
        );
        output.textures_delta.clear();
        cursor = output.platform_output.cursor_icon;
    }
    assert!(target.contains(position));
    assert_eq!(cursor, CursorIcon::PointingHand);
    assert!(!ctx.egui_is_using_pointer());
    assert!(!ctx.egui_wants_keyboard_input());
}
