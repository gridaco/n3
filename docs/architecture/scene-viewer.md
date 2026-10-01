# Imported assets in the editor

Imported glTF/GLB assets are ordinary placed objects in the authored document.
`Geometry::Asset` stores a source reference and scene index; the surrounding object
owns its stable ID, name, and transform. Native primitives, editable meshes, and
linked assets share Layers, selection, navigation, placement, duplication,
deletion, and undo. Imported internal meshes, materials, and animation remain
immutable. Entering vertex edit never silently flattens an asset.

Read-only access is a capability of the editor, independent of file format. It
blocks document mutations while preserving selection, navigation, inspection,
and playback. Opening a glTF asset does not select this capability automatically.
`--read-only [PATH]` enables it explicitly and retains it across New and Open.

## Ownership

| Boundary                     | Responsibility                                                                                               |
| ---------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `asset_io`                   | Unified document load/import, linked-source resolution, native persistence, and format dispatch              |
| `asset_io::gltf`             | Parse glTF/GLB, validate format rules, decode resources, and map supported semantics into the internal scene |
| `asset_io::ResourceResolver` | Supply bounded external resource bytes; the native implementation stays inside the asset's directory         |
| `scene`                      | Immutable internal scene, independent validation, and explicit-time evaluation into world-space results      |
| `scene_view`                 | Clip, time, play/pause/loop/speed, and exposure for each source/scene; no editing history                    |
| `render::renderer`           | Compose native geometry and placed imported scenes with shared camera, depth, and editor overlays            |
| `render::scene_renderer`     | Neutral Solid, PBR Material Preview, studio environment, and primitive draw passes                           |
| `ui::asset_instances`        | Imported source details and preview lighting in Properties; shared preview publication                       |
| `ui::animation_data`         | Immutable source clips projected into format-independent timeline presentation data                          |
| `ui::animation_panel`        | Selected-asset timeline, transport, and cancellable scrub sessions below the viewport                        |
| `ui::timeline`               | Reusable time presentation and input; emits requests without evaluating or authoring animation               |
| `native`                     | Background file loading, candidate installation, window lifecycle and filesystem effects                     |
| `documentation`              | The same controls, clock, evaluator, renderer, and capture path exercised by user guides                     |

This remains one application crate. [Authoring and interchange](authoring-and-interchange.md)
defines why the internal scene is a viewing/evaluation boundary rather than an
authoring schema or a promise of lossless export. Source extensions and metadata
are not round-tripped. Adding another adapter requires explicit semantic mapping
and diagnostics; it must not bypass resource policy or mutate editor history.

## Coordinates and evaluation

The glTF adapter converts spatial lengths from meters to N3 centimeters before
publishing an internal scene, including positions, node and animation
translations, morph position deltas, inverse-bind translations, camera lengths,
and light ranges. Scale and orientation remain unitless. `SceneAsset::new`
validates internal invariants without a parser or filesystem; evaluation consumes
only this centimeter-based model. Skinning uses joint world transforms and
inverse bind matrices; morph
position/normal/tangent deltas apply before skin deformation. Each sample starts
from authored values rather than accumulating transforms from the previous frame.

The document establishes a display transform from native geometry and resolved
asset bounds on load. The object's authored transform places each evaluated asset
in document space. Playback changes geometry inside this fixed frame: it never
rescales or recenters the model to make each pose fit. Frame and Frame Selection
use the shared editor camera and include asset placements. Imported camera data
remains available internally; activating a source camera in the mixed editor is
deferred. Physical light attenuation accounts for centimeter/display conversion.

Derived f32 display and rebased lighting coordinates are validated against a
`1e12` magnitude profile before publication, leaving room for shader arithmetic.
Canonical coordinates remain f64 centimeters; this display constraint is neither
a serialization limit nor permission to clamp authored values.

`SceneAsset::evaluate(scene, None)` returns a cached `Arc` for the authored pose.
An explicit animation sample uses absolute clip time in seconds. Playback exposes a
relative time from the clip's first key, owns looping, and supplies the absolute
sample. Evaluation is deterministic; it has no wall-clock access. Native UI and
executable tutorials supply their clocks through the same path. Pause retains the
current frame, and Rest pose returns the cached authored result.

GPU resources follow asset, evaluated-frame, and placement identity. Hover,
preferences, exposure, and paused redraws do not decode textures or deform source
geometry again. Dynamic deformation is currently on the CPU. Source meshes and
payloads are shared; evaluated draws and GPU placement work still have explicit
allocation bounds. Large-scene scheduling and GPU skinning remain separate work.

The editor keeps evaluated asset frames outside its history snapshots. Placement
uses the ordinary transaction lifecycle. Playback changes runtime frames, without
advancing document history or dirty state. Instances with the same source and
scene share playback. Pausing throughout an active edit session, including
previews between pointer gestures, keeps its interaction baseline stable. Resource caches retain deleted instances' sources for Undo.

## Loading, references, and recovery

`asset_io::load` supplies a `LoadedDocument`: the authored document, decoded source
assets keyed by reference, diagnostics, and exact native file bytes for conflict
checks. Open replaces the active document after candidate preparation; Import
appends objects with fresh IDs as one ordinary editor edit. Both use the same
format adapters. OBJ continues to import supported authored polygon topology.

Native loading runs off the UI thread. A generation token rejects superseded
results; comparison with the authored document snapshot protects edits made while
loading without rejecting results merely because playback advanced. Candidate
parsing, evaluation, and GPU preparation precede publication. Worker cancellation
remains stale-result cancellation, not cooperative decoder cancellation. A failed
new asset import leaves the current document intact. Animation failure preserves
the last usable frame and pauses playback.

A native document with missing or broken links still opens. It retains each
asset object and reference, reports diagnostics, and shows no invented replacement
mesh. The object remains selectable through Layers; restore the source and reopen
to recover it. An unavailable source scene likewise remains an authored reference.

Live references are absolute host paths. On save, a cloned serialization snapshot
rebases them relative to the destination directory where possible. Save As keeps
the same origins without mutating the working document or its history. Files on
a different filesystem root can retain absolute references. Exact original native
bytes remain the conflict baseline; their relative path spelling need not equal
the normalized in-memory document. `.n3.json` embeds neither glTF payloads nor
texture data, and saving never writes the linked sources.

Loading is a snapshot: reopening reads current source bytes. There is no file
watcher, automatic reload, or content-hash pinning yet. Moving a document and its
relative resource package together preserves links; moving only the document
may not. Repeated Import validates the whole retained resource cache, including
Undo resources: at most 64 unique sources and 512 MiB of major decoded payloads
and cached rest-pose arrays. Shared index arrays are charged conservatively.
Retained preview frames and combined placed-instance geometry each have separate
4-million-vertex and 12-million-index bounds, so source reuse cannot bypass
instance costs.
Per-source parser/evaluator and GPU limits apply separately; this is not a
peak-process-memory guarantee.

Document references may point to local assets outside the document directory.
Inside each glTF package, the resource resolver still rejects network URLs,
absolute paths, parent traversal, and symbolic links. Relative buffers and images
must live beneath the glTF file's own directory; supported base64 data URIs and
GLB buffer views remain available. These stricter dependency rules do not imply
that every valid glTF filesystem layout can be opened unchanged.

## Rendering profile

Viewport shading is a transient display policy shared with native objects:

- **Solid** (default): opaque neutral inspection shading, fixed editor lights,
  and double-sided surfaces, ignoring source textures, vertex colors, material
  factors, alpha, source lights, and exposure. Evaluated buffers retain source
  smooth normals and animation poses; lines and points remain visible.
- **Material Preview**: the supported source-material presentation described
  below. Native geometry without materials retains its editor-material fallback.
- **Wireframe**: topology inspection without filled surfaces. Selection occlusion
  remains controlled separately by X-ray. Imported triangle meshes naturally
  expose their triangle boundaries; native polygon boundaries remain intact.

X-ray is independent of shading and overrides filled imported surfaces with the
neutral translucent editing cue. Switching modes neither modifies source data
nor reloads resources. **Rendered** is deferred until scene-controlled lighting,
environment, and render settings have their own contract; it has no UI entry.

In Material Preview, imported surfaces use metallic-roughness shading, linear-light calculations, color/data
texture separation, a built-in procedural studio environment with filtered GGX
specular maps, cosine diffuse convolution, and an integrated BRDF lookup table.
Imported punctual lights supplement the environment for their own asset; they do
not illuminate unrelated native objects or other asset placements. Assets without lights also
receive studio key/fill lights. UI theme changes the background, not material
factors or the environment. Exposure is transient preview state, with its control
available in Material Preview. Sharing a viewport does not add material authoring.

Opaque and masked surfaces write depth. Blended primitives are sorted back to
front by their centers and do not write depth. Intersecting transparent geometry
and ordering within one primitive are not solved by this policy. The current
compact tone mapper runs per lit fragment before blending; a linear HDR composite
and configurable color management are follow-up work. Unlit colors bypass the
lit tone mapper. These are presentation limits, not lossless render parity with
other applications.

Triangle assets use ordinary selection silhouettes; line assets use selected and
hovered edge strokes. Imported standalone points render and can be picked, with
selection visible in Layers; expanded selected-point viewport feedback is deferred.

The supported material/extension profile, bounded resources, validation evidence,
and deferred format chunks are tracked in the
[viewer milestone](../milestones/modern-scene-viewer.md). The generated
[scene guide](../guide/scene-viewer.md) is the user-facing walkthrough; it never
links back to this engineering document.

## Timeline and live preview controls

The selected asset's animation controls live in a resizable panel below the
viewport. The reusable timeline receives immutable track/key presentation data
and the host's accepted playback values. It emits semantic requests; the
application adapter chooses and evaluates a clip before publishing a changed
time or pose. Properties retains source details and preview lighting.

Scrubbing uses one captured preview baseline. Updates pause and preview;
release keeps the accepted pose paused. Escape or focus loss restores the
baseline's exact frame, time, clip, and playback state. These sessions are
transient preview interactions, not authored document transactions or Undo
entries. Source selection and gesture identity prevent a late request from
changing an unrelated asset.

The time field uses the standard timeline transport widget. Displaying a live
value must not synthesize a Seek, round the stored clock, or pause playback.
Validate and clamp requested edits in the host; process each logical frame's
requests once, including when egui repeats layout. Failed evaluation retains the
last accepted pose and reports the error. The
[animation inspection contract](animation-inspection.md) covers track projection,
input ownership, shared-instance playback, and executable guide evidence.
