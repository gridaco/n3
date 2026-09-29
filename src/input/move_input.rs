//! Arrow-key movement in canonical centimeters, independent of display units, camera zoom and framing.
use crate::{camera::Camera, orientation::display_rotation};
use glam::{DVec3, Vec3};

/// Physical editing increments are independent of the chosen display unit.
pub const DEFAULT_NUDGE_CM: f64 = 1.0;
pub const COARSE_NUDGE_MULTIPLIER: f64 = 10.0;

// Accommodate f32 cardinal-angle roundoff, not an oblique-view snapping zone.
const ALIGNMENT_EPSILON: f32 = 1.0e-6;

/// Map screen-right/up intent to a document-world delta. Each nonzero intent
/// component represents one step, regardless of its integer magnitude.
///
/// Without a lock, the view basis must coincide with the document axes. With
/// a lock, screen projection determines the axis sign; perpendicular or end-on
/// arrows use Right/Up = positive, Left/Down = negative. Opposing diagonal
/// intents cancel in that fallback. During camera animation, use its destination
/// orientation without advancing or cancelling the live transition.
pub fn nudge_delta(
    camera: &Camera,
    z_up: bool,
    axis: Option<usize>,
    horizontal: i8,
    vertical: i8,
    step: f64,
) -> Option<DVec3> {
    if !step.is_finite() || step <= 0.0 {
        return None;
    }
    let horizontal = f32::from(horizontal.signum());
    let vertical = f32::from(vertical.signum());
    if horizontal == 0.0 && vertical == 0.0 {
        return None;
    }
    let destination = camera.is_transitioning().then(|| {
        let mut destination = camera.clone();
        destination.finish_transition();
        destination
    });
    let camera = destination.as_ref().unwrap_or(camera);
    let rotation = display_rotation(z_up);
    let projected = [Vec3::X, Vec3::Y, Vec3::Z]
        .map(|axis| camera.direction_in_view(rotation.transform_vector3(axis)));
    if projected.iter().any(|axis| !axis.is_finite()) {
        return None;
    }
    let mut delta = DVec3::ZERO;
    if let Some(axis) = axis {
        let direction = projected.get(axis)?;
        let dot = direction.x * horizontal + direction.y * vertical;
        let signed_intent = if dot.abs() > ALIGNMENT_EPSILON {
            dot
        } else {
            horizontal + vertical
        };
        if signed_intent == 0.0 {
            return None;
        }
        delta[axis] = f64::from(signed_intent.signum()) * step;
    } else {
        let (right_axis, right_sign) = aligned_axis(&projected, Vec3::X)?;
        let (up_axis, up_sign) = aligned_axis(&projected, Vec3::Y)?;
        if right_axis == up_axis {
            return None;
        }
        delta[right_axis] = f64::from(horizontal * right_sign) * step;
        delta[up_axis] = f64::from(vertical * up_sign) * step;
    }
    Some(delta)
}

fn aligned_axis(projected: &[Vec3; 3], screen_axis: Vec3) -> Option<(usize, f32)> {
    projected.iter().enumerate().find_map(|(index, axis)| {
        let sign = axis.dot(screen_axis).signum();
        axis.abs_diff_eq(screen_axis * sign, ALIGNMENT_EPSILON)
            .then_some((index, sign))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Transition, View};
    use std::time::Duration;

    #[test]
    fn six_aligned_views_map_exact_document_axes_with_either_up_convention() {
        for (view, y_right, y_up, z_right, z_up) in [
            (View::Front, DVec3::X, DVec3::Y, DVec3::X, DVec3::Z),
            (View::Right, DVec3::NEG_Z, DVec3::Y, DVec3::Y, DVec3::Z),
            (View::Back, DVec3::NEG_X, DVec3::Y, DVec3::NEG_X, DVec3::Z),
            (View::Left, DVec3::Z, DVec3::Y, DVec3::NEG_Y, DVec3::Z),
            (View::Top, DVec3::X, DVec3::NEG_Z, DVec3::X, DVec3::Y),
            (View::Bottom, DVec3::X, DVec3::Z, DVec3::X, DVec3::NEG_Y),
        ] {
            let mut camera = Camera::default();
            camera.set_view(view);
            camera.pan(23., -17., 500.);
            camera.zoom(2.);
            for (z_up, right, up) in [(false, y_right, y_up), (true, z_right, z_up)] {
                for (horizontal, vertical, expected) in [
                    (1, 0, right),
                    (-1, 0, -right),
                    (0, 1, up),
                    (0, -1, -up),
                    (1, 1, right + up),
                ] {
                    assert_eq!(
                        nudge_delta(&camera, z_up, None, horizontal, vertical, 0.125),
                        Some(expected * 0.125),
                        "{view:?}, z_up={z_up}, arrow=({horizontal}, {vertical})"
                    );
                }
            }
        }
    }

    #[test]
    fn unlocked_axes_require_alignment_and_use_animation_destination_without_mutation() {
        let mut camera = Camera::default();
        assert!(nudge_delta(&camera, false, None, 1, 0, 1.).is_none());
        camera.toggle_projection();
        assert!(nudge_delta(&camera, false, None, 0, 1, 1.).is_none());
        camera.set_view(View::Front);
        camera.orbit(0.001, 0.);
        assert!(nudge_delta(&camera, false, None, 1, 0, 1.).is_none());
        camera.set_view(View::Front);
        camera.toggle_projection();
        assert_eq!(nudge_delta(&camera, false, None, 1, 0, 1.), Some(DVec3::X));
        camera.set_view_with_transition(
            View::Right,
            Transition::Animated {
                duration: Duration::from_secs(1),
            },
        );
        for elapsed in [Duration::ZERO, Duration::from_nanos(999_999_999)] {
            camera.advance_transition(elapsed);
            assert!(camera.is_transitioning());
            let visible_pose = camera.view_projection(1.0);
            assert_eq!(
                nudge_delta(&camera, false, None, 1, 0, 1.),
                Some(DVec3::NEG_Z)
            );
            assert_eq!(
                nudge_delta(&camera, false, Some(0), 1, 0, 1.),
                Some(DVec3::X)
            );
            assert!(camera.is_transitioning());
            assert_eq!(camera.view_projection(1.0), visible_pose);
        }
        camera.finish_transition();
        assert_eq!(
            nudge_delta(&camera, false, None, 1, 0, 1.),
            Some(DVec3::NEG_Z)
        );
        camera.set_view(View::Perspective);
        camera.set_view_with_transition(
            View::Front,
            Transition::Animated {
                duration: Duration::from_millis(120),
            },
        );
        for elapsed in [Duration::ZERO, Duration::from_millis(60)] {
            camera.advance_transition(elapsed);
            assert_eq!(nudge_delta(&camera, false, None, 1, 0, 1.), Some(DVec3::X));
            assert_eq!(
                nudge_delta(&camera, false, Some(0), 0, 1, 1.),
                Some(DVec3::X)
            );
            assert!(camera.is_transitioning());
        }
        camera.finish_transition();
        camera.orbit_direction(
            1.,
            0.,
            Transition::Animated {
                duration: Duration::from_millis(120),
            },
        );
        assert!(nudge_delta(&camera, false, None, 1, 0, 1.).is_none());
        assert_eq!(
            nudge_delta(&camera, false, Some(0), 1, 0, 1.),
            Some(DVec3::X)
        );
    }

    #[test]
    fn explicit_locks_follow_projection_and_use_deterministic_perpendicular_fallback() {
        let mut camera = Camera::default();
        for (view, axis, horizontal, vertical, expected) in [
            (View::Front, 0, 1, 0, DVec3::X),
            (View::Front, 0, 0, 1, DVec3::X),
            (View::Front, 0, 0, -1, DVec3::NEG_X),
            (View::Back, 0, -1, 0, DVec3::X),
            (View::Back, 0, 0, 1, DVec3::X),
            (View::Back, 0, 1, 0, DVec3::NEG_X),
        ] {
            camera.set_view(view);
            assert_eq!(
                nudge_delta(&camera, false, Some(axis), horizontal, vertical, 0.5),
                Some(expected * 0.5)
            );
        }
        // The depth axis points either toward or away from the eye; neither
        // reverses the documented fallback for an axis with no screen extent.
        for view in [View::Front, View::Back] {
            camera.set_view(view);
            for (horizontal, vertical, sign) in [(1, 0, 1.), (0, 1, 1.), (-1, 0, -1.), (0, -1, -1.)]
            {
                assert_eq!(
                    nudge_delta(&camera, false, Some(2), horizontal, vertical, 2.),
                    Some(DVec3::Z * sign * 2.)
                );
            }
        }
        camera.set_view(View::Perspective);
        assert_eq!(
            nudge_delta(&camera, false, Some(0), 0, 1, 1.),
            Some(DVec3::NEG_X)
        );
        assert_eq!(
            nudge_delta(&camera, false, Some(2), 1, 0, 1.),
            Some(DVec3::NEG_Z)
        );
        assert_eq!(
            nudge_delta(&camera, true, Some(2), 0, 1, 1.),
            Some(DVec3::Z)
        );
        assert_eq!(
            nudge_delta(&camera, true, Some(1), 1, 0, 1.),
            Some(DVec3::Y)
        );
    }

    #[test]
    fn invalid_steps_axes_and_empty_intents_do_not_move() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        for step in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(nudge_delta(&camera, false, None, 1, 0, step).is_none());
            assert!(nudge_delta(&camera, false, Some(0), 1, 0, step).is_none());
        }
        assert!(nudge_delta(&camera, false, Some(3), 1, 0, 1.).is_none());
        assert!(nudge_delta(&camera, false, None, 0, 0, 1.).is_none());
        assert!(nudge_delta(&camera, false, Some(2), 1, -1, 1.).is_none());
    }
}
