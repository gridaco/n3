# Working on N3

These instructions apply throughout the repository. N3 is a minimal mesh editor
built in Rust with wgpu, winit, and egui, starting with macOS. Keep the implemented
scope small and understandable while preserving room to evolve the interaction
design. Consult the current code and guide before treating an old proposal as a
decision.

## Product and interaction principles

- Focus on hard-surface mesh modeling. Broader modeling tools and speculative
  UX ideas are separate work; do not silently expand a task to add them.
- Keep actions semantic and key bindings separate. UI controls, shortcuts, and
  tutorial replay should reuse the same behavior rather than implement variants.
- Respect input ownership: text fields, popups, active gestures, and edit sessions
  must not accidentally trigger viewport commands. Keep tap, hold, and release
  semantics distinct. Do not introduce unrequested shortcuts.
- Allow an empty selection. Keep mode transitions and toggles symmetrical where
  applicable, and resolve Enter/Escape through the current interaction context.
- Prefer explicit, testable policies over hidden heuristics. Document the default
  and its rationale when adding snapping, selection, or navigation behavior.
- Performance is a priority. Preserve revision-based caches and separate selection
  feedback from geometry updates. Measure representative behavior before a broad
  optimization or architectural rewrite; do not claim speedups without evidence.

For shortcut ideation, ergonomic review, or competing key meanings, use the
[hotkey UX design skill](.agents/skills/ux-design-hotkeys/SKILL.md). It frames
context, learnability, research, and focused design questions; the binding catalog
remains the source of truth for the implemented keymap.

## Model and editing invariants

- N3's authored document owns editing semantics; versioned `.n3.json` is its
  canonical persistence contract. Formats are adapters, not modeling authorities.
  Preserve authored polygon boundaries, recipes, and stable IDs; display triangles
  and normalized render coordinates are derived data.
- **1 document unit = 1 centimeter.** Fractional lengths remain valid. Display-unit
  changes must not resize geometry. The base unit does not prescribe the snap or
  nudge increment; keep movement policy separate from coordinate storage.
- Preserve parametric primitives during evaluation, mode entry, and selection.
  An effective vertex edit materializes a mesh through the ordinary transaction;
  returning exactly to its edit-session baseline restores the original recipe.
  Never replace editable parameters merely to expose generated vertices.
- Keep document data, selection, navigation, transient UI state, previews, and
  committed history distinct. A continuous adjustment previews from a stable
  session baseline; acceptance records one undo step and cancellation restores
  the baseline. Reuse that session mechanism across interaction sources.
- Validate candidates before applying them and refresh derived state through the
  existing edit lifecycle. Do not bypass cache invalidation or history by adding
  an independent document-mutation path.

Imported glTF/GLB contents remain immutable, but their asset instances are ordinary
placed objects in the same authored document and editor. Placement, duplication,
and deletion use the existing transaction/history lifecycle. Keep decoded scenes,
playback, and evaluation caches outside history; the document stores source
references and scene indices, not embedded payloads. Opening resolves a snapshot;
saving must preserve references, and missing sources must retain recoverable objects.
Read-only access is an editor capability, never inferred from the imported format.
The imported scene model is not a future authoring schema. Keep parsing, resource
resolution, and source-unit conversion in `asset_io`; keep the scene model and
evaluator independent of format types and filesystem access. Read
[authoring and interchange](docs/architecture/authoring-and-interchange.md) for
representation boundaries and the [imported asset contract](docs/architecture/scene-viewer.md)
for loading, placement, playback, and PBR rendering.

Read [architecture](docs/architecture/architecture.md) and
[edit transactions](docs/architecture/edit-transactions.md) before changing these
boundaries. The current Editor still depends on egui and is not a public kernel API.

## Code organization

Keep N3 as one application crate. The workspace also maintains the portable
`executable-docs` SDK in `crates/doc-harness` and its consumer examples in
`crates/doc-example-config`; N3's documentation adapter is a real SDK consumer.
This extraction is justified by a tested boundary between application replay and
document composition/publication. It does not make the editor a public kernel.
Prefer focused modules over further speculative crates or APIs. See
[executable documents](docs/architecture/executable-documents.md) for this boundary.

Read [platform boundaries and performance](docs/architecture/platform-boundaries.md)
before changing host adapters or platform-dependent behavior. Share product
semantics, keep target selection at module boundaries, and preserve each host's
execution policy. Browser constraints must not lower the primary native target's
capabilities, performance policy, or verification baseline.

| Location                     | Responsibility                                                                |
| ---------------------------- | ----------------------------------------------------------------------------- |
| `src/model/`                 | Authored documents, units, native text codec, geometry evaluation             |
| `src/asset_io/`              | Format adapters, resource resolution, native document persistence             |
| `src/scene/`                 | Internal imported scene model, validation and pose evaluation                 |
| `src/scene_view.rs`          | Transient asset inspection and playback; separate from edit history           |
| `src/editor/`                | Selection, edits, sessions, history, transform policies                       |
| `src/input/`                 | Key mapping, semantic actions, pointer and navigation routing                 |
| `src/render/`                | Camera, wgpu rendering, derived visual feedback                               |
| `src/ui/`                    | Panels, controls, gizmo, rulers                                               |
| `src/native.rs`              | Window lifecycle, native events, dialogs, filesystem effects                  |
| `src/settings/`              | Typed global preferences, validation, merge policy, storage interface         |
| `src/documentation/`         | Headless replay, virtual input, annotations, capture, scenarios               |
| `crates/doc-harness/`        | Portable document composition, evidence, audience exports, artifact lifecycle |
| `crates/doc-example-config/` | Standalone CLI and library consumers of the documentation SDK                 |

Keep code, identifiers, comments, tests, logs, and authored UI strings in English.
Explain non-obvious behavior and platform limitations near the owning code.
Preserve unrelated work when making changes.

For changes to egui layout, theme, Preferences, or viewport overlays, consult the
repo-local [N3 egui skill](.agents/skills/egui/SKILL.md) and its observed pitfalls.

Keep global preferences separate from documents and edit history. Route settings
through the typed model and storage interface; filesystem paths, polling, and
opening files belong to the native host. Tests and tutorials must use isolated
stores, never the user's actual settings. See
[user settings](docs/architecture/user-settings.md) for persistence and merge rules.

## Documentation is part of implementation

Use the repo-local [docs-driven development skill](.agents/skills/docs-driven-development/SKILL.md)
for user-visible feature work, executable guides, and changes to their harness.
The user guide is a primary development and review contract: maintainers give
feedback on the same narrative and evidence that real users read. Important
promises must be executable and fail visibly when they drift.

Native and web share one feature UI and user guide. Follow the
[shared product contract](docs/architecture/platform-boundaries.md#one-product-experience)
for capability-based availability and the boundary with contributor documentation.

Every guide feature owns one Rust scenario in `src/documentation/scenarios/`
and exactly one narrative source, selected in `src/documentation/features.rs`.
Rust-authored guides compose their narrative in the scenario through the N3
`Guide` adapter and register `template: None`; retained legacy features use one
`docs/templates/*.md.in` template. `hand-tool` and `navigation` are Rust-authored.
Do not keep a parallel template or generated-prose copy for an authored guide.
Edit the registered source; never hand-edit pages, media, or the manifest in
`docs/guide/`.

- Assert actual behavior, including meaningful intermediate states for a gesture.
  Fixture setup may prepare state; an illustrated interaction must use production
  input routing and the real UI/renderer.
- Bind labels and paths to witnessed controls. Bind callouts to live control IDs,
  not stale screen coordinates. Virtual input and annotations only paint; they
  must not alter focus, hover, selection, or document state.
- Rust-authored guides use the `Guide` adapter's typed control, shortcut, and
  captured-resource handles. A presented label is not evidence that an action
  ran; retain the actual replay and behavioral assertions.
- In retained templates, use `{{shortcut:semantic-id}}` for application shortcuts,
  `{{control:id}}` for witnessed menu paths, and `{{key:Name}}` / `{{modifier:name}}` for literal
  text-entry keys or physical gesture modifiers. Do not copy remappable keys
  into prose or scenario values. Replay actions with `Session::shortcut` and its
  held-key helpers; reserve raw keys for literal input or intentional ownership
  probes. The canonical input bindings supply the rendered key labels.
- Use the explicit scenario clock and timed replay for animation. Still and
  animated media use lossless WebP through the same strict pipeline. Do not add a
  separate manual screenshot or animation-generation path for the guide.
- Keep checks strict: missing, orphaned, duplicate, unreferenced, or changed
  artifacts fail. Do not add silent baseline updates, visual tolerances, or skips
  for unavailable GPU access. Investigate drift before regenerating.
- Review generated text and media. Assertions and byte comparisons do not prove
  prose accuracy, legibility, useful pacing, or physical trackpad recognition.

The [pipeline contract](docs/architecture/documentation-pipeline.md) documents
replay, timing, bindings, overlays, encoding limits, and authoring examples.
`just docs build --out .cache/docs-candidate` runs one replay and writes fresh
Reader and Contributor review trees. It does not accept a canonical baseline;
`just docs check/update` retain their existing reader-guide roles.

## Iteration and verification

For an implementation change:

1. Identify the owning model/input/UI boundary and any affected guide scenario.
2. Make a focused change and add meaningful behavioral regressions where needed.
   For visible interactions, update the owning scenario and its registered
   narrative together.
3. If guide output intentionally changes, run `just docs update`, then inspect the
   generated text, stills, and animation playback before accepting the baseline.
   Use the native host renderer; local development must not require CI infrastructure.
4. Run `just verify`: formatting, Clippy with warnings denied, tooling checks, and
   all Rust tests, including required documentation replay and exact artifact checks,
   on the native host.
5. Report the result, validation performed, and any remaining manual checks. Do
   not equate headless replay with physical-device testing.

Contributor setup is `just setup` (Node.js 24/npm with `npx`, a warmed cache for
the Oxfmt version pinned in `tools/format_docs.py`, and a repository-local
pre-push hook). `just fmt` formats Rust and authored docs;
`TODO.md` is excluded. Generated guide files are excluded from formatting: format
the authored Rust scenario or retained template, then regenerate through the
ordinary pipeline when its output intentionally changes.
The pre-push hook verifies a clean current `HEAD`, not arbitrary refs or dirty
working content. `just test`, `just docs update/check`, and `just verify` run
natively, as does the pre-push hook. Developer experience comes first: never make
these commands depend on Docker or a CI runner. `just ci`, `just ci-test`, and
`just ci-docs check/update` explicitly opt into the pinned Ubuntu 24.04
`linux/amd64` Docker environment through `tools/ci_runner.py`.
Exact media checks require a baseline from the same renderer. Ubuntu uses a
separate exact lavapipe receipt pinned to the complete published guide; its
explicit update writes ignored review captures while preserving the native
guide. A missing, stale or mismatched baseline must fail clearly, never silently
skip images or introduce tolerances.
The macOS CI lane runs
`just ci-macos` for the native bundle, ad-hoc signing verification, and focused
`native::` tests. `just test-metal` is a local real-Metal check when a Mac GPU is
available. See the development guide for these boundaries.

For changes limited to hand-authored documentation such as this file or the
README, check links and factual claims; unchanged application/media output does
not require rebuilding the renderer or regenerating the guide.

Use the root `justfile` for development commands (`just` 1.46 is the verified
version). Useful commands from the repository root:

```sh
just run --empty
just build
just check
just test
just docs
just docs update
just docs check
just verify
```

`just docs` opens a local browser preview of the existing guide using Python 3;
it does not regenerate or verify the guide. Ctrl-C stops the server. Use
`just docs check` for read-only replay verification and `just docs update` for
intentional regeneration. `just docs-preview-test` checks the preview server and
is included in `just verify`. Both replay commands run natively; only
`docs update` can write `docs/guide/`. Use `just cargo ARGS...` for focused native
Cargo commands not covered by a named recipe. After dependencies are available,
use `CARGO_NET_OFFLINE=true` for offline runs. Native recipes respect explicit
`CARGO_HOME` and `CARGO_TARGET_DIR` overrides; the optional CI container keeps
separate caches. See
[development](docs/development.md) for environment and format details.

## Repository and documentation boundaries

- Keep the README focused on the product, actual capabilities, and getting started.
  Put durable contributor instructions here, engineering rationale in
  `docs/architecture/`, concrete proposals in [rfcs/](rfcs/README.md), and
  exploratory or historical research in `docs/research/`. Link detailed contracts
  instead of copying them into each file.
- Keep larger deferred work and ideas in [TODO.md](TODO.md). Keep entries brief,
  link dedicated proposals or research when they exist, and narrow or remove
  items as they land. Use an RFC when a short item would lose meaningful
  rationale, alternatives, or open questions; self-explanatory keywords can stay
  in TODO. Record proposal status explicitly and do not infer implementation
  approval from its existence. See the RFC index for lightweight conventions.
- Repository-owned documentation (GitHub-hosted docs) includes `TODO.md`, the root
  README, contributor instructions, RFCs, and engineering/research documents.
  These may reference user-facing docs. User-facing guides and their navigation
  must never reference repository-owned docs. Audience determines this boundary;
  a path under `docs/` does not automatically make a document user-facing.
- Keep canonical fixtures and their provenance in `fixtures/`, and native sample
  documents in `examples/`. Preserve upstream attribution when adding assets.
- Disposable experiments belong in ignored `spikes/`. Promote reviewed work into
  the main source tree; maintained code must not depend on ignored experiments.
- Keep dependency caches, build output, bundles, and local review captures ignored
  (`.cache/`, `target/`, `build/`). Generated user-guide media belongs in the repository.
- Source promotion is separate from committing, publishing, or releasing. Do not
  infer authorization for those actions from a request to promote an experiment.
