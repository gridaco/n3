//! Internal component consumers. Fixtures own state/effects; production owns UI/input.
mod native;
mod replay;
mod terminal;
#[cfg(test)]
mod terminal_tests;
#[cfg(test)]
mod tests;
mod timeline;
mod timeline_measure;
#[cfg(test)]
mod timeline_tests;

use crate::{
    editor::Tool,
    input::{
        actions::{ActionId, ActionState},
        bindings,
    },
    pie_input::{PieContext, PieInput},
    render::shading::ShadingMode,
    settings::{AccentColor, ResolvedTheme},
    shortcuts::{self, Command},
    theme,
    ui::{controls::Control, menu, ruler_2d},
    units::LengthUnit,
};
use egui::{Context, Id, Rect, Response, Ui, Vec2};
use std::collections::VecDeque;

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    native::run()
}
pub(crate) fn evidence() -> Result<(), String> {
    replay::evidence()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Case {
    Dropdown,
    Context,
    MenuIsolation,
    ViewPie,
    ShadingPie,
    PieIsolation,
    RulerRanges,
    RulerDensity,
    TimelineSmall,
    TimelineMarquee,
    TimelineHierarchy,
    TimelineDense,
    TimelineEmpty,
    TimelineZero,
    TimelineIsolation,
    TimelineReject,
    TerminalPlaceholder,
    TerminalNarrow,
    TerminalIsolation,
    TerminalInput,
}
impl Case {
    const ALL: [Self; 20] = [
        Self::Dropdown,
        Self::Context,
        Self::MenuIsolation,
        Self::ViewPie,
        Self::ShadingPie,
        Self::PieIsolation,
        Self::RulerRanges,
        Self::RulerDensity,
        Self::TimelineSmall,
        Self::TimelineMarquee,
        Self::TimelineHierarchy,
        Self::TimelineDense,
        Self::TimelineEmpty,
        Self::TimelineZero,
        Self::TimelineIsolation,
        Self::TimelineReject,
        Self::TerminalPlaceholder,
        Self::TerminalNarrow,
        Self::TerminalIsolation,
        Self::TerminalInput,
    ];
    fn id(self) -> &'static str {
        match self {
            Self::Dropdown => "menus-dropdown",
            Self::Context => "menus-context",
            Self::MenuIsolation => "menus-isolation",
            Self::ViewPie => "pies-view",
            Self::ShadingPie => "pies-shading",
            Self::PieIsolation => "pies-isolation",
            Self::RulerRanges => "ruler-ranges",
            Self::RulerDensity => "ruler-density",
            Self::TimelineSmall => "timeline-small",
            Self::TimelineMarquee => "timeline-marquee",
            Self::TimelineHierarchy => "timeline-hierarchy",
            Self::TimelineDense => "timeline-dense",
            Self::TimelineEmpty => "timeline-empty",
            Self::TimelineZero => "timeline-zero",
            Self::TimelineIsolation => "timeline-isolation",
            Self::TimelineReject => "timeline-reject",
            Self::TerminalPlaceholder => "terminal-placeholder",
            Self::TerminalNarrow => "terminal-narrow",
            Self::TerminalIsolation => "terminal-isolation",
            Self::TerminalInput => "terminal-input",
        }
    }
    fn page(self) -> &'static str {
        match self {
            Self::Dropdown | Self::Context | Self::MenuIsolation => "Menus",
            Self::ViewPie | Self::ShadingPie | Self::PieIsolation => "Held-key pies",
            Self::RulerRanges | Self::RulerDensity => "2D ruler",
            Self::TimelineSmall
            | Self::TimelineMarquee
            | Self::TimelineHierarchy
            | Self::TimelineDense
            | Self::TimelineEmpty
            | Self::TimelineZero
            | Self::TimelineIsolation
            | Self::TimelineReject => "Timeline",
            Self::TerminalPlaceholder
            | Self::TerminalNarrow
            | Self::TerminalIsolation
            | Self::TerminalInput => "Terminal",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Dropdown => "Nested action dropdown",
            Self::Context => "Viewport context actions",
            Self::MenuIsolation => "Independent menu instances",
            Self::ViewPie => "View: hold, move, release",
            Self::ShadingPie => "Shading: selection and help",
            Self::PieIsolation => "Independent input owners",
            Self::RulerRanges => "Projected selection ranges",
            Self::RulerDensity => "Density and large coordinates",
            Self::TimelineSmall => "Keys, scrubbing and transport",
            Self::TimelineMarquee => "Box select across tracks",
            Self::TimelineHierarchy => "Nested tracks and long labels",
            Self::TimelineDense => "1,000 tracks · 100,000 keys",
            Self::TimelineEmpty => "Empty track list",
            Self::TimelineZero => "Zero-duration content",
            Self::TimelineIsolation => "Independent timeline instances",
            Self::TimelineReject => "Host rejects requested seeks",
            Self::TerminalPlaceholder => "Sample output and focus ownership",
            Self::TerminalNarrow => "Narrow view and scrollback",
            Self::TerminalIsolation => "Independent terminal sessions",
            Self::TerminalInput => "Interactive bytes and TUI modes",
        }
    }
    fn pies(self) -> bool {
        self.page() == "Held-key pies"
    }
    fn ruler(self) -> bool {
        self.page() == "2D ruler"
    }
    fn timeline(self) -> bool {
        self.page() == "Timeline"
    }
    fn terminal(self) -> bool {
        self.page() == "Terminal"
    }
    fn count(self) -> usize {
        if matches!(
            self,
            Self::MenuIsolation
                | Self::PieIsolation
                | Self::TimelineIsolation
                | Self::TerminalIsolation
        ) {
            2
        } else {
            1
        }
    }
}

struct Instance {
    pie: PieInput,
    bounds: Rect,
    checked: bool,
    shading: ShadingMode,
}
impl Instance {
    fn new(case: Case, index: usize) -> Self {
        Self {
            pie: PieInput::for_instance(instance_id(case, index)),
            bounds: Rect::NOTHING,
            checked: false,
            shading: ShadingMode::Solid,
        }
    }
}
fn instance_id(case: Case, index: usize) -> Id {
    Id::new(("n3.workbench", case.id(), index))
}

#[derive(Clone, Debug)]
struct Observed {
    instance: usize,
    target: String,
    id: Id,
    rect: Rect,
    enabled: bool,
}
#[derive(Clone, Debug, PartialEq)]
struct Delivered {
    instance: usize,
    command: Command,
}

struct Workbench {
    case: Case,
    catalog_case: Option<Case>,
    dimensions: Vec2,
    appearance: ResolvedTheme,
    applied_appearance: Option<ResolvedTheme>,
    disabled: bool,
    unavailable: bool,
    fit_enabled: bool,
    long_label: bool,
    scale: f64,
    origin: f64,
    range: [f32; 2],
    text: String,
    instances: [Instance; 2],
    timeline: Option<timeline::Fixture>,
    terminal: Option<terminal::Fixture>,
    observed: Vec<Observed>,
    events: VecDeque<Delivered>,
    pending: Vec<Delivered>,
    delivered_frame: Vec<Delivered>,
    frame: Option<u64>,
    raw_events: Vec<egui::Event>,
}
impl Default for Workbench {
    fn default() -> Self {
        Self {
            case: Case::Dropdown,
            catalog_case: None,
            dimensions: egui::vec2(760.0, 480.0),
            appearance: ResolvedTheme::Light,
            applied_appearance: None,
            disabled: true,
            unavailable: true,
            fit_enabled: true,
            long_label: false,
            scale: 4.0,
            origin: 160.0,
            range: [101.0, 201.0],
            text: String::new(),
            instances: [
                Instance::new(Case::Dropdown, 0),
                Instance::new(Case::Dropdown, 1),
            ],
            timeline: None,
            terminal: None,
            observed: Vec::new(),
            events: VecDeque::new(),
            pending: Vec::new(),
            delivered_frame: Vec::new(),
            frame: None,
            raw_events: Vec::new(),
        }
    }
}
impl Workbench {
    fn select_case(&mut self, ctx: &Context, case: Case) {
        self.case = case;
        self.reset(ctx);
    }
    fn reset(&mut self, ctx: &Context) {
        egui::Popup::close_all(ctx);
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
        self.instances = [Instance::new(self.case, 0), Instance::new(self.case, 1)];
        self.timeline = self
            .case
            .timeline()
            .then(|| timeline::Fixture::new(self.case));
        self.terminal = self
            .case
            .terminal()
            .then(|| terminal::Fixture::new(self.case));
        self.disabled = true;
        self.unavailable = true;
        self.fit_enabled = true;
        self.long_label = false;
        self.scale = if self.case == Case::RulerDensity {
            0.04
        } else {
            4.0
        };
        self.origin = if self.case == Case::RulerDensity {
            1.0e8
        } else {
            160.0
        };
        self.range = [101.0, 201.0];
        self.text.clear();
        self.events.clear();
        self.pending.clear();
        self.delivered_frame.clear();
        self.observed.clear();
        menu::finish_frame(ctx);
    }
    fn clear_input(&mut self, reason: crate::ui::timeline::CancelReason) {
        for instance in &mut self.instances {
            instance.pie.reset();
        }
        if let Some(timeline) = &mut self.timeline {
            timeline.clear_input(reason);
        }
        self.pending.clear();
    }
    fn observe(&mut self, instance: usize, target: impl Into<String>, response: &Response) {
        self.observed.push(Observed {
            instance,
            target: target.into(),
            id: response.id,
            rect: response.rect,
            enabled: response.enabled(),
        });
    }
    fn queue(&mut self, instance: usize, command: Command) {
        let event = Delivered { instance, command };
        // Egui can repeat layout in one frame. Effects belong to the host once.
        if !self.pending.contains(&event) && !self.delivered_frame.contains(&event) {
            self.pending.push(event);
        }
    }
    fn deliver(&mut self, ctx: &Context) {
        for event in self.pending.drain(..) {
            match event.command {
                Command::ToggleEdges | Command::ToggleXray | Command::ToggleRuler2D => {
                    self.instances[event.instance].checked = !self.instances[event.instance].checked
                }
                Command::SetShading(mode) => self.instances[event.instance].shading = mode,
                _ => {}
            }
            self.delivered_frame.push(event.clone());
            self.events.push_back(event);
            if self.events.len() > 32 {
                self.events.pop_front();
            }
            ctx.request_repaint();
        }
    }
    fn state(&self, instance: usize, action: ActionId) -> ActionState {
        // Deliberate fixture states, not a second implementation of editor capabilities.
        ActionState {
            visible: !(self.unavailable && matches!(action, ActionId::Import | ActionId::MakeFace)),
            enabled: !(self.disabled && matches!(action, ActionId::Save | ActionId::Delete)),
            checked: matches!(action, ActionId::Edges | ActionId::Xray | ActionId::Ruler2D)
                .then_some(self.instances[instance].checked),
        }
    }
    fn action(&mut self, ui: &mut Ui, instance: usize, action: ActionId) {
        let mut item = menu::Item::new(action, self.state(instance, action));
        if self.long_label && action == ActionId::Preferences {
            item =
                item.label("Preferences — a deliberately long contextual label for sizing review");
        }
        if let Some(response) = item.show(ui) {
            self.observe(instance, action.id(), &response);
            if response.clicked() {
                self.queue(instance, action.command());
                ui.close();
            }
        }
    }
    fn menu_instance(&mut self, ui: &mut Ui, instance: usize) {
        if self.case != Case::Context {
            let (response, _) =
                menu::dropdown(egui::Button::new(Control::N3Menu.label())).ui(ui, |ui| {
                    menu::content(ui, Control::N3Menu, |ui| {
                        let (file, _) = menu::submenu(ui, Control::FileMenu, |ui| {
                            for &action in crate::ui::workspace_menus::FILE_ACTIONS {
                                self.action(ui, instance, action);
                            }
                        });
                        self.observe(instance, "file-submenu", &file);
                        let (view, _) = menu::submenu(ui, Control::ViewMenu, |ui| {
                            for &action in crate::ui::workspace_menus::VIEW_TOGGLE_ACTIONS {
                                self.action(ui, instance, action);
                            }
                        });
                        self.observe(instance, "view-submenu", &view);
                        menu::separator(ui);
                        self.action(ui, instance, ActionId::Preferences);
                    })
                });
            self.observe(instance, "menu-trigger", &response);
        }
        ui.add_space(theme::space::LG);
        ui.label("Right-click the surface for the production viewport action groups.");
        let response = ui.allocate_response(
            egui::vec2(
                ui.available_width().max(1.0),
                ui.available_height().max(40.0),
            ),
            egui::Sense::click(),
        );
        ui.painter().rect_stroke(
            response.rect,
            theme::radius::MD,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
        self.observe(instance, "context-surface", &response);
        menu::context(&response).show(|ui| {
            menu::content(ui, Control::ViewportMenu, |ui| {
                for &action in crate::ui::workspace_menus::CONTEXT_EDIT_ACTIONS {
                    self.action(ui, instance, action);
                }
                menu::separator(ui);
                for &action in crate::ui::workspace_menus::CONTEXT_VIEW_ACTIONS {
                    self.action(ui, instance, action);
                }
            })
        });
    }
    fn ruler(&self, ui: &Ui, bounds: Rect) {
        let content = ruler_2d::content_rect(bounds);
        let make_axis = |rect: Rect, start: f32, end: f32| {
            let mut axis = ruler_2d::AxisRuler {
                rect,
                ticks: Vec::new(),
                ranges: vec![ruler_2d::Span {
                    start: start + self.range[0],
                    end: start + self.range[1],
                }],
                origin: f64::from(start) + self.origin,
                pixels_per_unit: self.scale,
            };
            axis.ticks = axis.make_ticks(start, end);
            axis
        };
        let horizontal = make_axis(
            Rect::from_min_max(
                egui::pos2(content.left(), bounds.top()),
                content.right_top(),
            ),
            content.left(),
            content.right(),
        );
        let vertical = make_axis(
            Rect::from_min_max(
                egui::pos2(bounds.left(), content.top()),
                content.left_bottom(),
            ),
            content.top(),
            content.bottom(),
        );
        // Camera projection and geometry ranges stay with the viewport adapter.
        ruler_2d::paint(
            ui,
            &ruler_2d::Ruler2DModel {
                horizontal,
                vertical,
                unit: LengthUnit::Centimeters,
            },
        );
    }
    fn begin_frame(&mut self, ctx: &Context) {
        let frame = ctx.cumulative_frame_nr();
        if self.frame == Some(frame) {
            return;
        }
        self.frame = Some(frame);
        self.delivered_frame.clear();
        self.raw_events = ctx.input(|i| i.events.clone());
        if let Some(timeline) = &mut self.timeline {
            timeline.begin_frame(ctx, self.case.count());
        }
        if !self.case.pies() {
            return;
        }
        for index in 0..self.case.count() {
            let owner = self
                .instances
                .iter()
                .position(|instance| instance.pie.active());
            let bounds = self.instances[index].bounds;
            let commands = self.instances[index].pie.begin_in(
                ctx,
                bounds,
                bounds,
                PieContext {
                    keys_available: shortcuts::viewport_keys_available(ctx),
                    can_start: owner.is_none() || owner == Some(index),
                    hand_held: false,
                    fit_enabled: self.fit_enabled,
                    tool: Tool::View,
                },
            );
            for command in commands {
                self.queue(index, command);
            }
        }
    }
    fn ui(&mut self, root: &mut Ui) {
        let ctx = root.ctx().clone();
        if self.applied_appearance != Some(self.appearance) {
            theme::apply_context(&ctx, self.appearance, AccentColor::DEFAULT);
            self.applied_appearance = Some(self.appearance);
        }
        root.set_style(ctx.global_style());
        self.begin_frame(&ctx);
        let previous = std::mem::take(&mut self.observed);
        egui::Panel::top("workbench.header").show(root, |ui| {
            ui.horizontal(|ui| {
                ui.heading("N3 · UI workbench");
                ui.selectable_value(&mut self.appearance, ResolvedTheme::Light, "Light");
                ui.selectable_value(&mut self.appearance, ResolvedTheme::Dark, "Dark");
                let reset = ui.button("Reset case");
                self.observe(0, "reset", &reset);
                if reset.clicked() {
                    self.reset(&ctx);
                }
                ui.label("Available size");
                let width = ui.add(
                    egui::DragValue::new(&mut self.dimensions.x)
                        .range(160.0..=1400.0)
                        .prefix("W "),
                );
                self.observe(0, "available-width", &width);
                let height = ui.add(
                    egui::DragValue::new(&mut self.dimensions.y)
                        .range(100.0..=900.0)
                        .prefix("H "),
                );
                self.observe(0, "available-height", &height);
            });
        });
        egui::Panel::left("workbench.catalog")
            .exact_size(185.0)
            .show(root, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("workbench.catalog.scroll")
                    .show(ui, |ui| {
                        for page in ["Menus", "Held-key pies", "2D ruler", "Timeline", "Terminal"] {
                            ui.heading(page);
                            for case in Case::ALL.into_iter().filter(|case| case.page() == page) {
                                let response = ui.selectable_label(self.case == case, case.name());
                                if self.case == case && self.catalog_case != Some(case) {
                                    response.scroll_to_me(Some(egui::Align::Center));
                                }
                                if response.clicked() {
                                    self.select_case(&ctx, case);
                                }
                            }
                            ui.separator();
                        }
                        ui.small("Internal fixtures. Effects are recorded here; no document or settings are changed.");
                    });
            });
        self.catalog_case = Some(self.case);
        egui::Panel::right("workbench.inspector")
            .exact_size(250.0)
            .show(root, |ui| {
                ui.heading("Event / state inspector");
                ui.label(format!("Focus: {:?}", ctx.memory(|m| m.focused())));
                ui.label(format!("Popup open: {}", egui::Popup::is_any_open(&ctx)));
                if let Some(timeline) = &self.timeline {
                    timeline.inspector(ui, self.case.count());
                }
                if let Some(terminal) = &self.terminal {
                    terminal.inspector(ui, self.case.count());
                }
                for (index, instance) in self
                    .instances
                    .iter()
                    .enumerate()
                    .take(self.case.count())
                    .filter(|_| !self.case.terminal())
                {
                    ui.label(format!(
                        "Instance {}: pie={}, checked={}, {:?}",
                        index + 1,
                        instance.pie.active(),
                        instance.checked,
                        instance.shading
                    ));
                }
                ui.separator();
                ui.label("Delivered commands (last 32)");
                for event in self.events.iter().rev().take(8) {
                    ui.monospace(format!("{}: {:?}", event.instance + 1, event.command));
                }
                ui.separator();
                ui.label("Native input (current frame)");
                for event in self.raw_events.iter().take(4) {
                    ui.small(format!("{event:?}"));
                }
                ui.collapsing("Live identities (last pass)", |ui| {
                    for observed in &previous {
                        ui.small(format!(
                            "{} / {} · {:?} · enabled={} · {:?}",
                            observed.instance + 1,
                            observed.target,
                            observed.id,
                            observed.enabled,
                            observed.rect
                        ));
                    }
                });
            });
        egui::CentralPanel::default().show(root, |ui| {
            ui.heading(self.case.name());
            ui.horizontal_wrapped(|ui| {
                if self.case.pies() {
                    ui.checkbox(&mut self.fit_enabled, "Frame selection available");
                } else if !self.case.ruler() && !self.case.timeline() && !self.case.terminal() {
                    ui.checkbox(&mut self.disabled, "Disabled rows");
                    ui.checkbox(&mut self.unavailable, "Unavailable rows");
                    ui.checkbox(&mut self.long_label, "Long label");
                }
            });
            ui.horizontal(|ui| {
                ui.label("Competing text owner");
                let response = ui.text_edit_singleline(&mut self.text);
                self.observe(0, "owner-field", &response);
                if self.case.pies() || self.case.terminal() {
                    let popup = ui.menu_button("Competing popup", |ui| {
                        ui.label("This native popup owns input while open.");
                    });
                    self.observe(0, "owner-popup", &popup.response);
                }
            });
            if self.case.ruler() {
                ui.add(
                    egui::DragValue::new(&mut self.scale)
                        .range(0.001..=100.0)
                        .prefix("Pixels/unit "),
                );
                ui.add(egui::DragValue::new(&mut self.origin).prefix("Zero offset "));
                ui.horizontal(|ui| {
                    ui.label("Projected range (points)");
                    ui.add(egui::DragValue::new(&mut self.range[0]));
                    ui.add(egui::DragValue::new(&mut self.range[1]));
                });
            } else if self.case.pies() {
                let binding = bindings::required(if self.case == Case::ShadingPie {
                    "shading.pie"
                } else {
                    "view.pie"
                });
                ui.label(format!(
                    "Hold {} over a canvas; move and release to choose. Escape cancels.",
                    binding.label()
                ));
            } else if let Some(timeline) = &mut self.timeline {
                self.observed
                    .extend(timeline.controls(ui, self.case.count()));
            }
            if let Some(terminal) = &mut self.terminal {
                self.observed
                    .extend(terminal.controls(ui, self.case.count()));
            }
            egui::ScrollArea::both()
                .id_salt("workbench.available")
                .show(ui, |ui| {
                    let (outer, _) = ui.allocate_exact_size(self.dimensions, egui::Sense::hover());
                    for index in 0..self.case.count() {
                        let width = outer.width() / self.case.count() as f32;
                        let cell = Rect::from_min_size(
                            outer.min + egui::vec2(width * index as f32, 0.0),
                            egui::vec2(width, outer.height()),
                        );
                        let bounds = cell.intersect(ui.clip_rect());
                        self.instances[index].bounds = bounds;
                        let mut child = ui.new_child(
                            egui::UiBuilder::new()
                                .id_salt(instance_id(self.case, index))
                                .max_rect(cell.shrink(theme::space::XL))
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        child.set_clip_rect(bounds);
                        child.painter().rect_filled(
                            cell,
                            0.0,
                            theme::Palette::from_context(&ctx, AccentColor::DEFAULT)
                                .workbench_viewport,
                        );
                        if self.case.pies() {
                            let response = child.interact(
                                bounds,
                                instance_id(self.case, index).with("canvas"),
                                egui::Sense::click(),
                            );
                            if response.clicked() {
                                ctx.memory_mut(|m| m.request_focus(shortcuts::viewport_focus_id()));
                            }
                            self.observe(index, "pie-canvas", &response);
                            child.label(format!(
                                "Instance {} · hold over any edge or corner",
                                index + 1
                            ));
                        } else if let Some(timeline) = &mut self.timeline {
                            self.observed.extend(timeline.show(
                                &mut child,
                                instance_id(self.case, index),
                                index,
                            ));
                        } else if let Some(terminal) = &mut self.terminal {
                            self.observed.extend(terminal.show(&mut child, index));
                        } else if self.case.ruler() {
                            let response = child.interact(
                                bounds,
                                instance_id(self.case, index).with("canvas"),
                                egui::Sense::hover(),
                            );
                            self.observe(index, "ruler-canvas", &response);
                            self.ruler(&child, bounds);
                        } else {
                            self.menu_instance(&mut child, index);
                        }
                    }
                });
        });
        for instance in &self.instances {
            if instance.pie.active() {
                instance.pie.paint(&ctx, self.fit_enabled, instance.shading);
            }
        }
        self.deliver(&ctx);
        menu::finish_frame(&ctx);
    }
}
