# Adaptive movement precision

Status: implemented in N3, September 29, 2026. This document records
the rationale and tuning choices; [movement-snapping.md](movement-snapping.md)
describes the implementation boundaries. Native pointer comfort remains under review.

## Why revise the default

The canonical length unit and the editing increment answer different questions:
one coordinate unit means one centimeter; a snap step controls the precision of
an interaction. Fractional centimeters are already valid document coordinates.
Keeping centimeters does not imply restricting interaction to whole centimeters.

The unchanged `fixtures/obj/suzanne.obj` has coordinate extents
2.673828 x 1.924805 x 1.626465. Our importer preserves these numbers and currently
interprets them as centimeters. A 1 cm step is therefore 37.4% of Suzanne's width.
DisplayFrame fits the mesh for rendering without rescaling its authored data.
The poor movement feel is an expected consequence of the fixed interaction step.

This is a small model, but a model this small must remain editable. Automatically
enlarging an import would hide the problem and guess its intended physical size.
Explicit source units or import scale can be designed separately; the fixture
and canonical centimeter convention should remain unchanged.

Replacing 1 cm with a universal 0.01 cm would fix one scale while imposing another
arbitrary increment on small details, furniture and large environments. The
default is adaptive pointer precision, with a fixed-step policy
retained for work that deliberately needs a particular measurement grid.

## Default rule

At the beginning of a viewport drag:

1. Measure canonical centimeters per logical UI pixel on the camera-facing plane
   at the manipulation pivot. Orthographic scale is uniform; perspective scale
   depends on depth. Use logical pixels/points so Retina backing resolution does
   not change interaction precision.
2. Multiply by a configurable screen-spacing budget, initially 4 logical pixels.
3. Choose the largest step no larger than that budget from the clean decimal
   sequence `... 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10 ... cm`.
4. Freeze the chosen step and input mapping until that drag ends. Resolve every
   preview from its original baseline, as today. Zoom changes can affect the
   next gesture, not re-quantize an active one or existing geometry.

Four pixels is a provisional UX tuning parameter, not an industry standard or a
physical constant. It provides roughly 1.6-4 pixels of spacing in the reference
plane and at most half a step of rounding error per coordinate there. The
1/2/5 sequence avoids the tenfold jumps of a pure powers-of-ten sequence while
keeping values simple. Its grids are not all nested, which reinforces the rule
that the step must remain fixed during a drag.

For illustration, if Suzanne's 2.673828 cm width spans 400 logical pixels, 1 cm
spans about 150 pixels. The rule chooses 0.02 cm, about 3 pixels. That
step is selected because of the current view scale, not because Suzanne has a
special case. A closer view permits a finer step; a more distant view permits a
coarser one. Projected width here is an illustrative aligned-view example, not
an object-bounds heuristic used by the algorithm.

Do not change precision based on cumulative drag distance or pointer speed.
That would change the measuring scale during the operation and make reversing
the same path unpredictable. Do not derive it from mesh bounds: a large object
can have tiny details, and adding another object must not alter sensitivity.

Show the resolved increment in movement feedback, such as `Snap: 0.02 cm`.
The feedback is visible in perspective as well, where rulers are absent.
Rulers and grid lines currently retain their independent display spacing. Their
coarse label spacing does not determine snap precision; unifying their visual
subdivisions with snap feedback can be reviewed later.

## Input and architecture boundaries

The pure translation resolver retains its shared selection delta. An upstream
step policy chooses fixed spacing or adaptive spacing from the screen budget.
The result is a concrete centimeter step captured with the gesture. This keeps
viewport math, quantization, document mutation and history separate.

`Projection::pixel_size(pivot) / DisplayFrame.scale` already supplies the local
canonical view scale. Use that depth-local camera-plane scale, not division by
the projected length of the selected axis: the latter becomes unstable when
the axis points into the screen. Existing end-on Move handling supplies a
screen-up drag mapping. Preserve the axis/plane constraints and shared movement
for groups and vertices.

Numeric field scrubs should use their explicit centimeters-per-pixel sensitivity
as the input to precision selection; orbiting or zooming the viewport should not
change a numeric field's behavior. The current Position sensitivity is
0.02 cm per logical pixel, producing a 0.05 cm step; vertex deltas use
0.01 cm per pixel, producing a 0.02 cm step. Typed values remain exact.
The previously chosen 1 cm / Shift 10 cm arrow nudges retain their own fixed
one-centimeter grid and existing off-grid alignment semantics. Auto or Fixed
pointer spacing cannot swallow a keyboard nudge. With snapping disabled, arrows
apply their exact requested displacement without aligning the destination.

Do not establish 0.01 cm, or another arbitrary physical size, as a hard minimum.
Apply finite-value and representability guards based on the actual numeric
context; those guards are not a promise of unlimited precision. Exact input
remains subject to normal document and rendering validation. An explicit fixed
step may be appropriate for measured assembly; geometry snapping stays deferred.

## Precedent and validation

Blender documents zoom-dependent Increment snapping in aligned views, including
decimal subdivisions on zoom-in and coarser steps on zoom-out. This supports
separating canonical units from interaction resolution. Extending the rule to
perspective at the manipulation depth, the 1/2/5 ladder, and the four-pixel budget
are N3 policy choices, not claims about Blender's implementation.
[Blender 5.0 precision manual](https://docs.blender.org/manual/en/5.0/scene_layout/object/editing/transform/control/precision.html).

Automated checks exercise physical scales, orthographic and perspective zoom,
end-on movement, captured drag settings, slow forward/reverse property scrubs,
selection shape preservation, and cancel/undo. The executable guide shows
Suzanne at overview and detail scales. Native review should additionally cover
close detail editing within a large scene and transitions between monitors with
different backing pixel densities. The resolution selector is deterministic and
unit-tested; native pointer feel must still determine whether the initial
four-pixel budget is comfortable.
