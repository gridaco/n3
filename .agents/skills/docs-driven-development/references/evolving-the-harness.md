# Evolving N3's documentation harness

Use this reference when a real feature exceeds the current replay or presentation
capabilities. These are extension criteria; they do not authorize building a
future feature merely because it appears here.

## Decide which capability is missing

Start with the user's observable workflow and the promise that needs evidence.
Distinguish a missing product behavior from a missing way to replay, assert, or
present behavior that the product supports. A more elaborate picture cannot
substitute for an absent application capability.

| Need                                             | First place to inspect                                                                                         |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| Input cannot be reproduced faithfully            | Production input adapter and [Session](../../../../src/documentation/mod.rs)                                   |
| A claim can regress without detection            | Scenario assertions and observable application state                                                           |
| A target or key name drifts                      | [Controls](../../../../src/ui/controls.rs), [bindings](../../../../src/input/bindings.rs), template resolution |
| Motion or ordering is unclear                    | [Tutorial clock and replay](../../../../src/documentation/tutorial.rs), virtual input, annotations             |
| State is correct but the image is unclear        | Saved layout and [presentation](../../../../src/documentation/presentation.rs)                                 |
| The renderer cannot capture the required surface | [Capture](../../../../src/documentation/capture.rs) and the owning production renderer                         |
| New output is not checked or attributable        | [Artifacts and manifest](../../../../src/documentation/artifacts.rs), encoding and inventory validation        |

A narrow change may only need a better assertion, an existing callout, or an
additional checkpoint. Introduce a shared concept when the workflow needs a new
kind of state, identity, timing, or composition. A second consumer is useful
evidence, but not a prerequisite when one real feature already needs the boundary.
Avoid a generic scenario language, plugin system, or speculative public API.

## Preserve these properties

- **The application remains authoritative.** Exercise its actual behavior through
  the ordinary routing. Setup and decorative composition cannot manufacture the
  result that the tutorial claims to demonstrate. Keep settings stores and fixture
  writes isolated from the maintainer's data.
- **Time and input remain explicit.** Preserve presses, holds, releases, focus,
  and event ordering. Capture and encoding must not advance state. If multiple
  surfaces share a moment, they must share a defined clock and checkpoint.
- **Presentation is separate from behavior.** Frames, cursor cues, and callouts
  may make the workflow readable; they do not deliver input or alter the model.
  Make any coordinate transform explicit so decoration cannot shift hit tests.
- **Evidence stays attributable.** Extend existing bindings and the manifest
  where needed to identify the source, relevant state, time, and composition.
  Missing or ambiguous inputs must produce useful errors.
- **Verification stays unified.** New stills or clips go through the existing
  artifact ownership and strict checks. All scenario/capture validation finishes
  before publishing generated output. Do not introduce a silent fallback to
  old media, manual screenshots, or a partially illustrated story.
- **The harness itself is tested.** Test behavior and meaningful failure cases
  at the new boundary, then exercise one real guide scenario end to end. A test
  that merely checks a helper returns an image is too weak.

Update the architecture contract when the extension changes it. Update this
skill only for durable authoring or design lessons; keep API details and numeric
limits in their owning code and contract.

## Future example: a tutorial involving two windows

**This is prospective guidance, not an implemented API.** The current Session
owns one workspace/context and captures one application surface. Its static
window frame composes that surface into the guide's WebP. A multi-window product
workflow will require revisiting these assumptions.

When that feature is requested, establish what the reader needs to understand:
for example, which window receives input and what changes in the other window.
Then implement enough of the production window/state model to demonstrate that
relationship. Two independent scenes placed side by side do not prove that the
windows actually interact or share the correct document state.

The capture extension could capture each real application surface separately at
an asserted checkpoint, then place both captures into a fixed arrangement and
encode one WebP, preserving that arrangement for every frame of an animation.
Inspect the production window model before choosing an API.
Useful design requirements for that implementation are:

- Stable surface identities, explicit focus and input ownership, and unambiguous
  control targets when the same control exists in both windows.
- Defined per-surface viewport sizes, pixel scales, and layouts. Captures must
  refer to the intended windows at the same logical checkpoint; reads of one
  surface must not advance another.
- A deterministic, reusable frame/composition template with explicit placement
  and coordinate mapping for each surface. Keep the source content legible.
  Prefer lossless placement; if resizing is necessary, make its policy explicit
  and review text and input cues at the final guide size.
- For an animation, synchronize input and presentation on the shared scenario
  timeline and keep a stable output canvas. Focus and pointer cues must clearly
  identify the window being operated, including cross-window gestures if the
  product supports them.
- Register the composite as one owned guide artifact with enough manifest data
  to trace its constituent captures and frame template. Intermediate buffers can
  stay internal; do not leave unreferenced images in the generated guide tree.

Test the composition with distinguishable source pixels, placements, dimensions,
and bounds. Test routing and the actual cross-window state effect separately.
Missing/duplicate surface IDs, ambiguous control targets, stale or mismatched
checkpoints, invalid dimensions, and memory/encoding overflow should fail
explicitly where applicable. A scenario failure must publish no new baseline.

Finish with the same public guide and strict check command. Review the composite
and animation as a user, and report any remaining native OS window/focus behavior
that headless replay does not establish. Do not build this capability during an
unrelated feature or skill-writing task.
