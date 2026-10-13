# Executable documents and the N3 adapter

The [lgtmd SDK](https://github.com/lgtmd/lgtmd) separates document
composition and artifact publication from the application being documented.
N3 remains one application crate and consumes lgtmd from an exact Git revision.
The SDK, its Node bridge, and portable Rust/React examples are maintained in the
lgtmd repository. N3 owns its application adapter, replay, renderer, scenarios,
and guide artifacts here. Neither SDK use nor its examples requires N3's renderer,
UI stack, website, or agent skill.

The SDK remains experimental and unpublished. Moving its source establishes
independent repository ownership; it does not establish a public release or a
stable API. A future registry dependency can replace the Git pin after the
package passes its release gates.

## The shared contract

An author writes a user explanation while ordinary Rust or Node code executes
behavior and checks important promises. Explanation-only pages are valid.
`observe` freezes a value without implying a check; successful named checks and
host outcomes remain distinct. The completed document owns its prose,
observations, checks, resources, and optional contributor notes. Reader and Contributor
exports reuse those frozen results; rendering another audience does not rerun the
application or recapture resources.

The SDK validates typed handle ownership, evidence identity, resource use,
audience visibility, and generated references. Its renderer can export Markdown,
arbitrary resources, and a structured manifest. Text, JSON, images, or other
file types are artifact choices supplied by the consumer. The SDK does not infer
correct application behavior from an artifact's existence or a declared MIME
type. Meaningful assertions, producer metadata, fixtures, runtime drivers,
review, and documentation organization remain the consumer's responsibility.

The [SDK README](https://github.com/lgtmd/lgtmd/blob/main/crates/lgtmd/README.md) is the API and manifest
reference. N3's [documentation pipeline](documentation-pipeline.md) owns product
replay, timing, captures, and renderer-specific evidence. This document records
the boundary between them.

The current public exporter targets Markdown. Artifact producers are generic,
but there is no public structured document-view API for a different publisher
yet. Prove that interface with an actual alternate exporter before exposing it;
it must preserve audience filtering, typed references, observations, and resource
ownership together. The private document model is the extension boundary, without
a plugin registry or a promise of arbitrary presentation support today.

## Internal Rust ownership

The SDK remains one crate. Its private `model` owns frozen document data and typed
handles; `authoring` executes builder operations and assertions; `artifact` owns
frozen resource bytes and explicit acquisition. CommonMark parsing and generated
HTML provenance live in `markdown`, bundle paths in `paths`, and intrinsic
audience visibility in `validation`. Builder validation does not depend on the
Markdown exporter.

`render` composes the selected audience and verifies its complete export.
`manifest` owns typed receipt emission and validation of the existing JSON
inventory. It preserves unknown-field compatibility and distinguishes explicit
JSON null observations from omitted private payloads. `store` applies that receipt
policy to `lifecycle` filesystem operations; `runner` coordinates project-owned
generation and publication. These boundaries add no public serialized document
model or host executor.

## Project-owned configuration

The SDK's layout is a default, not a required repository topology. `ExportLayout`
configures page and resource prefixes and exact per-document page paths. Complete
exports resolve typed resource links relative to their actual page locations;
authored local links must remain inside the exported bundle. Explicit
`resource_at` paths retain their bundle-relative names. Finishing a document
validates its intrinsic contract, while export validates the configured paths,
resource use, and audience-specific links before returning publishable bytes.

`RunnerConfig` captures a caller-selected path base, retained output paths per
audience, and directories for those audiences under a build candidate. Destinations
may live outside the source project. `runner::run` supports direct test/build-tool
integration; `run_cli_with` is an optional argument adapter. The original `run_cli`
and `render_many` retain their defaults. There is no Cargo or Git discovery, and
no required source, scenario, fixture, website, or agent-skill directory. The host
owns generation and fixture/process setup.

Configuration changes placement rather than integrity: manifest and ownership
metadata retain their protocol names, audience privacy and exact comparison stay
strict, all declared paths must be collision-free, and output trees cannot overlap.
That separation includes the adjacent lock, staging, and backup paths reserved by
each output's transaction. Artifact inventory paths use `/` on every host.
New audience trees must also have a provable path distinction before any private
bytes are written. Existing directory identities establish separation; missing
paths need distinct ASCII components beyond any shared ancestors. Case-only or
Unicode-only distinctions require preexisting separate directories rather than
guessing a filesystem's alias rules. Unicode parent directories and later path
components remain supported.
The [configured SDK example](https://github.com/lgtmd/lgtmd/blob/main/crates/lgtmd/examples/configured.rs)
exercises a source-only host, external baselines, nested pages, renamed audience
directories, private resources, and preservation of unrelated files. This boundary
does not add a configuration file format, plugin loader, or repository abstraction.

## N3 ownership

| Layer                                                        | Responsibility                                                                                                                               |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------- |
| `Session` and the production application                     | Real input routing, fixture setup, witnessed controls, behavioral assertions, explicit time, rendering, and capture.                         |
| [Guide adapter](../../src/documentation/guide.rs)            | Convert witnessed controls, canonical key labels, and captured N3 media into typed SDK bindings and resources.                               |
| SDK `Doc` and `Document`                                     | Compose narrative, retain immutable evidence and contributor notes, validate handles, and render the selected audience.                      |
| [N3 artifact pipeline](../../src/documentation/artifacts.rs) | Select the registered narrative, assemble the existing guide tree and text manifest, validate guide-only links, and enforce renderer policy. |
| SDK `lifecycle`                                              | Validate output paths and inventories, compare complete trees, stage updates, publish, and retain recovery material on interruption.         |

`Guide::control` requires that the same replay witnessed the control. Shortcut
labels come from the production binding catalog; requesting a label does not
deliver input or prove that its action ran. Scenarios still use
`Session::shortcut` and the normal input helpers for illustrated behavior.

The adapter's capture methods delegate to `Session`. Registering already captured
media freezes its existing bytes without another render or clock advance. N3
continues to distinguish stills from animations even though both use WebP. Before
finishing, the adapter checks that every captured media path was registered and
that each audience's referenced resources exactly match the captured paths and
bytes. N3's existing manifest requires every capture in the reader narrative, so
this adapter currently supports public captured media and private prose notes.
Contributor-only captures or additional non-capture resources fail explicitly.
That restriction belongs to N3's adapter; full SDK exports support private
resources and arbitrary artifact types.

N3's established `Session::require` calls remain the behavioral assertions.
`Guide::finish` imports their successful execution into the SDK document and keeps
the existing fact text in N3's manifest. It does not replace replay with checks
against a manufactured final state. A Rust-authored scenario must finish exactly
one authored document, and its ID must match the registered feature.

## Exactly one narrative source

Every feature owns one scenario and one registry entry in
[features.rs](../../src/documentation/features.rs). The registry selects exactly
one source of user prose:

| Registration          | Narrative source                                                                 |
| --------------------- | -------------------------------------------------------------------------------- |
| `template: None`      | The scenario composes a `Guide` and finishes its SDK document.                   |
| `template: Some(...)` | A retained `.md.in` template binds the scenario's witnessed values and captures. |

Both sources present or both absent is an error. Inventory checks reject orphaned
templates and scenarios. A source migration removes the old template instead of
keeping a second prose copy. Existing template-based features remain supported;
an unrelated behavior fix does not require a broader migration.

The [hand-tool](../../src/documentation/scenarios/hand_tool.rs) and
[navigation](../../src/documentation/scenarios/navigation.rs) guides now author
their narrative in Rust alongside the scenario. The other 26 registered features
still use templates. Their eventual migration remains work to schedule and
review, not evidence that all N3 authoring has already moved to the SDK.

For these two migrations, N3 uses `Document::render_fragment` to retain the
existing title, whitespace, media paths, and generated-file preamble. This mode
does not insert SDK page titles or block anchors. The N3 artifact pipeline still
validates links across the complete guide. Normal SDK `render_many` exports keep
their own complete-bundle validation. The existing N3 guide baseline, media, and
`manifest.txt` format are retained rather than converted to SDK manifest JSON.

## Shared publication with explicit ownership

N3 and the portable consumers share the SDK's `lifecycle` implementation while
retaining their own manifest and compatibility policies:

- **Dedicated ownership:** N3's generated guide is a dedicated tree. Every
  existing file is treated as owned output, absent candidate paths require
  explicit retirement, and publication adds no `.ownership.json` file. This
  preserves the established guide format.
- **Recorded ownership:** SDK `store` exports carry `.ownership.json` and the
  structured SDK manifest. Their wrapper checks schema, audience, and retained
  resource producer/profile compatibility in addition to exact tree contents.

Generation, assertions, rendering, and candidate validation finish before
publication begins. Checking is read-only. Building needs a new destination.
Updating explicitly stages complete output before replacing the compatible
baseline. Ordinary publication errors preserve or restore the previous tree;
this is not a crash-atomic filesystem transaction. Interrupted transactions
retain recovery material and fail visibly. The SDK runner publishes each audience
destination independently.

N3's `just docs build --out .cache/docs-candidate` performs one complete replay
and exports `reader/` and `contributor/` under a fresh candidate directory. Both
views render from the same completed sessions and frozen media; neither view
executes the application a second time. The two Rust-authored guides add
contributor notes, while the 26 retained template pages are identical in both
views. N3 prepares both subtrees and publishes them as one dedicated candidate
tree. The destination must not overlap `docs/guide` or `docs/baselines`, including
their ancestors or descendants. This review command does not accept or update
the canonical baseline.

N3 keeps its renderer-profile check and separate lavapipe receipt policy. An
explicit `just docs update` can establish an intentionally changed native
baseline; checking never silently accepts a renderer change, skips images, or
adds tolerances. A successful update records bytes and still requires human
review. It does not approve product behavior, publication, or a release.

## Verification and remaining validation

N3's `just verify` checks its formatting, Clippy, tooling, full Rust suite,
measurement contracts, and exact guide replay on the native renderer. An SDK
revision update goes through this same gate; changes to its API must remain
explicit in the N3 adapter and scenarios.

The SDK owns its independent
[verification workflow](https://github.com/lgtmd/lgtmd/blob/main/AGENTS.md),
including portable sources, Rust/Node tests, its own examples, and installed
Cargo/npm consumers. Those gates run in the lgtmd repository rather than through
N3's application commands or CI. They supplement consumer verification and do
not establish N3 behavior or renderer compatibility.

The SDK's static preview validates the complete export receipt before displaying
it. Page identity comes from that inventory rather than file extensions;
downloadable Markdown remains a resource. HTML conversion preflights collisions
and preserves audience separation. The preview is disposable review output, not
an accepted baseline or a public viewer-bundle contract.

N3's `just docs check` and `just verify` retain real replay and exact guide checks
on the native renderer; portable checks do not replace them.

The examples establish local integration with a CLI and an independently existing
Rust library. The serde_json guide is our authored integration, not external
maintainer adoption. Validation with independent project owners, the remaining
26 N3 source migrations, and longer-term API and schema compatibility remain
future work. The framework detects encoded behavioral and artifact drift;
prose clarity, meaningful coverage, visual legibility, and physical interaction
still need human review.

## Node and web integration

[packages/lgtmd](https://github.com/lgtmd/lgtmd/blob/main/packages/lgtmd/README.md) authors documents in Node 24 ESM;
[crates/lgtmd-node](https://github.com/lgtmd/lgtmd/blob/main/crates/lgtmd-node/Cargo.toml) is its coarse native bridge.
One serialized request per compile/publication carries schema-v1 frozen input to
the same Rust validation, Markdown exporters, runner and owned store. The wire
contains complete ordered content and resource snapshots, never process-local
handle owners or application callbacks. Rust transport round trips preserve
existing native exports, including literal Markdown versus typed parts. Built-in
JSON resources use one Rust encoder, including those registered from Node.

The [three React/Vite examples](https://github.com/lgtmd/lgtmd/blob/main/examples/lgtmd-web/README.md) exercise prose-only
authoring, application-owned async preparation/cleanup, supplied context,
observations, native checks, private notes/resources, JSON and CSV downloads, and
Playwright screenshot evidence. The React app uses ordinary Vite development and
production builds, with a project-owned HTTP API and isolated test workspaces.
Documentation captures run the built app through Vite preview on separate test
ports; the development servers can remain open.
The browser bundle imports no SDK code, and the examples import no N3 application
code. The optional
adapter extends a project's existing Playwright fixtures and assertions. It
collects candidate attachments and publishes only after the real runner process
and guarded reporters complete successfully. It initially supports one project,
no retries/repetition/sharding, and clean passed outcomes for all discovered tests.
Application state, services, seeds and DOM query correctness remain project-owned.

WASM was probed with the same compiler input and produced equal exports, but its
filesystem store returned an unsupported-platform error. The selected native
route retains the existing strict filesystem contract. It requires platform
binaries in npm tarballs; installation performs no Rust build. Node 24 ESM on
macOS arm64 is the executed local scope. Linux x64 has CI wiring, not a completed
runtime claim. A full viewer and browser-hosted SDK are deferred.

The SDK repository owns explicit browser setup and its Node/Chromium verification
workflow. It checks the native bridge, authoring and failure semantics, real
browser replay, exact repeated exports, and clean Cargo/npm consumers independently
of N3. The npm consumer installs with Rust absent from PATH and imports the core
before installing the optional Playwright peer. N3's existing browser editor and
its native documentation adapter retain their own host policies and checks.
