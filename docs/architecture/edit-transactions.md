# Edit transactions

The canonical history mechanism is
[`EditHistory<T>`](../../src/editor/edit_history.rs). It is independent of Move, geometry,
widgets, and input devices. A transform, a future color picker, or another
previewable edit uses the same transaction boundary. Move's axes, drag origins,
confirmation keys, and input guards belong to its editor controller.

The caller owns the **working state** shown by the application. History owns
committed undo/redo snapshots and, optionally, one transaction baseline. Updating
the visible working state is not itself a history entry. Ten preview updates can
produce one accepted change, or none if the user cancels.

## Core contract

Snapshots implement `Clone + PartialEq`. The core compares the complete value
supplied as `T`; it does not interpret document contents or determine whether a
change is meaningful to a particular feature.

| Operation                                      | History behavior                                                                                                                                           | Caller responsibility                                                                                                         |
| ---------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `begin_transaction(before)`                    | Stores the original baseline and returns `true`. If already active, returns `false` without replacing it.                                                  | Establish the owner of the interaction. A failed begin does not grant a second action permission to join it.                  |
| Preview                                        | No history operation. Neither undo nor redo changes.                                                                                                       | Construct and validate a candidate, then publish it as working state.                                                         |
| `commit_transaction(&current)`                 | Closes the transaction. A changed value records its baseline once, clears redo, and returns `true`. An unchanged value returns `false` and preserves redo. | Keep the accepted working state. Interpret `false` as no recorded change, including a call with no active transaction.        |
| `cancel_transaction()`                         | Closes the transaction and returns its baseline, without changing either stack.                                                                            | Restore that baseline when returned, and refresh derived state. Taking the baseline alone does not roll back the application. |
| `record(before, &current)`                     | Records a changed immediate edit and clears redo. Returns `false` for an unchanged value or while a transaction is active.                                 | Use only for a validated immediate edit outside a transaction. Do not publish an unrelated edit and ignore a refused record.  |
| `undo(current)` / `redo(current)` while active | Cancels the transaction and returns its baseline. Neither committed stack is traversed.                                                                    | Restore the returned baseline. A later invocation can traverse committed history.                                             |
| `undo(current)` / `redo(current)` while idle   | Returns the adjacent committed state and retains `current` on the opposite stack, or returns `None` if unavailable.                                        | Install the returned state and refresh derived state.                                                                         |

There is one baseline, with no nested transaction stack. Repeating begin is safe
for the baseline but is a refusal, not an independent nested edit. A controller
continuing its own transaction reuses the original baseline across pointer
releases, repeated keys, and changes of constraint. It must not begin again from
an intermediate preview and thereby redefine what Cancel means.

No-op acceptance, cancellation, and rejected nested or immediate records leave
an existing redo branch intact. Only accepting an actual new change invalidates
that branch. `clear_redo()` exists for caller-controlled coalescing of immediate
edits and is ignored while a transaction is active. `has_transaction()` and
`transaction_baseline()` expose the active state and immutable baseline without
changing either stack.

`new(limit)` bounds undo retention. The editor uses **64 snapshots**. A zero
limit disables undo retention while keeping transactions cancellable; accepting
a changed value still reports `true`. An open baseline is separate from the
retained undo entries.

## Validation and restoration

History does not validate, mutate the document, render a preview, write a file,
or refresh caches. Those responsibilities stay with the owner of `T`.

For geometry, build and validate the candidate before replacing working state.
An invalid candidate leaves the last valid preview and original baseline intact.
If the controller decides to abandon the operation, it cancels and restores the
baseline explicitly. Commit must receive an already-valid state; the generic
core cannot repair or reject invalid geometry.

The editor supplies a `Snapshot` containing the document, selected objects and
active object, selected vertices, and edit mode. Restoration keeps this context
consistent and invalidates derived geometry as needed. Camera state, hover,
axis constraints, pointer gestures, and input-release suppression remain outside
the generic history. Selection-only interactions do not record document edits;
the editor controls when a snapshot should enter history.

The pending-edit policy is also outside the core. Released transform previews
allow camera navigation while retaining their geometry, axis lock, numeric
amount, and original transaction baseline. The next transform drag continues
that session; accepting records one undo step, and cancelling restores the
original geometry without restoring the camera. `Editor::blocks_navigation()`
keeps active pointer gestures and property edits exclusive. Unrelated edits,
Local View changes, and file operations still wait for explicit Apply or Cancel.
Enter or viewport double-click applies; Escape cancels. During a camera drag,
Escape ends navigation first and leaves the pending transform intact. Tool
changes and focus loss cancel through the same baseline restoration. A future
feature may have different controls while retaining this history contract.

## Editing a derived representation

[`DerivedEdit<Source, Value>`](../../src/editor/derived_edit.rs) preserves an
authored source while exposing an evaluated value for editing. It owns the
original source and evaluated baseline, with no dependencies on geometry,
widgets, input, history, or file formats. The caller evaluates and validates;
the core compares values and resolves whether to retain the original source or
publish the changed representation. Exact equality is deliberate: this is
reversible materialization, not shape recognition or approximate reconstruction.

For primitive vertices, the source is the complete primitive recipe and the
derived value is its evaluated mesh. Entering edit mode opens this session
without replacing the canonical geometry. Selection reads the evaluated mesh.
An effective edit materializes a mesh in the ordinary validated candidate; an
exact return to the evaluated baseline restores the original recipe. Object
identity, name, and transform remain outside this geometry-specific replacement.

This session lasts for the edit visit and is separate from an operation's
`EditHistory` transaction. Every drag, numeric transform, or deletion keeps its
existing acceptance and cancellation boundary. Conversion travels with the
first real edit, so one Undo restores the primitive while vertex editing remains
available. No-op entry and exit create no history, and an unchanged or cancelled
operation does not erase redo. History restoration must also restore the source
provenance needed to resolve subsequent edits consistently.

Leaving closes the derived edit session. If the derived value is unchanged, the
source remains authoritative; otherwise the mesh does. Only the resolved
canonical geometry is saved. The transient source/baseline is not serialized as
a competing representation, and a later visit to an ordinary mesh does not
infer a former primitive recipe.

## Future color-picker example

No object-color property or color-picker UI is implemented by this contract.
The core's color-value example demonstrates reuse without geometry dependencies.

A future picker would capture the initial color with `begin_transaction`, then
validate and show each slider or swatch change directly in working state. These
previews call neither `record` nor `clear_redo`, so a hundred slider updates do
not create a hundred undo entries or erase redo. Accept calls
`commit_transaction` once. Cancel restores the result of `cancel_transaction`.
Returning to the initial color before Accept is a no-op and keeps redo intact.
Undo or Redo while the picker transaction is open cancels that preview first;
it does not also undo an earlier modeling operation.

This same separation should remain available if the Move feature is changed or
removed. New preview features depend on generic edit history, not on Move's
session type or its keyboard bindings.

See [editor foundation](editor-foundation.md) for document boundaries and the
[axis-lock guide](../guide/axis-locks.md) for the current Move interaction.
