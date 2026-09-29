use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::document::{Geometry, PrimitiveKind};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::time::Duration;

fn key(key: Key, pressed: bool, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    }
}

fn pointer(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn tap(s: &mut Session<'_>, key: Key, modifiers: Modifiers) {
    s.key(key, true, modifiers).unwrap();
    s.key(key, false, Modifiers::NONE).unwrap();
}

fn assert_insert_open(s: &Session<'_>) {
    assert!(egui::Popup::is_any_open(&s.ctx));
    for control in [
        Control::InsertPlane,
        Control::InsertCircle,
        Control::InsertCube,
        Control::InsertCylinder,
        Control::InsertCone,
        Control::InsertTorus,
        Control::InsertSphere,
        Control::InsertPolyhedron,
    ] {
        let item = s.trace.get(control).unwrap();
        assert!(item.enabled);
        assert_eq!(item.parents, [Control::InsertMenu]);
    }
}

fn assert_focused(s: &Session<'_>, control: Control) {
    let id = s.ctx.memory(|memory| memory.focused()).unwrap();
    assert_eq!(
        s.ctx.read_response(id).unwrap().rect,
        s.trace.get(control).unwrap().rect,
        "Keyboard focus must belong to the actual {control:?} menu item"
    );
}

#[test]
fn insert_shortcut_reuses_menu_and_keyboard_insertion_is_one_undo_step() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.editor.document.clone();
    s.state
        .editor
        .select_object(original.objects[0].id)
        .unwrap();
    s.extra_layout_pass = true;
    s.frame(
        vec![
            key(Key::I, true, Modifiers::SHIFT),
            key(Key::Enter, true, Modifiers::NONE),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(
        !s.state.editor.edit_mode,
        "The opening batch's Enter must not reach the document"
    );
    assert_eq!(s.state.editor.document, original);
    s.settle().unwrap();
    s.key(Key::Enter, false, Modifiers::NONE).unwrap();
    assert_insert_open(&s);
    assert_focused(&s, Control::InsertCube);
    let initial_focus = s.ctx.memory(|memory| memory.focused());
    s.extra_layout_pass = true;
    s.key(Key::I, true, Modifiers::SHIFT).unwrap();
    assert_insert_open(&s);
    assert_eq!(s.ctx.memory(|memory| memory.focused()), initial_focus);
    assert_eq!(s.state.editor.document, original);
    s.key(Key::I, false, Modifiers::NONE).unwrap();

    tap(&mut s, Key::ArrowDown, Modifiers::NONE);
    assert_focused(&s, Control::InsertCylinder);
    tap(&mut s, Key::Tab, Modifiers::NONE);
    assert_focused(&s, Control::InsertCone);
    s.extra_layout_pass = true;
    tap(&mut s, Key::Enter, Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(
        s.state.editor.document.objects.len(),
        original.objects.len() + 1
    );
    let added = s.state.editor.document.objects.last().unwrap();
    assert!(matches!(&added.geometry, Geometry::Primitive(p) if p.kind == PrimitiveKind::Cone));
    assert_eq!(s.state.editor.selected_object, Some(added.id));
    assert!(!s.state.editor.edit_mode);
    assert_eq!(
        s.ctx.memory(|memory| memory.focused()),
        Some(crate::shortcuts::viewport_focus_id())
    );
    let command = Modifiers {
        command: true,
        mac_cmd: true,
        ..Modifiers::NONE
    };
    tap(&mut s, Key::Z, command);
    assert_eq!(s.state.editor.document, original);
    assert!(
        !s.state.editor.undo(),
        "Insertion must create exactly one history entry"
    );
}

#[test]
fn insert_escape_restores_viewport_and_other_ui_owners_keep_shift_i() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let object = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(object).unwrap();
    let original = s.state.editor.document.clone();
    tap(&mut s, Key::I, Modifiers::SHIFT);
    assert_insert_open(&s);
    s.extra_layout_pass = true;
    tap(&mut s, Key::Escape, Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(s.state.editor.selected_object, Some(object));
    assert_eq!(
        s.ctx.memory(|memory| memory.focused()),
        Some(crate::shortcuts::viewport_focus_id())
    );
    s.state.editor.deselect();
    tap(&mut s, Key::Tab, Modifiers::NONE);
    assert_eq!(
        s.state.editor.selected_object,
        Some(object),
        "Viewport Tab resumes after dismissal"
    );

    for modifiers in [
        Modifiers::NONE,
        Modifiers {
            shift: true,
            ctrl: true,
            ..Modifiers::NONE
        },
        Modifiers {
            shift: true,
            alt: true,
            ..Modifiers::NONE
        },
        Modifiers {
            shift: true,
            command: true,
            mac_cmd: true,
            ..Modifiers::NONE
        },
    ] {
        tap(&mut s, Key::I, modifiers);
        assert!(!egui::Popup::is_any_open(&s.ctx));
    }
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
    tap(&mut s, Key::I, Modifiers::SHIFT);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(s.ctx.memory(|memory| memory.focused()), field);
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.right_click(Control::Viewport).unwrap();
    assert!(s.trace.get(Control::ViewportMenu).is_ok());
    tap(&mut s, Key::I, Modifiers::SHIFT);
    assert!(s.trace.get(Control::ViewportMenu).is_ok());
    assert!(s.trace.get(Control::InsertCube).is_err());
    tap(&mut s, Key::Escape, Modifiers::NONE);

    let anchor = s.state.viewport.center();
    s.frame(
        vec![
            Event::PointerMoved(anchor),
            key(Key::Backtick, true, Modifiers::NONE),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.view_pie_active());
    tap(&mut s, Key::I, Modifiers::SHIFT);
    assert!(s.state.view_pie_active());
    assert!(!egui::Popup::is_any_open(&s.ctx));
    s.frame(
        vec![key(Key::Backtick, false, Modifiers::NONE)],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.document, original);
    assert_eq!(s.state.editor.selected_object, Some(object));
}

#[test]
fn insert_shortcut_leaves_active_pointer_gestures_and_transform_preview_untouched() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let object = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(object).unwrap();
    let original = s.state.editor.document.clone();
    for held in [
        PointerButton::Primary,
        PointerButton::Secondary,
        PointerButton::Middle,
    ] {
        let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(40., -40.);
        s.frame(
            vec![Event::PointerMoved(start), pointer(start, held, true)],
            Duration::ZERO,
        )
        .unwrap();
        tap(&mut s, Key::I, Modifiers::SHIFT);
        assert!(!egui::Popup::is_any_open(&s.ctx));
        assert!(if held == PointerButton::Primary {
            s.state.editor.is_interacting()
        } else {
            s.state.mouse_navigation_active()
        });
        tap(&mut s, Key::Escape, Modifiers::NONE);
        s.frame(vec![pointer(start, held, false)], Duration::ZERO)
            .unwrap();
        assert_eq!(s.state.editor.selected_object, Some(object));
        assert_eq!(s.state.editor.document, original);
    }
    s.state.editor.tool = crate::editor::Tool::Move;
    s.settle().unwrap();
    let start = s.trace.get(Control::TransformX).unwrap().rect.center();
    // Cross one centimeter so the ownership check holds a changed preview.
    let end = start + egui::vec2(120., 0.);
    s.frame(
        vec![
            Event::PointerMoved(start),
            pointer(start, PointerButton::Primary, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(vec![Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    assert!(s.state.editor.is_transforming());
    let preview = s.state.editor.document.clone();
    assert_ne!(preview, original);
    let pose = s.state.camera.view_projection(s.state.aspect());
    tap(&mut s, Key::I, Modifiers::SHIFT);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert!(s.state.editor.is_transforming());
    assert_eq!(s.state.editor.document, preview);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
    tap(&mut s, Key::Escape, Modifiers::NONE);
    s.frame(
        vec![pointer(end, PointerButton::Primary, false)],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.document, original);
}
