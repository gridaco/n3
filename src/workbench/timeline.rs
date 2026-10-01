//! Synthetic timeline host. Playback and request acceptance belong here, never
//! to the reusable component or an artificial editor/animation scene.
use super::{Case, Observed};
use crate::ui::timeline::{
    CancelReason, Capabilities, Data, HostState, Key, KeyId, Metrics, Request, TimeRange, Timeline,
    Track, TrackId,
};
use egui::{Context, Id, Response, Ui};
use std::{collections::VecDeque, sync::Arc, time::Duration};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Recorded {
    pub instance: usize,
    pub request: Request,
    pub accepted: bool,
}

pub(super) struct Fixture {
    pub data: Arc<Data>,
    pub timelines: [Timeline; 2],
    pub hosts: [HostState; 2],
    pub capabilities: Capabilities,
    pub reject_seek: bool,
    pub events: VecDeque<Recorded>,
    pub delivered_frame: Vec<Recorded>,
    pub metrics: [Metrics; 2],
    case: Case,
    frame: Option<u64>,
    last_time: Option<f64>,
    scrub_baselines: [Option<(u64, f64)>; 2],
}

impl Fixture {
    pub fn new(case: Case) -> Self {
        let data = Arc::new(data(case, 0));
        let hosts = std::array::from_fn(|_| HostState {
            accepted_time: data.content.start,
            available: case != Case::TimelineEmpty,
            ..Default::default()
        });
        let capabilities = Capabilities {
            play_pause: case != Case::TimelineZero,
            looping: case != Case::TimelineZero,
            ..Default::default()
        };
        Self {
            data,
            timelines: std::array::from_fn(|_| Timeline::default()),
            hosts,
            capabilities,
            reject_seek: case == Case::TimelineReject,
            events: VecDeque::new(),
            delivered_frame: Vec::new(),
            metrics: [Metrics::default(); 2],
            case,
            frame: None,
            last_time: None,
            scrub_baselines: [None; 2],
        }
    }

    pub fn begin_frame(&mut self, ctx: &Context, count: usize) {
        let frame = ctx.cumulative_frame_nr();
        if self.frame == Some(frame) {
            return;
        }
        self.frame = Some(frame);
        self.delivered_frame.clear();
        let time = ctx.input(|input| input.time);
        let elapsed = self
            .last_time
            .map_or(0.0, |previous| (time - previous).max(0.0));
        self.last_time = Some(time);
        let duration = self.data.content.end - self.data.content.start;
        for host in self.hosts.iter_mut().take(count) {
            if !host.playing || !host.available {
                continue;
            }
            if duration <= 0.0 {
                host.accepted_time = self.data.content.start;
                host.playing = false;
                continue;
            }
            let next = host.accepted_time + elapsed * host.speed;
            if host.looping {
                host.accepted_time =
                    self.data.content.start + (next - self.data.content.start).rem_euclid(duration);
            } else {
                host.accepted_time = next.clamp(self.data.content.start, self.data.content.end);
                host.playing = next < self.data.content.end;
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    pub fn clear_input(&mut self, reason: CancelReason) {
        for index in 0..self.case.count() {
            if let Some(request) = self.timelines[index].cancel(reason) {
                self.deliver(index, request);
            }
        }
    }

    pub fn controls(&mut self, ui: &mut Ui, count: usize) -> Vec<Observed> {
        let mut observed = Vec::new();
        ui.push_id(("timeline-fixture-controls", self.case.id()), |ui| {
            ui.horizontal_wrapped(|ui| {
                observe(
                    &mut observed,
                    "timeline-reject-seeks",
                    &ui.checkbox(&mut self.reject_seek, "Reject seeks"),
                );
                for (index, host) in self.hosts.iter_mut().enumerate().take(count) {
                    let response =
                        ui.checkbox(&mut host.available, format!("Host {} available", index + 1));
                    observed.push(Observed {
                        instance: index,
                        target: "timeline-host-available".into(),
                        id: response.id,
                        rect: response.rect,
                        enabled: response.enabled(),
                    });
                }
                let replace = ui.button("Replace fixture data");
                observe(&mut observed, "timeline-replace-data", &replace);
                if replace.clicked() {
                    self.data = Arc::new(data(self.case, self.data.revision + 1));
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Host capabilities");
                observe(
                    &mut observed,
                    "timeline-cap-inspect",
                    &ui.checkbox(&mut self.capabilities.inspect, "Inspect keys"),
                );
                observe(
                    &mut observed,
                    "timeline-cap-seek",
                    &ui.checkbox(&mut self.capabilities.seek, "Seek"),
                );
                observe(
                    &mut observed,
                    "timeline-cap-play",
                    &ui.checkbox(&mut self.capabilities.play_pause, "Play/pause"),
                );
                observe(
                    &mut observed,
                    "timeline-cap-loop",
                    &ui.checkbox(&mut self.capabilities.looping, "Loop"),
                );
                observe(
                    &mut observed,
                    "timeline-cap-speed",
                    &ui.checkbox(&mut self.capabilities.speed, "Speed"),
                );
            });
        });
        observed
    }

    pub fn show(&mut self, ui: &mut Ui, id: Id, index: usize) -> Vec<Observed> {
        let data = self.data.clone();
        let frame =
            self.timelines[index].prepare(ui, id, &data, &self.hosts[index], &self.capabilities);
        // Every component request is recorded. No application-command dedup is
        // used here: duplicated gesture phases must remain visible to tests.
        for request in frame.requests().iter().copied() {
            self.deliver(index, request);
        }
        let output =
            self.timelines[index].finish(ui, frame, &self.hosts[index], &self.capabilities);
        self.metrics[index] = output.metrics;
        output
            .observations
            .into_iter()
            .map(|observation| Observed {
                instance: index,
                target: observation.target.name(),
                id: observation.id,
                rect: observation.rect,
                enabled: observation.enabled,
            })
            .collect()
    }

    fn deliver(&mut self, index: usize, request: Request) {
        let host = &mut self.hosts[index];
        if let Request::ScrubBegin { gesture, .. } = request {
            self.scrub_baselines[index] = Some((gesture, host.accepted_time));
        }
        let accepted = match &request {
            Request::Seek { time }
            | Request::ScrubBegin { time, .. }
            | Request::ScrubUpdate { time, .. }
            | Request::ScrubEnd { time, .. } => {
                host.available && self.capabilities.seek && !self.reject_seek && time.is_finite()
            }
            Request::ScrubCancel { .. } => true,
            Request::SetPlaying(_) => host.available && self.capabilities.play_pause,
            Request::SetLooping(_) => host.available && self.capabilities.looping,
            Request::SetSpeed(speed) => {
                host.available && self.capabilities.speed && speed.is_finite() && *speed > 0.0
            }
        };
        if accepted {
            match request {
                Request::Seek { time } => host.accepted_time = clamp_time(&self.data, time),
                Request::ScrubBegin { time, .. } => {
                    host.accepted_time = clamp_time(&self.data, time);
                }
                Request::ScrubUpdate { time, .. } => {
                    host.accepted_time = clamp_time(&self.data, time)
                }
                Request::ScrubEnd { time, .. } => {
                    host.accepted_time = clamp_time(&self.data, time);
                    self.scrub_baselines[index] = None;
                }
                Request::ScrubCancel { gesture, .. } => {
                    if let Some((baseline_gesture, baseline)) = self.scrub_baselines[index].take()
                        && baseline_gesture == gesture
                    {
                        host.accepted_time = baseline;
                    }
                }
                Request::SetPlaying(playing) => host.playing = playing,
                Request::SetLooping(looping) => host.looping = looping,
                Request::SetSpeed(speed) => host.speed = speed,
            }
        }
        if matches!(request, Request::ScrubEnd { .. }) {
            self.scrub_baselines[index] = None;
        }
        let event = Recorded {
            instance: index,
            request,
            accepted,
        };
        self.delivered_frame.push(event.clone());
        self.events.push_back(event);
        if self.events.len() > 32 {
            self.events.pop_front();
        }
    }

    pub fn inspector(&self, ui: &mut Ui, count: usize) {
        ui.label(format!(
            "Data revision {} · {} tracks",
            self.data.revision,
            self.data.tracks.len()
        ));
        for index in 0..count {
            let host = &self.hosts[index];
            let component = &self.timelines[index];
            ui.monospace(format!(
                "{}: accepted={:.3}s · playing={} · loop={} · {:.2}×",
                index + 1,
                host.accepted_time,
                host.playing,
                host.looping,
                host.speed
            ));
            ui.small(format!(
                "View: {:?} · scrub={} · marquee={}",
                component.visible_range(),
                component.is_scrubbing(),
                component.is_marquee_active()
            ));
            if let Some(selection) = component.selection() {
                ui.small(format!("Selected {} keys", selection.keys.len()));
                for selected in selection.keys.iter().take(3) {
                    let Some(track) = self
                        .data
                        .tracks
                        .iter()
                        .find(|track| track.id == selected.track)
                    else {
                        continue;
                    };
                    let Some(key) = track.keys.iter().find(|key| key.id == selected.key) else {
                        continue;
                    };
                    ui.small(format!(
                        "Track {} · key {} · {} s",
                        selected.track.0, key.id.0, key.time
                    ));
                    if let Some(metadata) = &key.metadata {
                        ui.small(metadata);
                    }
                }
                if selection.keys.len() > 3 {
                    ui.weak("More selected keys; zoom in to inspect individually.");
                }
            }
            let metrics = self.metrics[index];
            ui.small(format!(
                "Paint: {}/{} rows · {}/{} keys considered · {} markers",
                metrics.rows,
                metrics.total_tracks,
                metrics.keys_considered,
                metrics.total_keys,
                metrics.markers
            ));
        }
        ui.separator();
        ui.label("Typed timeline requests (last 32)");
        for event in self.events.iter().rev().take(6) {
            ui.small(format!(
                "{}: {:?} · {}",
                event.instance + 1,
                event.request,
                if event.accepted {
                    "accepted"
                } else {
                    "rejected"
                }
            ));
        }
        ui.separator();
    }
}

fn observe(observed: &mut Vec<Observed>, target: &str, response: &Response) {
    observed.push(Observed {
        instance: 0,
        target: target.into(),
        id: response.id,
        rect: response.rect,
        enabled: response.enabled(),
    });
}

fn clamp_time(data: &Data, time: f64) -> f64 {
    time.clamp(data.content.start, data.content.end)
}

fn data(case: Case, revision: u64) -> Data {
    let content = match case {
        Case::TimelineZero => TimeRange {
            start: 2.0,
            end: 2.0,
        },
        Case::TimelineHierarchy => TimeRange {
            start: -2.0,
            end: 8.0,
        },
        _ => TimeRange {
            start: 0.0,
            end: 10.0,
        },
    };
    let tracks = match case {
        Case::TimelineEmpty => Vec::new(),
        Case::TimelineZero => vec![Track {
            id: TrackId(1),
            parent: None,
            label: "Single instant".into(),
            keys: vec![Key {
                id: KeyId(1),
                time: 2.0,
                metadata: Some("Zero-duration content".into()),
            }],
        }],
        Case::TimelineDense => (0..1000)
            .map(|row| Track {
                id: TrackId(row + 1),
                parent: None,
                label: format!("Synthetic track {:04}", row + 1),
                keys: (0..100)
                    .map(|key| Key {
                        id: KeyId(row * 100 + key + 1),
                        time: key as f64 * 0.093
                            + ((key * 37 + row * 7) % 11) as f64 * 0.001
                            + (revision % 2) as f64 * 0.05,
                        metadata: Some(format!("Track {} · key {}", row + 1, key + 1)),
                    })
                    .collect(),
            })
            .collect(),
        // Repeated key IDs across tracks are valid. This fixture makes loss of
        // track-qualified identity visible when one rectangle crosses rows.
        Case::TimelineMarquee => ["Position X", "Position Y", "Visibility"]
            .into_iter()
            .enumerate()
            .map(|(index, label)| {
                let mut track = track(index, None, label, revision);
                for (index, key) in track.keys.iter_mut().enumerate() {
                    key.id = KeyId(index as u64 + 1);
                }
                track
            })
            .collect(),
        Case::TimelineHierarchy => {
            let labels = [
                (None, "Joint · Spine"),
                (Some(1), "Translation"),
                (Some(2), "X"),
                (Some(2), "Y"),
                (Some(2), "Z"),
                (Some(1), "Rotation · quaternion"),
                (Some(6), "Quaternion X"),
                (Some(6), "Quaternion Y"),
                (Some(6), "Quaternion Z"),
                (
                    Some(6),
                    "Quaternion W — a long component label that clips inside its own header",
                ),
                (Some(1), "Scale"),
                (Some(11), "X"),
                (Some(11), "Y"),
                (Some(11), "Z"),
                (None, "Blend shape weights"),
                (Some(15), "Weight 0"),
                (Some(15), "Weight 1"),
            ];
            labels
                .into_iter()
                .enumerate()
                .map(|(index, (parent, label))| {
                    let mut track = track(index, parent.map(TrackId), label, revision);
                    for (sample, key) in track.keys.iter_mut().enumerate() {
                        key.time -= 2.0;
                        let angle = sample as f64 * 0.2;
                        let weights = 0.2 + sample as f64 * 0.1;
                        key.metadata = Some(match track.id.0 {
                            6 => format!(
                                "Quaternion [0, 0, {:.6}, {:.6}]",
                                (angle * 0.5).sin(),
                                (angle * 0.5).cos()
                            ),
                            7 | 8 => "Quaternion component = 0".into(),
                            9 => format!("Quaternion Z = {:.6}", (angle * 0.5).sin()),
                            10 => format!("Quaternion W = {:.6}", (angle * 0.5).cos()),
                            15 => format!("Weights [{weights:.2}, {:.2}]", 1.0 - weights),
                            16 => format!("Weight 0 = {weights:.2}"),
                            17 => format!("Weight 1 = {:.2}", 1.0 - weights),
                            _ => format!("{label} · sample {}", sample + 1),
                        });
                    }
                    track
                })
                .collect()
        }
        _ => ["Position X", "Position Y", "Visibility"]
            .into_iter()
            .enumerate()
            .map(|(index, label)| track(index, None, label, revision))
            .collect(),
    };
    Data {
        revision,
        content,
        tracks,
    }
}

fn track(index: usize, parent: Option<TrackId>, label: &str, revision: u64) -> Track {
    Track {
        id: TrackId(index as u64 + 1),
        parent,
        label: label.into(),
        keys: [0.35, 1.2, 2.8, 4.1, 8.5]
            .into_iter()
            .enumerate()
            .map(|(key, time)| Key {
                id: KeyId(index as u64 * 10 + key as u64 + 1),
                time: time + index as f64 * 0.02 + (revision % 2) as f64 * 0.1,
                metadata: Some(format!("{label} · sample {}", key + 1)),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_cases_supply_valid_synthetic_data_and_the_dense_workload_is_exact() {
        for case in Case::ALL.into_iter().filter(|case| case.timeline()) {
            let data = data(case, 0);
            crate::ui::timeline::data::Prepared::new(&data).unwrap();
            if case == Case::TimelineDense {
                assert_eq!(data.tracks.len(), 1000);
                assert!(data.tracks.iter().all(|track| track.keys.len() == 100));
                let keys = &data.tracks[0].keys;
                let initial_spacing = keys[1].time - keys[0].time;
                assert!(keys.windows(2).any(|pair| {
                    (pair[1].time - pair[0].time - initial_spacing).abs() > 1.0e-9
                }));
            }
            if case == Case::TimelineEmpty {
                assert!(data.tracks.is_empty());
            }
            if case == Case::TimelineZero {
                assert_eq!(data.content.start, data.content.end);
            }
        }
    }

    #[test]
    fn fixture_keeps_requested_and_accepted_times_separate_and_restores_cancelled_scrubs() {
        let mut fixture = Fixture::new(Case::TimelineReject);
        fixture.deliver(0, Request::Seek { time: 5.0 });
        assert_eq!(fixture.hosts[0].accepted_time, 0.0);
        assert!(!fixture.events.back().unwrap().accepted);
        fixture.reject_seek = false;
        fixture.deliver(0, Request::Seek { time: 5.0 });
        assert_eq!(fixture.hosts[0].accepted_time, 5.0);
        assert!(fixture.events.back().unwrap().accepted);
        fixture.deliver(
            0,
            Request::ScrubBegin {
                gesture: 7,
                time: 6.0,
            },
        );
        fixture.deliver(
            0,
            Request::ScrubUpdate {
                gesture: 7,
                time: 8.0,
            },
        );
        assert_eq!(fixture.hosts[0].accepted_time, 8.0);
        fixture.deliver(
            0,
            Request::ScrubCancel {
                gesture: 7,
                reason: CancelReason::Escape,
            },
        );
        assert_eq!(fixture.hosts[0].accepted_time, 5.0);
        assert_eq!(fixture.events.len(), 5);
        assert_eq!(fixture.hosts[1].accepted_time, 0.0);
    }

    #[test]
    fn synthetic_playback_uses_explicit_time_once_per_frame_and_is_instance_local() {
        let ctx = Context::default();
        let mut fixture = Fixture::new(Case::TimelineIsolation);
        fixture.hosts[0].playing = true;
        fixture.hosts[0].speed = 2.0;
        for time in [0.0, 0.5] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ui| {
                    fixture.begin_frame(ui.ctx(), 2);
                    if ui.ctx().current_pass_index() == 0 {
                        ui.ctx()
                            .request_discard("exercise synthetic host clock retry");
                    }
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(fixture.hosts[0].accepted_time, 1.0);
        assert_eq!(fixture.hosts[1].accepted_time, 0.0);
        assert!(
            fixture.events.is_empty(),
            "Playback is accepted host state, never a UI request"
        );
    }
}
