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
| **High priority: Boolean operations** | Must-have union, difference, and intersection operations with a safe, straightforward workflow. Timing depends on the research and engineering groundwork required for the node/modifier system. |
| Bevel | Must-have feature; interaction paradigm remains open. Compare a traditional Blender-style bevel workflow with an easy-to-use approach inspired by Spline before choosing how N3 should support it. |
| Topology editing | Extend the initial [Make Face](docs/guide/make-face.md) operation with edge/face selection, extrusion, and inset; research the interaction model. |
| Paired selection in 2D | **Deferred:** consider verified front/back vertex pairing in aligned views; exact geometry does not prove selection intent. See [RFC 0001](rfcs/0001-paired-selection-in-2d.md). |
| Grouping and hierarchy entities | Add an explicit document entity for organizing objects; evaluate groups, collections, or folders, including nesting, selection, and shared transform semantics. |
| Multiple authored scenes | **Draft; implementation deferred.** Independent N3 scenes in one document/editor. Revisit [RFC 0002](rfcs/0002-authored-scenes.md), starting with a small native-scene proof of switching, history/recovery, and persistence before broader import work. |
| 3D format I/O | Build on read-only glTF/GLB viewing: scene export/authoring, OBJ export, compression and advanced extensions; evaluate OpenUSD composition/schema support. See the [viewer milestone](docs/milestones/modern-scene-viewer.md). |
| Materials and PBR | Extend imported PBR viewing into editable material assignment; HDR environment import, shadows, advanced materials and color management. See the [viewer milestone](docs/milestones/modern-scene-viewer.md). |
| Node Canvas UI Kit | Establish reusable UI primitives for node blocks, ports, connections, selection, dragging, pan, and zoom. Validate interactions in isolated workbench examples so future editors can adopt them confidently when a concrete consumer exists. Scope this as UI foundation work; shader/modifier semantics and evaluation remain separate. |
| UVs and normals | Imported glTF attributes are preserved for viewing; investigate editable document attributes, authoring and shading controls. |
| Animation | Build on imported clip playback and the read-only inspection timeline with canonical document animation, keyframe authoring and GPU deformation. See [inspection boundaries](docs/architecture/animation-inspection.md). Tutorial animations remain a separate system. |
| Video editing | **Exploratory:** consider evolving N3 to support video editing. Scope and timing remain open; media processing, playback synchronization, and export require substantial research and engineering beyond GPU rendering. |
| Multiple windows/views | Define shared document state and independent views; extend executable docs to compose captures. |
| Geometry snapping | Vertex, edge, and face targets alongside existing grid snapping. |
| Further Insert shapes | Evaluate Torus Knot as a distinct procedural recipe and Backdrop as a scene setup tool, with their parameters and rendering behavior defined before adding either. |
| Vertex marker size preference | Consider editable unselected/selected marker radii in Preferences, starting from the current 1.75/2.5-point defaults; keep visual size separate from the picking radius. |
| Inspector number overflow | Keep each numeric field frame fixed while clipping or scrolling only its value text; egui's `DragValue` currently expands both. |
| Native accessibility | Wire egui's AccessKit adapter through the native event loop, add polite live announcements for toasts, and verify with VoiceOver. Keyboard focus and widget labels alone do not provide screen-reader announcements. |
| OS font support | Fonts as document materials; discovery/loading boundary suitable for a later web implementation. Separate from bundled interface fonts. |
| Web platform follow-up | Build on the [WASM browser baseline](docs/development.md#browser-build): browser/device coverage, touch/IME, external asset resource workflows, persistence/recovery, and multiple embedded workspaces. |
| Native/browser viewport profiling | [Issue #2](https://github.com/gridaco/n3/issues/2): reproduce large-mesh orbit stutter with comparable builds, real browser runs, and stage-level timings. Distinguish shared costs, web-host mistakes, and measured platform constraints before optimizing; follow the WASM baseline delivery. |
| Linux capture test concurrency | Diagnose allocator aborts and segmentation faults in parallel lavapipe tests, reproduced on the native baseline. The [opt-in CI runner](docs/development.md#verification-and-git-hooks) uses one test thread while retaining every test and exact capture; establish the cause before restoring parallel execution. |
| Exact macOS 27 guide frame | Replace the approximate static window chrome with measurements from a native macOS 27 N3 capture or Apple's design kit. Evaluate the translucent traffic-light material, highlights, and shadow only if exact mirroring is worth the rendering complexity. |

Existing context: [UX ideas](docs/research/ux-ideas.md),
[deferred engineering boundaries](docs/architecture/milestone-review.md#deliberately-deferred),
and [interface versus document fonts](assets/fonts/README.md).
