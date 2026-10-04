---
name: docs-driven-development
description: Develop and review N3 features through its executable user guides. Use for user-visible behavior changes, guide scenarios and bindings, documentation drift, or extensions to replay, capture, and presentation.
---

# N3 docs-driven development

In N3, **DDD means docs-driven development**. Executable user documentation is
the primary development harness for documented user-facing behavior: a readable
account of using the product, backed
by assertions, real application input, and reproducible media. The generated
guide is the actual user guide. Maintainers and users read the same document;
do not create a parallel internal tutorial or acceptance-guide copy.

Use this skill when implementing or reviewing behavior worth explaining to a
user, maintaining an existing documented feature, or improving the harness.
An internal refactor need not invent a new tutorial. Existing behavioral and
unit tests still matter, particularly for cases a user guide should not enumerate.

Follow [AGENTS.md](../../../AGENTS.md). Read the relevant feature's scenario and
its registered narrative source before changing its behavior. Consult the
[pipeline contract](../../../docs/architecture/documentation-pipeline.md) for
capture defaults, encoding limits, and examples, and the
[executable document boundary](../../../docs/architecture/executable-documents.md)
for the maintained SDK and N3 adapter. This skill governs N3's workflow; the
portable SDK does not require it. Do not duplicate the API manuals here.

## The development and feedback loop

1. **Describe the experience.** Start from what the user wants to do, what they
   act on, what feedback they see, and how they finish or recover. Draft or adjust
   the owning user-guide narrative before or alongside implementation. Write in
   the user's vocabulary; implementation notes belong in architecture docs.
2. **Make the important promises executable.** Extend the matching scenario with
   meaningful outcomes. For a regression, reproduce the failure before fixing it
   when feasible. Assert the result the reader relies on, including relevant
   intermediate states, cancellation, selection, and undo boundaries. An image
   existing or a command returning successfully is insufficient evidence.
3. **Implement through the real application.** Setup can install fixtures and
   prepare initial state. The illustrated operation must run through production
   input routing, controls, and rendering. Use semantic shortcut replay and live
   control targets; do not mutate the expected final state into existence.
4. **Regenerate deliberately.** Investigate failures and changed captures before
   deciding whether behavior regressed or the intended contract changed. Change
   assertions and prose for a reason; never weaken them merely to make the
   output pass. Run `just docs update` on the native host when the new
   output is intentional. A renderer or image change requires regenerating and
   reviewing the entire guide baseline, not approving individual mismatches.
5. **Review as a user.** Open `just docs`, read the rendered instructions, inspect
   stills, and play animations. Check that the sequence is understandable, the
   controls are reachable, and the feedback is visible. Play the application for
   feedback that replay cannot establish, such as physical trackpad feel.
6. **Bring feedback into the same contract.** A maintainer can respond as a user:
   “I cannot tell what changed” is an actionable product observation. Identify
   the intended feedback, improve the application and guide together, and add
   an assertion or capture that detects the concrete regression where possible.
   Run `just verify` before handing off an implementation change. It runs the
   full Rust suite and strict guide comparison on the native host.
   Report what was checked and any remaining native-device or human-review boundary.

Do not force all exploratory ideas into implemented promises. Keep proposals
separate until the behavior is chosen. A guide-worthy feature is complete when
its user explanation and executable evidence land with its implementation.

## Sources of truth

Each guide feature has one scenario, one registry entry, and exactly one narrative
source. `hand-tool` and `navigation` author their narrative in Rust through the
N3 `Guide` adapter and register `template: None`. Other features retain their
registered templates. Do not keep both prose forms for one feature.

| Source                                                               | Owns                                                  |
| -------------------------------------------------------------------- | ----------------------------------------------------- |
| [docs/templates](../../../docs/templates/)                           | Retained legacy narratives and semantic bindings      |
| [src/documentation/scenarios](../../../src/documentation/scenarios/) | Real replay, assertions, and Rust-authored narratives |
| [features.rs](../../../src/documentation/features.rs)                | Feature registration and narrative-source selection   |
| [input/bindings.rs](../../../src/input/bindings.rs)                  | Production key bindings, labels, and replay inputs    |
| [ui/controls.rs](../../../src/ui/controls.rs)                        | Control identities and witnessed UI names/paths       |
| [docs/guide](../../../docs/guide/)                                   | Generated user pages, media, and evidence manifest    |

Edit the sources, never the generated guide or its media by hand. The browser
preview displays these same files; it does not own another content tree or
approve a baseline.
For paired Reader and Contributor review, `just docs build --out .cache/docs-candidate`
captures one replay into a fresh candidate tree without accepting the baseline.

`just fmt-docs` formats authored Markdown and `.md.in` templates through the
version-pinned `tools/format_docs.py` wrapper on the native host. It needs
Node.js/npm with `npx` there. The wrapper checks that all template bindings keep
their order before writing any template.
Rust-authored narratives follow Rust formatting. Generated guides are excluded;
when formatting changes narrative output, regenerate with `just docs update` and
review the resulting text through the same pipeline.

Keep the audience boundary explicit. Repository-owned, GitHub-hosted documents
such as `TODO.md`, contributor instructions, and engineering/research notes may
link to user docs. The user guide and its navigation must never link back to
those documents. A shared directory does not imply a shared audience; keep
planning and contributor references out of reader narratives and generated guide indexes.

Rust-authored guides use typed control, shortcut, and media handles from `Guide`.
Labels come from witnessed controls and canonical bindings; captures still run
through `Session`. A label or resource handle does not replace a meaningful
behavioral assertion. Keep the prose beside the replay and use the adapter's
finish step so registered and captured evidence stay accountable.

For retained templates:

- Use `{{shortcut:selection.duplicate}}` for an application shortcut. The binding
  registry supplies the `<kbd>` keys. Replay it with `Session::shortcut`, or
  `shortcut_down` / `shortcut_up` for a hold; use `shortcut_label` in a callout.
- Use `{{control:preferences.open}}` for a menu/control path. The scenario must
  witness the actual control and its parent scopes; the result renders as code.
- Use `{{key:Backspace}}` for literal typing behavior and `{{modifier:shift}}`
  for a physical gesture modifier. These are distinct from remappable actions.
- Use `{{value:name}}` for an asserted fact, not a copied key binding or a guessed
  result. Its existence alone does not prove that the value is correct.
- Reference captured output with `{{image:name}}` or `{{animation:name}}`.
  Callouts bind to live control IDs. Virtual cursors, key cues, and annotations
  observe input and paint; they must not change focus, hover, selection, or state.

## What must fail visibly

A documented contract must not silently drift. Preserve failures for violated
assertions, missing or unwitnessed controls, unknown bindings, incorrect media
kinds, missing or unreferenced captures, changed output, and orphaned artifacts.
Retiring a feature means explicitly reconciling its implementation, registry,
scenario, narrative source, references, and generated artifacts.

Treat a regression that escapes an intended assertion as a coverage problem to
repair. A page's existence does not prove every sentence or every possible
interaction: encode the important claims and retain user review for meaning,
legibility, and feel. Exact media checks expose changes; they do not decide
whether a change is an improvement. Do not claim cross-device pixel identity or
physical gesture recognition from headless replay.

`just docs check` verifies without writing. `just docs update` regenerates on
the native host, using Metal on macOS and Vulkan on Linux. `just docs` previews
through native Python. Developer commands and the pre-push verifier must not
require Docker. `just ci`, `just ci-test`, and `just ci-docs check/update` are
explicit opt-ins to the Ubuntu CI image through `tools/ci_runner.py`.

The guide illustrates macOS on either renderer: replay explicitly sets egui's
OS, and its static frame and bindings stay fixed. The manifest records the
renderer profile. Exact checks reject mismatched profiles rather than silently
skipping images; changing the baseline renderer requires intentional regeneration
and review. Do not add image tolerances, automatic updates, or missing-adapter
skips. The published Metal guide and exact lavapipe receipt are separate:
`just ci-docs update` writes ignored Linux review captures and the receipt, keeps
the guide read-only, and requires visual review followed by `just ci-docs check`.
The receipt pins every generated artifact and the complete published guide;
it does not establish cross-renderer pixel equivalence. Use `just test`
for the full native suite and `just test-metal` for focused capture checks.
For skill or hand-authored contributor-doc changes alone, validate instructions
and links; unchanged app/media output does not require regeneration.
For portable framework changes, `just doc-framework-test` provides focused SDK
and consumer checks; N3 adapter changes still require native replay and `just verify`.

## Evolve the harness when the user story needs it

The current scenario API is a tool, not the boundary of the product. When an
authorized feature cannot be truthfully exercised or clearly shown, extend the
owning harness layer instead of simplifying away its behavior, faking a capture,
or adding a second manual documentation path.

Read [evolving the harness](references/evolving-the-harness.md) when adding a
new input source, presentation format, capture surface, or interaction model.
It includes the future multi-window example. Choose the smallest reusable
capability justified by the real workflow, test its failure semantics, and keep
the same user narrative, replay, generated output, and strict verification loop.
