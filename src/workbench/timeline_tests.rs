//! Real component replay verifies the synthetic host contract as well as paint.
use super::{
    Case, Workbench,
    replay::{Replay, marquee_points},
};
use crate::{
    settings::ResolvedTheme,
    ui::timeline::{CancelReason, Capabilities, HostState, Request},
};
use egui::{Event, Modifiers, PointerButton, Pos2};
use std::{sync::Arc, time::Duration};

const STEP: Duration = Duration::from_millis(16);

fn frame(replay: &mut Replay, events: Vec<Event>, elapsed: Duration) -> egui::FullOutput {
    replay.retry = true;
    let mut output = replay.frame(events, elapsed).unwrap();
    output.textures_delta.clear();
    output
}

fn settle(replay: &mut Replay) {
    for _ in 0..3 {
        frame(replay, Vec::new(), STEP);
    }
}

fn fixture(case: Case) -> Replay {
    let mut replay = Replay::new(case, ResolvedTheme::Light);
    settle(&mut replay);
    replay
}

fn click(replay: &mut Replay, instance: usize, target: &str) {
    let point = replay.target(instance, target).unwrap().rect.center();
    frame(replay, vec![Event::PointerMoved(point)], STEP);
    frame(
        replay,
        vec![Replay::button(point, PointerButton::Primary, true)],
        STEP,
    );
    frame(
        replay,
        vec![Replay::button(point, PointerButton::Primary, false)],
        STEP,
    );
    settle(replay);
}

fn scrub(replay: &mut Replay, instance: usize) {
    let ruler = replay.target(instance, "timeline-ruler").unwrap().rect;
    let start = ruler.center();
    let end = Pos2::new(ruler.left() + ruler.width() * 0.7, start.y);
    frame(replay, vec![Event::PointerMoved(start)], STEP);
    frame(
        replay,
        vec![Replay::button(start, PointerButton::Primary, true)],
        STEP,
    );
    assert!(replay.bench.timeline.as_ref().unwrap().timelines[instance].is_scrubbing());
    frame(replay, vec![Event::PointerMoved(end)], STEP);
    frame(
        replay,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    );
    assert!(!replay.bench.timeline.as_ref().unwrap().timelines[instance].is_scrubbing());
}

#[test]
fn timeline_reset_restores_host_and_component_state_without_changing_dimensions_or_identity() {
    let mut replay = fixture(Case::TimelineSmall);
    let original = ["timeline-canvas", "timeline-playpause", "timeline-key-1-1"]
        .map(|target| replay.target(0, target).unwrap().id);
    click(&mut replay, 0, "timeline-key-1-1");
    assert!(
        replay.bench.timeline.as_ref().unwrap().timelines[0]
            .selection()
            .is_some()
    );
    click(&mut replay, 0, "timeline-replace-data");
    replay.bench.appearance = ResolvedTheme::Dark;
    replay.bench.dimensions = egui::vec2(800.0, 430.0);
    let timeline = replay.bench.timeline.as_mut().unwrap();
    timeline.hosts[0] = HostState {
        accepted_time: 7.0,
        playing: true,
        looping: true,
        speed: 2.0,
        available: false,
    };
    timeline.capabilities.seek = false;
    timeline.reject_seek = true;
    settle(&mut replay);
    click(&mut replay, 0, "reset");
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert_eq!(timeline.data.revision, 0);
    assert_eq!(timeline.hosts[0], HostState::default());
    assert_eq!(timeline.capabilities, Capabilities::default());
    assert!(!timeline.reject_seek && timeline.events.is_empty());
    assert!(!timeline.timelines[0].is_scrubbing());
    assert!(timeline.timelines[0].selection().is_none());
    assert_eq!(replay.bench.appearance, ResolvedTheme::Dark);
    assert_eq!(replay.bench.dimensions, egui::vec2(800.0, 430.0));
    let reset = ["timeline-canvas", "timeline-playpause", "timeline-key-1-1"]
        .map(|target| replay.target(0, target).unwrap().id);
    assert_eq!(reset, original);
}

#[test]
fn timeline_instances_keep_transport_selection_view_and_gestures_independent() {
    let mut replay = fixture(Case::TimelineIsolation);
    assert_ne!(
        replay.target(0, "timeline-canvas").unwrap().id,
        replay.target(1, "timeline-canvas").unwrap().id,
    );
    let initial_view = replay.bench.timeline.as_ref().unwrap().timelines[1].visible_range();
    scrub(&mut replay, 0);
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(timeline.hosts[0].accepted_time > 0.0);
    assert_eq!(timeline.hosts[1].accepted_time, 0.0);
    assert!(timeline.events.iter().all(|event| event.instance == 0));
    let point = replay.target(0, "timeline-ruler").unwrap().rect.center();
    frame(&mut replay, vec![Event::PointerMoved(point)], STEP);
    frame(
        &mut replay,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, 80.0),
            modifiers: egui::Modifiers::CTRL,
            phase: egui::TouchPhase::Move,
        }],
        STEP,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert_ne!(timeline.timelines[0].visible_range(), initial_view);
    assert_eq!(timeline.timelines[1].visible_range(), initial_view);
    click(&mut replay, 1, "timeline-key-1-1");
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(timeline.timelines[0].selection().is_none());
    assert!(timeline.timelines[1].selection().is_some());
    assert!(!timeline.hosts[0].playing && !timeline.hosts[1].playing);
}

#[test]
fn timeline_host_clock_advances_only_playing_instances_once_across_layout_retries() {
    let mut replay = fixture(Case::TimelineIsolation);
    click(&mut replay, 0, "timeline-playpause");
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(timeline.hosts[0].playing);
    assert!(!timeline.hosts[1].playing);
    assert_eq!(
        timeline
            .events
            .iter()
            .filter(|event| event.request == Request::SetPlaying(true))
            .count(),
        1
    );
    let before = timeline.hosts[0].accepted_time;
    frame(&mut replay, Vec::new(), Duration::from_millis(250));
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!((timeline.hosts[0].accepted_time - before - 0.25).abs() < 1.0e-9);
    assert_eq!(timeline.hosts[1].accepted_time, 0.0);
    assert_eq!(
        timeline.events.len(),
        1,
        "Host playback creates no extra seek requests"
    );
}

#[test]
fn timeline_rejected_seek_requests_are_visible_without_moving_accepted_time() {
    let mut replay = fixture(Case::TimelineReject);
    scrub(&mut replay, 0);
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert_eq!(timeline.hosts[0].accepted_time, 0.0);
    assert!(!timeline.events.is_empty());
    assert!(timeline.events.iter().all(|event| !event.accepted));
    assert!(
        timeline
            .events
            .iter()
            .any(|event| matches!(event.request, Request::ScrubEnd { time, .. } if time > 0.0))
    );
    click(&mut replay, 0, "timeline-reject-seeks");
    scrub(&mut replay, 0);
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(timeline.hosts[0].accepted_time > 0.0);
    assert!(timeline.events.back().unwrap().accepted);
}

#[test]
fn timeline_replacement_cancels_a_live_scrub_and_reconciles_removed_selection() {
    let mut replay = fixture(Case::TimelineSmall);
    let point = replay.target(0, "timeline-ruler").unwrap().rect.center();
    frame(&mut replay, vec![Event::PointerMoved(point)], STEP);
    frame(
        &mut replay,
        vec![Replay::button(point, PointerButton::Primary, true)],
        STEP,
    );
    assert!(replay.bench.timeline.as_ref().unwrap().timelines[0].is_scrubbing());
    let timeline = replay.bench.timeline.as_mut().unwrap();
    let mut replacement = (*timeline.data).clone();
    replacement.revision += 1;
    timeline.data = Arc::new(replacement);
    frame(&mut replay, Vec::new(), STEP);
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(!timeline.timelines[0].is_scrubbing());
    assert_eq!(timeline.hosts[0].accepted_time, 0.0);
    assert_eq!(
        timeline
            .events
            .iter()
            .filter(|event| matches!(
                event.request,
                Request::ScrubCancel {
                    reason: CancelReason::DataChanged,
                    ..
                }
            ))
            .count(),
        1
    );
    frame(
        &mut replay,
        vec![Replay::button(point, PointerButton::Primary, false)],
        STEP,
    );
    click(&mut replay, 0, "timeline-key-1-1");
    assert!(
        replay.bench.timeline.as_ref().unwrap().timelines[0]
            .selection()
            .is_some()
    );
    let timeline = replay.bench.timeline.as_mut().unwrap();
    let mut replacement = (*timeline.data).clone();
    replacement.revision += 1;
    replacement.tracks[0].keys.remove(0);
    timeline.data = Arc::new(replacement);
    settle(&mut replay);
    assert!(
        replay.bench.timeline.as_ref().unwrap().timelines[0]
            .selection()
            .is_none()
    );
    assert!(replay.target(0, "timeline-key-1-1").is_err());
}

#[test]
fn timeline_scrub_phases_deliver_once_and_remain_ordered_under_repeated_layout() {
    let mut replay = fixture(Case::TimelineSmall);
    scrub(&mut replay, 0);
    let timeline = replay.bench.timeline.as_ref().unwrap();
    let requests: Vec<_> = timeline.events.iter().map(|event| event.request).collect();
    assert_eq!(
        requests.len(),
        3,
        "Fixture host records every request without deduplication"
    );
    let (begin, update, end) = match requests.as_slice() {
        [
            Request::ScrubBegin { gesture: begin, .. },
            Request::ScrubUpdate {
                gesture: update, ..
            },
            Request::ScrubEnd { gesture: end, .. },
        ] => (*begin, *update, *end),
        other => panic!("Unexpected scrub request order: {other:?}"),
    };
    assert_eq!(begin, update);
    assert_eq!(update, end);
    assert!(timeline.events.iter().all(|event| event.accepted));
}

#[test]
fn ordinary_cases_do_not_allocate_timeline_fixtures() {
    assert!(Workbench::default().timeline.is_none());
}

#[test]
fn timeline_expansion_culls_hidden_descendants_and_preserves_inspection_identity() {
    let mut replay = fixture(Case::TimelineHierarchy);
    let key = replay
        .bench
        .observed
        .iter()
        .find(|o| o.target.starts_with("timeline-key-3-"))
        .unwrap()
        .target
        .clone();
    click(&mut replay, 0, &key);
    let selection = replay.bench.timeline.as_ref().unwrap().timelines[0]
        .selection()
        .cloned();
    click(&mut replay, 0, "timeline-row-1");
    assert!(replay.target(0, "timeline-row-2").is_err());
    assert!(replay.target(0, &key).is_err());
    assert_eq!(
        replay.bench.timeline.as_ref().unwrap().timelines[0].selection(),
        selection.as_ref()
    );
    click(&mut replay, 0, "timeline-row-1");
    assert!(replay.target(0, "timeline-row-2").is_ok());
    assert!(replay.target(0, &key).is_ok());
}

#[test]
fn empty_and_zero_duration_fixtures_expose_host_owned_unavailable_and_disabled_transport() {
    let empty = fixture(Case::TimelineEmpty);
    assert!(!empty.target(0, "timeline-playpause").unwrap().enabled);
    assert!(!empty.target(0, "timeline-ruler").unwrap().enabled);
    let zero = fixture(Case::TimelineZero);
    assert!(!zero.target(0, "timeline-playpause").unwrap().enabled);
    assert!(zero.target(0, "timeline-ruler").unwrap().enabled);
}

#[test]
fn native_host_cancellation_preserves_focus_and_resize_reasons_and_delivers_once() {
    for reason in [CancelReason::FocusLost, CancelReason::Resize] {
        let mut replay = fixture(Case::TimelineSmall);
        let point = replay.target(0, "timeline-ruler").unwrap().rect.center();
        frame(
            &mut replay,
            vec![
                Event::PointerMoved(point),
                Replay::button(point, PointerButton::Primary, true),
            ],
            STEP,
        );
        replay.bench.clear_input(reason);
        replay.bench.clear_input(reason);
        settle(&mut replay);
        let timeline = replay.bench.timeline.as_ref().unwrap();
        assert_eq!(
            timeline
                .events
                .iter()
                .filter(|e| matches!(e.request,Request::ScrubCancel{reason:r,..} if r==reason))
                .count(),
            1
        );
        assert!(!timeline.timelines[0].is_scrubbing());
        assert_eq!(timeline.hosts[0].accepted_time, 0.0);
    }
}

fn begin_marquee(replay: &mut Replay, first: &str, last: &str, modifiers: Modifiers) -> Pos2 {
    let (start, end) = marquee_points(replay, 0, first, last).unwrap();
    frame(replay, vec![Event::PointerMoved(start)], STEP);
    frame(
        replay,
        vec![Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        }],
        STEP,
    );
    frame(replay, vec![Event::PointerMoved(end)], STEP);
    assert!(replay.bench.timeline.as_ref().unwrap().timelines[0].is_marquee_active());
    end
}

#[test]
fn timeline_marquee_selects_across_tracks_on_release_and_shift_adds_without_seeking() {
    let mut replay = fixture(Case::TimelineMarquee);
    let end = begin_marquee(
        &mut replay,
        "timeline-key-1-1",
        "timeline-key-3-2",
        Modifiers::NONE,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(timeline.timelines[0].selection().is_none());
    assert!(timeline.events.is_empty());
    assert_eq!(timeline.hosts[0].accepted_time, 0.0);
    frame(
        &mut replay,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    let selection = timeline.timelines[0].selection().unwrap();
    assert_eq!(selection.keys.len(), 6);
    for track in 1..=3 {
        for key in 1..=2 {
            assert!(
                selection
                    .keys
                    .iter()
                    .any(|selected| selected.track.0 == track && selected.key.0 == key)
            );
        }
    }
    assert!(!timeline.timelines[0].has_pointer_gesture());
    let baseline = selection.clone();
    let end = begin_marquee(
        &mut replay,
        "timeline-key-1-3",
        "timeline-key-3-3",
        Modifiers::SHIFT,
    );
    assert_eq!(
        replay.bench.timeline.as_ref().unwrap().timelines[0].selection(),
        Some(&baseline),
    );
    frame(
        &mut replay,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert_eq!(timeline.timelines[0].selection().unwrap().keys.len(), 9);
    assert!(
        timeline.events.is_empty(),
        "Inspection never emits a playback request"
    );
    assert_eq!(timeline.hosts[0].accepted_time, 0.0);
}

#[test]
fn timeline_cancelled_marquee_restores_selection_and_does_not_turn_release_into_seek() {
    let mut replay = fixture(Case::TimelineMarquee);
    click(&mut replay, 0, "timeline-key-3-5");
    let baseline = replay.bench.timeline.as_ref().unwrap().timelines[0]
        .selection()
        .cloned();
    let end = begin_marquee(
        &mut replay,
        "timeline-key-1-1",
        "timeline-key-3-2",
        Modifiers::NONE,
    );
    frame(
        &mut replay,
        vec![Replay::literal(egui::Key::Escape, true)],
        STEP,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(!timeline.timelines[0].has_pointer_gesture());
    assert_eq!(timeline.timelines[0].selection(), baseline.as_ref());
    frame(
        &mut replay,
        vec![
            Replay::literal(egui::Key::Escape, false),
            Replay::button(end, PointerButton::Primary, false),
        ],
        STEP,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert_eq!(timeline.timelines[0].selection(), baseline.as_ref());
    assert!(timeline.events.is_empty());
    assert_eq!(timeline.hosts[0].accepted_time, 0.0);
}

#[test]
fn timeline_empty_space_click_seeks_once_on_release_after_intent_is_resolved() {
    let mut replay = fixture(Case::TimelineMarquee);
    let marker = replay.target(0, "timeline-key-3-4").unwrap().rect;
    let point = marker.center() + egui::vec2(20.0, 0.0);
    frame(&mut replay, vec![Event::PointerMoved(point)], STEP);
    frame(
        &mut replay,
        vec![Replay::button(point, PointerButton::Primary, true)],
        STEP,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(timeline.timelines[0].has_pointer_gesture());
    assert!(!timeline.timelines[0].is_marquee_active());
    assert!(timeline.events.is_empty());
    assert_eq!(timeline.hosts[0].accepted_time, 0.0);
    frame(
        &mut replay,
        vec![Replay::button(point, PointerButton::Primary, false)],
        STEP,
    );
    let timeline = replay.bench.timeline.as_ref().unwrap();
    assert!(timeline.hosts[0].accepted_time > 0.0);
    assert_eq!(
        timeline.events.len(),
        1,
        "Layout retries do not duplicate the seek"
    );
    assert!(matches!(
        timeline.events.back().unwrap().request,
        Request::Seek { .. }
    ));
    assert!(!timeline.timelines[0].has_pointer_gesture());
    assert!(timeline.timelines[0].selection().is_none());
}

#[test]
fn timeline_host_can_disable_inspection_independently_of_playback() {
    let mut replay = fixture(Case::TimelineMarquee);
    replay.bench.timeline.as_mut().unwrap().capabilities.seek = false;
    let end = begin_marquee(
        &mut replay,
        "timeline-key-1-1",
        "timeline-key-3-2",
        Modifiers::NONE,
    );
    frame(
        &mut replay,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    );
    let baseline = replay.bench.timeline.as_ref().unwrap().timelines[0]
        .selection()
        .cloned();
    assert_eq!(baseline.as_ref().unwrap().keys.len(), 6);
    let end = begin_marquee(
        &mut replay,
        "timeline-key-1-3",
        "timeline-key-3-3",
        Modifiers::NONE,
    );
    replay.bench.timeline.as_mut().unwrap().capabilities.inspect = false;
    frame(&mut replay, Vec::new(), STEP);
    assert!(!replay.bench.timeline.as_ref().unwrap().timelines[0].has_pointer_gesture());
    frame(
        &mut replay,
        vec![Replay::button(end, PointerButton::Primary, false)],
        STEP,
    );
    click(&mut replay, 0, "timeline-key-1-5");
    let fixture = replay.bench.timeline.as_ref().unwrap();
    assert_eq!(fixture.timelines[0].selection(), baseline.as_ref());
    assert!(fixture.events.is_empty());
}
