//! Real input routing for the temporary Alt/Option orbit tool.

use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{camera::View, shortcuts::viewport_focus_id};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::time::Duration;

fn pose(s: &Session<'_>) -> glam::Mat4 {
    s.state.camera.view_projection(s.state.aspect())
}

fn pointer(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
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

fn start(s: &Session<'_>) -> Pos2 {
    s.state.viewport_ui_rect.left_bottom() + egui::vec2(60.0, -75.0)
}

#[test]
fn alt_orbit_waits_for_drag_and_preserves_objects_vertices_and_history() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for edit_mode in [false, true] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        if edit_mode {
            s.state.editor.enter_edit().unwrap();
            s.state.editor.selected_vertices.insert(1);
        }
        let baseline = s.state.editor.document.clone();
        s.state
            .editor
            .commit("Rename", |document| {
                document.objects[0].name = "Orbit preserves this edit".into();
                Ok(())
            })
            .unwrap();
        let document = s.state.editor.document.clone();
        let revision = s.state.editor.revision;
        let objects = s.state.editor.selected_objects.clone();
        let vertices = s.state.editor.selected_vertices.clone();
        let initial = pose(&s);
        let position = s.state.viewport.center();
        s.modifiers_changed(Modifiers::ALT).unwrap();
        assert!(s.state.orbit_tool_active());
        // Even a repeated click on the object must not enter/leave edit mode.
        for _ in 0..2 {
            s.frame(
                vec![
                    Event::PointerMoved(position),
                    pointer(position, true, Modifiers::ALT),
                    pointer(position, false, Modifiers::ALT),
                ],
                Duration::from_millis(30),
            )
            .unwrap();
            assert!(s.state.is_planar_navigation());
            assert_eq!(pose(&s), initial);
            assert_eq!(s.state.editor.edit_mode, edit_mode);
        }
        let start = start(&s);
        s.frame(
            vec![
                Event::PointerMoved(start),
                pointer(start, true, Modifiers::ALT),
            ],
            Duration::ZERO,
        )
        .unwrap();
        s.frame(
            vec![Event::PointerMoved(start + egui::vec2(1.0, 0.0))],
            Duration::ZERO,
        )
        .unwrap();
        assert!(
            s.state.is_planar_navigation(),
            "Sub-threshold motion cannot leave Planar"
        );
        assert_eq!(pose(&s), initial);
        let delta = egui::vec2(32.0, -18.0);
        let end = start + delta;
        let mut expected = s.state.camera.clone();
        expected.orbit(delta.x, delta.y);
        s.extra_layout_pass = true;
        s.frame(vec![Event::PointerMoved(end)], Duration::ZERO)
            .unwrap();
        assert!(!s.state.is_planar_navigation());
        assert!(
            s.state.camera.is_orthographic(),
            "Manual orbit preserves projection"
        );
        assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
        assert!(!s.state.editor.is_interacting());
        s.frame(vec![pointer(end, false, Modifiers::ALT)], Duration::ZERO)
            .unwrap();
        s.modifiers_changed(Modifiers::NONE).unwrap();
        assert!(!s.state.orbit_tool_active() && !s.state.mouse_navigation_active());
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.editor.revision, revision);
        assert_eq!(s.state.editor.selected_objects, objects);
        assert_eq!(s.state.editor.selected_vertices, vertices);
        assert_eq!(s.state.editor.edit_mode, edit_mode);
        assert!(s.state.editor.undo());
        assert_eq!(s.state.editor.document, baseline);
        assert!(
            !s.state.editor.undo(),
            "Orbit must not create an undo entry"
        );
    }
}

#[test]
fn ordered_alt_release_stops_motion_without_reassigning_the_held_primary_press() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let start = start(&s);
    let delta = egui::vec2(28.0, -16.0);
    let end = start + delta;
    let mut expected = s.state.camera.clone();
    expected.orbit(delta.x, delta.y);
    // Preserve native Alt-down / drag / Alt-up / motion ordering in one frame.
    s.extra_layout_pass = true;
    s.frame(
        vec![
            Event::PointerMoved(start),
            Event::ModifiersChanged(Modifiers::ALT),
            pointer(start, true, Modifiers::ALT),
            Event::PointerMoved(end),
            Event::ModifiersChanged(Modifiers::NONE),
            Event::PointerMoved(start),
            key(Key::F24, false, Modifiers::NONE),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
    assert!(!s.state.orbit_tool_active() && !s.state.mouse_navigation_active());
    // Repressing Alt while the button remains held must not start a new orbit.
    s.modifiers_changed(Modifiers::ALT).unwrap();
    s.frame(
        vec![Event::PointerMoved(end + egui::vec2(30.0, 20.0))],
        Duration::ZERO,
    )
    .unwrap();
    assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
    s.modifiers_changed(Modifiers::NONE).unwrap();
    s.frame(vec![pointer(start, false, Modifiers::NONE)], Duration::ZERO)
        .unwrap();
    assert_eq!(s.state.editor.selected_objects, selection);
    assert_eq!(s.state.editor.document, document);
    assert!(!s.state.editor.is_interacting());

    // A fresh unmodified press remains normal selection input.
    s.click_at(start).unwrap();
    assert!(s.state.editor.selected_objects.is_empty());
}

#[test]
fn escape_cancels_owned_alt_orbit_without_leaking_the_primary_release() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for moved in [false, true] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        let document = s.state.editor.document.clone();
        let selected = s.state.editor.selected_objects.clone();
        let start = start(&s);
        let end = start + egui::vec2(35.0, -20.0);
        s.modifiers_changed(Modifiers::ALT).unwrap();
        s.frame(
            vec![
                Event::PointerMoved(start),
                pointer(start, true, Modifiers::ALT),
            ],
            Duration::ZERO,
        )
        .unwrap();
        if moved {
            s.frame(vec![Event::PointerMoved(end)], Duration::ZERO)
                .unwrap();
        }
        assert!(s.state.mouse_navigation_active());
        let before = pose(&s);
        s.extra_layout_pass = true;
        s.key(Key::Escape, true, Modifiers::ALT).unwrap();
        assert!(!s.state.mouse_navigation_active());
        s.frame(
            vec![Event::PointerMoved(end + egui::vec2(30.0, 10.0))],
            Duration::ZERO,
        )
        .unwrap();
        s.key(Key::Escape, false, Modifiers::ALT).unwrap();
        s.frame(vec![pointer(end, false, Modifiers::ALT)], Duration::ZERO)
            .unwrap();
        s.modifiers_changed(Modifiers::NONE).unwrap();
        assert_eq!(pose(&s), before);
        assert_eq!(s.state.editor.selected_objects, selected);
        assert_eq!(s.state.editor.document, document);
        assert!(!s.state.editor.is_interacting());
    }
}

#[test]
fn space_has_initial_priority_and_temporary_navigation_never_hands_off_mid_drag() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for begin_with_pan in [true, false] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        let start = start(&s);
        let delta = egui::vec2(28.0, -12.0);
        s.modifiers_changed(Modifiers::ALT).unwrap();
        if begin_with_pan {
            s.key(Key::Space, true, Modifiers::ALT).unwrap();
            assert!(s.state.hand_tool_active());
        }
        s.frame(
            vec![
                Event::PointerMoved(start),
                pointer(start, true, Modifiers::ALT),
            ],
            Duration::ZERO,
        )
        .unwrap();
        if !begin_with_pan {
            s.key(Key::Space, true, Modifiers::ALT).unwrap();
        }
        let mut expected = s.state.camera.clone();
        if begin_with_pan {
            expected.pan(delta.x, delta.y, s.state.viewport.height());
        } else {
            expected.orbit(delta.x, delta.y);
        }
        s.frame(vec![Event::PointerMoved(start + delta)], Duration::ZERO)
            .unwrap();
        assert!(pose(&s).abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5));
        assert_eq!(s.state.is_planar_navigation(), begin_with_pan);
        if begin_with_pan {
            s.key(Key::Space, false, Modifiers::ALT).unwrap();
        } else {
            s.modifiers_changed(Modifiers::NONE).unwrap();
        }
        let stopped = pose(&s);
        s.frame(vec![Event::PointerMoved(start)], Duration::ZERO)
            .unwrap();
        assert_eq!(
            pose(&s),
            stopped,
            "Releasing the owner cannot switch to the other held tool"
        );
        let modifiers = if begin_with_pan {
            Modifiers::ALT
        } else {
            Modifiers::NONE
        };
        s.frame(vec![pointer(start, false, modifiers)], Duration::ZERO)
            .unwrap();
        s.key(Key::Space, false, Modifiers::NONE).unwrap();
        s.modifiers_changed(Modifiers::NONE).unwrap();
        assert!(!s.state.editor.selected_objects.is_empty());
    }
}

#[test]
fn alt_orbit_respects_text_popup_focus_and_gizmo_ownership() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let before = pose(&s);
    s.state.animate_views = true;
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
    assert!(
        s.ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| id != viewport_focus_id())
    );
    s.modifiers_changed(Modifiers::ALT).unwrap();
    assert!(!s.state.orbit_tool_active());
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.settle().unwrap();
    assert!(
        !s.state.orbit_tool_active(),
        "An Alt hold begun in a field cannot latch after focus transfer"
    );
    s.modifiers_changed(Modifiers::NONE).unwrap();
    s.right_click(Control::Viewport).unwrap();
    s.modifiers_changed(Modifiers::ALT).unwrap();
    assert!(!s.state.orbit_tool_active());
    s.key(Key::Escape, true, Modifiers::ALT).unwrap();
    s.key(Key::Escape, false, Modifiers::ALT).unwrap();
    assert!(!s.state.orbit_tool_active());
    s.modifiers_changed(Modifiers::NONE).unwrap();
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.state.animate_views = false;
    s.modifiers_changed(Modifiers::ALT).unwrap();
    assert!(s.state.orbit_tool_active());
    s.hover(Control::AxisX).unwrap();
    assert_eq!(
        s.cursor,
        egui::CursorIcon::PointingHand,
        "Gizmo owns its cursor"
    );
    assert!(!s.state.mouse_navigation_active());
    s.click(Control::AxisX).unwrap();
    assert!(s.state.is_planar_navigation());
    assert!(s.state.camera.direction_in_view(glam::Vec3::X).z > 0.999);
    assert!(
        !s.state.mouse_navigation_active(),
        "Alt must not replace a gizmo click with viewport orbit"
    );
    s.state.set_view(View::Front);
    s.settle().unwrap();
    let id = s.state.editor.selected_object.unwrap();
    let row = s.state.layer_row_rect(id).unwrap().center();
    let end = start(&s);
    s.frame(
        vec![Event::PointerMoved(row), pointer(row, true, Modifiers::ALT)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(vec![Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    s.frame(vec![pointer(end, false, Modifiers::ALT)], Duration::ZERO)
        .unwrap();
    assert_eq!(
        pose(&s),
        before,
        "A Layers press cannot become viewport orbit when dragged across the boundary"
    );
    assert!(!s.state.mouse_navigation_active());
    s.modifiers_changed(Modifiers::NONE).unwrap();
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.modifiers_changed(Modifiers::ALT).unwrap();
    s.frame(
        vec![Event::PointerMoved(end), pointer(end, true, Modifiers::ALT)],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.mouse_navigation_active());
    s.frame(vec![Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    s.frame(vec![Event::WindowFocused(true)], Duration::ZERO)
        .unwrap();
    assert!(
        !s.state.orbit_tool_active(),
        "A missing unfocused Alt-up cannot leave a latched tool"
    );
    assert!(!s.state.mouse_navigation_active());
    s.frame(
        vec![
            Event::PointerMoved(end + egui::vec2(40.0, 20.0)),
            pointer(end, false, Modifiers::NONE),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.selected_object, Some(id));
    assert_eq!(pose(&s), before);
}

#[test]
fn alt_cannot_hijack_an_existing_selection_or_transform_drag() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for transforming in [false, true] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        let start = if transforming {
            s.click(Control::ToolMove).unwrap();
            s.target(Control::TransformX).unwrap()
        } else {
            start(&s)
        };
        s.frame(
            vec![
                Event::PointerMoved(start),
                pointer(start, true, Modifiers::NONE),
            ],
            Duration::ZERO,
        )
        .unwrap();
        s.frame(
            vec![Event::PointerMoved(start + egui::vec2(30.0, 0.0))],
            Duration::ZERO,
        )
        .unwrap();
        assert!(s.state.editor.is_pointer_interacting());
        let before = pose(&s);
        s.modifiers_changed(Modifiers::ALT).unwrap();
        assert!(!s.state.orbit_tool_active());
        s.frame(
            vec![Event::PointerMoved(start + egui::vec2(60.0, 0.0))],
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(pose(&s), before);
        assert!(s.state.editor.is_pointer_interacting());
        s.frame(
            vec![pointer(
                start + egui::vec2(60.0, 0.0),
                false,
                Modifiers::ALT,
            )],
            Duration::ZERO,
        )
        .unwrap();
        s.modifiers_changed(Modifiers::NONE).unwrap();
        assert!(!s.state.mouse_navigation_active());
    }
}
