//! Host navigation events shared by native, browser and executable tutorials.
//!
//! Deltas are logical points (precise scroll), wheel lines, native pinch delta,
//! or native rotation degrees. The replay cue is derived from the same event
//! that reaches the camera, never supplied as a second illustrative action.
use crate::axis_gizmo;
use crate::{pie_input, scroll_input::ScrollPhase, shortcuts, workspace_ui::WorkspaceUi};
use egui::{Context, Modifiers, Pos2, Vec2};

#[derive(Clone, Copy, Debug)]
pub enum Event {
    ModifiersChanged(Modifiers),
    Wheel {
        delta: Vec2,
        modifiers: Modifiers,
    },
    TrackpadScroll {
        delta: Vec2,
        phase: ScrollPhase,
        modifiers: Modifiers,
    },
    Pinch {
        delta: f64,
        modifiers: Modifiers,
    },
    Rotate {
        degrees: f32,
        modifiers: Modifiers,
    },
}

impl Event {
    pub fn modifiers(self) -> Modifiers {
        match self {
            Self::ModifiersChanged(modifiers)
            | Self::Wheel { modifiers, .. }
            | Self::TrackpadScroll { modifiers, .. }
            | Self::Pinch { modifiers, .. }
            | Self::Rotate { modifiers, .. } => modifiers,
        }
    }

    /// Match egui-winit's translation. Native windows already receive these
    /// events from egui-winit; browser scrolling and replay derive them here.
    pub fn egui_event(self) -> Option<egui::Event> {
        match self {
            Self::ModifiersChanged(modifiers) => Some(egui::Event::ModifiersChanged(modifiers)),
            Self::Wheel { delta, modifiers } => Some(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta,
                modifiers,
                phase: egui::TouchPhase::Move,
            }),
            Self::TrackpadScroll {
                delta,
                modifiers,
                phase,
            } => Some(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta,
                modifiers,
                phase: match phase {
                    ScrollPhase::Started => egui::TouchPhase::Start,
                    ScrollPhase::Moved => egui::TouchPhase::Move,
                    ScrollPhase::Ended => egui::TouchPhase::End,
                    ScrollPhase::Cancelled => egui::TouchPhase::Cancel,
                },
            }),
            Self::Pinch { delta, .. } => Some(egui::Event::Zoom((delta as f32).exp())),
            Self::Rotate { degrees, .. } => Some(egui::Event::Rotate(-degrees.to_radians())),
        }
    }
}

pub fn accepts(
    state: &WorkspaceUi,
    ctx: &Context,
    pointer: Option<Pos2>,
    focused: bool,
    pending_events: &[egui::Event],
) -> bool {
    let pending_pie = shortcuts::viewport_keys_available(ctx)
        && !state.temporary_navigation_active()
        && pie_input::pending_trigger(state.editor.tool, pending_events);
    focused
        && !state.mouse_navigation_active()
        && !state.pie_owns_input()
        && !state.terminal_focused(ctx)
        && !pending_pie
        && !pointer.is_some_and(|pos| {
            state
                .tool_dock
                .floating_tabs_rect
                .is_some_and(|rect| rect.contains(pos))
        })
        && pointer.is_some_and(|pos| viewport_accepts_pointer(ctx, state.viewport_ui_rect, pos))
}

/// Route once before the UI frame, using the same current ownership as native
/// window events. Blocked scrolling cancels its latch, including terminal phases.
pub fn route(
    state: &mut WorkspaceUi,
    ctx: &Context,
    pointer: Option<Pos2>,
    focused: bool,
    pending_events: &[egui::Event],
    event: Event,
) {
    state.navigation_modifiers_changed(event.modifiers());
    if matches!(event, Event::ModifiersChanged(_)) {
        return;
    }
    if !accepts(state, ctx, pointer, focused, pending_events) {
        if matches!(event, Event::Wheel { .. } | Event::TrackpadScroll { .. }) {
            state.cancel_trackpad_scroll();
        }
        return;
    }
    match event {
        Event::ModifiersChanged(_) => unreachable!(),
        Event::Wheel { delta, modifiers } => {
            state.scroll(delta.x, delta.y, false, modifiers, pointer)
        }
        Event::TrackpadScroll {
            delta,
            phase,
            modifiers,
        } => {
            state.trackpad_scroll(delta.x, delta.y, modifiers, phase, pointer);
        }
        Event::Pinch { delta, .. } => state.pinch(delta, pointer),
        Event::Rotate { degrees, .. } => state.trackpad_rotate(degrees),
    }
}

/// Native camera events must not pass through a floating egui window, including
/// the first press or wheel event before egui has captured an active drag.
pub fn viewport_accepts_pointer(
    ctx: &egui::Context,
    viewport: egui::Rect,
    pos: egui::Pos2,
) -> bool {
    viewport.contains(pos)
        && !axis_gizmo::bounds(viewport).contains(pos)
        && !egui::Popup::is_any_open(ctx)
        && !ctx.memory(|memory| memory.top_modal_layer().is_some())
        && !ctx.egui_is_using_pointer()
        && ctx.layer_id_at(pos) == Some(egui::LayerId::background())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_keep_native_units_and_conversion() {
        let modifiers = Modifiers::SHIFT;
        let delta = egui::vec2(8.0, -12.0);
        assert_eq!(
            Event::Wheel { delta, modifiers }.egui_event(),
            Some(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta,
                modifiers,
                phase: egui::TouchPhase::Move,
            })
        );
        assert_eq!(
            Event::TrackpadScroll {
                delta,
                phase: ScrollPhase::Started,
                modifiers
            }
            .egui_event(),
            Some(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta,
                modifiers,
                phase: egui::TouchPhase::Start,
            })
        );
        assert_eq!(
            Event::Pinch {
                delta: 0.25,
                modifiers
            }
            .egui_event(),
            Some(egui::Event::Zoom(0.25_f32.exp()))
        );
        assert_eq!(
            Event::Rotate {
                degrees: 90.0,
                modifiers
            }
            .egui_event(),
            Some(egui::Event::Rotate(-std::f32::consts::FRAC_PI_2))
        );
        assert_eq!(
            Event::ModifiersChanged(modifiers).egui_event(),
            Some(egui::Event::ModifiersChanged(modifiers))
        );
    }
}
