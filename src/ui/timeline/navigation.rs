//! Finite continuous seconds; viewport navigation never edits caller timestamps.
use super::data::TimeRange;

pub(super) fn time_at(range: TimeRange, left: f32, width: f32, x: f32) -> f64 {
    range.start + (f64::from(x) - f64::from(left)) / f64::from(width.max(1.0)) * range.duration()
}
pub(super) fn x_at(range: TimeRange, left: f32, width: f32, time: f64) -> f32 {
    left + ((time - range.start) / range.duration() * f64::from(width)) as f32
}
pub(super) fn zoom(range: TimeRange, anchor: f64, factor: f64) -> Option<TimeRange> {
    if !factor.is_finite() || factor <= 0.0 {
        return None;
    }
    let start = anchor + (range.start - anchor) / factor;
    let end = anchor + (range.end - anchor) / factor;
    TimeRange::new(start, end)
        .ok()
        .filter(|r| r.duration() > 0.0)
}
pub(super) fn pan(range: TimeRange, seconds: f64) -> Option<TimeRange> {
    TimeRange::new(range.start + seconds, range.end + seconds)
        .ok()
        .filter(|r| r.duration() > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fractional_negative_mapping_and_pointer_anchor_are_continuous() {
        let range = TimeRange::new(-3.75, 8.125).unwrap();
        let x = 173.25;
        let time = time_at(range, 20.0, 500.0, x);
        assert!((x_at(range, 20.0, 500.0, time) - x).abs() < 0.0001);
        let zoomed = zoom(range, time, 2.5).unwrap();
        assert!((x_at(zoomed, 20.0, 500.0, time) - x).abs() < 0.0001);
        assert_eq!(pan(range, -0.125).unwrap().start, -3.875);
        assert!(zoom(range, time, f64::NAN).is_none());
        assert!(pan(range, f64::INFINITY).is_none());
        assert_eq!(
            time_at(TimeRange::new(0.0, 1e9).unwrap(), 0.0, 1000.0, 270.0),
            270000000.0
        );
    }
}
