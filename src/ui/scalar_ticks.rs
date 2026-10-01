//! Labeled graduations for a projected linear scalar axis. Projection, units,
//! domain limits and painting belong to the caller.

pub(crate) const MAX_TICKS: usize = 4096;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ScalarTick {
    /// Absolute logical screen coordinate, suitable for an egui axis.
    pub position: f32,
    pub value: f64,
    pub label: String,
}

/// Plan increasing labeled ticks within absolute logical screen bounds.
/// `origin` is the projected coordinate of scalar zero. `pixels_per_unit` and
/// `min_spacing` must be positive and finite. The label callback receives the
/// candidate step as well as the value, allowing unit-appropriate precision.
///
/// Label spacing retains the ruler's inexpensive character-width estimate;
/// typography and clipping remain presentation concerns. Invalid axes produce
/// no ticks, and every axis is bounded to `MAX_TICKS` graduations.
pub(crate) fn plan(
    start: f64,
    end: f64,
    origin: f64,
    pixels_per_unit: f64,
    min_spacing: f64,
    label: impl Fn(f64, f64) -> String,
) -> Vec<ScalarTick> {
    if !start.is_finite()
        || !end.is_finite()
        || end <= start
        || !(end - start).is_finite()
        || !origin.is_finite()
        || !pixels_per_unit.is_finite()
        || pixels_per_unit <= 0.0
        || !min_spacing.is_finite()
        || min_spacing <= 0.0
    {
        return Vec::new();
    }
    let minimum = (start - origin) / pixels_per_unit;
    let maximum = (end - origin) / pixels_per_unit;
    if !minimum.is_finite() || !maximum.is_finite() || maximum <= minimum {
        return Vec::new();
    }
    let minimum_spacing = min_spacing.max((end - start) / (MAX_TICKS - 2) as f64);
    let mut major = nice_step(minimum_spacing / pixels_per_unit);
    for _ in 0..4 {
        if !major.is_finite() || major <= 0.0 {
            return Vec::new();
        }
        let characters = label(minimum, major).len().max(label(maximum, major).len());
        let spacing = minimum_spacing.max(characters as f64 * 6.0 + 18.0);
        if major * pixels_per_unit >= spacing {
            break;
        }
        major = nice_step(spacing / pixels_per_unit);
    }
    let spacing = major * pixels_per_unit;
    let first = (minimum / major).ceil();
    if !spacing.is_finite()
        || spacing <= 0.0
        || !first.is_finite()
        || first.abs() >= (1_u64 << 52) as f64
    {
        return Vec::new();
    }
    let count = (((end - start) / spacing).ceil() as usize)
        .saturating_add(2)
        .min(MAX_TICKS);
    let mut ticks = Vec::with_capacity(count);
    for offset in 0..count {
        let index = first + offset as f64;
        let value = if index == 0.0 { 0.0 } else { index * major };
        // Subtract visible values before multiplying, retaining useful precision
        // when scalar zero is far outside the visible axis.
        let position = start + (value - minimum) * pixels_per_unit;
        if !position.is_finite()
            || !(position as f32).is_finite()
            || !value.is_finite()
            || position < start
            || position > end
        {
            continue;
        }
        ticks.push(ScalarTick {
            position: position as f32,
            value,
            label: label(value, major),
        });
    }
    ticks
}

/// Stable decimal steps shared by graduation planning and endpoint labels.
pub(crate) fn nice_step(minimum: f64) -> f64 {
    let power = 10.0_f64.powf(minimum.log10().floor());
    let fraction = minimum / power;
    power
        * if fraction <= 1.0 {
            1.0
        } else if fraction <= 1.5 {
            1.5
        } else if fraction <= 2.0 {
            2.0
        } else if fraction <= 5.0 {
            5.0
        } else {
            10.0
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(value: f64, _: f64) -> String {
        value.to_string()
    }

    #[test]
    fn signed_axis_keeps_zero_and_projects_increasing_values() {
        let ticks = plan(100.0, 700.0, 400.0, 100.0, 72.0, label);
        assert_eq!(
            ticks.iter().map(|tick| tick.value).collect::<Vec<_>>(),
            [-3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0]
        );
        let zero = &ticks[3];
        assert_eq!(zero.position, 400.0);
        assert_eq!(zero.label, "0");
        assert!(zero.value.is_sign_positive(), "Avoid a negative-zero label");
        assert!(
            ticks
                .windows(2)
                .all(|pair| pair[0].position < pair[1].position)
        );
        assert!(ticks.iter().all(|tick| {
            (f64::from(tick.position) - (400.0 + tick.value * 100.0)).abs() < 1e-5
        }));
    }

    #[test]
    fn fractional_steps_are_available_to_the_callers_unit_formatter() {
        let ticks = plan(0.0, 500.0, 250.0, 500.0, 72.0, |value, step| {
            assert!((step - 0.15).abs() < 1e-12);
            format!("{value:.2}")
        });
        assert_eq!(ticks.len(), 7);
        assert!((ticks[1].value + 0.3).abs() < 1e-12);
        assert_eq!(ticks[1].label, "-0.30");
        assert_eq!(ticks[3].position, 250.0);
        assert_eq!(ticks[3].label, "0.00");
    }

    #[test]
    fn density_and_label_length_limit_visible_graduations() {
        let dense = plan(0.0, 800.0, 0.0, 100.0, 40.0, label);
        let sparse = plan(0.0, 800.0, 0.0, 100.0, 120.0, label);
        assert!(dense.len() > sparse.len());
        let long = plan(0.0, 800.0, 0.0, 100.0, 40.0, |value, _| {
            format!("long scalar label: {value}")
        });
        assert!(long.len() < dense.len());
        for (ticks, minimum) in [(&dense, 40.0), (&sparse, 120.0)] {
            assert!(
                ticks
                    .windows(2)
                    .all(|pair| { f64::from(pair[1].position - pair[0].position) >= minimum })
            );
        }
        let wide = plan(0.0, 1.0e8, 0.0, 1.0, 1.0, label);
        assert!(!wide.is_empty() && wide.len() <= MAX_TICKS);
    }

    #[test]
    fn invalid_and_extreme_axes_never_emit_nonfinite_positions() {
        for axis in [
            [f64::NAN, 800.0, 0.0, 1.0, 72.0],
            [0.0, f64::INFINITY, 0.0, 1.0, 72.0],
            [800.0, 0.0, 0.0, 1.0, 72.0],
            [0.0, 0.0, 0.0, 1.0, 72.0],
            [0.0, 800.0, f64::NAN, 1.0, 72.0],
            [0.0, 800.0, 0.0, 0.0, 72.0],
            [0.0, 800.0, 0.0, -1.0, 72.0],
            [0.0, 800.0, 0.0, f64::INFINITY, 72.0],
            [0.0, 800.0, 0.0, 1.0, f64::NAN],
            [0.0, 800.0, 0.0, 1.0, 0.0],
            [0.0, 800.0, 0.0, 1.0, -1.0],
            [-f64::MAX, f64::MAX, 0.0, 1.0, 72.0],
        ] {
            let [start, end, origin, scale, spacing] = axis;
            assert!(plan(start, end, origin, scale, spacing, label).is_empty());
        }
        for scale in [1.0e-300, 1.0e-100, 1.0, 1.0e100, 1.0e300] {
            for origin in [-1.0e15, 0.0, 1.0e15] {
                let ticks = plan(0.0, 800.0, origin, scale, 72.0, label);
                assert!(ticks.len() <= MAX_TICKS);
                assert!(ticks.iter().all(|tick| {
                    tick.position.is_finite()
                        && tick.value.is_finite()
                        && (0.0..=800.0).contains(&tick.position)
                }));
            }
        }
        assert!(plan(1.0e50, 2.0e50, 0.0, 1.0e48, 72.0, label).is_empty());
    }
}
