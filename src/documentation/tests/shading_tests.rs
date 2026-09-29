//! Production routing for the context-sensitive held shading shortcut.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{editor::Tool, render::shading::ShadingMode, shortcuts::viewport_focus_id};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
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

fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

fn setup(s: &mut Session<'_>) {
    s.load_fixture("cube-quads.obj").unwrap();
    let object = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(object).unwrap();
    s.state.editor.set_tool(Tool::View);
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.settle().unwrap();
}

fn open(s: &mut Session<'_>) {
    let anchor = s.state.viewport.center();
    s.frame(vec![Event::PointerMoved(anchor)], Duration::ZERO)
        .unwrap();
    s.shortcut_down("shading.pie").unwrap();
    assert!(s.state.shading_pie_active() && s.state.pie_owns_input());
}

#[test]
fn shading_z_is_exclusively_axis_lock_for_every_transform_tool_even_without_selection() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for (binding, tool) in [
        ("tool.move", Tool::Move),
        ("tool.rotate", Tool::Rotate),
        ("tool.scale", Tool::Scale),
    ] {
        for selected in [false, true] {
            let mut s = Session::new(&mut capture).unwrap();
            setup(&mut s);
            if !selected {
                s.shortcut("cancel").unwrap();
            }
            assert_eq!(s.state.editor.selected_objects.len(), usize::from(selected));
            let document = s.state.editor.document.clone();
            let objects = s.state.editor.selected_objects.clone();
            s.shortcut(binding).unwrap();
            assert_eq!(s.state.editor.tool, tool);
            // The same physical key serves disjoint contexts, with no delayed
            // pie when the axis action cannot apply to an empty selection.
            s.shortcut_down("transform.axis-z").unwrap();
            assert!(!s.state.shading_pie_active() && !s.state.pie_owns_input());
            assert_eq!(s.state.editor.transform_axis, Some(2));
            s.wait(Duration::from_millis(350)).unwrap();
            s.shortcut_up("transform.axis-z").unwrap();
            assert!(!s.state.shading_pie_active());
            assert_eq!(s.state.shading, ShadingMode::Solid);
            assert_eq!(s.state.editor.document, document);
            assert_eq!(s.state.editor.selected_objects, objects);
        }
    }
}

#[test]
fn shading_uses_tool_changes_earlier_in_the_same_input_batch() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let anchor = s.state.viewport.center();
    s.frame(
        vec![
            Event::PointerMoved(anchor),
            s.shortcut_event("tool.move", true).unwrap(),
            s.shortcut_event("shading.pie", true).unwrap(),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.tool, Tool::Move);
    assert_eq!(s.state.editor.transform_axis, Some(2));
    assert!(!s.state.shading_pie_active());
    s.frame(
        vec![
            s.shortcut_event("tool.move", false).unwrap(),
            s.shortcut_event("shading.pie", false).unwrap(),
        ],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![
            s.shortcut_event("tool.cursor", true).unwrap(),
            s.shortcut_event("shading.pie", true).unwrap(),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.tool, Tool::View);
    assert_eq!(s.state.editor.transform_axis, None);
    assert!(s.state.shading_pie_active());
    s.shortcut_up("tool.cursor").unwrap();
    s.shortcut_up("shading.pie").unwrap();
    assert_eq!(s.state.shading, ShadingMode::Solid);
}

#[test]
fn shading_release_applies_once_without_document_history_or_navigation_changes() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let baseline = s.state.editor.document.clone();
    s.state
        .editor
        .commit("Rename", |document| {
            document.objects[0].name = "Shading preserves this edit".into();
            Ok(())
        })
        .unwrap();
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let selected = s.state.editor.selected_objects.clone();
    let pose = s.state.camera.view_projection(s.state.aspect());
    for (choice, expected) in [
        (Control::PieWireframe, ShadingMode::Wireframe),
        (Control::PieSolid, ShadingMode::Solid),
    ] {
        let before = s.state.shading;
        open(&mut s);
        s.hover(choice).unwrap();
        assert_eq!(s.state.shading, before);
        // Neither camera gestures nor editing commands leak through the pie.
        s.scroll(20.0, -15.0, true, Modifiers::NONE).unwrap();
        s.pinch(0.2).unwrap();
        s.trackpad_rotate(15.0).unwrap();
        s.shortcut("selection.delete").unwrap();
        s.shortcut("view.front").unwrap();
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
        assert!(s.state.shading_pie_active());
        s.shortcut_up("shading.pie").unwrap();
        assert_eq!(s.state.shading, expected);
        assert!(!s.state.shading_pie_active());
        assert_eq!(s.state.editor.revision, revision);
        assert_eq!(s.state.editor.selected_objects, selected);
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
    }
    assert!(s.state.editor.undo());
    assert_eq!(s.state.editor.document, baseline);
}

#[test]
fn shading_center_escape_and_focus_loss_cancel_without_applying_hovered_choice() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let selected = s.state.editor.selected_objects.clone();
    let document = s.state.editor.document.clone();
    open(&mut s);
    s.hover(Control::PieWireframe).unwrap();
    let anchor = s.state.viewport.center();
    s.frame(vec![Event::PointerMoved(anchor)], Duration::ZERO)
        .unwrap();
    s.shortcut_up("shading.pie").unwrap();
    assert_eq!(s.state.shading, ShadingMode::Solid);
    for cancel in [
        key(Key::Escape, true),
        Event::PointerGone,
        Event::WindowFocused(false),
    ] {
        open(&mut s);
        s.hover(Control::PieWireframe).unwrap();
        s.frame(vec![cancel], Duration::ZERO).unwrap();
        assert!(!s.state.shading_pie_active());
        assert!(s.state.pie_owns_input());
        s.frame(
            vec![Event::WindowFocused(true), key(Key::Escape, false)],
            Duration::ZERO,
        )
        .unwrap();
        s.shortcut_up("shading.pie").unwrap();
        assert!(!s.state.shading_pie_active() && !s.state.pie_owns_input());
        assert_eq!(s.state.shading, ShadingMode::Solid);
        assert_eq!(s.state.editor.selected_objects, selected);
        assert_eq!(s.state.editor.document, document);
    }
}

#[test]
fn shading_respects_text_popup_and_pointer_gesture_ownership() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2.0 * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    let field = s.ctx.memory(|memory| memory.focused());
    assert!(field.is_some() && field != Some(viewport_focus_id()));
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(35.0, -35.0);
    s.frame(vec![Event::PointerMoved(empty)], Duration::ZERO)
        .unwrap();
    s.shortcut_down("shading.pie").unwrap();
    assert!(!s.state.shading_pie_active() && !s.state.pie_owns_input());
    assert_eq!(s.ctx.memory(|memory| memory.focused()), field);
    s.shortcut_up("shading.pie").unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.click_at(empty).unwrap();
    s.right_click(Control::Viewport).unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    s.shortcut_down("shading.pie").unwrap();
    assert!(!s.state.shading_pie_active() && !s.state.pie_owns_input());
    assert!(egui::Popup::is_any_open(&s.ctx));
    s.shortcut_up("shading.pie").unwrap();
    s.shortcut("cancel").unwrap();
    for held in [
        PointerButton::Primary,
        PointerButton::Middle,
        PointerButton::Secondary,
    ] {
        s.frame(
            vec![
                Event::PointerMoved(empty),
                button(empty, held, true),
                key(Key::Z, true),
            ],
            Duration::ZERO,
        )
        .unwrap();
        assert!(!s.state.shading_pie_active() && !s.state.pie_owns_input());
        let end = empty + egui::vec2(30.0, -25.0);
        s.frame(vec![Event::PointerMoved(end)], Duration::ZERO)
            .unwrap();
        s.shortcut_up("shading.pie").unwrap();
        s.shortcut_down("shading.pie").unwrap();
        assert!(!s.state.shading_pie_active() && !s.state.pie_owns_input());
        s.shortcut_up("shading.pie").unwrap();
        s.shortcut("cancel").unwrap();
        s.frame(vec![button(end, held, false)], Duration::ZERO)
            .unwrap();
    }
    assert_eq!(s.state.shading, ShadingMode::Solid);
}
