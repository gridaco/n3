# Timeline component contract

Use this reference for the read-only Timeline UI Kit in `src/ui/timeline/` and its
synthetic consumers in `src/workbench/timeline.rs`. This is an internal
presentation contract, not an animation authoring schema or public SDK.

## Ownership and input data

| Owner     | Responsibilities                                                                                                                               |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Caller    | Immutable `Data` snapshot: revision, content range, track hierarchy, stable track/key IDs, labels, finite timestamps, optional inspection text |
| Host      | Accepted time, playback/loop/speed state, clock, evaluation, capabilities, request acceptance and effects                                      |
| Component | Visible time range, row scroll, expansion, hover, inspection selection, active scrub/box and preparation cache                                 |

`TrackId` is snapshot-wide; `KeyId` is unique within its track. Parent references
must resolve and form an acyclic hierarchy. Preparation validates these invariants,
indexes identities and sorts timestamp indices without modifying caller order or
timestamps. Invalid data displays an error. There is no document, scene, glTF,
skinning or animation evaluation dependency.

Keep the data at a stable address (for example `Arc<Data>`) and increment its
revision for any track, key, label, metadata or content change. Preparation is
cached by address and revision. A replacement address also invalidates it.
Unchanged data does not require validation, hierarchy traversal, sorting or whole
snapshot cloning each frame. Reusing a revision after an in-place mutation
violates the contract.

Replacement cancels an active scrub with `DataChanged`, retains the navigated
visible range, retains expansion and selected IDs that still exist, and removes
missing selection/expansion IDs. It does not silently fit the new content; call
`fit_to_content` when the host wants that. One `Timeline` state and stable caller
`egui::Id` belong to each instance. Reset creates fresh component/fixture state
while preserving that caller identity.

All times and ranges use finite `f64` seconds. Content and visible ranges are
separate. Negative/nonzero starts, fractional and irregular timestamps are valid;
no frame rate or time rebasing is implied. A zero-duration content range fits to a
positive visible span while its data remains a single instant. Extreme finite
ranges use representable padding. Navigation limits are numeric representability,
not invented clip duration or frame boundaries.

## Interaction and acceptance

- Click the key area background to emit `Seek` on release; crossing the drag
  threshold instead begins box selection. The timeline owns this arbitration.
  Press the ruler or bare playhead to begin a scrub. Actual key/cluster hits win
  over a playhead hit in the rows.
- A box selects intersecting displayed markers on release; Shift adds and
  Escape cancels without seeking or changing the previous selection. Clusters
  retain every underlying key. Selection uses `(TrackId, KeyId)` because key
  IDs are unique only within a track. The kit freezes its navigation during
  the box; it does not auto-scroll.
- `Capabilities.inspect` gates key/box inspection independently of seeking.
  The editor adapter disables both during an owned editor interaction.
  Revoking inspection cancels a pending click or box, including its late release.
- `ui::marquee` shares only stateless rectangle/clipping and painting with the
  viewport. Keep pointer intent, thresholds, commit/cancel and selected IDs in
  their existing hosts; these examples do not establish a universal gesture API.
- A scrub emits `ScrubBegin`, zero or more ordered `ScrubUpdate` requests, then
  exactly one `ScrubEnd` or `ScrubCancel`, sharing its gesture ID. Identical
  consecutive pointer updates are distinct events.
- Release outside ends at the visible range edge. Escape, focus loss, resize,
  snapshot replacement, lost seek capability, pointer loss or a competing input
  owner cancels. The host decides whether cancellation restores time, whether
  scrubbing pauses playback, and when/how to evaluate.
- Wheel scrolls rows; horizontal wheel or Shift+wheel pans time. Ctrl/Cmd+wheel
  and pinch zoom around the pointer. Fit changes presentation only. The child
  consumes wheel events and their smooth tails so an enclosing scroll area does
  not move. Text fields, modal/popups and other active drags retain ownership.
- There are no global key bindings, including Space. The host owns shortcut
  registration and routing if a later product integration needs it.

The solid playhead always represents `HostState.accepted_time`. Pending or
rejected requests never move it themselves. The status distinguishes the latest
requested time from accepted time; a host may accept, clamp, defer or reject.
Transport uses native egui controls and the same accepted host snapshot. Each
capability gates its operation; invalid/unavailable host state disables transport
and seeking. Inspection and presentation navigation do not evaluate anything.

**Resolve requests before accepted-state painting.** Call `prepare`, consume its
ordered requests on every UI pass, then call `finish` with the refreshed accepted
host state. The prepared frame borrows the immutable data snapshot and must finish
in the same Ui/pass. Keep that snapshot stable between phases; acknowledgement
and scrub cancellation may update component state. Requests retained in the final
output are for inspection, not a second delivery.

The component delivers effects once per logical egui frame and suppresses
repetitions on legitimate layout retries. Do not discard earlier-pass requests
or deduplicate equal request values. A deferred/rejecting host may return its
unchanged accepted state; the kit truthfully retains pending feedback. Native
transport widgets preserve normal egui text buffers and next-frame refresh.
Do not force a whole-workspace layout retry to paint continuous accepted values.

## Integration example

This is a host adapter sketch, using the existing private crate types. The fixture
host is an executable example with explicit replay time, immediate acceptance,
rejection and cancellation-baseline policy.

```rust
use crate::ui::timeline::{
    Capabilities, Data, HostState, Key, KeyId, TimeRange, Timeline, Track, TrackId,
};
use std::sync::Arc;

let tracks = Arc::new(Data {
    revision: 1,
    content: TimeRange::new(-0.5, 2.0)?,
    tracks: vec![Track {
        id: TrackId(10), parent: None, label: "Joint rotation".into(),
        keys: vec![Key {
            id: KeyId(20), time: 0.125,
            metadata: Some("Quaternion [0, 0, 0, 1]; linear interpolation".into()),
        }],
    }],
});
let mut timeline = Timeline::default();
let mut playback = HostState { accepted_time: -0.5, ..Default::default() };
let capabilities = Capabilities::default();

// Inside every egui UI pass; host.accept_request supplies the actual policy.
let frame = timeline.prepare(
    ui, egui::Id::new(("clip-timeline", host.instance_id)),
    &tracks, &playback, &capabilities,
);
for &request in frame.requests() {
    host.accept_request(request); // Evaluate or reject; no component clock.
}
playback = host.playback_snapshot();
let output = timeline.finish(ui, frame, &playback, &capabilities);
for key in timeline.inspected_keys(&tracks) {
    host.inspect_key(key.id, key.time, key.metadata.as_deref());
}
```

The imported-animation adapter maps transform, joint and morph-weight channels
into ordinary tracks and keys. Metadata is opaque text; quaternion inspection does
not manufacture Euler curves. Content is read-only: no key movement, insertion,
deletion, curves, tangents, arrangement, blending, recording or persistence.
Transport can also be composed independently through `transport::show`.

## Rendering, observations and limits

Transport and time ruler share a compact header. The transport uses native
widgets, unit suffixes and accessible field names. At narrow widths the controls
wrap and the shared header grows; none are hidden behind the ruler. Native body
and ruler input surfaces exclude transport, and control identities remain stable
through wrapping. Header geometry changes cancel an active scrub as a resize.
Selection softly highlights the full row beneath its grid and keys. Group labels,
muted children and light hierarchy guides distinguish nesting; keys remain
diamonds and the accepted playhead is a thin accent line with a header tab.

Rows are virtualized. Cached sorted time indexes binary-cull each visible row's
keys. Close markers aggregate into capsules; individual keys remain diamonds.
Cluster selection preserves all underlying IDs and the tooltip explicitly says
it is ambiguous. Zoom in for individual inspection; no summary claims to be an
actual key. Layout and hit testing share geometry, including clipping. Long row
labels elide to one line; hover shows the full label.

Live `Target` observations are instance-qualified by the host. Visible row,
key and cluster bounds are recorded; offscreen keys are not registered. Replay
must target those observations rather than stale screen coordinates.

The dense case has 1,000 tracks and 100,000 keys. `timeline_measure::measure`
records first snapshot preparation and 120 representative CPU navigation frames,
including the real workbench host/inspector, in the evidence manifest. It excludes
GPU rendering/readback, tessellation, encoding and fixture construction. Timings
are machine/profile observations with no thresholds. Clustering still visits the
visible keys; pathological single-row density is not constant time. Preparation
is synchronous on revision change and stores per-key indexes. No GPU performance,
physical trackpad or screen-reader acceptance is implied by these measurements.
Continuous acceptance must also retain one settled UI pass per gesture frame;
keep actual rendered-playhead and rejection-feedback assertions alongside pass
counts. A single forced retry remains useful for once-only delivery tests.

## Cases and validation

The explicit Rust workbench catalog owns case names. Timeline cases cover small
keys/transport, nested joint/quaternion/weights, dense narrow layouts, empty and
zero duration, two instances, box selection across tracks, rejected seeks and
live replacement controls.
Keep deterministic playback in the fixture host and advance it with Replay time.

Run `just workbench`, `just cargo test timeline -- --test-threads=1`,
`just workbench evidence`, then `just verify`. Inspect both themed stills and
`timeline-scrub-navigation.webp` and `timeline-marquee.webp`; the animations
drive production transport, scrubbing, anchored zoom, pan and box selection
through real events. Internal captures and
measurements belong to ignored `.cache/workbench/`. When editor output intentionally changes, regenerate and review the public guide
through its ordinary pipeline as well.
