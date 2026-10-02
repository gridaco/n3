//! Ordered temporary input ownership shared by held viewport pie menus.

// Replay and workbench inspection, without a browser runtime dependency.
#[cfg(not(target_arch = "wasm32"))]
mod inspection;
use super::bindings;
use crate::{
    editor::Tool,
    render::shading::ShadingMode,
    shortcuts::{self, Command},
    ui::{shading_pie, view_pie},
};
use egui::{Context, Event, Key, Modifiers, Pos2, Rect};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    View,
    Shading,
}
impl Kind {
    fn binding(self) -> bindings::Binding {
        bindings::required(match self {
            Self::View => "view.pie",
            Self::Shading => "shading.pie",
        })
    }
    fn for_key(key: Key) -> Option<Self> {
        [Self::View, Self::Shading]
            .into_iter()
            .find(|kind| kind.binding().key() == Some(key))
    }
    fn eligible(self, tool: Tool) -> bool {
        // Z is exclusively an axis lock in every transform tool, even with
        // an empty selection. There is no tap/hold ambiguity or time threshold.
        self != Self::Shading || tool == Tool::View
    }
}

fn tool_key(key: Key, modifiers: Modifiers) -> Option<Tool> {
    bindings::BINDINGS
        .iter()
        .find_map(|binding| match binding.command {
            Some(Command::Tool(tool)) if binding.matches_key(key, modifiers) => Some(tool),
            _ => None,
        })
}

/// Reserve native gestures queued before the UI processes the menu trigger.
/// Predict preceding tool keys with the same binding catalog as normal routing.
pub fn pending_trigger(mut tool: Tool, events: &[Event]) -> bool {
    events.iter().any(|event| {
        if let Event::Key {
            key,
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } = event
        {
            if let Some(next) = tool_key(*key, *modifiers) {
                tool = next;
            }
            return Kind::for_key(*key).is_some_and(|kind| {
                kind.eligible(tool) && kind.binding().matches_key(*key, *modifiers)
            });
        }
        false
    })
}

enum ActivePie {
    View(view_pie::Pie),
    Shading(shading_pie::Pie),
}
impl ActivePie {
    fn open(
        ctx: &Context,
        kind: Kind,
        point: Pos2,
        bounds: Rect,
        instance: Option<egui::Id>,
    ) -> Option<Self> {
        match kind {
            Kind::View => match instance {
                Some(id) => view_pie::Pie::open_with_id(ctx, id.with("view"), point, bounds),
                None => view_pie::Pie::open(ctx, point, bounds),
            }
            .map(Self::View),
            Kind::Shading => match instance {
                Some(id) => shading_pie::Pie::open_with_id(ctx, id.with("shading"), point, bounds),
                None => shading_pie::Pie::open(ctx, point, bounds),
            }
            .map(Self::Shading),
        }
    }
    fn kind(&self) -> Kind {
        match self {
            Self::View(_) => Kind::View,
            Self::Shading(_) => Kind::Shading,
        }
    }
    fn command(&self, point: Pos2, fit_enabled: bool) -> Option<Command> {
        match self {
            Self::View(pie) => pie.hovered(point, fit_enabled).map(|a| a.command()),
            Self::Shading(pie) => pie.hovered(point).map(|a| a.command()),
        }
    }
}

pub struct PieContext {
    pub keys_available: bool,
    pub can_start: bool,
    pub hand_held: bool,
    pub fit_enabled: bool,
    pub tool: Tool,
}

#[derive(Default)]
pub struct PieInput {
    instance: Option<egui::Id>,
    pie: Option<ActivePie>,
    pointer: Option<Pos2>,
    buttons: [bool; 5],
    suppress_buttons: bool,
    pub owns_frame: bool,
    screen: Option<Rect>,
}

impl PieInput {
    pub fn active(&self) -> bool {
        self.pie.is_some()
    }
    pub fn reset(&mut self) {
        self.pie = None;
        self.pointer = None;
        self.buttons = [false; 5];
        self.suppress_buttons = false;
        self.owns_frame = false;
        self.screen = None;
    }

    /// First pass only. A hold/move/release can arrive in one native batch.
    /// Tool changes before an opened pie are carried with its command because
    /// claiming the frame intentionally suppresses the normal shortcut collector.
    pub fn begin(&mut self, ctx: &Context, viewport: Rect, context: PieContext) -> Vec<Command> {
        self.begin_with_pointer_policy(
            ctx,
            viewport,
            ctx.content_rect(),
            context,
            crate::navigation_events::viewport_accepts_pointer,
        )
    }

    fn begin_with_pointer_policy(
        &mut self,
        ctx: &Context,
        viewport: Rect,
        bounds: Rect,
        context: PieContext,
        accepts_pointer: fn(&Context, Rect, Pos2) -> bool,
    ) -> Vec<Command> {
        let PieContext {
            keys_available,
            can_start,
            mut hand_held,
            fit_enabled,
            mut tool,
        } = context;
        self.owns_frame = self.active() || self.suppress_buttons;
        let available = ctx.input(|input| input.focused)
            && !egui::Popup::is_any_open(ctx)
            && !ctx.memory(|memory| memory.top_modal_layer().is_some());
        let resized = self.screen.is_some_and(|screen| screen != bounds);
        self.screen = Some(bounds);
        if !available || resized {
            self.pie = None;
        }
        let mut command = None;
        let mut preceding_tool = None;
        let mut adopted_tool = None;
        for event in ctx.input(|input| input.events.clone()) {
            match event {
                Event::PointerMoved(pos) => self.pointer = Some(pos),
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    ..
                } => {
                    self.pointer = Some(pos);
                    self.buttons[button as usize] = pressed;
                    if self.owns_frame && pressed {
                        self.suppress_buttons = true;
                    }
                }
                Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } if Some(key) == bindings::required("navigation.pan").key() => {
                    if !pressed {
                        hand_held = false;
                    } else if !repeat
                        && bindings::required("navigation.pan").matches_key(key, modifiers)
                        && keys_available
                    {
                        hand_held = true;
                    }
                }
                Event::Key {
                    key,
                    pressed,
                    repeat,
                    modifiers,
                    ..
                } if Kind::for_key(key).is_some() => {
                    let kind = Kind::for_key(key).unwrap();
                    if !pressed {
                        if self.pie.as_ref().is_some_and(|pie| pie.kind() == kind) {
                            let pie = self.pie.take().unwrap();
                            command = self
                                .pointer
                                .and_then(|point| pie.command(point, fit_enabled));
                        }
                    } else if !repeat
                        && kind.binding().matches_key(key, modifiers)
                        && kind.eligible(tool)
                        && available
                        && !resized
                        && keys_available
                        && can_start
                        && !hand_held
                        && !self.owns_frame
                        && !self.buttons.iter().any(|down| *down)
                        && let Some(point) = self
                            .pointer
                            .filter(|point| accepts_pointer(ctx, viewport, *point))
                    {
                        // Opening belongs to the viewport, but the held pie may
                        // extend over panels within this window's content area.
                        self.pie = ActivePie::open(ctx, kind, point, bounds, self.instance);
                        self.owns_frame = self.active();
                        if self.owns_frame {
                            adopted_tool = preceding_tool;
                        }
                    }
                }
                Event::Key {
                    key, pressed: true, ..
                } if self.owns_frame && Some(key) == bindings::required("cancel").key() => {
                    self.pie = None;
                    command = None;
                }
                Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if !self.owns_frame && keys_available && can_start && available => {
                    if let Some(next) = tool_key(key, modifiers) {
                        tool = next;
                        preceding_tool = Some(next);
                    }
                }
                Event::PointerGone | Event::WindowFocused(false) => {
                    // An explicit reset releases all transient state. A
                    // cancellation inside an owned event batch still reserves
                    // that batch so its remaining events cannot reach tools.
                    let owns_frame = self.owns_frame;
                    self.reset();
                    self.owns_frame = owns_frame;
                    command = None;
                    adopted_tool = None;
                }
                _ => {}
            }
        }
        self.suppress_buttons &= self.buttons.iter().any(|down| *down);
        if self.owns_frame {
            shortcuts::claim_viewport_input(ctx);
            ctx.stop_dragging();
        }
        adopted_tool
            .map(Command::Tool)
            .into_iter()
            .chain(command)
            .collect()
    }

    pub fn paint(&self, ctx: &Context, fit_enabled: bool, shading: ShadingMode) {
        match &self.pie {
            Some(ActivePie::View(pie)) => pie.paint(ctx, self.pointer, fit_enabled),
            Some(ActivePie::Shading(pie)) => pie.paint(ctx, self.pointer, shading),
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Key;

    fn binding_event(id: &str, pressed: bool) -> Event {
        let binding = bindings::required(id);
        Event::Key {
            key: binding.key().unwrap(),
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: binding.modifiers,
        }
    }

    fn fixture_context() -> PieContext {
        PieContext {
            keys_available: true,
            can_start: true,
            hand_held: false,
            fit_enabled: true,
            tool: Tool::View,
        }
    }

    fn frame(ctx: &Context, screen: Rect, events: Vec<Event>, draw: impl FnMut(&mut egui::Ui)) {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                focused: true,
                events,
                ..Default::default()
            },
            draw,
        )
        .textures_delta
        .clear();
    }

    #[test]
    fn isolated_hosts_route_the_real_binding_to_one_instance_and_deliver_release_once() {
        let ctx = Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0));
        let bounds = [
            Rect::from_min_size(Pos2::ZERO, egui::vec2(580.0, 800.0)),
            Rect::from_min_size(Pos2::new(620.0, 0.0), egui::vec2(580.0, 800.0)),
        ];
        let mut hosts = [
            PieInput::for_instance(egui::Id::new("pie-host-left")),
            PieInput::for_instance(egui::Id::new("pie-host-right")),
        ];
        frame(&ctx, screen, vec![], |_| {});
        frame(
            &ctx,
            screen,
            vec![
                Event::PointerMoved(bounds[1].center()),
                binding_event("view.pie", true),
            ],
            |ui| {
                for (host, rect) in hosts.iter_mut().zip(bounds) {
                    assert!(
                        host.begin_in(ui.ctx(), rect, rect, fixture_context())
                            .is_empty()
                    );
                }
            },
        );
        assert!(!hosts[0].active() && !hosts[0].owns_frame);
        assert!(hosts[1].view_active() && hosts[1].owns_frame);
        let target = match hosts[1].pie.as_ref().unwrap() {
            ActivePie::View(pie) => pie.item_rect(view_pie::Action::Front).center(),
            _ => unreachable!(),
        };
        let mut commands = Vec::new();
        frame(
            &ctx,
            screen,
            vec![
                Event::PointerMoved(target),
                binding_event("view.pie", false),
            ],
            |ui| {
                for (host, rect) in hosts.iter_mut().zip(bounds) {
                    commands.extend(host.begin_in(ui.ctx(), rect, rect, fixture_context()));
                }
                // The host processes ordered input only on the first layout
                // pass; painting can retry without issuing another command.
                hosts[1].paint(ui.ctx(), true, ShadingMode::Solid);
                hosts[1].paint(ui.ctx(), true, ShadingMode::Solid);
            },
        );
        assert_eq!(commands, vec![view_pie::Action::Front.command()]);
        assert!(!hosts.iter().any(PieInput::active));
        frame(&ctx, screen, vec![], |ui| {
            for (host, rect) in hosts.iter_mut().zip(bounds) {
                assert!(
                    host.begin_in(ui.ctx(), rect, rect, fixture_context())
                        .is_empty()
                );
            }
        });
        assert!(!hosts.iter().any(|host| host.owns_frame));
    }

    #[test]
    fn component_bounds_resize_cancels_pending_choice_before_release() {
        let ctx = Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let original = Rect::from_min_size(Pos2::new(50.0, 50.0), egui::vec2(800.0, 650.0));
        let smaller = Rect::from_min_size(original.min, egui::vec2(600.0, 400.0));
        let id = egui::Id::new("resizable-pie");
        let mut host = PieInput::for_instance(id);
        frame(&ctx, screen, vec![], |_| {});
        frame(
            &ctx,
            screen,
            vec![
                Event::PointerMoved(original.center()),
                binding_event("view.pie", true),
            ],
            |ui| {
                assert!(
                    host.begin_in(ui.ctx(), original, original, fixture_context())
                        .is_empty()
                )
            },
        );
        assert!(host.active());
        let target = match host.pie.as_ref().unwrap() {
            ActivePie::View(pie) => pie.item_rect(view_pie::Action::Right).center(),
            _ => unreachable!(),
        };
        frame(
            &ctx,
            screen,
            vec![
                Event::PointerMoved(target),
                binding_event("view.pie", false),
            ],
            |ui| {
                assert!(
                    host.begin_in(ui.ctx(), smaller, smaller, fixture_context())
                        .is_empty()
                )
            },
        );
        assert!(!host.active());
        assert!(
            host.owns_frame,
            "The cancelled release still belongs to the pie"
        );
        host.reset();
        assert_eq!(host.instance, Some(id));
        assert!(!host.owns_frame);
        frame(
            &ctx,
            screen,
            vec![
                Event::PointerMoved(original.center()),
                binding_event("view.pie", true),
            ],
            |ui| {
                assert!(
                    host.begin_in(ui.ctx(), original, original, fixture_context())
                        .is_empty()
                )
            },
        );
        assert!(host.active(), "Reset allows reopening at new dimensions");
        frame(
            &ctx,
            screen,
            vec![
                Event::WindowFocused(false),
                binding_event("view.pie", false),
            ],
            |ui| {
                assert!(
                    host.begin_in(ui.ctx(), original, original, fixture_context())
                        .is_empty()
                )
            },
        );
        assert!(!host.active());
        assert!(
            host.owns_frame,
            "Focus loss keeps the cancelled batch owned"
        );
    }

    #[test]
    fn isolated_pie_host_has_no_phantom_viewport_gizmo_hit_region() {
        let ctx = Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 800.0));
        let point = crate::axis_gizmo::bounds(screen).center();
        let mut editor_host = PieInput::default();
        let mut component_host = PieInput::for_instance(egui::Id::new("corner-pie"));
        frame(&ctx, screen, vec![], |_| {});
        frame(
            &ctx,
            screen,
            vec![Event::PointerMoved(point), binding_event("view.pie", true)],
            |ui| {
                assert!(
                    editor_host
                        .begin(ui.ctx(), screen, fixture_context())
                        .is_empty()
                );
                assert!(
                    component_host
                        .begin_in(ui.ctx(), screen, screen, fixture_context())
                        .is_empty()
                );
            },
        );
        assert!(
            !editor_host.active(),
            "The editor reserves the actual axis gizmo"
        );
        assert!(
            component_host.active(),
            "An isolated component has no axis gizmo"
        );
    }

    #[test]
    fn queued_trigger_reserves_gestures_until_the_ui_processes_the_batch() {
        let press = Event::Key {
            key: Key::Backtick,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut release = press.clone();
        if let Event::Key { pressed, .. } = &mut release {
            *pressed = false;
        }
        assert!(pending_trigger(
            Tool::View,
            &[press.clone(), release.clone()]
        ));
        assert!(!pending_trigger(Tool::View, &[release]));
        let mut modified = press;
        if let Event::Key { modifiers, .. } = &mut modified {
            *modifiers = egui::Modifiers::SHIFT;
        }
        assert!(!pending_trigger(Tool::View, &[modified]));
        assert!(!pending_trigger(Tool::View, &[]));
    }

    #[test]
    fn eligible_pan_hold_suppresses_pie_in_the_same_batch() {
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let pan = bindings::required("navigation.pan");
        let pie = bindings::required("view.pie");
        // Space retains priority when Option is also held. The pie router must
        // use the same eligibility policy as temporary navigation, including
        // when Option is released before Backtick in one native event batch.
        for pan_modifiers in [
            None,
            Some(pan.modifiers),
            Some(pan.modifiers | egui::Modifiers::ALT),
        ] {
            let ctx = Context::default();
            let mut input = PieInput::default();
            let raw = egui::RawInput {
                screen_rect: Some(viewport),
                focused: true,
                ..Default::default()
            };
            ctx.run_ui(raw.clone(), |_| {}).textures_delta.clear();
            let mut events = vec![Event::PointerMoved(viewport.center())];
            if let Some(modifiers) = pan_modifiers {
                events.push(Event::Key {
                    key: pan.key().unwrap(),
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                });
                events.push(Event::ModifiersChanged(pie.modifiers));
            }
            events.push(Event::Key {
                key: pie.key().unwrap(),
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: pie.modifiers,
            });
            ctx.run_ui(egui::RawInput { events, ..raw }, |ui| {
                input.begin(
                    ui.ctx(),
                    viewport,
                    PieContext {
                        keys_available: true,
                        can_start: true,
                        hand_held: false,
                        fit_enabled: true,
                        tool: Tool::View,
                    },
                );
            })
            .textures_delta
            .clear();
            assert_eq!(input.pie.is_some(), pan_modifiers.is_none());
        }
    }
}
