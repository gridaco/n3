# Movement snapping

This describes the implementation. The
[adaptive precision rationale](adaptive-movement-snapping.md) records why the
centimeter remains the canonical unit while pointer precision adapts to scale.

The default is **Auto**: viewport movement uses a clean decimal centimeter step
chosen from the current scale at the manipulation pivot. Position scrubs use their
own explicit sensitivity. **Fixed** uses the configured centimeter interval for
pointer movement. Both are editing aids; neither quantizes documents, imports,
primitive dimensions, rotations, or scales. Directly typed positions remain exact.

## One movement boundary

Viewport handles, axis-locked drags, arrow nudges, and Properties position or
vertex-delta scrubs must resolve movement through the same validation boundary
before publishing the candidate. Input mapping, step selection, and quantization
are separate concerns. The pure resolver accepts a concrete centimeter interval;
the step policy resolves one from explicit interaction scale without depending on
egui, camera types, or a particular device.

Auto chooses the largest interval from `1/2/5 × 10^k cm` no larger than four
logical UI pixels. Viewport input supplies canonical centimeters per pixel at
the manipulation pivot. Numeric fields supply their own drag sensitivity:
0.02 cm per pixel resolves to a 0.05 cm Position step, and 0.01 cm per pixel
resolves to a 0.02 cm vertex-delta step. Display units do not alter either.
A field never changes precision because the viewport was zoomed or rotated.

Arrow nudges have explicit physical increments: 1 cm, or 10 cm with Shift.
They still use the shared translation boundary, but select their own fixed
one-centimeter grid so coarse Auto or Fixed pointer spacing cannot swallow a
key press. Enabled snapping preserves the existing off-grid alignment behavior:
the requested destination rounds to that keyboard grid. An on-grid anchor moves
by the exact requested amount. Typed values bypass quantization while retaining
validation and the operation's constraints.

A move retains its explicit axis or plane constraint. Only allowed coordinates
can change; snapping must never shift a perpendicular coordinate to a grid
line. The absolute grid places the active object's origin on grid coordinates.
A vertex selection uses its world-space centroid as the anchor. The internal
relative mode rounds displacement from the gesture's starting anchor; it is not
a second visible preference. A selected group receives one resolved translation,
retaining offsets between objects or vertices. Snapping each vertex independently
would distort the shape and is unsupported. Vertex editing resolves movement in
world centimeters and maps the shared displacement back to object-local space.

## Gesture and history boundaries

A gesture computes each preview from its immutable starting state. Repeated
previews do not accumulate rounding error or overwrite that baseline. The
resolved interval and input projection are captured for a drag. Settings and
zoom changes affect a later drag; they do not re-quantize existing geometry or
an active preview. Multiple drags in a pending Move session can resolve different
steps while retaining one original cancel/undo baseline for the session.

Grid resolution is subordinate to validation and edit history: previews remain
previews, Escape restores the original state, and acceptance creates at most one
history entry. A no-op adds none. The viewport's Snap feedback displays the actual
captured step during a drag and the next eligible step while idle.

Preferences expose **Snap movement to grid**, **Auto / Fixed**, and the fixed
interval. They are session state, survive New/Open, and are not serialized into
document geometry. No new shortcut is assigned. Disabling snapping allows
continuous pointer movement; exact input and explicit nudges remain available.

## Geometry snapping is deferred

Geometry snapping needs candidate discovery, a target identity, visibility and
tolerance rules, and an explicit priority relative to grid snapping. Keep those
choices in the movement resolution layer, upstream of validated geometry
mutation. Do not approximate it by rounding individual vertices or by adding
heuristics inside each pointer handler. This checkpoint does not claim target
acquisition, edge/face snapping, or any geometry-snap UI.

## Verification

The executable [snapping guide](../guide/snapping.md) drives production UI events
for Suzanne moves before and after zoom, orthographic and perspective projection,
Fixed and disabled comparisons, live Position scrubbing, exact typed input, and a
millimeter display. It verifies session accept/cancel and undo/redo alongside
resolved positions and visible feedback. Core tests cover scale selection,
mathematical constraints, and shared displacement. Native pointer feel remains
a manual review; headless input does not establish a comfortable physical-device
experience.
