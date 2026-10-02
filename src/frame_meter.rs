//! Passive application frame cadence, independent of UI passes and host clocks.
//!
//! Hosts report one timestamp after handing a rendered surface to `present`.
//! This is not a GPU completion or display timestamp. No timer, redraw request,
//! allocation, or GPU synchronization is needed to collect a sample.
use std::time::Duration;

const SAMPLE_PERIOD: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    intervals: u64,
    elapsed: Duration,
}

impl Sample {
    pub(crate) fn fps(self) -> f64 {
        self.intervals as f64 / self.elapsed.as_secs_f64()
    }

    pub(crate) fn mean_interval_ms(self) -> f64 {
        self.elapsed.as_secs_f64() * 1000.0 / self.intervals as f64
    }

    pub(crate) fn elapsed(self) -> Duration {
        self.elapsed
    }
}

#[derive(Default)]
pub(crate) struct FrameMeter {
    enabled: bool,
    first: Option<Duration>,
    last: Option<Duration>,
    intervals: u64,
    sample: Option<Sample>,
}

impl FrameMeter {
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }

    pub(crate) fn toggle(&mut self) {
        *self = Self {
            enabled: !self.enabled,
            ..Self::default()
        };
    }

    /// The last completed sample stays visible while the app is idle. Publishing
    /// never schedules a frame; the next ordinary redraw displays the result.
    pub(crate) fn sample(&self) -> Option<Sample> {
        self.sample
    }

    /// `now` is monotonic elapsed host time, taken only when enabled and after a
    /// successful submission/presentation handoff. Failed acquisition attempts
    /// don't call this, but their delay remains in the next successful interval.
    pub(crate) fn record_submission(&mut self, now: Duration) {
        if !self.enabled {
            return;
        }
        if self.last.is_some_and(|last| now < last) {
            // A broken/reset clock cannot yield a meaningful rate across epochs.
            // Equal timestamps are valid on a host with coarse clock precision.
            self.first = None;
            self.intervals = 0;
            self.sample = None;
        }
        self.last = Some(now);
        let Some(first) = self.first else {
            self.first = Some(now);
            return;
        };
        self.intervals += 1;
        let elapsed = now - first;
        if elapsed >= SAMPLE_PERIOD {
            self.sample = Some(Sample {
                intervals: self.intervals,
                elapsed,
            });
            // Share the boundary timestamp, not its interval, with the next
            // window. Long gaps count in full; filtering them could hide stalls.
            self.first = Some(now);
            self.intervals = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(meter: &mut FrameMeter, milliseconds: u64) {
        meter.record_submission(Duration::from_millis(milliseconds));
    }

    fn assert_rate(sample: Sample, fps: f64, mean_ms: f64) {
        assert!((sample.fps() - fps).abs() < 1e-9);
        assert!((sample.mean_interval_ms() - mean_ms).abs() < 1e-9);
    }

    #[test]
    fn disabled_and_reenabled_meter_never_reuses_a_sample() {
        let mut meter = FrameMeter::default();
        at(&mut meter, 0);
        at(&mut meter, 1000);
        assert_eq!(meter.first, None);
        assert_eq!(meter.sample(), None);
        meter.toggle();
        at(&mut meter, 1000);
        assert_eq!(meter.sample(), None);
        at(&mut meter, 1500);
        assert_rate(meter.sample().unwrap(), 2.0, 500.0);
        meter.toggle();
        assert!(!meter.enabled());
        assert_eq!(meter.sample(), None);
        meter.toggle();
        at(&mut meter, 9000);
        assert_eq!(meter.sample(), None);
        at(&mut meter, 9500);
        assert_rate(meter.sample().unwrap(), 2.0, 500.0);
    }

    #[test]
    fn counts_intervals_not_endpoints_at_60_and_120_fps() {
        for fps in [60, 120] {
            let mut meter = FrameMeter::default();
            meter.toggle();
            for i in 0..=fps / 2 {
                meter.record_submission(Duration::from_secs_f64(i as f64 / fps as f64));
            }
            let first = meter.sample().unwrap();
            assert_eq!(first.elapsed(), Duration::from_millis(500));
            assert_rate(first, fps as f64, 1000.0 / fps as f64);
            for i in fps / 2 + 1..=fps {
                meter.record_submission(Duration::from_secs_f64(i as f64 / fps as f64));
            }
            assert_eq!(meter.sample(), Some(first));
        }
    }

    #[test]
    fn uneven_frames_use_total_elapsed_not_average_instantaneous_fps() {
        let mut meter = FrameMeter::default();
        meter.toggle();
        for t in [0, 10, 20, 600] {
            at(&mut meter, t);
        }
        assert_rate(meter.sample().unwrap(), 5.0, 200.0);
        assert_eq!(
            meter.sample().unwrap().elapsed(),
            Duration::from_millis(600)
        );
    }

    #[test]
    fn idle_stalls_and_failed_attempts_remain_in_elapsed_time() {
        let mut meter = FrameMeter::default();
        meter.toggle();
        for t in [0, 100, 200, 300, 400, 500] {
            at(&mut meter, t);
        }
        let previous = meter.sample();
        at(&mut meter, 600);
        // No handoff callbacks during idle, occlusion, or failed acquisition.
        assert_eq!(meter.sample(), previous);
        at(&mut meter, 10500);
        assert_rate(meter.sample().unwrap(), 0.2, 5000.0);
        assert_eq!(meter.sample().unwrap().elapsed(), Duration::from_secs(10));
    }

    #[test]
    fn repeated_clock_values_count_frames_without_dividing_by_zero() {
        let mut meter = FrameMeter::default();
        meter.toggle();
        for _ in 0..4 {
            at(&mut meter, 0);
        }
        assert_eq!(meter.sample(), None);
        at(&mut meter, 500);
        assert_rate(meter.sample().unwrap(), 8.0, 125.0);
    }

    #[test]
    fn clock_regression_starts_a_new_epoch() {
        let mut meter = FrameMeter::default();
        meter.toggle();
        at(&mut meter, 1000);
        at(&mut meter, 1500);
        at(&mut meter, 0);
        assert_eq!(meter.sample(), None);
        at(&mut meter, 500);
        assert_rate(meter.sample().unwrap(), 2.0, 500.0);
    }
}
