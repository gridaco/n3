//! X-ray shares the native keyboard, pie, and temporary navigation routes.
use super::{Capture, HEIGHT, Session, WIDTH};
use crate::{camera::View, shortcuts::viewport_focus_id};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::time::Duration;

fn pointer(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

fn setup(s: &mut Session<'_>) {
    s.load_fixture("cube-quads.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    s.state.animate_views = false;
    s.state.set_view(View::Front);
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.settle().unwrap();
}

#[test]
fn option_z_changes_only_xray_in_each_tool_and_option_drag_still_orbits() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    for tool in ["tool.cursor", "tool.move", "tool.rotate", "tool.scale"] {
        s.shortcut(tool).unwrap();
        if tool != "tool.cursor" {
            s.shortcut("transform.axis-x").unwrap();
        }
        let axis = s.state.editor.transform_axis;
        let document = s.state.editor.document.clone();
        let revision = s.state.editor.revision;
        let selected = s.state.editor.selected_objects.clone();
        let pose = s.state.camera.view_projection(s.state.aspect());
        s.modifiers_changed(Modifiers::ALT).unwrap();
        s.shortcut_down("view.xray").unwrap();
        assert!(s.state.editor.xray_enabled());
        assert!(!s.state.shading_pie_active());
        assert_eq!(s.state.editor.transform_axis, axis);
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
        s.frame(
            vec![
                Event::Key {
                    key: Key::Z,
                    physical_key: Some(Key::Z),
                    pressed: true,
                    repeat: true,
                    modifiers: Modifiers::ALT,
                },
                Event::Text("Ω".into()),
            ],
            Duration::ZERO,
        )
        .unwrap();
        assert!(
            s.state.editor.xray_enabled(),
            "repeat must not toggle again"
        );
        s.shortcut_up("view.xray").unwrap();
        s.modifiers_changed(Modifiers::NONE).unwrap();
        s.shortcut("view.xray").unwrap();
        assert!(!s.state.editor.xray_enabled());
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.editor.revision, revision);
        assert_eq!(s.state.editor.selected_objects, selected);
    }
    s.shortcut("tool.cursor").unwrap();
    let pose = s.state.camera.view_projection(s.state.aspect());
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(80.0, -100.0);
    s.modifiers_changed(Modifiers::ALT).unwrap();
    s.frame(
        vec![
            Event::PointerMoved(start),
            pointer(start, true, Modifiers::ALT),
        ],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![Event::PointerMoved(start + egui::vec2(60.0, -25.0))],
        Duration::from_millis(30),
    )
    .unwrap();
    assert_ne!(s.state.camera.view_projection(s.state.aspect()), pose);
    s.shortcut("view.xray").unwrap();
    assert!(
        !s.state.editor.xray_enabled(),
        "an active orbit owns its input"
    );
    s.frame(vec![pointer(start, false, Modifiers::ALT)], Duration::ZERO)
        .unwrap();
    s.modifiers_changed(Modifiers::NONE).unwrap();
}

#[test]
fn xray_does_not_change_an_active_marquee_policy() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.shortcut("tool.cursor").unwrap();
    s.shortcut("edit.confirm").unwrap();
    let start = s.state.viewport.center() - egui::vec2(150.0, 150.0);
    s.frame(
        vec![
            Event::PointerMoved(start),
            pointer(start, true, Modifiers::NONE),
        ],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![Event::PointerMoved(start + egui::vec2(300.0, 300.0))],
        Duration::from_millis(30),
    )
    .unwrap();
    assert!(s.state.editor.is_pointer_interacting());
    s.shortcut("view.xray").unwrap();
    assert!(!s.state.editor.xray_enabled());
    s.frame(
        vec![pointer(
            start + egui::vec2(300.0, 300.0),
            false,
            Modifiers::NONE,
        )],
        Duration::ZERO,
    )
    .unwrap();
    s.shortcut("view.xray").unwrap();
    assert!(s.state.editor.xray_enabled());
}
