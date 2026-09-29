# Viewer-first scope and later editing proposal

> Historical scope from September 28, 2026, preserved for context. The editor has
> since been promoted and the old `spikes/obj-viewer/` directory removed; see
> [current architecture](../architecture/architecture.md) and
> [development commands](../development.md) (`just run`) for the maintained implementation.

September 28, 2026. The current authorized slice is a disposable local OBJ viewer.
Rust, macOS first, and wgpu are agreed directions. The bracket fixture has rendered
in the native app through Metal, and the Open dialog has appeared. Completing a
picker selection, physical pinch/twist gestures, and manual drag-and-drop remain
for user validation. These observations do not establish full interaction
acceptance.

## Current scope

This scope supersedes the earlier document-first pre-PoC proposal. The viewer can
load polygons into display geometry without an editable document, stable editor
IDs, commands, undo history, or a topology kernel. None of those systems is a
prerequisite for the current slice.

| Concern         | Current viewer scope                                                                                                          |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Location        | Local, gitignored `spikes/obj-viewer/`; absent from a fresh clone.                                                            |
| Fixtures        | Original OBJ geometry under tracked `fixtures/obj/`.                                                                          |
| Integration     | Rust app using winit, wgpu, and egui for this experiment.                                                                     |
| Loading         | Open or drop an OBJ; parse in the background; replace the displayed model on success and retain the last good model on error. |
| Display         | Shaded geometry, original polygon-edge overlay, optional grid, model statistics, and import notices.                          |
| Camera          | Orbit, pan, zoom, frame, perspective/orthographic projection, and preset views with provisional controls.                     |
| Orientation     | An optional Z-up display conversion for OBJ sources such as Blender; no change to the source file.                            |
| Local artifacts | Dependency caches, Cargo builds, app bundle, screenshots, and review evidence remain under ignored `spikes/`.                 |

The [viewer guide](viewer-spike.md) records commands and controls. The
[fixture notes](../../fixtures/README.md) record expected geometry and import
behavior. Quads and supported n-gons are triangulated for display; the edge
overlay follows source polygon boundaries. This is a viewing contract, not an
editable topology or export contract.

No editing, selection tools, OBJ export, native project format, editable document
model, operational transformation (OT), or collaboration system has been
implemented or selected through this spike. The integration libraries used here
do not settle the complete product stack.

## Questions the viewer should answer

- Does real OBJ geometry display clearly, including concave polygon boundaries
  and authored or generated normals?
- Are orbit, pan, zoom, framing, and projection understandable on a Mac trackpad
  and mouse?
- Do Retina scaling, resizing, input focus, and the UI/viewport boundary behave
  correctly in a native run?
- Does loading stay responsive, and do errors and unsupported-data notices make
  the result understandable?

Compilation, loader checks, native runtime checks, and visual acceptance provide
different evidence. The integrated build, strict Clippy check, and loader/camera
tests passed. Native validation is still in progress, with the outstanding
manual checks noted above. Any promotion of spike code into the tracked
application needs a separate decision.

## Later editing work: proposal only

The following retains useful ideas from the earlier pre-PoC draft. It describes
possible future work and is not a selected architecture or an extension of the
current viewer scope.

A later editing experiment could create a known polygon fixture, select its
components, preview and commit a small edit, undo/redo, then write and reload its
supported geometry. Before starting, decide what must remain editable and which
operations the experiment actually needs.

| Concern                 | Candidate approach to evaluate later                                                                                        |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| Canonical state         | Current polygon geometry and the object state needed by the supported edits.                                                |
| Changes                 | Typed commands with explicit inputs, validation, and atomic commit or failure.                                              |
| Undo/redo               | A bounded local transaction history; small before/after snapshots are one starting option.                                  |
| Persistence             | OBJ geometry-only output when export is needed. Consider native persistence only when requirements exceed mesh interchange. |
| Synchronization         | Revisit if concurrent editing becomes a requirement.                                                                        |
| Procedural construction | A separate decision from both undo history and collaboration.                                                               |

OBJ need not preserve editor IDs or history. Its lack of a standard unit/up-axis
contract calls for explicit import/export conventions. Output could preserve a
declared subset of polygon connectivity and positions while assigning fresh
editor IDs on reopening. No such round-trip is currently implemented or promised.

GLB may later serve finished render assets. The
[glTF specification](https://github.com/KhronosGroup/glTF/blob/main/specification/2.0/Specification.adoc)
uses triangle-based surface primitives and deliberately excludes authoring
information. It does not preserve original quad/n-gon boundaries by default.

## Possible ownership boundaries for an editor

| Layer          | Candidate responsibility                                                                 |
| -------------- | ---------------------------------------------------------------------------------------- |
| Document       | Objects, names, transforms, and geometry ownership required by the chosen editing scope. |
| Editable mesh  | Connectivity, positions, element identity, and supported attribute domains.              |
| Editor session | Selection, active tool, camera, preview transaction, and undo/redo.                      |
| Derived data   | Triangles, generated normals, bounds, picking structures, and GPU resources.             |

These boundaries do not choose Rust structs, a mesh library, a native file schema,
or a crate layout. If stable element IDs become necessary, specify survival,
deletion, replacement, and undo behavior before exposing references to tools.
Likewise, decide how edits affect vertex, edge, face, and face-corner attributes
without assuming a general attribute framework is needed immediately.

OT usually means operational transformation for reconciling concurrent edits;
[ShareDB](https://share.github.io/sharedb/) is one example. It does not define the
document or valid modeling operations. Stable IDs and explicit commands may help
future synchronization, but they do not resolve conflicts such as concurrently
deleting and extruding the same face. An operation log, snapshot history,
procedural graph, and collaboration algorithm are distinct choices.

## Decisions before a future editing experiment

- Choose a small operation and supported topology envelope, including the role
  of open surfaces and loose edges.
- Specify preview, cancellation, failed-edit, selection, and undo behavior.
- Define coordinates, units, precision, tolerances, attribute handling, and the
  treatment of degenerate, crossing, or non-planar polygons for editing.
- Decide whether source polygon identity is needed for picking and editing;
  display buffers alone do not supply that contract.
- Define any export subset and its round-trip guarantees before implementing it.
- Revisit portability and collaboration requirements without treating a wasm
  compile as browser runtime proof.

The final gestures/keymap, pen-tool UX, automatic topology decisions, AI,
general booleans/bevels, procedural nodes, collaborative synchronization, broad
asset import, photorealistic rendering, and a browser product remain future
questions. Sculpting remains outside V1.
