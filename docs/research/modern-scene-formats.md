# Modern scene formats: viewer coverage and implementation order

Research snapshot: September 30, 2026. This is repository-owned planning for the
local viewer milestone, not a declaration that every listed feature is supported.
No dependency choice, library integration, or USD rendering capability is proved
by this document. Capability claims must follow implementation and fixture tests.

## Recommendation

Build the shared scene evaluator and renderer around **glTF 2.0 core first**.
Then evaluate a pinned Rust USD library against the reference USD implementation
before choosing its production adapter. Keep the existing editable `.n3.json`
mesh document distinct from imported scene data. A scene viewer must retain
hierarchy, resources, material assignments, and animation instead of silently
discarding them on the way into an editable mesh.

glTF is a bounded delivery format: scenes, nodes, meshes, materials, textures,
cameras, skins, and animation have a defined core representation. Its coordinates
use meters, so an N3 centimeter conversion must apply consistently to geometry,
translations, cameras, animation, and skinning. Unknown required extensions must
prevent a false success; optional extensions may use their defined fallback with
an explicit capability report. GLB is a container for glTF, not a guarantee that
all referenced resources are embedded. [Khronos glTF 2.0 specification](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html)

USD adds composition and schema evaluation: reading text syntax is not equivalent
to opening a composed stage. It also permits domain and renderer extensions.
“Full USD support” is therefore not a useful blanket milestone. Name the file
containers, composition features, and schema/rendering profile that are tested.
[OpenUSD introduction](https://openusd.org/release/intro.html)

## Starting point and boundaries

At the start of this research, N3's maintained paths supported OBJ import,
editable polygon meshes and parametric primitives, Solid/Wireframe presentation,
selection, and camera navigation. The Cargo manifest had no glTF or USD parser.
The current GPU mesh vertex contains position and generated normal, rather than
the complete material, texture, deformation, and hierarchy data described below.
Later milestone changes must update their own implemented capability matrix;
this starting snapshot is not that matrix.

Suggested ownership within the existing application crate:

- **Format adapters:** parse source data, resolve its semantics, report unsupported
  features, and retain source identity. Do not issue GPU calls or mutate editor history.
- **Resource resolver:** bounded byte reads relative to an asset origin. Native
  paths and optional converter processes belong here; future web byte providers
  can implement the same requests without emulating a filesystem.
- **Scene data/evaluation:** hierarchy, shared meshes/materials/images, cameras,
  lights, animation channels, skins, and evaluated poses. Keep source resources
  immutable; playback changes evaluated state, not the authored document.
- **GPU scene/rendering:** resource upload, material shading, deformation, passes,
  and render caches. Reuse workbench navigation and selection presentation where
  their semantics still apply; do not make a PBR shader own editor state.
- **Viewer controls:** scene/clip/camera selection, playback, and diagnostics.
  Converting a supported part of an imported scene to editable N3 geometry is a
  separate, explicit future operation with a preservation report.

These are responsibilities, not a request for new public crates, a plugin system,
or a universal authoring schema. A small scene representation is justified by
the viewer's real consumers; replacing the canonical editing format is not.

## Coverage chunks and acceptance evidence

All entries below are proposed work/checks. A parsed field does not establish
that it is rendered, evaluated, preserved, or editable.

| Chunk                     | Initial implementation target                                                                                                      | Evidence before claiming support                                                                                                  |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Container and resources   | `.gltf` plus external resources and `.glb`; accessors with stride, offsets, sparse values, normalized integers, and checked bounds | Equivalent external/embedded fixtures, malformed/truncated inputs, missing dependencies, no partial scene installation            |
| Hierarchy and instances   | Scene roots, local/world transforms, shared mesh resources, distinct node instances                                                | Nested and mirrored transforms; one mesh instantiated with distinct transforms and shared materials; stable source-node selection |
| Units and coordinates     | One explicit conversion into centimeters, source up-axis/handedness, inverse-transpose normals and winding under reflections       | Known physical dimensions, transformed camera/light distances, animated translations, skin bind poses                             |
| Geometry and attributes   | Indexed/non-indexed primitives, authored normals/tangents, UV sets, colors; explicit handling of point/line/triangle modes         | Attribute/index correspondence and seam preservation; visible unsupported-mode report if any core mode is deferred                |
| Materials                 | Metallic/roughness factors and textures, normal/occlusion/emissive maps, alpha modes, double-sided materials                       | Textured PBR sample, isolated material-channel fixtures, mirrored tangent basis, mask threshold and transparent overlap           |
| Texture resources         | PNG/JPEG, image versus sampler identity, UV-set choice, filtering/wrapping/mipmaps, correct color versus data decoding             | One image reused by different samplers/channels, external and GLB image sources, UV seams and minification                        |
| Lighting and presentation | Linear-light rendering, environment lighting, exposure and tone mapping; a clear fallback environment when the asset supplies none | Dielectric/metal comparison under fixed illumination; stable Light/Dark UI captures without changing material appearance          |
| Cameras                   | Authored perspective/orthographic cameras alongside a separate navigation camera                                                   | Node-transformed camera, field of view/aspect, clipping, switch back to user navigation without modifying source                  |
| Animation                 | Clip selection, time sampling, play/pause/scrub, deterministic explicit clock, translation/rotation/scale channels                 | Beginning/intermediate/end poses, interpolation modes, seek versus sequential-play equivalence, no history changes                |
| Skinning                  | Joint hierarchy, inverse bind matrices, weights and joint attributes, correct coordinate spaces                                    | Animated two-joint fixture, bind-pose identity, nonidentity mesh transform, normals under deformation                             |
| Morph targets             | Position/normal/tangent deltas, mesh/node default weights, animated weights                                                        | Nonzero default pose plus two timed morph states; morph and skin applied in defined order                                         |
| Extensions/compression    | Capability registry separate from parsing; decoding and rendering each accepted extension                                          | Required-extension rejection, allowed fallback for optional extensions, paired compressed/uncompressed fixtures                   |
| IO and lifetime           | Background loading with cancellation, atomic scene replacement, resource ownership and clear diagnostics                           | Failed load leaves the current scene intact; repeat-load/unload releases resources; no UI stalls on a representative large asset  |
| Performance               | Reuse shared resources, update transforms/poses without rebuilding static buffers, bounded uploads and decoding                    | Measured cold load, peak memory, draw calls, and animation frame cost on named fixtures and hardware                              |

Khronos provides separate explanations of [texture coordinates and texture
bindings](https://github.khronos.org/glTF-Tutorials/gltfTutorial/gltfTutorial_013_SimpleTexture.html),
[PBR material maps](https://github.khronos.org/glTF-Tutorials/gltfTutorial/gltfTutorial_014_AdvancedMaterial.html),
[skinning](https://github.com/KhronosGroup/glTF-Tutorials/blob/main/gltfTutorial/gltfTutorial_020_Skins.md),
and [morph animation](https://github.khronos.org/glTF-Tutorials/gltfTutorial/gltfTutorial_017_SimpleMorphTarget.html).
Use the [official Sample Viewer](https://github.com/KhronosGroup/glTF-Sample-Viewer)
as a comparison under matched lighting and exposure; matching its appearance is
not established merely by importing the same mesh.

### Extension sequence

Treat extensions as independent entries with parser, evaluator/decoder, renderer,
fixture, and documentation status. Prioritize common small additions such as
unlit materials, texture transforms, and punctual lights after the core path.
Then evaluate material variants, quantization, instancing, and the more involved
material models as separate chunks. The [Khronos extension registry](https://github.com/KhronosGroup/glTF/tree/main/extensions)
is the authority for names and status, not a frozen list in this plan.

Compression is not another material flag. [Draco](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_draco_mesh_compression)
and [meshopt](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Vendor/EXT_meshopt_compression)
need decoding before ordinary accessor use. [KTX2/BasisU](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_texture_basisu)
needs texture transcoding and a GPU-format fallback policy. Declaring extension
JSON readable must never advertise those decoders as working.

[Punctual lights](https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_lights_punctual)
are an extension, while the viewer's default environment is its own presentation
choice. Keep imported lighting, fallback lighting, viewport background, and UI
theme separate. A dark theme must not silently darken an asset's base color.

## USD needs more than another parser

| USD area                | Questions the adapter and viewer must answer                                                                                                                                                                   |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Stage composition       | Which sublayers, references, payloads, variants, inherits, specializes, and overrides are resolved? Are unresolved dependencies errors or explicitly incomplete views?                                         |
| Stage metadata          | How are `defaultPrim`, `upAxis`, `metersPerUnit`, time range, time-code rate, active/loaded state, and visibility/purpose interpreted?                                                                         |
| Transform evaluation    | Honor ordered transform operations, reset/inverse operations, hierarchy and time samples; do not read only a convenient translation field                                                                      |
| Instances               | Preserve native prototypes/instances and distinguish them from PointInstancer; resolve per-instance transforms, visibility, IDs, and material associations                                                     |
| Geometry schemas        | Polygon mesh indices/counts, orientation, holes, indexed primvars and interpolation domains, face subsets, subdivision and creases; distinguish curves/points/analytic shapes/volumes from unsupported content |
| Materials               | Resolve inherited/collection/subset bindings and shader connections; initially declare a UsdPreviewSurface profile, not support for arbitrary shader networks or MaterialX execution                           |
| Lighting and cameras    | Map UsdLux and camera schemas with explicit units/exposure and supported light types; report approximations instead of treating area lights as identical point lights                                          |
| Time and deformation    | Evaluate time samples/offsets, skinning and blend shapes, changing topology and value clips; define what the first static-time slice does with animated content                                                |
| Layers and assets       | Keep stage/session state distinct from source files; anchor asset paths to contributing layers and packaged resources, with resolver diagnostics and dependency inventory                                      |
| Containers              | Identify USDA, binary USDC, ambiguous `.usd`, and USDZ package behavior independently; a USDA fixture alone proves neither crate parsing nor package resolution                                                |
| Unknown schemas/plugins | Preserve names/diagnostics and fail or show an explicitly incomplete profile; do not execute arbitrary discovered plugins as part of opening a user file                                                       |

The mesh schema includes subdivision semantics, holes and primvar interpolation;
rendering every mesh as raw face polygons can visibly change its intended shape.
[UsdGeomMesh](https://openusd.org/release/api/class_usd_geom_mesh.html)
Transform stacks and point instancing each have their own evaluation contracts.
[UsdGeomXformable](https://openusd.org/release/api/class_usd_geom_xformable.html),
[UsdGeomPointInstancer](https://openusd.org/release/api/class_usd_geom_point_instancer.html)

USD's stage length metadata falls back to centimeters when unauthored. Read and
convert authored values; do not infer scale from an asset's apparent size.
[OpenUSD linear units](https://openusd.org/release/api/group___usd_geom_linear_units__group.html)

UsdPreviewSurface is a particular portable shading model, UsdLux defines lighting
semantics, and UsdSkel defines skeleton/blend-shape data. Supporting these selected
schemas still does not render every USD scene or every shader network.
[UsdPreviewSurface](https://openusd.org/release/spec_usdpreviewsurface.html),
[UsdLux](https://openusd.org/release/api/usd_lux_page_front.html),
[UsdSkel](https://openusd.org/release/api/usd_skel_page_front.html)

USDZ has packaging rules in addition to ordinary file parsing. Package-contained
assets must remain resolvable after loading, and package entries should not require
unrestricted extraction into the user's filesystem.
[USDZ specification](https://openusd.org/release/spec_usdz.html)

## Library and native-tool choices

| Candidate                            | Evidence and fit                                                                                                                                                                                                    | Recommendation                                                                                                                                                                           |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `gltf-rs/gltf`                       | Rust glTF 2 loader; inspected manifest identifies 1.4.1, optional import/image support, typed extension feature flags and raw extension/extras access                                                               | Use as the first glTF parser candidate. Keep resource policy under N3 control rather than assuming convenience import supplies our limits.                                               |
| `mxpv/openusd` and `openusd-schemas` | Rust implementation with no C++ dependency; advertises file formats, composition and typed schema access. Published schema docs identify 0.7.0. Development README and roadmap also include unreleased `main` work. | Best lightweight USD experiment. Pin the evaluated release, exercise composition/schema cases against the reference tools, and record unsupported features.                              |
| `AndrejOrsula/pxr_rs`                | Bindings around native OpenUSD; its README calls the project early development, incomplete and primarily targeted at OpenUSD 22.11                                                                                  | Do not make it the default merely because it wraps USD. Native build/API/version maintenance is a separate commitment.                                                                   |
| Reference OpenUSD helper process     | A small versioned exporter can open a stage using the reference implementation and emit a defined evaluated-scene payload                                                                                           | Sound fallback if Rust coverage is insufficient; isolate native runtime/toolchain dependencies from the application and resolver API. It still needs packaging and reproducibility work. |

Sources: [`gltf` manifest](https://github.com/gltf-rs/gltf/blob/main/Cargo.toml),
[`openusd` project](https://github.com/mxpv/openusd),
[`openusd` roadmap](https://github.com/mxpv/openusd/blob/main/ROADMAP.md),
[`openusd-schemas` API](https://docs.rs/openusd-schemas/latest/openusd_schemas/),
[`pxr_rs` project](https://github.com/AndrejOrsula/pxr_rs).

The inspected `gltf` convenience importer enables PNG/JPEG through `image`;
N3's starting manifest enabled PNG/WebP only. JPEG support and decoded texture
budgets need explicit verification. Typed material-extension flags do not imply
that N3's shader implements those materials.

The Rust USD roadmap explicitly leaves work including splines, color-space
features, and some validation unfinished. Some schema/fallback work is marked
`main`, so copying the development README's feature list into an N3 release claim
would be misleading. The candidate has not been compiled or tested by this
research task.

### Locally available reference tools

Read-only probes on this Mac found `/usr/bin/usdcat` and `/usr/bin/usdchecker`.
`usdcat --version` reported **Apple USD Tools (0.25.2)**. Its help exposes
`--flatten`, `--flattenLayerStack`, load-only and population-mask options. The
system Python had no `pxr` module, and `usdview` was not on PATH. No installation
was attempted. These are observed host capabilities, not portable dependencies
or a guarantee about the installed SDK's public linking surface.

`usdcat --flatten` can provide a useful reference composition result for a spike,
but does not emit a renderer-ready mesh interchange. Flattening removes variant
choices except the selected result and retains instancing through prototypes and
internal references. Materials, schemas, transforms, time samples, and assets
still require evaluation. Never route USD through a small ad hoc USDA parser and
call that complete USD support. [UsdStage flattening contract](https://openusd.org/release/api/class_usd_stage.html)

The installed checker's help also states that relevant animated attributes are
checked only at their first sample. Validation is useful evidence, not proof of
all animation frames or appearance. Apple documents that conversions can change
between OS versions; record converter versions if a native helper becomes part
of a tested path. [Apple USD creation guidance](https://developer.apple.com/documentation/usd/creating-usd-files-for-apple-devices)

## Validation, fixtures, and resource policy

Use tiny authored fixtures for individual invariants and a small selection of
licensed [Khronos sample assets](https://github.com/KhronosGroup/glTF-Sample-Assets)
for integration. Keep the source revision, exact downloaded paths, checksums,
licenses, and intended assertions beside each fixture. Do not download the whole
asset collection merely to test one feature.

Run the [Khronos validator](https://github.com/KhronosGroup/glTF-Validator) on glTF
fixtures and the reference USD checker where available. In addition, assert N3's
actual evaluated values and render outcome: format validity alone does not prove
the viewer correctly consumes the asset.

Independent fixture evidence is now recorded in the
[glTF fixture README](../../fixtures/gltf/README.md): official Khronos validator
**2.0.0-dev.3.10** checked Water Bottle, Simple Skin, and Animated Morph Cube,
including external buffers, with zero errors and zero warnings. The retained
information/hints are documented there. This validates that pinned sample set;
it does not establish every importer or renderer capability in this roadmap.

- Bound file bytes, decoded images, counts, accessor arithmetic, graph depth,
  deformation sizes, decompression expansion, and GPU allocations before use.
- Define external asset roots and URI handling. Reject unexpected network schemes
  by default, prevent path traversal/symlink escape beyond permitted roots, and
  distinguish missing resources from unsupported features.
- For a helper process, use argument arrays, bounded output/time, cancellation,
  and a versioned response contract; contain plugin and asset resolution policy.
- Install a scene only when the accepted profile is validated. A failed or
  cancelled load must preserve the previous scene and editor document.
- Cache by resource identity and revision. Static texture/geometry uploads must
  not repeat because playback advanced or the pointer hovered an object.
- Add executable user guides through the real loading, camera, diagnostics and
  playback paths. Fixed animation times and fixed lighting make screenshots
  reviewable. Physical-device and large-asset performance checks stay separate.

## Suggested milestone order

1. **Static glTF scene:** real hierarchy, resource resolution, attribute-correct
   geometry, material assignment, camera selection, and capability diagnostics.
2. **PBR appearance:** complete chosen material/texture profile, alpha behavior,
   fixed environment lighting, and visual comparison. Keep editor Solid styling
   separate from imported materials.
3. **Animated glTF:** clip evaluation, skins, morphs, deterministic playback and
   bounds. A static-only loader is a useful slice, but is not full glTF core.
4. **Extension batches:** independently verified decoders, lighting and material
   extensions, prioritized by real user assets rather than registry size.
5. **USD adapter proof:** same small scene expressed with sublayers, references,
   variants, instancing, textures, units and time samples. Compare a pinned Rust
   candidate with reference USD results before adopting its stage path.
6. **USD schema expansion:** track actual evaluator and renderer coverage. Report
   skipped/approximated content and keep complex schemas as explicit follow-up
   chunks. Export, authoring composition, and lossless round trips are separate
   commitments from viewing.

For each chunk, completion means a tested capability row, representative fixtures,
visible diagnostics for unsupported content, and executable documentation. A broad
roadmap remains useful without presenting an unfinished viewer as a complete
implementation of either ecosystem.
