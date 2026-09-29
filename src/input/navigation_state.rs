//! Navigation mode and return policy, independent of UI controls and projection.

use crate::camera::{Camera, CameraOrientation, Transition};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlanarExit {
    /// Keep the visible angle and change to perspective.
    PerspectiveOnly,
    /// Restore the saved angle, retaining the current projection destination.
    /// During a Planar snap this is orthographic, even before its blend finishes.
    OrientationOnly,
    /// Restore the saved angle and change to perspective.
    #[default]
    OrientationAndPerspective,
}

#[derive(Clone, Debug, Default)]
pub struct ViewNavigation {
    planar: bool,
    free_orientation: Option<CameraOrientation>,
    return_transition: Option<u64>,
}

impl ViewNavigation {
    pub fn is_planar(&self) -> bool {
        self.planar
    }

    /// All zoom sources share the mode policy. Future point-based tools can
    /// supply the same scene-space anchor without duplicating camera math.
    pub fn zoom(&self, camera: &mut Camera, amount: f32, anchor: Option<glam::Vec2>, aspect: f32) {
        if self.planar
            && let Some(anchor) = anchor
        {
            camera.zoom_at(amount, anchor, aspect);
        } else {
            camera.zoom(amount);
        }
    }

    /// Call before snapping the camera. Changing between Planar axes retains
    /// the original Free angle. Re-entering during our own return animation also
    /// retains that angle instead of recording its temporary interpolated pose.
    pub fn enter_planar(&mut self, camera: &Camera) {
        let returning =
            self.return_transition.is_some() && self.return_transition == camera.transition_token();
        if !(self.planar || returning) {
            self.free_orientation = Some(camera.orientation());
        }
        self.planar = true;
        self.return_transition = None;
    }

    /// Manual orbit and explicit view commands leave Planar without recalling a
    /// saved camera angle or changing projection. The caller controls the camera.
    pub fn leave_planar(&mut self) {
        self.planar = false;
        self.return_transition = None;
    }

    /// Apply the selected return policy once, preserving the current target and
    /// zoom. Repeated requests while Free do not restart an active animation.
    pub fn return_to_free(
        &mut self,
        camera: &mut Camera,
        policy: PlanarExit,
        transition: Transition,
    ) {
        if !self.planar {
            return;
        }
        let orientation = match policy {
            PlanarExit::PerspectiveOnly => camera.orientation(),
            PlanarExit::OrientationOnly | PlanarExit::OrientationAndPerspective => {
                self.free_orientation.unwrap_or_default()
            }
        };
        let orthographic = if policy == PlanarExit::OrientationOnly {
            let mut destination = camera.clone();
            destination.finish_transition();
            destination.is_orthographic()
        } else {
            false
        };
        self.planar = false;
        camera.set_orientation_with_transition(orientation, orthographic, transition);
        self.return_transition = camera.transition_token();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::View;
    use std::time::Duration;

    fn animated() -> Transition {
        Transition::Animated {
            duration: Duration::from_millis(100),
        }
    }

    #[test]
    fn zoom_anchor_depends_on_navigation_mode_not_projection_alone() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        let mut navigation = ViewNavigation::default();
        let anchor = glam::vec2(0.4, -0.3);
        let mut expected = camera.clone();
        expected.zoom(0.2);
        navigation.zoom(&mut camera, 0.2, Some(anchor), 1.5);
        assert_eq!(camera.view_projection(1.5), expected.view_projection(1.5));
        navigation.enter_planar(&camera);
        expected.zoom_at(0.2, anchor, 1.5);
        navigation.zoom(&mut camera, 0.2, Some(anchor), 1.5);
        assert_eq!(camera.view_projection(1.5), expected.view_projection(1.5));
    }

    #[test]
    fn return_policies_keep_current_framing_and_remember_only_free_entry() {
        for policy in [
            PlanarExit::PerspectiveOnly,
            PlanarExit::OrientationOnly,
            PlanarExit::OrientationAndPerspective,
        ] {
            let mut camera = Camera::default();
            camera.orbit(24.0, -13.0);
            let free_orientation = camera.orientation();
            let mut navigation = ViewNavigation::default();
            assert!(!navigation.is_planar());
            navigation.enter_planar(&camera);
            camera.set_view(View::Front);
            navigation.enter_planar(&camera);
            camera.set_view(View::Top);
            camera.pan(41.0, -27.0, 600.0);
            camera.zoom(0.7);
            let planar_orientation = camera.orientation();
            let mut expected = camera.clone();
            expected.set_orientation_with_transition(
                if policy == PlanarExit::PerspectiveOnly {
                    planar_orientation
                } else {
                    free_orientation
                },
                policy == PlanarExit::OrientationOnly,
                Transition::Instant,
            );
            navigation.return_to_free(&mut camera, policy, Transition::Instant);
            assert!(!navigation.is_planar());
            assert_eq!(camera.view_projection(1.0), expected.view_projection(1.0));
            assert_eq!(camera.eye(), expected.eye());
            let unchanged = camera.view_projection(1.0);
            navigation.return_to_free(&mut camera, PlanarExit::default(), animated());
            assert!(!camera.is_transitioning());
            assert_eq!(camera.view_projection(1.0), unchanged);
        }
    }

    #[test]
    fn returns_start_at_visible_pose_and_reentry_keeps_the_original_destination() {
        let mut camera = Camera::default();
        camera.orbit(30.0, -20.0);
        let remembered = camera.orientation();
        let mut navigation = ViewNavigation::default();
        navigation.enter_planar(&camera);
        camera.set_view(View::Front);
        navigation.return_to_free(&mut camera, PlanarExit::default(), animated());
        camera.advance_transition(Duration::from_millis(30));
        let visible = camera.orientation();
        assert_ne!(visible, remembered);
        let token = camera.transition_token();
        navigation.return_to_free(&mut camera, PlanarExit::default(), animated());
        assert_eq!(camera.transition_token(), token);
        navigation.enter_planar(&camera);
        camera.set_view_with_transition(View::Top, animated());
        camera.advance_transition(Duration::from_millis(20));
        let before_return = camera.view_projection(1.0);
        navigation.return_to_free(&mut camera, PlanarExit::OrientationOnly, animated());
        assert_eq!(camera.view_projection(1.0), before_return);
        camera.finish_transition();
        assert_eq!(camera.orientation(), remembered);
        assert!(camera.is_orthographic());
    }

    #[test]
    fn manual_leave_and_unrelated_retarget_do_not_reuse_return_provenance() {
        for manual_leave in [false, true] {
            let mut camera = Camera::default();
            let mut navigation = ViewNavigation::default();
            navigation.enter_planar(&camera);
            camera.set_view(View::Front);
            navigation.return_to_free(&mut camera, PlanarExit::default(), animated());
            camera.advance_transition(Duration::from_millis(30));
            if manual_leave {
                let unchanged = camera.view_projection(1.0);
                navigation.leave_planar();
                assert_eq!(camera.view_projection(1.0), unchanged);
                camera.orbit(12.0, 5.0);
            } else {
                // Even if a caller replaces the return without leave_planar,
                // its new animation cannot masquerade as the previous return.
                camera.set_view_with_transition(View::Right, animated());
                camera.advance_transition(Duration::from_millis(20));
            }
            let visible = camera.orientation();
            navigation.enter_planar(&camera);
            camera.set_view(View::Bottom);
            navigation.return_to_free(&mut camera, PlanarExit::default(), Transition::Instant);
            assert_eq!(camera.orientation(), visible);
            assert!(!camera.is_orthographic());
        }
    }

    #[test]
    fn missing_return_angle_uses_the_default_without_changing_framing() {
        let mut camera = Camera::default();
        camera.set_view(View::Left);
        camera.pan(-42.0, 18.0, 600.0);
        camera.zoom(0.4);
        let mut navigation = ViewNavigation {
            planar: true,
            ..Default::default()
        };
        let mut expected = camera.clone();
        expected.set_orientation_with_transition(
            CameraOrientation::default(),
            false,
            Transition::Instant,
        );
        navigation.return_to_free(&mut camera, PlanarExit::default(), Transition::Instant);
        assert_eq!(camera.view_projection(1.0), expected.view_projection(1.0));
    }
}
