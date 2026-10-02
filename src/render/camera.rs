use glam::{Mat4, Vec3, camera::rh::proj::directx as projection};
use std::{
    f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU},
    time::Duration,
};

const FOV_Y: f32 = FRAC_PI_4;
const DEFAULT_YAW: f32 = FRAC_PI_4;
const DEFAULT_PITCH: f32 = 0.45;
const MIN_DISTANCE: f32 = 0.001;
const MAX_DISTANCE: f32 = 1_000_000.0;
const BOUNDS_RADIUS: f32 = 1.732_050_8;
// The floor grid fades to transparent at radius 11, beyond the model bounds.
const CLIP_RADIUS: f32 = 12.0;
const FIT_MARGIN: f32 = 1.15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Perspective,
    Front,
    Right,
    Top,
    Back,
    Left,
    Bottom,
}

/// A camera angle without its target, zoom, or projection. Values capture the
/// visible pose, including an intermediate animation frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraOrientation {
    yaw: f32,
    pitch: f32,
}

impl Default for CameraOrientation {
    fn default() -> Self {
        Self {
            yaw: DEFAULT_YAW,
            pitch: DEFAULT_PITCH,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    Instant,
    Animated { duration: Duration },
}

impl Default for Transition {
    fn default() -> Self {
        Self::Animated {
            duration: Duration::from_millis(120),
        }
    }
}

#[derive(Clone, Debug)]
struct CameraTransition {
    start_yaw: f32,
    start_pitch: f32,
    start_projection: f32,
    target_yaw: f32,
    target_pitch: f32,
    target_orthographic: bool,
    // Orbit steps can retain an interrupted projection instead of targeting an endpoint.
    target_projection: Option<f32>,
    directional_orbit: bool,
    framing: Option<FramingTransition>,
    elapsed: Duration,
    duration: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FitBounds {
    center: Vec3,
    radius: f32,
}

#[derive(Clone, Copy, Debug)]
struct CameraFraming {
    target: Vec3,
    distance: f32,
    fit_bounds: Option<FitBounds>,
}

#[derive(Clone, Copy, Debug)]
struct FramingTransition {
    start: CameraFraming,
    end: CameraFraming,
}

impl CameraFraming {
    fn bounds(self) -> FitBounds {
        // This sphere matches the default eye-distance calculation and stays
        // inside CLIP_RADIUS, so it also leaves the default clip planes intact.
        self.fit_bounds.unwrap_or(FitBounds {
            center: Vec3::ZERO,
            radius: BOUNDS_RADIUS,
        })
    }
}

/// Y-up camera; initial framing assumes normalized [-1, 1] geometry.
#[derive(Clone, Debug)]
pub struct Camera {
    target: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    orthographic: bool,
    // A cancelled transition retains its visible intermediate projection.
    projection_blend: Option<f32>,
    transition: Option<CameraTransition>,
    transition_generation: u64,
    // Selection framing can include geometry outside the original normalized bounds.
    fit_bounds: Option<FitBounds>,
}

impl Default for Camera {
    fn default() -> Self {
        let mut camera = Self {
            target: Vec3::ZERO,
            yaw: DEFAULT_YAW,
            pitch: DEFAULT_PITCH,
            distance: 5.0,
            orthographic: false,
            projection_blend: None,
            transition: None,
            transition_generation: 0,
            fit_bounds: None,
        };
        camera.frame(1.0);
        camera
    }
}

impl Camera {
    /// The returned matrix uses wgpu's zero-to-one depth range.
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let (right, up, outward) = self.basis();
        let eye = self.eye();
        let center_depth = eye.dot(outward);
        let minimum_near = (self.distance * 0.005).clamp(0.00001, 0.05);
        let mut nearest = center_depth - CLIP_RADIUS;
        let mut farthest = center_depth + CLIP_RADIUS;
        if let Some(bounds) = self.fit_bounds {
            let depth = (eye - bounds.center).dot(outward);
            nearest = nearest.min(depth - bounds.radius);
            farthest = farthest.max(depth + bounds.radius);
        }
        let near = nearest.max(minimum_near);
        let far = farthest.max(near + 1.0);
        let aspect = valid_aspect(aspect);
        let weight = self.projection_weight();
        let projection = if weight == 0.0 {
            projection::perspective(FOV_Y, aspect, near, far)
        } else {
            let half_height = self.half_height();
            let ortho = projection::orthographic(
                -half_height * aspect,
                half_height * aspect,
                -half_height,
                half_height,
                near,
                far,
            );
            if weight == 1.0 {
                ortho
            } else {
                // Both matrices have w=1 and identical scale at the target plane.
                // Adjust the perspective FOV as the eye moves out of the model,
                // then blend away foreshortening without changing the zoom.
                let eye_distance = self.eye_distance();
                let fov = 2.0 * (half_height / eye_distance).atan();
                let perspective =
                    projection::perspective(fov, aspect, near, far) * eye_distance.recip();
                perspective * (1.0 - weight) + ortho * weight
            }
        };
        // Building the basis directly avoids eye-target cancellation at close zoom.
        let view = Mat4::from_cols(
            Vec3::new(right.x, up.x, outward.x).extend(0.0),
            Vec3::new(right.y, up.y, outward.y).extend(0.0),
            Vec3::new(right.z, up.z, outward.z).extend(0.0),
            Vec3::new(-eye.dot(right), -eye.dot(up), -eye.dot(outward)).extend(1.0),
        );
        projection * view
    }

    pub fn eye(&self) -> Vec3 {
        self.target + self.basis().2 * self.eye_distance()
    }

    pub fn orientation(&self) -> CameraOrientation {
        CameraOrientation {
            yaw: self.yaw,
            pitch: self.pitch,
        }
    }

    /// Change only angle and projection, retaining the current target and zoom.
    /// Retargeting an animation starts at its currently visible pose.
    pub fn set_orientation_with_transition(
        &mut self,
        orientation: CameraOrientation,
        orthographic: bool,
        transition: Transition,
    ) {
        self.request_orientation(orientation.yaw, orientation.pitch, orthographic, transition);
    }

    /// Move to another camera's visible pose, including its target and zoom.
    /// The destination's pending animation is not resumed. Retargeting starts
    /// from the current visible frame; caller-supplied time drives every step.
    pub fn transition_to(&mut self, destination: &Self, transition: Transition) {
        let framing = FramingTransition {
            start: self.framing(),
            end: destination.framing(),
        };
        self.begin_transition(
            destination.yaw,
            destination.pitch,
            destination.orthographic,
            transition,
        );
        if let Some(active) = &mut self.transition {
            active.target_projection = destination.projection_blend;
            active.framing = Some(framing);
        } else {
            self.projection_blend = destination.projection_blend;
            self.apply_framing(framing.end);
        }
    }

    /// Identifies the active animation within this camera's lifetime. Navigation
    /// can recognize its own return animation without confusing a later retarget.
    pub(crate) fn transition_token(&self) -> Option<u64> {
        self.transition.as_ref().map(|_| self.transition_generation)
    }

    /// Transform a world direction into screen-right, screen-up, and toward-eye axes.
    /// Translation, zoom, and projection mode do not affect the result.
    pub fn direction_in_view(&self, direction: Vec3) -> Vec3 {
        let (right, up, outward) = self.basis();
        Vec3::new(
            direction.dot(right),
            direction.dot(up),
            direction.dot(outward),
        )
    }

    /// Closest cardinal view to the visible orientation, including mid-transition.
    /// Maximizing the outward dot product minimizes angular distance. Exact ties
    /// use this fixed order, independently of previous views or navigation mode.
    pub fn nearest_axis_direction(&self) -> Vec3 {
        let outward = self.basis().2;
        let mut nearest = Vec3::Z;
        for axis in [-Vec3::Z, Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y] {
            if outward.dot(axis) > outward.dot(nearest) {
                nearest = axis;
            }
        }
        nearest
    }

    /// Snap to an orthographic view from this world direction, preserving target and zoom.
    #[allow(dead_code)] // Instant orientation is also useful to replay and measurement consumers.
    pub fn look_from(&mut self, direction: Vec3) {
        self.look_from_with_transition(direction, Transition::Instant);
    }

    pub fn look_from_with_transition(&mut self, direction: Vec3, transition: Transition) {
        if let Some((yaw, pitch)) = direction_angles(direction) {
            self.request_orientation(yaw, pitch, true, transition);
        }
    }

    pub fn orbit(&mut self, dx: f32, dy: f32) {
        if !dx.is_finite() || !dy.is_finite() {
            return;
        }
        self.cancel_transition();
        self.yaw = (self.yaw - dx.clamp(-1e6, 1e6) * 0.006).rem_euclid(TAU);
        self.pitch =
            (self.pitch + dy.clamp(-1e6, 1e6) * 0.006).clamp(-FRAC_PI_2 + 0.001, FRAC_PI_2 - 0.001);
    }

    /// Orbit the viewpoint in 15-degree increments: positive means right/up.
    /// Unlike cardinal snaps, this retains the visible projection, including a blend.
    pub fn orbit_direction(
        &mut self,
        horizontal_steps: f32,
        vertical_steps: f32,
        transition: Transition,
    ) {
        if !horizontal_steps.is_finite()
            || !vertical_steps.is_finite()
            || (horizontal_steps == 0.0 && vertical_steps == 0.0)
        {
            return;
        }
        // Distinct quick taps each count as a full step, while the new animation
        // still starts at the visible pose. Cardinal-view transitions do not accumulate.
        let (base_yaw, base_pitch) = self
            .transition
            .as_ref()
            .filter(|active| active.directional_orbit)
            .map(|active| (active.target_yaw, active.target_pitch))
            .unwrap_or((self.yaw, self.pitch));
        let step = PI / 12.0;
        let yaw = (base_yaw + horizontal_steps.rem_euclid(24.0) * step).rem_euclid(TAU);
        let pitch = (base_pitch + vertical_steps.clamp(-24.0, 24.0) * step)
            .clamp(-FRAC_PI_2 + 0.001, FRAC_PI_2 - 0.001);
        let projection = self.projection_blend;
        self.begin_transition(yaw, pitch, self.orthographic, transition);
        if let Some(active) = &mut self.transition {
            active.target_projection = projection;
            active.directional_orbit = true;
        } else {
            self.projection_blend = projection;
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32, viewport_height: f32) {
        if !dx.is_finite() || !dy.is_finite() || !viewport_height.is_finite() {
            return;
        }
        self.cancel_transition();
        let (right, up, _) = self.basis();
        let scale = 2.0 * self.half_height() / viewport_height.max(1.0);
        let target = self.target + (up * dy - right * dx) * scale;
        if target.is_finite() {
            self.target = target.clamp(Vec3::splat(-MAX_DISTANCE), Vec3::splat(MAX_DISTANCE));
        }
    }

    /// Positive amounts zoom in; one unit scales the view by 1/e.
    pub fn zoom(&mut self, amount: f32) {
        if amount.is_finite() {
            self.cancel_transition();
            self.distance = (self.distance * (-amount.clamp(-20.0, 20.0)).exp())
                .clamp(MIN_DISTANCE, MAX_DISTANCE);
        }
    }

    /// Zoom around a point in the target plane. The anchor is in normalized
    /// device coordinates (right/up positive), independent of UI insets.
    pub fn zoom_at(&mut self, amount: f32, anchor: glam::Vec2, aspect: f32) {
        if !amount.is_finite() || !anchor.is_finite() {
            return;
        }
        let before = self.half_height();
        self.zoom(amount);
        let (right, up, _) = self.basis();
        let target = self.target
            + (right * anchor.x * valid_aspect(aspect) + up * anchor.y)
                * (before - self.half_height());
        if target.is_finite() {
            self.target = target.clamp(Vec3::splat(-MAX_DISTANCE), Vec3::splat(MAX_DISTANCE));
        }
    }

    /// Reset the orbit and target, retaining the current projection mode.
    pub fn frame(&mut self, aspect: f32) {
        self.cancel_transition();
        self.target = Vec3::ZERO;
        self.fit_bounds = None;
        self.yaw = DEFAULT_YAW;
        self.pitch = DEFAULT_PITCH;
        let (right, up, outward) = self.basis();
        let aspect = valid_aspect(aspect);
        let tangent = (FOV_Y * 0.5).tan();
        let perspective_weight = 1.0 - self.projection_weight();
        let mut distance: f32 = MIN_DISTANCE;
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    let corner = Vec3::new(x, y, z);
                    let projected = corner.dot(up).abs().max(corner.dot(right).abs() / aspect);
                    let required = FIT_MARGIN * projected / tangent;
                    distance = distance.max(required + perspective_weight * corner.dot(outward));
                }
            }
        }
        self.distance = distance.clamp(MIN_DISTANCE, MAX_DISTANCE);
    }

    /// Frame displayed points without changing the visible orientation or projection.
    /// Empty, nonfinite, and unrepresentable fits leave even an active transition intact.
    /// A coincident selection recenters while retaining the current zoom.
    pub fn frame_points(&mut self, points: &[Vec3], aspect: f32) -> bool {
        let Some(first) = points.first() else {
            return false;
        };
        if points.iter().any(|point| !point.is_finite()) {
            return false;
        }
        // Compute bounds in f64 so finite f32 endpoints cannot overflow their midpoint.
        let mut low = first.as_dvec3();
        let mut high = low;
        for point in points {
            low = low.min(point.as_dvec3());
            high = high.max(point.as_dvec3());
        }
        let target = ((low + high) * 0.5).as_vec3();
        let (right, up, outward) = self.basis();
        let aspect = valid_aspect(aspect);
        let tangent = f64::from((FOV_Y * 0.5).tan());
        let perspective_weight = f64::from(1.0 - self.projection_weight());
        let mut radius = 0.0_f64;
        let mut distance = f64::from(MIN_DISTANCE);
        let mut forward_depth = 0.0_f64;
        for point in points {
            let relative = point.as_dvec3() - target.as_dvec3();
            radius = radius.max(relative.length());
            let depth = relative.dot(outward.as_dvec3()).max(0.0);
            forward_depth = forward_depth.max(depth);
            let projected = relative
                .dot(up.as_dvec3())
                .abs()
                .max(relative.dot(right.as_dvec3()).abs() / f64::from(aspect));
            // The blended projection's eye is at least `distance` away. Positive
            // depth is the conservative case; geometry behind the target needs
            // no extra allowance. This also covers portrait aspect ratios.
            distance = distance
                .max(f64::from(FIT_MARGIN) * projected / tangent + perspective_weight * depth);
        }
        if radius == 0.0 {
            distance = f64::from(self.distance);
        } else {
            // A selection along the viewing axis still needs room in front of
            // the eye and near plane even though its projected width is zero.
            distance = distance.max(perspective_weight * (forward_depth + radius * 0.02 + 0.001));
        }
        if !distance.is_finite() || distance > f64::from(MAX_DISTANCE) {
            return false;
        }
        let mut candidate = self.clone();
        candidate.target = target;
        candidate.distance = distance as f32;
        candidate.fit_bounds = Some(FitBounds {
            center: target,
            radius: (radius * FIT_MARGIN as f64).max(0.001) as f32,
        });
        candidate.cancel_transition();
        let matrix = candidate.view_projection(aspect);
        // The document can contain coordinates beyond useful f32 display
        // precision. Do not publish a camera which cannot actually show them.
        if !matrix.is_finite()
            || points.iter().any(|point| {
                let clip = matrix * point.extend(1.0);
                let ndc = clip.truncate() / clip.w;
                clip.w <= 0.0
                    || !ndc.is_finite()
                    || ndc.x.abs() > 1.0
                    || ndc.y.abs() > 1.0
                    || !(-0.0001..=1.0001).contains(&ndc.z)
            })
        {
            return false;
        }
        *self = candidate;
        true
    }

    pub fn set_view(&mut self, view: View) {
        self.set_view_with_transition(view, Transition::Instant);
    }

    pub fn set_view_with_transition(&mut self, view: View, transition: Transition) {
        let (yaw, pitch, orthographic) = match view {
            View::Perspective => (DEFAULT_YAW, DEFAULT_PITCH, false),
            View::Front => (0.0, 0.0, true),
            View::Right => (FRAC_PI_2, 0.0, true),
            View::Top => (0.0, FRAC_PI_2, true),
            View::Back => (PI, 0.0, true),
            View::Left => (-FRAC_PI_2, 0.0, true),
            View::Bottom => (0.0, -FRAC_PI_2, true),
        };
        self.request_orientation(yaw, pitch, orthographic, transition);
    }

    pub fn toggle_projection(&mut self) {
        let orthographic = self.projection_weight() < 0.5;
        self.apply_orientation(self.yaw, self.pitch, orthographic);
    }

    pub fn is_orthographic(&self) -> bool {
        self.projection_weight() == 1.0
    }

    /// Advance using caller-supplied frame time. Returns whether animation remains active.
    pub fn advance_transition(&mut self, dt: Duration) -> bool {
        let Some(mut transition) = self.transition.take() else {
            return false;
        };
        transition.elapsed = transition.elapsed.saturating_add(dt);
        if transition.elapsed >= transition.duration {
            self.apply_transition_target(&transition);
            return false;
        }
        if transition.elapsed.is_zero() {
            self.transition = Some(transition);
            return true;
        }
        let progress =
            (transition.elapsed.as_secs_f64() / transition.duration.as_secs_f64()) as f32;
        let eased = progress * progress * (3.0 - 2.0 * progress);
        self.yaw = transition.start_yaw
            + shortest_yaw(transition.start_yaw, transition.target_yaw) * eased;
        self.pitch =
            transition.start_pitch + (transition.target_pitch - transition.start_pitch) * eased;
        let target_projection =
            transition
                .target_projection
                .unwrap_or(if transition.target_orthographic {
                    1.0
                } else {
                    0.0
                });
        self.projection_blend = Some(
            transition.start_projection + (target_projection - transition.start_projection) * eased,
        );
        if let Some(framing) = transition.framing {
            let start_bounds = framing.start.bounds();
            let end_bounds = framing.end.bounds();
            self.apply_framing(CameraFraming {
                target: framing.start.target.lerp(framing.end.target, eased),
                // Interpolate zoom proportionally so fitting a much smaller
                // selection does not spend most of the transition far away.
                distance: (framing.start.distance.ln()
                    + (framing.end.distance.ln() - framing.start.distance.ln()) * eased)
                    .exp(),
                // Eye distance and depth bounds must evolve with the framing,
                // especially in orthographic views. Replacing bounds up front
                // would change the visible projection before time advances.
                fit_bounds: Some(FitBounds {
                    center: start_bounds.center.lerp(end_bounds.center, eased),
                    radius: start_bounds.radius + (end_bounds.radius - start_bounds.radius) * eased,
                }),
            });
        }
        self.transition = Some(transition);
        true
    }

    pub fn is_transitioning(&self) -> bool {
        self.transition.is_some()
    }

    /// Freeze the visible pose and framing, including an intermediate projection blend.
    pub fn cancel_transition(&mut self) {
        self.transition = None;
    }

    pub fn finish_transition(&mut self) {
        if let Some(transition) = self.transition.take() {
            self.apply_transition_target(&transition);
        }
    }

    fn apply_transition_target(&mut self, transition: &CameraTransition) {
        self.apply_orientation(
            transition.target_yaw,
            transition.target_pitch,
            transition.target_orthographic,
        );
        self.projection_blend = transition.target_projection;
        if let Some(framing) = transition.framing {
            self.apply_framing(framing.end);
        }
    }

    fn framing(&self) -> CameraFraming {
        CameraFraming {
            target: self.target,
            distance: self.distance,
            fit_bounds: self.fit_bounds,
        }
    }

    fn apply_framing(&mut self, framing: CameraFraming) {
        self.target = framing.target;
        self.distance = framing.distance;
        self.fit_bounds = framing.fit_bounds;
    }

    /// Shared by named views, gizmo directions, and navigation-mode returns.
    /// Repeating the active destination must not restart its animation. Framing
    /// and incremental orbit requests use their own complete targets below.
    fn request_orientation(
        &mut self,
        yaw: f32,
        pitch: f32,
        orthographic: bool,
        transition: Transition,
    ) {
        let matches = |other_yaw: f32, other_pitch: f32, projection: f32| {
            shortest_yaw(other_yaw, yaw).abs() <= 1e-6
                && (other_pitch - pitch).abs() <= 1e-6
                && projection == if orthographic { 1.0 } else { 0.0 }
        };
        if let Some(active) = &self.transition
            && active.framing.is_none()
            && matches(
                active.target_yaw,
                active.target_pitch,
                active
                    .target_projection
                    .unwrap_or(if active.target_orthographic { 1.0 } else { 0.0 }),
            )
        {
            if matches!(
                transition,
                Transition::Instant
                    | Transition::Animated {
                        duration: Duration::ZERO
                    }
            ) {
                self.finish_transition();
            }
            return;
        }
        if matches(self.yaw, self.pitch, self.projection_weight()) {
            // A different pending destination can be cancelled at its unchanged
            // start pose without creating a zero-distance animation.
            self.cancel_transition();
            return;
        }
        self.begin_transition(yaw, pitch, orthographic, transition);
    }

    fn begin_transition(
        &mut self,
        yaw: f32,
        pitch: f32,
        orthographic: bool,
        transition: Transition,
    ) {
        self.transition_generation = self.transition_generation.wrapping_add(1);
        let duration = match transition {
            Transition::Instant => Duration::ZERO,
            Transition::Animated { duration } => duration,
        };
        if duration.is_zero() {
            self.apply_orientation(yaw, pitch, orthographic);
        } else {
            self.transition = Some(CameraTransition {
                start_yaw: self.yaw,
                start_pitch: self.pitch,
                start_projection: self.projection_weight(),
                target_yaw: yaw,
                target_pitch: pitch,
                target_orthographic: orthographic,
                target_projection: None,
                directional_orbit: false,
                framing: None,
                elapsed: Duration::ZERO,
                duration,
            });
        }
    }

    fn apply_orientation(&mut self, yaw: f32, pitch: f32, orthographic: bool) {
        self.yaw = yaw;
        self.pitch = pitch;
        self.orthographic = orthographic;
        self.projection_blend = None;
        self.transition = None;
    }

    fn projection_weight(&self) -> f32 {
        self.projection_blend
            .unwrap_or(if self.orthographic { 1.0 } else { 0.0 })
    }

    fn eye_distance(&self) -> f32 {
        // Orthographic zoom changes scale, without pushing the eye into the model.
        let outside_bounds = match self.fit_bounds {
            Some(bounds) => (self.target - bounds.center).length() + bounds.radius * 2.0,
            None => self.target.length() + BOUNDS_RADIUS * 2.0,
        };
        let ortho_distance = self.distance.max(outside_bounds);
        self.distance + (ortho_distance - self.distance) * self.projection_weight()
    }

    fn half_height(&self) -> f32 {
        self.distance * (FOV_Y * 0.5).tan()
    }

    fn basis(&self) -> (Vec3, Vec3, Vec3) {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        let outward = Vec3::new(cp * sy, sp, cp * cy);
        let right = Vec3::new(cy, 0.0, -sy);
        (right, outward.cross(right), outward)
    }
}

fn shortest_yaw(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

fn direction_angles(direction: Vec3) -> Option<(f32, f32)> {
    if !direction.is_finite() {
        return None;
    }
    let scale = direction.abs().max_element();
    if scale == 0.0 {
        return None;
    }
    // Scale before normalization to avoid overflowing or underflowing length squared.
    let direction = (direction / scale).normalize();
    let horizontal = direction.x.hypot(direction.z);
    Some(if horizontal < 1e-6 {
        (0.0, direction.y.signum() * FRAC_PI_2)
    } else {
        (
            direction.x.atan2(direction.z),
            direction.y.atan2(horizontal),
        )
    })
}

fn valid_aspect(aspect: f32) -> f32 {
    if aspect.is_finite() && aspect > 0.0 {
        aspect.clamp(0.0001, 10_000.0)
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_snapshot_transition_animates_framing_and_restores_exactly() {
        for orthographic in [false, true] {
            let mut camera = Camera::default();
            if orthographic {
                camera.set_view(View::Front);
            }
            let original = camera.clone();
            let before = camera.view_projection(1.5);
            let mut destination = camera.clone();
            assert!(destination.frame_points(
                &box_points(Vec3::new(2.0, 0.5, -0.7), Vec3::splat(0.2)),
                1.5,
            ));
            camera.transition_to(&destination, animated(100));
            assert_eq!(camera.view_projection(1.5), before);
            assert!(camera.advance_transition(Duration::ZERO));
            assert_eq!(camera.view_projection(1.5), before);

            assert!(camera.advance_transition(Duration::from_millis(50)));
            assert_eq!(camera.target, original.target.lerp(destination.target, 0.5));
            assert!(
                (camera.distance - (original.distance * destination.distance).sqrt()).abs() < 1e-6
            );
            assert_eq!(camera.orientation(), original.orientation());
            assert_ne!(camera.view_projection(1.5), before);
            assert_ne!(
                camera.view_projection(1.5),
                destination.view_projection(1.5)
            );

            assert!(!camera.advance_transition(Duration::from_millis(50)));
            assert_eq!(
                camera.view_projection(1.5),
                destination.view_projection(1.5)
            );
            assert_eq!(camera.fit_bounds, destination.fit_bounds);
            camera.transition_to(&original, animated(100));
            camera.finish_transition();
            assert_eq!(camera.view_projection(1.5), before);
            assert_eq!(camera.fit_bounds, None);
        }
    }

    #[test]
    fn camera_snapshot_transition_uses_visible_destination_without_resuming_it() {
        let mut destination = Camera::default();
        destination.set_view_with_transition(View::Top, animated(100));
        destination.advance_transition(Duration::from_millis(35));
        assert!(destination.is_transitioning());
        for transition in [Transition::Instant, animated(0), animated(100)] {
            let mut camera = Camera::default();
            camera.pan(100.0, -70.0, 600.0);
            camera.zoom(0.4);
            camera.transition_to(&destination, transition);
            assert_eq!(camera.is_transitioning(), transition == animated(100));
            camera.finish_transition();
            assert_eq!(
                camera.view_projection(0.7),
                destination.view_projection(0.7)
            );
            assert_eq!(camera.projection_blend, destination.projection_blend);
            assert!(!camera.is_transitioning());
        }
        assert!(destination.is_transitioning());
    }

    #[test]
    fn reversing_or_interrupting_framing_keeps_the_visible_camera() {
        let mut camera = Camera::default();
        let original = camera.clone();
        let mut destination = camera.clone();
        assert!(
            destination.frame_points(&box_points(Vec3::new(3.0, 2.0, 1.0), Vec3::splat(0.1)), 1.0,)
        );
        destination.set_view(View::Right);
        camera.transition_to(&destination, animated(100));
        camera.advance_transition(Duration::from_millis(35));
        let before = camera.view_projection(1.0);
        let token = camera.transition_token();
        camera.transition_to(&original, animated(100));
        assert_eq!(camera.view_projection(1.0), before);
        assert_ne!(camera.transition_token(), token);
        camera.advance_transition(Duration::from_millis(25));
        let before_cancel = camera.view_projection(1.0);
        camera.pan(0.0, 0.0, 600.0);
        assert_eq!(camera.view_projection(1.0), before_cancel);
        assert!(!camera.is_transitioning());
        camera.advance_transition(Duration::from_secs(1));
        assert_eq!(camera.view_projection(1.0), before_cancel);

        let target = camera.target;
        let distance = camera.distance;
        let bounds = camera.fit_bounds;
        camera.set_view_with_transition(View::Bottom, animated(100));
        camera.finish_transition();
        assert_eq!(camera.target, target);
        assert_eq!(camera.distance, distance);
        assert_eq!(camera.fit_bounds, bounds);
    }

    #[test]
    fn orthographic_framing_depth_is_continuous_at_both_ends() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        camera.pan(30.0, -40.0, 600.0);
        camera.zoom(1.0);
        let original = camera.clone();
        let mut destination = camera.clone();
        assert!(destination.frame_points(
            &box_points(Vec3::new(0.5, 0.1, 0.0), Vec3::new(0.01, 0.02, 1.5)),
            1.0,
        ));
        for destination in [&destination, &original] {
            let before = camera.view_projection(1.0);
            camera.transition_to(destination, animated(100));
            camera.advance_transition(Duration::from_micros(1));
            assert!(camera.view_projection(1.0).abs_diff_eq(before, 1e-5));
            camera.advance_transition(Duration::from_micros(99_998));
            assert!(
                camera
                    .view_projection(1.0)
                    .abs_diff_eq(destination.view_projection(1.0), 1e-5)
            );
            camera.advance_transition(Duration::from_micros(1));
            assert_eq!(
                camera.view_projection(1.0),
                destination.view_projection(1.0)
            );
        }
    }

    #[test]
    fn captured_orientation_restores_visible_angles_without_restoring_old_framing() {
        let mut camera = Camera::default();
        camera.set_view_with_transition(
            View::Top,
            Transition::Animated {
                duration: Duration::from_millis(100),
            },
        );
        camera.advance_transition(Duration::from_millis(35));
        let orientation = camera.orientation();
        let basis = camera.basis();
        camera.finish_transition();
        camera.pan(70.0, -25.0, 600.0);
        camera.zoom(0.8);
        let (target, distance) = (camera.target, camera.distance);
        camera.set_orientation_with_transition(orientation, false, Transition::Instant);
        assert_eq!(camera.orientation(), orientation);
        assert_eq!(camera.basis(), basis);
        assert_eq!((camera.target, camera.distance), (target, distance));
        assert!(!camera.is_orthographic());
        assert!(camera.view_projection(1.0).is_finite());
    }

    #[test]
    fn orientation_transition_retargets_continuously_and_reaches_exact_poles() {
        let mut camera = Camera::default();
        let transition = Transition::Animated {
            duration: Duration::from_millis(100),
        };
        camera.set_view_with_transition(View::Right, transition);
        camera.advance_transition(Duration::from_millis(30));
        let before = camera.view_projection(1.0);
        let first_token = camera.transition_token();
        let mut pole = Camera::default();
        pole.set_view(View::Bottom);
        camera.set_orientation_with_transition(pole.orientation(), false, transition);
        assert_eq!(camera.view_projection(1.0), before);
        assert_ne!(camera.transition_token(), first_token);
        let (target, distance) = (camera.target, camera.distance);
        for _ in 0..4 {
            camera.advance_transition(Duration::from_millis(25));
            assert!(camera.view_projection(1.0).is_finite());
        }
        assert_eq!(camera.orientation(), pole.orientation());
        assert_eq!((camera.target, camera.distance), (target, distance));
        assert_eq!(camera.transition_token(), None);
        assert!(!camera.is_orthographic());
    }

    #[test]
    fn nearest_axis_follows_all_six_directions_and_angular_boundaries() {
        let mut camera = Camera::default();
        for axis in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            camera.look_from(axis + Vec3::new(0.13, -0.09, 0.17));
            camera.pan(50.0, -23.0, 600.0);
            camera.zoom(0.3);
            assert_eq!(camera.nearest_axis_direction(), axis);
            camera.toggle_projection();
            assert_eq!(camera.nearest_axis_direction(), axis);
        }
        camera.pitch = 0.0;
        camera.yaw = FRAC_PI_4 - 0.001;
        assert_eq!(camera.nearest_axis_direction(), Vec3::Z);
        camera.yaw = FRAC_PI_4 + 0.001;
        assert_eq!(camera.nearest_axis_direction(), Vec3::X);
        camera.yaw = 0.0;
        camera.pitch = FRAC_PI_4 - 0.001;
        assert_eq!(camera.nearest_axis_direction(), Vec3::Z);
        camera.pitch = FRAC_PI_4 + 0.001;
        assert_eq!(camera.nearest_axis_direction(), Vec3::Y);
    }

    #[test]
    fn nearest_axis_reads_the_visible_transition_pose_without_changing_it() {
        let mut camera = Camera::default();
        camera.set_view(View::Right);
        camera.set_view_with_transition(
            View::Top,
            Transition::Animated {
                duration: Duration::from_millis(100),
            },
        );
        camera.advance_transition(Duration::from_millis(25));
        let visible = camera.view_projection(1.0);
        assert_eq!(camera.nearest_axis_direction(), Vec3::X);
        assert_eq!(camera.view_projection(1.0), visible);
        assert!(camera.is_transitioning());
        camera.advance_transition(Duration::from_millis(55));
        assert_eq!(camera.nearest_axis_direction(), Vec3::Y);
    }

    #[test]
    fn frames_every_cube_corner_in_portrait_and_landscape() {
        for orthographic in [false, true] {
            for aspect in [0.0001, 0.05, 0.5, 1.0, 16.0 / 9.0, 20.0, 10_000.0] {
                let mut camera = Camera {
                    orthographic,
                    ..Camera::default()
                };
                camera.frame(aspect);
                let matrix = camera.view_projection(aspect);
                for x in [-1.0, 1.0] {
                    for y in [-1.0, 1.0] {
                        for z in [-1.0, 1.0] {
                            let clip = matrix * Vec3::new(x, y, z).extend(1.0);
                            let ndc = clip.truncate() / clip.w;
                            assert!(clip.w > 0.0 && ndc.is_finite());
                            assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
                            assert!((0.0..=1.0).contains(&ndc.z));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn anchored_zoom_preserves_the_screen_point_in_every_axis_view() {
        for view in [
            View::Front,
            View::Back,
            View::Left,
            View::Right,
            View::Top,
            View::Bottom,
        ] {
            for aspect in [0.6, 1.8] {
                let mut camera = Camera::default();
                camera.set_view(view);
                camera.pan(32.0, -19.0, 600.0);
                let anchor = glam::vec2(0.45, -0.35);
                let point = camera
                    .view_projection(aspect)
                    .inverse()
                    .project_point3(anchor.extend(0.5));
                for amount in [0.4, -0.7, 20.0, 0.5] {
                    camera.zoom_at(amount, anchor, aspect);
                    let projected = camera.view_projection(aspect).project_point3(point);
                    // Close zoom magnifies ordinary f32 world-coordinate error.
                    assert!(
                        projected.truncate().abs_diff_eq(anchor, 0.002),
                        "{view:?}: {projected:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn pan_tracks_screen_pixels_and_current_zoom() {
        let mut camera = Camera::default();
        let (right, up, outward) = camera.basis();
        let scale = 2.0 * camera.half_height() / 600.0;
        camera.pan(80.0, 30.0, 600.0);
        assert!((camera.target.dot(right) + 80.0 * scale).abs() < 1e-5);
        assert!((camera.target.dot(up) - 30.0 * scale).abs() < 1e-5);
        assert!(camera.target.dot(outward).abs() < 1e-5);
        let before = camera.target;
        camera.zoom(2.0_f32.ln());
        camera.pan(80.0, 30.0, 600.0);
        assert!(((camera.target - before) - before * 0.5).length() < 1e-5);
    }

    #[test]
    fn projection_toggle_preserves_target_scale_and_removes_foreshortening() {
        let mut camera = Camera::default();
        let (right, _, outward) = camera.basis();
        let point = right * 0.25;
        let farther = point - outward;
        let perspective = camera.view_projection(1.0);
        let near_x = perspective.project_point3(point).x;
        assert!(near_x > perspective.project_point3(farther).x);
        camera.toggle_projection();
        assert!(camera.is_orthographic());
        let ortho = camera.view_projection(1.0);
        assert!((near_x - ortho.project_point3(point).x).abs() < 1e-5);
        assert!((ortho.project_point3(point).x - ortho.project_point3(farther).x).abs() < 1e-5);
    }

    #[test]
    fn distant_views_keep_the_fading_grid_inside_the_depth_range() {
        for orthographic in [false, true] {
            for zoom in [-1.5, -4.0] {
                let mut camera = Camera {
                    orthographic,
                    ..Camera::default()
                };
                camera.zoom(zoom);
                let matrix = camera.view_projection(1.0);
                for point in [
                    Vec3::new(10.9, -1.05, 0.0),
                    Vec3::new(-10.9, -1.05, 0.0),
                    Vec3::new(0.0, -1.05, 10.9),
                    Vec3::new(0.0, -1.05, -10.9),
                ] {
                    let depth = matrix.project_point3(point).z;
                    assert!((0.0..=1.0).contains(&depth));
                }
            }
        }
    }

    #[test]
    fn extreme_zoom_and_pole_views_remain_finite() {
        let mut camera = Camera::default();
        for view in [
            View::Perspective,
            View::Front,
            View::Right,
            View::Top,
            View::Back,
            View::Left,
            View::Bottom,
        ] {
            camera.set_view(view);
            for zoom in [f32::MAX, -f32::MAX, f32::NAN, f32::INFINITY] {
                camera.zoom(zoom);
                camera.orbit(f32::MAX, f32::MAX);
                assert!(camera.eye().is_finite());
                assert!(camera.view_projection(0.0).is_finite());
                assert!(camera.pitch.abs() < FRAC_PI_2);
            }
        }
        camera.set_view(View::Top);
        assert!(camera.view_projection(1.0).is_finite());
        camera.zoom(f32::MAX);
        assert!(camera.eye().length() > BOUNDS_RADIUS);
        let center = camera.view_projection(1.0).project_point3(Vec3::ZERO);
        assert!((0.0..=1.0).contains(&center.z));
    }

    #[test]
    fn cardinal_snaps_preserve_target_and_zoom_with_stable_poles() {
        let mut camera = Camera::default();
        camera.pan(80.0, -30.0, 600.0);
        camera.zoom(0.7);
        let target = camera.target;
        let distance = camera.distance;
        for direction in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            camera.look_from(direction);
            assert!(camera.is_orthographic());
            assert_eq!(camera.target, target);
            assert_eq!(camera.distance, distance);
            assert!(camera.view_projection(1.5).is_finite());
            assert!(((camera.eye() - target).normalize() - direction).length() < 1e-5);
            assert!((camera.direction_in_view(direction) - Vec3::Z).length() < 1e-5);
            if direction.y.abs() == 1.0 {
                assert_eq!(camera.yaw, 0.0);
                assert!((camera.direction_in_view(Vec3::X) - Vec3::X).length() < 1e-5);
            }
        }
    }

    #[test]
    fn gizmo_directions_match_rendered_axes_with_either_source_up_axis() {
        let mut camera = Camera::default();
        camera.orbit(43.0, -29.0);
        camera.pan(80.0, -30.0, 600.0);
        for orthographic in [false, true] {
            camera.orthographic = orthographic;
            for aspect in [0.5, 2.0] {
                let matrix = camera.view_projection(aspect);
                let y_scale = if orthographic {
                    camera.half_height()
                } else {
                    (FOV_Y * 0.5).tan()
                };
                for model in [Mat4::IDENTITY, Mat4::from_rotation_x(-FRAC_PI_2)] {
                    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
                        let world_direction = model.transform_vector3(axis);
                        let screen_direction = camera.direction_in_view(world_direction);
                        // w=0 cancels translation; undo projection scale to compare orientation.
                        let clip_direction = matrix * world_direction.extend(0.0);
                        assert!(
                            (clip_direction.x * y_scale * aspect - screen_direction.x).abs() < 1e-5
                        );
                        assert!((clip_direction.y * y_scale - screen_direction.y).abs() < 1e-5);
                        let outward = (camera.eye() - camera.target).normalize();
                        assert!((screen_direction.z - world_direction.dot(outward)).abs() < 1e-5);
                    }
                }
            }
        }
    }

    #[test]
    fn gizmo_orientation_ignores_pan_zoom_and_projection() {
        let mut camera = Camera::default();
        camera.orbit(-24.0, 17.0);
        let directions = [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(0.3, -0.4, 0.8)];
        let before = directions.map(|direction| camera.direction_in_view(direction));
        camera.pan(-300.0, 140.0, 600.0);
        camera.zoom(3.0);
        camera.toggle_projection();
        let after = directions.map(|direction| camera.direction_in_view(direction));
        assert_eq!(before, after);
    }

    #[test]
    fn invalid_snap_directions_leave_the_camera_unchanged() {
        let mut camera = Camera::default();
        let original = camera.clone();
        for direction in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::new(f32::INFINITY, 0.0, 0.0),
        ] {
            camera.look_from(direction);
            assert_eq!(camera.yaw, original.yaw);
            assert_eq!(camera.pitch, original.pitch);
            assert_eq!(camera.target, original.target);
            assert_eq!(camera.distance, original.distance);
            assert_eq!(camera.orthographic, original.orthographic);
        }
        for magnitude in [f32::MIN_POSITIVE, f32::MAX] {
            camera.look_from(Vec3::X * magnitude);
            assert!((camera.direction_in_view(Vec3::X) - Vec3::Z).length() < 1e-5);
        }
    }

    fn animated(milliseconds: u64) -> Transition {
        Transition::Animated {
            duration: Duration::from_millis(milliseconds),
        }
    }

    #[test]
    fn instant_and_zero_duration_align_without_scheduling() {
        assert_eq!(Transition::default(), animated(120));
        for transition in [Transition::Instant, animated(0)] {
            let mut camera = Camera::default();
            camera.pan(42.0, -17.0, 600.0);
            camera.zoom(0.4);
            let target = camera.target;
            let distance = camera.distance;
            camera.look_from_with_transition(-Vec3::Y, transition);
            assert!(!camera.is_transitioning());
            assert!(camera.is_orthographic());
            assert_eq!(camera.target, target);
            assert_eq!(camera.distance, distance);
            assert_eq!(camera.yaw, 0.0);
            assert_eq!(camera.pitch, -FRAC_PI_2);
            assert!(!camera.advance_transition(Duration::from_secs(1)));
            camera.set_view_with_transition(View::Perspective, transition);
            assert!(!camera.is_orthographic());
            assert!(!camera.is_transitioning());
            assert_eq!(camera.yaw, DEFAULT_YAW);
            assert_eq!(camera.pitch, DEFAULT_PITCH);
        }
    }

    #[test]
    fn repeated_view_requests_do_not_start_or_restart_animation() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        let matrix = camera.view_projection(1.0);
        camera.set_view_with_transition(View::Front, animated(100));
        camera.look_from_with_transition(Vec3::Z, animated(100));
        assert!(!camera.is_transitioning());
        assert_eq!(camera.view_projection(1.0), matrix);

        camera.set_view_with_transition(View::Right, animated(100));
        camera.advance_transition(Duration::from_millis(40));
        let token = camera.transition_token();
        let matrix = camera.view_projection(1.0);
        camera.look_from_with_transition(Vec3::X, animated(100));
        assert_eq!(camera.transition_token(), token);
        assert_eq!(camera.view_projection(1.0), matrix);
        assert!(!camera.advance_transition(Duration::from_millis(60)));

        camera.set_view_with_transition(View::Back, animated(100));
        camera.advance_transition(Duration::from_millis(40));
        camera.look_from_with_transition(Vec3::NEG_Z, animated(100));
        assert!(!camera.advance_transition(Duration::from_millis(60)));
        camera.set_view_with_transition(View::Front, animated(100));
        camera.set_view(View::Front);
        assert!(
            !camera.is_transitioning(),
            "An instant request still finishes immediately"
        );
    }

    #[test]
    fn animation_uses_smoothstep_and_finishes_at_the_exact_destination() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        camera.pan(21.0, -19.0, 600.0);
        let target = camera.target;
        let distance = camera.distance;
        camera.set_view_with_transition(View::Right, animated(1_000));
        assert!(camera.advance_transition(Duration::ZERO));
        assert_eq!(camera.yaw, 0.0);
        assert!(camera.advance_transition(Duration::from_millis(250)));
        assert!((camera.yaw - FRAC_PI_2 * 0.15625).abs() < 1e-6);
        assert!(camera.advance_transition(Duration::from_millis(250)));
        assert!((camera.yaw - FRAC_PI_4).abs() < 1e-6);
        assert!(!camera.advance_transition(Duration::from_millis(500)));
        assert_eq!(camera.yaw, FRAC_PI_2);
        assert_eq!(camera.pitch, 0.0);
        assert_eq!(camera.target, target);
        assert_eq!(camera.distance, distance);
        assert!(camera.is_orthographic());
        assert!(!camera.is_transitioning());
    }

    #[test]
    fn opposite_axes_and_yaw_wrap_take_finite_short_paths() {
        for direction in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            let mut camera = Camera::default();
            camera.look_from(direction);
            camera.look_from_with_transition(-direction, animated(1_000));
            for step in 1..=10 {
                assert_eq!(
                    camera.advance_transition(Duration::from_millis(100)),
                    step < 10
                );
                assert!(camera.eye().is_finite());
                assert!(camera.view_projection(1.0).is_finite());
                let outward = camera.basis().2;
                assert!((outward.length() - 1.0).abs() < 1e-5);
                if step == 5 {
                    assert!(outward.dot(direction).abs() < 1e-5);
                }
            }
            assert!((camera.basis().2 + direction).length() < 1e-5);
        }
        let mut camera = Camera::default();
        let direction = |degrees: f32| {
            let (sin, cos) = degrees.to_radians().sin_cos();
            Vec3::new(sin, 0.0, cos)
        };
        camera.look_from(direction(179.0));
        camera.look_from_with_transition(direction(-179.0), animated(1_000));
        camera.advance_transition(Duration::from_millis(500));
        assert!((camera.basis().2 + Vec3::Z).length() < 1e-5);
        camera.finish_transition();
        assert!((camera.basis().2 - direction(-179.0)).length() < 1e-5);
        assert!(!camera.is_transitioning());
    }

    #[test]
    fn retargeting_starts_from_the_visible_intermediate_state() {
        let mut camera = Camera::default();
        camera.look_from_with_transition(Vec3::X, animated(250));
        camera.advance_transition(Duration::from_millis(90));
        let matrix = camera.view_projection(1.5);
        let eye = camera.eye();
        camera.look_from_with_transition(-Vec3::Z, animated(400));
        assert_eq!(camera.view_projection(1.5), matrix);
        assert_eq!(camera.eye(), eye);
        camera.advance_transition(Duration::ZERO);
        assert_eq!(camera.view_projection(1.5), matrix);
        assert!(!camera.advance_transition(Duration::from_millis(400)));
        assert!((camera.basis().2 + Vec3::Z).length() < 1e-5);
    }

    #[test]
    fn animation_depends_on_elapsed_time_rather_than_frame_count() {
        let mut one_frame = Camera::default();
        one_frame.look_from_with_transition(-Vec3::Y, animated(250));
        let mut many_frames = one_frame.clone();
        one_frame.advance_transition(Duration::from_millis(125));
        for elapsed in [7, 16, 23, 1, 39, 39] {
            many_frames.advance_transition(Duration::from_millis(elapsed));
        }
        assert_eq!(
            one_frame.view_projection(0.5),
            many_frames.view_projection(0.5)
        );
        assert_eq!(one_frame.eye(), many_frames.eye());
        assert!(!one_frame.advance_transition(Duration::MAX));
        assert!(!many_frames.advance_transition(Duration::from_millis(125)));
        assert_eq!(
            one_frame.view_projection(0.5),
            many_frames.view_projection(0.5)
        );
    }

    #[test]
    fn cancellation_and_manual_controls_do_not_jump_to_the_old_destination() {
        let mut camera = Camera::default();
        camera.look_from_with_transition(Vec3::X, animated(250));
        camera.advance_transition(Duration::from_millis(125));
        let matrix = camera.view_projection(1.0);
        for action in [
            Camera::cancel_transition,
            |camera: &mut Camera| camera.orbit(0.0, 0.0),
            |camera: &mut Camera| camera.pan(0.0, 0.0, 600.0),
            |camera: &mut Camera| camera.zoom(0.0),
        ] {
            let mut interrupted = camera.clone();
            action(&mut interrupted);
            assert!(!interrupted.is_transitioning());
            assert_eq!(interrupted.view_projection(1.0), matrix);
            assert!(!interrupted.advance_transition(Duration::from_secs(1)));
            assert_eq!(interrupted.view_projection(1.0), matrix);
        }
        let mut framed = camera.clone();
        framed.frame(1.0);
        assert!(!framed.is_transitioning());
        assert_eq!(framed.target, Vec3::ZERO);
        assert_eq!(framed.projection_weight(), camera.projection_weight());
        let mut toggled = camera.clone();
        toggled.toggle_projection();
        assert!(!toggled.is_transitioning());
        assert_eq!(toggled.yaw, camera.yaw);
        assert_eq!(toggled.pitch, camera.pitch);
    }

    #[test]
    fn invalid_directions_do_not_interrupt_an_active_transition() {
        let mut camera = Camera::default();
        camera.look_from_with_transition(Vec3::X, animated(250));
        camera.advance_transition(Duration::from_millis(50));
        let mut reference = camera.clone();
        for direction in [
            Vec3::ZERO,
            Vec3::splat(f32::NAN),
            Vec3::splat(f32::INFINITY),
        ] {
            camera.look_from_with_transition(direction, Transition::Instant);
            camera.look_from_with_transition(direction, animated(300));
        }
        camera.advance_transition(Duration::from_millis(50));
        reference.advance_transition(Duration::from_millis(50));
        assert!(camera.is_transitioning());
        assert_eq!(camera.view_projection(1.0), reference.view_projection(1.0));
    }

    #[test]
    fn blended_projection_is_continuous_at_poles_and_deep_zoom() {
        for pole in [Vec3::Y, -Vec3::Y] {
            for zoom in [0.0, 6.0] {
                for start_orthographic in [false, true] {
                    let mut camera = Camera::default();
                    camera.look_from(pole);
                    if !start_orthographic {
                        camera.toggle_projection();
                    }
                    camera.pan(30.0, -15.0, 600.0);
                    camera.zoom(zoom);
                    camera.begin_transition(
                        camera.yaw,
                        camera.pitch,
                        !start_orthographic,
                        animated(1_000),
                    );
                    let point = camera.target + camera.basis().0 * camera.half_height() * 0.5;
                    let initial = camera.view_projection(1.0).project_point3(point);
                    camera.advance_transition(Duration::from_nanos(1));
                    let after_start = camera.view_projection(1.0).project_point3(point);
                    assert!((initial - after_start).length() < 0.003);
                    camera.advance_transition(Duration::from_millis(500));
                    assert!(camera.view_projection(1.0).is_finite());
                    let midpoint = camera.view_projection(1.0).project_point3(point);
                    assert!((midpoint.x - initial.x).abs() < 0.003);
                    let mut cancelled = camera.clone();
                    let matrix = cancelled.view_projection(1.0);
                    let eye = cancelled.eye();
                    cancelled.cancel_transition();
                    assert_eq!(cancelled.view_projection(1.0), matrix);
                    assert_eq!(cancelled.eye(), eye);
                    camera.advance_transition(Duration::from_nanos(499_999_998));
                    let before_end = camera.view_projection(1.0).project_point3(point);
                    assert!(!camera.advance_transition(Duration::from_nanos(1)));
                    let after_end = camera.view_projection(1.0).project_point3(point);
                    assert!((before_end - after_end).length() < 0.003);
                    assert!((after_end.x - initial.x).abs() < 0.003);
                    assert_eq!(camera.is_orthographic(), !start_orthographic);
                }
            }
        }
    }

    #[test]
    fn framing_an_interrupted_projection_still_fits_all_corners() {
        for weight in [0.1, 0.5, 0.9] {
            for aspect in [0.05, 0.5, 1.0, 20.0] {
                let mut camera = Camera {
                    projection_blend: Some(weight),
                    ..Camera::default()
                };
                camera.frame(aspect);
                let matrix = camera.view_projection(aspect);
                for x in [-1.0, 1.0] {
                    for y in [-1.0, 1.0] {
                        for z in [-1.0, 1.0] {
                            let point = matrix.project_point3(Vec3::new(x, y, z));
                            assert!(point.is_finite());
                            assert!(point.x.abs() < 1.0 && point.y.abs() < 1.0);
                            assert!((0.0..=1.0).contains(&point.z));
                        }
                    }
                }
            }
        }
    }

    fn box_points(center: Vec3, extent: Vec3) -> Vec<Vec3> {
        let mut points = Vec::new();
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    points.push(center + extent * Vec3::new(x, y, z));
                }
            }
        }
        points
    }

    #[test]
    fn selection_fit_preserves_view_and_fits_translated_geometry_in_every_projection() {
        let points = box_points(Vec3::new(120.0, -30.0, 85.0), Vec3::new(2.0, 4.0, 3.0));
        for weight in [0.0, 0.2, 0.5, 0.8, 1.0] {
            for aspect in [0.15, 0.7, 1.0, 2.0, 12.0] {
                for direction in [Vec3::Z, Vec3::new(1.0, 2.0, -3.0), Vec3::Y] {
                    let mut camera = Camera::default();
                    camera.look_from(direction);
                    camera.projection_blend = Some(weight);
                    let orientation = camera.basis();
                    assert!(
                        camera.frame_points(&points, aspect),
                        "weight={weight}, aspect={aspect}"
                    );
                    assert_eq!(camera.basis(), orientation);
                    assert_eq!(camera.projection_weight(), weight);
                    assert_eq!(camera.target, Vec3::new(120.0, -30.0, 85.0));
                    let matrix = camera.view_projection(aspect);
                    for point in &points {
                        let ndc = matrix.project_point3(*point);
                        assert!(ndc.x.abs() < 0.9 && ndc.y.abs() < 0.9, "{ndc:?}");
                        assert!((0.0..=1.0).contains(&ndc.z), "{ndc:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn selection_fit_clips_large_geometry_and_survives_subsequent_orthographic_zoom() {
        let points = box_points(
            Vec3::new(-800.0, 900.0, 400.0),
            Vec3::new(300.0, 500.0, 700.0),
        );
        for orthographic in [false, true] {
            let mut camera = Camera {
                orthographic,
                ..Camera::default()
            };
            assert!(camera.frame_points(&points, 0.5));
            for point in &points {
                let ndc = camera.view_projection(0.5).project_point3(*point);
                assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
                assert!((0.0..=1.0).contains(&ndc.z));
            }
            if orthographic {
                camera.zoom(20.0);
                let bounds = camera.fit_bounds.unwrap();
                assert!(camera.eye().distance(bounds.center) > bounds.radius);
                for point in &points {
                    let depth = camera.view_projection(0.5).project_point3(*point).z;
                    assert!((0.0..=1.0).contains(&depth));
                }
            }
        }
    }

    #[test]
    fn single_point_recenters_without_zoom_and_axis_aligned_depth_selection_fits() {
        for weight in [0.0, 0.25, 1.0] {
            let mut camera = Camera::default();
            camera.look_from(Vec3::Z);
            camera.projection_blend = Some(weight);
            let distance = camera.distance;
            let target = Vec3::new(80.0, -20.0, 40.0);
            assert!(camera.frame_points(&[target, target], 0.5));
            assert_eq!(camera.distance, distance);
            let ndc = camera.view_projection(0.5).project_point3(target);
            assert!(ndc.x.abs() < 0.001 && ndc.y.abs() < 0.001);
            assert!((0.0..=1.0).contains(&ndc.z));
            let points = [target - Vec3::Z * 5.0, target + Vec3::Z * 5.0];
            assert!(camera.frame_points(&points, 0.5));
            for point in points {
                let ndc = camera.view_projection(0.5).project_point3(point);
                assert!(ndc.is_finite() && (0.0..=1.0).contains(&ndc.z));
            }
        }
    }

    #[test]
    fn selection_fit_only_cancels_animation_after_success() {
        let mut camera = Camera::default();
        camera.set_view_with_transition(View::Front, animated(1_000));
        camera.advance_transition(Duration::from_millis(300));
        let matrix = camera.view_projection(1.0);
        let orientation = camera.basis();
        let weight = camera.projection_weight();
        for points in [
            vec![],
            vec![Vec3::NAN],
            vec![Vec3::INFINITY],
            vec![-Vec3::splat(f32::MAX), Vec3::splat(f32::MAX)],
        ] {
            assert!(!camera.frame_points(&points, 1.0));
            assert_eq!(camera.view_projection(1.0), matrix);
            assert!(camera.is_transitioning());
        }
        assert!(camera.frame_points(&box_points(Vec3::splat(20.0), Vec3::ONE), 1.0));
        assert!(!camera.is_transitioning());
        assert_eq!(camera.basis(), orientation);
        assert_eq!(camera.projection_weight(), weight);
        camera.frame(1.0);
        assert!(camera.fit_bounds.is_none());
    }

    #[test]
    fn discrete_orbits_use_viewpoint_directions_and_preserve_projection_target_zoom() {
        for (horizontal, vertical, expected) in [
            (1.0, 0.0, Vec3::X),
            (-1.0, 0.0, -Vec3::X),
            (0.0, 1.0, Vec3::Y),
            (0.0, -1.0, -Vec3::Y),
        ] {
            for weight in [0.0, 0.35, 1.0] {
                for transition in [Transition::Instant, animated(200)] {
                    let mut camera = Camera::default();
                    camera.look_from(Vec3::Z);
                    camera.pan(20.0, 10.0, 600.0);
                    camera.projection_blend = Some(weight);
                    let target = camera.target;
                    let distance = camera.distance;
                    camera.orbit_direction(horizontal, vertical, transition);
                    if camera.is_transitioning() {
                        camera.advance_transition(Duration::from_millis(100));
                        assert_eq!(camera.projection_weight(), weight);
                        camera.advance_transition(Duration::from_millis(100));
                    }
                    let outward = camera.basis().2;
                    assert!((outward.dot(expected) - (PI / 12.0).sin()).abs() < 1e-6);
                    assert!((outward.dot(Vec3::Z) - (PI / 12.0).cos()).abs() < 1e-6);
                    assert_eq!(camera.projection_weight(), weight);
                    assert_eq!(camera.target, target);
                    assert_eq!(camera.distance, distance);
                    assert!((0.0..TAU).contains(&camera.yaw));
                }
            }
        }
    }

    #[test]
    fn discrete_orbits_retarget_visibly_clamp_pitch_and_ignore_nonfinite_input() {
        let mut camera = Camera::default();
        camera.set_view_with_transition(View::Front, animated(500));
        camera.advance_transition(Duration::from_millis(150));
        let matrix = camera.view_projection(1.0);
        camera.orbit_direction(f32::NAN, 0.0, Transition::Instant);
        camera.orbit_direction(0.0, f32::INFINITY, Transition::Instant);
        assert_eq!(camera.view_projection(1.0), matrix);
        assert!(camera.is_transitioning());
        let projection = camera.projection_weight();
        camera.orbit_direction(1.0, 0.0, animated(200));
        assert_eq!(camera.view_projection(1.0), matrix);
        camera.finish_transition();
        assert_eq!(camera.projection_weight(), projection);
        camera.orbit_direction(f32::MAX, f32::MAX, Transition::Instant);
        assert!((0.0..TAU).contains(&camera.yaw));
        assert_eq!(camera.pitch, FRAC_PI_2 - 0.001);
        camera.orbit_direction(-f32::MAX, -f32::MAX, Transition::Instant);
        assert_eq!(camera.pitch, -FRAC_PI_2 + 0.001);
        assert!(camera.view_projection(1.0).is_finite());
    }

    #[test]
    fn consecutive_directional_taps_accumulate_without_visual_jumps() {
        let mut camera = Camera::default();
        camera.look_from(Vec3::Z);
        camera.orbit_direction(1.0, 0.0, animated(200));
        camera.advance_transition(Duration::from_millis(40));
        let visible = camera.view_projection(1.0);
        camera.orbit_direction(1.0, 0.0, animated(200));
        assert_eq!(camera.view_projection(1.0), visible);
        camera.finish_transition();
        assert!((camera.yaw - PI / 6.0).abs() < 1e-6);
        camera.orbit_direction(-1.0, 1.0, animated(200));
        camera.advance_transition(Duration::from_millis(50));
        let visible = camera.view_projection(1.0);
        camera.orbit_direction(1.0, -1.0, animated(200));
        assert_eq!(camera.view_projection(1.0), visible);
        camera.finish_transition();
        assert!((shortest_yaw(camera.yaw, PI / 6.0)).abs() < 1e-6);
        assert!(camera.pitch.abs() < 1e-6);

        camera.set_view_with_transition(View::Right, animated(200));
        camera.advance_transition(Duration::from_millis(40));
        let yaw = camera.yaw;
        camera.orbit_direction(1.0, 0.0, animated(200));
        camera.finish_transition();
        assert!(
            (camera.yaw - yaw - PI / 12.0).abs() < 1e-6,
            "A directional command interrupts an unrelated cardinal snap at its visible pose"
        );
    }

    #[test]
    fn all_six_named_views_reach_the_exact_axis_and_orthographic_projection() {
        let views = [
            (View::Front, Vec3::Z),
            (View::Right, Vec3::X),
            (View::Top, Vec3::Y),
            (View::Back, -Vec3::Z),
            (View::Left, -Vec3::X),
            (View::Bottom, -Vec3::Y),
        ];
        for (view, direction) in views {
            for start_orthographic in [false, true] {
                for transition in [Transition::Instant, animated(0), animated(240)] {
                    let mut camera = Camera {
                        orthographic: start_orthographic,
                        ..Camera::default()
                    };
                    camera.pan(47.0, -19.0, 600.0);
                    camera.zoom(0.7);
                    let target = camera.target;
                    let distance = camera.distance;
                    let original = camera.view_projection(0.7);
                    camera.set_view_with_transition(view, transition);
                    if camera.is_transitioning() {
                        assert_eq!(camera.view_projection(0.7), original);
                        assert!(camera.advance_transition(Duration::from_millis(120)));
                        assert!(camera.view_projection(0.7).is_finite());
                        assert!(!camera.advance_transition(Duration::from_millis(120)));
                    }
                    assert!(camera.is_orthographic());
                    assert!(!camera.is_transitioning());
                    assert_eq!(camera.target, target);
                    assert_eq!(camera.distance, distance);
                    assert!((camera.direction_in_view(direction) - Vec3::Z).length() < 1e-6);
                    assert!(((camera.eye() - target).normalize() - direction).length() < 1e-6);
                    if direction.y.abs() == 1.0 {
                        assert_eq!(camera.yaw, 0.0);
                        assert!((camera.direction_in_view(Vec3::X) - Vec3::X).length() < 1e-6);
                    }
                    let mut direct = camera.clone();
                    direct.look_from(direction);
                    assert!(
                        camera
                            .view_projection(0.7)
                            .abs_diff_eq(direct.view_projection(0.7), 1e-5)
                    );
                }
            }
        }
    }
}
