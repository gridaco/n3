# RFC 0002: N3-owned scenes

Status: **Draft — architecture and implementation plan; not implemented**

Date: 2026-09-30

The user requested a format-independent scene design, followed by a feasibility
review against the current internal APIs and scene interchange. This RFC records
the proposed decisions and implementation gates. It does not approve an OpenUSD
backend, imported-content editing, or a new rendering system.

## Problem and recommendation

A document should hold several independent modeling arrangements: a working
model, an alternative, and a presentation setup, for example. Each arrangement
needs its own objects and remembered view while using the same editor, tools,
history, and viewport. This need exists with native primitives alone.

Introduce an authored **Scene**, owned by the N3 **Document**. A scene contains
ordinary N3 objects. The editor displays one active scene. Scene membership is
exclusive initially; decoded immutable resources may still be shared.

Do not derive this contract from an importer's scene array. A source's entry
points and evaluated render frames remain different concepts with different
identities. Source compatibility is reviewed after the authored design below.

## Current implementation and migration surface

### Readiness review

The ownership model is sound, but the full proposal is not yet implementation-
proven. It combines authored scenes/persistence, history/activation changes, and
import catalogs/pose residency. Keep this RFC **Draft** until a smaller integrated
proof ratifies the signatures; do not treat all later stages as one approved
implementation batch.

First prove two native scenes using the ordinary editor: switching and remembered
camera/selection, edits and cross-scene Undo/Redo, pending-session isolation, stable
IDs, v1 migration and all-scene save/reference rebasing. Existing linked objects
and default-entry import must keep working. Defer catalog/import-all UI and more
general inactive-pose eviction until that proof passes. This proof must use the
shared editor path, not introduce a second viewer or scene-specific editor.

One recovery policy remains to be settled before accepting the proposed history
API: **presentation failure must not unexpectedly strand authored Undo/Redo**.
Keeping the stack unchanged prevents corruption but can still leave a user unable
to recover. Exercise restoration with a missing linked source, a preview pose
different from edit time, and unavailable presentation resources. Prove recovery
with an explicit placeholder/unavailable-presentation state, or document a precise
limitation; do not quietly make generic history depend on a working GPU. The
preflight guarantees below are candidate behavior, not proof that this recovery
question is resolved.

### Observed code

These are observations from the local code, not proposed behavior:

- [`Document`](../src/model/document.rs) has one `objects: Vec<Object>`.
  `eval_object`, `validate`, `render_mesh_with_assets`, and
  `DisplayFrame::from_document_with_assets` assume that one object domain.
- [`Editor`](../src/editor/mod.rs) owns one document, selection, edit session,
  display frame, geometry cache, and `EditHistory<Snapshot>`.
- [`WorkspaceUi`](../src/ui/workspace_ui.rs) owns one camera/navigation state,
  Local View return state, ruler/inspector caches, and asset previews.
- [`asset_io::load`](../src/asset_io/linked.rs) both loads external content and
  immediately chooses its document organization. A scene asset becomes one
  object referencing `asset.default_scene`.
- Imported `SceneAsset` retains multiple source scenes. An `AssetInstance` stores
  a source and source-scene index; the current UI does not switch that reference.
- [`SceneRenderer`](../src/render/renderer.rs) already composes native and
  imported objects in one viewport. Its input need not become format-specific.

This is a cross-cutting migration of ownership and function arguments, not just
an extra selector. Keeping one editor per scene would obscure these boundaries
and create separate history/permission/input behavior; that approach is rejected.

## Authored model, independent of interchange

The following Rust is signature-level design, not a public SDK or compiling patch.
`ObjectId` denotes today's `u64` object identity; a newtype is useful but does not
require a simultaneous rewrite of every geometry-local ID.

```rust
struct Document {
    version: u32,
    length_unit: CanonicalLengthUnit,
    scenes: Vec<Scene>,
}

struct Scene {
    id: SceneId,
    name: String,
    objects: Vec<Object>,
}

struct SceneId(u64);
```

Invariants and defaults:

- A document has an ordered, nonempty list of scenes. Empty scenes are valid;
  deleting the last scene is rejected. A new document starts with `Scene 1`.
  Add an explicit document-wide `MAX_SCENES` bound, including empty scenes;
  object counts alone do not bound scene metadata or remembered views.
- Scene IDs are nonzero and document-wide unique. Object IDs are also
  document-wide unique in their own namespace. Vertex/face/edge identities remain
  object-local. Names are labels, may repeat, and never identify a scene.
- Each object belongs to exactly one scene. Moving or duplicating an object in
  one scene does not change authored objects in another. Scene duplication gives
  the copy a fresh scene ID and fresh object IDs, preserving geometry-local IDs.
- Scene order is authored and serialized. Active scene, camera, selection, and
  playback are session state. A fresh native open starts at the first scene;
  restoring last-visited state across launches is deferred. No authored default
  scene or default-selection preference is necessary for this milestone.
- One document unit remains one centimeter. Scenes do not have separate physical
  units, parent transforms, coordinate conventions, or inherited settings.
- Geometry variants keep their current meaning. Scene support does not introduce
  grouping, shared editable object membership, nested scenes, or scene instancing.

Use an editor-owned monotonic ID allocator initialized above the loaded maxima.
Undo must not rewind it. This prevents a deleted/undone scene's remembered state
or an asynchronous request from accidentally addressing a newly created scene
with a reused ID. Checked exhaustion is an error; failed operations may leave
unused IDs. The allocator is transient because history, pending requests, and
view memories do not survive closing the document. IDs already saved in a file
remain stable. A separate document-generation token scopes IDs across file opens.

Keep this model in `src/model/`, with a focused `scene.rs` if useful. The existing
`src/scene/` remains the imported runtime model for now. Qualified names such as
`model::Scene` and `scene::SceneAsset` make ownership explicit; a broad directory
rename or crate extraction is not a prerequisite.

## Ownership of state

| State                                                                        | Owner and lifetime                                                   |
| ---------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Scenes, names, membership, objects, geometry, linked references              | Authored document; native persistence and history                    |
| Active scene and editor access                                               | One editor session; switching is not an edit                         |
| Current object selection                                                     | Editor; remembered separately for each scene                         |
| Vertex-edit provenance and pending transform/property session                | Current interaction only; never parked in an inactive scene          |
| Camera, navigation orientation, Local View and display normalization         | Per-scene viewport memory; not authored or undone                    |
| Hover, pressed keys, pies, drags, rename fields, rulers and inspector caches | Current UI; settle or invalidate during activation                   |
| Decoded immutable asset resources                                            | Document resource store, shared across scenes and history references |
| Clip/time/exposure for an asset preview                                      | Per-scene transient preview state; not document animation            |
| Evaluated poses, picking proxies and GPU instances                           | Derived, bounded caches; only active scene must be resident          |

Remember object selection and camera per scene. Normal scene switching leaves
vertex edit mode after any operation is settled; it does not preserve an open
primitive edit visit in the background. History may restore vertex-edit context
when undoing an actual edit. Tool choice remains shared across the editor.
Viewport-wide preferences such as theme remain where they are today.

Prune remembered scene state when its ID is no longer reachable from the working
document, retained history or the current transaction. Retaining deleted scenes
for Undo must not turn repeated create/delete operations into an unbounded cache.

Local View memory includes its return camera and isolated object IDs. Reconcile
it against the destination scene after history changes; clear invalid isolation
rather than allowing invisible selected objects. View memory must not retain
unfinished navigation animations, gestures, or a held-key owner.

## Core API changes

Prefer explicit scene scope at mutation, evaluation, import, and history
boundaries. UI commands may resolve the active scene at dispatch time; functions
that outlive that dispatch must keep the resolved ID.

```rust
impl Document {
    fn scene(&self, id: SceneId) -> Result<&Scene>;
    fn all_objects(&self) -> impl Iterator<Item = (SceneId, &Object)>;
    fn validate(&self) -> Result<()>;
}

impl Scene {
    fn object(&self, id: ObjectId) -> Result<&Object>;
}

impl Object {
    fn evaluate_mesh(&self) -> Result<EditableMesh>;
}

impl Editor {
    fn active_scene_id(&self) -> SceneId;
    fn active_scene(&self) -> &Scene;

    fn create_scene(&mut self, name: String) -> Result<SceneId>;
    fn rename_scene(&mut self, id: SceneId, name: String) -> Result<bool>;
    fn duplicate_scene(&mut self, id: SceneId) -> Result<SceneId>;
    fn delete_scene(&mut self, id: SceneId) -> Result<bool>;

    fn commit_scene(
        &mut self,
        target: SceneId,
        edit: impl FnOnce(&mut Scene) -> Result<()>,
    ) -> Result<bool>;
}
```

Keep mutable access inside the existing validated candidate lifecycle. A
`scene_mut` helper may exist there; UI code must not bypass history with it.
Whole-document transactions remain necessary for creation/deletion, ID remapping,
and imports adding several scenes. No second mutable copy of the active scene is
authoritative. `Object::evaluate_mesh` removes unnecessary document membership
from primitive/mesh evaluation and still rejects immutable asset contents.

Read-only permission allows switching, selection, navigation and playback; it
rejects scene creation, rename, duplication, deletion, import and authored edits.
No new shortcuts are proposed.

## Scene activation and history

### Activation is a prepared transition

The UI exposes a compact scene selector above Layers. Layers, selection tools,
Frame All, rulers, properties, Local View, and statistics address the active
scene. Scene management controls reuse semantic editor operations.

Proposed transition:

1. Validate the target and current document generation. A same-scene request is
   a no-op. Reject a switch during a pointer/property interaction or unresolved
   transform preview with the existing Apply/Cancel guidance; do not implicitly
   accept or discard an edit. Text fields finish through their own focus policy.
2. Prepare the target's derived geometry, linked poses, selection and remembered
   viewport state. Preflight ordinary CPU/GPU validation before publication.
3. On success, settle the outgoing vertex-edit visit, remember its object
   selection/view, activate the target, and restore/reconcile target memory.
   Clear hover, transient controls, and held input; invalidate derived caches.
4. On preparation failure, preserve the active scene, pending state, document,
   history and displayed result. Report the unavailable destination. Device-loss
   or process-crash recovery is outside this ordinary validation guarantee.

Use a small internal prepared value, not a new command framework:

```rust
prepare_activation(document, target, assets, remembered_view)
    -> Result<PreparedSceneActivation>;

// Publish after host GPU preflight; reject a stale document/view generation.
WorkspaceUi::install_activation(prepared) -> Result<()>;
```

The native and documentation hosts must use the same preparation/publication
path. `Editor::new` is for opening a document, not for switching scenes. A fresh
scene gets default navigation and framing; switching back restores its view.

### Global undo needs entry-owned context

Keep one document history, including scene management. Scene switching creates
no entry, does not clear Redo, and does not change the saved-document comparison.
Undo/Redo reveal the scene associated with the edit, restoring its selection and
edit provenance without undoing its camera navigation.

Merely adding `active_scene` to today's `Snapshot` is incorrect. The generic
history's `undo(current)` pushes the caller's current snapshot onto Redo. After
editing A and browsing B, that captures B as the redo destination for an edit in
A. Scene deletion also needs distinct before/after focus destinations.

Separate authored state from restoration context:

```rust
struct EditContext {
    scene: SceneId,
    selection: SelectionContext,
    vertex_edit: Option<GeometryEditContext>,
}

// Signature-level extension of the existing action-independent history.
EditHistory<Document, EditContext>
```

Each committed history entry records its **before and after contexts** when the
edit occurs. Traversal may continue exchanging document snapshots with the
caller, avoiding two retained document copies per entry, but takes the opposite
context from that entry, never from the scene currently being browsed. Compare
authored state alone to decide whether an edit is effective; a selection or focus
change cannot create history. Generic history remains unaware of scene semantics.

Cancellation restores the transaction's original state/context and preserves
Redo. A zero-change acceptance also preserves Redo. Repeated nudges must update
the coalesced entry's after-context as well as its document result.

History restoration uses the same scene preparation path. Preview the restoration
without consuming the stack, prepare the result, then publish and advance history
once. A failed activation must not lose an undo entry. Start with an internal
candidate/peek boundary; do not build a general distributed transaction protocol.

Deleting the active scene chooses its next neighbor, or the preceding one at the
end. Undo restores the deleted scene and its prior edit context; Redo returns to
the recorded surviving scene. Deleting an inactive scene does not change focus.
Scene create/duplicate activates the new scene, and those before/after destinations
are also recorded. The final scene cannot be deleted.

## Evaluation, camera and performance boundaries

Make these inputs scene-local:

```rust
DisplayFrame::from_scene(scene: &Scene, poses: &AssetFrames)
    -> Result<DisplayFrame>;

render_scene_mesh(scene: &Scene, display: &DisplayFrame, poses: &AssetFrames)
    -> Result<MeshData>;

placed_assets(scene: &Scene, previews: &SceneAssetPreviews)
    -> Vec<PlacedScene>;
```

`GeometryCache::with_assets`, selection eligibility, bounds, snapping anchors,
and transform baselines must receive the same scene scope. Document validation,
ID allocation, resource-reference traversal and save rebasing use all scenes.
Audit each current `.objects` access deliberately; a mechanical flattening hides
wrong scopes. Do not add an accessor named `objects()` with ambiguous meaning.

Our camera uses normalized display coordinates. Its remembered pose must travel
with the exact `DisplayFrame` used to interpret it. Keep a scene's frame stable
through interactions. If a history/import change requires rebuilding the frame,
explicitly rebase remembered camera target and distance through canonical units;
do not reuse camera numbers under a different normalization. In particular, a
tiny model in one scene must not become unpickable because another scene contains
a large environment. Native world-space bounds validation also becomes per-scene.

Pass only the active scene to the existing renderer's `set_mesh` and `set_assets`.
Its one camera/display-frame input already fits this design. Do not pass inactive
scenes merely to retain GPU caches. `PlacedScenes::set` currently evicts omitted
instances; bounded rebuilding on scene switches is acceptable initially. Retain
existing cache behavior within a scene, including Local View. Split shared GPU
source resources from instance residency only after measuring a real need.

Track authored changes separately from view invalidation. Use explicit cache keys
including scene identity and the relevant geometry/pose/display revisions;
activation invalidates view caches without masquerading as a document edit.
Begin with conservative invalidation. No speedup or large-document performance
claim follows from this RFC; whole-document history snapshots remain a known cost.

Resource limits remain layered:

- Native object/topology counts and serialized bytes are document-wide; many
  scenes cannot multiply the existing document budget.
- Decoded source resources, including those retained for Undo, have a bounded
  document-wide budget.
- Evaluated poses/proxies and active rendering have their existing bounded
  working-set budgets. Evict inactive derived frames before rejecting an otherwise
  usable active scene; retain small playback/view records and resolved source
  snapshots. Avoid requiring every scene's GPU data to be resident at once.
- Validate canonical content in every scene before saving, without requiring a
  GPU or re-reading linked files. Missing linked sources remain recoverable
  authored objects as they do today.

## Import APIs: a feasibility check after the core design

Loading should not decide document organization. Separate opening a native
document from discovering external entries, preparing selected content, and
choosing an authored destination. Retain the existing resolver/host filesystem
boundary; scene activation never performs synchronous file I/O.

```rust
struct ImportEntryId(u32); // scoped to one retained import catalog

struct ImportCatalog {
    entries: Vec<ImportEntry>,
    preferred_entry: Option<ImportEntryId>,
    // Retained source snapshot/resources; exact representation stays private.
}

enum ImportDestination {
    IntoScene(SceneId),
    NewScenes,
}

inspect_import(source, resolver) -> Result<ImportCatalog>;
prepare_import(catalog, selected_entries) -> Result<PreparedImport>;
open_native_document(source, resolver) -> Result<LoadedDocument>;

Editor::prepare_import(prepared, destination) -> Result<PreparedDocumentEdit>;
// The host preflights presentation before publishing this ordinary edit.
Editor::apply_prepared_import(candidate) -> Result<ImportOutcome>;
```

Keep this as concrete internal structs/functions. No importer registry, plugin
ABI, generic scene trait, arbitrary metadata dictionary or new crate is needed.
`PreparedImport` owns ordered scene/object drafts, diagnostics and resolved asset
snapshots. It contains no parser objects and does not carry destination IDs.
The editor allocates fresh identities, validates the candidate, and publishes one
history entry. Adapter preparation allocates no destination identities. Editor
preparation may reserve IDs from the transient allocator, but cannot mutate the
live authored document. Publish the same prepared candidate after preflight;
do not re-run import and mint different IDs. The return outcome reports inserted
scene/object IDs explicitly.

Catalog inspection and preparation use one retained source snapshot. Preparing
selected entries must not reopen a path whose contents may have changed since
the catalog was presented; entry handles are valid only for that catalog.

`IntoScene` takes one selected entry for now. `NewScenes` creates one scene per
selected entry in a single transaction. Combining several source arrangements
inside one destination is a separate explicit operation, not an accidental
flatten. Opening a native file restores its document directly; importing native
scenes uses drafts and remaps IDs rather than replacing the working document.
Creation/import activates the inserted scene (the first selected entry for a
batch); the operation records that after-context.

Proposed host policies make opening and importing unambiguous:

| Intent                                | Initial behavior                                                                                                                                                                 |
| ------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Open native document                  | Restore all authored scenes; activate the first.                                                                                                                                 |
| Open external multi-entry scene asset | Build a new unsaved N3 document with one scene per entry in source order; initially activate the preferred entry, otherwise the first. Do not keep an extra empty starter scene. |
| Import into current scene             | Prepare one entry, preferred/first by default; allow choosing a different entry when several are available. Capture the target before asynchronous work.                         |
| Import as new scenes                  | Explicitly select entries, including an All choice; append one scene per entry as one history operation.                                                                         |

These choices can share an import control; they do not require parallel editors
or a dedicated viewer. Loading the catalog discovers whether a choice is needed.
Opening an external asset uses the same preparation path to construct a new
document, rather than inventing a second import implementation. Replacing an open
document still follows the existing unsaved-work policy.

Preflight a multi-scene import's selected scenes one at a time using bounded
scratch evaluation, then retain only the active presentation. A failure rejects
the complete import, not a partially inserted list. Native open continues to
preserve broken links with diagnostics; importing unavailable new content may
fail. Structural native-file errors still reject the open.

An import-entry handle and N3 `SceneId` are never interchangeable. A durable
linked source selector remains asset-owned: the existing source-scene index can
stay in `AssetInstance` for this milestone, with a typed wrapper at the runtime
boundary. Do not invent a universal serialized selector for unimplemented
formats. Future adapters may need richer source-specific selections without
changing `Document::scene`, scene membership, or editor commands. No positional
source handle guarantees identity after external source reorganization.

### Asynchronous target ownership

Replace native `append: bool` with explicit intent:

```rust
enum LoadIntent {
    OpenDocument,
    ImportObjects { target: SceneId },
    ImportScenes,
}
```

Capture the destination ID, document generation, and authored-content guard when
the request starts. A scene switch changes none of the authored content; it must
not redirect the completed import to a newly active scene. Retain the current
conservative stale-document rejection if authored content changed. Missing
targets, replaced documents and pending interactions reject publication with
feedback instead of retargeting or silently accepting a transform.

On a successful import into the original target after the user browsed elsewhere,
activate that target to show the result. Record its remembered selection as the
before-context and its inserted selection as the after-context: Undo and Redo
both show the target, without making the intervening browsing part of the edit.
Import-as-new-scenes instead records the prior scene and first inserted scene
as its two focus destinations, like scene creation. This is an explicit
first-version completion policy; it avoids invisible edits and does not infer
the destination from the active scene.

### Preview resources and playback

Keep decoded `Arc<SceneAsset>` resources shared document-wide. Key transient asset
previews by `(SceneId, AssetInstance)` rather than just `AssetInstance`. Existing
duplicates within one scene may continue sharing playback; different authored
scenes do not share their time/clip/exposure merely because they reference the
same source. This is preview scope, not an authored timeline design.

Tick active-scene previews only. On deactivation pause advancement, preserve the
position/play preference, and reset the clock on return so elapsed wall time does
not cause a jump. Scene duplication starts previews at rest/default exposure;
it copies authored content, not transient playback. Evicted poses can be
re-evaluated from retained resources without reopening source files.

This requires a focused runtime split: today's `SceneView` holds playback plus a
mandatory `Arc<EvaluatedScene>`. Separate its small preview record from optional
pose residency. `SceneAsset` also retains a cached rest pose; account for that
cache and make it evictable when needed. Dropping a UI reference alone does not
prove that the evaluated allocation has been released.

## Compatibility review

The mappings below are proposed N3 adapter policies inferred from primary source
contracts. They are not claims of new format support or round-trip identity.

| Source            | Source semantics                                                                                                                                  | Fit to the proposed boundary                                                                                                                                                                                                      |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| OBJ and native N3 | Our current OBJ adapter returns editable objects; N3 owns the native document schema.                                                             | OBJ offers one draft. Native open restores scenes; native import remaps selected scenes/objects. Neither needs a GLB-shaped core API.                                                                                             |
| glTF / GLB        | glTF permits no scenes, an optional default, and roots shared by multiple scenes. GLB packages glTF rather than defining a different scene model. | Catalog source scenes as entries. Each selected entry can become a linked object in an existing N3 scene or in its own new N3 scene. Shared source roots remain inside immutable resources; authored placements stay independent. |
| OpenUSD           | A `UsdStage` presents a composed scenegraph from contributing layers.                                                                             | A resolved stage/configuration can supply an entry. Layers and variant choices are not automatically N3 scenes. A future adapter owns composition policy.                                                                         |
| USDZ              | A package's default layer, when present, is used as the root layer when opened as a stage.                                                        | Packaging belongs to resource resolution. Do not turn every file/layer in the archive into a scene tab.                                                                                                                           |

Sources: [glTF scenes](https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#scenes),
[OpenUSD object model](https://openusd.org/release/api/_usd__page__object_model.html),
and [USDZ specification](https://openusd.org/release/spec_usdz.html).

The glTF catalog can preserve source order and expose its preferred entry without
making either an authored identity. Importing one entry initially selects the
preferred entry or first available entry. Import-all uses source order. A source
with no importable entries returns an explicit no-entry result. Today the glTF
adapter deliberately synthesizes a parentless-root preview when no source scene
is declared and emits a warning. Preserve that existing fallback as a clearly
identified derived import entry, not a claimed source-authored scene; do not
silently change this behavior as part of the native model migration. Empty named
scenes, duplicate names, shared roots and the synthetic entry need separate tests.

The current decoder also maps an absent source default to index zero. Catalog
discovery must retain the original optional hint separately: an evaluator's
fallback index is not proof that the source declared a preference. Apply the
first-entry fallback in import policy and test absence versus an explicit default.

For native persistence, first-scene reopening remains the proposed N3 policy;
an external preferred entry is an import-selection hint, not last-visited state
silently saved into the document. A future authored entry-scene feature can be
added if product requirements justify it.

The existing `SceneAsset::evaluate(source_scene, sample)` handles an immutable
node forest, clips, skins and morphs. It stays behind the imported asset boundary;
it is not the evaluator signature for authored N3 scenes. Full USD composition
would require a deliberate resolved-snapshot conversion or another asset
evaluation backend that produces supported presentation data. Changing a parser
alone cannot establish USD semantics. Neither route requires adopting USD layers,
paths, variants, or its authored graph as N3's scene model.

## Migration and implementation sequence

Introduce native document version 2 explicitly. Decode version 1 into a migration
type and wrap its unchanged objects in `Scene 1`, preserving IDs, recipes,
coordinates, units and linked references. Reject unknown versions and invalid
mixed schemas. Keep original file bytes for save-conflict detection; opening
never rewrites the file. Saving writes v2, while the migrated loaded document is
the clean comparison baseline until an actual edit.

Recommended sequence; each gate has its own focused tests before integration:

- [ ] **1. Scope existing APIs.** Separate object evaluation from document lookup;
      identify active-scene consumers versus all-document traversal. Preserve
      today's behavior and guide output. No speculative multi-window API.
- [ ] **2. Authored model and codec.** Add scenes/IDs/validation and v1 migration;
      adapt fixtures/builders. Introduce explicit scene mutation targets and
      monotonic identity allocation. Native geometry alone proves the design.
- [ ] **3. History and activation.** Add entry-owned contexts, pending-session
      guards, prepared restoration, scene-local selection/view memory and display
      frames. Create/rename/duplicate/delete through the common lifecycle.
- [ ] **4. Unified UI and renderer.** Add scene controls above Layers and active
      scene submission to the existing viewport. Test rulers, Local View,
      picking, read-only permission and cache invalidation across switches.
- [ ] **5. Adapter integration.** Separate catalog/preparation/destination,
      capture asynchronous targets, support selected/all source scenes, and scope
      playback/resources. Save, Save As, reopening and failures cover all scenes.
- [ ] **6. Executable guide and final review.** Land the native scene workflow
      and imported-scene additions, regenerate/review media, run `just verify`,
      and rebuild the native app. Record actual results rather than carrying
      forward earlier single-scene test counts.

Stages describe the eventual scope. The readiness review above gates proceeding
beyond the initial integrated proof. They are not separate editor implementations
or permission to publish an incomplete persistence change.

| Current owner                                               | Required change                                                                            | Main risk                                                              |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------- |
| `model/document.rs`, `asset_geometry.rs`                    | Scene-scoped geometry/bounds; all-document structural validation; explicit codec migration | Wrong scope silently joins unrelated scenes or drops inactive objects  |
| `editor/mod.rs`, `edit_history.rs`, `geometry_edit.rs`      | Explicit target, entry context, session and ID ownership                                   | Redo focuses the wrong scene; primitive provenance leaks across scenes |
| `ui/workspace_ui.rs`, `local_view.rs`, `asset_instances.rs` | One active scene with separate remembered view/preview state                               | Camera normalization, held input, Local View, stale selection          |
| `asset_io/linked.rs`, `document.rs`, `native.rs`            | All-scene reference traversal; import intent/preparation/publication                       | Wrong async target, ID collision, partial import, broken Save As links |
| `render/renderer.rs`, `placed_scenes.rs`                    | Active-scene submission and bounded cache residency                                        | Mixing display frames or defeating existing Local View caches          |
| Documentation scenarios/builders                            | Explicit scene targets in setup; production controls in illustrated operations             | Tests accidentally keep proving only the former flat document          |

History/activation is the highest-risk part. Most geometry math, material
rendering and imported pose evaluation should survive unchanged. This is a
feasibility review against actual call sites, not a compilation or runtime proof
of the proposed signatures.

## Acceptance and docs-driven development

Use the existing [DDD skill](../.agents/skills/docs-driven-development/SKILL.md).
When implemented, add one `scenes` feature with a template, scenario, and registry
entry. First demonstrate the experience with native geometry: create an
alternative, change it, switch back, then undo/redo across scenes. Add source-entry
import behavior to the existing imported-assets guide. New scene controls use
live control bindings; do not introduce shortcuts just to simplify replay.

Required behavioral regressions beyond the illustrated guide:

- [ ] Legacy empty/mesh/primitive/linked documents migrate without geometry or ID
      changes; original disk bytes still detect save conflicts; Save As rebases
      links from inactive scenes too; missing links survive save/reopen.
- [ ] Reject zero/duplicate scene IDs, duplicate object IDs across scenes, unknown
      targets, final-scene deletion, ID exhaustion and aggregate budget overflow.
      Repeated scene names and empty scenes are valid.
- [ ] Editing A, browsing B, Undo, then Redo focuses A on both traversals. Scene
      deletion/creation/duplication restores the correct before/after scene.
      Browsing, cancellation and no-op acceptance preserve Redo and clean state.
- [ ] Released transform previews, property drags and active input block scene
      switching without losing their baseline. Ordinary vertex-edit exit keeps
      primitive preservation/materialization behavior. No held key affects the
      destination scene after switching.
- [ ] A tiny object in A and a distant large object in B retain independent camera
      framing, rulers, selection and snap behavior. No inactive objects enter
      selection, statistics or Local View. Camera restoration handles a rebuilt
      display frame explicitly.
- [ ] Import begun in A and completed after browsing B still targets A; document
      replacement, deleted targets, pending sessions and stale authored content
      cannot publish the result. Multi-entry failure leaves no inserted scenes,
      changed focus, history entry or replaced GPU result.
- [ ] Shared decoded resources do not imply shared authored objects or cross-scene
      preview time. Inactive playback does not advance or jump on reactivation.
      Evicted poses restore without re-reading source files; resource accounting
      remains bounded across duplicates and Undo.
- [ ] Multi-entry fixtures cover source order, absent/non-first preference,
      duplicate labels, empty entries and shared roots. Native-only scenes remain
      fully usable without importing any external scene format.
- [ ] Read-only mode can browse every scene but cannot modify scenes or objects.
      Invalid activation/restore preserves the prior state and history cursor.

Review animated switching and undo in the browser, not only byte comparisons.
Run native `just docs update` and `just verify` for the implementation; this
planning-only change requires authored-document formatting and link review.

## Deferred decisions and non-goals

Shared editable objects/collections across scenes, scene nesting/instancing,
cross-scene constraints, grouping, multiple windows, authoring cameras/timelines,
default-scene selection UI, persistent workspace/view layouts, per-instance clip
overrides, streaming schedulers and graph/delta history are separate work. Keep
the existing topology and material boundaries. No GLB export, USD import, or
format round-trip guarantee is implied.

The proposed first-scene reopen rule, explicit Apply/Cancel before switching,
and focus-on-completed-import rule are concrete defaults for UX review. They can
change without changing the ownership model or interchange adapters. There is no
remaining format-driven blocker to beginning the native-scene foundation.
