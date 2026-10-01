//! Keyboard ownership for N3's animation panel. The reusable timeline kit does
//! not know application bindings; this host routes its canonical semantic keys.
use super::{
    bindings::{self, Scope, Trigger},
    shortcuts::{self, Command},
};
use egui::{Context, Event, Key};

pub(crate) fn timeline_id() -> egui::Id {
    egui::Id::new("n3.animation.timeline")
}

pub(crate) fn timeline_focus_id() -> egui::Id {
    timeline_id().with("canvas")
}

#[derive(Clone, Copy)]
pub(crate) struct TimelineKeyboardContext {
    /// The panel is visible, including its empty or unavailable state.
    pub active: bool,
    /// The current source and editor/gesture state can accept playback input.
    pub ready: bool,
}

#[derive(Default)]
pub(crate) struct TimelineKeyboard {
    frame: Option<u64>,
    down: Vec<Key>,
    captured: Vec<Key>,
    /// Retained through the release frame, even if focus or the source changes.
    pub owns_frame: bool,
}

impl TimelineKeyboard {
    /// Run before viewport held-key routing. The parent retains this state when
    /// replacing sources/documents and claims animation input on `owns_frame`.
    /// Raw event positions stay unchanged for native physical-key metadata.
    pub fn route(&mut self, ctx: &Context, context: TimelineKeyboardContext) -> Vec<Command> {
        let frame = ctx.cumulative_frame_nr();
        if self.frame == Some(frame) {
            return Vec::new();
        }
        self.frame = Some(frame);
        self.owns_frame = !self.captured.is_empty();
        let owner = context.active
            && ctx.memory(|memory| memory.has_focus(timeline_focus_id()))
            && !egui::Popup::is_any_open(ctx)
            && !ctx.memory(|memory| memory.top_modal_layer().is_some())
            // Routing precedes widgets, so a pointer press may be transferring
            // focus into a field/menu in this batch. Let that pointer establish
            // the new owner before accepting another timeline shortcut.
            && !ctx.input(|input| input.pointer.any_pressed());
        let (window_focused, events) = ctx.input(|input| (input.focused, input.events.clone()));
        let mut focused = window_focused;
        let mut focus_boundary = false;
        let mut commands = Vec::new();
        for event in events {
            if let Event::WindowFocused(value) = event {
                focused = value;
                if !focused {
                    // A blur later in the same native batch cancels earlier
                    // commands. A repeat after refocus is never a new press.
                    commands.clear();
                    focus_boundary = true;
                    self.down.clear();
                    self.captured.clear();
                }
                continue;
            }
            let Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if !bindings::BINDINGS
                .iter()
                .any(|binding| binding.scope == Scope::Timeline && binding.key() == Some(key))
            {
                continue;
            }
            let captured = self.captured.contains(&key);
            self.owns_frame |= captured;
            if !pressed {
                self.down.retain(|down| *down != key);
                self.captured.retain(|down| *down != key);
                continue;
            }
            if self.down.contains(&key) {
                continue;
            }
            self.down.push(key);
            if !focused || !window_focused || focus_boundary || !owner || repeat {
                continue;
            }
            if let Some(binding) = bindings::BINDINGS.iter().find(|binding| {
                binding.scope == Scope::Timeline
                    && binding.trigger == Trigger::Press
                    && binding.matches_key(key, modifiers)
            }) {
                // An unavailable source remains an inert timeline command;
                // it must not turn the same Space into viewport panning.
                self.captured.push(key);
                self.owns_frame = true;
                if context.ready
                    && let Some(command) = binding.command
                {
                    commands.push(command);
                }
            }
        }
        if !window_focused {
            commands.clear();
            self.down.clear();
            self.captured.clear();
        }
        if self.owns_frame {
            shortcuts::suppress_viewport_input(ctx);
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, RawInput};

    const READY: TimelineKeyboardContext = TimelineKeyboardContext {
        active: true,
        ready: true,
    };

    fn key(pressed: bool, repeat: bool, modifiers: Modifiers) -> Event {
        Event::Key {
            key: bindings::required("animation.play-pause").key().unwrap(),
            physical_key: None,
            pressed,
            repeat,
            modifiers,
        }
    }

    fn frame(
        router: &mut TimelineKeyboard,
        ctx: &Context,
        context: TimelineKeyboardContext,
        events: Vec<Event>,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        ctx.run_ui(
            RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.interact(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)),
                    timeline_focus_id(),
                    egui::Sense::click(),
                );
                commands.extend(router.route(ui.ctx(), context));
                if router.owns_frame {
                    assert!(shortcuts::viewport_input_claimed(ui.ctx()));
                }
            },
        )
        .textures_delta
        .clear();
        commands
    }

    #[test]
    fn focused_timeline_press_is_once_per_physical_press_and_release_is_owned() {
        let ctx = Context::default();
        ctx.memory_mut(|memory| memory.request_focus(timeline_focus_id()));
        let mut router = TimelineKeyboard::default();
        assert_eq!(
            frame(
                &mut router,
                &ctx,
                READY,
                vec![key(true, false, Modifiers::NONE)]
            ),
            vec![Command::ToggleAnimationPlayback]
        );
        for repeat in [true, false] {
            assert!(
                frame(
                    &mut router,
                    &ctx,
                    READY,
                    vec![key(true, repeat, Modifiers::NONE)]
                )
                .is_empty()
            );
            assert!(router.owns_frame);
        }
        assert!(
            frame(
                &mut router,
                &ctx,
                READY,
                vec![key(false, false, Modifiers::NONE)]
            )
            .is_empty()
        );
        assert!(router.owns_frame);
        assert!(frame(&mut router, &ctx, READY, Vec::new()).is_empty());
        assert!(!router.owns_frame);
        assert_eq!(
            frame(
                &mut router,
                &ctx,
                READY,
                vec![key(true, false, Modifiers::NONE)]
            ),
            vec![Command::ToggleAnimationPlayback]
        );
    }

    #[test]
    fn timeline_hold_and_release_cannot_migrate_to_viewport_after_panel_closes() {
        let ctx = Context::default();
        ctx.memory_mut(|memory| memory.request_focus(timeline_focus_id()));
        let mut router = TimelineKeyboard::default();
        frame(
            &mut router,
            &ctx,
            READY,
            vec![key(true, false, Modifiers::NONE)],
        );
        ctx.memory_mut(|memory| memory.request_focus(shortcuts::viewport_focus_id()));
        let closed = TimelineKeyboardContext {
            active: false,
            ready: false,
        };
        for events in [
            Vec::new(),
            vec![key(true, true, Modifiers::NONE)],
            vec![key(false, false, Modifiers::NONE)],
        ] {
            assert!(frame(&mut router, &ctx, closed, events).is_empty());
            assert!(router.owns_frame);
        }
        frame(&mut router, &ctx, closed, Vec::new());
        assert!(!router.owns_frame);
    }

    #[test]
    fn unavailable_timeline_owns_space_without_starting_playback() {
        let ctx = Context::default();
        ctx.memory_mut(|memory| memory.request_focus(timeline_focus_id()));
        let mut router = TimelineKeyboard::default();
        assert!(
            frame(
                &mut router,
                &ctx,
                TimelineKeyboardContext {
                    active: true,
                    ready: false
                },
                vec![key(true, false, Modifiers::NONE)],
            )
            .is_empty()
        );
        assert!(router.owns_frame);
        assert!(
            frame(
                &mut router,
                &ctx,
                READY,
                vec![key(true, true, Modifiers::NONE)]
            )
            .is_empty()
        );
    }

    #[test]
    fn other_focus_hidden_panel_and_modified_keys_do_not_play() {
        for (focus, context, modifiers) in [
            (shortcuts::viewport_focus_id(), READY, Modifiers::NONE),
            (egui::Id::new("text-field"), READY, Modifiers::NONE),
            (
                timeline_focus_id(),
                TimelineKeyboardContext {
                    active: false,
                    ready: true,
                },
                Modifiers::NONE,
            ),
            (timeline_focus_id(), READY, Modifiers::SHIFT),
            (timeline_focus_id(), READY, Modifiers::MAC_CMD),
            (timeline_focus_id(), READY, Modifiers::ALT),
        ] {
            let ctx = Context::default();
            ctx.memory_mut(|memory| memory.request_focus(focus));
            let mut router = TimelineKeyboard::default();
            assert!(
                frame(
                    &mut router,
                    &ctx,
                    context,
                    vec![key(true, false, modifiers)]
                )
                .is_empty()
            );
            assert!(!router.owns_frame);
        }
    }

    #[test]
    fn press_before_timeline_focus_does_not_become_playback_when_held() {
        let ctx = Context::default();
        ctx.memory_mut(|memory| memory.request_focus(shortcuts::viewport_focus_id()));
        let mut router = TimelineKeyboard::default();
        assert!(
            frame(
                &mut router,
                &ctx,
                READY,
                vec![key(true, false, Modifiers::NONE)]
            )
            .is_empty()
        );
        ctx.memory_mut(|memory| memory.request_focus(timeline_focus_id()));
        assert!(
            frame(
                &mut router,
                &ctx,
                READY,
                vec![key(true, true, Modifiers::NONE)]
            )
            .is_empty()
        );
        assert!(!router.owns_frame);
    }

    #[test]
    fn popup_modal_and_pointer_focus_transfer_keep_their_input() {
        for kind in 0..3 {
            let ctx = Context::default();
            if kind == 0 {
                egui::Popup::open_id(&ctx, egui::Id::new("timeline-keyboard-popup"));
            } else if kind == 1 {
                ctx.run_ui(RawInput::default(), |ui| {
                    egui::Modal::new(egui::Id::new("timeline-keyboard-modal")).show(
                        ui.ctx(),
                        |ui| {
                            ui.label("Modal owns input");
                        },
                    );
                })
                .textures_delta
                .clear();
                assert!(ctx.memory(|memory| memory.top_modal_layer().is_some()));
            }
            ctx.memory_mut(|memory| memory.request_focus(timeline_focus_id()));
            let mut events = vec![key(true, false, Modifiers::NONE)];
            if kind == 2 {
                events.push(Event::PointerButton {
                    pos: egui::pos2(300.0, 200.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::NONE,
                });
            }
            let mut router = TimelineKeyboard::default();
            assert!(
                frame(&mut router, &ctx, READY, events).is_empty(),
                "kind={kind}"
            );
            assert!(!router.owns_frame, "kind={kind}");
        }
    }

    #[test]
    fn layout_retry_does_not_repeat_playback_or_steal_timeline_focus() {
        let ctx = Context::default();
        let mut shortcuts = shortcuts::ShortcutFrame::new(&ctx);
        ctx.memory_mut(|memory| memory.request_focus(timeline_focus_id()));
        let mut router = TimelineKeyboard::default();
        let mut commands = Vec::new();
        let mut passes = 0;
        let mut focus_preserved = true;
        ctx.run_ui(
            RawInput {
                events: vec![key(true, false, Modifiers::NONE)],
                ..Default::default()
            },
            |ui| {
                passes += 1;
                ui.interact(
                    egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0)),
                    timeline_focus_id(),
                    egui::Sense::click(),
                );
                shortcuts.begin_pass(ui.ctx());
                commands.extend(router.route(ui.ctx(), READY));
                shortcuts.collect(ui.ctx());
                focus_preserved &= ui.memory(|memory| memory.has_focus(timeline_focus_id()));
                if ui.ctx().current_pass_index() == 0 {
                    ui.ctx()
                        .request_discard("test unrelated layout measurement");
                }
            },
        )
        .textures_delta
        .clear();
        assert_eq!(passes, 2);
        assert!(focus_preserved);
        assert_eq!(commands, vec![Command::ToggleAnimationPlayback]);
        assert!(shortcuts.commands().is_empty());
        assert!(router.owns_frame);
    }

    #[test]
    fn blur_in_the_same_batch_cancels_queued_playback() {
        let ctx = Context::default();
        ctx.memory_mut(|memory| memory.request_focus(timeline_focus_id()));
        let mut router = TimelineKeyboard::default();
        assert!(
            frame(
                &mut router,
                &ctx,
                READY,
                vec![
                    key(true, false, Modifiers::NONE),
                    Event::WindowFocused(false),
                    Event::WindowFocused(true),
                    key(true, true, Modifiers::NONE),
                ]
            )
            .is_empty()
        );
    }
}
