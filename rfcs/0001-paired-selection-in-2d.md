# RFC 0001: Paired selection in 2D

- **Status:** Deferred
- **Date:** 2026-09-30
- **Decision:** Preserve the idea for further design. Do not implement it or
  change the current selection defaults. Exact geometric correspondence does
  not guarantee the user's intent to select both sides.

## Problem and intended experience

In an axis-aligned orthographic view, front and rear vertices may occupy the
same projected position. A user shaping a cube's silhouette can reasonably
expect selecting a corner to select both depth endpoints, preserving the
thickness when moving them within the view plane.

Today, with X-ray off, N3 selects visible vertices. X-ray explicitly enables
selection through surfaces. The proposal is to make silhouette editing more
natural when the object has a verified symmetry relationship along the viewing
direction, without making every 2D selection a general through-selection.

The original suggestion was to enable X-ray automatically for suitable symmetric
objects. The reviewed alternative is narrower: expand a visible selection to
its verified mirror partners while leaving the X-ray setting independent.
Neither approach is approved for implementation.

## Candidate rule

Treat this as a selection policy, provisionally called **paired selection in
2D**. It is not a new shading mode or a persistent symmetry constraint.

1. Require explicit 2D navigation, settled orthographic projection, and an exact
   supported viewing axis. Perspective, camera transitions, and merely similar
   angles do not qualify.
2. Limit the operation to the object currently being vertex-edited. Respect
   Local View, clipping, and object visibility. Do not pair separate objects.
3. Verify a reflection correspondence across a plane perpendicular to the view
   direction. The plane belongs to the geometry; it is not necessarily at the
   world origin or the object's pivot.
4. Require matching positions and authored edge/face connectivity under that
   reflection, accounting for reversed winding. Reject ambiguous correspondence
   instead of choosing an arbitrary vertex ID.
5. Start from the ordinary visible selection and add each selected vertex's
   unique reflected partner. A vertex on the symmetry plane maps to itself.
6. If qualification is unavailable, retain ordinary visible-only selection.
   Explicit X-ray continues to provide general through-selection.

In coordinates aligned to the view, a reflection maps
`(u, v, depth)` to `(u, v, 2 * center - depth)`. The projected `u` and `v` stay
unchanged. For a finite nonempty vertex set, the midpoint of its depth extent
provides a candidate center; the entire correspondence still requires checking.
Neither an identical silhouette nor a few coincident vertices proves symmetry.

## Useful cases and counterexamples

| Case                                                              | Intended outcome or limitation                                                                                 |
| ----------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Cube viewed directly from the front                               | Each selected front corner can include its rear counterpart.                                                   |
| Cylinder viewed along its length, facing a cap                    | Corresponding vertices on the two cap rings can pair.                                                          |
| Cylinder viewed from the side                                     | Qualification depends on the actual polygon arrangement and its orientation, not simply an even segment count. |
| Asymmetric mesh with coincident projected vertices                | Coincidence alone must not enable pairing.                                                                     |
| Matching vertex positions but different front/back topology       | Positional symmetry alone is insufficient.                                                                     |
| Nested shells or internal geometry                                | General X-ray could select unrelated interior vertices; partner expansion must not include every depth hit.    |
| Disconnected symmetric components in one object                   | A global reflection may map one part onto another; whether this is allowed needs an explicit scope decision.   |
| Duplicate positions or several coincident depth layers            | Reject ambiguous pairings; do not resolve them by arbitrary ordering.                                          |
| Symmetric object whose front needs an intentional asymmetric edit | Symmetry is real, but automatic pairing would still contradict the user's intent.                              |

## Exactness and numerical limits

Determinism and exactness are separate properties. A tolerance-based algorithm
can be deterministic while still accepting geometry that is only approximately
symmetric. A screen-pixel threshold also introduces zoom-dependent behavior.
Neither meets the original requirement of exact correspondence.

N3 stores object-local vertex coordinates as `f64`, applies object transforms,
and derives lower-precision screen coordinates. Qualification should belong to
canonical/evaluated geometry rather than rendered pixel positions. See
[the document model and primitive evaluator](../src/model/document.rs).

The cylinder evaluator copies its first ring to create the opposite cap, giving
those vertices exact local correspondence. Side-view reflection pairs are
computed through separate sine/cosine evaluations and can differ by rounding.
Imported meshes and transformed geometry introduce similar complications.
Strict coordinate equality can therefore reject visually symmetric objects.

Two possible approaches need further evaluation:

- **Conservative exact qualification:** use verified construction relationships
  or a rigorous check of stored geometry, supporting only transformations for
  which the relationship can be established. Accept false negatives instead of
  guessing. Primitive origin alone is insufficient after edits.
- **General approximate qualification:** define an explicit geometric tolerance
  and validate topology as well. This broadens coverage but does not satisfy the
  original exactness requirement and would need a new product decision.

Retained construction correspondence must remain valid for the current geometry
and view. It must not become a permanent claim merely because a mesh began as a
cube or cylinder. No canonical format change or metadata design is chosen here.

## Interaction and state questions

- **Default or explicit choice:** should qualified pairing happen automatically,
  or should the user request silhouette/profile editing? Automatic qualification
  cannot prove intent; a visible active indicator and a front-only escape are
  necessary design questions.
- **Selection vocabulary:** decide whether click, box, additive selection,
  deselection, Select All, and cycling all operate on pairs. A partial rule must
  not leave hidden counterparts unexpectedly selected.
- **Scope:** decide whether the whole edited object must qualify, and whether
  disconnected components, internal shells, and partial symmetry are supported.
  A small first experiment could reject those cases.
- **Edits and navigation:** freeze correspondence for a selection gesture.
  Reevaluate qualification after geometry changes, but preserve already selected
  IDs when symmetry breaks or the user orbits out of 2D. Do not drop half of a
  selection during an edit.
- **Selection versus symmetry editing:** selecting both partners applies normal
  transforms to both. Moving in depth or rotating can break reflection symmetry;
  continuously mirrored transforms are a separate feature.
- **Feedback:** show how many vertices a projected marker represents and make
  paired selection understandable without silently changing the X-ray checkbox.

## Architectural fit, if revisited

The current [editor selection path](../src/editor/mod.rs) already separates
selection depth from shading and retains projected depth/occlusion information.
Partner expansion could follow visible hit collection without changing X-ray's
global depth policy. Geometry correspondence should use stable vertex IDs and
be cached against geometry revision, object transform, and viewing axis as
appropriate, rather than inferred repeatedly from screen overlap.

Ordinary selection remains transient and does not mutate geometry or create
history entries. Keep that invariant. This proposal does not require a new
document entity, a mesh conversion on selection, or a general symmetry editor.

## Evidence required before adoption

An experiment should use executable documentation to show selection in 2D,
orbit to reveal the included rear vertices, and compare with ordinary selection.
Include negative cases, not only a cube demonstration:

- Matching and nonmatching cylinder orientations/segment counts.
- Asymmetric topology despite matching point positions.
- Rounding differences, transformed meshes, duplicate positions, and internal
  or disconnected geometry.
- Equivalent qualification across zoom levels.
- Entering/leaving 2D and breaking/restoring symmetry through edits and undo.
- Gesture stability, unchanged document/history on selection, and consistency
  between the proposed selection operations.

Replay can verify a chosen rule. Human use must still establish whether its
feedback and defaults match intent. No implementation or prototype has been
approved by this RFC.

## References

- [Maya Select Tool](https://help.autodesk.com/cloudhelp/ENU/MayaCRE-Basics/files/GUID-60FD5F79-AC1D-46DE-B66D-2FBE73E15A30.htm): camera-based selection and symmetry are separate settings. Symmetry defaults to off; reflection matching has a tolerance. This is a comparator, not evidence that automatic pairing is safe.
- [Maya symmetry command](https://help.autodesk.com/cloudhelp/ENU/MayaCRE-Tech-Docs/Commands/symmetricModelling.html): explicit controls for reflection tolerance, coordinate space, and topological symmetry.
- [N3 X-ray guide](../docs/guide/xray.md): implemented selection-through-surfaces behavior, including the select-then-orbit comparison.
- [N3 selection architecture](../docs/architecture/architecture.md): current depth policy, picking, and rendering boundaries.

The external references were reviewed on 2026-09-30.
