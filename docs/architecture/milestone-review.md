# Promotion review

This milestone promotes the editor from an ignored experiment into one maintained
`n3` package. It is a source-organization checkpoint, not a promise of a stable
extension API, completed modeling tools, or browser support.

## Resolved during cleanup

- Native and tutorial navigation now share one routing adapter. Gesture cues are
  derived from the same event; tutorials cannot bypass popup, focus, or pointer
  ownership while independently illustrating a scroll or pinch.
- Timed clips, live-control callouts, and lossless animated WebP use the existing
  renderer and strict artifact check. Three walkthroughs cover gizmo navigation,
  held Space panning, and box selection through release. A clip explicitly
  establishes its starting state and leaves time to inspect its result.
- Clean still capture now hides only the tutorial overlay. It no longer sends a
  synthetic pointer-exit event that silently changes hover and input state.
- Menu traversal is explicit in scenarios. The harness no longer maintains an
  implicit second menu hierarchy.
- Artifact insertion is atomic on a duplicate-owner error, and normalized path
  aliases are rejected. Renderer identity is reported at runtime rather than
  embedded in deterministic guide text.
- Numeric transforms and pointer transforms now share the normalized selection
  pivot calculation. Averaging large absolute world coordinates could overflow
  for otherwise valid geometry; both paths now compute the same display-space
  center and convert it once. A regression covers object and vertex transforms
  at large world offsets, including acceptance and undo.
- Tutorial drag cues use the editor's shared drag threshold. Crossing that
  threshold remains a drag even if the pointer returns to its starting position.
- Virtual held and released key cues preserve top-row and numpad identity.
  Pressing both sources for the same logical digit no longer collapses them into
  one held key or releases the remaining source prematurely.
- The large inline test suites were moved into private child modules. Their
  assertions and access to implementation details remain unchanged.
- The legacy display-only OBJ loader is entirely test-only, and `tobj` is a
  development dependency. Application import continues through the canonical
  document importer.

## Promotion checks

Relocation must preserve fixture lookup, scenario registration, authored template
lookup, generated output paths, and runnable native/documentation commands. Run
validation from the new root package rather than relying on a prior spike build.
Keep build caches, old application bundles, and local review artifacts ignored.

Generated documentation remains a required check. Hardware names are runtime
provenance, not deterministic artifact identity; the documented rendering profile
still matters, and identical pixels across arbitrary GPU drivers are not promised.
Regenerate and review media before accepting a changed baseline. Strict byte
comparison detects drift but cannot establish that arbitrary prose is correct or
that an animation has useful pacing.

Pure tests and compilation can run without a graphics adapter. GPU capture and
integrated interaction scenarios require a real Metal adapter and must not be
silently skipped when one is unavailable. Native trackpad recognition and device
feel remain separate manual acceptance gates. A headless replay of scroll,
pinch, or twist verifies routing after recognition, not human-finger recognition.

## Verified at this checkpoint

On September 29, 2026, from the promoted root package on macOS with Rust 1.92:

- `./scripts/verify.sh` passed formatting, warning-free Clippy, and all 384 tests,
  including the required Metal documentation replay and exact artifact check.
- The guide contains 16 verified feature scenarios and 70 generated artifacts,
  including three animated walkthroughs. Their sampled frames were visually
  reviewed for input cues, callouts, and the resulting editor state.
- Independent Pillow decoding compared optimized animation playback against
  full-canvas references at all 192 authored sample boundaries. RGBA pixels,
  duration, dimensions, and looping were identical. Changed-region encoding and
  coalesced holds reduced the three clips from 31,644,844 to 7,973,584 bytes.
- The native bundle built successfully; its local signature and property list
  passed validation. Both the native cube example and Suzanne OBJ passed the
  command-line document validator.
- Launch-script syntax, executable flags, documentation links, scenario/template
  inventories, and ignored build/cache boundaries were checked. Nothing was
  staged or committed by this promotion.

These are checkpoint results, not substitutes for rerunning verification after
future changes. They used the then-current shell launchers, which have since been
retired; the current verification command is `just verify`. See
[development commands](../development.md) for the maintained workflow. This review
did not retest physical device gestures.

## Deliberately deferred

- A public model/editing library or multiple crates. The present editor is still
  egui-coupled; see [architecture](architecture.md) for the seam to establish first.
- A fully enforced document mutation API. Current application paths invalidate
  caches and record history correctly, but a future public consumer must not be
  able to bypass those responsibilities through unrestricted fields.
- A performance redesign without measurements. Continue using revisions and
  separate feedback state; profile representative models before replacing the
  cache, picking, or renderer design.
- Broader OBJ attributes and export fidelity, topology construction, edge/face
  editing, x-ray selection, and geometry snapping. These are feature decisions,
  not requirements for moving reviewed source into the main project.

The promotion retains the text document, explicit sessions, semantic input
actions, and executable user guides as foundations. It leaves these boundaries
changeable while their actual consumers and UX continue to develop.
