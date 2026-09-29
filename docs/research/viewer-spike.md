# Local OBJ viewer spike

> Historical viewer checkpoint. The old `spikes/obj-viewer/` directory and its
> launchers have been removed. Commands below are historical and no longer run;
> use [current development commands](../development.md) (`just run`) and the
> [generated user guide](../guide/README.md) for the maintained editor.

The disposable Rust viewer lived at `spikes/obj-viewer/`, which was gitignored.
This guide recorded local work: a fresh clone at that checkpoint contained the
fixtures and research, but not the viewer or its launcher.

The spike explores OBJ loading, geometry display, and camera interaction on
macOS using winit, wgpu, and egui. It does not implement editing, export, a native
document format, undo/redo, or collaboration.

## Historical launch commands (retired)

With Rust/Cargo available and the local spike present, run from the repository
root:

```sh
./spikes/obj-viewer/run.sh
```

The default model is `fixtures/obj/bracket.obj`. To choose a model or start empty:

```sh
./spikes/obj-viewer/run.sh fixtures/obj/cube-quads.obj
./spikes/obj-viewer/run.sh --empty
```

Use `./spikes/obj-viewer/run.sh --build` to create the app bundle without launching.
The launcher builds, ad-hoc signs locally, and opens the native app unless
`--build` is given. Dependency caches go into
`spikes/.cargo/`, Cargo output into `spikes/.target/`, and the app bundle and
review artifacts into `spikes/obj-viewer/`. All remain local and ignored.

Use **Open OBJ…**, **Cmd+O**, or drop an OBJ into the window to load another
model. Loading runs in the background. A successful load replaces the previous
model and frames the new one; an error leaves the last good model visible.

## Provisional controls

Camera gestures apply over the viewport. These are spike controls, not the
final modeling interaction design.

| Input                      | Action                                                                                  |
| -------------------------- | --------------------------------------------------------------------------------------- |
| Axis gizmo (top-right)     | Drag its body or any axis handle to orbit; click a handle for an orthographic axis view |
| Facing axis handle         | Click again to view from the opposite side                                              |
| Trackpad two-finger scroll | Orbit                                                                                   |
| Shift + two-finger scroll  | Pan                                                                                     |
| Pinch                      | Zoom                                                                                    |
| Two-finger twist           | Rotate camera yaw                                                                       |
| Left mouse drag            | Orbit                                                                                   |
| Shift + left mouse drag    | Pan                                                                                     |
| Right or middle mouse drag | Pan                                                                                     |
| Mouse wheel                | Zoom                                                                                    |
| F                          | Frame the model                                                                         |
| P                          | Toggle perspective/orthographic projection                                              |
| E                          | Toggle original polygon edges                                                           |
| G                          | Toggle ground grid                                                                      |
| 0 / 1 / 3 / 7              | Perspective / front / right / top view                                                  |
| Cmd+O                      | Open OBJ                                                                                |
| H                          | Toggle help                                                                             |

For a Magic Mouse or another mouse that emits precise scrolling, open **Help**
and enable **Precise scroll zooms (for Magic Mouse)**. By default, precise scroll
events use the trackpad orbit mapping.

The gizmo follows camera rotation, while pan and zoom leave its orientation
unchanged. Red is X, green is Y, and blue is Z; outlined handles mark negative
directions. Clicking preserves the camera target and zoom. Its area captures
input separately from viewport orbit/pan/zoom. Primary-pointer drags use the same
orbit sensitivity as viewport drags and continue outside the gizmo until release.
Releasing a drag does not snap to an axis; losing window focus cancels the drag.

Axis clicks and preset views animate by default over **120 ms**, easing into the
exact requested orientation. Perspective foreshortening also blends into the
orthographic view. In **Help**, turn **Animate axis views** off for instant
alignment, or adjust **Duration** (0–1000 ms; zero is instant). Duration changes
apply to the next request. Turning animation off completes the current request.
Dragging, panning, or zooming interrupts at the currently visible pose; another
axis request transitions from that pose to its new destination. Target and zoom
are preserved during alignment.

The camera API accepts `Transition::Instant` or `Transition::Animated { duration }`
for axis and preset view requests. Its caller supplies elapsed frame time through
`advance_transition`; the kernel has no dependency on egui, a window, or a wall
clock. The UI redraws while a transition is active, then returns to idle.

**Z-up** rotates the displayed geometry and its gizmo axes together for sources such as Blender. OBJ has no
standard up-axis; this control does not edit the source file.

## Geometry and notices

- Quads and supported n-gons are triangulated for surface rendering. The edge
  overlay follows original polygon boundaries without triangulation diagonals.
- Display geometry is centered and uniformly scaled so its largest extent is
  2.0. Source extents remain available as statistics; the file is unchanged.
- Materials and textures are ignored, with a notice when material records are
  present. The viewport uses a shared inspection material.
- Standalone OBJ lines and points are ignored with a notice.
- Warped polygons use projected triangulation with a notice: their displayed
  surface may differ from the exporting application. Detected crossing
  boundaries, degenerate polygons, invalid data, or input beyond the spike's
  declared limits can fail loading.

Hover the notice count to read import messages. The
[fixture notes](../../fixtures/README.md) record the original sample geometry and
expected counts. Loading and displaying a file establish no editing or export
guarantees.

## Validation status

The integrated build, strict Clippy check, and loader/camera tests passed. The
bracket fixture has rendered in the native app on Metal with an Apple M4 Pro,
and the native Open dialog has appeared. The axis gizmo has been visually
checked in the native window; automated camera and egui pointer tests cover
axis alignment, source up-axis conversion, opposite-view switching, and
body/handle dragging beyond the gizmo bounds without an unintended snap. Timed
transition tests cover exact endpoints, zero duration, retargeting, interruption,
frame-rate independence, and projection continuity. Completing a picker selection,
physical pinch/twist gestures, and manual drag-and-drop remain for user
validation. These observations do not establish full interaction or visual
acceptance, and the spike has not been promoted into the tracked application.
