# TODO

Ideas and larger work areas for future research and implementation sessions.
This is a backlog, not a committed roadmap; listing an item does not approve its
scope. Priority is explicit where noted.

Keep entries brief. Give concrete proposals with meaningful rationale or open
questions a document in [rfcs/](rfcs/README.md), and link it here. Exploratory
research can stay in `docs/research/`. As work lands, narrow or remove its entry
and document the implemented behavior in the executable user guide.

| Item | Starting scope |
| --- | --- |
| **Highest priority: 2D Pen tool** | Start with a dedicated Pen tool for creating geometry in 2D mode. Define point placement, finishing, and cancellation; reserve P for the tool. Consider 3D behavior later. |
| Topology editing | Extend the initial [Make Face](docs/guide/make-face.md) operation with edge/face selection, extrusion, inset, and bevel; research the interaction model. |
| Paired selection in 2D | **Deferred:** consider verified front/back vertex pairing in aligned views; exact geometry does not prove selection intent. See [RFC 0001](rfcs/0001-paired-selection-in-2d.md). |
| Grouping and hierarchy entities | Add an explicit document entity for organizing objects; evaluate groups, collections, or folders, including nesting, selection, and shared transform semantics. |
| 3D format I/O | GLB/glTF import/export, OBJ export, and explicit preservation limits; evaluate other formats as needed. |
| Materials and PBR | Material model, assignment, textures, and viewport rendering. |
| UVs and normals | Preserve imported attributes; investigate authoring and shading controls. |
| Animation | Document animation, keyframes, timeline, and playback. Tutorial animations already exist and are a separate system. |
| Multiple windows/views | Define shared document state and independent views; extend executable docs to compose captures. |
| Geometry snapping | Vertex, edge, and face targets alongside existing grid snapping. |
| Further Insert shapes | Evaluate Torus Knot as a distinct procedural recipe and Backdrop as a scene setup tool, with their parameters and rendering behavior defined before adding either. |
| Vertex marker size preference | Consider editable unselected/selected marker radii in Preferences, starting from the current 1.75/2.5-point defaults; keep visual size separate from the picking radius. |
| Inspector number overflow | Keep each numeric field frame fixed while clipping or scrolling only its value text; egui's `DragValue` currently expands both. |
| Native accessibility | Wire egui's AccessKit adapter through the native event loop, add polite live announcements for toasts, and verify with VoiceOver. Keyboard focus and widget labels alone do not provide screen-reader announcements. |
| OS font support | Fonts as document materials; discovery/loading boundary suitable for a later web implementation. Separate from bundled interface fonts. |
| Initial WASM spike | Prove a minimal browser build and identify platform dependencies before deciding on a full port. |
| Exact macOS 27 guide frame | Replace the approximate static window chrome with measurements from a native macOS 27 N3 capture or Apple's design kit. Evaluate the translucent traffic-light material, highlights, and shadow only if exact mirroring is worth the rendering complexity. |

Existing context: [UX ideas](docs/research/ux-ideas.md),
[deferred engineering boundaries](docs/architecture/milestone-review.md#deliberately-deferred),
and [interface versus document fonts](assets/fonts/README.md).
