//! The floating Tool Dock tab bar own their pointer origin, not the viewport below.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{editor::Tool, navigation_events::Event as NavigationEvent, scroll_input::ScrollPhase};
use egui::{Event, Modifiers, PointerButton, Pos2};
use std::time::Duration;

fn setup(s: &mut Session<'_>) {
    s.load_fixture("cube-quads.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    s.state.editor.set_tool(Tool::View);
    s.state
        .mark_saved("tool-dock-tabs.n3.json".into(), Vec::new());
    s.ctx
        .memory_mut(|memory| memory.request_focus(crate::shortcuts::viewport_focus_id()));
    s.settle().unwrap();
    assert!(!s.state.animation_panel_is_open());
    assert!(s.state.tool_dock.floating_tabs_rect.is_some());
}

fn pose(s: &Session<'_>) -> glam::Mat4 {
    s.state.camera.view_projection(s.state.aspect())
}

fn pointer(pos: Pos2, button: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers,
    }
}

fn move_to(s: &mut Session<'_>, pos: Pos2) {
    s.frame(vec![Event::PointerMoved(pos)], Duration::ZERO)
        .unwrap();
}

#[test]
fn tool_dock_header_separator_paints_edge_to_edge_for_empty_and_imported_panels() {
    fn lines(shape: &egui::Shape, into: &mut Vec<[Pos2; 2]>) {
        match shape {
            egui::Shape::LineSegment { points, stroke } if stroke.width > 0.0 => {
                into.push(*points);
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    lines(shape, into);
                }
            }
            _ => {}
        }
    }

    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for theme in [
        crate::settings::ThemeMode::Light,
        crate::settings::ThemeMode::Dark,
    ] {
        for imported in [false, true] {
            let mut s = Session::new(&mut capture).unwrap();
            if imported {
                s.load_scene_fixture("SimpleSkin/glTF/SimpleSkin.gltf")
                    .unwrap();
            }
            s.state.theme_mode = theme;
            s.settle().unwrap();
            s.click(Control::AnimationPanelToggle).unwrap();
            let ctx = s.ctx.clone();
            // Inspect the shapes from a real, settled application UI pass.
            // No helper's predicted geometry substitutes for the painted line.
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        Pos2::ZERO,
                        egui::vec2(WIDTH as f32, HEIGHT as f32),
                    )),
                    time: Some(s.time),
                    focused: true,
                    ..Default::default()
                },
                |ui| s.state.ui(ui),
            );
            output.textures_delta.clear();
            let trace = crate::controls::snapshot(&ctx);
            let panel = trace.get(Control::AnimationTimeline).unwrap().rect;
            let tabs = trace.get(Control::ToolDockTabBar).unwrap().rect;
            let tab = trace.get(Control::AnimationPanelToggle).unwrap().rect;
            let close = trace.get(Control::ToolDockClose).unwrap().rect;
            let mut horizontal = Vec::new();
            for clipped in &output.shapes {
                let mut segments = Vec::new();
                lines(&clipped.shape, &mut segments);
                horizontal.extend(segments.into_iter().filter_map(|points| {
                    (points[0].y == points[1].y
                        && points[0].y > tabs.bottom()
                        && points[0].y < tabs.bottom() + 32.0
                        && (points[1].x - points[0].x).abs() > panel.width() * 0.5)
                        .then_some((points, clipped.clip_rect))
                }));
            }
            horizontal.sort_by(|(a, _), (b, _)| a[0].y.total_cmp(&b[0].y));
            let (points, clip) = horizontal.first().expect("Painted header separator");
            let left = points[0].x.min(points[1].x).max(clip.left());
            let right = points[0].x.max(points[1].x).min(clip.right());
            assert_eq!(
                (left, right),
                (panel.left(), panel.right()),
                "{theme:?}, imported={imported}: separator must reach the panel edges"
            );
            assert_eq!(tabs.height(), crate::theme::TOOL_DOCK_TAB_BAR_HEIGHT);
            assert_eq!(
                tab.top() - tabs.top(),
                crate::theme::TOOL_DOCK_TAB_BAR_INSET
            );
            assert_eq!(
                tabs.bottom() - tab.bottom(),
                crate::theme::TOOL_DOCK_TAB_BAR_INSET
            );
            assert_eq!(tab.center().y, tabs.center().y);
            assert_eq!(
                tab.left() - panel.left(),
                crate::theme::TOOL_DOCK_TAB_BAR_INSET
            );
            assert_eq!(
                panel.right() - close.right(),
                crate::theme::TOOL_DOCK_TAB_BAR_INSET
            );
            assert_eq!(tab.center().y, close.center().y);
            assert_eq!(tab.height(), close.height());
        }
    }
}

#[test]
fn tool_dock_resize_preserves_playback_and_owns_the_border_drag() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_scene_fixture("SimpleSkin/glTF/SimpleSkin.gltf")
        .unwrap();
    s.state
        .mark_saved("resize-animation.n3.json".into(), Vec::new());
    s.click(Control::AnimationPanelToggle).unwrap();
    s.shortcut("animation.play-pause").unwrap();
    let accepted = s.state.selected_asset_view().unwrap().clone();
    assert!(accepted.playback.playing);
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    // Compare the camera at a fixed aspect: resizing changes the viewport's
    // aspect legitimately, but must not pan, orbit, or reframe its camera.
    let camera = s.state.camera.view_projection(1.0);
    let initial = s.trace.get(Control::AnimationTimeline).unwrap().rect;
    for dy in [-80.0, 80.0] {
        let before = s.trace.get(Control::AnimationTimeline).unwrap().rect;
        // Target the panel side of egui's resize hit band, without relying on
        // the viewport and panel assigning their exact shared edge identically.
        let start = egui::pos2(before.center().x, before.top() + 1.0);
        move_to(&mut s, start);
        let border_cursor = s.cursor;
        assert_eq!(border_cursor, egui::CursorIcon::ResizeVertical);
        s.pointer_button(PointerButton::Primary, true).unwrap();
        move_to(&mut s, egui::pos2(start.x, before.top() + dy));
        assert!(!s.state.animation.timeline.has_pointer_gesture());
        assert!(
            !s.state.editor.is_pointer_interacting(),
            "Border {start:?} with cursor {border_cursor:?} leaked a viewport gesture; panel {before:?} -> {:?}, pointer {:?}",
            s.trace.get(Control::AnimationTimeline).unwrap().rect,
            s.input.position()
        );
        s.pointer_button(PointerButton::Primary, false).unwrap();
        s.settle().unwrap();
        let after = s.trace.get(Control::AnimationTimeline).unwrap().rect;
        assert_eq!(after.height(), before.height() - dy);
        assert_eq!(after.bottom(), before.bottom());
        let tab = s.trace.get(Control::AnimationPanelToggle).unwrap().rect;
        let close = s.trace.get(Control::ToolDockClose).unwrap().rect;
        let ruler = s.trace.get(Control::AnimationRuler).unwrap().rect;
        assert_eq!(tab.center().y, close.center().y);
        assert!(after.contains_rect(ruler));
        assert!(ruler.top() > tab.bottom());
        assert_eq!(s.state.camera.view_projection(1.0), camera);
        let view = s.state.selected_asset_view().unwrap();
        assert_eq!(view.playback, accepted.playback);
        assert_eq!(view.revision, accepted.revision);
        assert!(std::sync::Arc::ptr_eq(&view.frame, &accepted.frame));
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.editor.selected_objects, selection);
    }
    assert_eq!(
        s.trace.get(Control::AnimationTimeline).unwrap().rect,
        initial
    );
    s.click(Control::AnimationPanelToggle).unwrap();
    s.shortcut("animation.play-pause").unwrap();
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn floating_tool_dock_tab_drag_origins_do_not_select_navigate_or_open() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for (button, navigation) in [
        (PointerButton::Primary, None),
        (PointerButton::Secondary, None),
        (PointerButton::Middle, None),
        (PointerButton::Primary, Some("navigation.pan")),
        (PointerButton::Primary, Some("navigation.orbit")),
    ] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        let start = s
            .trace
            .get(Control::AnimationPanelToggle)
            .unwrap()
            .rect
            .center();
        let end = s.state.viewport_ui_rect.center();
        let camera = pose(&s);
        let document = s.state.editor.document.clone();
        let selection = s.state.editor.selected_objects.clone();
        move_to(&mut s, start);
        if let Some(binding) = navigation {
            s.shortcut_down(binding).unwrap();
        }
        let modifiers = s.input.modifiers();
        s.frame(
            vec![pointer(start, button, true, modifiers)],
            Duration::ZERO,
        )
        .unwrap();
        move_to(&mut s, start.lerp(end, 0.5));
        assert_eq!(
            pose(&s),
            camera,
            "{button:?} {navigation:?} leaked during drag"
        );
        assert!(!s.state.editor.is_pointer_interacting());
        move_to(&mut s, end);
        s.frame(vec![pointer(end, button, false, modifiers)], Duration::ZERO)
            .unwrap();
        if let Some(binding) = navigation {
            s.shortcut_up(binding).unwrap();
        }
        s.settle().unwrap();
        assert_eq!(pose(&s), camera, "{button:?} {navigation:?}");
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.editor.selected_objects, selection);
        assert!(
            !s.state.animation_panel_is_open(),
            "A drag is not a tab activation"
        );
        assert!(s.trace.get(Control::ViewportMenu).is_err());
        assert!(!s.state.mouse_navigation_active());
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
    }
}

#[test]
fn floating_tool_dock_tab_rejects_native_camera_gestures() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.hover(Control::AnimationPanelToggle).unwrap();
    let camera = pose(&s);
    for event in [
        NavigationEvent::Wheel {
            delta: egui::vec2(0.0, 2.0),
            modifiers: Modifiers::NONE,
        },
        NavigationEvent::TrackpadScroll {
            delta: egui::vec2(25.0, -15.0),
            phase: ScrollPhase::Started,
            modifiers: Modifiers::NONE,
        },
        NavigationEvent::TrackpadScroll {
            delta: egui::Vec2::ZERO,
            phase: ScrollPhase::Ended,
            modifiers: Modifiers::NONE,
        },
        NavigationEvent::Pinch {
            delta: 0.25,
            modifiers: Modifiers::NONE,
        },
        NavigationEvent::Rotate {
            degrees: 20.0,
            modifiers: Modifiers::NONE,
        },
    ] {
        s.navigation(event, Duration::ZERO).unwrap();
        assert_eq!(pose(&s), camera, "{event:?} leaked through the tab bar");
    }
    assert!(!s.state.animation_panel_is_open());
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn viewport_camera_drag_keeps_ownership_when_crossing_floating_tool_dock_tabs() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for (button, navigation) in [
        (PointerButton::Secondary, None),
        (PointerButton::Middle, None),
        (PointerButton::Primary, Some("navigation.pan")),
        (PointerButton::Primary, Some("navigation.orbit")),
    ] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        let tab = s.trace.get(Control::AnimationPanelToggle).unwrap().rect;
        let start = egui::pos2(s.state.viewport_ui_rect.center().x, tab.center().y - 90.0);
        let inside = tab.center() - egui::vec2(8.0, 0.0);
        let next = tab.center() + egui::vec2(8.0, 0.0);
        let selection = s.state.editor.selected_objects.clone();
        let camera = pose(&s);
        move_to(&mut s, start);
        if let Some(binding) = navigation {
            s.shortcut_down(binding).unwrap();
        }
        let modifiers = s.input.modifiers();
        s.frame(
            vec![pointer(start, button, true, modifiers)],
            Duration::ZERO,
        )
        .unwrap();
        move_to(&mut s, inside);
        let crossed = pose(&s);
        assert_ne!(crossed, camera, "Camera gesture must start in the viewport");
        move_to(&mut s, next);
        assert_ne!(
            pose(&s),
            crossed,
            "{button:?} {navigation:?} stopped over the tab"
        );
        s.frame(
            vec![pointer(next, button, false, modifiers)],
            Duration::ZERO,
        )
        .unwrap();
        if let Some(binding) = navigation {
            s.shortcut_up(binding).unwrap();
        }
        s.settle().unwrap();
        assert!(
            !s.state.animation_panel_is_open(),
            "A viewport drag cannot activate a tab on release"
        );
        assert!(!s.state.mouse_navigation_active());
        assert_eq!(s.state.editor.selected_objects, selection);
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
    }
}

#[test]
fn newly_shown_floating_tool_dock_tabs_exclude_press_before_hit_geometry_settles() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let start = s
        .trace
        .get(Control::AnimationPanelToggle)
        .unwrap()
        .rect
        .center();
    let selection = s.state.editor.selected_objects.clone();
    s.shortcut("ui.toggle").unwrap();
    assert!(!s.state.show_ui);
    // Show UI through the real command, then immediately press at its tab on
    // the first visible layout. No settling frames can supply old area hits.
    s.frame(
        vec![s.shortcut_event("ui.toggle", true).unwrap()],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.show_ui);
    s.frame(
        vec![
            s.shortcut_event("ui.toggle", false).unwrap(),
            Event::PointerMoved(start),
            pointer(start, PointerButton::Primary, true, Modifiers::NONE),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(
        s.state
            .tool_dock
            .floating_tabs_rect
            .unwrap()
            .contains(start)
    );
    assert!(
        !s.state.editor.is_pointer_interacting(),
        "First tab press started an underlying marquee"
    );
    let camera = pose(&s);
    let end = s.state.viewport_ui_rect.center();
    move_to(&mut s, end);
    s.frame(
        vec![pointer(end, PointerButton::Primary, false, Modifiers::NONE)],
        Duration::ZERO,
    )
    .unwrap();
    s.settle().unwrap();
    assert_eq!(pose(&s), camera);
    assert_eq!(s.state.editor.selected_objects, selection);
    assert!(!s.state.animation_panel_is_open());
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}
