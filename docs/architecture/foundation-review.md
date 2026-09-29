# Foundation review agenda

Status: proposed agenda for day-zero review, before the first commit and push.
The current implementation is evidence for discussion, not a ratified architecture.
CI, formatting, and local hooks establish a verification baseline; they do not
approve these boundaries or authorize a refactor. This document proposes no new
features, crate split, public API, or changes to the generated user guide.

Read this alongside [architecture](architecture.md),
[edit transactions](edit-transactions.md), and
[user settings](user-settings.md). Those documents describe the current contracts;
the questions below identify what to review before changing them.

## First discussion

1. **Who owns each kind of state?** Does the draft table below express the intended
   product behavior, especially selection restored by Undo, primitive edit
   provenance, and viewport isolation that remains outside history?
2. **Who owns an interaction from press through release?** Agree on precedence
   among focused controls, popups, active edits, temporary navigation, and idle
   viewport actions. Decide which interruptions accept, cancel, defer, or ignore
   input before discussing where their implementation should live.

## State authority draft

This table summarizes current responsibilities for review. It is not a proposed
type hierarchy or a decision to move fields between modules.

| State                                                                                                 | Current authority                                | Persistence and history                                                                                         |
| ----------------------------------------------------------------------------------------------------- | ------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| Objects, transforms, primitive recipes, authored polygon topology, stable IDs, centimeter coordinates | `Document`                                       | Canonical `.n3.json`; document edits participate in history.                                                    |
| Selected objects, active object, selected vertices, edit mode                                         | `Editor`                                         | Not saved; included in edit snapshots for contextual restoration. Selection-only changes create no undo step.   |
| Primitive source and evaluated baseline during vertex editing                                         | `GeometryEdit` / `DerivedEdit`                   | Not saved independently; provenance travels with edit snapshots. The resolved primitive or mesh is canonical.   |
| Working preview and immutable operation baseline                                                      | `Editor` and `EditHistory<Snapshot>`             | Preview is visible working state; acceptance records one changed operation, cancellation restores the baseline. |
| Tool, axis constraint, numeric draft, pointer gesture, hover, held keys, release suppression          | Editor and input/UI controllers                  | Transient; outside document serialization and committed history.                                                |
| Camera, navigation mode, Local View                                                                   | Workspace navigation and editor visibility state | Outside document and history; history restoration reconciles selection with current visibility.                 |
| Global preferences                                                                                    | Typed settings model and controller              | Separate settings persistence; outside document and edit history.                                               |
| Display frame, evaluated mesh, picking data, GPU resources, selection feedback                        | Derived editor/UI/render state                   | Rebuilt or refreshed by their owning lifecycle; not editable document authorities.                              |
| File paths, last-read bytes, saved-document comparison, pending file requests                         | Workspace and native host                        | Save/conflict coordination; not modeling history. Native host performs filesystem effects.                      |

## Proposed review sequence

1. **State and model authority.** Review the table against
   [`Editor` and its `Snapshot`](../../src/editor/mod.rs) and
   [`WorkspaceUi`](../../src/ui/workspace_ui.rs). Public document, selection, and
   revision fields currently make consistency partly a caller responsibility.
   Use empty selection, edit-mode transitions, display-unit changes, and Local
   View to distinguish document changes from session changes.
2. **Intent and bindings.** Trace [`Command`](../../src/input/shortcuts.rs) and
   the [binding table](../../src/input/bindings.rs). Commands currently include
   application effects, editing, navigation, numeric characters, and nudge
   release events. Review that vocabulary before introducing another action
   abstraction. Use Cursor aliases, physical number-key identity, and numeric
   input taking precedence over view presets as examples.
3. **Context and gesture lifetime.** Review `KeyboardOwner`, `ShortcutFrame`,
   [held navigation](../../src/input/temporary_navigation.rs), and workspace
   gesture state together. Ownership spans both sides of an egui UI pass and
   layout retries; numeric routing predicts context changes within an input
   batch. Existing editor pointer gestures still use egui frame summaries, with
   a documented same-batch release/new-press limitation. Use popup dismissal,
   Space/Option precedence, focus loss, and Enter/Escape to make the policy explicit.
4. **An edit's lifetime.** Distinguish
   [`Document::transact`](../../src/model/document.rs), which validates a candidate,
   from `Editor::commit`, which also integrates history and derived-state refresh.
   [`EditHistory`](../../src/editor/edit_history.rs) stores baselines and snapshots;
   the caller owns preview validation, restoration, and interaction ownership.
   Review one accepted edit, one cancelled edit, one invalid candidate, and one no-op.
5. **Evaluation and invalidation.** Review
   [primitive adaptation](../../src/editor/geometry_edit.rs), document render-mesh
   construction, and editor candidate validation. Validation currently invokes
   `render_mesh`, and cache invalidation relies on reaching the editor's change
   lifecycle. Use reversible primitive materialization, preserved polygon IDs,
   and selection feedback without geometry uploads to evaluate this coupling.
6. **UI and host boundary.** Review `WorkspaceUi::dispatch` and
   [navigation routing](../../src/input/navigation_events.rs). Dispatch requires
   egui context and combines eligibility, editor calls, repainting, and host
   requests; some UI controls call editor methods directly. Trace equivalent
   menu, shortcut, and replay paths before deciding whether another host needs
   a narrower interface. Native dialogs and physical gesture recognition remain
   separate from replay evidence.

These are observed dependencies and review risks, not findings that the current
behavior is incorrect or that any particular replacement is approved.

## First candidate trace

Follow one selected vertex of a live primitive through an axis-constrained Move:

1. Enter vertex mode and select a vertex; the primitive recipe remains canonical.
2. Arm an axis and drag to a nonzero pointer preview. Identify its immutable
   baseline and input owner while the pointer remains held.
3. Type an exact numeric amount. It replaces the total operation from that same
   baseline, rather than adding to the pointer result. Check geometry, primitive
   representation, selection, revision, and undo/redo state at each transition.
   Release the retired pointer gesture; it must not overwrite the numeric preview.
4. Follow two branches from equivalent starting states: accept once and Undo once;
   or cancel, confirm baseline restoration, then Undo a prior committed edit.
   A delayed pointer/key release must not accept or select again.
5. Also inspect the no-op return to baseline: it should restore the original
   primitive representation and preserve the redo branch.

This is a proposed review trace assembled from existing behavior. It is not a
claim that one current scenario covers the entire sequence, or that the sequence
was newly run for this document.

## Existing evidence to inspect

- [Editing](../../src/documentation/scenarios/editing.rs) and
  [axis locks](../../src/documentation/scenarios/axis_locks.rs): primitive entry,
  pointer and constrained previews, acceptance, cancellation, and undo.
- [Numeric transforms](../../src/documentation/scenarios/numeric_transforms.rs):
  pointer-to-numeric handoff on an object, total-baseline numeric previews, and
  command ownership. [Primitive edit tests](../../src/editor/primitive_edit_tests.rs)
  and [numeric tests](../../src/editor/editor_numeric_tests.rs) cover the related
  vertex and source-representation invariants.
- [Hand tool](../../src/documentation/scenarios/hand_tool.rs),
  [orbit tool](../../src/documentation/scenarios/orbit_tool.rs),
  [view pie](../../src/documentation/scenarios/view_pie.rs), and
  [shortcut tests](../../src/input/shortcuts_tests.rs): held input, interruptions,
  focus ownership, numeric provenance, and layout retries.
- [Local View](../../src/documentation/scenarios/local_view.rs),
  [length units](../../src/documentation/scenarios/length_units.rs), and
  [object feedback](../../src/documentation/scenarios/object_feedback.rs): session,
  presentation, selection, and document boundaries.

For each discussion, record the behavior to preserve, the unresolved choice,
and the smallest evidence needed to resolve it. Agree on behavior before
proposing code movement. Existing tests and replay scenarios are inspection
material here; their presence does not establish a fresh verification result or
physical-device acceptance.
