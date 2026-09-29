use super::{Capture, Control, HEIGHT, Session, WIDTH};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::time::Duration;

fn key(key: Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn open(s: &mut Session<'_>, anchor: Pos2) {
    s.frame(
        vec![Event::PointerMoved(anchor), key(Key::Backtick, true)],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.view_pie_active());
    assert!(s.state.pie_owns_input());
}

#[test]
fn view_pie_respects_real_numeric_field_and_popup_ownership() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let document = s.state.editor.document.clone();
    let pose = s.state.camera.view_projection(s.state.aspect());
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    let field = s.ctx.memory(|memory| memory.focused());
    assert!(field.is_some() && field != Some(crate::shortcuts::viewport_focus_id()));
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(35., -35.);
    s.frame(
        vec![Event::PointerMoved(empty), key(Key::Backtick, true)],
        Duration::ZERO,
    )
    .unwrap();
    assert!(!s.state.view_pie_active() && !s.state.pie_owns_input());
    assert_eq!(s.ctx.memory(|memory| memory.focused()), field);
    s.frame(vec![key(Key::Backtick, false)], Duration::ZERO)
        .unwrap();
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.click_at(empty).unwrap();
    s.right_click(Control::Viewport).unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    s.frame(
        vec![Event::PointerMoved(empty), key(Key::Backtick, true)],
        Duration::ZERO,
    )
    .unwrap();
    assert!(!s.state.view_pie_active() && !s.state.pie_owns_input());
    assert!(egui::Popup::is_any_open(&s.ctx));
    s.frame(vec![key(Key::Backtick, false)], Duration::ZERO)
        .unwrap();
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
}

#[test]
fn view_pie_cancel_events_preserve_selection_and_prevent_release_action() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let object = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(object).unwrap();
    let document = s.state.editor.document.clone();
    let pose = s.state.camera.view_projection(s.state.aspect());
    for cancel in [
        key(Key::Escape, true),
        Event::PointerGone,
        Event::WindowFocused(false),
    ] {
        let anchor = s.state.viewport.center();
        open(&mut s, anchor);
        let choice = s.trace.get(Control::PieRight).unwrap().rect.center();
        s.frame(vec![Event::PointerMoved(choice)], Duration::ZERO)
            .unwrap();
        s.frame(vec![cancel], Duration::ZERO).unwrap();
        assert!(!s.state.view_pie_active());
        assert!(
            s.state.pie_owns_input(),
            "The cancellation frame remains captured"
        );
        if s.focused {
            assert_eq!(
                s.ctx.memory(|memory| memory.focused()),
                Some(crate::shortcuts::viewport_focus_id()),
                "Disabling the captured viewport must not surrender next-frame focus"
            );
        }
        assert_eq!(s.state.editor.selected_object, Some(object));
        s.frame(
            vec![
                Event::WindowFocused(true),
                key(Key::Escape, false),
                key(Key::Backtick, false),
            ],
            Duration::ZERO,
        )
        .unwrap();
        assert!(!s.state.view_pie_active() && !s.state.pie_owns_input());
        assert_eq!(s.state.editor.selected_object, Some(object));
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
        assert!(!s.state.camera.is_transitioning());
    }
}

#[test]
fn view_pie_cannot_take_over_pointer_selection_navigation_or_transforms() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let object = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(object).unwrap();
    let document = s.state.editor.document.clone();
    for held in [
        PointerButton::Primary,
        PointerButton::Secondary,
        PointerButton::Middle,
    ] {
        let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(40., -40.);
        let end = start + egui::vec2(25., -15.);
        let pose = s.state.camera.view_projection(s.state.aspect());
        // Even a trigger delivered in the pointer-down batch cannot steal it.
        s.frame(
            vec![
                Event::PointerMoved(start),
                button(start, held, true),
                key(Key::Backtick, true),
            ],
            Duration::ZERO,
        )
        .unwrap();
        assert!(!s.state.view_pie_active() && !s.state.pie_owns_input());
        s.frame(vec![Event::PointerMoved(end)], Duration::ZERO)
            .unwrap();
        if held == PointerButton::Primary {
            assert!(s.state.editor.is_interacting());
            assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
        } else {
            assert!(s.state.mouse_navigation_active());
            assert_ne!(s.state.camera.view_projection(s.state.aspect()), pose);
        }
        s.frame(vec![key(Key::Backtick, false)], Duration::ZERO)
            .unwrap();
        // Retry while the established gesture is still held.
        s.frame(vec![key(Key::Backtick, true)], Duration::ZERO)
            .unwrap();
        assert!(!s.state.view_pie_active() && !s.state.pie_owns_input());
        s.frame(
            vec![key(Key::Backtick, false), key(Key::Escape, true)],
            Duration::ZERO,
        )
        .unwrap();
        s.frame(
            vec![key(Key::Escape, false), button(end, held, false)],
            Duration::ZERO,
        )
        .unwrap();
        assert!(!s.state.editor.is_interacting() && !s.state.mouse_navigation_active());
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.editor.selected_object, Some(object));
    }

    let aspect = s.state.aspect();
    s.state.camera.frame(aspect);
    s.state.editor.tool = crate::editor::Tool::Move;
    s.settle().unwrap();
    let start = s.trace.get(Control::TransformX).unwrap().rect.center();
    // Cross one centimeter so the ownership check holds a changed preview.
    let end = start + egui::vec2(120., 0.);
    s.frame(
        vec![
            Event::PointerMoved(start),
            button(start, PointerButton::Primary, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(vec![Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    assert!(s.state.editor.is_transforming());
    let preview = s.state.editor.document.clone();
    assert_ne!(preview, document);
    let pose = s.state.camera.view_projection(s.state.aspect());
    s.frame(vec![key(Key::Backtick, true)], Duration::ZERO)
        .unwrap();
    assert!(!s.state.view_pie_active() && !s.state.pie_owns_input());
    assert!(s.state.editor.is_transforming());
    assert_eq!(s.state.editor.document, preview);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
    s.frame(
        vec![key(Key::Backtick, false), key(Key::Escape, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![
            key(Key::Escape, false),
            button(end, PointerButton::Primary, false),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.selected_object, Some(object));
}
