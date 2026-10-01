# Imported animation inspection

N3's animation panel presents immutable imported clips in the ordinary editor
viewport. It is an inspection and playback surface, not an authored animation
model. The [imported asset contract](scene-viewer.md) owns source loading and
evaluation; [authoring and interchange](authoring-and-interchange.md) records
why those contracts do not define N3's future animation authoring schema.

## Ownership

- `scene::Animation` and its channels own immutable source timing, targets,
  values, and interpolation. Decoded resources are shared snapshots.
- `SceneView` owns the accepted preview clip, time, playback state, exposure,
  and evaluated frame. These values stay outside the authored document and
  history.
- The application animation panel adapts the selected asset into the reusable
  `ui::timeline` presentation data and handles its semantic requests. The
  component does not parse assets, evaluate geometry, or mutate the document.
- `ui::timeline::Timeline` owns presentation state: visible interval, expansion,
  scrolling, inspection selection, and active scrub or box-selection gestures. The application
  owns the accepted playhead and playback clock.

The viewport and timeline share only the stateless `ui::marquee` rectangle
geometry and painter. Their gesture ownership, click-versus-drag decisions,
selection policies, and cancellation remain with their existing owners. These
two real uses can inform a later interaction boundary; they do not establish a
universal gesture or user-intent model.

Panel visibility is transient workspace state, initially closed. Import and
selection never open it automatically. The Tool Dock tab
bar floats at the viewport's lower-left while closed, below scene statistics
and above the separate status bar. Opening Animation moves that same tab bar
into the Tool Dock header and focuses its timeline. Tab activation is
idempotent: activating the selected Animation tab only restores timeline focus;
it never closes the Tool Dock, seeks, or changes playback. The header's close button
closes the Tool Dock and returns to its floating tab bar. The control identities and witnessed Tool Dock tab-bar
parent remain the same in either placement. Once open it follows the active selected imported
object; selecting a native object or no object leaves the panel open with an
empty state. A selected asset without clips also has an empty state and disabled
playback/seeking. Closing cancels an unfinished pointer gesture, returns focus
to the viewport, and otherwise preserves accepted playback. Timeline inspection
does not replace the editor's object selection. One editor and renderer remain
authoritative.

## Projection into tracks

Time is continuous seconds. A source channel key at `t` is presented at
`t - clip.start`; evaluation adds the source start back. The panel does not infer
an FPS, quantize key times, or convert imported timing into authored tracks.

Tracks group channels by source node and property. Source-node and channel
indices provide presentation identity within the retained asset snapshot; they
are not new authored object or track IDs. Rebuild presentation data when its
source or clip changes, not on each playback tick. Key metadata describes the
immutable sampled values and interpolation.

Projection has its own synchronous presentation budget: at most 250,000 keys,
20,480 tracks, and 64 MiB of retained labels and key metadata. The track budget
accommodates the current source bounds of 4,096 nodes plus 16,384 channels; the
key budget includes the workbench's 100,000-key case. These guard the expanded
UI snapshot and its indexes, independently of source decoding and pose-evaluation
budgets. Exceeding a limit reports that tracks are unavailable, without truncating
the clip or disabling preview transport. Text is charged as it is constructed so
an oversized snapshot is rejected before unbounded text accumulation.

Joint animation is ordinary node translation, rotation, or scale animation;
skin data maps joint transforms into mesh deformation. Morph weights are one
vector-valued channel, potentially with several components sharing each key
time. Do not imply that each weight is an independently timed channel, or
present quaternion rotation values as Euler angles.

Clips belong to the source asset and may target nodes outside its selected
source scene. The panel shows the full selected clip, explicitly labelled as
source-clip inspection. Filtering only rendered descendants would also risk
hiding joints or ancestors that influence a skin.

At Rest pose, the panel may show the first clip's tracks without evaluating it.
The selector and viewport still identify Rest pose. Playing, choosing a clip,
or seeking explicitly selects an animation; inspecting a key does not.

## Interaction and recovery

Transport controls and timeline requests converge on the existing preview
actions. A seek evaluates a candidate pose before publishing its accepted time
and frame. Ordinary validation failure retains the last accepted result.

Presentation requests use `f64` seconds; the current evaluator publishes `f32`
seconds. After a successful seek the adapter acknowledges the request when its
`f32` conversion exactly equals the accepted position, clearing the component's
pending-request indication. This avoids a rounding-only "requested/accepted"
discrepancy without inventing a different host playhead or applying an epsilon.
Clamped or rejected requests retain the indication of the actual host result.

A pointer scrub has a baseline containing the preview clip, time, playback
state, and evaluated frame. Begin/update requests pause playback and preview
candidate poses. End retains the accepted pose and remains paused. Escape or
focus loss cancels the scrub and restores the baseline, including whether it
was playing. Gesture identity and the selected preview owner prevent a late
update or release from affecting another asset.

Key inspection, expansion, zoom, pan, and Fit are presentation operations. They
do not seek unless an explicit seek/scrub request is emitted. Text fields,
popups, and active gestures retain input ownership. The application owns a
timeline-specific shortcut context: focused Space requests semantic Play/Pause,
while viewport Space remains the held Hand tool. The input binding and playback
action remain separate from the reusable timeline component. Numeric editors
and popups do not forward Space to transport, and a held gesture or document
edit session retains ownership. Only fresh key presses toggle playback; repeats,
release, and legitimate layout retries cannot toggle it again. This local context
is the extension point for later timeline shortcuts, not a new global hotkey.
Layout retries must not deliver the same request twice.
The timeline separates layout/input preparation from accepted-state painting.
The host consumes the prepared requests, publishes the evaluated pose, and then
finishes the timeline with the refreshed accepted state in the same UI pass.
Clamped and rejected requests still show the actual accepted playhead. Native
transport widgets retain ordinary egui behavior: an externally changed label or
value refreshes on the next frame; their active text buffers remain native.

Dragging empty key space draws a marquee. On release it selects displayed
markers intersecting the rectangle across tracks; each selected key retains both
its track and key identity. Shift adds to the selection. A clustered marker
selects all of its represented keys; hidden or offscreen markers are excluded.
Selection remains unchanged during the drag. Escape cancels without a seek or
selection change. Empty-space clicks seek only on release, after distinguishing
the click from box selection. The ruler and bare playhead retain scrubbing.
This inspection does not author keys or change object selection, animation
evaluation, or document history.
The timeline's inspection capability is independent of seeking. The editor
disables inspection during an active document-edit session, including a released
transform preview awaiting acceptance, so the timeline cannot take its gesture.

Do not use `Context::request_discard` to synchronize continuous preview values.
That API reruns the whole workspace and is intended for exceptional layout
measurement. The earlier integration retried every scrub frame, which triggered
egui's consecutive-multipass performance warning. A sustained-scrub regression
now requires one settled UI pass, current pose/playhead agreement, and one
evaluation per update. Playback ticking and request delivery remain gated to the
logical frame for legitimate layout retries initiated elsewhere.

Placements with the same source and source-scene reference currently share a
`SceneView`, so preview playback affects all such placements. The panel does not
claim an independent timeline for each instance. Authored scene timelines,
per-instance clip overrides, mixing, key editing, curves, and animation export
remain separate work.

## Verification and documentation

The [animation guide template](../templates/animation.md.in) and
[scenario](../../src/documentation/scenarios/animation.rs) exercise the production
panel through real pointer/key input, including explicit opening/closing and
focused playback shortcuts. The skinned and morph fixtures render
actual changing geometry in the same viewport, with assertions for the held
scrub, acceptance, cancellation, rest recovery, and unchanged document/history.
Inspection uses live timeline key observations; ruler gestures derive their
coordinates from the current ruler and visible time interval.

Component cases in the internal workbench cover presentation and input
isolation. They complement application regressions for selection ownership,
source switching, failed evaluation, input routing, and shared playback. The
public guide uses the existing deterministic replay, explicit clock, and
lossless WebP pipeline. Its media and prose are generated together.
