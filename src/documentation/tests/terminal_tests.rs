//! Real Tool Dock integration retains focus and document/history boundaries.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{editor::Tool, workspace_ui::ToolDockPanel};
use egui::{Event, Key, Modifiers};
use std::time::Duration;

fn setup(s: &mut Session<'_>) {
    s.state.tool_dock.terminal = crate::terminal::TerminalSession::interactive_fixture();
    s.state
        .tool_dock
        .terminal
        .ingest(b"Test terminal output\r\n");
    s.load_fixture("cube-quads.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    s.state.editor.set_tool(Tool::View);
    s.state.mark_saved("terminal.n3.json".into(), Vec::new());
    s.settle().unwrap();
}

#[test]
fn terminal_focus_blocks_viewport_actions_and_returns_after_close() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for additional_pass in [false, true] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        s.extra_layout_pass = additional_pass;
        let document = s.state.editor.document.clone();
        let selection = s.state.editor.selected_objects.clone();
        let revision = s.state.editor.revision;
        let camera = s.state.camera.view_projection(1.0);
        s.click(Control::TerminalPanelToggle).unwrap();
        assert_eq!(s.state.tool_dock.active, Some(ToolDockPanel::Terminal));
        assert!(s.state.tool_dock.terminal.is_interactive());
        s.witness(Control::TerminalStatus).unwrap();
        s.click(Control::TerminalViewport).unwrap();
        let output = s.state.tool_dock.terminal.visible_text();
        s.frame(vec![Event::Text("typed text".into())], Duration::ZERO)
            .unwrap();
        assert_eq!(
            s.state.tool_dock.terminal.take_fixture_input(),
            b"typed text"
        );
        s.key(Key::Enter, true, Modifiers::NONE).unwrap();
        s.key(Key::Enter, false, Modifiers::NONE).unwrap();
        assert_eq!(s.state.tool_dock.terminal.take_fixture_input(), b"\r");
        s.key(Key::C, true, Modifiers::CTRL).unwrap();
        s.key(Key::C, false, Modifiers::CTRL).unwrap();
        s.modifiers_changed(Modifiers::NONE).unwrap();
        assert_eq!(s.state.tool_dock.terminal.take_fixture_input(), b"\x03");
        s.state.tool_dock.terminal.ingest(b"\x1b[?2004h");
        s.frame(vec![Event::Paste("one\ntwo".into())], Duration::ZERO)
            .unwrap();
        assert_eq!(
            s.state.tool_dock.terminal.take_fixture_input(),
            b"\x1b[200~one\ntwo\x1b[201~"
        );
        for action in [
            "edit.confirm",
            "selection.duplicate",
            "selection.backspace",
            "tool.move",
            "view.front",
            "history.undo",
            "animation.play-pause",
        ] {
            s.shortcut(action).unwrap();
            assert_eq!(
                s.state.tool_dock.terminal.visible_text(),
                output,
                "{action}"
            );
            assert_eq!(s.state.editor.document, document, "{action}");
            assert_eq!(s.state.editor.selected_objects, selection, "{action}");
            assert_eq!(s.state.editor.tool, Tool::View, "{action}");
            assert_eq!(s.state.editor.revision, revision, "{action}");
            assert!(!s.state.editor.edit_mode, "{action}");
            assert!(!s.state.editor.is_interacting(), "{action}");
            assert_eq!(s.state.camera.view_projection(1.0), camera, "{action}");
            assert_eq!(s.state.tool_dock.active, Some(ToolDockPanel::Terminal));
        }
        s.click(Control::ToolDockClose).unwrap();
        assert!(s.state.tool_dock.active.is_none());
        assert_eq!(
            s.ctx.memory(|memory| memory.focused()),
            Some(crate::shortcuts::viewport_focus_id())
        );
        assert!(!s.state.is_dirty());
        assert!(!s.state.editor.undo());
        s.shortcut("tool.move").unwrap();
        assert_eq!(s.state.editor.tool, Tool::Move);
    }
}

#[test]
fn terminal_switching_preserves_animation_playback_and_shared_dock_bounds() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.state.tool_dock.terminal = crate::terminal::TerminalSession::interactive_fixture();
    s.load_scene_fixture("SimpleSkin/glTF/SimpleSkin.gltf")
        .unwrap();
    s.state
        .mark_saved("terminal-animation.n3.json".into(), Vec::new());
    s.click(Control::AnimationPanelToggle).unwrap();
    s.shortcut("animation.play-pause").unwrap();
    let accepted = s.state.selected_asset_view().unwrap().clone();
    assert!(accepted.playback.playing);
    let dock = s.trace.get(Control::ToolDock).unwrap().rect;
    let document = s.state.editor.document.clone();
    s.extra_layout_pass = true;
    for _ in 0..2 {
        s.click(Control::TerminalPanelToggle).unwrap();
        assert_eq!(s.state.tool_dock.active, Some(ToolDockPanel::Terminal));
        assert_eq!(s.trace.get(Control::ToolDock).unwrap().rect, dock);
        assert!(s.trace.get(Control::AnimationTimeline).is_err());
        s.click(Control::TerminalViewport).unwrap();
        s.key(Key::Space, true, Modifiers::NONE).unwrap();
        s.key(Key::Space, false, Modifiers::NONE).unwrap();
        assert_eq!(
            s.state.selected_asset_view().unwrap().playback,
            accepted.playback
        );
        assert_eq!(s.state.editor.document, document);
        s.click(Control::TerminalPanelToggle).unwrap();
        assert_eq!(s.state.tool_dock.active, Some(ToolDockPanel::Terminal));
        s.click(Control::AnimationPanelToggle).unwrap();
        assert!(s.state.animation_panel_is_open());
        assert_eq!(s.trace.get(Control::ToolDock).unwrap().rect, dock);
        assert!(s.trace.get(Control::TerminalPanel).is_err());
        assert_eq!(
            s.state.selected_asset_view().unwrap().playback,
            accepted.playback
        );
    }
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
    s.shortcut("animation.play-pause").unwrap();
    assert!(!s.state.selected_asset_view().unwrap().playback.playing);
}

#[test]
fn terminal_and_animation_tabs_align_in_both_tool_dock_placements() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for theme in [
        crate::settings::ThemeMode::Light,
        crate::settings::ThemeMode::Dark,
    ] {
        let mut s = Session::new(&mut capture).unwrap();
        s.state.tool_dock.terminal = crate::terminal::TerminalSession::interactive_fixture();
        s.state.theme_mode = theme;
        s.settle().unwrap();
        let floating = s.trace.get(Control::ToolDockTabBar).unwrap().rect;
        assert!(
            s.trace
                .get(Control::Viewport)
                .unwrap()
                .rect
                .contains_rect(floating)
        );
        let initial_height = s
            .trace
            .get(Control::TerminalPanelToggle)
            .unwrap()
            .rect
            .height();
        s.click(Control::TerminalPanelToggle).unwrap();
        for open_panel in [Control::TerminalPanelToggle, Control::AnimationPanelToggle] {
            s.click(open_panel).unwrap();
            let dock = s.trace.get(Control::ToolDock).unwrap().rect;
            let bar = s.trace.get(Control::ToolDockTabBar).unwrap().rect;
            let animation = s.trace.get(Control::AnimationPanelToggle).unwrap().rect;
            let terminal = s.trace.get(Control::TerminalPanelToggle).unwrap().rect;
            let close = s.trace.get(Control::ToolDockClose).unwrap().rect;
            assert!(dock.contains_rect(bar));
            for control in [animation, terminal, close] {
                assert!(bar.contains_rect(control));
                assert_eq!(control.center().y, terminal.center().y);
                assert_eq!(control.height(), initial_height);
            }
            assert!(animation.right() <= terminal.left());
            assert!(terminal.right() <= close.left());
            let before = (dock, bar, animation, terminal, close);
            for control in [open_panel, Control::ToolDockClose] {
                s.hover(control).unwrap();
                assert_eq!(
                    (
                        s.trace.get(Control::ToolDock).unwrap().rect,
                        s.trace.get(Control::ToolDockTabBar).unwrap().rect,
                        s.trace.get(Control::AnimationPanelToggle).unwrap().rect,
                        s.trace.get(Control::TerminalPanelToggle).unwrap().rect,
                        s.trace.get(Control::ToolDockClose).unwrap().rect,
                    ),
                    before,
                    "Hover must not change Tool Dock geometry"
                );
            }
        }
        s.click(Control::ToolDockClose).unwrap();
        assert_eq!(s.trace.get(Control::ToolDockTabBar).unwrap().rect, floating);
    }
}

#[test]
fn terminal_scrollback_owns_wheel_without_navigating_viewport_or_repeating_layout_input() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut results = Vec::new();
    for extra in [false, true] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        // Input fixture: the actual parser ingests output before the illustrated scroll.
        for line in 0..80 {
            s.state
                .tool_dock
                .terminal
                .ingest(format!("\r\nOutput line {line:02}").as_bytes());
        }
        s.click(Control::TerminalPanelToggle).unwrap();
        s.extra_layout_pass = extra;
        s.hover(Control::TerminalViewport).unwrap();
        let camera = s.state.camera.view_projection(1.0);
        let document = s.state.editor.document.clone();
        let before = s.state.tool_dock.terminal.visible_text();
        assert!(s.state.tool_dock.terminal.history_size() > 0);
        assert_eq!(s.state.tool_dock.terminal.display_offset(), 0);
        s.scroll(0.0, 90.0, false, Modifiers::NONE).unwrap();
        let offset = s.state.tool_dock.terminal.display_offset();
        assert!(offset > 0);
        assert_ne!(s.state.tool_dock.terminal.visible_text(), before);
        results.push(offset);
        assert_eq!(s.state.camera.view_projection(1.0), camera);
        assert_eq!(s.state.editor.document, document);
        s.scroll(0.0, -90.0, false, Modifiers::NONE).unwrap();
        assert_eq!(s.state.tool_dock.terminal.display_offset(), 0);
        assert_eq!(s.state.tool_dock.terminal.visible_text(), before);
    }
    assert_eq!(
        results[0], results[1],
        "A layout retry must not scroll twice"
    );
}

#[test]
fn terminal_held_keys_and_escape_do_not_leak_after_focus_moves() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.click(Control::TerminalPanelToggle).unwrap();
    s.click(Control::TerminalViewport).unwrap();
    let camera = s.state.camera.view_projection(1.0);
    let document = s.state.editor.document.clone();
    s.key(Key::Space, true, Modifiers::NONE).unwrap();
    let empty = s.empty_viewport_point().unwrap();
    s.click_at(empty).unwrap();
    assert_eq!(
        s.ctx.memory(|memory| memory.focused()),
        Some(crate::shortcuts::viewport_focus_id())
    );
    for pressed in [true, false] {
        s.frame(
            vec![Event::Key {
                key: Key::Space,
                physical_key: None,
                pressed,
                repeat: pressed,
                modifiers: Modifiers::NONE,
            }],
            Duration::ZERO,
        )
        .unwrap();
        assert!(!s.state.temporary_navigation_active());
        assert!(!s.state.mouse_navigation_active());
        assert_eq!(s.state.camera.view_projection(1.0), camera);
    }
    s.click(Control::TerminalViewport).unwrap();
    let selection = s.state.editor.selected_objects.clone();
    let terminal_focus = s.ctx.memory(|memory| memory.focused());
    s.state.tool_dock.terminal.take_fixture_input();
    for pressed in [true, false] {
        s.key(Key::Escape, pressed, Modifiers::NONE).unwrap();
        assert_eq!(s.state.editor.selected_objects, selection);
        assert_eq!(s.state.tool_dock.active, Some(ToolDockPanel::Terminal));
        assert_eq!(s.state.editor.document, document);
        assert_eq!(s.ctx.memory(|memory| memory.focused()), terminal_focus);
    }
    assert_eq!(s.state.tool_dock.terminal.take_fixture_input(), b"\x1b");
    assert!(!s.state.is_dirty());
    assert!(!s.state.editor.undo());
}
