# Architecture

N3 is one Rust package with one native application and an executable documentation
harness. Module boundaries are useful now; separate crates would imply an API
contract that the editor does not yet have. The package is not a published SDK.

## Source layout

| Territory            | Responsibility                                                                           |
| -------------------- | ---------------------------------------------------------------------------------------- |
| `src/model/`         | Canonical documents, geometry evaluation, units, validation, and file-format support.    |
| `src/editor/`        | Selection, editing sessions, history integration, transforms, and derived picking state. |
| `src/input/`         | Physical input identity, semantic shortcuts, ownership, and shared navigation routing.   |
| `src/render/`        | Camera projection, derived mesh data, GPU resources, and scene rendering.                |
| `src/ui/`            | egui panels and viewport controls, properties, gizmo, rulers, and feedback.              |
| `src/documentation/` | Executable scenarios, virtual input, annotations, captures, and media generation.        |
| `src/native.rs`      | Window lifecycle, native events, dialogs, filesystem effects, and GPU presentation.      |
| `src/settings/`      | Global preferences, validation, merging, and the host storage interface.                 |
| `src/main.rs`        | Composition and application entry points.                                                |

The composition root retains internal module aliases so implementation modules
can refer to each other without a broad import rewrite. They remain private to
the application crate. These aliases are not public kernel exports, and the
directory layout alone does not enforce a strict dependency graph.

Authored guide templates live in `docs/templates/`; their generated Markdown and
WebP media live in `docs/guide/`. Architecture decisions live in
`docs/architecture/`, research in `docs/research/`, canonical fixtures in
`fixtures/`, and example documents in `examples/`. Regenerable binaries, bundles,
build caches, and local review output do not belong to the promoted source tree.

## Authorities and derived state

The `.n3.json` document is the geometry authority: centimeters, object transforms,
primitive parameters, authored polygon topology, and explicit loose edges whose
endpoints reference stable vertex IDs. Render normalization, GPU
triangles, normals, selection colors, camera state, input state, and history are
not serialized into that document. Primitive evaluation and triangulation must
not become competing editable representations.

The editor owns the working document and its edit lifecycle. A preview uses an
immutable session baseline, validates a candidate before publication, and either
accepts one history entry or restores the baseline. The generic history component
does not perform geometry validation or refresh caches. Input and properties
controllers reuse this boundary rather than recording each intermediate value.

Geometry and picking caches follow document revisions. Selection feedback has its
own identity and is synchronized after mesh replacement; selecting vertices must
not trigger geometry uploads. Original polygon boundaries and object-scoped
vertex IDs survive in render metadata so face highlighting cannot accidentally
use triangulation diagonals or another object's matching local IDs.
Loose edges share that topology and the line buffer, with a separate draw range
so unfilled geometry remains visible when surface edge overlays are disabled.
Segment picking uses the ordinary object selection path and surface occlusion;
an unfilled Circle has no selectable interior. Its vertices use the existing
edit session, materialization, and undo lifecycle.

`Editor` currently combines editing with egui-space picking and viewport painting.
`WorkspaceUi::viewport` is the full scene rectangle shared by GPU render sizing,
camera projection, and geometry picking. `viewport_ui_rect` is the usable area
for controls and pointer input, inset when the 2D ruler is visible. The ruler
overlays the scene and measures against the full viewport, so toggling it moves
floating controls without moving or resizing the rendered geometry. The editor
receives both the full projection rectangle and the inset interaction response.
`ViewNavigation` owns 2D/3D mode independently of the interpolated camera angle.
All pointer zoom sources pass their current cursor through shared navigation
routing. `ViewNavigation` chooses cursor anchoring in 2D and target-centered zoom
in 3D; `Camera` owns the target-plane anchor math. Anchor coordinates use the full
scene rectangle, independent of ruler visibility. Command-scroll selects zoom
in 2D, and modifier changes release the previous scroll action latch.
The 2D ruler remains visible while that mode is planar and the live projection is
orthographic, including axis animations and gizmo holds. Its measurements use
the live projection. Camera orientation requests share destination comparison,
so repeated named views and gizmo snaps do not start or restart animations.

Document evaluation also produces CPU mesh structures shaped for rendering.
Neither is advertised as a UI-independent kernel. Before exposing a public API,
make document mutation and cache invalidation enforceable through that API.

Transform endpoints are small derived 3D meshes in `render/transform_gizmo.rs`:
cones for Move and object-aligned cubes for Scale. The editor projects the shared
triangles and ray-picks the same surfaces; overlapping solids select the nearest
face. A five-point screen-space margin around endpoint silhouettes accepts near
misses, with direct surface or shaft hits taking priority over padded targets.
Axis shafts retain their forgiving pointer tolerance and plane controls
retain priority. Screen sizing uses logical UI points at the endpoint's depth.
Locked-axis affordances unproject their screen anchor, including the end-on
fallback, so painting and picking use the same position.

`render/gizmo_overlay.rs` draws the solids and shafts through a small wgpu pass.
It clears depth after the model pass, keeping handles visible over the model
while correctly occluding their own faces. It reuses the viewport color and
depth attachments, retains one bounded vertex buffer, and uploads only changed
render data. UI controls, rulers, and menus composite afterward. Native frames
and documentation captures feed this same pass; these meshes never become
document objects, selection entries, or history edits.

`render/shading.rs` defines viewport shading independently of documents and
history. Solid remains the default. Wireframe reuses authored polygon edge
buffers with a line pipeline, avoiding triangulation diagonals and optional GPU
polygon-mode features. A colorless surface pass preserves the depth information
used by edit feedback; all base wire edges remain visible. Shading does not
change the independent X-ray selection policy.
Shading is transient viewport state, separate from the persisted Solid edge
preference. Material preview and rendered shading await a material/lighting pipeline.

X-ray is transient viewport state owned by the editor's explicit selection
occlusion policy. `VisibleOnly` retains surface occlusion; `Through` expands
eligible projected candidates without expanding the current object scope.
Local View filtering and single-object vertex editing still apply first. Box
selection, vertex picking, Select All, and vertex cycling share the policy.
Point picking prefers the nearer of overlapping projected vertices, with a
stable identity tie-break. Switching policy filters cached projected candidates without rebuilding geometry
or changing committed history, and does not remove previously selected occluded
vertices. Cached projections retain each candidate's depth and occlusion state.

The same X-ray state reaches the scene renderer and component overlays. Solid
uses faint surface shading and subdued occluded topology as an editing cue;
Wireframe remains unfilled. This is separate from future material transparency,
which must establish its own rendering contract. The semantic `ToggleXray`
action is shared by the View menu and the `view.xray` binding. Shortcut routing
keeps text, popups, and active pointer gestures in control of their input.

`input/pie_input.rs` owns the shared held-key lifecycle for View and Shading:
ordered press/move/release, cancellation, and exclusive frame ownership. Each menu
supplies its choices to the common layout/painter in `ui/pie.rs`. The Shading
trigger is eligible only in Cursor; transform tools retain exclusive ownership
of Z for axis locking, even without a selection. Tool changes earlier in the same
input batch use the binding catalog to resolve this context before a pie opens.

Global preferences use a separate typed model and byte-storage interface. The
native host owns the user path, file synchronization, and text-editor launch;
the UI and settings controller do not perform filesystem I/O. See
[user settings](user-settings.md) for the merge and recovery contract.

## Transient notifications

`ui/toast.rs` owns the **toast** component and its bounded queue. Callers enqueue
dynamic text with `state.toasts.push(Toast::new(message))`. Optional builders add
Info, Success, Warning, or Error severity, a duration, and an action:

```rust
state.toasts.push(
    Toast::new("Choose every visible object")
        .action("Select all", Command::SelectAll),
);
```

`push` returns a stable `ToastId`; identical queued or visible messages refresh
the existing entry and return its ID. `update(id, toast)` replaces an entry in
place and resets its reading time. `dismiss(id)` removes one, and `clear()` removes
all. Updates and dismissals return false for stale IDs. `Toast::persistent()`
stays until dismissed or updated, so a producer can retain an ID for progress and
replace that message with timed success or failure feedback. There is no separate
async task framework inside the toaster.

Actions carry semantic commands rather than closures that can mutate the editor
outside its lifecycle. `WorkspaceUi::take_ui_commands` drains clicked actions
after egui finishes all layout passes; native and documentation hosts dispatch
them through the same command path, including native host effects. Toast state
is separate from documents, history, and persisted preferences. Existing errors
that need explicit dismissal retain their error overlay.

The stack uses the viewport UI rectangle, the active theme, and the shared popup
shadow. At most three notifications are visible, with the oldest at the bottom;
later arrivals queue. Reading the stack pauses expiry and promotion so controls
stay in place. The queue holds at most 35 entries: three display candidates and
a 32-message backlog. Overflow drops the oldest backlog entry, preserving the
three candidates even when a small viewport shows fewer. Waiting entries retain
their full duration. Long messages wrap.
Menus, Preferences, and active viewport gestures hide and suspend the stack.
The egui clock accounts elapsed time according to the preceding interval's pause
state, then applies the new state. Repaint scheduling governs expiry, including
deterministic guide replay; no wall-clock timer or background task is needed.

`notifications.focus` (F6) transfers focus from an idle viewport to the first
toast's action, or its close button. Escape returns to the previous widget
without dismissing; closing or acting on a focused toast also restores focus.
Text fields, popups, and gestures retain their input ownership. F6 avoids the
existing Option-held navigation and viewport Tab selection bindings. Native
screen-reader announcements remain deferred: N3 does not yet wire the AccessKit
host adapter, and widget labels alone are not live announcements.

**Shortcut hints** are one producer, not a second keymap. The keyboard router
recognizes an explicit unfamiliar input only after ordinary bindings and input
ownership have had priority. The first example is plain A suggesting the
canonical `selection.all` binding. Its message and optional action use that
binding's current label and command. A hint appears once per application session;
arbitrary unbound keys remain silent. Future real bindings take precedence over
hints, and existing aliases retain their behavior.

## When to split crates

Split only when a second host or independent headless consumer needs a stable
pure model/editing interface. First separate evaluation output from render
adapters, and selection/transaction operations from egui coordinates and paint.
A crate split should enforce those verified seams, not merely move the existing
coupling across package boundaries. Browser delivery remains future work.

## Evidence boundaries

Native and documentation hosts use the same UI, semantic input routing, and scene
renderer. Deterministic replay proves application routing, state transitions, and
rendered outcomes. It does not prove macOS gesture recognition, actual device
feel, file-dialog behavior, or window-system delivery. Keep focused native/manual
acceptance alongside headless assertions and visual review of generated media.
