//! Composable transport widgets emit requests; accepted playback stays with the host.
use super::data::{Capabilities, HostState, Request};
use egui::{Id, Rect, Response, Ui};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransportControl {
    PlayPause,
    Loop,
    Speed,
    Time,
    Fit,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransportObservation {
    pub control: TransportControl,
    pub id: Id,
    pub rect: Rect,
    pub enabled: bool,
}

#[derive(Default)]
pub(crate) struct TransportOutput {
    pub requests: Vec<Request>,
    pub observations: Vec<TransportObservation>,
    /// Fitting is a presentation change, rather than a playback host request.
    pub fit: bool,
}

impl TransportOutput {
    fn observe(&mut self, control: TransportControl, response: &Response) {
        self.observations.push(TransportObservation {
            control,
            id: response.id,
            rect: response.rect,
            enabled: response.enabled(),
        });
    }
}

pub(crate) fn show(ui: &mut Ui, id: Id, host: &HostState, cap: &Capabilities) -> TransportOutput {
    let mut output = TransportOutput::default();
    let available = host.valid();
    ui.scope_builder(egui::UiBuilder::new().id(id), |ui| {
        // The same native controls compose into a compact shared header. Their
        // fixed control order and explicit instance ID preserve identities when wrapping.
        ui.style_mut().override_font_id =
            Some(egui::FontId::proportional(crate::theme::text::SMALL_UI_11));
        ui.spacing_mut().button_padding = egui::vec2(4.0, 2.0);
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        ui.spacing_mut().interact_size.y = 20.0;
        ui.horizontal_wrapped(|ui| {
            // The bundled icon vocabulary has no transport icons yet. Native
            // text buttons keep labels readable and preserve accessibility.
            let play = ui.add_enabled(
                available && cap.play_pause,
                egui::Button::new(if host.playing { "Pause" } else { "Play" }),
            );
            output.observe(TransportControl::PlayPause, &play);
            if play.clicked() {
                output.requests.push(Request::SetPlaying(!host.playing));
            }

            let mut looping = host.looping;
            let looping_response = ui.add_enabled(
                available && cap.looping,
                egui::Checkbox::new(&mut looping, "Loop"),
            );
            output.observe(TransportControl::Loop, &looping_response);
            if looping_response.changed() {
                output.requests.push(Request::SetLooping(looping));
            }

            let mut speed = host.speed;
            let speed_response = ui
                .add_enabled(
                    available && cap.speed,
                    egui::DragValue::new(&mut speed)
                        .speed(0.01)
                        .max_decimals(6)
                        .suffix("×"),
                )
                .on_hover_text("Playback speed · positive multiplier");
            name_value(&speed_response, "Playback speed", ui.ctx());
            output.observe(TransportControl::Speed, &speed_response);
            if speed_response.changed() && speed.is_finite() && speed > 0.0 {
                output.requests.push(Request::SetSpeed(speed));
            }

            let mut time = host.accepted_time;
            let time_response = ui
                .add_enabled(
                    available && cap.seek,
                    egui::DragValue::new(&mut time)
                        .speed(0.01)
                        .max_decimals(6)
                        .suffix(" s"),
                )
                .on_hover_text("Accepted current time in seconds");
            name_value(&time_response, "Current time in seconds", ui.ctx());
            output.observe(TransportControl::Time, &time_response);
            if time_response.changed() && time.is_finite() {
                output.requests.push(Request::Seek { time });
            }

            let fit = ui
                .add_enabled(available, egui::Button::new("Fit"))
                .on_hover_text("Fit the timeline content range");
            output.observe(TransportControl::Fit, &fit);
            output.fit = fit.clicked();
        });
    });
    // Native widgets may report the same activation again on a discarded
    // layout pass. Gate by the instance and logical frame, never by value: two
    // successive scrubs or seeks at the same time remain distinct requests.
    if !output.requests.is_empty() || output.fit {
        let frame = ui.ctx().cumulative_frame_nr();
        let delivery = id.with("transport.delivered_frame");
        let already_delivered = ui.ctx().data_mut(|data| {
            if data.get_temp::<u64>(delivery) == Some(frame) {
                true
            } else {
                data.insert_temp(delivery, frame);
                false
            }
        });
        if already_delivered {
            output.requests.clear();
            output.fit = false;
        }
    }
    output
}

fn name_value(response: &Response, label: &str, ctx: &egui::Context) {
    // Removing a redundant visual prefix must not remove the accessible name.
    // Preserve native roles, edit-buffer text, selection and output events.
    ctx.accesskit_node_builder(response.id, |node| {
        node.set_label(label);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Harness {
        ctx: egui::Context,
        host: HostState,
        cap: Capabilities,
        output: TransportOutput,
        retry: bool,
        id: Id,
    }

    impl Harness {
        fn new() -> Self {
            let ctx = egui::Context::default();
            crate::ui::workspace_ui::configure_context(&ctx);
            Self {
                ctx,
                host: HostState::default(),
                cap: Capabilities::default(),
                output: TransportOutput::default(),
                retry: false,
                id: Id::new("transport.test"),
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> Vec<Request> {
            let mut requests = Vec::new();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 100.0),
                )),
                events,
                ..Default::default()
            };
            let mut output = self.ctx.run_ui(input, |ui| {
                self.output = show(ui, self.id, &self.host, &self.cap);
                requests.extend(self.output.requests.clone());
                if self.retry && ui.ctx().current_pass_index() == 0 {
                    ui.ctx().request_discard("transport retry regression");
                }
            });
            output.textures_delta.clear();
            requests
        }

        fn control(&self, control: TransportControl) -> TransportObservation {
            *self
                .output
                .observations
                .iter()
                .find(|observation| observation.control == control)
                .unwrap()
        }

        fn click(&mut self, control: TransportControl) -> Vec<Request> {
            let pos = self.control(control).rect.center();
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            self.frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }])
        }
    }

    #[test]
    fn transport_requests_use_accepted_host_state_and_deduplicate_layout_retries() {
        let mut harness = Harness::new();
        harness.frame(Vec::new());
        harness.retry = true;
        assert_eq!(
            harness.click(TransportControl::PlayPause),
            vec![Request::SetPlaying(true)]
        );
        assert!(
            !harness.host.playing,
            "A presentation widget cannot accept its own request."
        );
        // The same request in another logical frame is a valid new intent.
        assert_eq!(
            harness.click(TransportControl::PlayPause),
            vec![Request::SetPlaying(true)]
        );
        harness.host.playing = true;
        harness.frame(Vec::new());
        assert_eq!(
            harness.click(TransportControl::PlayPause),
            vec![Request::SetPlaying(false)]
        );
    }

    #[test]
    fn unavailable_invalid_and_capability_disabled_controls_do_not_emit() {
        let mut harness = Harness::new();
        for host in [
            HostState {
                available: false,
                ..HostState::default()
            },
            HostState {
                accepted_time: f64::NAN,
                ..HostState::default()
            },
            HostState {
                speed: 0.0,
                ..HostState::default()
            },
        ] {
            harness.host = host;
            harness.frame(Vec::new());
            assert!(
                harness
                    .output
                    .observations
                    .iter()
                    .all(|control| !control.enabled)
            );
            assert!(harness.click(TransportControl::PlayPause).is_empty());
        }
        harness.host = HostState::default();
        harness.cap.play_pause = false;
        harness.frame(Vec::new());
        assert!(!harness.control(TransportControl::PlayPause).enabled);
        assert!(harness.control(TransportControl::Loop).enabled);
        assert!(harness.click(TransportControl::PlayPause).is_empty());
    }

    #[test]
    fn fractional_negative_time_is_not_rounded_by_passive_layout() {
        let mut harness = Harness::new();
        harness.host.accepted_time = -0.123_456_789_123;
        harness.host.speed = 1.234_567_891_23;
        for _ in 0..3 {
            assert!(harness.frame(Vec::new()).is_empty());
            assert_eq!(harness.host.accepted_time, -0.123_456_789_123);
            assert_eq!(harness.host.speed, 1.234_567_891_23);
        }
        let time = harness.control(TransportControl::Time);
        harness
            .ctx
            .memory_mut(|memory| memory.request_focus(time.id));
        let requests = harness.frame(vec![
            egui::Event::Key {
                key: egui::Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND,
            },
            egui::Event::Text("-0.375".into()),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        assert_eq!(requests, vec![Request::Seek { time: -0.375 }]);
    }

    #[test]
    fn compact_fields_keep_accessible_names_and_native_text_entry_buffers() {
        let ctx = egui::Context::default();
        crate::ui::workspace_ui::configure_context(&ctx);
        ctx.enable_accesskit();
        let host = HostState::default();
        let cap = Capabilities::default();
        let mut controls = TransportOutput::default();
        let render = |events: Vec<egui::Event>, controls: &mut TransportOutput| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    *controls = show(ui, Id::new("transport.accessibility"), &host, &cap);
                },
            );
            output.textures_delta.clear();
            output
        };
        let output = render(Vec::new(), &mut controls);
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        for (control, label, value) in [
            (TransportControl::Speed, "Playback speed", 1.0),
            (TransportControl::Time, "Current time in seconds", 0.0),
        ] {
            let id = controls
                .observations
                .iter()
                .find(|o| o.control == control)
                .unwrap()
                .id;
            let node = &tree
                .nodes
                .iter()
                .find(|(node, _)| *node == id.accesskit_id())
                .unwrap()
                .1;
            assert_eq!(node.label(), Some(label));
            assert_eq!(node.numeric_value(), Some(value));
        }
        let time = controls
            .observations
            .iter()
            .find(|o| o.control == TransportControl::Time)
            .unwrap()
            .id;
        ctx.memory_mut(|memory| memory.request_focus(time));
        let output = render(
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                },
                egui::Event::Text("-".into()),
            ],
            &mut controls,
        );
        let tree = output.platform_output.accesskit_update.as_ref().unwrap();
        let node = &tree
            .nodes
            .iter()
            .find(|(node, _)| *node == time.accesskit_id())
            .unwrap()
            .1;
        assert_eq!(node.label(), Some("Current time in seconds"));
        assert_eq!(
            node.value(),
            Some("-"),
            "Native edit text must not be replaced by accepted numeric time"
        );
    }
}
