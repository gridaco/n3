//! CPU observations from the real workbench replay, without GPU capture work.
use super::{Case, replay::Replay};
use crate::{settings::ResolvedTheme, ui::timeline::Metrics};
use egui::{Event, Modifiers, Pos2};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const STEP: Duration = Duration::from_millis(16);
const SHARED_WARMUP: usize = 8;
const NAVIGATION_WARMUP: usize = 12;
const SAMPLES: usize = 120;

/// These are observations of one native run, not performance acceptance gates.
/// The elapsed wall-clock timer surrounds Replay::frame, including Context::run_ui,
/// fixture host, inspector, tracing, and virtual input presentation. It excludes
/// caller fixture construction, output disposal, tessellation, and GPU work.
pub(super) fn measure() -> Result<Value, String> {
    let mut replay = Replay::new(Case::TimelineEmpty, ResolvedTheme::Light);
    for _ in 0..SHARED_WARMUP {
        discard_frame(&mut replay, Vec::new())?;
    }
    // Prepare the caller snapshot before the first component-layout timing.
    // Snapshot validation/indexing itself happens inside this first frame.
    replay
        .bench
        .select_case(&replay.ctx.clone(), Case::TimelineDense);
    let preparation_ms = timed_frame(&mut replay, Vec::new())?;
    let first_metrics = metrics(&replay)?;
    for index in 0..NAVIGATION_WARMUP {
        let events = navigation_events(&replay, index)?;
        discard_frame(&mut replay, events)?;
    }
    let mut cpu_ms = Vec::with_capacity(SAMPLES);
    let mut visible = Vec::with_capacity(SAMPLES);
    for index in 0..SAMPLES {
        let events = navigation_events(&replay, NAVIGATION_WARMUP + index)?;
        cpu_ms.push(timed_frame(&mut replay, events)?);
        visible.push(metrics(&replay)?);
    }
    cpu_ms.sort_by(f64::total_cmp);
    let median = (cpu_ms[SAMPLES / 2 - 1] + cpu_ms[SAMPLES / 2]) * 0.5;
    // The nearest-rank percentile: ceil(0.95 * 120) is sample 114.
    let p95 = cpu_ms[(SAMPLES * 95).div_ceil(100) - 1];
    let maximum = cpu_ms[SAMPLES - 1];
    Ok(json!({
        "case": Case::TimelineDense.id(),
        "appearance": "light",
        "build_profile": if cfg!(debug_assertions) { "development" } else { "release" },
        "platform": { "os": std::env::consts::OS, "architecture": std::env::consts::ARCH },
        "timer_scope": "Elapsed wall time around CPU-only Replay::frame: production egui layout, fixture host, inspector, tracing and virtual input",
        "excluded": ["fixture construction", "output disposal", "tessellation", "GPU rendering", "GPU readback", "image encoding"],
        "dataset": { "tracks": first_metrics.total_tracks, "keys": first_metrics.total_keys },
        "shared_warmup_frames": SHARED_WARMUP,
        "first_preparation_frame": {
            "cpu_ms": preparation_ms,
            "visible_rows": first_metrics.rows,
            "keys_considered": first_metrics.keys_considered,
            "markers": first_metrics.markers,
        },
        "navigation": {
            "warmup_frames": NAVIGATION_WARMUP,
            "sample_frames": SAMPLES,
            "inputs": ["pointer movement", "vertical wheel", "Shift+wheel pan", "modifier+wheel zoom", "pinch zoom"],
            "cpu_ms": { "median": median, "p95": p95, "maximum": maximum },
            "visible_rows": counts(&visible, |metrics| metrics.rows),
            "keys_considered": counts(&visible, |metrics| metrics.keys_considered),
            "markers": counts(&visible, |metrics| metrics.markers),
            "snapshot_preparation_frames": visible.iter().filter(|metrics| metrics.prepared).count(),
        },
    }))
}

fn timed_frame(replay: &mut Replay, events: Vec<Event>) -> Result<f64, String> {
    let start = Instant::now();
    let mut output = replay.frame(events, STEP)?;
    let elapsed = start.elapsed().as_secs_f64() * 1_000.0;
    output.textures_delta.clear();
    Ok(elapsed)
}

fn discard_frame(replay: &mut Replay, events: Vec<Event>) -> Result<(), String> {
    replay.frame(events, STEP)?.textures_delta.clear();
    Ok(())
}

fn metrics(replay: &Replay) -> Result<Metrics, String> {
    replay
        .bench
        .timeline
        .as_ref()
        .map(|fixture| fixture.metrics[0])
        .ok_or_else(|| "Dense timeline measurement has no fixture host.".into())
}

fn counts(metrics: &[Metrics], value: impl Fn(&Metrics) -> usize) -> Value {
    let mut values = metrics.iter().map(value);
    let Some(first) = values.next() else {
        return json!({"minimum": 0, "maximum": 0, "last": 0});
    };
    let (minimum, maximum, last) = values.fold((first, first, first), |(min, max, _), value| {
        (min.min(value), max.max(value), value)
    });
    json!({"minimum": minimum, "maximum": maximum, "last": last})
}

fn navigation_events(replay: &Replay, index: usize) -> Result<Vec<Event>, String> {
    let canvas = replay.target(0, "timeline-canvas")?.rect;
    let ruler = replay.target(0, "timeline-ruler")?.rect;
    if !canvas.is_positive() || !ruler.is_positive() {
        return Err("Dense timeline measurement needs visible canvas and ruler bounds.".into());
    }
    let fraction = 0.2 + (index % 10) as f32 * 0.06;
    let pointer = Pos2::new(ruler.left() + ruler.width() * fraction, canvas.center().y);
    let wheel = |delta, modifiers| super::replay::wheel(egui::vec2(0.0, delta), modifiers);
    let navigation = match index % 8 {
        0 => wheel(-48.0, Modifiers::NONE),
        1 => wheel(-12.0, Modifiers::SHIFT),
        2 => wheel(12.0, Modifiers::CTRL),
        3 => Event::Zoom(1.05),
        4 => wheel(24.0, Modifiers::NONE),
        5 => wheel(12.0, Modifiers::SHIFT),
        6 => wheel(-12.0, Modifiers::CTRL),
        _ => Event::Zoom(1.0 / 1.05),
    };
    Ok(vec![Event::PointerMoved(pointer), navigation])
}

#[cfg(test)]
mod tests {
    #[test]
    fn dense_navigation_measurement_runs_without_gpu_or_capture() {
        // Validate the measurement path itself. CPU timings are observations;
        // there is deliberately no machine-dependent acceptance threshold.
        let measured = super::measure().expect("Actual dense timeline replay measurement");
        println!("{}", serde_json::to_string_pretty(&measured).unwrap());
    }
}
