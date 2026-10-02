//! Tool Dock: one shared container and tab bar for transient tools.
//! Placement is a host layout choice; panel sessions never enter document history.
//! Panel content, keyboard focus, and playback remain with their host adapters.
use super::*;
use crate::terminal::{TerminalSession, TerminalView};
use egui::AtomExt;

pub(crate) struct ToolDock {
    pub(crate) active: Option<ToolDockPanel>,
    pub(crate) floating_tabs_rect: Option<egui::Rect>,
    pub(crate) terminal: TerminalSession,
    terminal_view: TerminalView,
    input_frame: Option<u64>,
    captured_keys: Vec<egui::Key>,
    pub(crate) terminal_start_requested: bool,
}

impl Default for ToolDock {
    fn default() -> Self {
        Self {
            active: None,
            floating_tabs_rect: None,
            terminal: TerminalSession::placeholder(),
            terminal_view: TerminalView::new(egui::Id::new("n3.terminal")),
            input_frame: None,
            captured_keys: Vec::new(),
            terminal_start_requested: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolDockPanel {
    Animation,
    Terminal,
}

// Add a tab only when its actual panel exists. This is the workspace's catalog,
// not a docking framework or a separate copy of application action metadata.
const TABS: &[ToolDockPanel] = &[ToolDockPanel::Animation, ToolDockPanel::Terminal];

impl ToolDockPanel {
    fn action(self) -> ActionId {
        match self {
            Self::Animation => ActionId::AnimationPanel,
            Self::Terminal => ActionId::TerminalPanel,
        }
    }
}

impl WorkspaceUi {
    pub(crate) fn animation_panel_is_open(&self) -> bool {
        self.tool_dock.active == Some(ToolDockPanel::Animation)
    }

    pub(crate) fn terminal_focused(&self, ctx: &egui::Context) -> bool {
        self.show_ui
            && self.tool_dock.active == Some(ToolDockPanel::Terminal)
            && self.tool_dock.terminal_view.is_focused(ctx)
    }

    pub(super) fn tool_dock_owns_input(&self, ctx: &egui::Context) -> bool {
        self.animation_owns_input(ctx)
            || self.tool_dock.input_frame == Some(ctx.cumulative_frame_nr())
    }

    fn claim_tool_dock_input(&mut self, ctx: &egui::Context) {
        self.tool_dock.input_frame = Some(ctx.cumulative_frame_nr());
        shortcuts::suppress_viewport_input(ctx);
    }

    /// Snapshot ownership before widgets can surrender focus. Retain held keys
    /// through release so clicking back to the viewport cannot turn a terminal
    /// Space/Option hold into a camera gesture. No terminal shortcut is added.
    pub(super) fn prepare_tool_dock_frame(&mut self, ctx: &egui::Context) {
        if ctx.current_pass_index() != 0 {
            return;
        }
        let palette = Palette::from_context(ctx, crate::settings::AccentColor::DEFAULT);
        self.tool_dock.terminal.set_default_colors(
            [
                palette.foreground.r(),
                palette.foreground.g(),
                palette.foreground.b(),
            ],
            [
                palette.background.r(),
                palette.background.g(),
                palette.background.b(),
            ],
        );
        self.tool_dock.terminal.poll(ctx);
        let owner = self.terminal_focused(ctx);
        let mut owns = owner || !self.tool_dock.captured_keys.is_empty();
        let (focused, events) = ctx.input(|input| (input.focused, input.events.clone()));
        for event in events {
            if let egui::Event::Key { key, pressed, .. } = event {
                if !pressed {
                    owns |= self.tool_dock.captured_keys.contains(&key);
                    self.tool_dock.captured_keys.retain(|held| *held != key);
                } else if owner && !self.tool_dock.captured_keys.contains(&key) {
                    self.tool_dock.captured_keys.push(key);
                }
            }
        }
        if !focused {
            self.tool_dock.captured_keys.clear();
        }
        if owns {
            self.claim_tool_dock_input(ctx);
        }
        if !self.show_ui || self.tool_dock.active != Some(ToolDockPanel::Terminal) {
            self.tool_dock
                .terminal_view
                .deactivate(ctx, &mut self.tool_dock.terminal);
        }
        if !self.show_ui || !self.animation_panel_is_open() {
            self.cancel_animation_interaction(ctx);
            self.animation.observations.clear();
        }
    }

    fn queue_tool_dock_command(&mut self, command: Command) {
        if !self.pending_ui_commands.contains(&command) {
            self.pending_ui_commands.push(command);
        }
    }

    pub(super) fn toggle_tool_dock_panel(&mut self, panel: ToolDockPanel, ctx: &egui::Context) {
        if self.tool_dock.active == Some(panel) {
            self.close_tool_dock(ctx);
        } else {
            self.cancel_animation_interaction(ctx);
            self.tool_dock
                .terminal_view
                .deactivate(ctx, &mut self.tool_dock.terminal);
            self.tool_dock.active = Some(panel);
            self.show_ui = true;
            self.focus_tool_dock_panel(panel, ctx);
        }
    }

    fn focus_tool_dock_panel(&mut self, panel: ToolDockPanel, ctx: &egui::Context) {
        self.claim_tool_dock_input(ctx);
        match panel {
            ToolDockPanel::Animation => self.focus_animation_panel(ctx),
            ToolDockPanel::Terminal => self.tool_dock.terminal_view.request_focus(ctx),
        }
        ctx.request_repaint();
    }

    pub(super) fn close_tool_dock(&mut self, ctx: &egui::Context) {
        self.cancel_animation_interaction(ctx);
        self.tool_dock
            .terminal_view
            .deactivate(ctx, &mut self.tool_dock.terminal);
        self.tool_dock.active = None;
        self.claim_tool_dock_input(ctx);
        ctx.memory_mut(|memory| memory.request_focus(shortcuts::viewport_focus_id()));
        ctx.request_repaint();
    }

    pub(super) fn tool_dock_panel(&mut self, root: &mut egui::Ui) {
        let Some(active) = self.tool_dock.active.filter(|_| self.show_ui) else {
            return;
        };
        let ctx = root.ctx().clone();
        let panel = egui::Panel::bottom("n3.tool-dock")
            .frame(egui::Frame::side_top_panel(root.style()).inner_margin(egui::Margin::ZERO))
            .resizable(true)
            .default_size(260.0)
            .min_size(150.0)
            .max_size((root.available_height() * 0.65).max(150.0))
            .show(root, |ui| {
                ui.set_min_height(ui.available_height());
                self.pie_underlay(ui);
                ui.spacing_mut().item_spacing.y = 0.0;
                self.tool_dock_tab_bar(ui);
                workspace_panel_separator(ui);
                // The shared dock adds no content inset. Each panel owns its
                // own spacing, including full-width canvases and separators.
                ui.set_min_width(ui.available_width());
                match active {
                    ToolDockPanel::Animation => self.animation_panel_contents(ui),
                    ToolDockPanel::Terminal => {
                        let output = self
                            .tool_dock
                            .terminal_view
                            .show(ui, &mut self.tool_dock.terminal);
                        self.tool_dock.terminal_start_requested |= output.start_requested;
                        for (control, rect) in [
                            (Control::TerminalViewport, output.content_rect),
                            (Control::TerminalStatus, output.notice_rect),
                        ] {
                            controls::record(
                                &ctx,
                                control,
                                if control == Control::TerminalStatus {
                                    &output.status
                                } else {
                                    control.label()
                                },
                                rect,
                                true,
                            );
                        }
                        if output.focused {
                            self.claim_tool_dock_input(&ctx);
                        }
                    }
                }
            });
        for control in [
            Control::ToolDock,
            match active {
                ToolDockPanel::Animation => Control::AnimationTimeline,
                ToolDockPanel::Terminal => Control::TerminalPanel,
            },
        ] {
            controls::record(&ctx, control, control.label(), panel.response.rect, true);
        }
    }

    pub(super) fn tool_dock_tab_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let active = self.tool_dock.active;
        let margin = if active.is_some() {
            theme::space::margin(theme::TOOL_DOCK_TAB_BAR_INSET)
        } else {
            egui::Margin::ZERO
        };
        let bar = controls::scope(&ctx, Control::ToolDockTabBar, || {
            egui::Frame::NONE.inner_margin(margin).show(ui, |ui| {
                if active.is_some() {
                    ui.set_min_width(ui.available_width());
                }
                ui.horizontal(|ui| {
                    // A selectable/ghost Button omits its idle frame. A nonzero
                    // stroke then changes measurement when that frame appears on
                    // hover. Keep this row's geometry fixed and use fill feedback.
                    let widgets = &mut ui.visuals_mut().widgets;
                    for widget in [
                        &mut widgets.noninteractive,
                        &mut widgets.inactive,
                        &mut widgets.open,
                        &mut widgets.hovered,
                        &mut widgets.active,
                    ] {
                        widget.bg_stroke = egui::Stroke::NONE;
                        widget.expansion = 0.0;
                    }
                    for &tab in TABS {
                        let action = tab.action();
                        let state = self.action_state(action);
                        if !state.visible {
                            continue;
                        }
                        let response = ui
                            .add_enabled(
                                state.enabled,
                                egui::Button::selectable(active == Some(tab), action.label()),
                            )
                            .on_hover_text(format!(
                                "Open and focus {} in the Tool Dock",
                                action.label()
                            ));
                        controls::record(
                            &ctx,
                            match tab {
                                ToolDockPanel::Animation => Control::AnimationPanelToggle,
                                ToolDockPanel::Terminal => Control::TerminalPanelToggle,
                            },
                            action.label(),
                            response.rect,
                            response.enabled(),
                        );
                        if response.clicked() {
                            match active {
                                Some(current) if current == tab => {
                                    // Activation is idempotent; an active tab is
                                    // focused, while its close button dismisses it.
                                    self.focus_tool_dock_panel(tab, &ctx);
                                }
                                _ => self.queue_tool_dock_command(action.command()),
                            }
                        }
                    }
                    if active.is_some() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            self.tool_dock_close_button(ui);
                        });
                    }
                });
            })
        });
        controls::record(
            &ctx,
            Control::ToolDockTabBar,
            Control::ToolDockTabBar.label(),
            bar.response.rect,
            bar.response.enabled(),
        );
    }

    fn tool_dock_close_button(&mut self, ui: &mut egui::Ui) {
        let response = ui
            .scope(|ui| {
                ui.spacing_mut().button_padding = egui::Vec2::splat(theme::space::SM);
                ui.add_enabled(
                    self.action_state(ActionId::CloseToolDock).enabled,
                    egui::Button::new(
                        lucide::Icon::X
                            .text(theme::text::SM)
                            .atom_size(egui::Vec2::splat(theme::size::XL_2)),
                    )
                    .frame_when_inactive(false)
                    .min_size(egui::Vec2::splat(theme::ROW_HEIGHT)),
                )
            })
            .inner
            .on_hover_text(Control::ToolDockClose.label());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                response.enabled(),
                Control::ToolDockClose.label(),
            )
        });
        controls::record(
            ui.ctx(),
            Control::ToolDockClose,
            Control::ToolDockClose.label(),
            response.rect,
            response.enabled(),
        );
        if response.clicked() {
            self.queue_tool_dock_command(Command::CloseToolDock);
        }
    }

    pub(super) fn floating_tool_dock_tabs(&mut self, ctx: &egui::Context) -> Option<egui::Rect> {
        if !self.show_ui || self.tool_dock.active.is_some() || !self.viewport_ui_rect.is_positive()
        {
            return None;
        }
        let palette = Palette::from_context(ctx, self.accent_color);
        let area = egui::Area::new(egui::Id::new("n3.tool-dock.tabs"))
            .order(egui::Order::Middle)
            .pivot(egui::Align2::LEFT_BOTTOM)
            .fixed_pos(egui::pos2(
                self.viewport_ui_rect.left() + theme::space::XL,
                self.viewport_ui_rect.bottom() - theme::space::XL,
            ))
            .show(ctx, |ui| {
                self.pie_underlay(ui);
                egui::Frame::popup(ui.style())
                    .fill(palette.workbench_hud)
                    .stroke(egui::Stroke::new(1.0, palette.workbench_hud_border))
                    .corner_radius(theme::radius::MD)
                    .inner_margin(theme::space::SM)
                    .show(ui, |ui| self.tool_dock_tab_bar(ui));
            });
        Some(area.response.rect)
    }
}
