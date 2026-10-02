//! Browser gesture units translated into the shared navigation vocabulary.
//!
//! The host owns DOM listeners, focus and pointer coordinates. This adapter does
//! not choose a camera action or bypass the shared viewport ownership checks.
//! Browsers do not identify the device behind pixel wheel events, so ordinary
//! pixel scrolling follows precise-scroll policy, including pixel mouse wheels.

use crate::{navigation_events::Event, scroll_input::ScrollPhase};
use egui::{Modifiers, Vec2};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WheelUnit {
    Pixels,
    Lines,
    Pages,
}

#[derive(Clone, Copy, Debug)]
pub struct Wheel {
    /// DOM deltas: down/right are positive, opposite N3's navigation deltas.
    pub delta: Vec2,
    pub unit: WheelUnit,
    /// Browser pinch and physical Control-wheel are indistinguishable here.
    pub ctrl: bool,
    /// The host's physical modifiers, without the pinch's synthetic Control key.
    pub modifiers: Modifiers,
    pub points_per_css_pixel: f32,
    pub page_height_css: f32,
}

/// Convert one wheel event without inventing gesture phases or device identity.
pub fn wheel(input: Wheel) -> Option<Event> {
    if !input.delta.is_finite()
        || !input.points_per_css_pixel.is_finite()
        || input.points_per_css_pixel <= 0.0
    {
        return None;
    }
    let css_pixels_per_unit = match input.unit {
        WheelUnit::Pixels => 1.0,
        // DOM line units do not prescribe a pixel size. Use an explicit 40 CSS
        // pixel normalization only for browser pinch/Control-wheel zoom.
        WheelUnit::Lines => 40.0,
        WheelUnit::Pages => {
            if !input.page_height_css.is_finite() || input.page_height_css <= 0.0 {
                return None;
            }
            f64::from(input.page_height_css)
        }
    };
    if input.ctrl {
        // A hundred CSS pixels represents one canonical pinch delta. Keep this
        // independent of display density and egui zoom; the shared camera owns
        // its existing pinch sensitivity and pointer anchoring.
        return Some(Event::Pinch {
            delta: -f64::from(input.delta.y) * css_pixels_per_unit * 0.01,
            modifiers: input.modifiers,
        });
    }
    if input.unit == WheelUnit::Lines {
        return Some(Event::Wheel {
            delta: -input.delta,
            modifiers: input.modifiers,
        });
    }
    let delta = -input.delta * (css_pixels_per_unit as f32 * input.points_per_css_pixel);
    delta.is_finite().then_some(Event::TrackpadScroll {
        delta,
        // WheelEvent exposes no finger/momentum phase. Shared scroll routing
        // deliberately supports unphased Moved events, including Shift latching.
        phase: ScrollPhase::Moved,
        modifiers: input.modifiers,
    })
}

/// WebKit reports cumulative scale and clockwise rotation from gesturestart.
#[derive(Debug, Default)]
pub struct WebKitGesture {
    previous: Option<(f64, f32)>,
}

impl WebKitGesture {
    pub fn begin(&mut self, scale: f64, rotation: f32) -> bool {
        self.reset();
        if !valid_gesture(scale, rotation) {
            return false;
        }
        self.previous = Some((scale, rotation));
        true
    }

    pub fn update(
        &mut self,
        scale: f64,
        rotation: f32,
        modifiers: Modifiers,
    ) -> Option<[Event; 2]> {
        let (previous_scale, previous_rotation) = self.previous?;
        if !valid_gesture(scale, rotation) {
            return None;
        }
        // Subtract logs instead of dividing first, avoiding ratio overflow for
        // finite positive values. Successive deltas compose to the total ratio.
        let delta = scale.ln() - previous_scale.ln();
        let degrees = previous_rotation - rotation;
        if !delta.is_finite() || !degrees.is_finite() {
            return None;
        }
        self.previous = Some((scale, rotation));
        Some([
            Event::Pinch { delta, modifiers },
            Event::Rotate { degrees, modifiers },
        ])
    }

    pub fn reset(&mut self) {
        self.previous = None;
    }

    pub fn is_active(&self) -> bool {
        self.previous.is_some()
    }
}

fn valid_gesture(scale: f64, rotation: f32) -> bool {
    scale.is_finite() && scale > 0.0 && rotation.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(unit: WheelUnit) -> Wheel {
        Wheel {
            delta: Vec2::new(2.0, -3.0),
            unit,
            ctrl: false,
            modifiers: Modifiers::SHIFT,
            points_per_css_pixel: 0.5,
            page_height_css: 800.0,
        }
    }

    fn pinch(event: Event) -> f64 {
        let Event::Pinch { delta, .. } = event else {
            panic!("expected a pinch event, got {event:?}");
        };
        delta
    }

    fn rotation(event: Event) -> f32 {
        let Event::Rotate { degrees, .. } = event else {
            panic!("expected a rotation event, got {event:?}");
        };
        degrees
    }

    #[test]
    fn ordinary_wheel_preserves_units_sign_modifiers_and_unphased_scroll() {
        for (unit, expected) in [
            (WheelUnit::Pixels, Vec2::new(-1.0, 1.5)),
            (WheelUnit::Pages, Vec2::new(-800.0, 1200.0)),
        ] {
            let Some(Event::TrackpadScroll {
                delta,
                phase,
                modifiers,
            }) = wheel(sample(unit))
            else {
                panic!("expected precise scrolling");
            };
            assert_eq!(delta, expected);
            assert_eq!(phase, ScrollPhase::Moved);
            assert_eq!(modifiers, Modifiers::SHIFT);
        }
        let Some(Event::Wheel { delta, modifiers }) = wheel(sample(WheelUnit::Lines)) else {
            panic!("expected wheel lines");
        };
        assert_eq!(delta, Vec2::new(-2.0, 3.0));
        assert_eq!(modifiers, Modifiers::SHIFT);
    }

    #[test]
    fn browser_pinch_is_zoom_in_all_units_without_synthetic_control() {
        for (unit, expected) in [
            (WheelUnit::Pixels, 0.03),
            (WheelUnit::Lines, 1.2),
            (WheelUnit::Pages, 24.0),
        ] {
            let input = Wheel {
                ctrl: true,
                ..sample(unit)
            };
            for points_per_css_pixel in [0.5, 1.0, 2.0] {
                let event = wheel(Wheel {
                    points_per_css_pixel,
                    ..input
                })
                .unwrap();
                assert!((pinch(event) - expected).abs() < 1e-12);
                assert_eq!(event.modifiers(), Modifiers::SHIFT);
                assert!(!event.modifiers().ctrl);
            }
        }
    }

    #[test]
    fn wheel_rejects_nonfinite_deltas_and_invalid_conversion() {
        let input = sample(WheelUnit::Pixels);
        for delta in [Vec2::new(f32::NAN, 1.0), Vec2::new(1.0, f32::INFINITY)] {
            for ctrl in [false, true] {
                assert!(
                    wheel(Wheel {
                        delta,
                        ctrl,
                        ..input
                    })
                    .is_none()
                );
            }
        }
        for points_per_css_pixel in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(
                wheel(Wheel {
                    points_per_css_pixel,
                    ..input
                })
                .is_none()
            );
        }
        for page_height_css in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(
                wheel(Wheel {
                    page_height_css,
                    ..sample(WheelUnit::Pages)
                })
                .is_none()
            );
        }
        assert!(
            wheel(Wheel {
                delta: Vec2::splat(f32::MAX),
                points_per_css_pixel: 2.0,
                ..input
            })
            .is_none()
        );
    }

    #[test]
    fn wheel_zero_and_inverse_deltas_do_not_accumulate_zoom() {
        let input = Wheel {
            ctrl: true,
            ..sample(WheelUnit::Pixels)
        };
        assert_eq!(
            pinch(
                wheel(Wheel {
                    delta: Vec2::ZERO,
                    ..input
                })
                .unwrap()
            ),
            0.0
        );
        assert_eq!(
            pinch(wheel(input).unwrap())
                + pinch(
                    wheel(Wheel {
                        delta: -input.delta,
                        ..input
                    })
                    .unwrap()
                ),
            0.0,
        );
    }

    #[test]
    fn webkit_cumulative_values_become_incremental_native_events() {
        let mut gesture = WebKitGesture::default();
        assert!(gesture.begin(1.0, 0.0));
        assert!(gesture.is_active());
        let first = gesture.update(1.5, 20.0, Modifiers::ALT).unwrap();
        let second = gesture.update(2.0, 35.0, Modifiers::ALT).unwrap();
        assert!((pinch(first[0]) - 1.5_f64.ln()).abs() < 1e-12);
        assert!((pinch(second[0]) - (2.0_f64 / 1.5).ln()).abs() < 1e-12);
        assert!((pinch(first[0]) + pinch(second[0]) - 2.0_f64.ln()).abs() < 1e-12);
        assert_eq!(rotation(first[1]), -20.0);
        assert_eq!(rotation(second[1]), -15.0);
        assert!(
            first
                .into_iter()
                .chain(second)
                .all(|event| event.modifiers() == Modifiers::ALT)
        );
    }

    #[test]
    fn webkit_zero_inverse_and_reset_preserve_the_gesture_baseline() {
        let mut gesture = WebKitGesture::default();
        assert!(gesture.update(2.0, 10.0, Modifiers::NONE).is_none());
        assert!(gesture.begin(1.0, 0.0));
        let zero = gesture.update(1.0, 0.0, Modifiers::NONE).unwrap();
        assert_eq!(pinch(zero[0]), 0.0);
        assert_eq!(rotation(zero[1]), 0.0);
        let outward = gesture.update(2.0, -15.0, Modifiers::NONE).unwrap();
        let inward = gesture.update(1.0, 0.0, Modifiers::NONE).unwrap();
        assert_eq!(pinch(outward[0]) + pinch(inward[0]), 0.0);
        assert_eq!(rotation(outward[1]) + rotation(inward[1]), 0.0);
        gesture.reset();
        assert!(!gesture.is_active());
        assert!(gesture.update(2.0, 10.0, Modifiers::NONE).is_none());
        assert!(gesture.begin(2.0, 10.0));
        let restarted = gesture.update(4.0, 12.0, Modifiers::NONE).unwrap();
        assert!((pinch(restarted[0]) - 2.0_f64.ln()).abs() < 1e-12);
        assert_eq!(rotation(restarted[1]), -2.0);
    }

    #[test]
    fn webkit_invalid_values_never_poison_the_last_valid_sample() {
        let mut gesture = WebKitGesture::default();
        for (scale, invalid_rotation) in [
            (0.0, 0.0),
            (-1.0, 0.0),
            (f64::NAN, 0.0),
            (f64::INFINITY, 0.0),
            (1.0, f32::NAN),
            (1.0, f32::INFINITY),
        ] {
            assert!(!gesture.begin(scale, invalid_rotation));
            assert!(!gesture.is_active());
            assert!(gesture.begin(1.0, 0.0));
            assert!(
                gesture
                    .update(scale, invalid_rotation, Modifiers::NONE)
                    .is_none()
            );
            let recovered = gesture.update(2.0, 5.0, Modifiers::NONE).unwrap();
            assert!((pinch(recovered[0]) - 2.0_f64.ln()).abs() < 1e-12);
            assert_eq!(rotation(recovered[1]), -5.0);
        }
        assert!(gesture.begin(f64::MIN_POSITIVE, -f32::MAX));
        assert!(
            gesture
                .update(f64::MAX, f32::MAX, Modifiers::NONE)
                .is_none()
        );
        let finite = gesture
            .update(f64::MAX, -f32::MAX, Modifiers::NONE)
            .unwrap();
        assert!(pinch(finite[0]).is_finite());
        assert_eq!(rotation(finite[1]), 0.0);
    }
}
