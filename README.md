# N3

Nothing graphics editor — 3D.

A minimal 3D editor focused on making mesh modeling easier to learn. Start with
a shape, adjust its proportions, and edit its vertices when you need more
control. N3 brings familiar design-tool interactions to a small, focused modeling
workspace.

N3 is open source and macOS-first. It is in early development, with an initial
focus on hard-surface modeling.

![N3 workspace with Layers, a 3D viewport, and live Properties](docs/guide/assets/workspace-layout.webp)

[User guide](docs/guide/README.md) · [Animated walkthroughs](docs/guide/gizmo.md) ·
[Contributing](CONTRIBUTING.md)

## Shape, explore, refine

- **Start with a shape.** Insert cubes, cylinders, cones, and tori. Adjust their
  parameters until you change their vertices. Inspecting or selecting vertices
  preserves the primitive; undoing the edit restores its parameters.
- **Work directly.** Select objects or visible vertices, then move, rotate, and
  scale with handles, axis constraints, or exact numeric input. Preview changes
  as you make them and undo a completed adjustment in one step.
- **Find your bearings.** Navigate with a mouse or Mac trackpad. Drag the axis
  gizmo to orbit, or align to a planar view with rulers. Animated transitions help
  you follow the change in orientation. [Local View](docs/guide/local-view.md)
  isolates selected objects while you work, then returns to the full scene.
- **Keep the workspace simple.** Layers on the left, live properties on the
  right, and modeling tools over the viewport. Resize the panels or hide the
  interface when you want more room.
- **Keep your work readable.** Import OBJ meshes and save editable `.n3.json`
  documents. Lengths use centimeters internally, with other units available for
  input and display.

The [guide](docs/guide/README.md) includes screenshots and animated tutorials
generated from the application, with visible cursor, key, and gesture cues.

## Try N3

For now, run N3 from source on macOS. Install Rust (pinned to 1.98.1), `just`
(tested with 1.46), and the macOS command-line developer tools, then run from
this repository:

```sh
just run
```

This builds and opens N3 with a sample model. Start an empty document or explore
Suzanne instead:

```sh
just run --empty
just run fixtures/obj/suzanne.obj
```

Drag an OBJ or `.n3.json` file into the viewport to open it. An imported OBJ
becomes an editable N3 document; save it as `.n3.json` to keep your changes.

Read the illustrated guide in your browser with `just docs` (requires Python 3).
It opens a local preview of the checked-in documentation; press Ctrl-C in the
terminal to stop the server.

## Current scope

N3 currently supports object transforms, vertex editing,
[making a face](docs/guide/make-face.md) from a triangle or a single flat outline,
and explicit [X-ray selection](docs/guide/xray.md). Broader topology tools,
edge/face selection, geometry snapping, and sculpting are future work.
Web support is also deferred.

OBJ import focuses on vertex positions and polygon faces. Materials, textures,
UVs, and authored normals are not preserved, and OBJ export is not implemented.
See [format details](docs/development.md#documents-and-imports) before relying on
an import workflow.

## Build with us

N3 is written in Rust with wgpu, winit, and egui. See
[contributing](CONTRIBUTING.md) to get started, [development](docs/development.md)
for commands, and [AGENTS.md](AGENTS.md) for development principles and iteration
guidance.

## License

[MIT](LICENSE). Suzanne's upstream provenance and retained license are recorded
in [fixtures/README.md](fixtures/README.md).
