# Vertex selection feedback

This feature changes the presentation of the current vertex selection. It does
not add edge/face selection modes, select more geometry, or change click,
Shift-click, marquee, visibility, transform, or history semantics.

## One selection payload

The editor's active mesh object and selected vertex IDs produce one semantic
selection payload. Native and headless rendering consume that same payload.
It is derived state and is not serialized into the document or recorded as a
geometry edit. The palette is shared: warm orange for selected vertices and dark
for unselected vertices.

Vertex markers expose that state directly. Each real polygon boundary edge gets
one color for each endpoint, interpolated along the edge: orange/orange for two
selected endpoints, orange/dark for one, and dark/dark for none. Internal
triangulation diagonals are not authored edges and must never be drawn as this
selection feedback.

A face tint is selected only if every vertex of the original polygon is in the
selected set. All triangles derived from that polygon receive the same tint
classification. Classifying triangles independently would incorrectly tint part
of a quad or ngon after selecting only some polygon vertices. The fill remains
subtle enough for surface shading and the stronger vertex/edge feedback to be
read together.

## Visibility and editing context

All feedback respects scene depth, including occlusion by other objects.
Selected geometry behind a surface does not become an x-ray overlay. Existing
visible-only vertex selection remains unchanged. The active edited mesh always
shows its real polygon edges; the general object-edge display remains a separate
presentation preference. Leaving edit mode returns to ordinary object rendering.

Selection color and topology association are separate from document geometry.
Object transforms and display-frame normalization must preserve vertex IDs and
polygon membership when deriving render data. Scene depth and rendered geometry,
not a screen-space approximation of a face bounding rectangle, govern what is
visible.

## Verification

The executable [selection feedback guide](../guide/edit-selection.md) enters edit
mode on the canonical quad cube, selects adjacent front vertices with real
clicks and Shift-clicks, and captures a partial selection and a complete front
face. It checks original-boundary edge classifications, original-polygon tint
eligibility, an intermediate three-of-four selection, and unchanged authored
geometry. A real right-button drag turns the complete selection to an oblique
view for visual review. Renderer checks cover depth, display-axis conventions,
and polygon classification independently.
Screenshots use the same headless scene renderer and selection payload as the
native application; no documentation-only recoloring is applied.
