//! Translation snapping in canonical centimeters, independent of input devices,
//! document topology, presentation units, and edit history.
//!
//! Callers supply the unsnapped movement from a stable interaction baseline and
//! one anchor for the whole selection. Apply the resulting shared delta to the
//! selection; snapping every vertex or object independently would change its
//! shape or spacing. Exact numeric input remains capable of fractional lengths.
//!
//! Geometry snapping can later supply candidate targets to this resolution
//! boundary. Its priority relative to the grid, eligibility constraints, and
//! acquisition/release tolerances need an explicit policy before implementation;
//! the grid must not silently round a chosen geometry target away from geometry.

use glam::DVec3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GridReference {
    /// Snap the moved anchor to multiples of the step measured from world zero.
    #[default]
    WorldGrid,
    /// Preserve the anchor's original grid offset and snap only the movement.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Configurable policy; the current UI defaults to the world grid."
        )
    )]
    Relative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranslationSource {
    Interactive,
    /// Explicit numeric input bypasses quantization, not constraints/validation.
    Exact,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapSettings {
    pub enabled: bool,
    pub step_cm: f64,
    pub reference: GridReference,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            step_cm: 1.0,
            reference: GridReference::WorldGrid,
        }
    }
}

/// Chooses a concrete movement increment before an interaction starts. The
/// resulting `SnapSettings` stays fixed for that interaction: zoom, pointer
/// speed, and distance already dragged must not change its measuring scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StepPolicy {
    /// Use the largest clean 1/2/5 decimal increment that occupies no more than
    /// this many logical screen points at the manipulation depth.
    Adaptive { max_step_points: f64 },
    /// Use `SnapSettings::step_cm`, independent of viewport scale.
    Fixed,
}

impl Default for StepPolicy {
    fn default() -> Self {
        Self::Adaptive {
            max_step_points: 4.0,
        }
    }
}

impl StepPolicy {
    /// `cm_per_point` is supplied by the interaction, not inferred from the
    /// rendered grid or display unit. A viewport uses its manipulation depth;
    /// a property scrub may use its numeric sensitivity instead.
    ///
    /// Fixed and disabled snapping do not require a usable projection scale.
    /// The configured fixed step is still validated, just as it is for exact
    /// numeric input. Enabled adaptive snapping rejects an unrepresentable
    /// budget rather than introducing an arbitrary physical minimum increment.
    pub fn resolve(
        self,
        settings: SnapSettings,
        cm_per_point: f64,
    ) -> Result<SnapSettings, String> {
        validate_step(settings.step_cm)?;
        let Self::Adaptive { max_step_points } = self else {
            return Ok(settings);
        };
        if !settings.enabled {
            return Ok(settings);
        }
        if !max_step_points.is_finite() || max_step_points <= 0.0 {
            return Err(
                "The adaptive movement snap spacing must be finite and greater than zero.".into(),
            );
        }
        if !cm_per_point.is_finite() || cm_per_point <= 0.0 {
            return Err("The movement scale must be finite and greater than zero.".into());
        }
        let budget_cm = max_step_points * cm_per_point;
        if !budget_cm.is_finite() || budget_cm <= 0.0 {
            return Err(
                "The adaptive movement increment exceeds the supported numeric range.".into(),
            );
        }

        // Decimal parsing gives the canonical f64 for each ladder member.
        // Repeated multiplication by powers of ten could produce 0.02000...004
        // instead of 0.02. Inspect adjacent exponents because log10 can round
        // either side of an exact power-of-ten boundary.
        let exponent = budget_cm.log10().floor() as i32;
        let mut step_cm: f64 = 0.0;
        for power in (exponent - 1)..=(exponent + 1) {
            for coefficient in [1, 2, 5] {
                let candidate = format!("{coefficient}e{power}").parse::<f64>().unwrap();
                if candidate.is_finite() && candidate > step_cm && candidate <= budget_cm {
                    step_cm = candidate;
                }
            }
        }
        if step_cm == 0.0 {
            return Err(
                "The adaptive movement increment is below the supported numeric range.".into(),
            );
        }
        Ok(SnapSettings {
            step_cm,
            ..settings
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TranslationResolution {
    /// One world-space delta to apply to every member of the selection.
    pub delta: DVec3,
    /// The authoritative destination of the anchor. Prefer this when assigning
    /// an object's position directly: reconstructing it as anchor + delta can
    /// reintroduce floating-point cancellation after an exact grid result.
    pub target: DVec3,
}

impl SnapSettings {
    /// Resolve one total translation from the interaction baseline. `axes`
    /// identifies the world axes the operation is permitted to change.
    ///
    /// Inactive axes and exactly zero raw components retain the original
    /// coordinate. In particular, pressing a handle without moving it never
    /// pulls an off-grid selection onto the grid. This is not a per-frame delta
    /// accumulator: resolve the total raw displacement again on every preview.
    pub fn resolve_translation(
        self,
        anchor: DVec3,
        raw_delta: DVec3,
        axes: [bool; 3],
        source: TranslationSource,
    ) -> Result<TranslationResolution, String> {
        validate_step(self.step_cm)?;
        if !anchor.is_finite() || !raw_delta.is_finite() {
            return Err("Movement coordinates must be finite.".into());
        }

        let quantize = self.enabled && source == TranslationSource::Interactive;
        let mut target = anchor;
        let mut delta = DVec3::ZERO;
        for axis in 0..3 {
            if !axes[axis] || raw_delta[axis] == 0.0 {
                continue;
            }
            if quantize && self.reference == GridReference::WorldGrid {
                let raw_target = finite(anchor[axis] + raw_delta[axis])?;
                target[axis] = snap_component(raw_target, self.step_cm)?;
                delta[axis] = finite(target[axis] - anchor[axis])?;
            } else {
                delta[axis] = if quantize {
                    snap_component(raw_delta[axis], self.step_cm)?
                } else {
                    raw_delta[axis]
                };
                target[axis] = finite(anchor[axis] + delta[axis])?;
            }
        }
        Ok(TranslationResolution { delta, target })
    }
}

fn snap_component(value: f64, step: f64) -> Result<f64, String> {
    // Ties go away from zero on both sides of the origin. Check intermediate
    // results too: finite coordinates and step can still overflow division or
    // multiplication. Returning an error leaves the caller's preview unchanged.
    let rounded = finite(value / step)?.round();
    let mut snapped = finite(rounded * step)?;
    // The adaptive ladder is decimal. Construct its result in decimal too, so
    // three 0.1 cm increments produce the canonical representation of 0.3 cm,
    // not the neighboring f64 0.30000000000000004. This is actual coordinate
    // resolution, not lossy display formatting. Other custom fixed increments
    // keep their original multiplication behavior, and exact input bypasses
    // this function entirely.
    let decimal_step = format!("{step:e}");
    // At subnormal scales the decimal spelling can differ materially from the
    // stored increment (5e-324 is actually about 4.94e-324). Keep the same
    // representable increment used by division instead of magnifying that gap.
    if let Some((coefficient, exponent)) = decimal_step.split_once('e').filter(|_| step.is_normal())
    {
        let coefficient = match coefficient {
            "1" => Some(1.0),
            "2" => Some(2.0),
            "5" => Some(5.0),
            _ => None,
        };
        if let Some(coefficient) = coefficient {
            let integral = rounded * coefficient;
            if integral.is_finite() {
                snapped = finite(format!("{integral:.0}e{exponent}").parse::<f64>().unwrap())?;
            }
        }
    }
    Ok(if snapped == 0.0 { 0.0 } else { snapped })
}

fn validate_step(step_cm: f64) -> Result<(), String> {
    if !step_cm.is_finite() || step_cm <= 0.0 {
        return Err("The movement grid step must be finite and greater than zero.".into());
    }
    Ok(())
}

fn finite(value: f64) -> Result<f64, String> {
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| "Movement exceeds the supported numeric range.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(settings: SnapSettings, anchor: DVec3, movement: DVec3) -> TranslationResolution {
        settings
            .resolve_translation(anchor, movement, [true; 3], TranslationSource::Interactive)
            .unwrap()
    }

    fn adaptive_step(cm_per_point: f64) -> f64 {
        StepPolicy::default()
            .resolve(SnapSettings::default(), cm_per_point)
            .unwrap()
            .step_cm
    }

    #[test]
    fn adaptive_default_fits_clean_increments_to_four_screen_points() {
        assert_eq!(
            StepPolicy::default(),
            StepPolicy::Adaptive {
                max_step_points: 4.0
            }
        );
        // Suzanne at roughly 400 points wide is approximately 0.0067 cm per
        // point. Its movement step becomes 0.02 cm, rather than its old 1 cm.
        for (scale, expected) in [
            (0.0067, 0.02),
            (0.01, 0.02),
            (0.025, 0.1),
            (0.1, 0.2),
            (0.25, 1.0),
            (1.0, 2.0),
            (2.5, 10.0),
        ] {
            let step = adaptive_step(scale);
            assert_eq!(step, expected);
            assert!(step <= 4.0 * scale);
        }
    }

    #[test]
    fn adaptive_ladder_includes_boundaries_without_crossing_the_screen_budget() {
        let policy = StepPolicy::Adaptive {
            max_step_points: 1.0,
        };
        for boundary in [0.001_f64, 0.002, 0.005, 0.01, 0.1, 1.0, 2.0, 5.0, 10.0] {
            let below = f64::from_bits(boundary.to_bits() - 1);
            let above = f64::from_bits(boundary.to_bits() + 1);
            assert_eq!(
                policy
                    .resolve(SnapSettings::default(), boundary)
                    .unwrap()
                    .step_cm,
                boundary
            );
            assert_eq!(
                policy
                    .resolve(SnapSettings::default(), above)
                    .unwrap()
                    .step_cm,
                boundary
            );
            let previous = policy
                .resolve(SnapSettings::default(), below)
                .unwrap()
                .step_cm;
            assert!(previous < boundary);
            assert!(previous <= below);
        }
    }

    #[test]
    fn adaptive_resolution_scales_with_the_view_over_many_orders_of_magnitude() {
        for exponent in [-250, -100, -10, -1, 0, 1, 10, 100, 250] {
            let scale = format!("0.7e{exponent}").parse::<f64>().unwrap();
            let expected = format!("2e{exponent}").parse::<f64>().unwrap();
            let step = adaptive_step(scale);
            assert_eq!(step, expected);
            assert!(step / scale <= 4.0);
        }
    }

    #[test]
    fn adaptive_has_no_physical_minimum_and_handles_representable_extremes() {
        let policy = StepPolicy::Adaptive {
            max_step_points: 1.0,
        };
        let smallest = f64::from_bits(1);
        for (budget, expected) in [
            (smallest, smallest),
            (1e-300, 1e-300),
            (1e300, 1e300),
            (f64::MAX, 1e308),
        ] {
            let resolved = policy.resolve(SnapSettings::default(), budget).unwrap();
            assert_eq!(resolved.step_cm, expected);
            assert!(resolved.step_cm <= budget);
        }
    }

    #[test]
    fn adaptive_rejects_invalid_scale_tuning_and_unrepresentable_budgets() {
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(
                StepPolicy::default()
                    .resolve(SnapSettings::default(), invalid)
                    .is_err()
            );
            assert!(
                StepPolicy::Adaptive {
                    max_step_points: invalid
                }
                .resolve(SnapSettings::default(), 1.0)
                .is_err()
            );
        }
        assert!(
            StepPolicy::default()
                .resolve(SnapSettings::default(), f64::MAX)
                .is_err()
        );
        assert!(
            StepPolicy::Adaptive {
                max_step_points: 0.25
            }
            .resolve(SnapSettings::default(), f64::from_bits(1))
            .is_err()
        );
    }

    #[test]
    fn fixed_or_disabled_policy_needs_no_projection_and_preserves_settings() {
        let settings = SnapSettings {
            step_cm: 0.037,
            reference: GridReference::Relative,
            ..Default::default()
        };
        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                StepPolicy::Fixed.resolve(settings, scale).unwrap(),
                settings
            );
            let disabled = SnapSettings {
                enabled: false,
                ..settings
            };
            assert_eq!(
                StepPolicy::Adaptive {
                    max_step_points: f64::NAN
                }
                .resolve(disabled, scale)
                .unwrap(),
                disabled
            );
        }
        assert!(
            StepPolicy::Fixed
                .resolve(
                    SnapSettings {
                        step_cm: 0.0,
                        ..settings
                    },
                    1.0
                )
                .is_err()
        );
    }

    #[test]
    fn resolving_adaptive_settings_preserves_enablement_and_grid_reference() {
        let settings = SnapSettings {
            reference: GridReference::Relative,
            ..Default::default()
        };
        let resolved = StepPolicy::default().resolve(settings, 0.01).unwrap();
        assert!(resolved.enabled);
        assert_eq!(resolved.reference, GridReference::Relative);
        assert_eq!(resolved.step_cm, 0.02);
        // A concrete captured setting no longer depends on any projection.
        assert_eq!(
            resolve(resolved, DVec3::X * 0.125, DVec3::X * 0.13).delta.x,
            0.14
        );
    }

    #[test]
    fn decimal_ladder_results_use_clean_coordinates_in_both_directions() {
        for (step_cm, raw, expected) in [
            (0.1, 0.31, 0.3),
            (0.01, 0.291, 0.29),
            (0.02, 0.31, 0.32),
            (0.05, 0.31, 0.3),
            (1e-200, 3.1e-200, 3e-200),
            (1e200, 3.1e200, 3e200),
        ] {
            for sign in [-1.0, 1.0] {
                let result = resolve(
                    SnapSettings {
                        step_cm,
                        ..Default::default()
                    },
                    DVec3::ZERO,
                    DVec3::X * raw * sign,
                );
                assert_eq!(result.target.x, expected * sign);
            }
        }
    }

    #[test]
    fn decimal_cleanup_does_not_reinterpret_arbitrary_fixed_steps_or_exact_input() {
        for step_cm in [0.037, 0.3333333333333333, std::f64::consts::PI] {
            let settings = StepPolicy::Fixed
                .resolve(
                    SnapSettings {
                        step_cm,
                        ..Default::default()
                    },
                    0.0,
                )
                .unwrap();
            let raw = step_cm * 3.1;
            assert_eq!(
                resolve(settings, DVec3::ZERO, DVec3::X * raw).target.x,
                (raw / step_cm).round() * step_cm
            );
        }
        let exact = 0.30000000000000004;
        let result = SnapSettings {
            step_cm: 0.1,
            ..Default::default()
        }
        .resolve_translation(
            DVec3::ZERO,
            DVec3::new(exact, 7.0, 0.0),
            [true, false, true],
            TranslationSource::Exact,
        )
        .unwrap();
        assert_eq!(result.target.x, exact);
        assert_eq!(result.target.y, 0.0);
    }

    #[test]
    fn subnormal_snapping_keeps_the_actual_representable_increment() {
        let step_cm = f64::from_bits(1);
        let movement = f64::from_bits(2_024);
        let result = resolve(
            SnapSettings {
                step_cm,
                ..Default::default()
            },
            DVec3::ZERO,
            DVec3::X * movement,
        );
        assert_eq!(result.target.x, movement);
    }

    #[test]
    fn defaults_snap_destinations_to_whole_centimeters() {
        let settings = SnapSettings::default();
        assert!(settings.enabled);
        assert_eq!(settings.step_cm, 1.0);
        assert_eq!(settings.reference, GridReference::WorldGrid);
        let result = resolve(
            settings,
            DVec3::new(0.17, -0.31, 4.23),
            DVec3::new(0.7, -1.3, 0.45),
        );
        assert_eq!(result.target, DVec3::new(1.0, -2.0, 5.0));
        assert_eq!(result.delta, result.target - DVec3::new(0.17, -0.31, 4.23));
    }

    #[test]
    fn relative_grid_preserves_original_offsets() {
        let anchor = DVec3::new(0.17, -0.31, 4.23);
        let result = resolve(
            SnapSettings {
                reference: GridReference::Relative,
                ..Default::default()
            },
            anchor,
            DVec3::new(0.7, -1.3, 0.45),
        );
        assert_eq!(result.delta, DVec3::new(1.0, -1.0, 0.0));
        assert_eq!(result.target, anchor + result.delta);
    }

    #[test]
    fn inactive_and_untouched_components_do_not_jump_to_grid() {
        let anchor = DVec3::new(0.17, -0.31, 4.23);
        let settings = SnapSettings::default();
        let idle = resolve(settings, anchor, DVec3::ZERO);
        assert_eq!(idle.target, anchor);
        assert_eq!(idle.delta, DVec3::ZERO);
        let locked = settings
            .resolve_translation(
                anchor,
                DVec3::new(0.7, 9.9, 0.0),
                [true, false, true],
                TranslationSource::Interactive,
            )
            .unwrap();
        assert_eq!(locked.target, DVec3::new(1.0, -0.31, 4.23));
        assert_eq!(locked.delta, DVec3::new(1.0 - 0.17, 0.0, 0.0));
        let blocked = settings
            .resolve_translation(
                anchor,
                DVec3::ONE,
                [false; 3],
                TranslationSource::Interactive,
            )
            .unwrap();
        assert_eq!(blocked, idle);
    }

    #[test]
    fn rounding_is_symmetric_and_normalizes_snapped_negative_zero() {
        for (raw, expected) in [
            (0.49, 0.0),
            (0.5, 1.0),
            (1.5, 2.0),
            (-0.49, 0.0),
            (-0.5, -1.0),
            (-1.5, -2.0),
        ] {
            let result = resolve(SnapSettings::default(), DVec3::ZERO, DVec3::X * raw);
            assert_eq!(result.target.x, expected);
            assert_eq!(result.delta.x, expected);
            if expected == 0.0 {
                assert!(!result.target.x.is_sign_negative());
            }
        }
    }

    #[test]
    fn disabled_and_exact_movements_preserve_fractional_intent_and_constraints() {
        for (enabled, source) in [
            (false, TranslationSource::Interactive),
            (true, TranslationSource::Exact),
            (false, TranslationSource::Exact),
        ] {
            let anchor = DVec3::new(0.17, 0.21, 0.39);
            let movement = DVec3::new(0.013, 0.023, 0.037);
            let result = SnapSettings {
                enabled,
                ..Default::default()
            }
            .resolve_translation(anchor, movement, [true, false, true], source)
            .unwrap();
            assert_eq!(result.delta, DVec3::new(0.013, 0.0, 0.037));
            assert_eq!(result.target, anchor + result.delta);
        }
    }

    #[test]
    fn fractional_grid_steps_are_lengths_in_centimeters() {
        for (step, movement, expected) in [(0.1, 0.16, 0.2), (0.25, 0.7, 0.75), (10.0, 16.0, 20.0)]
        {
            let result = resolve(
                SnapSettings {
                    step_cm: step,
                    ..Default::default()
                },
                DVec3::ZERO,
                DVec3::X * movement,
            );
            assert_eq!(result.target.x, expected);
        }
    }

    #[test]
    fn preview_resolution_uses_baseline_and_does_not_accumulate_roundoff() {
        let settings = SnapSettings::default();
        let anchor = DVec3::new(0.1, 0.2, 0.3);
        let before = resolve(settings, anchor, DVec3::X * 0.45);
        let beyond = resolve(settings, anchor, DVec3::X * 2.6);
        let back = resolve(settings, anchor, DVec3::X * 0.45);
        assert_eq!(before.target.x, 1.0);
        assert_eq!(beyond.target.x, 3.0);
        assert_eq!(before, back);
        assert_eq!(resolve(settings, anchor, DVec3::ZERO).target, anchor);
    }

    #[test]
    fn shared_delta_preserves_selection_spacing_instead_of_rounding_each_member() {
        let anchor = DVec3::new(0.125, 0.25, 0.375);
        let other = anchor + DVec3::new(0.25, 0.5, 0.75);
        let result = resolve(SnapSettings::default(), anchor, DVec3::new(0.8, 0.8, 0.8));
        assert_eq!(result.target, DVec3::ONE);
        assert_eq!((other + result.delta) - result.target, other - anchor);
        assert_ne!(other + result.delta, (other + result.delta).round());
    }

    #[test]
    fn invalid_settings_and_nonfinite_inputs_are_rejected_even_when_bypassed() {
        for source in [TranslationSource::Interactive, TranslationSource::Exact] {
            for enabled in [true, false] {
                for step_cm in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                    let settings = SnapSettings {
                        enabled,
                        step_cm,
                        ..Default::default()
                    };
                    assert!(
                        settings
                            .resolve_translation(DVec3::ZERO, DVec3::ZERO, [true; 3], source)
                            .is_err()
                    );
                }
                for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                    let settings = SnapSettings {
                        enabled,
                        ..Default::default()
                    };
                    let bad = DVec3::new(0.0, invalid, 0.0);
                    assert!(
                        settings
                            .resolve_translation(bad, DVec3::ZERO, [false; 3], source)
                            .is_err()
                    );
                    assert!(
                        settings
                            .resolve_translation(DVec3::ZERO, bad, [false; 3], source)
                            .is_err()
                    );
                }
            }
        }
    }

    #[test]
    fn finite_inputs_cannot_overflow_the_resolution() {
        for (settings, anchor, movement) in [
            (
                SnapSettings::default(),
                DVec3::X * f64::MAX,
                DVec3::X * f64::MAX,
            ),
            (
                SnapSettings {
                    step_cm: f64::MIN_POSITIVE,
                    ..Default::default()
                },
                DVec3::ZERO,
                DVec3::X * f64::MAX,
            ),
            (
                SnapSettings {
                    step_cm: f64::MAX * 0.6,
                    ..Default::default()
                },
                DVec3::ZERO,
                DVec3::X * f64::MAX,
            ),
        ] {
            assert!(
                settings
                    .resolve_translation(
                        anchor,
                        movement,
                        [true; 3],
                        TranslationSource::Interactive
                    )
                    .is_err()
            );
        }
        for source in [TranslationSource::Interactive, TranslationSource::Exact] {
            assert!(
                SnapSettings {
                    enabled: false,
                    ..Default::default()
                }
                .resolve_translation(DVec3::X * f64::MAX, DVec3::X * f64::MAX, [true; 3], source)
                .is_err()
            );
        }
    }
}
