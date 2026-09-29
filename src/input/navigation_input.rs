//! Ordered pointer input shared by native and headless viewport navigation.

use egui::{PointerButton, Pos2, Vec2};

use crate::pointer_policy::crossed_drag_threshold;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NavigationMotion {
    /// Emitted only when camera manipulation begins, never for a context click.
    Begin,
    Orbit(Vec2),
    Pan(Vec2),
    ContextClick(Pos2),
}

#[derive(Clone, Copy, Debug)]
enum Event {
    Press {
        button: PointerButton,
        motion: Option<NavigationTool>,
        eligible: bool,
        position: Pos2,
    },
    Release {
        button: PointerButton,
        position: Pos2,
    },
    Motion(Pos2),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// A resolved navigation intent, independent of its key or pointer binding.
pub enum NavigationTool {
    Orbit,
    Pan,
}

#[derive(Clone, Copy, Debug)]
struct Gesture {
    button: PointerButton,
    motion: NavigationTool,
    start: Pos2,
    last: Pos2,
    dragged: bool,
}

impl Gesture {
    fn advance(&mut self, position: Pos2, output: &mut Vec<NavigationMotion>) {
        let delta = if self.dragged {
            position - self.last
        } else if crossed_drag_threshold(self.start, position) {
            self.dragged = true;
            output.push(NavigationMotion::Begin);
            // Include pending displacement once, so small steps do not vanish.
            position - self.start
        } else {
            Vec2::ZERO
        };
        self.last = position;
        if delta.is_finite() && delta != Vec2::ZERO {
            output.push(match self.motion {
                NavigationTool::Pan => NavigationMotion::Pan(delta),
                NavigationTool::Orbit => NavigationMotion::Orbit(delta),
            });
        }
    }
}

#[derive(Clone, Default)]
pub struct NavigationInput {
    active: Option<Gesture>,
    queued: Vec<Event>,
}

impl NavigationInput {
    pub fn queue_press(&mut self, button: PointerButton, eligible: bool, position: Pos2) {
        self.queued.push(Event::Press {
            button,
            motion: match button {
                PointerButton::Secondary => Some(NavigationTool::Orbit),
                PointerButton::Middle => Some(NavigationTool::Pan),
                _ => None,
            },
            eligible,
            position,
        });
    }

    /// Explicit temporary navigation ownership. The host resolves the semantic
    /// tool from the binding and press context; ordinary primary presses remain
    /// selection input. This does not assign any modifier globally, so future
    /// context-specific Alt gestures can be resolved before this boundary.
    pub fn queue_primary_press(&mut self, tool: NavigationTool, eligible: bool, position: Pos2) {
        self.queued.push(Event::Press {
            button: PointerButton::Primary,
            motion: Some(tool),
            eligible,
            position,
        });
    }

    pub fn primary_drag_active(&self) -> bool {
        self.primary_drag_tool().is_some()
    }

    /// Includes pending orbit motion below the drag threshold: a primary click
    /// already belongs to navigation even before it changes the camera.
    pub fn primary_drag_tool(&self) -> Option<NavigationTool> {
        self.active
            .filter(|gesture| gesture.button == PointerButton::Primary)
            .map(|gesture| gesture.motion)
    }

    /// Modifier release ends only primary navigation, including queued input.
    /// Preserve other gestures and do not promote button presses that were
    /// originally blocked behind the primary gesture's first-button ownership.
    pub fn cancel_primary_drag(&mut self) {
        let mut owner = self.active.map(|gesture| gesture.button);
        self.queued.retain(|event| match *event {
            Event::Press {
                button,
                motion,
                eligible,
                position,
            } => {
                let accepted =
                    owner.is_none() && eligible && motion.is_some() && position.is_finite();
                if accepted {
                    owner = Some(button);
                }
                accepted && button != PointerButton::Primary
            }
            Event::Motion(position) => {
                let primary_owned = owner == Some(PointerButton::Primary);
                if !position.is_finite() {
                    owner = None;
                }
                !primary_owned
            }
            Event::Release { button, .. } => {
                let primary_owned =
                    owner == Some(PointerButton::Primary) && button == PointerButton::Primary;
                if owner == Some(button) {
                    owner = None;
                }
                !primary_owned
            }
        });
        if self.primary_drag_active() {
            self.active = None;
        }
    }

    pub fn queue_release(&mut self, button: PointerButton, position: Pos2) {
        self.queued.push(Event::Release { button, position });
    }

    pub fn queue_motion(&mut self, position: Pos2) {
        self.queued.push(Event::Motion(position));
    }

    /// Pending right-clicks count as owned input, so Escape can cancel them.
    pub fn wants_input(&self) -> bool {
        self.active.is_some()
            || self.queued.iter().any(|event| {
                matches!(event, Event::Press { motion: Some(_), eligible: true, position, .. }
                    if position.is_finite())
            })
    }

    /// Escape and focus loss also suppress a later release/context click.
    pub fn cancel(&mut self) {
        self.active = None;
        self.queued.clear();
    }

    /// Cancel an established gesture while preserving this frame's new presses.
    pub fn context_changed(&mut self) {
        self.active = None;
    }

    pub fn flush(&mut self) -> Vec<NavigationMotion> {
        let mut output = Vec::new();
        for event in self.queued.drain(..) {
            match event {
                Event::Press {
                    button,
                    motion,
                    eligible,
                    position,
                } => {
                    if self.active.is_none()
                        && eligible
                        && position.is_finite()
                        && let Some(motion) = motion
                    {
                        // Orbit begins only after deliberate pointer movement:
                        // an Alt-primary click must not leave Planar navigation.
                        // Pan has no click action and retains its immediate start.
                        let dragged = motion == NavigationTool::Pan;
                        self.active = Some(Gesture {
                            button,
                            motion,
                            start: position,
                            last: position,
                            dragged,
                        });
                        if dragged {
                            output.push(NavigationMotion::Begin);
                        }
                    }
                }
                Event::Motion(position) => {
                    if !position.is_finite() {
                        self.active = None;
                    } else if let Some(gesture) = &mut self.active {
                        gesture.advance(position, &mut output);
                    }
                }
                Event::Release { button, position } => {
                    if self.active.is_some_and(|gesture| gesture.button == button) {
                        let mut gesture = self.active.take().unwrap();
                        if position.is_finite() {
                            gesture.advance(position, &mut output);
                            if button == PointerButton::Secondary && !gesture.dragged {
                                output.push(NavigationMotion::ContextClick(position));
                            }
                        }
                    }
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer_policy::DRAG_THRESHOLD;

    const START: Pos2 = Pos2::new(100.0, 120.0);

    #[test]
    fn primary_orbit_click_and_exact_threshold_never_begin_or_open_a_context_menu() {
        for offset in [Vec2::ZERO, Vec2::new(DRAG_THRESHOLD, 0.0)] {
            let mut input = NavigationInput::default();
            input.queue_primary_press(NavigationTool::Orbit, true, START);
            assert!(input.wants_input());
            assert!(input.flush().is_empty());
            assert_eq!(input.primary_drag_tool(), Some(NavigationTool::Orbit));
            input.queue_motion(START + offset);
            assert!(input.flush().is_empty());
            input.queue_release(PointerButton::Primary, START + offset);
            assert!(input.flush().is_empty());
            assert!(!input.primary_drag_active());
            assert!(!input.wants_input());
        }
    }

    #[test]
    fn primary_orbit_preserves_the_threshold_displacement_and_stays_latched() {
        let mut input = NavigationInput::default();
        input.queue_primary_press(NavigationTool::Orbit, true, START);
        assert!(input.flush().is_empty());
        input.queue_motion(START + Vec2::new(1.0, 0.0));
        assert!(input.flush().is_empty());
        // A second intent cannot change an already owned gesture.
        input.queue_primary_press(NavigationTool::Pan, true, START);
        input.queue_press(PointerButton::Middle, true, START);
        let delta = Vec2::new(DRAG_THRESHOLD + 1.0, 2.0);
        input.queue_motion(START + delta);
        assert_eq!(
            input.flush(),
            vec![NavigationMotion::Begin, NavigationMotion::Orbit(delta)]
        );
        assert_eq!(input.primary_drag_tool(), Some(NavigationTool::Orbit));
        input.queue_release(PointerButton::Middle, START);
        input.queue_motion(START);
        input.queue_release(PointerButton::Primary, START);
        assert_eq!(input.flush(), vec![NavigationMotion::Orbit(-delta)]);
        assert!(!input.wants_input());
    }

    #[test]
    fn primary_orbit_release_can_cross_threshold_in_a_complete_event_batch() {
        let mut input = NavigationInput::default();
        input.queue_primary_press(NavigationTool::Orbit, true, START);
        let delta = Vec2::splat(3.0);
        input.queue_release(PointerButton::Primary, START + delta);
        assert_eq!(
            input.flush(),
            vec![NavigationMotion::Begin, NavigationMotion::Orbit(delta)]
        );
        assert!(!input.wants_input());
    }

    #[test]
    fn primary_orbit_cancellation_drops_motion_without_promoting_other_buttons() {
        for established in [false, true] {
            let mut input = NavigationInput::default();
            input.queue_primary_press(NavigationTool::Orbit, true, START);
            if established {
                assert!(input.flush().is_empty());
                input.queue_motion(START + Vec2::splat(20.0));
                assert_eq!(
                    input.flush(),
                    vec![
                        NavigationMotion::Begin,
                        NavigationMotion::Orbit(Vec2::splat(20.0))
                    ]
                );
            }
            input.queue_press(PointerButton::Secondary, true, START);
            input.queue_press(PointerButton::Middle, true, START);
            input.queue_motion(START + Vec2::splat(30.0));
            input.cancel_primary_drag();
            assert!(!input.primary_drag_active());
            input.queue_motion(START + Vec2::splat(40.0));
            for button in [
                PointerButton::Primary,
                PointerButton::Middle,
                PointerButton::Secondary,
            ] {
                input.queue_release(button, START + Vec2::splat(40.0));
            }
            assert!(input.flush().is_empty());
            assert!(!input.wants_input());
            // A fresh, explicit press can claim the next gesture.
            input.queue_press(PointerButton::Secondary, true, START);
            input.queue_release(PointerButton::Secondary, START);
            assert_eq!(input.flush(), vec![NavigationMotion::ContextClick(START)]);
        }
    }

    #[test]
    fn explicit_primary_pan_starts_immediately_and_preserves_every_displacement() {
        let mut input = NavigationInput::default();
        input.queue_primary_press(NavigationTool::Pan, true, START);
        assert!(input.wants_input());
        assert_eq!(input.flush(), vec![NavigationMotion::Begin]);
        assert!(input.primary_drag_active());
        for x in [1.0, 2.0, 3.0] {
            input.queue_motion(START + Vec2::new(x, 0.0));
            assert_eq!(
                input.flush(),
                vec![NavigationMotion::Pan(Vec2::new(1.0, 0.0))]
            );
        }
        input.queue_release(PointerButton::Primary, START + Vec2::new(3.5, 2.0));
        assert_eq!(
            input.flush(),
            vec![NavigationMotion::Pan(Vec2::new(0.5, 2.0))]
        );
        assert!(!input.primary_drag_active());
        assert!(!input.wants_input());

        input.queue_primary_press(NavigationTool::Pan, true, START);
        input.queue_release(PointerButton::Primary, START);
        assert_eq!(
            input.flush(),
            vec![NavigationMotion::Begin],
            "A stationary hand click cannot become a context click"
        );
    }

    #[test]
    fn primary_cancellation_ends_pan_before_later_motion_and_primary_release() {
        let mut input = NavigationInput::default();
        input.queue_primary_press(NavigationTool::Pan, true, START);
        assert_eq!(input.flush(), vec![NavigationMotion::Begin]);
        input.queue_motion(START + Vec2::new(10.0, 0.0));
        assert_eq!(
            input.flush(),
            vec![NavigationMotion::Pan(Vec2::new(10.0, 0.0))]
        );
        // The host observes Space-up here, before the still-held pointer moves.
        input.cancel_primary_drag();
        assert!(!input.primary_drag_active());
        assert!(!input.wants_input());
        input.queue_motion(START + Vec2::new(50.0, 0.0));
        assert!(input.flush().is_empty());
        input.queue_release(PointerButton::Primary, START + Vec2::new(60.0, 0.0));
        assert!(input.flush().is_empty());
        // Ordinary primary input is still selection input after hand release.
        input.queue_press(PointerButton::Primary, true, START);
        input.queue_motion(START + Vec2::splat(20.0));
        input.queue_release(PointerButton::Primary, START);
        assert!(input.flush().is_empty());
    }

    #[test]
    fn primary_cancel_preserves_middle_pan_and_secondary_context_ownership() {
        for established in [false, true] {
            let mut input = NavigationInput::default();
            input.queue_press(PointerButton::Middle, true, START);
            if established {
                assert_eq!(input.flush(), vec![NavigationMotion::Begin]);
            }
            input.queue_primary_press(NavigationTool::Pan, true, START);
            input.queue_motion(START + Vec2::new(6.0, 0.0));
            input.cancel_primary_drag();
            input.queue_release(PointerButton::Primary, START + Vec2::new(7.0, 0.0));
            input.queue_release(PointerButton::Middle, START + Vec2::new(8.0, 0.0));
            let mut expected = if established {
                Vec::new()
            } else {
                vec![NavigationMotion::Begin]
            };
            expected.extend([
                NavigationMotion::Pan(Vec2::new(6.0, 0.0)),
                NavigationMotion::Pan(Vec2::new(2.0, 0.0)),
            ]);
            assert_eq!(input.flush(), expected);
            assert!(!input.primary_drag_active());

            input.queue_press(PointerButton::Secondary, true, START);
            if established {
                assert!(input.flush().is_empty());
            }
            input.queue_primary_press(NavigationTool::Pan, true, START);
            input.cancel_primary_drag();
            input.queue_release(PointerButton::Primary, START);
            input.queue_release(PointerButton::Secondary, START);
            assert_eq!(input.flush(), vec![NavigationMotion::ContextClick(START)]);
        }
    }

    #[test]
    fn primary_cancel_does_not_promote_buttons_blocked_by_its_original_press() {
        for established in [false, true] {
            let mut input = NavigationInput::default();
            input.queue_primary_press(NavigationTool::Pan, true, START);
            if established {
                assert_eq!(input.flush(), vec![NavigationMotion::Begin]);
            }
            input.queue_press(PointerButton::Secondary, true, START);
            input.queue_press(PointerButton::Middle, true, START);
            input.queue_motion(START + Vec2::splat(20.0));
            input.cancel_primary_drag();
            input.queue_release(PointerButton::Primary, START);
            input.queue_release(PointerButton::Secondary, START);
            input.queue_release(PointerButton::Middle, START);
            assert!(input.flush().is_empty());
            assert!(!input.wants_input());
            input.queue_press(PointerButton::Secondary, true, START);
            input.queue_release(PointerButton::Secondary, START);
            assert_eq!(
                input.flush(),
                vec![NavigationMotion::ContextClick(START)],
                "A fresh press can acquire navigation after cancellation"
            );
        }
    }

    #[test]
    fn ineligible_and_nonfinite_primary_pan_input_cannot_emit_delayed_navigation() {
        let invalid = Pos2::new(f32::NAN, 0.0);
        for (eligible, position) in [(false, START), (true, invalid)] {
            let mut input = NavigationInput::default();
            input.queue_primary_press(NavigationTool::Pan, eligible, position);
            input.queue_motion(START + Vec2::splat(20.0));
            input.queue_release(PointerButton::Primary, START);
            assert!(!input.wants_input());
            assert!(input.flush().is_empty());
        }
        for invalid_release in [false, true] {
            let mut input = NavigationInput::default();
            input.queue_primary_press(NavigationTool::Pan, true, START);
            assert_eq!(input.flush(), vec![NavigationMotion::Begin]);
            if invalid_release {
                input.queue_release(PointerButton::Primary, invalid);
            } else {
                input.queue_motion(invalid);
                input.queue_motion(START + Vec2::splat(20.0));
                input.queue_release(PointerButton::Primary, START);
            }
            assert!(input.flush().is_empty());
            assert!(!input.primary_drag_active());
            assert!(!input.wants_input());
        }
    }

    #[test]
    fn secondary_click_including_exact_threshold_never_begins_camera_motion() {
        for offset in [Vec2::ZERO, Vec2::new(DRAG_THRESHOLD, 0.0)] {
            let mut input = NavigationInput::default();
            input.queue_press(PointerButton::Secondary, true, START);
            assert!(input.wants_input());
            assert!(input.flush().is_empty());
            // There is no duration cutoff, regardless of frames spent holding.
            for _ in 0..120 {
                assert!(input.flush().is_empty());
            }
            input.queue_motion(START + offset);
            assert!(input.flush().is_empty());
            input.queue_release(PointerButton::Secondary, START + offset);
            assert_eq!(
                input.flush(),
                vec![NavigationMotion::ContextClick(START + offset)]
            );
            assert!(!input.wants_input());
        }
    }

    #[test]
    fn small_steps_accumulate_and_returning_to_origin_remains_a_drag() {
        let mut input = NavigationInput::default();
        input.queue_press(PointerButton::Secondary, true, START);
        for x in [1.0, 2.0, 3.0, 4.0] {
            input.queue_motion(START + Vec2::new(x, 0.0));
            assert!(input.flush().is_empty());
        }
        let delta = Vec2::new(5.0, 0.0);
        input.queue_motion(START + delta);
        assert_eq!(
            input.flush(),
            vec![NavigationMotion::Begin, NavigationMotion::Orbit(delta)]
        );
        input.queue_motion(START);
        input.queue_release(PointerButton::Secondary, START);
        assert_eq!(input.flush(), vec![NavigationMotion::Orbit(-delta)]);
        assert!(!input.wants_input());
    }

    #[test]
    fn release_position_and_radial_distance_are_part_of_the_threshold() {
        let mut input = NavigationInput::default();
        input.queue_press(PointerButton::Secondary, true, START);
        let delta = Vec2::splat(3.0);
        input.queue_release(PointerButton::Secondary, START + delta);
        assert_eq!(
            input.flush(),
            vec![NavigationMotion::Begin, NavigationMotion::Orbit(delta)]
        );
        assert!(!input.wants_input());
    }

    #[test]
    fn first_eligible_button_wins_and_primary_never_navigates() {
        let mut input = NavigationInput::default();
        input.queue_press(PointerButton::Primary, true, START);
        input.queue_press(PointerButton::Secondary, false, START);
        input.queue_motion(START + Vec2::splat(20.0));
        input.queue_release(PointerButton::Primary, START);
        assert!(!input.wants_input());
        assert!(input.flush().is_empty());

        input.queue_press(PointerButton::Middle, true, START);
        input.queue_press(PointerButton::Secondary, true, START);
        input.queue_release(PointerButton::Secondary, START);
        input.queue_motion(START + Vec2::new(10.0, 0.0));
        input.queue_release(PointerButton::Middle, START + Vec2::new(12.0, 0.0));
        assert_eq!(
            input.flush(),
            vec![
                NavigationMotion::Begin,
                NavigationMotion::Pan(Vec2::new(10.0, 0.0)),
                NavigationMotion::Pan(Vec2::new(2.0, 0.0))
            ]
        );

        input.queue_press(PointerButton::Secondary, true, START);
        input.queue_press(PointerButton::Middle, true, START);
        input.queue_release(PointerButton::Middle, START);
        input.queue_release(PointerButton::Secondary, START);
        assert_eq!(input.flush(), vec![NavigationMotion::ContextClick(START)]);
    }

    #[test]
    fn cancellation_drops_queued_input_and_eventual_context_release() {
        for established in [true, false] {
            let mut input = NavigationInput::default();
            input.queue_press(PointerButton::Secondary, true, START);
            if established {
                input.flush();
            }
            input.queue_motion(START + Vec2::splat(20.0));
            input.cancel();
            input.queue_release(PointerButton::Secondary, START);
            assert!(input.flush().is_empty());
            assert!(!input.wants_input());
        }
        let mut input = NavigationInput::default();
        input.queue_press(PointerButton::Middle, true, START);
        input.flush();
        input.queue_motion(START + Vec2::splat(20.0));
        input.context_changed();
        assert!(input.flush().is_empty());
        input.queue_press(PointerButton::Secondary, true, START);
        input.context_changed();
        input.queue_release(PointerButton::Secondary, START);
        assert_eq!(input.flush(), vec![NavigationMotion::ContextClick(START)]);
    }

    #[test]
    fn invalid_positions_cannot_emit_motion_or_a_delayed_context_click() {
        let invalid = Pos2::new(f32::NAN, 0.0);
        let mut input = NavigationInput::default();
        input.queue_press(PointerButton::Secondary, true, invalid);
        input.queue_release(PointerButton::Secondary, START);
        assert!(!input.wants_input());
        assert!(input.flush().is_empty());
        input.queue_press(PointerButton::Secondary, true, START);
        input.queue_motion(invalid);
        input.queue_release(PointerButton::Secondary, START);
        assert!(input.flush().is_empty());
        input.queue_press(PointerButton::Secondary, true, START);
        input.queue_release(PointerButton::Secondary, invalid);
        assert!(input.flush().is_empty());
        assert!(!input.wants_input());
    }
}
