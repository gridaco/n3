//! Timeline input reaches the production host without becoming an authored edit.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{
    document::{AssetInstance, Document, Geometry, Object, Primitive, PrimitiveKind},
    scene::{SceneAsset, test_support},
    scene_view::Action,
    ui::timeline::Target,
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

fn setup(s: &mut Session<'_>) {
    s.load_scene_fixture("SimpleSkin/glTF/SimpleSkin.gltf")
        .unwrap();
    assert!(!s.state.animation_panel_is_open());
    assert!(s.trace.get(Control::AnimationTimeline).is_err());
    s.click(Control::AnimationPanelToggle).unwrap();
    // Mark this in-memory fixture saved so a playback-only dirty transition
    // cannot hide behind the ordinary dirty state of an imported document.
    s.state.mark_saved("timeline.n3.json".into(), Vec::new());
    s.settle().unwrap();
}

fn ruler_point(s: &Session<'_>, fraction: f32) -> Pos2 {
    let rect = s.trace.get(Control::AnimationRuler).unwrap().rect;
    egui::pos2(egui::lerp(rect.x_range(), fraction), rect.center().y)
}

fn move_pointer(s: &mut Session<'_>, pos: Pos2) {
    s.frame(vec![Event::PointerMoved(pos)], Duration::ZERO)
        .unwrap();
}

fn begin_scrub(s: &mut Session<'_>, fraction: f32) {
    let initial = s.trace.get(Control::AnimationRuler).unwrap().rect;
    move_pointer(s, ruler_point(s, fraction));
    let before = s.trace.get(Control::AnimationRuler).unwrap().rect;
    s.pointer_button(PointerButton::Primary, true).unwrap();
    assert!(
        s.state.animation.timeline.is_scrubbing(),
        "Scrub ended after press/settle: ruler {initial:?} -> pointer {before:?} -> {:?}, playback {:?}, error {:?}, focus {:?}",
        s.trace.get(Control::AnimationRuler).unwrap().rect,
        s.state.selected_asset_view().unwrap().playback,
        s.state.error,
        s.ctx.memory(|m| m.focused())
    );
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
}

fn release(s: &mut Session<'_>) {
    s.pointer_button(PointerButton::Primary, false).unwrap();
}

fn assert_panel_tab_placement(s: &Session<'_>, open: bool) {
    let tabs = s.trace.get(Control::ToolDockTabBar).unwrap().rect;
    let tab = s.trace.get(Control::AnimationPanelToggle).unwrap().rect;
    let viewport = s.trace.get(Control::Viewport).unwrap().rect;
    let status = s.trace.get(Control::StatusBar).unwrap().rect;
    assert!(tabs.contains_rect(tab));
    assert!(tabs.bottom() <= status.top());
    if open {
        let panel = s.trace.get(Control::AnimationTimeline).unwrap().rect;
        let close = s.trace.get(Control::ToolDockClose).unwrap().rect;
        assert!(panel.contains_rect(tabs));
        assert!(tabs.contains_rect(close));
        assert!(tabs.top() < panel.center().y);
        assert!(viewport.bottom() <= tabs.top());
    } else {
        let stats = s.trace.get(Control::SceneInfo).unwrap().rect;
        assert!(viewport.contains_rect(tabs));
        assert!(tabs.left() < viewport.center().x);
        assert!(stats.bottom() <= tabs.top());
        assert!(s.trace.get(Control::ToolDockClose).is_err());
    }
}

#[test]
fn animation_tab_moves_between_floating_and_docked_header_and_reactivation_only_focuses() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    assert_panel_tab_placement(&s, true);
    s.click(Control::ToolDockClose).unwrap();
    assert!(!s.state.animation_panel_is_open());
    assert_panel_tab_placement(&s, false);
    s.click(Control::AnimationPanelToggle).unwrap();
    assert!(s.state.animation_panel_is_open());
    assert_panel_tab_placement(&s, true);
    begin_scrub(&mut s, 0.6);
    release(&mut s);
    let accepted = s.state.selected_asset_view().unwrap().clone();
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    let canvas = s
        .state
        .animation
        .observations
        .iter()
        .find(|o| o.target == Target::Canvas)
        .unwrap()
        .id;
    // Use the exact 1x speed field to transfer native text focus. The time
    // field commits its displayed precision on blur, independently of tabs.
    s.click(Control::SceneSpeed).unwrap();
    assert!(s.ctx.text_edit_focused());
    assert_ne!(s.ctx.memory(|memory| memory.focused()), Some(canvas));
    s.extra_layout_pass = true;
    s.click(Control::AnimationPanelToggle).unwrap();
    assert!(
        s.state.animation_panel_is_open(),
        "A selected tab is activation, not a visibility toggle"
    );
    assert_panel_tab_placement(&s, true);
    assert_eq!(s.ctx.memory(|memory| memory.focused()), Some(canvas));
    let reactivated = s.state.selected_asset_view().unwrap();
    assert_eq!(reactivated.playback, accepted.playback);
    assert_eq!(reactivated.revision, accepted.revision);
    assert!(Arc::ptr_eq(&reactivated.frame, &accepted.frame));
    assert!(!s.state.animation.timeline.has_pointer_gesture());
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.selected_objects, selection);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
    s.shortcut("animation.play-pause").unwrap();
    assert!(s.state.selected_asset_view().unwrap().playback.playing);
    assert!(!s.state.hand_tool_active());
    s.shortcut("animation.play-pause").unwrap();
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
    s.click(Control::ToolDockClose).unwrap();
    assert!(!s.state.animation_panel_is_open());
    assert_panel_tab_placement(&s, false);
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn panel_tab_chrome_keeps_geometry_when_hovered_and_pressed_in_both_themes() {
    fn geometry(s: &Session<'_>) -> Vec<(Control, egui::Rect)> {
        [
            Control::Viewport,
            Control::StatusBar,
            Control::ToolDockTabBar,
            Control::AnimationPanelToggle,
            Control::AnimationTimeline,
            Control::ToolDockClose,
        ]
        .into_iter()
        .filter_map(|control| s.trace.get(control).ok().map(|o| (control, o.rect)))
        .collect()
    }

    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut failures = Vec::new();
    for theme in [
        crate::settings::ThemeMode::Light,
        crate::settings::ThemeMode::Dark,
    ] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        s.state.theme_mode = theme;
        s.settle().unwrap();
        s.click(Control::ToolDockClose).unwrap();
        let document = s.state.editor.document.clone();
        let selection = s.state.editor.selected_objects.clone();
        let accepted = s.state.selected_asset_view().unwrap().clone();
        let mut floating_tab_height = None;
        for open in [false, true] {
            if open {
                s.click(Control::AnimationPanelToggle).unwrap();
            }
            let empty = s.empty_viewport_point().unwrap();
            move_pointer(&mut s, empty);
            s.settle().unwrap();
            let camera = s.state.camera.view_projection(s.state.aspect());
            let baseline = geometry(&s);
            let floating_bounds = s.state.tool_dock.floating_tabs_rect;
            let tab = s.trace.get(Control::AnimationPanelToggle).unwrap().rect;
            if open {
                let close = s.trace.get(Control::ToolDockClose).unwrap().rect;
                let bar = s.trace.get(Control::ToolDockTabBar).unwrap().rect;
                if tab.height() != close.height() || tab.center().y != close.center().y {
                    failures.push(format!(
                        "{theme:?} docked tab/close alignment: tab {tab:?}, close {close:?}"
                    ));
                }
                if tab.center().y != bar.center().y {
                    failures.push(format!(
                        "{theme:?} docked tab is not centered in its bar: {tab:?}, {bar:?}"
                    ));
                }
                if Some(tab.height()) != floating_tab_height {
                    failures.push(format!(
                        "{theme:?} tab height changed with placement: {floating_tab_height:?} -> {}",
                        tab.height()
                    ));
                }
            } else {
                floating_tab_height = Some(tab.height());
            }
            let targets = if open {
                &[Control::AnimationPanelToggle, Control::ToolDockClose][..]
            } else {
                &[Control::AnimationPanelToggle][..]
            };
            for &target in targets {
                let focus_before_hover = s.ctx.memory(|memory| memory.focused());
                s.hover(target).unwrap();
                assert_eq!(s.ctx.memory(|memory| memory.focused()), focus_before_hover);
                for pressed in [false, true] {
                    if pressed {
                        s.pointer_button(PointerButton::Primary, true).unwrap();
                    }
                    let current = geometry(&s);
                    if current != baseline
                        || s.state.tool_dock.floating_tabs_rect != floating_bounds
                    {
                        failures.push(format!(
                            "{theme:?} open={open}, {target:?}, pressed={pressed}:\n  before {baseline:?}, floating {floating_bounds:?}\n  after {current:?}, floating {:?}",
                            s.state.tool_dock.floating_tabs_rect
                        ));
                    }
                    assert_eq!(s.state.animation_panel_is_open(), open);
                    assert!(!s.state.animation.timeline.has_pointer_gesture());
                    assert!(!s.state.editor.is_interacting());
                }
                // Release away from the pressed button: geometry probes must
                // not activate a tab, dismiss its panel, or seek underneath it.
                move_pointer(&mut s, empty);
                release(&mut s);
                assert_eq!(s.state.animation_panel_is_open(), open);
                let view = s.state.selected_asset_view().unwrap();
                assert_eq!(view.playback, accepted.playback);
                assert_eq!(view.revision, accepted.revision);
                assert!(Arc::ptr_eq(&view.frame, &accepted.frame));
                assert_eq!(s.state.editor.document, document);
                assert_eq!(s.state.editor.selected_objects, selection);
                assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
            }
        }
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn empty_animation_panel_close_is_unframed_at_rest_and_filled_on_hover() {
    fn pixel(frame: &super::capture::CapturedFrame, point: Pos2) -> [u8; 4] {
        let index = (point.y as usize * frame.width as usize + point.x as usize) * 4;
        frame.rgba[index..index + 4].try_into().unwrap()
    }

    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for theme in [
        crate::settings::ThemeMode::Light,
        crate::settings::ThemeMode::Dark,
    ] {
        let mut s = Session::new(&mut capture).unwrap();
        s.state.theme_mode = theme;
        // The tutorial pointer is presentation only; hide its drawing when
        // sampling native widget pixels, while retaining real pointer events.
        s.show_inputs = false;
        s.state.mark_saved("empty.n3.json".into(), Vec::new());
        s.settle().unwrap();
        s.click(Control::AnimationPanelToggle).unwrap();
        assert_panel_tab_placement(&s, true);
        let empty = s.empty_viewport_point().unwrap();
        move_pointer(&mut s, empty);
        s.wait(Duration::from_millis(200)).unwrap();
        let tab = s.trace.get(Control::AnimationPanelToggle).unwrap().rect;
        let close = s.trace.get(Control::ToolDockClose).unwrap().rect;
        let bar = s.trace.get(Control::ToolDockTabBar).unwrap().rect;
        let panel = s.trace.get(Control::AnimationTimeline).unwrap().rect;
        let viewport = s.trace.get(Control::Viewport).unwrap().rect;
        assert_eq!(close.height(), tab.height());
        assert_eq!(close.width(), close.height());
        assert_eq!(close.center().y, tab.center().y);
        assert_eq!(tab.left() - panel.left(), panel.right() - close.right());
        let focus = s.ctx.memory(|memory| memory.focused());
        let document = s.state.editor.document.clone();
        let selection = s.state.editor.selected_objects.clone();
        let camera = s.state.camera.view_projection(s.state.aspect());
        let idle = s.capture.read_frame().unwrap();
        let outside = egui::pos2(close.left() - 3.0, close.center().y);
        // Sample both the edge and inner padding, outside the icon glyph and
        // the rounded corners. An idle frame or border would change these.
        for inset in [0.5, 2.5] {
            let point = egui::pos2(close.left() + inset, close.center().y);
            assert_eq!(
                pixel(&idle, point),
                pixel(&idle, outside),
                "{theme:?}: the idle close control paints a frame at inset {inset}"
            );
        }
        s.hover(Control::ToolDockClose).unwrap();
        s.wait(Duration::from_millis(200)).unwrap();
        let hovered = s.capture.read_frame().unwrap();
        let padding = egui::pos2(close.left() + 2.5, close.center().y);
        assert_ne!(
            pixel(&hovered, padding),
            pixel(&hovered, outside),
            "{theme:?}: hovering close must visibly fill its background"
        );
        assert_eq!(s.trace.get(Control::ToolDockClose).unwrap().rect, close);
        assert_eq!(s.trace.get(Control::ToolDockTabBar).unwrap().rect, bar);
        assert_eq!(s.trace.get(Control::AnimationTimeline).unwrap().rect, panel);
        assert_eq!(s.trace.get(Control::Viewport).unwrap().rect, viewport);
        assert_eq!(s.ctx.memory(|memory| memory.focused()), focus);
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.state.editor.selected_objects, selection);
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
        assert!(!s.state.animation.timeline.has_pointer_gesture());
        assert!(s.state.animation_panel_is_open());
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
    }
}

#[test]
fn animation_opens_on_demand_and_closing_preserves_accepted_playback() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    assert!(!s.state.animation_panel_is_open());
    assert!(s.trace.get(Control::AnimationTimeline).is_err());
    // An empty document still allows explicit opening; selection is not a
    // visibility policy and must not make the user's chosen panel disappear.
    s.click(Control::AnimationPanelToggle).unwrap();
    assert!(s.state.animation_panel_is_open());
    assert!(s.trace.get(Control::AnimationTimeline).is_ok());
    s.click(Control::ToolDockClose).unwrap();
    setup(&mut s);
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    s.shortcut("animation.play-pause").unwrap();
    s.wait(Duration::from_millis(180)).unwrap();
    let playing = s.state.selected_asset_view().unwrap().clone();
    assert!(playing.playback.playing);
    s.extra_layout_pass = true;
    s.click(Control::ToolDockClose).unwrap();
    assert!(!s.state.animation_panel_is_open());
    assert!(s.trace.get(Control::AnimationTimeline).is_err());
    assert_eq!(
        s.ctx.memory(|m| m.focused()),
        Some(crate::shortcuts::viewport_focus_id())
    );
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        playing.playback
    );
    assert!(Arc::ptr_eq(
        &s.state.selected_asset_view().unwrap().frame,
        &playing.frame
    ));
    s.wait(Duration::from_millis(150)).unwrap();
    assert!(s.state.selected_asset_view().unwrap().playback.position > playing.playback.position);
    s.click(Control::AnimationPanelToggle).unwrap();
    assert!(s.state.animation_panel_is_open());
    s.shortcut("animation.play-pause").unwrap();
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.selected_objects, selection);
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn timeline_space_is_focus_local_and_toggles_once_per_fresh_press() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let document = s.state.editor.document.clone();
    let objects = s.state.editor.selected_objects.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    let canvas = s
        .state
        .animation
        .observations
        .iter()
        .find(|o| o.target == Target::Canvas)
        .unwrap()
        .id;
    assert_eq!(s.ctx.memory(|m| m.focused()), Some(canvas));
    s.extra_layout_pass = true;
    s.shortcut_down("animation.play-pause").unwrap();
    assert!(s.state.selected_asset_view().unwrap().playback.playing);
    assert!(!s.state.hand_tool_active());
    let mut repeat = s.shortcut_event("animation.play-pause", true).unwrap();
    if let Event::Key { repeat, .. } = &mut repeat {
        *repeat = true;
    }
    s.frame(vec![repeat], Duration::ZERO).unwrap();
    assert!(s.state.selected_asset_view().unwrap().playback.playing);
    s.shortcut_up("animation.play-pause").unwrap();
    assert!(s.state.selected_asset_view().unwrap().playback.playing);
    s.shortcut("animation.play-pause").unwrap();
    assert!(
        !s.state
            .asset_views
            .values()
            .next()
            .unwrap()
            .playback
            .playing
    );
    assert_eq!(s.state.editor.selected_objects, objects);
    // Hovering the timeline is insufficient: the viewport keeps its held tool
    // while focused, and moving over the panel cannot steal keyboard intent.
    s.click(Control::Viewport).unwrap();
    let selection_after_click = s.state.editor.selected_objects.clone();
    s.hover(Control::AnimationTimeline).unwrap();
    s.shortcut_down("navigation.pan").unwrap();
    assert!(s.state.hand_tool_active());
    assert!(
        !s.state
            .asset_views
            .values()
            .next()
            .unwrap()
            .playback
            .playing
    );
    s.shortcut_up("navigation.pan").unwrap();
    assert!(!s.state.hand_tool_active());
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.selected_objects, selection_after_click);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn timeline_box_owns_release_and_escape_without_seeking_or_editing_the_document() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let keys: Vec<_> = s
        .state
        .animation
        .observations
        .iter()
        .filter(|o| matches!(o.target, Target::Key { .. }))
        .map(|o| o.rect)
        .collect();
    assert!(keys.len() >= 3);
    s.click_at(keys[2].center()).unwrap();
    let selection = s.state.animation.timeline.selection().cloned();
    let before = s.state.selected_asset_view().unwrap().clone();
    let document = s.state.editor.document.clone();
    let objects = s.state.editor.selected_objects.clone();
    let tool = s.state.editor.tool;
    let start = keys[0].left_top() - egui::vec2(10., 5.);
    let end = keys[1].right_bottom() + egui::vec2(10., 5.);
    move_pointer(&mut s, start);
    s.pointer_button(PointerButton::Primary, true).unwrap();
    assert!(s.state.animation.timeline.has_pointer_gesture());
    assert!(!s.state.animation.timeline.is_marquee_active());
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        before.playback
    );
    move_pointer(&mut s, end);
    assert!(s.state.animation.timeline.is_marquee_active());
    assert_eq!(s.state.animation.timeline.selection(), selection.as_ref());
    // A viewport tool key cannot steal the held timeline gesture.
    s.shortcut("tool.rotate").unwrap();
    s.shortcut("animation.play-pause").unwrap();
    assert_eq!(s.state.editor.tool, tool);
    s.extra_layout_pass = true;
    s.shortcut("cancel").unwrap();
    release(&mut s);
    assert!(!s.state.animation.timeline.has_pointer_gesture());
    assert_eq!(s.state.animation.timeline.selection(), selection.as_ref());
    assert_eq!(s.state.editor.selected_objects, objects);
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        before.playback
    );
    assert!(Arc::ptr_eq(
        &before.frame,
        &s.state.selected_asset_view().unwrap().frame
    ));
    assert_eq!(s.state.editor.document, document);
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn leaving_or_hiding_animation_panel_cancels_box_even_before_the_drag_threshold() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for (hide, drag) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut s = Session::new(&mut capture).unwrap();
        let sources = mixed_setup(&mut s);
        let before = s.state.asset_views[&sources[0]].clone();
        let key = s
            .state
            .animation
            .observations
            .iter()
            .find(|o| matches!(o.target, Target::Key { .. }))
            .unwrap()
            .rect;
        let start = key.left_top() - egui::vec2(10., 5.);
        move_pointer(&mut s, start);
        s.pointer_button(PointerButton::Primary, true).unwrap();
        if drag {
            move_pointer(&mut s, key.right_bottom() + egui::vec2(10., 5.));
        }
        assert!(s.state.animation.timeline.has_pointer_gesture());
        if hide {
            s.state.show_ui = false;
        } else {
            s.state.editor.select_object(3).unwrap();
        }
        s.settle().unwrap();
        assert!(!s.state.animation.timeline.has_pointer_gesture());
        release(&mut s);
        let after = &s.state.asset_views[&sources[0]];
        assert_eq!(after.playback, before.playback);
        assert!(Arc::ptr_eq(&after.frame, &before.frame));
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
    }
}

#[test]
fn closing_during_scrub_restores_baseline_and_drops_the_late_release() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.shortcut("animation.play-pause").unwrap();
    s.wait(Duration::from_millis(200)).unwrap();
    let baseline = s.state.selected_asset_view().unwrap().clone();
    begin_scrub(&mut s, 0.75);
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
    // A host request can close a panel while its pointer is held. The same
    // semantic command used by both explicit visibility controls owns recovery.
    s.state.dispatch(
        crate::shortcuts::Command::ToggleAnimationPanel,
        &s.ctx,
        false,
    );
    s.settle().unwrap();
    release(&mut s);
    assert!(!s.state.animation_panel_is_open());
    assert!(!s.state.animation.timeline.has_pointer_gesture());
    let restored = s.state.selected_asset_view().unwrap();
    assert_eq!(restored.playback, baseline.playback);
    assert!(Arc::ptr_eq(&restored.frame, &baseline.frame));
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn pending_viewport_transform_keeps_ownership_when_pointer_drags_over_timeline() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.ctx
        .memory_mut(|m| m.request_focus(crate::shortcuts::viewport_focus_id()));
    s.hover(Control::Viewport).unwrap();
    s.shortcut("tool.move").unwrap();
    s.shortcut("transform.axis-x").unwrap();
    let baseline = s.state.editor.document.clone();
    // A released key leaves a transaction pending without an egui drag owner.
    s.shortcut("nudge.right").unwrap();
    assert!(s.state.editor.has_transform_session());
    assert!(!s.state.editor.is_pointer_interacting());
    assert!(!s.trace.get(Control::AnimationPanelToggle).unwrap().enabled);
    assert!(!s.trace.get(Control::ToolDockClose).unwrap().enabled);
    let preview = s.state.editor.document.clone();
    assert_ne!(preview, baseline);
    let focus = s.ctx.memory(|m| m.focused());
    let key = s
        .state
        .animation
        .observations
        .iter()
        .find(|o| matches!(o.target, Target::Key { .. }))
        .unwrap()
        .rect;
    move_pointer(&mut s, key.left_top() - egui::vec2(10., 5.));
    s.pointer_button(PointerButton::Primary, true).unwrap();
    move_pointer(&mut s, key.right_bottom() + egui::vec2(10., 5.));
    assert!(!s.state.animation.timeline.has_pointer_gesture());
    assert!(s.state.animation.timeline.selection().is_none());
    release(&mut s);
    assert_eq!(s.ctx.memory(|m| m.focused()), focus);
    assert_eq!(s.state.editor.document, preview);
    s.shortcut("cancel").unwrap();
    assert_eq!(s.state.editor.document, baseline);
    assert!(!s.state.editor.has_transform_session());
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn scrub_previews_on_press_and_motion_then_ends_paused_without_history() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let document = s.state.editor.document.clone();
    let rest = s.state.selected_asset_view().unwrap().frame.clone();
    begin_scrub(&mut s, 0.25);
    let first = s.state.selected_asset_view().unwrap().clone();
    assert!(first.playback.position > 0.);
    assert!(first.playback.clip.is_some());
    assert!(!Arc::ptr_eq(&rest, &first.frame));
    let point = ruler_point(&s, 0.65);
    move_pointer(&mut s, point);
    let second = s.state.selected_asset_view().unwrap().clone();
    assert!(second.playback.position > first.playback.position);
    assert_ne!(second.frame.node_world, first.frame.node_world);
    s.extra_layout_pass = true;
    release(&mut s);
    assert!(!s.state.animation.timeline.is_scrubbing());
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        second.playback
    );
    let frame = s.state.selected_asset_view().unwrap().frame.clone();
    s.wait(Duration::from_millis(250)).unwrap();
    assert!(Arc::ptr_eq(
        &frame,
        &s.state.selected_asset_view().unwrap().frame
    ));
    assert_eq!(s.state.editor.document, document);
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn sustained_scrub_paints_published_time_in_one_pass_without_repeating_evaluation() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    begin_scrub(&mut s, 0.25);
    let document = s.state.editor.document.clone();
    let context = s.ctx.clone();
    let mut pass_counts = Vec::new();
    let mut reasons = Vec::new();
    let mut warnings = Vec::new();
    let mut changed_pose = false;
    // Exercise consecutive motion frames without settling in between. Empty
    // frames would reset egui's multipass warning and hide a one-frame lag.
    for step in 0..12 {
        let before = s.state.selected_asset_view().unwrap().clone();
        let point = ruler_point(&s, 0.30 + step as f32 * 0.04);
        let mut frame_reasons = Vec::new();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(WIDTH as f32, HEIGHT as f32),
                )),
                time: Some(s.time + f64::from(step) / 60.0),
                events: vec![Event::PointerMoved(point)],
                ..Default::default()
            },
            |ui| {
                s.state.ui(ui);
                frame_reasons.extend(
                    ui.ctx()
                        .output(|output| output.request_discard_reasons.clone()),
                );
            },
        );
        pass_counts.push(output.platform_output.num_completed_passes);
        reasons.push(format!("{frame_reasons:?}"));
        let painted_text: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.clone()),
                _ => None,
            })
            .collect();
        warnings.extend(
            painted_text
                .iter()
                .filter(|text| text.contains("egui PERF WARNING"))
                .cloned(),
        );
        let accepted = s.state.selected_asset_view().unwrap();
        let ruler = s
            .state
            .animation
            .observations
            .iter()
            .find(|observation| observation.target == Target::Ruler)
            .unwrap()
            .rect;
        let range = s.state.animation.timeline.visible_range().unwrap();
        let expected_x = ruler.left()
            + ruler.width()
                * ((f64::from(accepted.playback.position) - range.start) / range.duration()) as f32;
        let has_accepted_playhead = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::LineSegment { points, .. }
                if (points[0].x - expected_x).abs() < 0.001
                    && points[0].x == points[1].x
                    && points[0].y == ruler.top()
                    && points[1].y > ruler.bottom())
        });
        // This test inspects final paint before GPU submission. Explicitly
        // discard the unused texture delta even if a later assertion fails.
        output.textures_delta.clear();
        assert!(
            !painted_text
                .iter()
                .any(|text| text.starts_with("Requested ")),
            "Accepted seeks must not paint a pending-request footer: {painted_text:?}"
        );
        assert!(painted_text.iter().any(|text| text == "Read-only tracks"));
        assert!(
            has_accepted_playhead,
            "Frame {step} must paint the published playhead at {expected_x}"
        );
        assert!(accepted.playback.position > before.playback.position);
        assert_eq!(accepted.revision, before.revision.wrapping_add(1));
        assert!(!Arc::ptr_eq(&accepted.frame, &before.frame));
        changed_pose |= accepted.frame.node_world != before.frame.node_world;
        assert!(s.state.animation.timeline.is_scrubbing());
        assert_eq!(s.state.editor.document, document);
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
    }
    assert!(
        changed_pose,
        "The repeated seeks must evaluate actual deformation"
    );
    assert!(
        pass_counts.iter().all(|&passes| passes == 1) && warnings.is_empty(),
        "Settled scrubbing must process input, accept, and paint once per frame. Passes: {pass_counts:?}; reasons: {reasons:?}; warnings: {warnings:?}"
    );
}

#[test]
fn escape_and_focus_loss_restore_the_exact_scrub_baseline() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for cancel in [
        Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        },
        Event::WindowFocused(false),
    ] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        s.click(Control::ScenePlay).unwrap();
        s.wait(Duration::from_millis(350)).unwrap();
        let baseline = s.state.selected_asset_view().unwrap().clone();
        let selected = s.state.editor.selected_objects.clone();
        let document = s.state.editor.document.clone();
        assert!(baseline.playback.playing);
        begin_scrub(&mut s, 0.8);
        assert_ne!(
            s.state.selected_asset_view().unwrap().playback.position,
            baseline.playback.position
        );
        s.frame(vec![cancel], Duration::ZERO).unwrap();
        s.settle().unwrap();
        assert!(!s.state.animation.timeline.is_scrubbing());
        let restored = s.state.selected_asset_view().unwrap();
        assert_eq!(restored.playback, baseline.playback);
        assert!(Arc::ptr_eq(&restored.frame, &baseline.frame));
        assert_eq!(
            s.state.editor.selected_objects, selected,
            "Escape belongs to the scrub, not viewport deselection"
        );
        assert_eq!(s.state.editor.document, document);
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
    }
}

#[test]
fn timeline_focus_and_drag_do_not_leak_viewport_shortcuts_or_navigation() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let document = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    let tool = s.state.editor.tool;
    let camera = s.state.camera.view_projection(s.state.aspect());
    begin_scrub(&mut s, 0.5);
    let viewport = s.trace.get(Control::Viewport).unwrap().rect.center();
    move_pointer(&mut s, viewport);
    assert!(s.state.animation.timeline.is_scrubbing());
    for shortcut in [
        "selection.delete",
        "tool.rotate",
        "view.front",
        "edit.confirm",
        "animation.play-pause",
    ] {
        s.shortcut(shortcut).unwrap();
        assert_eq!(s.state.editor.document, document, "{shortcut} during scrub");
        assert_eq!(s.state.editor.tool, tool, "{shortcut} during scrub");
        assert_eq!(
            s.state.editor.selected_objects, selected,
            "{shortcut} during scrub"
        );
        assert!(!s.state.editor.edit_mode);
        assert!(!s.state.selected_asset_view().unwrap().playback.playing);
    }
    s.scroll(14., -12., true, Modifiers::NONE).unwrap();
    s.pinch(0.2).unwrap();
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
    release(&mut s);
    assert!(!s.state.animation.timeline.is_scrubbing());
    for shortcut in ["selection.delete", "tool.rotate", "view.front"] {
        s.shortcut(shortcut).unwrap();
        assert_eq!(
            s.state.editor.document, document,
            "{shortcut} with timeline focus"
        );
        assert_eq!(s.state.editor.tool, tool, "{shortcut} with timeline focus");
    }
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
    assert!(!s.state.is_dirty());
}

fn mixed_setup(s: &mut Session<'_>) -> [AssetInstance; 2] {
    let first = test_support::animated_asset();
    let mut data = (*first).clone();
    let mut second_clip = data.animations[0].clone();
    second_clip.name = "Alternate clip".into();
    second_clip.channels[0].values[3] = 50.;
    data.animations.push(second_clip);
    let asset = Arc::new(SceneAsset::new(data).unwrap());
    let keys = ["first-source", "second-source"].map(|source| AssetInstance {
        source: source.into(),
        scene: 0,
    });
    let document = Document {
        objects: vec![
            Object {
                id: 1,
                name: "First".into(),
                transform: Default::default(),
                geometry: Geometry::Asset(keys[0].clone()),
            },
            Object {
                id: 2,
                name: "Second".into(),
                transform: Default::default(),
                geometry: Geometry::Asset(keys[1].clone()),
            },
            Object {
                id: 3,
                name: "Native".into(),
                transform: Default::default(),
                geometry: Geometry::Primitive(Primitive::new(PrimitiveKind::Cube)),
            },
        ],
        ..Default::default()
    };
    s.state
        .install_loaded_document(
            "mixed.n3.json".into(),
            crate::asset_io::LoadedDocument {
                document,
                assets: BTreeMap::from([
                    (keys[0].source.clone(), asset.clone()),
                    (keys[1].source.clone(), asset),
                ]),
                diagnostics: Vec::new(),
                saved_bytes: Some(Vec::new()),
            },
        )
        .unwrap();
    s.state.editor.select_object(1).unwrap();
    s.settle().unwrap();
    s.click(Control::AnimationPanelToggle).unwrap();
    keys
}

#[test]
fn source_or_native_selection_change_cancels_the_old_scrub() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for selected in [2, 3] {
        let mut s = Session::new(&mut capture).unwrap();
        let keys = mixed_setup(&mut s);
        let baseline = s.state.asset_views[&keys[0]].clone();
        begin_scrub(&mut s, 0.7);
        // Selection can be changed by the host while a pointer gesture is held.
        // It must cancel the old source rather than applying its release to the new one.
        s.state.editor.select_object(selected).unwrap();
        s.settle().unwrap();
        assert!(!s.state.animation.timeline.is_scrubbing());
        let restored = &s.state.asset_views[&keys[0]];
        assert_eq!(restored.playback, baseline.playback);
        assert!(Arc::ptr_eq(&restored.frame, &baseline.frame));
        let second = s.state.asset_views[&keys[1]].clone();
        release(&mut s);
        assert_eq!(s.state.asset_views[&keys[1]].playback, second.playback);
        assert!(Arc::ptr_eq(
            &s.state.asset_views[&keys[1]].frame,
            &second.frame
        ));
        assert!(!s.state.is_dirty());
        if selected == 3 {
            assert!(s.state.animation_panel_is_open());
            assert!(s.trace.get(Control::AnimationTimeline).is_ok());
            assert!(s.trace.get(Control::ScenePlay).is_err());
            assert!(s.state.animation.observations.is_empty());
        }
    }
}

#[test]
fn immutable_track_data_survives_playback_and_duplicate_placements_share_transport() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let original = s.state.editor.selected_object.unwrap();
    let data = s.state.animation.data.clone();
    s.click(Control::ScenePlay).unwrap();
    s.wait(Duration::from_millis(220)).unwrap();
    assert!(Arc::ptr_eq(&data, &s.state.animation.data));
    s.click(Control::ScenePlay).unwrap();
    let playback = s.state.selected_asset_view().unwrap().playback.clone();
    s.ctx
        .memory_mut(|memory| memory.request_focus(crate::shortcuts::viewport_focus_id()));
    s.hover(Control::Viewport).unwrap();
    s.shortcut("selection.duplicate").unwrap();
    assert_eq!(s.state.editor.document.objects.len(), 2);
    assert_eq!(s.state.asset_views.len(), 1);
    assert_eq!(s.state.selected_asset_view().unwrap().playback, playback);
    assert!(Arc::ptr_eq(&data, &s.state.animation.data));
    begin_scrub(&mut s, 0.75);
    release(&mut s);
    let changed = s.state.selected_asset_view().unwrap().clone();
    s.state.editor.select_object(original).unwrap();
    s.settle().unwrap();
    let shared = s.state.selected_asset_view().unwrap();
    assert_eq!(shared.playback, changed.playback);
    assert!(Arc::ptr_eq(&shared.frame, &changed.frame));
    assert!(Arc::ptr_eq(&data, &s.state.animation.data));
    assert!(s.state.editor.undo(), "Only duplication created history");
    assert_eq!(s.state.editor.document.objects.len(), 1);
    assert!(!s.state.editor.undo());
}

#[test]
fn distinct_sources_and_clips_reset_inspection_identity_without_rebuilding_on_seek() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    let keys = mixed_setup(&mut s);
    let first_data = s.state.animation.data.clone();
    let key = s
        .state
        .animation
        .observations
        .iter()
        .find(|o| matches!(o.target, Target::Key { .. }))
        .unwrap()
        .rect
        .center();
    s.click_at(key).unwrap();
    assert!(s.state.animation.timeline.selection().is_some());
    s.state.editor.select_object(2).unwrap();
    s.settle().unwrap();
    assert!(!Arc::ptr_eq(&first_data, &s.state.animation.data));
    assert!(s.state.animation.timeline.selection().is_none());
    let second_data = s.state.animation.data.clone();
    begin_scrub(&mut s, 0.6);
    release(&mut s);
    assert!(Arc::ptr_eq(&second_data, &s.state.animation.data));
    assert_eq!(s.state.asset_views[&keys[0]].playback.clip, None);
    assert!(s.state.asset_views[&keys[1]].playback.clip.is_some());
    // Host-driven clip replacement exercises the same invalidation boundary
    // independently of the dropdown's presentation.
    s.state
        .asset_views
        .get_mut(&keys[1])
        .unwrap()
        .apply(Action::Clip(Some(1)))
        .unwrap();
    s.settle().unwrap();
    assert!(!Arc::ptr_eq(&second_data, &s.state.animation.data));
    assert_eq!(
        s.state
            .animation
            .data
            .tracks
            .iter()
            .flat_map(|t| &t.keys)
            .count(),
        2
    );
    assert!(!s.state.is_dirty());
}

#[test]
fn clip_command_during_scrub_cancels_before_publishing_the_new_clip() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    mixed_setup(&mut s);
    begin_scrub(&mut s, 0.65);
    s.state.dispatch(
        crate::shortcuts::Command::Scene(Action::Clip(Some(1))),
        &s.ctx,
        false,
    );
    s.settle().unwrap();
    assert!(!s.state.animation.timeline.is_scrubbing());
    let changed = s.state.selected_asset_view().unwrap().clone();
    assert_eq!(changed.playback.clip, Some(1));
    assert_eq!(changed.playback.position, 0.);
    assert!(!changed.playback.playing);
    release(&mut s);
    let after_release = s.state.selected_asset_view().unwrap();
    assert_eq!(after_release.playback, changed.playback);
    assert!(Arc::ptr_eq(&after_release.frame, &changed.frame));
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn rejected_speed_change_keeps_the_accepted_playback_and_pose() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.click(Control::ScenePlay).unwrap();
    s.wait(Duration::from_millis(200)).unwrap();
    let baseline = s.state.selected_asset_view().unwrap().clone();
    // One large native DragValue change exceeds the host's supported 4x speed.
    // Rejecting that preference must not turn into a playback failure/pause.
    s.hover(Control::SceneSpeed).unwrap();
    let start = s.input.position().unwrap();
    s.pointer_button(PointerButton::Primary, true).unwrap();
    move_pointer(&mut s, start + egui::vec2(600., 0.));
    release(&mut s);
    assert!(
        s.state
            .error
            .as_deref()
            .is_some_and(|error| error.contains("speed"))
    );
    let retained = s.state.selected_asset_view().unwrap();
    assert_eq!(retained.playback, baseline.playback);
    assert!(Arc::ptr_eq(&retained.frame, &baseline.frame));
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn preferences_opened_from_timeline_owns_space_and_escape_returns_to_viewport() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    let before = s.state.selected_asset_view().unwrap().clone();
    s.shortcut("preferences.open").unwrap();
    assert!(s.state.show_preferences);
    s.shortcut("animation.play-pause").unwrap();
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        before.playback
    );
    assert!(!s.state.hand_tool_active());
    s.extra_layout_pass = true;
    s.shortcut("cancel").unwrap();
    assert!(!s.state.show_preferences);
    assert!(s.trace.get(Control::PreferencesWindow).is_err());
    assert_eq!(
        s.ctx.memory(|memory| memory.focused()),
        Some(crate::shortcuts::viewport_focus_id())
    );
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        before.playback
    );
    assert!(Arc::ptr_eq(
        &s.state.selected_asset_view().unwrap().frame,
        &before.frame
    ));
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.selected_objects, selection);
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), camera);
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}

#[test]
fn overlapping_preferences_control_keeps_focus_out_of_the_timeline_underneath() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    // Enlarge the real panel, then move the real Preferences window over it.
    // The overlap is asserted rather than assuming a particular screen layout.
    let panel = s.trace.get(Control::AnimationTimeline).unwrap().rect;
    s.drag_at(
        panel.center_top() + egui::vec2(0., 1.),
        egui::vec2(0., -350.),
    )
    .unwrap();
    let before_preferences = s.ctx.memory(|memory| memory.focused());
    s.shortcut("preferences.open").unwrap();
    s.wait(Duration::from_millis(250)).unwrap();
    let after_preferences = s.ctx.memory(|memory| memory.focused());
    let before_preferences_space = s.state.selected_asset_view().unwrap().clone();
    s.shortcut("animation.play-pause").unwrap();
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        before_preferences_space.playback,
        "Opening Preferences must move keyboard ownership away from the underlying timeline"
    );
    assert!(!s.state.hand_tool_active());
    let canvas_before_move = s
        .state
        .animation
        .observations
        .iter()
        .find(|observation| observation.target == Target::Canvas)
        .unwrap()
        .rect;
    let grid_before_move = s.trace.get(Control::Grid).unwrap().rect.center();
    let target_y = egui::lerp(canvas_before_move.y_range(), 0.1);
    s.drag(
        Control::PreferencesTitle,
        egui::vec2(120., target_y - grid_before_move.y),
    )
    .unwrap();
    let canvas = s
        .state
        .animation
        .observations
        .iter()
        .find(|observation| observation.target == Target::Canvas)
        .unwrap()
        .clone();
    let control = s.trace.get(Control::Grid).unwrap().rect.center();
    assert!(
        canvas.rect.contains(control),
        "Grid preference {control:?} must overlap timeline {:?}",
        canvas.rect
    );
    let baseline = s.state.selected_asset_view().unwrap().clone();
    let grid = s.state.show_grid;
    move_pointer(&mut s, control);
    let before_grid = s.ctx.memory(|memory| memory.focused());
    s.pointer_button(PointerButton::Primary, true).unwrap();
    let after_grid_down = s.ctx.memory(|memory| memory.focused());
    let gesture_on_down = s.state.animation.timeline.has_pointer_gesture();
    assert!(!s.state.animation.timeline.is_scrubbing());
    release(&mut s);
    let after_grid_up = s.ctx.memory(|memory| memory.focused());
    assert_ne!(
        after_grid_down,
        Some(canvas.id),
        "Covered timeline must not retain or take focus on Preferences pointer-down: canvas={:?}, before Preferences={before_preferences:?}, after Preferences={after_preferences:?}, before Grid={before_grid:?}, after Grid down={after_grid_down:?}, after Grid release={after_grid_up:?}, timeline gesture on down={gesture_on_down}, Grid changed={}",
        canvas.id,
        s.state.show_grid != grid,
    );
    assert_ne!(
        s.state.show_grid, grid,
        "The visible Preferences checkbox receives its click"
    );
    assert_ne!(s.ctx.memory(|memory| memory.focused()), Some(canvas.id));
    let retained = s.state.selected_asset_view().unwrap();
    assert_eq!(retained.playback, baseline.playback);
    assert!(Arc::ptr_eq(&retained.frame, &baseline.frame));
    assert!(!s.state.is_dirty());
}

#[test]
fn transport_time_field_keeps_text_focus_and_accepts_typed_seek() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let canvas = s
        .state
        .animation
        .observations
        .iter()
        .find(|observation| observation.target == Target::Canvas)
        .unwrap()
        .clone();
    let selected = s.state.editor.selected_objects.clone();
    s.click(Control::SceneTime).unwrap();
    assert!(
        s.ctx.text_edit_focused(),
        "Transport DragValue enters its native text editor"
    );
    assert_ne!(s.ctx.memory(|memory| memory.focused()), Some(canvas.id));
    let before_space = s.state.selected_asset_view().unwrap().clone();
    s.extra_layout_pass = true;
    s.shortcut("animation.play-pause").unwrap();
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback,
        before_space.playback
    );
    assert!(Arc::ptr_eq(
        &s.state.selected_asset_view().unwrap().frame,
        &before_space.frame
    ));
    assert!(!s.state.hand_tool_active());
    assert!(s.ctx.text_edit_focused());
    let command = Modifiers {
        mac_cmd: true,
        command: true,
        ..Modifiers::NONE
    };
    s.key(Key::A, true, command).unwrap();
    s.key(Key::A, false, command).unwrap();
    s.frame(vec![Event::Text("0.75".into())], Duration::ZERO)
        .unwrap();
    s.key(Key::Enter, true, Modifiers::NONE).unwrap();
    s.key(Key::Enter, false, Modifiers::NONE).unwrap();
    assert_eq!(
        s.state.selected_asset_view().unwrap().playback.position,
        0.75
    );
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
    assert_eq!(s.state.editor.selected_objects, selected);
    assert!(!s.state.editor.edit_mode);
    assert!(!s.state.is_dirty());
}
