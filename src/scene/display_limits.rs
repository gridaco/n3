//! Numeric contract for derived f32 presentation and lighting coordinates.
//! Canonical scene/document coordinates remain f64 centimeters. These limits do
//! not constrain serialization: they reject an unusable evaluated display before
//! either an edit candidate or a GPU frame becomes visible.
use glam::{DMat4, DVec3, Vec3};

use super::EvaluatedScene;

/// Leaves room for squared distances and normal/lighting arithmetic in f32.
pub(crate) const MAX_SHADER_MAGNITUDE: f32 = 1.0e12;

pub(crate) fn finite_values(values: &[f32]) -> bool {
    values
        .iter()
        .all(|value| value.is_finite() && value.abs() <= MAX_SHADER_MAGNITUDE)
}

pub(crate) fn meters_per_display_unit(scale: f64) -> f32 {
    (0.01 / scale) as f32
}

pub(crate) fn point(center_cm: DVec3, scale: f64, world_cm: DVec3) -> Vec3 {
    ((world_cm - center_cm) * scale).as_vec3()
}

pub(crate) fn validate_frame(center_cm: DVec3, scale: f64) -> Result<(), String> {
    let units = meters_per_display_unit(scale);
    if !center_cm.is_finite()
        || !scale.is_finite()
        || scale <= 0.0
        || !units.is_finite()
        || units <= 0.0
        || units > MAX_SHADER_MAGNITUDE
    {
        return Err("Scene framing exceeds the renderer's coordinate range.".into());
    }
    Ok(())
}

/// Validate exactly the conversions performed by rendering: normalize in f64,
/// convert to f32, then multiply by the f32 meter scale. A single f64 world-to-
/// meter calculation would miss overflow introduced by the actual GPU boundary.
pub(crate) fn validate_display(
    center_cm: DVec3,
    scale: f64,
    placement: DMat4,
    frame: &EvaluatedScene,
) -> Result<(), String> {
    validate_frame(center_cm, scale)?;
    if !placement.is_finite() {
        return Err("Asset placement must be finite.".into());
    }
    let units = meters_per_display_unit(scale);
    let valid = |world_cm| {
        let position = point(center_cm, scale, placement.transform_point3(world_cm));
        finite_values(&position.to_array()) && finite_values(&(position * units).to_array())
    };
    for draw in &frame.draws {
        for vertex in &draw.vertices {
            if !valid(DVec3::from_array(vertex.position)) {
                return Err("Animated geometry exceeds the renderer's numeric range.".into());
            }
        }
    }
    for light in &frame.lights {
        if !valid(light.position_cm) {
            return Err("A punctual light exceeds the renderer's numeric range.".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{EvaluatedLight, SceneAsset, test_support};

    #[test]
    fn rebasing_preserves_large_origins_and_checks_actual_f32_scale() {
        let frame = SceneAsset::new(test_support::triangle_data())
            .unwrap()
            .evaluate(0, None)
            .unwrap();
        // The bound concerns display and rebased lighting distances, not the
        // absolute canonical origin, which can be much larger.
        let origin = DVec3::splat(1.0e16);
        validate_display(origin, 1.0, DMat4::from_translation(origin), &frame).unwrap();
        assert!(validate_frame(DVec3::ZERO, 1.0e-16).is_err());
        assert!(validate_frame(DVec3::ZERO, 1.0e308).is_err());
        assert!(validate_frame(DVec3::ZERO, 0.0).is_err());
    }

    #[test]
    fn display_and_rebased_meter_limits_are_independent_and_cover_lights() {
        let frame = SceneAsset::new(test_support::triangle_data())
            .unwrap()
            .evaluate(0, None)
            .unwrap();
        // Small units keep meters admissible while display coordinates overflow.
        let placement = DMat4::from_translation(DVec3::X * 2.0e12);
        assert!(validate_display(DVec3::ZERO, 1.0, placement, &frame).is_err());
        // Large units keep display coordinates admissible while meters overflow.
        let placement = DMat4::from_translation(DVec3::X * 2.0e14);
        assert!(validate_display(DVec3::ZERO, 1.0e-10, placement, &frame).is_err());
        let mut lights = EvaluatedScene::default();
        lights.lights.push(EvaluatedLight {
            node: 0,
            light: 0,
            position_cm: DVec3::ZERO,
            direction: DVec3::NEG_Z,
        });
        assert_eq!(
            validate_display(DVec3::ZERO, 1.0, placement, &lights).unwrap_err(),
            "A punctual light exceeds the renderer's numeric range."
        );
    }
}
