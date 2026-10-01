# Modern scene viewer milestone

Status: **glTF profile and unified editor integration implemented and verified locally**.
Started 2026-09-30. Initial viewer evidence appears below. OpenUSD and the expansion
checklist remain open. The earlier local-only gates below remain historical
evidence for the implemented profile.

## Outcome

Import modern scene assets into N3's ordinary editor, place them alongside native
geometry, inspect their contents, and preview animation through the same native
and executable-guide renderer. Asset placement is authored and undoable; internal
source geometry, materials, and animation remain immutable. No flattening or
lossless source authoring/export is implied.

The first delivery target is **glTF 2.0 / GLB core**, with an explicit extension
compatibility policy. OpenUSD is a separate ingestion/composition track. A parser
that reads a few USD meshes is not complete OpenUSD support.

## Architectural decisions

- One document and editor contain native geometry and placed asset references.
  Placement, duplication and deletion use ordinary history. Decoded source
  resources and transient playback remain outside it; read-only editor access
  is independent of the source format.
- `.n3.json` stores relative linked references where possible, not embedded glTF
  payloads. Save As preserves source origins; missing dependencies retain
  recoverable authored objects. Reopening refreshes the loaded snapshot.
- Introduce a format-independent evaluated scene boundary for renderable geometry,
  materials, textures, cameras and lights. Source hierarchy and animation remain
  available; render triangles are not an editable topology authority.
- Resource resolution is a host boundary. Local glTF loading has bounded reads,
  no network fetching, and explicit errors for unavailable/unsafe resources.
- glTF meters convert to centimeters in the import adapter. Display normalization is a
  stable view transform, never a per-frame change to asset size or animation.
- Loading must be transactional: a failed asset leaves the existing document or
  scene available. Playback and scrubbing never enter edit history; placement does.
- Unsupported **required** extensions reject the asset. Unsupported optional
  extensions produce visible diagnostics; approximations are named explicitly.

## Delivery checklist

Check a box only after implementation and evidence exist. Partial support is
recorded beside the item; it is never counted as full format conformance.

## Unified scene editor milestone

This follow-up replaces the separate viewer workspace with one document/editor.
Native geometry and linked assets share placement, selection, history, and the
viewport. It adds an authored asset-instance contract, not editable glTF internals.

Acceptance tracks:

- [x] Linked object contract: stable object identity, source and scene reference,
      ordinary transform, and shared resource caches outside undo snapshots.
- [x] One viewport: native and imported surfaces share camera, world coordinates,
      depth, selection feedback, and Local View; standalone-point chrome is deferred.
- [x] Editor permission: explicit read-only capability independent of format,
      preserving navigation and animation preview while rejecting authored edits.
- [x] Import and history: the real Import action appends objects as one edit;
      asset placement, duplication, deletion, and undo reuse ordinary behavior.
- [x] Save and reopen: relative links, Save As rebasing, package relocation,
      missing-source recovery, snapshot refresh on reopen, and source preservation.
- [x] Executable guide: real Import, mixed placement with undo/redo, PBR textures,
      skin and morph playback, live controls, and explicit-time replay assertions.
- [x] Regenerated media visually accepted, full native verification and bundle
      checks rerun for the unified implementation.

Asset-content editing, conversion to native mesh topology, animation/material
authoring, source-camera activation, source-scene switching UI, embedded dependency
packages, automatic reload, and glTF export remain deferred. Existing format and
rendering profile limits below still apply. Initial viewer validation at the end
is historical evidence, not a substitute for this follow-up's final gate.

## glTF profile checklist

### Container and assets

- [x] `.gltf` JSON and `.glb` binary containers.
- [x] External buffers/images and embedded buffer views/data URIs.
- [x] Resource resolver boundary, URI handling and asset-root restrictions.
- [x] Explicit memory/image/mesh limits and useful malformed-file errors.
- [x] Accessors: component types, normalization, offsets, stride, sparse data.
- [x] Default scene, authored scene references, node hierarchy and instanced mesh reuse.
- [x] Matrix/TRS transforms, handedness, mirrored scale and meter/cm conversion.
- [x] Indexed/nonindexed primitives, triangle/line/point topology handling.
- [x] File open, drag/drop and CLI validation use the same scene loader.

### Materials and rendering

- [x] Metallic-roughness PBR with linear lighting and correct output encoding.
- [x] Base color, metallic/roughness, normal, occlusion and emissive textures.
- [x] UV sets, vertex colors, texture samplers, wrapping and mipmaps.
- [x] Alpha OPAQUE/MASK/BLEND, sorting and depth policies.
- [x] Double-sided materials and correct transformed normals/tangents.
- [x] `KHR_materials_unlit` and `KHR_texture_transform`.
- [x] Directional, point and spot lights (`KHR_lights_punctual`).
- [x] Neutral studio lighting and exposure; distinguish ambient approximation
      from physically integrated HDR image-based lighting.
- [x] Parse and retain source perspective/orthographic camera data.
- [ ] Activate source cameras in the mixed editor; free navigation is the current policy.
- [x] Preserve editor Solid/Wireframe/X-ray behavior and existing guide pixels.

### Animation

- [x] Multiple clips, select/play/pause/scrub/loop/speed controls.
- [x] Translation, rotation, scale and quaternion interpolation.
- [x] STEP, LINEAR and CUBICSPLINE, including endpoint behavior.
- [x] Hierarchical animation and skin inverse-bind/joint evaluation.
- [x] Morph position/normal/tangent targets and animated weights.
- [x] Deterministic explicit-time evaluation shared by native and guides.
- [x] Reset to authored pose, clip changes and navigation during playback.
- [x] Cache static data; bounded dynamic deformation and measured limitations.

### Proof and user documentation

- [x] Purpose-built fixtures for exact numerical and failure tests.
- [x] Official Khronos sample assets with licenses and pinned provenance.
- [x] Static textured model, skin animation and morph animation viewed end to end.
- [x] GPU tests for material/color/alpha behavior and shader validation.
- [x] Real UI replay: open, inspect, animate, scrub, switch and recover.
- [x] Read-only editor capability and imported content boundaries.
- [x] Unified placement, persistence, mixed-scene rendering, and guide acceptance.
- [x] Generated user guide with stills and animated WebP, visually reviewed.
- [x] `just fmt`, `just verify`, native bundle build.
- [x] Final support matrix distinguishes implemented, partial and deferred work.

### OpenUSD and subsequent compatibility work

- [x] Research Rust candidates and native reference tools; adoption and runtime
      packaging still need a pinned implementation spike.
- [ ] Prove USDA, USDC and USDZ containers independently.
- [ ] Composition: layers, references, payloads, variants and asset resolution.
- [ ] Stage units/up-axis, xform stacks, visibility, purpose and time codes.
- [ ] Mesh subdivision policy, primvars, normals, material subsets and instances.
- [ ] UsdShade/UsdPreviewSurface textures, lights and cameras.
- [ ] UsdSkel, blend shapes and sampled animation.
- [ ] Bound supported schemas and clearly reject/report unsupported constructs.
- [ ] Compare representative results with an authoritative USD implementation.

### Major work that must not disappear from the plan

- Compressed geometry: Draco and meshopt; compressed textures: KTX2/BasisU.
- Advanced glTF materials: transmission, volume, IOR, specular, clearcoat, sheen,
  anisotropy, iridescence, dispersion and emissive strength; extension-by-extension
  visual proof, not blanket "all glTF" advertising.
- External HDR environment import, shadows, linear HDR transparency compositing,
  configurable tone mapping and color management. Procedural studio IBL has landed.
- Material variants, GPU instancing, animation-pointer extensions and metadata.
- Very large assets: cancellation, progress, scheduling, GPU skinning, streaming,
  LOD and profiling against a stated model-size budget.
- Export and asset-content authoring: embedded dependency packages/materials,
  stable IDs, animation editing and loss/preservation reports; separate from viewing.
- Linux renderer baselines and hosted CI remain a known independent day-zero issue.

## Research and evidence

See [modern scene format research](../research/modern-scene-formats.md) for source
links and the glTF/OpenUSD distinction. Implementation evidence and exact limits
will be filled in as the checklist advances.

## Implemented profile and explicit limits

| Area               | Implemented                                                                                                                                          | Limits / next proof                                                                                                                                                                                             |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Containers         | glTF 2 JSON, GLB binary, external files, base64 data URIs, embedded image buffer views                                                               | Local dependencies beneath the asset directory only; no network URLs or symlinks                                                                                                                                |
| Scene graph        | Authored scene reference, hierarchy, TRS/matrix transforms, shared source meshes, reflections, cm conversion                                         | Evaluated instances currently own separate world-space vertex arrays; GPU instancing deferred                                                                                                                   |
| Geometry           | All seven glTF primitive modes, indexed/nonindexed, sparse/interleaved/normalized accessors, normals/tangents/colors, UV0/UV1                        | Material references to higher UV sets fail explicitly; point/line appearance uses an unlit fallback                                                                                                             |
| Materials          | Metallic-roughness factors and all five core texture roles; PNG/JPEG, sampling/wrapping/mips; OPAQUE/MASK/BLEND, double-sided                        | Transparency sorts primitives, not triangles; intersecting transparency can be wrong. Lit tone mapping precedes LDR alpha composition. Derivative tangent fallback can differ from authored MikkTSpace tangents |
| Extensions         | `KHR_materials_unlit`, `KHR_texture_transform`, `KHR_lights_punctual`                                                                                | Unsupported required extensions reject; optional ones warn and use available core data. No Draco, meshopt or KTX2/BasisU                                                                                        |
| Lighting           | Directional/point/spot, studio key/fill fallback, procedural diffuse/specular IBL, exposure                                                          | 32 active punctual lights, no shadows or imported HDR environments. Appearance parity with another renderer is not claimed                                                                                      |
| Cameras            | Authored perspective/orthographic camera data retained; ordinary editor navigation                                                                   | Source-camera activation in the mixed editor is deferred; no camera authoring                                                                                                                                   |
| Animation          | Multiple clips, TRS and morph weights, STEP/LINEAR/CUBICSPLINE, quaternion interpolation, skin joints/inverse bind, morph positions/normals/tangents | CPU deformation and vertex upload; no animation-pointer, crossfade, authoring, or large-scene performance qualification                                                                                         |
| User workflow      | Native open/import/drop/startup, shared selection and placement, playback/pause/seek/loop/speed/rest, exposure, Solid/Wireframe/Material Preview     | Asset contents remain read-only; Save persists linked references and placement, not source payloads. Internal node picking/editing deferred                                                                     |
| Recovery           | Candidate load/evaluation/GPU replacement; stale-result generation guard; authored document snapshot guard; error feedback                           | Worker decoding is not cooperatively cancelled; allocation/device loss recovery is still the host's existing behavior                                                                                           |
| OpenUSD            | Research, schema/composition checklist, native reference-tool discovery                                                                              | No production USDA/USDC/USDZ loader or viewer claim in this delivery                                                                                                                                            |
| Export / authoring | N3/OBJ editing and linked asset placement share the document/history                                                                                 | No glTF/GLB export, source-content editing, or conversion to authored mesh topology                                                                                                                             |

The [architecture](../architecture/scene-viewer.md) describes ownership and
recovery. This is a tested viewer profile, not glTF conformance certification or
"every glTF extension" support.

## Resource and performance envelope

These are rejection bounds, not a promise that boundary-sized scenes are fast:

- Linked resources retained per document: 64 unique sources and 512 MiB of major
  decoded payloads and cached rest poses, including sources retained for Undo. Native files retain
  unresolved links with diagnostics; Import rejects an over-budget union atomically.
- Retained preview frames and combined placed-instance geometry each allow
  4 million vertices and 12 million indices. These bounds account for scene
  references and repeated placements, not only unique source meshes.
- Derived f32 display and rebased lighting coordinates have a magnitude limit of
  `1e12`, leaving headroom for shader arithmetic. This presentation profile is
  separate from canonical f64 centimeter coordinates and does not constrain
  serialized coordinates to that magnitude; unusable display candidates reject
  before publication.
- Input glTF/GLB: 64 MiB. Each referenced resource: 256 MiB; source/resource
  bytes together: 512 MiB. Decoded images: 256 MiB total, at most 8192 × 8192 each.
- At most 4096 nodes, 2048 meshes, 4096 total materials (including the importer's
  default material), 4096 textures, 512 images, 128 scenes/clips, and 1024 skins.
- Retained geometry/skin/animation payload: 256 MiB, charging repeated accessor
  uses. At most 2 million source vertices and 12 million expanded indices,
  4096 primitives/draws, 64 morph targets and 8 joint-influence sets per primitive.
- An evaluated pose allows 4 million vertices and 32 million charged deformation
  operations. Only the most recent rest scene is cached. These separate bounds
  are not a single peak-process-memory cap.
- GPU texture mip chains/color-space copies and geometry each have a 512 MiB
  budget, plus the device's individual texture/buffer limits. Wireframe edge
  allocations are included. The generated studio maps occupy about 1.1 MiB.

Informational CPU timing, Apple M4 Pro / arm64, Rust 1.98.1 optimized test profile,
2026-09-30 (initial viewer before adapter/core separation; one local run, tiny
fixtures, no GPU work included):

| Fixture           | Evaluated vertices / draws | Cold decode + rest |                120 animated evaluations |
| ----------------- | -------------------------: | -----------------: | --------------------------------------: |
| WaterBottle       |                   2549 / 1 |          70.263 ms | Static; 10,000 cached lookups: 0.057 ms |
| SimpleSkin        |                     24 / 1 |           0.411 ms |                          0.149 ms total |
| AnimatedMorphCube |                     24 / 1 |           0.187 ms |                          0.101 ms total |

Reproduce the non-gating probe with:

```sh
CARGO_NET_OFFLINE=true just cargo test --locked asset_io::gltf::tests::fixture_evaluation_timings -- --ignored --nocapture
```

GPU regressions separately inspect actual pixels and resource upload counts.
Large-asset frame time, peak memory and cancellation responsiveness still require
a representative benchmark set; these small results do not establish them.

## Validation evidence

- The three pinned Khronos fixtures pass official `gltf-validator`
  **2.0.0-dev.3.10** with **zero errors and warnings**. See their
  [validation record and provenance](../../fixtures/gltf/README.md).
- Numerical/failure fixtures exercise accessors, all primitive modes, hierarchy,
  normals/reflections, units, resource restrictions, animation interpolation,
  morphs, skins, bind matrices and allocation/work budgets.
- Native Metal pixel tests cover color-space handling, PBR texture roles, alpha,
  double-sided/reflected winding, UV/samplers, lights, cameras, wireframe and caches.
- User-input replay covers asset placement, content-edit rejection, editor access,
  playback, pause, scrub, rest, layout retries, and failure recovery. Native
  persistence tests cover relative links, Save As, package relocation, missing
  dependencies, source snapshot behavior, and source-file preservation.
- The generated guide uses the same production renderer and explicit clock for
  its PBR still and skin/morph clips. Physical trackpad/Finder interaction and
  cross-renderer visual parity remain separate manual/platform checks.

### Source review gate

October 1, 2026, macOS/Metal: `just verify` passed **915 Rust tests** and
**54 tooling tests**, with one intentionally ignored informational timing probe.
`just ci-macos` verified the native bundle, signature and metadata and passed
**19 focused native tests**. The production UI workbench evidence also regenerated
successfully.

Review fixed legal omitted final padding in dense/sparse matrix accessors,
included geometry-empty asset anchors in mixed-selection transform pivots, and
made oversized selection-proxy GPU uploads fail before scene publication.
Focused regressions cover these cases and their recovery boundaries.

Visual review inspected 20 guide/workbench stills and seven animation frame
sequences with their timing. No blocking legibility or outcome issue was found.
Live browser playback and physical-device acceptance were not re-established
by this review.

### Unified editor gate

Historical local gate (macOS/Metal, 2026-09-30): `just verify` passed **710 Rust tests**
and **52 tooling tests**, with zero failures and one intentionally ignored
informational timing probe. `just build` produced the locally signed native
`build/N3.app` bundle.
Guide regeneration and exact checks cover **25 features / 120 artifacts**.

The material still and mixed-placement, skinning, and morph animation keyframes
were reviewed for legibility. The placement example shows movement, undo, and
redo alongside native geometry; the animation examples show deformation and
rest-pose recovery. Isolated Chrome playback captured three distinct frames from
each animated WebP at 950 ms intervals. This proves actual browser playback,
while physical trackpad/Finder interaction and Linux renderer output remain
separate checks. This gate preceded the source commit and push.

### Initial viewer gate, before unified placement

Historical local gate (macOS/Metal, 2026-09-30): `just verify` passed **660 Rust tests**
and **52 tooling tests**, with one intentionally ignored informational timing
probe run separately. `just build` produced the locally signed `build/N3.app`.
Guide regeneration verified **25 features / 119 artifacts**. Every pre-existing
guide page and media file remained byte-identical except the intentionally
updated index and manifest. The new page and both animated WebP examples were
also reviewed in the local browser preview; browser captures confirm changing
animation frames. Native physical-device behavior and Linux output were not
re-qualified by this milestone.
