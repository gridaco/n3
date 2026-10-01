//! Menus keep egui's keyboard focus and route activation through real actions.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{
    document::{Geometry, PrimitiveKind},
    settings::ThemeMode,
    shortcuts::HostEffect,
};
use egui::{Key, Modifiers};

fn tap(s: &mut Session<'_>, key: Key) {
    tap_modified(s, key, Modifiers::NONE);
}

fn tap_modified(s: &mut Session<'_>, key: Key, modifiers: Modifiers) {
    s.key(key, true, modifiers).unwrap();
    s.key(key, false, Modifiers::NONE).unwrap();
}

fn assert_focused(s: &Session<'_>, control: Control) {
    let focused = s.ctx.memory(|memory| memory.focused());
    let focused = focused.expect("An open menu must own keyboard focus");
    assert_eq!(
        s.ctx.read_response(focused).unwrap().rect,
        s.trace.get(control).unwrap().rect,
        "Keyboard focus must belong to the actual {control:?} menu row"
    );
}

fn assert_viewport_focused(s: &Session<'_>) {
    assert_eq!(
        s.ctx.memory(|memory| memory.focused()),
        Some(crate::shortcuts::viewport_focus_id()),
        "Dismissing the root menu returns keyboard control to the viewport"
    );
}

#[test]
fn mouse_open_main_menu_supports_arrow_submenus_and_keyboard_activation() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.editor.document.clone();

    s.click(Control::N3Menu).unwrap();
    assert_focused(&s, Control::FileMenu);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::ViewMenu);
    tap(&mut s, Key::ArrowUp);
    assert_focused(&s, Control::FileMenu);
    tap(&mut s, Key::ArrowRight);
    assert_focused(&s, Control::New);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::Open);
    tap(&mut s, Key::ArrowLeft);
    assert!(egui::Popup::is_any_open(&s.ctx));
    assert!(s.trace.get(Control::New).is_err());
    assert_focused(&s, Control::FileMenu);

    tap(&mut s, Key::ArrowRight);
    assert_focused(&s, Control::New);
    s.extra_layout_pass = true;
    tap(&mut s, Key::Escape);
    assert!(egui::Popup::is_any_open(&s.ctx));
    assert!(s.trace.get(Control::New).is_err());
    assert_focused(&s, Control::FileMenu);
    tap(&mut s, Key::Escape);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_viewport_focused(&s);
    assert_eq!(s.state.editor.document, original);

    s.click(Control::N3Menu).unwrap();
    tap(&mut s, Key::Enter);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::Open);
    s.extra_layout_pass = true;
    tap(&mut s, Key::Enter);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(s.take_host_effects(), [HostEffect::Open]);
    assert_eq!(s.state.editor.document, original);
    assert!(!s.state.editor.edit_mode);
    assert_viewport_focused(&s);
}

#[test]
fn mouse_open_context_menu_skips_disabled_rows_and_owns_document_keys() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    s.state.editor.deselect();
    s.settle().unwrap();
    let original = s.state.editor.document.clone();

    s.right_click(Control::Viewport).unwrap();
    assert_focused(&s, Control::SelectAll);
    assert!(!s.trace.get(Control::DuplicateSelection).unwrap().enabled);
    assert!(!s.trace.get(Control::DeleteSelection).unwrap().enabled);
    assert!(!s.trace.get(Control::FrameSelection).unwrap().enabled);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::ViewportFrame);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::ViewportPreferences);
    tap(&mut s, Key::ArrowUp);
    assert_focused(&s, Control::ViewportFrame);
    tap(&mut s, Key::ArrowUp);
    assert_focused(&s, Control::SelectAll);
    assert!(s.state.editor.selected_objects.is_empty());
    assert_eq!(s.state.editor.document, original);

    s.extra_layout_pass = true;
    tap(&mut s, Key::Enter);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(
        s.state.editor.selected_objects.len(),
        original.objects.len()
    );
    assert!(!s.state.editor.edit_mode);
    assert_viewport_focused(&s);

    s.right_click(Control::Viewport).unwrap();
    assert_focused(&s, Control::SelectAll);
    assert!(s.trace.get(Control::DeleteSelection).unwrap().enabled);
    s.shortcut("selection.delete").unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    assert_eq!(s.state.editor.document, original);
    assert_eq!(
        s.state.editor.selected_objects.len(),
        original.objects.len()
    );
    s.extra_layout_pass = true;
    tap(&mut s, Key::Escape);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_viewport_focused(&s);
    assert_eq!(s.state.editor.document, original);
    assert_eq!(
        s.state.editor.selected_objects.len(),
        original.objects.len()
    );
    assert!(
        !s.state.editor.undo(),
        "Menu navigation, selection, and cancellation must not create document history"
    );
}

#[test]
fn keyboard_submenu_changes_ignore_stationary_pointer_and_mouse_insert_is_navigable() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.editor.document.clone();

    // Leave the physical pointer over File while navigating to its sibling.
    // Merely hovering that stationary pointer must not reopen the old submenu.
    s.click_path(&[Control::N3Menu, Control::FileMenu]).unwrap();
    tap(&mut s, Key::ArrowLeft);
    assert_focused(&s, Control::FileMenu);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::ViewMenu);
    tap(&mut s, Key::ArrowRight);
    assert_focused(&s, Control::ViewPerspective);
    assert!(s.trace.get(Control::New).is_err());
    assert_eq!(s.state.editor.document, original);
    tap(&mut s, Key::Escape);
    assert_focused(&s, Control::ViewMenu);
    tap(&mut s, Key::Escape);
    assert_viewport_focused(&s);

    s.click(Control::InsertMenu).unwrap();
    assert_focused(&s, Control::InsertCube);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::InsertCylinder);
    s.extra_layout_pass = true;
    tap(&mut s, Key::Enter);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_viewport_focused(&s);
    assert_eq!(
        s.state.editor.document.objects.len(),
        original.objects.len() + 1
    );
    assert!(matches!(
        &s.state.editor.document.objects.last().unwrap().geometry,
        Geometry::Primitive(primitive) if primitive.kind == PrimitiveKind::Cylinder
    ));
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, original);
    assert!(!s.state.editor.undo());
}

#[test]
fn menu_traversal_wraps_and_tab_stays_inside_until_space_activation() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    s.state.editor.deselect();
    s.settle().unwrap();
    let original = s.state.editor.document.clone();

    s.right_click(Control::Viewport).unwrap();
    assert_focused(&s, Control::SelectAll);
    tap(&mut s, Key::ArrowUp);
    assert_focused(&s, Control::ViewportPreferences);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::SelectAll);
    for control in [
        Control::ViewportFrame,
        Control::ViewportPreferences,
        Control::SelectAll,
    ] {
        tap(&mut s, Key::Tab);
        assert_focused(&s, control);
        assert!(egui::Popup::is_any_open(&s.ctx));
    }
    tap_modified(&mut s, Key::Tab, Modifiers::SHIFT);
    assert_focused(&s, Control::ViewportPreferences);
    tap(&mut s, Key::Tab);
    assert_focused(&s, Control::SelectAll);
    assert!(s.state.editor.selected_objects.is_empty());
    tap(&mut s, Key::Space);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_viewport_focused(&s);
    assert_eq!(
        s.state.editor.selected_objects.len(),
        original.objects.len()
    );
    assert_eq!(s.state.editor.document, original);
    assert!(!s.state.editor.edit_mode);

    s.right_click(Control::Viewport).unwrap();
    let outside = s.empty_viewport_point().unwrap();
    s.click_at(outside).unwrap();
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_viewport_focused(&s);
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn dismissing_menu_into_a_numeric_field_preserves_the_clicked_fields_focus() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.editor.document.clone();
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    let field_position = egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    );
    s.click_at(field_position).unwrap();
    let field = s.ctx.memory(|memory| memory.focused());
    assert!(field.is_some() && field != Some(crate::shortcuts::viewport_focus_id()));
    assert!(s.ctx.egui_wants_keyboard_input());

    s.click(Control::N3Menu).unwrap();
    assert_focused(&s, Control::FileMenu);
    s.click_at(field_position).unwrap();
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(s.ctx.memory(|memory| memory.focused()), field);
    assert!(s.ctx.egui_wants_keyboard_input());
    s.settle().unwrap();
    assert_eq!(s.ctx.memory(|memory| memory.focused()), field);
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn preference_value_menu_starts_on_selected_option_and_restores_trigger_focus() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.editor.document.clone();
    assert_eq!(s.state.theme_mode, ThemeMode::Light);
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.click(Control::AppearanceThemeMenu).unwrap();
    assert_focused(&s, Control::ThemeLight);
    tap(&mut s, Key::Escape);
    assert_focused(&s, Control::AppearanceThemeMenu);
    assert!(s.state.show_preferences);
    tap(&mut s, Key::Enter);
    assert_focused(&s, Control::ThemeLight);
    tap(&mut s, Key::ArrowDown);
    assert_focused(&s, Control::ThemeDark);
    assert_eq!(s.state.theme_mode, ThemeMode::Light);
    s.extra_layout_pass = true;
    tap(&mut s, Key::Enter);
    assert_eq!(s.state.theme_mode, ThemeMode::Dark);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert!(s.state.show_preferences);
    assert_focused(&s, Control::AppearanceThemeMenu);

    s.click(Control::AppearanceThemeMenu).unwrap();
    assert_focused(&s, Control::ThemeDark);
    tap(&mut s, Key::ArrowUp);
    assert_focused(&s, Control::ThemeLight);
    assert_eq!(s.state.theme_mode, ThemeMode::Dark);
    s.extra_layout_pass = true;
    tap(&mut s, Key::Escape);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert!(s.state.show_preferences);
    assert_focused(&s, Control::AppearanceThemeMenu);
    assert_eq!(s.state.theme_mode, ThemeMode::Dark);
    tap(&mut s, Key::Escape);
    assert!(
        !s.state.show_preferences,
        "The next Escape closes the parent dialog"
    );
    assert_eq!(s.state.editor.document, original);
    assert!(!s.state.editor.undo());
}
