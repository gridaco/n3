//! Gesture ownership for precise scrolling. View-dependent policy supplies the
//! desired action. Ordinary input latches per gesture; Shift input latches for
//! the complete key hold, including a finger-to-momentum phase handoff.
use egui::Vec2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollMotion {
    Pan,
    Orbit,
    Zoom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScrollPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

#[derive(Debug)]
struct Gesture {
    shift: bool,
    motion: Option<ScrollMotion>,
}

#[derive(Debug, Default)]
pub struct ScrollInput {
    gesture: Option<Gesture>,
    shift_motion: Option<ScrollMotion>,
    command: bool,
}

impl ScrollInput {
    pub fn reset(&mut self) {
        self.gesture = None;
        self.shift_motion = None;
    }

    /// Forward real modifier changes even when no scroll delta accompanies them.
    /// Releasing and pressing Shift between deltas permits a fresh choice.
    /// Command changes clear both latches so 2D pan/zoom can switch mid-gesture.
    pub fn modifiers_changed(&mut self, shift: bool, command: bool) {
        if self.command != command {
            self.reset();
            self.command = command;
        }
        if !shift {
            self.shift_motion = None;
        }
        if let Some(gesture) = &mut self.gesture
            && gesture.shift != shift
        {
            gesture.shift = shift;
            gesture.motion = None;
        }
    }

    /// Return an action only for a finite, nonzero delta. A phase-aware gesture
    /// latches its first action. Shift owns its first action until key release,
    /// including across Ended/Started momentum boundaries. Without Shift,
    /// unphased Moved events deliberately do not latch.
    pub fn update(
        &mut self,
        phase: ScrollPhase,
        shift: bool,
        command: bool,
        desired: ScrollMotion,
        delta: Vec2,
    ) -> Option<ScrollMotion> {
        if phase == ScrollPhase::Cancelled {
            self.reset();
            return None;
        }
        self.modifiers_changed(shift, command);
        if phase == ScrollPhase::Started {
            self.gesture = Some(Gesture {
                shift,
                motion: None,
            });
        }
        let motion = if delta.is_finite() && delta != Vec2::ZERO {
            Some(if shift {
                *self.shift_motion.get_or_insert(desired)
            } else {
                match &mut self.gesture {
                    Some(gesture) => *gesture.motion.get_or_insert(desired),
                    None => desired,
                }
            })
        } else {
            None
        };
        if phase == ScrollPhase::Ended {
            // macOS can immediately follow finger Ended with momentum Started.
            // The held Shift action survives both; only release/reset clears it.
            self.gesture = None;
        }
        motion
    }
}

#[cfg(test)]
mod tests {
    use super::{ScrollMotion::*, ScrollPhase::*, *};

    fn movement() -> Vec2 {
        Vec2::new(4.0, -7.0)
    }

    #[test]
    fn command_changes_release_pan_and_zoom_latches_including_shift_hold() {
        for shift in [false, true] {
            let mut input = ScrollInput::default();
            assert_eq!(
                input.update(Started, shift, false, Pan, movement()),
                Some(Pan)
            );
            input.modifiers_changed(shift, true);
            assert_eq!(
                input.update(Moved, shift, true, Zoom, movement()),
                Some(Zoom)
            );
            assert_eq!(
                input.update(Ended, shift, true, Zoom, movement()),
                Some(Zoom)
            );
            assert_eq!(
                input.update(Started, shift, true, Zoom, movement()),
                Some(Zoom)
            );
            input.modifiers_changed(shift, false);
            assert_eq!(
                input.update(Moved, shift, false, Pan, movement()),
                Some(Pan)
            );
        }
    }

    #[test]
    fn an_aligned_shift_orbit_stays_orbit_after_leaving_alignment() {
        let mut input = ScrollInput::default();
        assert_eq!(
            input.update(Started, true, false, Orbit, movement()),
            Some(Orbit)
        );
        assert_eq!(
            input.update(Moved, true, false, Pan, movement()),
            Some(Orbit)
        );
        assert_eq!(
            input.update(Ended, true, false, Pan, movement()),
            Some(Orbit)
        );
        assert!(input.gesture.is_none());
        // A momentum Started is indistinguishable from another finger Started.
        // Both retain the action while the original Shift key remains held.
        assert_eq!(
            input.update(Started, true, false, Pan, movement()),
            Some(Orbit)
        );
        assert_eq!(
            input.update(Moved, true, false, Pan, movement()),
            Some(Orbit)
        );
        input.modifiers_changed(false, false);
        input.modifiers_changed(true, false);
        assert_eq!(input.update(Moved, true, false, Pan, movement()), Some(Pan));
        assert_eq!(
            input.update(Moved, true, false, Orbit, movement()),
            Some(Pan)
        );
    }

    #[test]
    fn pressing_and_releasing_shift_reselects_the_current_action() {
        let mut input = ScrollInput::default();
        assert_eq!(
            input.update(Started, false, false, Pan, movement()),
            Some(Pan)
        );
        assert_eq!(
            input.update(Moved, false, false, Zoom, movement()),
            Some(Pan)
        );
        assert_eq!(
            input.update(Moved, true, false, Orbit, movement()),
            Some(Orbit)
        );
        assert_eq!(
            input.update(Moved, true, false, Pan, movement()),
            Some(Orbit)
        );
        assert_eq!(
            input.update(Moved, false, false, Zoom, movement()),
            Some(Zoom)
        );
        assert_eq!(
            input.update(Moved, false, false, Pan, movement()),
            Some(Zoom)
        );
        assert_eq!(input.update(Ended, true, false, Pan, movement()), Some(Pan));
        assert!(input.gesture.is_none());
    }

    #[test]
    fn ending_applies_its_last_delta_once_then_clears_ownership() {
        let mut input = ScrollInput::default();
        input.update(Started, false, false, Zoom, movement());
        assert_eq!(
            input.update(Ended, false, false, Orbit, movement()),
            Some(Zoom)
        );
        assert!(input.gesture.is_none());
        assert_eq!(
            input.update(Moved, false, false, Pan, movement()),
            Some(Pan)
        );
        assert_eq!(input.update(Ended, false, false, Orbit, Vec2::ZERO), None);
    }

    #[test]
    fn cancellation_drops_its_delta_and_explicit_reset_drops_the_latch() {
        let mut input = ScrollInput::default();
        input.update(Started, true, false, Orbit, movement());
        assert_eq!(input.update(Cancelled, true, false, Zoom, movement()), None);
        assert!(input.gesture.is_none() && input.shift_motion.is_none());
        assert_eq!(input.update(Moved, true, false, Pan, movement()), Some(Pan));
        input.update(Started, true, false, Orbit, movement());
        input.reset();
        assert_eq!(
            input.update(Moved, true, false, Zoom, movement()),
            Some(Zoom)
        );
    }

    #[test]
    fn unphased_high_resolution_mouse_events_never_acquire_a_sticky_action() {
        let mut input = ScrollInput::default();
        for motion in [Pan, Orbit, Zoom, Pan] {
            assert_eq!(
                input.update(Moved, false, false, motion, movement()),
                Some(motion)
            );
            assert!(input.gesture.is_none());
        }
        assert_eq!(
            input.update(Ended, false, false, Zoom, movement()),
            Some(Zoom)
        );
        assert!(input.gesture.is_none());
    }

    #[test]
    fn zero_start_defers_choice_until_finite_nonzero_movement() {
        let mut input = ScrollInput::default();
        assert_eq!(input.update(Started, false, false, Pan, Vec2::ZERO), None);
        assert!(input.gesture.as_ref().unwrap().motion.is_none());
        assert_eq!(input.update(Moved, true, false, Zoom, Vec2::ZERO), None);
        assert_eq!(
            input.update(Moved, true, false, Orbit, movement()),
            Some(Orbit)
        );
        assert_eq!(
            input.update(Moved, true, false, Pan, movement()),
            Some(Orbit)
        );
        // Invalid movements do not choose the replacement after a Shift change.
        assert_eq!(
            input.update(Moved, false, false, Zoom, Vec2::new(f32::NAN, 1.0)),
            None
        );
        assert_eq!(
            input.update(Moved, false, false, Pan, movement()),
            Some(Pan)
        );
    }

    #[test]
    fn zero_and_nonfinite_deltas_never_apply_but_terminal_phases_always_reset() {
        let mut input = ScrollInput::default();
        for invalid in [
            Vec2::ZERO,
            Vec2::new(f32::NAN, 1.0),
            Vec2::new(1.0, f32::INFINITY),
        ] {
            assert_eq!(input.update(Started, false, false, Pan, invalid), None);
            assert_eq!(input.update(Moved, false, false, Orbit, invalid), None);
            assert_eq!(
                input.update(Moved, false, false, Zoom, movement()),
                Some(Zoom)
            );
            assert_eq!(input.update(Moved, false, false, Pan, invalid), None);
            assert_eq!(
                input.update(Moved, false, false, Pan, movement()),
                Some(Zoom)
            );
            assert_eq!(input.update(Ended, false, false, Pan, invalid), None);
            assert!(input.gesture.is_none());
            input.update(Started, false, false, Orbit, movement());
            assert_eq!(input.update(Cancelled, false, false, Pan, invalid), None);
            assert!(input.gesture.is_none());
        }
    }

    #[test]
    fn a_new_ordinary_start_replaces_ownership_and_tiny_valid_deltas_count() {
        let mut input = ScrollInput::default();
        input.update(Started, false, false, Orbit, movement());
        assert_eq!(input.update(Started, false, false, Pan, Vec2::ZERO), None);
        assert_eq!(
            input.update(Moved, false, false, Zoom, Vec2::new(f32::MIN_POSITIVE, 0.0)),
            Some(Zoom)
        );
        assert_eq!(
            input.update(Moved, false, false, Orbit, movement()),
            Some(Zoom)
        );
    }

    #[test]
    fn shift_action_survives_zero_finger_end_and_zero_momentum_start() {
        let mut input = ScrollInput::default();
        input.modifiers_changed(true, false);
        assert_eq!(input.update(Started, true, false, Pan, Vec2::ZERO), None);
        assert_eq!(
            input.update(Moved, true, false, Orbit, movement()),
            Some(Orbit)
        );
        assert_eq!(input.update(Ended, true, false, Pan, Vec2::ZERO), None);
        assert_eq!(input.update(Started, true, false, Pan, Vec2::ZERO), None);
        assert_eq!(
            input.update(Moved, true, false, Pan, movement()),
            Some(Orbit)
        );
        assert_eq!(
            input.update(Ended, true, false, Pan, movement()),
            Some(Orbit)
        );
        assert!(input.gesture.is_none());
        assert_eq!(input.shift_motion, Some(Orbit));
        input.modifiers_changed(false, false);
        assert!(input.shift_motion.is_none());
        input.modifiers_changed(true, false);
        assert_eq!(
            input.update(Started, true, false, Pan, movement()),
            Some(Pan)
        );
    }

    #[test]
    fn modifier_only_release_repress_does_not_reuse_an_in_progress_shift_choice() {
        let mut input = ScrollInput::default();
        input.update(Started, true, false, Orbit, movement());
        input.modifiers_changed(false, false);
        input.modifiers_changed(true, false);
        for invalid in [
            Vec2::ZERO,
            Vec2::new(f32::NAN, 1.0),
            Vec2::new(f32::INFINITY, 1.0),
        ] {
            assert_eq!(input.update(Moved, true, false, Zoom, invalid), None);
            assert!(input.shift_motion.is_none());
        }
        assert_eq!(input.update(Moved, true, false, Pan, movement()), Some(Pan));
        assert_eq!(
            input.update(Ended, true, false, Zoom, movement()),
            Some(Pan)
        );
        // Unphased Shift input follows the same key-hold policy.
        assert_eq!(
            input.update(Moved, true, false, Orbit, movement()),
            Some(Pan)
        );
        assert_eq!(
            input.update(Moved, false, false, Zoom, movement()),
            Some(Zoom)
        );
        assert_eq!(
            input.update(Moved, true, false, Orbit, movement()),
            Some(Orbit)
        );
    }
}
