//! Read-only time presentation. Hosts own evaluation, clocks, and accepted state.
pub(crate) mod data;
mod navigation;
mod paint;
#[cfg(test)]
mod tests;
pub(crate) mod transport;

use crate::{input::pointer_policy::crossed_drag_threshold, theme, ui::marquee};
use data::Prepared;
pub(crate) use data::{
    CancelReason, Capabilities, Data, HostState, Key, KeyId, Request, TimeRange, Track, TrackId,
};
use egui::{Event, Id, PointerButton, Pos2, Rect, Ui, Vec2};
use std::{collections::BTreeSet, ops::Range};

const ROW: f32 = theme::size::STEP_6;
const HEADER: f32 = theme::size::STEP_8;
const MARKER: f32 = theme::size::STEP_2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Target {
    Canvas,
    Ruler,
    Transport(transport::TransportControl),
    Row(TrackId),
    Key {
        track: TrackId,
        key: KeyId,
    },
    Cluster {
        track: TrackId,
        first: KeyId,
        last: KeyId,
    },
}
impl Target {
    pub(crate) fn name(&self) -> String {
        match self {
            Self::Canvas => "timeline-canvas".into(),
            Self::Ruler => "timeline-ruler".into(),
            Self::Row(id) => format!("timeline-row-{}", id.0),
            Self::Key { track, key } => format!("timeline-key-{}-{}", track.0, key.0),
            Self::Cluster { track, first, last } => {
                format!("timeline-cluster-{}-{}-{}", track.0, first.0, last.0)
            }
            Self::Transport(control) => format!(
                "timeline-{}",
                match control {
                    transport::TransportControl::PlayPause => "playpause",
                    transport::TransportControl::Loop => "loop",
                    transport::TransportControl::Speed => "speed",
                    transport::TransportControl::Time => "time",
                    transport::TransportControl::Fit => "fit",
                }
            ),
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Observation {
    pub target: Target,
    pub id: Id,
    pub rect: Rect,
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Metrics {
    pub rows: usize,
    pub keys_considered: usize,
    pub markers: usize,
    pub prepared: bool,
    pub total_tracks: usize,
    pub total_keys: usize,
}
#[derive(Default)]
pub(crate) struct Output {
    pub requests: Vec<Request>,
    pub observations: Vec<Observation>,
    pub metrics: Metrics,
}

/// One pass of input/layout, awaiting the host's synchronous response. Consume
/// requests once, then finish in the same Ui and pass with the accepted state.
/// The immutable snapshot stays borrowed; host evaluation cannot replace the
/// data underlying this frame's indexes and visible geometry.
#[must_use = "Finish the prepared timeline in the same egui pass"]
pub(crate) struct PreparedFrame<'a> {
    data: &'a Data,
    id: Id,
    ui: Id,
    pass: u64,
    output: Output,
    geometry: Option<FrameGeometry>,
}

impl PreparedFrame<'_> {
    pub(crate) fn requests(&self) -> &[Request] {
        &self.output.requests
    }
}

struct FrameGeometry {
    rect: Rect,
    ruler: Rect,
    keys: Rect,
    rows: Vec<RowLayout>,
    markers: Vec<Marker>,
}
/// Key IDs are only unique within a track. Selection never loses that qualifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct KeyRef {
    pub track: TrackId,
    pub key: KeyId,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub keys: Vec<KeyRef>,
}
/// Timeline-specific click/box-select arbitration. This is not shared with
/// viewport tools: hosts differ in what empty space and a drag mean.
struct BoxSelect {
    start: Pos2,
    current: Pos2,
    dragging: bool,
    additive: bool,
}
impl BoxSelect {
    fn update(&mut self, point: Pos2) {
        self.current = point;
        self.dragging |= crossed_drag_threshold(self.start, point);
    }
}
struct Scrub {
    id: u64,
    time: f64,
}
struct Marker {
    track: usize,
    indices: Range<usize>,
    center: Pos2,
    rect: Rect,
}
struct RowLayout {
    track: usize,
    depth: usize,
    center: f32,
    label: Rect,
    keys: Rect,
}

#[derive(Default)]
pub(crate) struct Timeline {
    identity: Option<Id>,
    source: Option<(usize, u64)>,
    prepared: Option<Prepared>,
    error: Option<String>,
    rows: Vec<(usize, usize)>,
    collapsed: BTreeSet<TrackId>,
    visible: Option<TimeRange>,
    scroll: f32,
    selected: Option<Selection>,
    selected_keys: BTreeSet<KeyRef>,
    selected_tracks: BTreeSet<TrackId>,
    box_select: Option<BoxSelect>,
    scrub: Option<Scrub>,
    gesture_counter: u64,
    requested: Option<f64>,
    pointer: Option<Pos2>,
    last_rect: Option<(Rect, Rect)>,
    frame: Option<u64>,
}
impl Timeline {
    pub(crate) fn visible_range(&self) -> Option<TimeRange> {
        self.visible
    }
    pub(crate) fn selection(&self) -> Option<&Selection> {
        self.selected.as_ref()
    }
    pub(crate) fn is_scrubbing(&self) -> bool {
        self.scrub.is_some()
    }
    pub(crate) fn is_marquee_active(&self) -> bool {
        self.box_select
            .as_ref()
            .is_some_and(|gesture| gesture.dragging)
    }
    pub(crate) fn has_pointer_gesture(&self) -> bool {
        self.scrub.is_some() || self.box_select.is_some()
    }
    /// A synchronous host may accept at its source's numeric precision. Clear
    /// only that pending request after publication; accepted time still comes
    /// from HostState. Rejected or additionally clamped seeks stay unresolved.
    pub(crate) fn acknowledge_seek(&mut self, requested_time: f64) {
        if self.requested == Some(requested_time) {
            self.requested = None;
        }
    }
    pub(crate) fn cancel(&mut self, reason: CancelReason) -> Option<Request> {
        // Selection is accepted only on release, so cancellation leaves it
        // intact and clears the pending click as well as the visible marquee.
        self.box_select = None;
        self.scrub.take().map(|scrub| {
            self.requested = None;
            Request::ScrubCancel {
                gesture: scrub.id,
                reason,
            }
        })
    }
    pub(crate) fn fit_to_content(&mut self, data: &Data) {
        self.visible = TimeRange::new(data.content.start, data.content.end)
            .ok()
            .map(TimeRange::fit_visible);
    }
    /// Read only selected identities through the current revision's indexes.
    /// Hosts can present metadata without searching/cloning the whole snapshot.
    pub(crate) fn inspected_keys<'a>(&'a self, data: &'a Data) -> impl Iterator<Item = &'a Key> {
        self.selected.iter().flat_map(move |selection| {
            selection.keys.iter().filter_map(move |key| {
                let prepared = self.prepared.as_ref()?;
                if self.source != Some((std::ptr::from_ref(data) as usize, data.revision)) {
                    return None;
                }
                let track = prepared.track_index(key.track)?;
                data.tracks
                    .get(track)?
                    .keys
                    .get(prepared.key_index(key.track, key.key)?)
            })
        })
    }

    fn reconcile(&mut self, data: &Data) {
        let Some(prepared) = &self.prepared else {
            self.selected = None;
            return;
        };
        self.collapsed
            .retain(|track| prepared.track_index(*track).is_some());
        if let Some(selection) = &mut self.selected {
            selection
                .keys
                .retain(|key| prepared.key_index(key.track, key.key).is_some());
            if selection.keys.is_empty() {
                self.selected = None;
            }
        }
        self.selected_keys = self
            .selected
            .as_ref()
            .map(|selection| selection.keys.iter().copied().collect())
            .unwrap_or_default();
        self.selected_tracks = self.selected_keys.iter().map(|key| key.track).collect();
        self.rows = prepared.visible_rows(data, &self.collapsed);
    }
    fn layouts(
        &self,
        rect: Rect,
        labels: f32,
        header: f32,
        data: &Data,
    ) -> (Vec<RowLayout>, Vec<Marker>, Metrics) {
        let mut metrics = Metrics {
            total_tracks: data.tracks.len(),
            total_keys: self.prepared.as_ref().map_or(0, |p| p.total_keys),
            ..Default::default()
        };
        let mut rows = Vec::new();
        let mut markers = Vec::new();
        let (Some(prepared), Some(range)) = (&self.prepared, self.visible) else {
            return (rows, markers, metrics);
        };
        let body = Rect::from_min_max(rect.min + Vec2::Y * header, rect.max);
        let first = (self.scroll / ROW).floor().max(0.0) as usize;
        let count = (body.height().max(0.0) / ROW).ceil() as usize + 1;
        for (offset, &(track, depth)) in self.rows.iter().skip(first).take(count).enumerate() {
            let y = body.top() + (first + offset) as f32 * ROW - self.scroll;
            let full =
                Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(rect.width(), ROW));
            let label =
                Rect::from_min_max(full.min, egui::pos2(rect.left() + labels, full.bottom()))
                    .intersect(body);
            let keys =
                Rect::from_min_max(egui::pos2(rect.left() + labels, y), full.max).intersect(body);
            if !keys.is_positive() || !label.is_positive() {
                continue;
            }
            rows.push(RowLayout {
                track,
                depth,
                center: full.center().y,
                label,
                keys,
            });
            metrics.rows += 1;
            let ordered = prepared.key_indices(track);
            let track_keys = &data.tracks[track].keys;
            let visible_keys = prepared.key_range(track, range);
            let start = visible_keys.start;
            let end = visible_keys.end;
            metrics.keys_considered += end - start;
            let mut cursor = start;
            while cursor < end {
                let x = navigation::x_at(
                    range,
                    keys.left(),
                    keys.width(),
                    track_keys[ordered[cursor]].time,
                );
                let mut next = cursor + 1;
                while next < end
                    && navigation::x_at(
                        range,
                        keys.left(),
                        keys.width(),
                        track_keys[ordered[next]].time,
                    ) - x
                        < MARKER + 3.0
                {
                    next += 1;
                }
                let right_x = navigation::x_at(
                    range,
                    keys.left(),
                    keys.width(),
                    track_keys[ordered[next - 1]].time,
                );
                let width = if next - cursor > 1 {
                    (right_x - x + 10.0).max(14.0)
                } else {
                    MARKER
                };
                let center = egui::pos2((x + right_x) * 0.5, full.center().y);
                let marker =
                    Rect::from_center_size(center, egui::vec2(width, MARKER + 4.0)).intersect(keys);
                if marker.is_positive() {
                    markers.push(Marker {
                        track,
                        indices: cursor..next,
                        center,
                        rect: marker,
                    });
                }
                cursor = next;
            }
        }
        metrics.markers = markers.len();
        (rows, markers, metrics)
    }
    fn select_markers<'a>(
        &mut self,
        markers: impl Iterator<Item = &'a Marker>,
        data: &Data,
        additive: bool,
    ) {
        if !additive {
            self.selected_keys.clear();
        }
        let prepared = self.prepared.as_ref().unwrap();
        for marker in markers {
            let track = &data.tracks[marker.track];
            self.selected_keys.extend(
                prepared.key_indices(marker.track)[marker.indices.clone()]
                    .iter()
                    .map(|&index| KeyRef {
                        track: track.id,
                        key: track.keys[index].id,
                    }),
            );
        }
        self.selected_tracks = self.selected_keys.iter().map(|key| key.track).collect();
        self.selected = (!self.selected_keys.is_empty()).then(|| Selection {
            keys: self.selected_keys.iter().copied().collect(),
        });
    }

    fn begin_scrub(&mut self, time: f64, out: &mut Output) {
        self.gesture_counter = self.gesture_counter.wrapping_add(1);
        self.scrub = Some(Scrub {
            id: self.gesture_counter,
            time,
        });
        self.requested = Some(time);
        out.requests.push(Request::ScrubBegin {
            gesture: self.gesture_counter,
            time,
        });
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Shared visible geometry and immutable presentation inputs"
    )]
    fn process(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        ruler: Rect,
        keys: Rect,
        rows: &[RowLayout],
        markers: &[Marker],
        data: &Data,
        host: &HostState,
        cap: &Capabilities,
        out: &mut Output,
    ) {
        let ctx = ui.ctx();
        let frame = ctx.cumulative_frame_nr();
        let focused = ctx.input(|i| i.focused);
        let owner = egui::Popup::is_any_open(ctx)
            || ctx.memory(|m| m.top_modal_layer().is_some())
            || ctx.text_edit_focused()
            || ctx.dragged_id().is_some_and(|drag| {
                !self
                    .identity
                    .is_some_and(|id| drag == id.with("canvas") || drag == id.with("ruler"))
            });
        // Transport lives in the left header. It keeps native pointer/wheel
        // ownership; only the body and time ruler are navigation surfaces.
        let on_surface = |p: Pos2| rect.contains(p) && (p.y >= keys.top() || ruler.contains(p));
        if self.frame == Some(frame) {
            if focused && !owner && self.pointer.is_some_and(on_surface) {
                ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            }
            return;
        }
        self.frame = Some(frame);
        let reason = if !focused {
            Some(CancelReason::FocusLost)
        } else if !host.valid()
            || (!cap.seek && self.scrub.is_some())
            || (!cap.inspect && self.box_select.is_some())
        {
            Some(CancelReason::Unavailable)
        } else if self.last_rect.is_some_and(|last| last != (rect, ruler)) {
            Some(CancelReason::Resize)
        } else if owner {
            Some(CancelReason::InputOwner)
        } else {
            None
        };
        self.last_rect = Some((rect, ruler));
        if let Some(reason) = reason
            && let Some(request) = self.cancel(reason)
        {
            out.requests.push(request);
        }
        let events = ctx.input(|i| i.events.clone());
        let mut used_wheel = false;
        let mut expand = None;
        for event in events {
            let range = self.visible.unwrap();
            match event {
                Event::PointerMoved(point) => {
                    self.pointer = Some(point);
                    if let Some(gesture) = &mut self.box_select {
                        gesture.update(point);
                    }
                    if let Some(scrub) = &mut self.scrub {
                        let time = navigation::time_at(
                            range,
                            keys.left(),
                            keys.width(),
                            point.x.clamp(keys.left(), keys.right()),
                        );
                        scrub.time = time;
                        self.requested = Some(time);
                        out.requests.push(Request::ScrubUpdate {
                            gesture: scrub.id,
                            time,
                        });
                    }
                }
                Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers,
                } => {
                    self.pointer = Some(pos);
                    if !pressed {
                        if let Some(mut gesture) = self.box_select.take() {
                            gesture.update(pos);
                            if gesture.dragging {
                                let selection_rect =
                                    marquee::rectangle(gesture.start, gesture.current, keys);
                                self.select_markers(
                                    markers.iter().filter(|marker| {
                                        selection_rect.is_positive()
                                            && marker.rect.intersects(selection_rect)
                                    }),
                                    data,
                                    gesture.additive,
                                );
                            } else if host.valid() && cap.seek && keys.contains(pos) {
                                let time =
                                    navigation::time_at(range, keys.left(), keys.width(), pos.x);
                                self.requested = Some(time);
                                out.requests.push(Request::Seek { time });
                            }
                        }
                        if let Some(scrub) = self.scrub.take() {
                            let time = navigation::time_at(
                                range,
                                keys.left(),
                                keys.width(),
                                pos.x.clamp(keys.left(), keys.right()),
                            );
                            self.requested = Some(time);
                            out.requests.push(Request::ScrubEnd {
                                gesture: scrub.id,
                                time,
                            });
                        }
                    } else if focused
                        && reason.is_none()
                        && !owner
                        && on_surface(pos)
                        && ctx.layer_id_at(pos) == Some(ui.layer_id())
                    {
                        if ruler.contains(pos) {
                            if host.valid() && cap.seek && self.scrub.is_none() {
                                self.begin_scrub(
                                    navigation::time_at(range, keys.left(), keys.width(), pos.x),
                                    out,
                                );
                            }
                        } else if let Some(marker) =
                            markers.iter().rev().find(|m| m.rect.contains(pos))
                        {
                            if cap.inspect {
                                self.select_markers(std::iter::once(marker), data, modifiers.shift);
                            }
                        } else if keys.contains(pos)
                            && (navigation::x_at(
                                range,
                                keys.left(),
                                keys.width(),
                                host.accepted_time,
                            ) - pos.x)
                                .abs()
                                < 5.0
                        {
                            if host.valid() && cap.seek && self.scrub.is_none() {
                                self.begin_scrub(
                                    navigation::time_at(range, keys.left(), keys.width(), pos.x),
                                    out,
                                );
                            }
                        } else if let Some(row) = rows.iter().find(|r| r.label.contains(pos)) {
                            if self.prepared.as_ref().unwrap().has_children(row.track) {
                                expand = Some(data.tracks[row.track].id);
                            }
                        } else if keys.contains(pos) && host.valid() && cap.inspect {
                            // Defer empty-space seeking until release. Once the
                            // threshold is crossed, this press only selects keys.
                            self.box_select = Some(BoxSelect {
                                start: pos,
                                current: pos,
                                dragging: false,
                                additive: modifiers.shift,
                            });
                        }
                    }
                }
                Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } if self.has_pointer_gesture() => {
                    if let Some(request) = self.cancel(CancelReason::Escape) {
                        out.requests.push(request);
                    }
                    ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                    });
                }
                Event::PointerGone => {
                    if let Some(request) = self.cancel(CancelReason::PointerGone) {
                        out.requests.push(request);
                    }
                    self.pointer = None;
                }
                Event::WindowFocused(false) => {
                    if let Some(request) = self.cancel(CancelReason::FocusLost) {
                        out.requests.push(request);
                    }
                }
                Event::MouseWheel {
                    delta,
                    unit,
                    modifiers,
                    ..
                } if focused && !owner && self.pointer.is_some_and(on_surface) => {
                    if self.box_select.is_some() {
                        // Keep the hit-test projection fixed until release. No
                        // auto-scroll or time navigation during box selection.
                        used_wheel = true;
                        continue;
                    }
                    let multiplier = match unit {
                        egui::MouseWheelUnit::Point => 1.0,
                        egui::MouseWheelUnit::Line => 24.0,
                        egui::MouseWheelUnit::Page => keys.width(),
                    };
                    let delta = delta * multiplier;
                    if modifiers.command || modifiers.ctrl {
                        let anchor = navigation::time_at(
                            self.visible.unwrap(),
                            keys.left(),
                            keys.width(),
                            self.pointer.unwrap().x,
                        );
                        self.visible = navigation::zoom(
                            self.visible.unwrap(),
                            anchor,
                            (f64::from(delta.y) * 0.01).exp(),
                        )
                        .or(self.visible);
                    } else if modifiers.shift || delta.x.abs() > delta.y.abs() {
                        let amount = if modifiers.shift { delta.y } else { delta.x };
                        self.visible = navigation::pan(
                            self.visible.unwrap(),
                            -f64::from(amount) / f64::from(keys.width())
                                * self.visible.unwrap().duration(),
                        )
                        .or(self.visible);
                    } else {
                        self.scroll = (self.scroll - delta.y)
                            .clamp(0.0, (self.rows.len() as f32 * ROW - keys.height()).max(0.0));
                    }
                    used_wheel = true;
                }
                Event::Zoom(factor)
                    if focused && !owner && self.pointer.is_some_and(on_surface) =>
                {
                    if self.box_select.is_some() {
                        used_wheel = true;
                        continue;
                    }
                    let anchor = navigation::time_at(
                        self.visible.unwrap(),
                        keys.left(),
                        keys.width(),
                        self.pointer.unwrap().x,
                    );
                    self.visible =
                        navigation::zoom(self.visible.unwrap(), anchor, f64::from(factor))
                            .or(self.visible);
                    used_wheel = true;
                }
                _ => {}
            }
        }
        if focused && !owner && self.pointer.is_some_and(on_surface) {
            // Consume smooth tails as well as the original events, on every
            // layout pass. Otherwise a parent's ScrollArea scrolls next frame.
            ctx.input_mut(|i| {
                i.smooth_scroll_delta = Vec2::ZERO;
                i.events
                    .retain(|e| !matches!(e, Event::MouseWheel { .. } | Event::Zoom(_)));
            });
        }
        if used_wheel {
            ctx.request_repaint();
        }
        if let Some(track) = expand {
            if !self.collapsed.remove(&track) {
                self.collapsed.insert(track);
            }
            self.rows = self
                .prepared
                .as_ref()
                .unwrap()
                .visible_rows(data, &self.collapsed);
            self.scroll = self
                .scroll
                .min((self.rows.len() as f32 * ROW - keys.height()).max(0.0));
            ctx.request_repaint();
        }
        if !out.requests.is_empty() {
            ctx.request_repaint();
        }
    }
    /// Convenience for a fixed snapshot or an asynchronous host. Requests are
    /// returned after painting; their acceptance is displayed on a later frame.
    /// Synchronous hosts use `prepare`/`finish` instead of requesting another
    /// egui layout pass merely to repaint their newly accepted playback state.
    #[cfg(test)]
    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        id: Id,
        data: &Data,
        host: &HostState,
        cap: &Capabilities,
    ) -> Output {
        let frame = self.prepare(ui, id, data, host, cap);
        self.finish(ui, frame, host, cap)
    }

    /// Lay out controls and produce ordered requests before painting playback
    /// feedback. Apply/reject every request, then call `finish` with the latest
    /// accepted host state. Native transport widgets use this initial snapshot
    /// and refresh normally next frame; they are not redrawn with duplicate IDs.
    pub(crate) fn prepare<'a>(
        &mut self,
        ui: &mut Ui,
        id: Id,
        data: &'a Data,
        host: &HostState,
        cap: &Capabilities,
    ) -> PreparedFrame<'a> {
        let mut frame = PreparedFrame {
            data,
            id,
            ui: ui.id(),
            pass: ui.ctx().cumulative_pass_nr(),
            output: Output::default(),
            geometry: None,
        };
        let out = &mut frame.output;
        if self.requested == Some(host.accepted_time) {
            self.requested = None;
        }
        if self.identity.is_some_and(|old| old != id) {
            if let Some(request) = self.cancel(CancelReason::DataChanged) {
                out.requests.push(request);
            }
            *self = Self::default();
        }
        self.identity = Some(id);
        let source = (std::ptr::from_ref(data) as usize, data.revision);
        if self.source != Some(source) {
            if let Some(request) = self.cancel(CancelReason::DataChanged) {
                out.requests.push(request);
            }
            self.source = Some(source);
            match Prepared::new(data) {
                Ok(prepared) => {
                    self.prepared = Some(prepared);
                    self.error = None;
                    self.reconcile(data);
                }
                Err(error) => {
                    self.prepared = None;
                    self.rows.clear();
                    self.selected = None;
                    self.error = Some(error);
                }
            }
            if self.visible.is_none() {
                self.fit_to_content(data);
            }
            out.metrics.prepared = true;
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
            return frame;
        }
        let width = ui.available_width().max(32.0);
        let height = ui.available_height().max(80.0) - 28.0;
        let proposed = Rect::from_min_size(ui.cursor().min, egui::vec2(width, height));
        let labels = (width * 0.4).clamp(144.0, 288.0).min(width - 16.0);
        // Reserve the background before native widgets so painting the body
        // later cannot cover the embedded header. The header's measured height
        // accommodates wrapped controls in narrow instances.
        let background = ui.painter().add(egui::Shape::Noop);
        let mut header_ui = ui.new_child(egui::UiBuilder::new().id(id.with("header")).max_rect(
            Rect::from_min_max(
                proposed.min + egui::vec2(8.0, 6.0),
                egui::pos2(proposed.left() + labels - 8.0, proposed.bottom()),
            ),
        ));
        let header_clip = Rect::from_min_max(
            proposed.min,
            egui::pos2(proposed.left() + labels, proposed.bottom()),
        )
        .intersect(ui.clip_rect());
        header_ui.set_clip_rect(header_clip);
        let transport = transport::show(&mut header_ui, id.with("transport"), host, cap);
        let header = (header_ui.min_rect().bottom() - proposed.top() + 6.0).max(HEADER);
        for observed in transport.observations {
            let bounds = observed.rect.intersect(header_clip);
            if !bounds.is_positive() {
                continue;
            }
            out.observations.push(Observation {
                target: Target::Transport(observed.control),
                id: observed.id,
                rect: bounds,
                enabled: observed.enabled,
            });
        }
        out.requests.extend(transport.requests);
        for request in &out.requests {
            if let Request::Seek { time } = request {
                self.requested = Some(*time);
            }
        }
        if transport.fit {
            self.fit_to_content(data);
        }
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
        let rect = rect.intersect(ui.clip_rect());
        ui.painter().set(
            background,
            egui::Shape::rect_filled(rect, theme::radius::SM, ui.visuals().panel_fill),
        );
        if !rect.is_positive() {
            if let Some(request) = self.cancel(CancelReason::Resize) {
                out.requests.push(request);
            }
            return frame;
        }
        if rect.width() < 32.0 || rect.height() <= header {
            if let Some(request) = self.cancel(CancelReason::Resize) {
                out.requests.push(request);
            }
            ui.painter().with_clip_rect(rect).text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Timeline needs more room",
                egui::FontId::proportional(theme::text::SMALL_UI_11),
                ui.visuals().weak_text_color(),
            );
            return frame;
        }
        let keys = Rect::from_min_max(rect.min + egui::vec2(labels, header), rect.max);
        let ruler = Rect::from_min_max(
            rect.min + Vec2::X * labels,
            egui::pos2(rect.right(), rect.top() + header),
        );
        let (rows, markers, metrics) = self.layouts(rect, labels, header, data);
        let prepared = out.metrics.prepared;
        out.metrics = metrics;
        out.metrics.prepared = prepared;
        let before = (self.visible, self.scroll, self.rows.len());
        self.process(ui, rect, ruler, keys, &rows, &markers, data, host, cap, out);
        let (rows, markers, metrics) = if before != (self.visible, self.scroll, self.rows.len()) {
            self.layouts(rect, labels, header, data)
        } else {
            (rows, markers, metrics)
        };
        out.metrics = Metrics {
            prepared,
            ..metrics
        };
        frame.geometry = Some(FrameGeometry {
            rect,
            ruler,
            keys,
            rows,
            markers,
        });
        frame
    }

    /// Paint custom timeline content from the accepted state after synchronous
    /// host evaluation. A rejected or additionally clamped seek keeps pending
    /// feedback; the playhead always reflects the host's actual accepted time.
    /// Output retains the requests for inspection, but split hosts must not
    /// deliver them again after already consuming `PreparedFrame::requests`.
    pub(crate) fn finish(
        &mut self,
        ui: &mut Ui,
        frame: PreparedFrame<'_>,
        host: &HostState,
        cap: &Capabilities,
    ) -> Output {
        debug_assert_eq!(frame.ui, ui.id(), "Finish in the original timeline Ui");
        debug_assert_eq!(
            frame.pass,
            ui.ctx().cumulative_pass_nr(),
            "Finish in the original egui pass"
        );
        debug_assert_eq!(self.identity, Some(frame.id));
        debug_assert_eq!(
            self.source,
            Some((std::ptr::from_ref(frame.data) as usize, frame.data.revision))
        );
        let mut out = frame.output;
        let Some(FrameGeometry {
            rect,
            ruler,
            keys,
            rows,
            markers,
        }) = frame.geometry
        else {
            return out;
        };
        if self.requested == Some(host.accepted_time) {
            self.requested = None;
        }
        self.paint(
            ui, frame.id, rect, ruler, keys, &rows, &markers, frame.data, host, cap, &mut out,
        );
        if !host.valid() {
            ui.weak("Playback unavailable");
        } else if let Some(time) = self.requested.filter(|time| *time != host.accepted_time) {
            ui.weak(format!(
                "Requested {time:.3} s · accepted {:.3} s",
                host.accepted_time
            ));
        } else if let Some(selection) = &self.selected {
            if self.selected_tracks.len() > 1 {
                ui.weak(format!(
                    "Inspection: {} key(s) · {} tracks",
                    selection.keys.len(),
                    self.selected_tracks.len()
                ));
            } else {
                ui.weak(format!(
                    "Inspection: {} key(s) · track {}",
                    selection.keys.len(),
                    selection.keys[0].track.0
                ));
            }
        } else if !cap.seek && !cap.inspect {
            ui.weak("Seeking and inspection disabled");
        } else if !cap.seek {
            ui.weak("Seeking disabled · keys can be inspected");
        } else if !cap.inspect {
            ui.weak("Key inspection disabled");
        } else {
            ui.add(egui::Label::new(egui::RichText::new("Read-only · wheel: rows · Shift+wheel: pan · Ctrl/Cmd+wheel or pinch: zoom").weak()).truncate());
        }
        out
    }
}
