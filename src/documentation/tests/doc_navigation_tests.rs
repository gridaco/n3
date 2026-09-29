use super::*;
use crate::{navigation_events::Event as NavigationEvent, scroll_input::ScrollPhase};
use egui::{Modifiers, Vec2};

#[test]
fn replay_modifiers_keep_number_origin_and_pointer_order() {
    use crate::keyboard_input::{NumberKey, NumberKeyEvent};
    use egui::{CursorIcon, Event, Key};

    let numeric = |pressed, modifiers| Event::Key {
        key: Key::Num3,
        physical_key: Some(Key::Num3),
        pressed,
        repeat: false,
        modifiers,
    };
    let events = vec![
        Event::PointerMoved(egui::pos2(10.0, 20.0)),
        numeric(true, Modifiers::SHIFT),
        Event::ModifiersChanged(Modifiers::NONE),
        Event::PointerMoved(egui::pos2(20.0, 30.0)),
        numeric(true, Modifiers::ALT),
    ];
    let numbers = vec![
        NumberKeyEvent {
            event_index: 1,
            key: NumberKey::Numpad(3),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::SHIFT,
        },
        NumberKeyEvent {
            event_index: 4,
            key: NumberKey::TopRow(3),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT,
        },
    ];
    let (events, numbers, modifiers) =
        normalize_replay_events(events, numbers, Modifiers::NONE, None).unwrap();
    assert_eq!(
        numbers.iter().map(|n| n.event_index).collect::<Vec<_>>(),
        [2, 6]
    );
    assert!(matches!(
        events[1],
        Event::ModifiersChanged(Modifiers::SHIFT)
    ));
    assert!(matches!(
        events[3],
        Event::ModifiersChanged(Modifiers::NONE)
    ));
    assert!(matches!(events[4], Event::PointerMoved(_)));
    assert!(matches!(events[5], Event::ModifiersChanged(Modifiers::ALT)));
    let mut input = VirtualInput::default();
    input.observe(&events, &numbers, modifiers, true);
    let summary = input.summary(CursorIcon::Default);
    assert!(summary.contains("Numpad 3"), "{summary}");
    assert_eq!(input.modifiers(), Modifiers::ALT);
    let ctx = egui::Context::default();
    ctx.run_ui(
        egui::RawInput {
            events,
            ..Default::default()
        },
        |ui| {
            assert_eq!(ui.input(|input| input.modifiers), Modifiers::ALT);
        },
    )
    .textures_delta
    .clear();

    let (events, numbers, modifiers) = normalize_replay_events(
        vec![Event::WindowFocused(false), Event::WindowFocused(true)],
        Vec::new(),
        Modifiers::ALT,
        None,
    )
    .unwrap();
    input.observe(&events, &numbers, modifiers, true);
    assert_eq!(input.modifiers(), Modifiers::NONE);
    assert!(!input.key_down(Key::Num3));
}

fn pose(s: &Session<'_>) -> glam::Mat4 {
    s.state.camera.view_projection(s.state.aspect())
}

fn front(s: &mut Session<'_>) {
    s.click_path(&[Control::N3Menu, Control::ViewMenu]).unwrap();
    s.click(Control::ViewFront).unwrap();
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )
    .unwrap();
    s.settle().unwrap();
    s.hover(Control::Viewport).unwrap();
}

#[test]
fn replayed_host_gestures_apply_once_and_supply_their_own_cues() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("cube-quads.obj").unwrap();
    s.hover(Control::Viewport).unwrap();
    let mut expected = s.state.camera.clone();
    expected.orbit(18.0, -12.0);
    s.trackpad_scroll(18.0, -12.0, Modifiers::NONE, ScrollPhase::Started)
        .unwrap();
    assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
    assert!(
        s.input
            .summary(s.cursor)
            .contains("Scroll +18.0, -12.0 Point")
    );
    s.trackpad_scroll(0.0, 0.0, Modifiers::NONE, ScrollPhase::Ended)
        .unwrap();
    expected.zoom(0.25 * 1.5);
    s.pinch(0.25).unwrap();
    assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
    assert!(s.input.summary(s.cursor).contains("Pinch 1.28×"));
    expected.orbit(-15.0_f32.to_radians() / 0.006, 0.0);
    s.trackpad_rotate(15.0).unwrap();
    assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
    assert!(s.input.summary(s.cursor).contains("Rotate -0.26 rad"));
    expected.zoom(2.0 * 0.12);
    s.scroll(0.0, 2.0, false, Modifiers::NONE).unwrap();
    assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
    assert!(s.input.summary(s.cursor).contains("Scroll +0.0, +2.0 Line"));

    front(&mut s);
    let mut expected = s.state.camera.clone();
    expected.pan(18.0, -12.0, s.state.viewport.height());
    s.trackpad_scroll(18.0, -12.0, Modifiers::NONE, ScrollPhase::Started)
        .unwrap();
    assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
    s.trackpad_rotate(25.0).unwrap();
    assert_eq!(pose(&s), expected.view_projection(s.state.aspect()));
    assert!(s.state.is_planar_navigation());
}

#[test]
fn replayed_gestures_obey_pointer_focus_popups_and_pending_pie() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("cube-quads.obj").unwrap();
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.hover(Control::PreferencesWindow).unwrap();
    let before = pose(&s);
    s.trackpad_scroll(30.0, 15.0, Modifiers::NONE, ScrollPhase::Started)
        .unwrap();
    s.pinch(0.4).unwrap();
    s.trackpad_rotate(35.0).unwrap();
    assert_eq!(
        pose(&s),
        before,
        "Floating UI rejects the same host events as native"
    );
    assert!(
        s.input.summary(s.cursor).contains("Rotate"),
        "An attempted input can be shown even when UI ownership rejects navigation"
    );
    s.click(Control::PreferencesClose).unwrap();
    s.hover(Control::Viewport).unwrap();
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    s.pinch(0.4).unwrap();
    assert_eq!(pose(&s), before, "Unfocused windows cannot navigate");
    assert!(!s.input.summary(s.cursor).contains("Pinch"));
    s.frame(vec![egui::Event::WindowFocused(true)], Duration::ZERO)
        .unwrap();
    s.hover(Control::Viewport).unwrap();
    s.frame(vec![egui::Event::PointerGone], Duration::ZERO)
        .unwrap();
    s.pinch(0.4).unwrap();
    assert_eq!(pose(&s), before, "Leaving the window blocks gestures");
    s.hover(Control::Viewport).unwrap();
    let event = NavigationEvent::Wheel {
        delta: Vec2::new(0.0, 2.0),
        modifiers: Modifiers::NONE,
    };
    let pending = [egui::Event::Key {
        key: egui::Key::Backtick,
        physical_key: Some(egui::Key::Backtick),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }];
    navigation_events::route(
        &mut s.state,
        &s.ctx,
        s.input.position(),
        true,
        &pending,
        event,
    );
    assert_eq!(
        pose(&s),
        before,
        "A queued pie press reserves input before the next UI pass"
    );
    s.click_path(&[Control::N3Menu, Control::ViewMenu]).unwrap();
    // A pointer can still be over uncovered viewport while a menu owns input.
    s.frame(
        vec![egui::Event::PointerMoved(s.state.viewport.center())],
        Duration::ZERO,
    )
    .unwrap();
    s.pinch(0.4).unwrap();
    assert_eq!(
        pose(&s),
        before,
        "An open menu blocks navigation outside its own rectangle too"
    );
}

#[test]
fn modifier_only_replay_keeps_shift_scroll_as_pan_in_planar_and_free_views() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("cube-quads.obj").unwrap();
    front(&mut s);
    for planar in [true, false] {
        if !planar {
            s.state.orbit(20.0, -15.0);
            s.settle().unwrap();
        }
        s.modifiers_changed(Modifiers::SHIFT).unwrap();
        for phase in [
            ScrollPhase::Started,
            ScrollPhase::Ended,
            ScrollPhase::Started,
        ] {
            let mut expected = s.state.camera.clone();
            expected.pan(18.0, -12.0, s.state.viewport.height());
            s.trackpad_scroll(18.0, -12.0, Modifiers::SHIFT, phase)
                .unwrap();
            assert_eq!(s.state.is_planar_navigation(), planar);
            assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
        }
        s.modifiers_changed(Modifiers::NONE).unwrap();
        s.modifiers_changed(Modifiers::SHIFT).unwrap();
        let mut expected = s.state.camera.clone();
        expected.pan(12.0, 8.0, s.state.viewport.height());
        s.trackpad_scroll(12.0, 8.0, Modifiers::SHIFT, ScrollPhase::Moved)
            .unwrap();
        assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
        assert_eq!(s.input.modifiers(), Modifiers::SHIFT);
        assert_eq!(s.state.is_planar_navigation(), planar);
        s.modifiers_changed(Modifiers::NONE).unwrap();
    }
}

#[test]
fn mouse_camera_and_transform_drags_exclude_host_gestures() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("cube-quads.obj").unwrap();
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(55.0, -75.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    s.frame(
        vec![egui::Event::PointerMoved(start), button(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    let end = start + egui::vec2(24.0, -12.0);
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    assert!(s.state.mouse_navigation_active());
    let before = pose(&s);
    s.pinch(0.5).unwrap();
    s.trackpad_rotate(25.0).unwrap();
    s.trackpad_scroll(18.0, -12.0, Modifiers::NONE, ScrollPhase::Started)
        .unwrap();
    assert_eq!(
        pose(&s),
        before,
        "A held mouse orbit owns navigation until release"
    );
    s.frame(vec![button(end, false)], Duration::ZERO).unwrap();
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])
        .unwrap();
    s.click_at(s.state.viewport.center()).unwrap();
    s.click(Control::ToolMove).unwrap();
    let start = s.target(Control::TransformX).unwrap();
    s.frame(
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![egui::Event::PointerMoved(start + egui::vec2(40.0, 0.0))],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.editor.is_interacting());
    let before = pose(&s);
    let document = s.state.editor.document.clone();
    s.pinch(0.5).unwrap();
    s.trackpad_rotate(25.0).unwrap();
    s.scroll(0.0, 2.0, false, Modifiers::NONE).unwrap();
    assert_eq!(
        pose(&s),
        before,
        "A transform drag excludes camera gestures"
    );
    assert_eq!(s.state.editor.document, document);
    s.frame(
        vec![pointer(start + egui::vec2(40.0, 0.0), false)],
        Duration::ZERO,
    )
    .unwrap();
}
