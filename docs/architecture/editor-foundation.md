# N3 editor foundation

Implementation checkpoint, September 29, 2026. The editor is now maintained in
the root package under `src/`. Integrated checks passed for editing,
synchronized object feedback, three-button mouse controls and multiple-object
selection, keyboard selection, deletion, temporary Space pan and Alt orbit tools,
number-key navigation, selection fitting and the held-Backtick view pie.
The pie tutorial capture was visually reviewed.
Native interaction acceptance remains ongoing.
This document describes implemented
boundaries and deferred ideas. See the [promotion review](milestone-review.md)
for the current source-organization boundary.

## Implemented first slice

- The application is **N3**, with executable `n3` and local bundle `N3.app`.
  Implementation lives in `src/` and the generated user guide in `docs/guide/`.
  Local application bundles remain ignored under `build/`.
- The canonical saved state is readable `.n3.json` text. Agents can edit it
  directly and use `--check PATH` to validate it without opening the GUI.
- Opening an OBJ imports supported geometry into an N3 document. Saving that
  document uses its own filename and does not overwrite the source OBJ.
- Preserve parameterized geometry after insertion. A cube, cylinder, cone or
  torus remains a primitive while inspecting or selecting its vertices. The
  first effective vertex edit materializes a mesh as part of that operation.
  Heart and gear remain ideas, not initial requirements.
- Shift-I opens the existing Insert menu without changing geometry or selection.
  A menu item performs insertion; Escape dismisses the menu. Repeated held-key
  events do not toggle it, and focused fields or existing popups keep their keys.
  Plain I remains unassigned.
- Component editing supports vertices and a deliberately small operation set.
  Make Face adds one triangle or a simple planar polygon from a single existing
  boundary. Edge/face component modes and broader topology construction are deferred.
- V and Q both select Cursor (the existing `Tool::View`), W Move, E Scale,
  and R Rotate. G also selects Move and S also selects Scale for Blender users.
  Cursor hides transform handles and preserves geometry, selection,
  mode and history. Cursor is selected on startup.
  Idle Enter toggles a selected
  object between object and vertex mode; with no selection it does nothing.
  Double-clicking an object enters vertex mode; double-clicking actual empty
  viewport space exits while retaining the object. An object surface away from
  vertices is still geometry, not empty space. Mode changes alone never convert
  a primitive.
  Done and Shift-Enter leave editing directly. Escape backs out contextually,
  including clearing vertex and object selections. Tab cycles model selection
  while the viewport has actual keyboard focus; otherwise it navigates the UI.
- Command-A selects all document objects in object mode, or currently visible
  vertices of the edited mesh. Delete and Backspace delete that selection as
  one undoable document operation. The viewport menu exposes both commands.
- Without an explicit transform axis lock, left-button viewport drags select in either mode with every tool. Middle-button
  drags pan and right-button drags orbit in either mode. Explicit transform
  handles retain their own drag behavior.
- Hold Space before a primary drag for a temporary hand tool in either mode.
  Grab and Grabbing cursors show ready and active panning. Releasing Space stops
  the pan and returns to the persistent tool without changing selection.
- Hold Alt (Option on macOS) before a primary drag for temporary orbit. Holding
  the key or clicking without motion leaves Planar mode unchanged; actual orbit
  leaves Planar from the visible pose and retains projection. Space wins when
  both are held before a fresh press. The established drag keeps its owner until
  its initiating modifier or button is released; there is no mid-press handoff.
  Remaining button events after modifier release are consumed. The binding is
  resolved through viewport gesture context, leaving a future object-duplicate
  context possible without implementing that behavior now.
- Select visible vertices only initially. X-ray/select-through and a dedicated
  wireframe mode are deferred.
- Use conventional, explicitly constrained movement. Automatic axis inference
  or locking from gesture direction is a deferred UX idea.
- OBJ imports retain mesh geometry and polygon topology. Materials, UVs, authored
  normals, standalone lines/points and free-form records are not retained.

## Object and geometry responsibilities

An object has an ID, a name, a transform, and a geometry definition. Its geometry
is one of two alternatives:

- A parameterized primitive, with its kind, full XYZ dimensions, segment counts,
  and recipe-specific settings. Circle exposes Radius, Vertices, and Fill;
  it defaults to an unfilled 32-vertex loop in local XY. Its stored X/Y dimensions
  are equal diameters, and Fill produces one polygon using the same vertex IDs.
  Evaluated vertices, edges, and faces are derived data.
- An editable mesh, with stable vertex and face IDs, object-local `f64` positions
  and ordered polygon boundaries. Optional explicit loose edges reference pairs
  of vertex IDs and cannot duplicate face boundaries. IDs are scoped to an
  object's mesh. Its render triangles are derived data.

These alternatives use a small tagged Rust enum and corresponding JSON records.
`Document.version` is **1**, binding both the schema and primitive evaluation
recipes. There is no separate `generator_version` field. A future recipe change
that changes evaluated geometry requires a document migration or version change.
A node editor, modifier stack and general graph execution framework are deferred.
The additive `edges` and Circle `fill` fields default to empty and false in
version 1, respectively, and are omitted when at those defaults. Existing
documents keep their evaluated geometry and serialized recipes.

Object translation, rotation and scale preserve the primitive. Changing its
dimensions or segment counts changes its parameters. Effective vertex editing
materializes a mesh; primitive parameters and manually edited mesh positions
must never be competing authorities.

Implemented conversion behavior:

1. **Edit vertices**, Enter, or double-click opens either geometry kind. For a
   primitive, retain its source recipe and evaluated baseline in an editor-owned
   derived edit session. Entry and component selection change no document data,
   revision, saved state, or history.
2. Selection and feedback use the evaluated mesh and its stable IDs. The first
   effective vertex edit publishes a validated mesh candidate through the
   ordinary operation transaction. Conversion has no separate undo entry.
3. Keep the object's ID, name, transform and object selection unchanged. Undo
   restores the source recipe while retaining vertex edit mode; redo restores
   the edited mesh and component IDs.
4. Returning exactly to the evaluated baseline restores the exact source recipe.
   Equality includes positions, IDs, polygon topology, and explicit edges, not an approximate
   visual resemblance. Leaving an unchanged edit visit retains all parameters;
   leaving a changed visit retains the mesh and removes its primitive fields.

Generated vertex array indices are not persistent component identities. A
segment-count change can replace the generated topology. Primitive parameter
fields are shown in object mode, while component mode uses its captured
evaluation. It does not try to carry component selections across procedural
topology changes. The reusable source/derived-value session is independent of
geometry and input; see [edit transactions](edit-transactions.md).

## Text and editing foundation

The `.n3.json` format stores a self-contained list of objects with stable IDs and
deterministic formatting. It persists current document state; camera, active
tools, selection, unfinished interactions, preferences and undo history remain
in the editor session. Transforms store `f64` translation and scale plus a unit
quaternion in XYZW order.

One canonical N3 length unit is **one centimeter**. Version 1 includes additive
`length_unit: "cm"` metadata. Missing metadata in older version-1 files defaults
to cm without modifying coordinates; unsupported canonical units are rejected.
This gives every document the same length meaning while keeping display choices
separate. Coordinates and sizes remain `f64`; fractional centimeters are valid.
Default movement snapping is an interaction policy, not document quantization.
Viewport movement and Properties scrubbing use one configurable centimeter-grid
resolver. Auto chooses clean 1/2/5 decimal steps from interaction scale: viewport
zoom/depth for handles, explicit sensitivity for numeric scrubs. A gesture freezes
its resolved step; fixed spacing remains available. Typed input and imported/saved
geometry remain unchanged. Arrow nudges retain their explicit physical increments.
See [movement snapping](movement-snapping.md) for constraints, session boundaries,
and the reserved geometry-snapping seam.

The Properties display-unit preference converts lengths at the UI boundary:
cm, mm, m, in or ft. It resets to cm on New/Open and is not serialized. Rulers
use that same choice. Switching units must preserve canonical values, untouched
drafts, document revision, dirty state and both history branches. Fields write
converted centimeters back only after an actual input change. Explicit suffixes
override the chosen unit; plain numbers and simple fractions use it. General
arithmetic expressions are not evaluated. Angles and scale remain separate.
Length fields use round-trippable numeric text, with scientific notation for
very small or large values: focusing and blurring an untouched field must not
round its canonical draft or turn a fractional distance into zero.

OBJ source coordinates and position indices are preserved without welding
coincident vertices. Raw OBJ numbers are interpreted as centimeters without
automatic rescaling. Nonempty object/group sections become separate objects,
each owning its referenced geometry. The up-axis convention is Y-up. Display
normals are generated per face. See the [length-unit guide](../guide/length-units.md).

A separate `DisplayFrame` centers the document and uniformly maps its largest
extent to 2.0 for rendering and picking. It stays fixed during an edit; Frame
recomputes it from document bounds. Camera fitting, Z-up display conversion,
triangulation and GPU buffers never rewrite canonical coordinates.

A shared validator checks syntax, document version, unknown fields, IDs,
references, finite coordinates, primitive parameters, invertible transforms and
supported polygon geometry. Native saving serializes a validated snapshot to a
temporary file before replacing the destination. It compares the last-read or
saved bytes to detect external changes; Save as selects another destination.
The native host handles file dialogs and unsaved-change prompts.

The [edit transaction contract](edit-transactions.md) is an action-agnostic
core boundary. `EditHistory<T>` owns committed snapshots and one immutable
preview baseline; the caller owns working state, validation and restoration.
Move's interaction session uses this mechanism. Other preview features must not
depend on Move's axes or input controls, and preview updates must not record
undo entries or clear redo. Cancellation and unchanged acceptance preserve
committed history. Undo or redo during a transaction only cancels it; a later
invocation can traverse committed history.

Handle mutations have preview, commit and cancel semantics. An unlocked complete
drag produces one undo entry; a zero-change interaction does not. An axis-locked
Move session keeps its original baseline across pointer releases, arrow nudges
and axis changes. Enter or a viewport double-click applies the complete session
as one undo entry and clears the axis. Escape restores the baseline and clears
the axis, including while a pointer drag is active. Committed changes
retain at most **64 undo snapshots**. Cancellation restores the starting state.
Invalid evaluation or triangulation rejects the candidate before publication.
Outside a locked Move session, Enter finishes an active transform or marquee
through the same completion path as pointer release. It keeps the visible result
and remains in the current mode;
a later pointer release cannot commit it twice. The next idle Enter toggles mode.

## Interaction design discipline

Controls presented as toggles should work symmetrically: the same control should
return to the prior mode while preserving the relevant object and tool. Active
gestures need a clear confirmation step before a mode switch. Primitive
conversion belongs to the first effective vertex edit, and focused UI keeps its input
ownership. This is guidance for future controls, not a claim that every command
already has a reversible counterpart.

Changing the primary tool bindings remains a deferred idea. The current V/Q
cursor aliases, W/E/R primary transform bindings, and G/S transform aliases
remain in use.

The Space hand tool is a transient input state, not a replacement for the
persistent editor tool. It affects primary dragging and preserves model geometry
and selection. Space-up stops panning immediately and consumes the remaining
primary release, preventing a navigation gesture from becoming a selection
click. A marquee or transform begun first retains ownership when Space arrives
later. Between released transform drags, Space pan and other camera navigation
preserve the pending geometry, axis lock, and original edit baseline. Fields and popups keep Space,
focus loss clears the hold, and existing
two-finger gesture mappings remain unchanged.

## Selection and the third dimension

Top-row number keys and numpad keys are separate input sources. Top-row 1 selects
Perspective, 2 Front, 3 Right, 4 Back, 5 Left, 6 Top, and 7 Bottom. All six
orthographic directions are available in the View menu. Numpad 0/1/3/7 select
Perspective/Front/Right/Top; the opposite directions have no direct numpad binding.
Numpad 4/6 orbit left/right and 2/8 down/up in 15-degree steps without changing
projection; numpad 5 toggles projection without changing orientation.
Shift-top-row-1 performs the existing Frame all operation. Shift-top-row-2 and
the Frame selection menu command fit the selected object set or selected
vertices. Selection fitting preserves orientation and projection, centers a
single point without changing zoom, and does nothing for an empty selection.
It uses derived display coordinates and does not rewrite canonical geometry,
selection or history. Numeric navigation and fitting yield to active editor
gestures and normal keyboard ownership.
Plain F invokes Make Face in idle vertex editing; G aliases Move. Neither is a
view shortcut. Frame all uses Shift-top-row-1 or its UI command, and the Grid
checkbox in Preferences controls grid visibility.

The view pie reuses those view and fit actions. Holding Backtick over the
viewport opens seven choices: Top north, Front northwest, Back northeast, Left
west, Right east, Frame selection southeast, and Bottom south. The southwest
slot is empty. Hover has no camera effect; key release applies the pointed
choice. Returning to the opening point or pressing Escape cancels without
changing selection or the persistent tool.
Frame selection uses the current object/vertex selection context and is disabled
when that selection is empty. Focused text fields and existing popups retain
keyboard ownership.

Object selection and hover are session state shared by the viewport and Layers.
The selected object IDs form a set; a separate active ID remains a member of
that set. A plain click replaces the set with one object. Object box selection
matches intersections with projected object bounds and requires at least one
visible vertex per object. This is a vertex-sampled visibility policy; a visible
face fragment without a visible vertex does not qualify. Shift-click toggles set
membership, and Shift-box selection adds to it. Box selection leaves the current
set and active object unchanged until pointer release or explicit Enter confirms
it. Escape cancels a box without adding history. Vertex editing requires one
selected mesh.

`Editor::box_selection` holds a `BoxSelectionPolicy` with two independent
choices: `SelectionTiming::{OnRelease, Live}` and
`BoxHitPolicy::{Intersects, Contains}`. The defaults are `OnRelease` and
`Intersects`. A gesture captures both policies when it starts,
so configuration changes cannot alter an in-progress box. Visibility eligibility
is independent of the hit relation; intersection does not enable x-ray selection.
Both hit relations include point vertices identically, while vertex selection
uses the same timing policy as object selection.

Viewport-focused Tab selects the first eligible item when selection is empty,
advances through Layers/document order for objects or source vertex order for
visible vertices, and wraps. Shift-Tab reverses that order and starts at the last
item when nothing is selected. Cycling replaces the current selection with one
item and remains session state, without document history. Text fields, popups
and modals own their keyboard input; a hovered viewport alone is not focus.

Command-D and the viewport Duplicate command copy the selected objects in place,
appending them in document order with fresh object IDs and ` copy` names. Each
copy owns its geometry; mesh vertex/face IDs remain valid within the copied
object's scope, and primitive recipes remain live. Originals and transforms are
unchanged. Only the copies become selected, with the active source mapped to its
copy. The operation is one atomic undo entry, including selection restoration,
and starts no transform. Empty selection, vertex mode, and unfinished editor
interactions (including an armed Move axis) make duplication unavailable.

Vertex deletion removes selected vertex records and every incident face or loose edge. It
preserves unselected vertices, surviving polygon faces, object identity and
transform, including an object left empty. It performs no implicit welding,
hole filling or topology repair. Object deletion removes the complete selected
object set. Each deletion is one atomic undo entry; undo restores IDs, geometry
and selection, and redo repeats the same mutation.

Make Face is a semantic immediate edit, bound to F and available in the vertex
editing viewport context menu. Selection is an unordered set of stable IDs:
three distinct non-collinear vertices define a triangle; larger selections must
define one unambiguous closed cycle of existing loose edges or open surface
boundary edges. The command authors one polygon, consumes any explicit loose
edges that become its boundary, and leaves all vertex positions and IDs intact.
Concave planar cycles are supported. Nonplanar, crossing, or ambiguous cycles
are rejected rather than sorted by selection order, repaired, or flattened.
An existing matching face is a no-op.

Adjacent face winding determines the new face direction when consistent. An
isolated face uses positive dominant object-local normal orientation, independent
of the camera. Local checks reject duplicate faces and adding a third incident
face to a boundary edge; they do not prove global manifoldness or that the new
face intersects no other surface. Existing imported non-manifold geometry remains
valid under the document's existing contract.

The complete candidate passes the ordinary document/render validation and edit
commit path. One successful Make Face is one undo entry, including primitive
materialization; Undo restores the original primitive recipe. Invalid and no-op
attempts leave the document and history unchanged. Active gestures, property
edits, and pending transform sessions retain input ownership. The executable
[Make Face guide](../guide/make-face.md) demonstrates filling an unfilled Circle,
orbiting to inspect it, and undoing/redoing the operation.

Hover previews are suppressed during selection drags and resume after release,
so pointer feedback does not compete with the marquee.

Every selected object has the stronger blue silhouette; hover uses a thinner,
subdued blue outline. Selection wins for the same object, and selection of one
object can coexist with hover over another. Hovering a visible Layers row or a
visible surface resolves to the same object ID and updates both surfaces.
Moving onto empty viewport space clears hover. Object outlines are suppressed
in vertex mode, where the edited object's Layers row stays selected. Outlines do
not depend on the polygon-edge overlay. Neither hover nor
selection modifies the document or its undo history.
Loose edges participate in object hover and selection through their visible
segments, with the same surface occlusion policy. An unfilled Circle's interior
does not count as a surface hit. Its vertices can be selected and edited through
the ordinary component tools; materialization preserves explicit edges.

The first slice uses visible-only selection and explicit transform constraints:

- A click selects a visible vertex; Shift-click toggles its membership.
- Without a transform axis lock, a left-button viewport drag draws a rectangular selection marquee, even when starting
  over an already selected vertex. It does not move vertices or the camera.
  Selection updates on release or explicit Enter, retaining the previous
  selection throughout the held drag. Escape cancels that pending selection.
- Click and marquee selection use the same visibility/occlusion policy.
  Select visible vertices only. X-ray/select-through and a dedicated wireframe
  mode are deferred. The existing polygon-edge overlay does not enable selection
  through surfaces; drawing style and selection visibility are separate choices.
- Unlocked left-drag selection does not depend on Q/V/W/E/R. Middle-drag pans and right-drag
  orbits in object and vertex modes. A right press/release within four logical
  pixels opens the viewport context menu without moving the camera. Once the
  threshold is crossed, returning to the press point cannot turn the drag into
  a context click. Trackpad navigation and the dedicated camera gizmo retain
  their existing mappings.
- Move uses X/Y/Z axis handles and XY/XZ/YZ plane handles. Object Move and Rotate
  use document axes; object Scale uses the object's local axes, avoiding an
  unrepresented shear when scaling a rotated object. Multiple-object Move and
  Rotate affect the selected group around its bounds-center pivot; group Scale
  is uniform only. Vertex Move, Rotate and
  Scale operate on the selected vertices around their selection pivot.
  Unselected vertices remain unchanged. Numeric controls complement handles;
  no free-move center handle is implemented.
- X/Y/Z toggle explicit Move constraints on the matching document axes. The locked axis is
  highlighted through the selection. A primary drag anywhere in the viewport
  previews movement on that axis. Pointer/key release pauses the Move session;
  further drags, nudges and axis changes preserve its original baseline.
  Toggling off the constraint retains any pending session, so background
  selection remains blocked. Enter or viewport double-click applies all preview
  changes; Escape cancels all of them. Both clear the constraint. Tool changes
  and focus loss cancel. Camera navigation between drags preserves the preview
  and lock; subsequent drags join the same undo step. Active pointer gestures
  and property edits still block navigation. Other edits, Local View changes,
  and file operations wait for explicit apply/cancel; Preferences can remain
  open without cancelling.
- Arrows in Move request a 1 cm translation of objects or selected vertices;
  Shift multiplies this step by 10. Display units, camera zoom, and Auto/Fixed
  pointer spacing do not change the increment or absorb it into a coarser grid.
  Enabled snapping resolves their destination on the fixed one-centimeter
  keyboard grid, preserving the existing off-grid alignment behavior.
  Aligned views determine screen horizontal and
  vertical document axes. Oblique unlocked views do not guess an axis. An explicit
  lock follows the projected axis sign; perpendicular/end-on input uses Right/Up
  positive and Left/Down negative. Camera transitions use their destination basis.
- Without a Move session, a held arrow is one undo step, including repeats, and
  new presses start new steps. Within a session they accumulate into its single
  eventual undo entry. Ownership changes stop a hold. Translation validates
  atomically and preserves
  primitives. Vertex movement uses the inverse object transform.
- Fix the chosen constraint for the duration of a drag and compute displacement
  from its starting state. Do not infer an axis from gesture direction, switch
  constraints automatically. Apply the explicit movement snapping policy before
  publishing the candidate, retaining the chosen constraint. An explicitly locked axis
  viewed end-on uses vertical pointer motion (up increases its coordinate) at the
  current zoom, with a depth label on the guide. Unlocked transform handles in
  unsolvable pointer/constraint configurations produce no displacement. The
  projection, pivot and starting document stay fixed throughout a handle drag.

Text fields, popups, and modal UI keep priority over Escape. With no focused
field, Escape closes the modeless Preferences window. In the editor, successive
presses cancel an unfinished transform or marquee (a Move session rolls back
all previews and clears its axis together), clear an armed Move axis, clear selected vertices while
remaining in edit mode, leave edit mode with the object still selected, then
deselect the object. With no active interaction or selection it is a no-op.
Done and Shift-Enter bypass the deselection steps and leave vertex editing
directly. Deselecting an object hides its editable fields and transform handles;
the display-unit preference remains available. Selection changes never modify
or dirty the document.
Opening a different document or losing focus cancels unfinished interactions.

Moving a vertex can make a quad non-planar. Supported warped faces use projected
triangulation for display; degenerate or crossing projected boundaries are
rejected. Open surfaces and non-manifold connectivity are not automatically
errors. Conversion performs no topology repair and retains polygon boundaries.

## Verification and deferred work

Document, editor, input, renderer and persistence checks run alongside mandatory
executable user-guide scenarios. The current test suite includes the complete
generated guide inventory. Scenarios drive real UI pointer events for middle/right-button
navigation, context-click thresholds, multiple-object marquees in every tool,
hover suppression and restoration, and cancellation. Group transform tests
verify move, rotation, uniform scaling and undo across every selected object.
GPU checks verify outlines, visibility, selection precedence, resize and clearing.
The keyboard guide drives focused Tab/Shift-Tab, Command-A, Delete/Backspace,
context-menu commands and undo/redo in both selection contexts. Focus checks
include the first key after a viewport click, held keys, layout retries, and
field/menu ownership. The two keyboard captures and changed context-menu image
were visually reviewed.
The hand-tool scenario checks open/closed cursor state, translation without
rotation or zoom, unchanged selection and tool in both modes across Q/W/E/R,
gesture priority, Space-up consumption and focus loss. Its two captures and the
gizmo's updated virtual hand cursor were visually reviewed.
The orbit-tool scenario drives native modifier changes and primary pointer
input through production routing. It verifies ready/click/drag/release states,
Planar exit from the visible pose with projection retained, every tool in both
editing modes, gesture ownership, both-modifier precedence and unchanged
selection, document state and history. Its timed animation shows Alt release
while the primary button remains held.

The number-key scenario verifies all six axis directions, source-specific presets,
signed 15-degree orbit steps, projection toggles, whole-document and selected-geometry
fitting, and unchanged document history. Its object/vertex fit captures and the
updated View and viewport context menus were visually reviewed. Regressions cover
rapid orbit taps, single-point fitting, clipping, shifted symbols, repeat state,
UI ownership, and active-edit protection. Native keyboard delivery remains manual.

The new pie scenario covers all seven positions, six view releases, selection
fitting, center/empty-slot/Escape cancellation, delayed release after cancellation,
and unchanged geometry, selection, tool and undo history. The integrated run
passed and the tutorial capture was visually reviewed. Ownership regressions
cover fields, popups, active drags, focus loss, layout retries, viewport resize,
and Space/Backtick ordering. Native gestures queued after the trigger wait for
its UI routing, and focus returns to the viewport after dismissal.

Editor gestures still use egui's per-frame pointer summary. Releasing an existing
selection gesture and pressing again for a hand drag within the same event batch
can miss that new hand press; replaying separate editor gestures within one frame
remains deferred. Space release during an established hand drag is processed in
event order and covered by regression tests.

Navigation is explicit session state: Planar (2D) or Free (3D), independent of
projection. The gizmo owns the mode controls. Axis presets enter Planar; orbiting
exits it. `TogglePlanarNavigation` is the semantic action bound to a plain dot
tap, sharing the gizmo's navigation setter. Keyboard gesture recognition is
separate from that action: an eligible press arms a tap, and release invokes it
once. Focus/ownership changes cancel the tap. A future held-key menu must claim
the gesture and suppress its pending tap, so one physical gesture cannot invoke
both actions. No dot hold behavior is assigned yet.
Every Free-to-Planar entry captures the visible Free orientation before
camera mutation: toggle, gizmo snap, preset, number binding or pie action. Planar
axis changes do not overwrite it. Re-entering during an active return retains
its remembered destination, identified by the camera transition token. A gizmo
press preserves a prospective Planar entry before freezing animated handles;
only a completed axis click adopts it, while an orbit or body release discards
it. The 3D toggle applies `PlanarExit`: perspective
only, remembered orientation only with current projection, or both (default).
Each option preserves the current Planar pan/zoom and uses the shared animation
settings. This state belongs to the navigation controller rather than the gizmo
widget, and the preference is saved in global user settings. Manual orbit starts
directly from the visible camera without recall, including Option-primary drag,
right-button or gizmo drag, and numpad orbit. Explicit Perspective remains the default preset. Switching
to Planar chooses the nearest of the six cardinal axes from the current visible
camera direction, retaining target and zoom. Planar scroll pans. Free scroll
orbits (or uses the zoom preference). Shift-scroll always pans in either mode;
Alt is a primary-drag binding and does not change scroll semantics. This removes
the earlier mode-dependent Shift-to-orbit escape in favor of explicit orbit
input. Focus loss resets held input.
Pan/zoom during a pending Planar snap completes alignment before applying motion;
orbit interrupts at the visible pose. These policies are shared by native input
and the executable guides, including an aligned orthographic Free-view case.
Native trackpad rotation has its own adapter: twists are ignored in Planar mode
(including pending alignment) and orbit only in Free mode. This addresses the
reported physical macOS twist escaping 2D. Synthetic adapter tests cover the
policy; recognition and interleaving of real gestures remain manual checks.

Rulers are view chrome for Planar navigation in fully orthographic cardinal views only. They share
the arrow-nudge alignment policy but use the actual settled camera, hiding during
transitions and in oblique or perspective views. Their horizontal/vertical axes
refer to screen directions, with selected-display-unit ticks and zero at the projected
document origin; neither strip is labeled as a world X/Y/Z axis. Top and left
gutters lie outside viewport hit testing. Per-object projected bounds are merged
only when their one-dimensional spans overlap; vertex mode uses the selected
vertices' combined span. Cached display-space bounds invalidate on geometry,
selection, display frame or source-up changes; camera navigation only projects
their eight corners. Ruler presentation never mutates document or history.

Clippy, formatting and launcher checks pass. New tutorial captures were visually
reviewed and the local N3 bundle rebuilt. Native dialogs, device gesture delivery
and interaction feel require separate checks; no completed native acceptance
is implied.

Each implemented user-facing feature belongs in the existing executable user
guide. This checkpoint is engineering documentation outside generated `docs/guide/`;
it does not replace executable feature scenarios or native interaction checks.

Keep an explicit OBJ support matrix as the importer evolves. Materials, textures,
UV editing, free-form surfaces, standalone OBJ line/point import, broader attribute
preservation and export round trips are unfinished work. Heart and gear
primitives, edge/face component editing, broader topology construction and a
free-move handle are also deferred. Circle's loose-edge vertex editing and
single-face construction are supported; broader line and point construction
tools remain deferred.

Sculpting, automatic topology decisions, general node graphs, live collaboration,
and web delivery remain outside this first editor checkpoint.

Later UX ideas include gesture-driven axis locking and other movement assistance.
Keep these separate from the initial deterministic movement contract. Explicit
X-ray selection and Wireframe shading are implemented independently of the
optional Solid edge overlay; see their current guide and architecture contracts.

## Numeric transform input

Axis-constrained Move, Rotate, and Scale accept exact signed decimal values.
Input ownership, total-session preview semantics, history boundaries, coordinate
spaces, and the multi-object scale limitation are defined in
[numeric-transforms.md](numeric-transforms.md) and exercised by the
[executable user guide](../guide/numeric-transforms.md).
