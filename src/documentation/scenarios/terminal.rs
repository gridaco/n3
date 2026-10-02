//! Real local shell input through the Tool Dock, with a controlled native PTY.
use super::{Result, Session};
use crate::{controls::Control, workspace_ui::ToolDockPanel};
use egui::{Event, Key, Modifiers, PointerButton};
use std::time::{Duration, Instant};

fn is_terminal(s: &Session<'_>) -> bool {
    s.state.tool_dock.active == Some(ToolDockPanel::Terminal)
}

fn aligned_tabs(s: &mut Session<'_>, open: bool) -> Result<()> {
    let bar = s.trace.get(Control::ToolDockTabBar)?.rect;
    let terminal = s.trace.get(Control::TerminalPanelToggle)?.rect;
    let animation = s.trace.get(Control::AnimationPanelToggle)?.rect;
    s.require(
        bar.contains_rect(terminal)
            && bar.contains_rect(animation)
            && terminal.center().y == animation.center().y
            && terminal.height() == animation.height()
            && animation.right() <= terminal.left(),
        "Animation and Terminal are aligned, separate controls in the same Tool Dock tab bar",
    )?;
    if open {
        let dock = s.trace.get(Control::ToolDock)?.rect;
        let close = s.trace.get(Control::ToolDockClose)?.rect;
        s.require(
            dock.contains_rect(bar)
                && bar.contains_rect(close)
                && bar.height() == crate::theme::TOOL_DOCK_TAB_BAR_HEIGHT
                && animation.left() - bar.left() == crate::theme::TOOL_DOCK_TAB_BAR_INSET
                && animation.top() - bar.top() == crate::theme::TOOL_DOCK_TAB_BAR_INSET
                && terminal.center().y == bar.center().y
                && close.center().y == terminal.center().y
                && close.height() == terminal.height(),
            "The open Tool Dock owns a compact 32-point tab bar with aligned controls and equal 2-point container padding",
        )
    } else {
        s.require(
            s.trace.get(Control::Viewport)?.rect.contains_rect(bar)
                && s.trace.get(Control::ToolDock).is_err()
                && s.trace.get(Control::ToolDockClose).is_err(),
            "Closing the Tool Dock returns its tab bar to the viewport without a second container",
        )
    }
}

/// PTY output is asynchronous host input. Wait for the actual shell response
/// under a bounded deadline while keeping the replay clock fixed; no expected
/// output is painted into the terminal by the scenario.
fn wait_for_shell(s: &mut Session<'_>, expected_line: Option<&str>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        s.state.tool_dock.terminal.poll(&s.ctx);
        s.settle()?;
        let text = s.state.tool_dock.terminal.visible_text();
        let ready = text.trim_end().ends_with("n3>")
            && expected_line.is_none_or(|expected| text.lines().any(|line| line == expected));
        if ready {
            // Two additional drains must see the same idle shell transcript.
            let before = text.clone();
            for _ in 0..2 {
                std::thread::sleep(Duration::from_millis(10));
                s.state.tool_dock.terminal.poll(&s.ctx);
                s.settle()?;
            }
            if s.state.tool_dock.terminal.visible_text() == before {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "Controlled terminal shell did not reach its prompt: {text:?}"
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.state.tool_dock.terminal = crate::terminal::TerminalSession::dormant_native();
    s.load_fixture("cube-quads.obj")?;
    let object = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(object)?;
    s.state
        .mark_saved("terminal-example.n3.json".into(), Vec::new());
    s.settle()?;
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let selection = s.state.editor.selected_objects.clone();
    let tool = s.state.editor.tool;
    // Dock resizing changes aspect but must not alter the camera itself.
    let camera = s.state.camera.view_projection(1.0);
    s.require(
        s.state.tool_dock.active.is_none(),
        "The Tool Dock starts closed, including for a nonempty document",
    )?;
    aligned_tabs(s, false)?;
    s.click(Control::TerminalPanelToggle)?;
    s.require(
        is_terminal(s),
        "The floating Terminal tab opens Terminal in the Tool Dock",
    )?;
    for control in [
        Control::ToolDock,
        Control::ToolDockTabBar,
        Control::TerminalPanel,
        Control::TerminalViewport,
        Control::TerminalStatus,
        Control::ToolDockClose,
    ] {
        s.witness(control)?;
    }
    // Use a real shell with no login/profile scripts. Only the native program
    // setup is a fixture; typed commands and displayed output use production IO.
    s.state.tool_dock.terminal.start_program(
        &s.ctx,
        "/usr/bin/env",
        &[
            "-i",
            "PATH=/usr/bin:/bin",
            "TERM=xterm-256color",
            "COLORTERM=truecolor",
            "LC_ALL=C",
            "PS1=n3> ",
            "ENV=",
            "BASH_ENV=",
            "INPUTRC=/dev/null",
            "HISTFILE=/dev/null",
            "/bin/sh",
            "-i",
        ],
        Some(&std::env::temp_dir()),
        &[],
    )?;
    wait_for_shell(s, None)?;
    s.require(
        matches!(
            s.state.tool_dock.terminal.status(),
            crate::terminal::SessionStatus::Running
        ),
        "Terminal connects to a real controlled local shell through the native PTY",
    )?;
    s.witness(Control::TerminalStatus)?;
    aligned_tabs(s, true)?;
    s.click(Control::TerminalViewport)?;
    s.frame(vec![Event::Text("printf 'Local shell ready.\\nANSI: \\033[31mred\\033[0m \\033[32mgreen\\033[0m \\033[34mblue\\033[0m\\n'".into())], Duration::ZERO)?;
    s.key(Key::Enter, true, Modifiers::NONE)?;
    s.key(Key::Enter, false, Modifiers::NONE)?;
    wait_for_shell(s, Some("Local shell ready."))?;
    s.require(
        s.state.tool_dock.terminal.visible_text().lines().any(|line| line == "ANSI: red green blue")
            && s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.selected_objects == selection
            && s.state.editor.tool == tool
            && !s.state.editor.edit_mode
            && s.state.camera.view_projection(1.0) == camera
            && !s.state.editor.is_pointer_interacting(),
        "Real Terminal input executes printf through the PTY and displays its ANSI output without changing viewport selection, tools, geometry, or camera",
    )?;
    s.hover(Control::TerminalViewport)?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("terminal-light")?;
    let rows_before = s.state.tool_dock.terminal.rows();
    let before = s.trace.get(Control::ToolDock)?.rect;
    let start = egui::pos2(before.center().x, before.top() + 1.0);
    s.frame(vec![Event::PointerMoved(start)], Duration::ZERO)?;
    s.require(
        s.cursor == egui::CursorIcon::ResizeVertical,
        "The live Tool Dock upper edge exposes its resize interaction",
    )?;
    s.pointer_button(PointerButton::Primary, true)?;
    s.frame(
        vec![Event::PointerMoved(egui::pos2(
            start.x,
            before.top() - 72.0,
        ))],
        Duration::ZERO,
    )?;
    s.pointer_button(PointerButton::Primary, false)?;
    s.settle()?;
    let resized = s.trace.get(Control::ToolDock)?.rect;
    s.require(
        s.state.tool_dock.terminal.rows() > rows_before
            && resized.height() > before.height() + 60.0
            && resized.bottom() == before.bottom()
            && resized.contains_rect(s.trace.get(Control::TerminalViewport)?.rect)
            && !s.state.editor.is_pointer_interacting(),
        "Resizing the Tool Dock expands Terminal inside its actual bounds without starting a viewport gesture",
    )?;
    aligned_tabs(s, true)?;
    s.hover(Control::TerminalViewport)?;
    s.capture_image("terminal-resized")?;

    // Read existing history through actual wheel routing, without moving the camera.
    s.scroll(0.0, 100.0, false, Modifiers::NONE)?;
    s.scroll(0.0, -100.0, false, Modifiers::NONE)?;
    s.click(Control::AnimationPanelToggle)?;
    s.require(
        s.state.animation_panel_is_open()
            && s.trace.get(Control::TerminalPanel).is_err()
            && s.trace.get(Control::ToolDock)?.rect == resized,
        "Switching to Animation replaces Tool Dock content and preserves its size",
    )?;
    s.click(Control::TerminalPanelToggle)?;
    s.click(Control::TerminalPanelToggle)?;
    s.require(
        is_terminal(s) && s.trace.get(Control::ToolDock)?.rect == resized,
        "Returning to Terminal and activating its selected tab keeps the same Tool Dock open",
    )?;
    let retained_output = s.state.tool_dock.terminal.visible_text();
    s.click(Control::ToolDockClose)?;
    s.require(
        s.state.tool_dock.active.is_none(),
        "Close dismisses the Tool Dock",
    )?;
    aligned_tabs(s, false)?;
    s.click(Control::TerminalPanelToggle)?;
    s.require(
        matches!(
            s.state.tool_dock.terminal.status(),
            crate::terminal::SessionStatus::Running
        ) && s.state.tool_dock.terminal.visible_text() == retained_output,
        "Closing and reopening the Tool Dock retains the running shell and its real output",
    )?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click(Control::AppearanceThemeMenu)?;
    s.click(Control::ThemeDark)?;
    s.click(Control::PreferencesClose)?;
    s.require(
        s.ctx.theme() == egui::Theme::Dark && is_terminal(s),
        "Terminal remains available when the application appearance changes to Dark",
    )?;
    s.hover(Control::TerminalViewport)?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("terminal-dark")?;
    let has_no_undo = !s.state.editor.undo();
    s.require(
        s.state.editor.document == document
            && s.state.editor.revision == revision
            && s.state.editor.selected_objects == selection
            && s.state.camera.view_projection(1.0) == camera
            && !s.state.is_dirty()
            && has_no_undo,
        "Opening, focusing, scrolling, resizing, switching, and closing Tool Dock panels do not alter geometry or add document history",
    )?;

    s.click(Control::ToolDockClose)?;
    s.state.host_capabilities = crate::workspace_ui::HostCapabilities::BROWSER;
    s.settle()?;
    let action = crate::input::actions::ActionId::TerminalPanel;
    let availability = s.state.action_state(action);
    s.require(
        !availability.visible
            && !availability.enabled
            && s.trace.get(Control::TerminalPanelToggle).is_err()
            && s.trace.get(Control::AnimationPanelToggle).is_ok(),
        "Hosts without a local shell omit Terminal from the shared Tool Dock and retain Animation",
    )?;
    s.state.dispatch(action.command(), &s.ctx, false);
    s.settle()?;
    s.require(
        s.state.tool_dock.active.is_none() && s.trace.get(Control::TerminalPanel).is_err(),
        "Semantic dispatch cannot open a Terminal that the host does not support",
    )?;
    s.click(Control::AnimationPanelToggle)?;
    s.require(
        s.state.animation_panel_is_open(),
        "Animation remains usable in the shared Tool Dock without Terminal",
    )
}
