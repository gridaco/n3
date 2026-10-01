use super::*;

struct Harness {
    ctx: egui::Context,
    timeline: Timeline,
    data: Data,
    host: HostState,
    cap: Capabilities,
    output: Output,
    size: Vec2,
    retry: bool,
    parent_scroll: Vec2,
    text: String,
    synchronous: Option<fn(&mut Timeline, &mut HostState, Request)>,
    shapes: Vec<egui::epaint::ClippedShape>,
    passes: usize,
    discard_reasons: Vec<String>,
}
impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        crate::ui::workspace_ui::configure_context(&ctx);
        Self {
            ctx,
            timeline: Timeline::default(),
            host: HostState::default(),
            cap: Capabilities::default(),
            data: Data {
                revision: 0,
                content: TimeRange::new(-2.0, 8.0).unwrap(),
                tracks: (0..100)
                    .map(|row| Track {
                        id: TrackId(row),
                        parent: None,
                        label: format!("Track {row}"),
                        keys: vec![
                            Key {
                                id: KeyId(1),
                                time: 0.125,
                                metadata: None,
                            },
                            Key {
                                id: KeyId(2),
                                time: 0.126,
                                metadata: None,
                            },
                        ],
                    })
                    .collect(),
            },
            output: Output::default(),
            size: egui::vec2(700.0, 430.0),
            retry: true,
            parent_scroll: Vec2::ZERO,
            text: String::new(),
            synchronous: None,
            shapes: Vec::new(),
            passes: 0,
            discard_reasons: Vec::new(),
        }
    }
    fn frame(&mut self, events: Vec<Event>) -> Vec<Request> {
        let mut requests = Vec::new();
        let ctx = self.ctx.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                events,
                ..Default::default()
            },
            |ui| {
                ui.push_id("competing-text", |ui| {
                    ui.text_edit_singleline(&mut self.text);
                });
                let scroller = egui::ScrollArea::vertical()
                    .id_salt("parent")
                    .max_height(300.0)
                    .show(ui, |ui| {
                        let rect = Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(ui.available_width(), 280.0),
                        );
                        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                        self.output = if let Some(apply) = self.synchronous {
                            let frame = self.timeline.prepare(
                                &mut child,
                                Id::new("timeline.test"),
                                &self.data,
                                &self.host,
                                &self.cap,
                            );
                            for &request in frame.requests() {
                                apply(&mut self.timeline, &mut self.host, request);
                            }
                            self.timeline
                                .finish(&mut child, frame, &self.host, &self.cap)
                        } else {
                            self.timeline.show(
                                &mut child,
                                Id::new("timeline.test"),
                                &self.data,
                                &self.host,
                                &self.cap,
                            )
                        };
                        requests.extend(self.output.requests.iter().copied());
                        ui.advance_cursor_after_rect(rect);
                        ui.add_space(800.0);
                    });
                self.parent_scroll = scroller.state.offset;
                if self.retry && ui.ctx().current_pass_index() == 0 {
                    ui.ctx().request_discard("timeline regression");
                }
            },
        );
        self.passes = output.platform_output.num_completed_passes;
        self.discard_reasons = output
            .platform_output
            .request_discard_reasons
            .iter()
            .map(|reason| format!("{reason:?}"))
            .collect();
        self.shapes = std::mem::take(&mut output.shapes);
        output.textures_delta.clear();
        requests
    }
    fn settle(&mut self) {
        for _ in 0..3 {
            assert!(self.frame(Vec::new()).is_empty());
        }
    }
    fn ruler(&self) -> Rect {
        self.output
            .observations
            .iter()
            .find(|o| o.target == Target::Ruler)
            .unwrap()
            .rect
    }
    fn button(point: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos: point,
            button: PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn begin(&mut self) -> (Pos2, u64) {
        let point = self.ruler().center();
        self.frame(vec![Event::PointerMoved(point)]);
        let requests = self.frame(vec![Self::button(point, true)]);
        let gesture = match requests.as_slice() {
            [Request::ScrubBegin { gesture, .. }] => *gesture,
            other => panic!("{other:?}"),
        };
        (point, gesture)
    }

    fn assert_playhead(&self, time: f64) {
        let ruler = self.ruler();
        let x = navigation::x_at(
            self.timeline.visible_range().unwrap(),
            ruler.left(),
            ruler.width(),
            time,
        );
        let expected = egui::pos2(x, ruler.top() + 10.0);
        fn has_playhead(shape: &egui::Shape, expected: Pos2) -> bool {
            match shape {
                egui::Shape::Path(path) => {
                    path.closed && path.points.len() == 5 && path.points[3] == expected
                }
                egui::Shape::Vec(shapes) => {
                    shapes.iter().any(|shape| has_playhead(shape, expected))
                }
                _ => false,
            }
        }
        assert!(
            self.shapes
                .iter()
                .any(|shape| has_playhead(&shape.shape, expected)),
            "The rendered accepted playhead must match time {time} in this pass"
        );
    }

    fn painted_text(&self) -> String {
        fn append(shape: &egui::Shape, text: &mut String) {
            match shape {
                egui::Shape::Text(shape) => {
                    text.push_str(shape.galley.text());
                    text.push('\n');
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        append(shape, text);
                    }
                }
                _ => {}
            }
        }
        let mut text = String::new();
        for shape in &self.shapes {
            append(&shape.shape, &mut text);
        }
        text
    }
}

fn accept_seek(_: &mut Timeline, host: &mut HostState, request: Request) {
    match request {
        Request::Seek { time }
        | Request::ScrubBegin { time, .. }
        | Request::ScrubUpdate { time, .. }
        | Request::ScrubEnd { time, .. } => host.accepted_time = time,
        _ => {}
    }
}

#[test]
fn synchronous_acceptance_paints_the_new_playhead_without_additional_layout_passes() {
    let mut h = Harness::new();
    h.retry = false;
    h.synchronous = Some(accept_seek);
    h.settle();
    let (point, gesture) = h.begin();
    h.assert_playhead(h.host.accepted_time);
    assert!(!h.painted_text().contains("Requested"));
    assert_eq!(h.passes, 1);
    assert!(h.discard_reasons.is_empty());
    for offset in 1..12 {
        let position = point + Vec2::X * (offset as f32 * 5.0);
        let requests = h.frame(vec![Event::PointerMoved(position)]);
        assert!(
            matches!(requests.as_slice(), [Request::ScrubUpdate { gesture: g, .. }] if *g == gesture)
        );
        assert_eq!(
            h.passes, 1,
            "Continuous input must not request layout retries"
        );
        assert!(h.discard_reasons.is_empty());
        h.assert_playhead(h.host.accepted_time);
        assert!(!h.painted_text().contains("Requested"));
    }
}

#[test]
fn synchronous_rejection_and_clamping_keep_pending_feedback_and_actual_accepted_playhead() {
    for apply in [
        (|_: &mut Timeline, _: &mut HostState, _: Request| {})
            as fn(&mut Timeline, &mut HostState, Request),
        |_, host, _| host.accepted_time = 1.0,
    ] {
        let mut h = Harness::new();
        h.retry = false;
        h.synchronous = Some(apply);
        h.settle();
        h.begin();
        assert!(h.timeline.requested.is_some());
        assert!(h.painted_text().contains("Requested"));
        assert!(
            h.painted_text()
                .contains(&format!("accepted {:.3} s", h.host.accepted_time))
        );
        h.assert_playhead(h.host.accepted_time);
        assert_eq!(h.passes, 1);
    }
}

#[test]
fn synchronous_acceptance_delivers_once_when_unrelated_widgets_retry_layout() {
    let mut h = Harness::new();
    h.synchronous = Some(accept_seek);
    h.settle();
    let (point, gesture) = h.begin();
    assert_eq!(h.passes, 2, "The regression deliberately retries layout");
    let requests = h.frame(vec![Event::PointerMoved(point), Event::PointerMoved(point)]);
    assert_eq!(
        requests.len(),
        2,
        "Distinct source events survive; retry duplicates do not"
    );
    assert!(requests.iter().all(
        |request| matches!(request, Request::ScrubUpdate { gesture: g, .. } if *g == gesture)
    ));
    h.assert_playhead(h.host.accepted_time);
    assert!(!h.painted_text().contains("Requested"));
}

#[test]
fn synchronous_source_precision_acknowledgment_and_cancellation_are_visible_before_finish() {
    let mut h = Harness::new();
    h.retry = false;
    h.synchronous = Some(|timeline, host, request| match request {
        Request::ScrubBegin { time, .. } | Request::ScrubUpdate { time, .. } => {
            host.accepted_time = f64::from(time as f32);
            timeline.acknowledge_seek(time);
        }
        Request::ScrubCancel { .. } => {
            host.accepted_time = 0.0;
            assert!(timeline.cancel(CancelReason::Escape).is_none());
        }
        _ => {}
    });
    h.settle();
    let (point, _) = h.begin();
    h.frame(vec![Event::PointerMoved(point + Vec2::X * 1.123)]);
    assert!(h.timeline.requested.is_none());
    assert!(h.timeline.is_scrubbing());
    h.assert_playhead(h.host.accepted_time);
    assert!(!h.painted_text().contains("Requested"));
    let requests = h.frame(vec![Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }]);
    assert!(matches!(requests.as_slice(), [Request::ScrubCancel { .. }]));
    assert!(!h.timeline.is_scrubbing());
    h.assert_playhead(0.0);
    assert!(!h.painted_text().contains("Requested"));
}

#[test]
fn time_labels_use_seconds_without_frame_quantization() {
    assert_eq!(paint::time_label(-0.125, 0.025), "-0.125 s");
    assert_eq!(paint::time_label(0.0, 1.0), "0 s");
}

#[test]
fn shared_header_wraps_without_overlapping_ruler_or_changing_control_identities() {
    let mut h = Harness::new();
    let mut identities = None;
    let mut heights = Vec::new();
    for width in [700.0, 356.0, 320.0, 800.0] {
        h.size.x = width;
        h.settle();
        let ruler = h.ruler();
        let controls: Vec<_> = h
            .output
            .observations
            .iter()
            .filter(|o| matches!(o.target, Target::Transport(_)))
            .collect();
        assert_eq!(controls.len(), 5);
        for control in &controls {
            assert!(control.rect.is_positive() && control.enabled, "{control:?}");
            assert!(
                control.rect.right() <= ruler.left(),
                "Transport cannot overlap time: {control:?}"
            );
            assert!(control.rect.top() >= ruler.top() && control.rect.bottom() <= ruler.bottom());
        }
        let ids: Vec<_> = controls.iter().map(|o| o.id).collect();
        if let Some(first) = &identities {
            assert_eq!(&ids, first);
        } else {
            identities = Some(ids);
        }
        let row = h
            .output
            .observations
            .iter()
            .find(|o| o.target == Target::Row(TrackId(0)))
            .unwrap();
        assert_eq!(row.rect.right(), ruler.left());
        assert_eq!(row.rect.top(), ruler.bottom());
        heights.push(ruler.height());
    }
    assert!(
        heights[1] > heights[0],
        "Narrow headers must make room for every control"
    );
    assert_eq!(heights[3], heights[0]);
}

#[test]
fn embedded_transport_keeps_native_input_and_never_starts_a_scrub() {
    let mut h = Harness::new();
    h.size.x = 356.0;
    h.settle();
    for (control, expected) in [
        (
            transport::TransportControl::PlayPause,
            Request::SetPlaying(true),
        ),
        (transport::TransportControl::Loop, Request::SetLooping(true)),
    ] {
        let target = Target::Transport(control);
        let point = h
            .output
            .observations
            .iter()
            .find(|o| o.target == target)
            .unwrap()
            .rect
            .center();
        h.frame(vec![Event::PointerMoved(point)]);
        assert!(h.frame(vec![Harness::button(point, true)]).is_empty());
        assert!(!h.timeline.is_scrubbing());
        assert_eq!(h.frame(vec![Harness::button(point, false)]), vec![expected]);
        h.settle();
    }
    let time = h
        .output
        .observations
        .iter()
        .find(|o| o.target == Target::Transport(transport::TransportControl::Time))
        .unwrap()
        .rect
        .center();
    h.frame(vec![Event::PointerMoved(time)]);
    assert!(h.frame(vec![Harness::button(time, true)]).is_empty());
    let requests = h.frame(vec![Event::PointerMoved(time + Vec2::X * 24.0)]);
    assert!(
        !requests.is_empty(),
        "Native current-time dragging should emit a seek"
    );
    assert!(requests.iter().all(|r| matches!(r, Request::Seek { .. })));
    assert!(!h.timeline.is_scrubbing());
    assert!(
        h.frame(vec![Harness::button(time + Vec2::X * 24.0, false)])
            .iter()
            .all(|r| matches!(r, Request::Seek { .. }))
    );
    h.settle();
    h.begin();
    assert!(
        h.timeline.is_scrubbing(),
        "Ruler retains its own native input surface"
    );
    h.size.y = 70.0;
    assert!(matches!(
        h.frame(Vec::new()).as_slice(),
        [Request::ScrubCancel {
            reason: CancelReason::Resize,
            ..
        }]
    ));
    assert!(!h.timeline.is_scrubbing());
    assert!(
        h.frame(Vec::new()).is_empty(),
        "Clipping below header height cancels only once across retries"
    );
}

#[test]
fn continuous_events_remain_ordered_even_when_identical_and_release_outside() {
    let mut h = Harness::new();
    h.settle();
    let (p, gesture) = h.begin();
    assert!(
        h.ctx.egui_is_using_pointer(),
        "Native pointer ownership belongs to the timeline"
    );
    let requests = h.frame(vec![Event::PointerMoved(p), Event::PointerMoved(p)]);
    assert_eq!(
        requests.len(),
        2,
        "Equal updates are distinct events; layout retries are not"
    );
    assert!(
        requests
            .iter()
            .all(|r| matches!(r,Request::ScrubUpdate{gesture:g,..} if *g==gesture))
    );
    let outside = egui::pos2(1000.0, 800.0);
    let requests = h.frame(vec![
        Event::PointerMoved(outside),
        Harness::button(outside, false),
    ]);
    assert!(
        matches!(requests.as_slice(),[Request::ScrubUpdate{..},Request::ScrubEnd{gesture:g,..}] if *g==gesture)
    );
    assert!(!h.timeline.is_scrubbing());
    assert_eq!(
        h.host.accepted_time, 0.0,
        "Kit requests never accept their own time"
    );
}

#[test]
fn cancellation_is_terminal_once_for_escape_focus_resize_replacement_and_availability() {
    for reason in [
        CancelReason::Escape,
        CancelReason::FocusLost,
        CancelReason::Resize,
        CancelReason::DataChanged,
        CancelReason::Unavailable,
        CancelReason::PointerGone,
        CancelReason::InputOwner,
    ] {
        let mut h = Harness::new();
        h.settle();
        let (point, gesture) = h.begin();
        let events = match reason {
            CancelReason::Escape => vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            CancelReason::FocusLost => vec![Event::WindowFocused(false)],
            CancelReason::PointerGone => vec![Event::PointerGone],
            CancelReason::Resize => {
                h.size.x -= 120.0;
                vec![]
            }
            CancelReason::DataChanged => {
                h.data.revision += 1;
                h.data.tracks.remove(0);
                vec![]
            }
            CancelReason::Unavailable => {
                h.cap.seek = false;
                vec![]
            }
            CancelReason::InputOwner => {
                h.ctx.set_dragged_id(Id::new("other gesture"));
                vec![]
            }
        };
        assert_eq!(h.frame(events), [Request::ScrubCancel { gesture, reason }]);
        assert!(!h.timeline.is_scrubbing());
        assert!(h.timeline.requested.is_none());
        assert!(h.frame(vec![Harness::button(point, false)]).is_empty());
    }
}

#[test]
fn acknowledged_requests_do_not_become_stale_rejection_feedback_when_host_clock_advances() {
    let mut h = Harness::new();
    h.settle();
    let (point, _) = h.begin();
    h.host.accepted_time = h.timeline.requested.unwrap();
    h.frame(vec![Harness::button(point, false)]);
    h.frame(vec![]);
    assert!(h.timeline.requested.is_none());
    h.host.accepted_time += 0.125;
    h.frame(vec![]);
    assert!(h.timeline.requested.is_none());
}

#[test]
fn navigation_preserves_pointer_anchor_and_consumes_parent_scroll_including_retries_and_tail() {
    let mut h = Harness::new();
    h.settle();
    let ruler = h.ruler();
    let point = egui::pos2(ruler.left() + ruler.width() * 0.3, ruler.center().y);
    h.frame(vec![Event::PointerMoved(point)]);
    let before = h.timeline.visible_range().unwrap();
    let anchor = navigation::time_at(before, ruler.left(), ruler.width(), point.x);
    h.frame(vec![Event::Zoom(2.0)]);
    let after = h.timeline.visible_range().unwrap();
    assert!((before.duration() / 2.0 - after.duration()).abs() < 1e-9);
    assert!(
        (navigation::time_at(after, ruler.left(), ruler.width(), point.x) - anchor).abs() < 1e-9
    );
    h.frame(vec![Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -200.0),
        modifiers: egui::Modifiers::NONE,
        phase: egui::TouchPhase::Move,
    }]);
    assert!(h.timeline.scroll > 0.0);
    for _ in 0..8 {
        h.frame(Vec::new());
    }
    assert_eq!(
        h.parent_scroll,
        Vec2::ZERO,
        "Inner timeline navigation must not move the enclosing scroll area"
    );
    assert!(h.output.metrics.rows < 100);
    assert!(h.output.observations.iter().all(|o| o.rect.is_positive()));
}

#[test]
fn dense_cluster_inspection_preserves_ids_and_revision_reconciles_removal() {
    let mut h = Harness::new();
    h.settle();
    let cluster = h
        .output
        .observations
        .iter()
        .find(|o| {
            matches!(
                o.target,
                Target::Cluster {
                    track: TrackId(0),
                    ..
                }
            )
        })
        .unwrap()
        .clone();
    h.frame(vec![
        Event::PointerMoved(cluster.rect.center()),
        Harness::button(cluster.rect.center(), true),
    ]);
    h.frame(vec![Harness::button(cluster.rect.center(), false)]);
    assert_eq!(
        h.timeline.selection().unwrap().keys,
        [
            KeyRef {
                track: TrackId(0),
                key: KeyId(1)
            },
            KeyRef {
                track: TrackId(0),
                key: KeyId(2)
            }
        ]
    );
    assert_eq!(h.timeline.inspected_keys(&h.data).count(), 2);
    assert!(!h.output.metrics.prepared);
    h.data.revision += 1;
    h.data.tracks[0].keys.remove(0);
    h.frame(vec![]);
    assert_eq!(
        h.timeline.selection().unwrap().keys,
        [KeyRef {
            track: TrackId(0),
            key: KeyId(2)
        }]
    );
    h.data.revision += 1;
    h.data.tracks.clear();
    h.frame(vec![]);
    assert!(h.timeline.selection().is_none());
}

#[test]
fn key_at_accepted_playhead_wins_inspection_and_tiny_clips_have_no_invalid_bounds() {
    let mut h = Harness::new();
    h.data.tracks.truncate(1);
    h.data.tracks[0].keys.truncate(1);
    h.host.accepted_time = 0.125;
    h.settle();
    let key = h
        .output
        .observations
        .iter()
        .find(|o| matches!(o.target, Target::Key { .. }))
        .unwrap()
        .rect
        .center();
    assert!(
        h.frame(vec![Event::PointerMoved(key), Harness::button(key, true)])
            .is_empty()
    );
    assert!(h.timeline.selection().is_some());
    assert!(!h.timeline.is_scrubbing());
    h.frame(vec![Harness::button(key, false)]);
    for width in [8.0, 20.0, 32.0, 90.0] {
        let ctx = h.ctx.clone();
        let mut rendered = ctx.run_ui(egui::RawInput::default(), |ui| {
            let rect = Rect::from_min_size(ui.cursor().min, egui::vec2(width, 10.0));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
            child.set_clip_rect(rect);
            let output = h.timeline.show(
                &mut child,
                Id::new("timeline.tiny"),
                &h.data,
                &h.host,
                &h.cap,
            );
            assert!(
                output
                    .observations
                    .iter()
                    .all(|o| !matches!(o.target, Target::Key { .. } | Target::Row(_)))
            );
        });
        rendered.textures_delta.clear();
    }
}

#[test]
fn text_popup_and_disabled_seek_own_input_and_space_has_no_binding() {
    let mut h = Harness::new();
    h.settle();
    let point = h.ruler().center();
    h.cap.seek = false;
    assert!(
        h.frame(vec![
            Event::PointerMoved(point),
            Harness::button(point, true)
        ])
        .is_empty()
    );
    assert!(
        !h.output
            .observations
            .iter()
            .find(|o| o.target == Target::Ruler)
            .unwrap()
            .enabled
    );
    h.frame(vec![Harness::button(point, false)]);
    h.cap.seek = true;
    // A native focused text field remains the keyboard owner; literal Space
    // neither plays nor seeks. The production kit has no global keymap.
    h.ctx
        .run_ui(egui::RawInput::default(), |ui| {
            let r = ui.text_edit_singleline(&mut h.text);
            r.request_focus();
        })
        .textures_delta
        .clear();
    let space = Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    assert!(h.frame(vec![space]).is_empty());
    egui::Popup::open_id(&h.ctx, Id::new("external-popup"));
    assert!(h.frame(vec![Harness::button(point, true)]).is_empty());
    assert!(!h.timeline.is_scrubbing());
}

#[test]
fn explicit_seek_acknowledgment_only_resolves_the_matching_request() {
    let mut timeline = Timeline::default();
    let mut output = Output::default();
    timeline.begin_scrub(0.1, &mut output);
    assert_ne!(0.1_f64, f64::from(0.1_f32));
    timeline.acknowledge_seek(0.2);
    assert_eq!(
        timeline.requested,
        Some(0.1),
        "An older/different acceptance cannot clear the pending seek"
    );
    timeline.acknowledge_seek(0.1);
    assert!(timeline.requested.is_none());
    assert!(
        timeline.is_scrubbing(),
        "Acceptance does not end the gesture"
    );
}

fn marker_on(h: &Harness, track: u64) -> Rect {
    h.output
        .observations
        .iter()
        .find(|observation| match observation.target {
            Target::Key { track: id, .. } | Target::Cluster { track: id, .. } => {
                id == TrackId(track)
            }
            _ => false,
        })
        .unwrap()
        .rect
}

fn box_points(h: &Harness) -> (Pos2, Pos2) {
    let first = marker_on(h, 0);
    let third = marker_on(h, 2);
    (
        egui::pos2(first.right() + 18.0, first.top() - 3.0),
        egui::pos2(third.left() - 3.0, third.bottom() + 3.0),
    )
}

fn start_box(h: &mut Harness, start: Pos2, shift: bool) {
    let mut button = Harness::button(start, true);
    if let Event::PointerButton { modifiers, .. } = &mut button {
        modifiers.shift = shift;
    }
    assert!(h.frame(vec![Event::PointerMoved(start), button]).is_empty());
    assert!(h.timeline.has_pointer_gesture());
    assert!(!h.timeline.is_marquee_active());
    assert!(!h.timeline.is_scrubbing());
}

#[test]
fn empty_key_space_click_seeks_only_on_release_but_box_select_never_seeks() {
    let mut h = Harness::new();
    h.retry = false;
    h.synchronous = Some(accept_seek);
    h.settle();
    let point = egui::pos2(h.ruler().right() - 25.0, marker_on(&h, 0).center().y);
    let before = h.host.accepted_time;
    start_box(&mut h, point, false);
    assert_eq!(h.host.accepted_time, before);
    assert!(
        h.frame(vec![Event::PointerMoved(point + Vec2::X * 2.0)])
            .is_empty()
    );
    assert!(!h.timeline.is_marquee_active());
    assert!(matches!(
        h.frame(vec![Harness::button(point + Vec2::X * 2.0, false)])
            .as_slice(),
        [Request::Seek { .. }]
    ));
    assert_ne!(h.host.accepted_time, before);
    assert!(!h.timeline.has_pointer_gesture());

    let (start, end) = box_points(&h);
    let before = h.host.accepted_time;
    start_box(&mut h, start, false);
    assert!(h.frame(vec![Event::PointerMoved(end)]).is_empty());
    assert!(h.timeline.is_marquee_active());
    assert!(
        h.timeline.selection().is_none(),
        "Selection waits for release"
    );
    assert!(h.frame(vec![Harness::button(end, false)]).is_empty());
    assert_eq!(h.host.accepted_time, before);
    assert_eq!(h.timeline.selection().unwrap().keys.len(), 6);
    assert!(!h.timeline.has_pointer_gesture());
}

#[test]
fn marquee_intersection_selects_underlying_cluster_ids_across_tracks_and_adds_with_shift() {
    let mut h = Harness::new();
    h.retry = false;
    h.settle();
    // A marker edge overlap is sufficient; the capsule need not be contained.
    let first = marker_on(&h, 0);
    let second = marker_on(&h, 1);
    let start = egui::pos2(first.right() + 10.0, first.top() - 2.0);
    let end = egui::pos2(first.right() - 1.0, second.bottom() + 2.0);
    start_box(&mut h, start, false);
    // Keep an unrelated layout retry on the release: acceptance is once-only.
    h.retry = true;
    assert!(
        h.frame(vec![Event::PointerMoved(end), Harness::button(end, false)])
            .is_empty()
    );
    h.retry = false;
    assert_eq!(
        h.timeline.selection().unwrap().keys,
        [
            KeyRef {
                track: TrackId(0),
                key: KeyId(1)
            },
            KeyRef {
                track: TrackId(0),
                key: KeyId(2)
            },
            KeyRef {
                track: TrackId(1),
                key: KeyId(1)
            },
            KeyRef {
                track: TrackId(1),
                key: KeyId(2)
            },
        ]
    );
    assert_eq!(h.timeline.inspected_keys(&h.data).count(), 4);
    let third = marker_on(&h, 2);
    start_box(&mut h, third.right_top() + egui::vec2(10.0, -2.0), true);
    let end = third.left_bottom() + egui::vec2(-2.0, 2.0);
    assert!(
        h.frame(vec![Event::PointerMoved(end), Harness::button(end, false)])
            .is_empty()
    );
    assert_eq!(h.timeline.selection().unwrap().keys.len(), 6);
    assert_eq!(h.timeline.inspected_keys(&h.data).count(), 6);
    // Revision reconciliation uses both identities, not the coincident KeyId.
    h.data.revision += 1;
    h.data.tracks[0].keys.remove(0);
    h.frame(vec![]);
    assert_eq!(h.timeline.selection().unwrap().keys.len(), 5);
    assert!(h.timeline.selection().unwrap().keys.contains(&KeyRef {
        track: TrackId(1),
        key: KeyId(1),
    }));
}

#[test]
fn marquee_holds_navigation_stable_and_finishes_when_released_outside() {
    let mut h = Harness::new();
    h.retry = false;
    h.settle();
    let (start, end) = box_points(&h);
    let range = h.timeline.visible_range();
    let scroll = h.timeline.scroll;
    start_box(&mut h, start, false);
    for step in 1..=12 {
        let point = start.lerp(end, step as f32 / 12.0);
        assert!(
            h.frame(vec![
                Event::PointerMoved(point),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 12.0),
                    modifiers: egui::Modifiers::COMMAND,
                    phase: egui::TouchPhase::Move,
                },
                Event::Zoom(1.1)
            ])
            .is_empty()
        );
        assert_eq!(h.timeline.visible_range(), range);
        assert_eq!(h.timeline.scroll, scroll);
        assert_eq!(h.passes, 1);
        assert!(h.discard_reasons.is_empty());
        assert!(!h.painted_text().contains("PERF WARNING"));
        assert!(h.timeline.selection().is_none());
    }
    let end = egui::pos2(h.ruler().left() - 20.0, end.y);
    assert!(h.frame(vec![Harness::button(end, false)]).is_empty());
    assert_eq!(h.timeline.selection().unwrap().keys.len(), 6);
    assert!(!h.timeline.has_pointer_gesture());
    assert_eq!(h.parent_scroll, Vec2::ZERO);
}

#[test]
fn marquee_cancel_paths_preserve_selection_and_discard_pending_seek() {
    for reason in [
        CancelReason::Escape,
        CancelReason::PointerGone,
        CancelReason::FocusLost,
        CancelReason::InputOwner,
        CancelReason::Resize,
        CancelReason::DataChanged,
        CancelReason::Unavailable,
    ] {
        for move_pointer in [false, true] {
            let mut h = Harness::new();
            h.retry = false;
            h.settle();
            let selected = marker_on(&h, 4).center();
            h.frame(vec![
                Event::PointerMoved(selected),
                Harness::button(selected, true),
            ]);
            h.frame(vec![Harness::button(selected, false)]);
            let selection = h.timeline.selection().cloned();
            let time = h.host.accepted_time;
            let (start, end) = box_points(&h);
            start_box(&mut h, start, false);
            if move_pointer {
                h.frame(vec![Event::PointerMoved(end)]);
                assert!(h.timeline.is_marquee_active());
            }
            let events = match reason {
                CancelReason::Escape => vec![Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                CancelReason::PointerGone => vec![Event::PointerGone],
                CancelReason::FocusLost => vec![Event::WindowFocused(false)],
                CancelReason::InputOwner => {
                    egui::Popup::open_id(&h.ctx, Id::new("marquee-owner"));
                    vec![]
                }
                CancelReason::Resize => {
                    h.size.x -= 40.0;
                    vec![]
                }
                CancelReason::DataChanged => {
                    h.data.revision += 1;
                    vec![]
                }
                CancelReason::Unavailable => {
                    h.host.available = false;
                    vec![]
                }
            };
            assert!(h.frame(events).is_empty(), "{reason:?}");
            assert!(!h.timeline.has_pointer_gesture(), "{reason:?}");
            assert!(
                h.frame(vec![Harness::button(end, false)]).is_empty(),
                "Cancelled release must not seek: {reason:?}"
            );
            assert_eq!(h.timeline.selection(), selection.as_ref(), "{reason:?}");
            assert_eq!(h.host.accepted_time, time);
        }
    }
}

#[test]
fn empty_marquee_replaces_or_retains_selection_and_seek_capability_does_not_disable_inspection() {
    let mut h = Harness::new();
    h.retry = false;
    h.settle();
    let marker = marker_on(&h, 0).center();
    h.frame(vec![
        Event::PointerMoved(marker),
        Harness::button(marker, true),
    ]);
    h.frame(vec![Harness::button(marker, false)]);
    h.cap.seek = false;
    let start = egui::pos2(h.ruler().right() - 30.0, marker.y);
    let end = start + egui::vec2(15.0, 20.0);
    start_box(&mut h, start, true);
    assert!(
        h.frame(vec![Event::PointerMoved(end), Harness::button(end, false)])
            .is_empty()
    );
    assert_eq!(h.timeline.selection().unwrap().keys.len(), 2);
    start_box(&mut h, start, false);
    assert!(
        h.frame(vec![Event::PointerMoved(end), Harness::button(end, false)])
            .is_empty()
    );
    assert!(h.timeline.selection().is_none());
    let (start, end) = box_points(&h);
    start_box(&mut h, start, false);
    assert!(
        h.frame(vec![Event::PointerMoved(end), Harness::button(end, false)])
            .is_empty()
    );
    assert_eq!(h.timeline.selection().unwrap().keys.len(), 6);
}

#[test]
fn inspection_capability_revocation_cancels_pending_and_active_marquee_without_leaking_release() {
    for drag in [false, true] {
        let mut h = Harness::new();
        h.retry = false;
        h.settle();
        let marker = marker_on(&h, 4).center();
        h.frame(vec![
            Event::PointerMoved(marker),
            Harness::button(marker, true),
        ]);
        h.frame(vec![Harness::button(marker, false)]);
        let selection = h.timeline.selection().cloned();
        let (start, end) = box_points(&h);
        start_box(&mut h, start, false);
        if drag {
            h.frame(vec![Event::PointerMoved(end)]);
            assert!(h.timeline.is_marquee_active());
        }
        h.cap.inspect = false;
        assert!(h.frame(vec![]).is_empty());
        assert!(!h.timeline.has_pointer_gesture());
        assert!(h.frame(vec![Harness::button(end, false)]).is_empty());
        assert_eq!(h.timeline.selection(), selection.as_ref());
        let other = marker_on(&h, 1).center();
        assert!(
            h.frame(vec![
                Event::PointerMoved(other),
                Harness::button(other, true)
            ])
            .is_empty()
        );
        assert!(h.frame(vec![Harness::button(other, false)]).is_empty());
        assert_eq!(h.timeline.selection(), selection.as_ref());
        assert!(
            h.output
                .observations
                .iter()
                .filter(|o| matches!(o.target, Target::Key { .. } | Target::Cluster { .. }))
                .all(|o| !o.enabled)
        );
        // Seeking permission is separate, so ruler scrubbing still works.
        let (_, gesture) = h.begin();
        assert!(h.timeline.is_scrubbing());
        assert!(
            matches!(h.timeline.cancel(CancelReason::Escape), Some(Request::ScrubCancel { gesture: id, .. }) if id == gesture)
        );
    }
}

#[test]
fn degenerate_marquee_has_no_area_and_cannot_select_unpainted_hits() {
    for vertical in [false, true] {
        let mut h = Harness::new();
        h.retry = false;
        h.host.accepted_time = 7.0;
        h.settle();
        let marker = marker_on(&h, 1);
        let (start, end) = if vertical {
            (
                marker.center_top() - Vec2::Y * 4.0,
                marker.center_bottom() + Vec2::Y * 4.0,
            )
        } else {
            (
                marker.right_center() + Vec2::X * 15.0,
                marker.left_center() - Vec2::X * 15.0,
            )
        };
        start_box(&mut h, start, false);
        assert!(h.frame(vec![Event::PointerMoved(end)]).is_empty());
        assert!(h.timeline.is_marquee_active());
        assert!(h.frame(vec![Harness::button(end, false)]).is_empty());
        assert!(h.timeline.selection().is_none());
    }
}

#[test]
fn marquee_suppresses_marker_hover_without_erasing_accepted_selection() {
    let mut h = Harness::new();
    h.retry = false;
    for track in &mut h.data.tracks {
        track.keys.truncate(1);
    }
    h.settle();
    let selected = marker_on(&h, 4).center();
    h.frame(vec![
        Event::PointerMoved(selected),
        Harness::button(selected, true),
    ]);
    h.frame(vec![Harness::button(selected, false)]);
    let selection = h.timeline.selection().cloned();
    let (start, _) = box_points(&h);
    start_box(&mut h, start, false);
    let hover = marker_on(&h, 2).center();
    h.frame(vec![Event::PointerMoved(hover)]);
    assert!(h.timeline.is_marquee_active());
    assert_eq!(h.timeline.selection(), selection.as_ref());
    let fill = h
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Path(path)
                if path.closed
                    && path.points.len() == 4
                    && (path.points[0] + Vec2::Y * (MARKER * 0.5)).distance(hover) < 0.01 =>
            {
                Some(path.fill)
            }
            _ => None,
        })
        .expect("Hovered single-key diamond remains painted");
    assert_eq!(
        fill,
        h.ctx.style_of(h.ctx.theme()).visuals.weak_text_color()
    );
}

#[test]
fn dense_marquee_uses_visible_clusters_without_losing_underlying_identities() {
    let mut h = Harness::new();
    h.retry = false;
    for track in &mut h.data.tracks {
        track.keys = (0..1000)
            .map(|key| Key {
                id: KeyId(key),
                time: 0.125,
                metadata: None,
            })
            .collect();
    }
    h.settle();
    assert_eq!(h.output.metrics.total_keys, 100_000);
    assert!(h.output.metrics.rows < h.output.metrics.total_tracks);
    assert_eq!(
        h.output.metrics.keys_considered,
        h.output.metrics.rows * 1000,
        "Only visible rows contribute candidate keys"
    );
    assert!(
        h.output.metrics.markers > 0 && h.output.metrics.markers <= h.output.metrics.rows,
        "At most one aggregate marker per row; boundary rows can clip the marker entirely"
    );
    let (start, end) = box_points(&h);
    start_box(&mut h, start, false);
    assert!(h.frame(vec![Event::PointerMoved(end)]).is_empty());
    assert!(!h.output.metrics.prepared);
    assert!(h.frame(vec![Harness::button(end, false)]).is_empty());
    let selected = &h.timeline.selection().unwrap().keys;
    assert_eq!(selected.len(), 3000);
    for track in 0..3 {
        for key in 0..1000 {
            assert!(
                selected
                    .binary_search(&KeyRef {
                        track: TrackId(track),
                        key: KeyId(key)
                    })
                    .is_ok()
            );
        }
    }
    assert_eq!(h.timeline.inspected_keys(&h.data).count(), 3000);
}
