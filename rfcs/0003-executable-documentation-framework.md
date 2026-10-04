# RFC 0003: A reusable executable documentation framework

Status: **Implemented locally — experimental SDK and N3 bridge; independent adoption and release remain open**

Date: 2026-10-04

Incubate this work inside N3, with an eventual move to a dedicated repository.
The user authorized local implementation after reviewing the initial plan. The
maintained SDK and representative integrations now live in this repository;
[the architecture contract](../docs/architecture/executable-documents.md) records
the implemented boundary. Broader design sketches below remain proposals where
they exceed that boundary. The provisional package is unpublished.

Authoring requirements: concise, honest authoring and scenarios that write the
document's text, resources, and explicit contributor notes themselves. The primary
model below is an executable document; separate text templates are an optional
integration or migration facility. The maintained SDK README is the source for
current syntax; the findings below distinguish implemented behavior from the
broader design and remaining adoption questions.

## Recommendation

Build a Rust-first framework for documentation whose important promises are
connected to executable scenarios and retained evidence. Ship a small SDK and
runner interface first. An optional skill explains the workflow; the framework
works without an agent, skill, particular editor, or documentation website.

The unit of adoption is a **documented capability**: an explanation of something
a developer wants users to understand and continue relying on. Its contract
connects authored intent, real execution, selected assertions, and the evidence
presented to the reader. A capability can produce values, command transcripts,
structured data, file examples, diagrams, images, or animation. A renderer is one
possible producer, not the framework's organizing abstraction.

Authors express that capability as an **executable document**: ordinary Rust
that composes headings, paragraphs, resources, and contributor notes alongside product actions and
checks. A reader's explanation and its demonstrated behavior are authored
together. The framework handles evidence registration and reference bookkeeping
without requiring a separate template or a duplicate list of resource names.

The development loop is:

1. Describe the intended experience, including relevant failure or recovery.
2. Exercise the real behavior and assert the promises worth preserving.
3. Compose the explanation with observed names, checked results, and captured
   output directly in the document function.
4. Generate the documentation and inspect it as a reader.
5. Retain the reviewed output and fail future checks when the contract drifts.

This complements unit, property, integration, and performance tests. Developers
choose which behavior deserves a documented contract; internal implementation
details and exhaustive edge cases need not become user tutorials. The value is
that adding a documented feature also leaves an executable account of its intent.
Source control retains the history; the framework need not become an archive
service or prescribe that every run be stored forever.

## Pre-extraction N3 baseline

The pipeline and development skill established the working precedent. The
following observations describe N3 before this extraction; the linked source
files now contain the maintained implementation described later in this RFC.
See the [current pipeline](../docs/architecture/documentation-pipeline.md) and
[development skill](../.agents/skills/docs-driven-development/SKILL.md) for the
updated contract.

| Current source                                                 | Useful foundation                                                                         | Coupling or limitation                                                                   |
| -------------------------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| [Feature registry](../src/documentation/features.rs)           | Stable feature identity, scenario, and narrative registration                             | One N3 scenario and template per page                                                    |
| [Session](../src/documentation/mod.rs)                         | Real interactions, assertions, values, control witnesses, captures                        | Owns N3 workspace state, egui, input, and capture                                        |
| [Artifact pipeline](../src/documentation/artifacts.rs)         | Reference resolution, unique paths, complete inventory, exact comparison, explicit update | Constructs a GPU capture before running any feature; templates know N3 controls and WebP |
| [Replay and animation](../src/documentation/tutorial.rs)       | Explicit time and meaningful intermediate states                                          | N3 input and frame sampling policy                                                       |
| [Capture](../src/documentation/capture.rs)                     | Production rendering and separate UI-only capture                                         | wgpu, egui, scene rendering, window composition                                          |
| [Renderer receipts](../src/documentation/renderer_baseline.rs) | Independent exact baselines tied to the published guide                                   | N3 renderer profiles and output paths                                                    |

The artifact collection already stored paths and arbitrary bytes. The reusable
seam was collection, validation, composition, and verification of evidence. N3's
split between scenario and text template was an existing authoring choice,
not a constraint to reproduce in the SDK.
Exporting `Session` would export much of N3 with it.

Several stronger properties required new work at that point:

- `Session::require` recorded a successful assertion as a string. `value` inserted
  a string independently. There was no typed claim-to-value relationship.
- At least one assertion is required per feature. That does not establish that
  every sentence, value, or image is meaningfully covered.
- Generation and validation finished before publication, but files were then
  written sequentially. An I/O failure could interrupt an update without the
  staged replacement and recovery now provided by the shared lifecycle.
- The manifest identifies a broad renderer profile. It is not a complete record
  of all driver, dependency, and execution-environment inputs.

Preserve the existing guarantees during extraction. Treat stronger provenance,
publication recovery, and broader output support as explicit additions.

## The contract model

Keep the model small enough to author without a specification bureaucracy:

| Concept     | Meaning                                                            | Example                                            |
| ----------- | ------------------------------------------------------------------ | -------------------------------------------------- |
| Narrative   | Human-authored purpose and explanation                             | "An explicit port overrides the default."          |
| Scenario    | Code exercising behavior within an executable document             | Parse configuration through the public library API |
| Claim       | A stable identity for a selected promise                           | `explicit-port`                                    |
| Check       | An executed assertion supporting that claim                        | Parsed port equals the supplied port               |
| Observation | A value or name obtained during execution                          | A port value or witnessed UI label                 |
| Artifact    | Retained output with identity and format                           | JSON result, transcript, PNG, SVG, or WebP         |
| Note        | Authored explanation for contributors, excluded from reader output | Rationale for an assertion or capture choice       |
| Document    | Authored blocks, references, resources, and contributor notes      | Reader and contributor views of the same document  |
| Baseline    | Output retained for subsequent comparison                          | Reviewed pages, assets, and manifest               |

A named assertion creates a claim record; authors need not repeat every sentence
in a separate claim file. Checked-value handles connect rendered values to the
assertions that produced them. More general assertions can return claim handles
to which artifacts are explicitly associated. Stable IDs belong to a contract
namespace; titles and file paths can change without silently changing identity.
These are internal relationships and a few authoring primitives, not a checklist
of separate objects the author must manually wire together. Literal prose needs no
ID. Declare stable IDs once for contracts, claims, and retained resources; reuse
their handles. Do not derive persistent IDs from line numbers or call order.

The evidence manifest records these relationships. It can establish that a
required check ran, that a value came from that check, and that an artifact was
emitted during the scenario. It cannot establish that the assertion is sufficient,
that a picture proves a claim, or that arbitrary prose is true. Those remain
authoring and review responsibilities. A snapshot alone is an observation, not a
behavioral assertion.

Bindings also need an honest origin. A label read from a production registry and
a control actually witnessed in a running UI are different evidence. Preserve
that distinction without requiring every project to have UI controls.

Each runnable contract requires a successful behavioral check. Introductory prose
and navigation can compose these contracts without inventing assertions of their
own. Several scenarios may support one page; the framework should not bake N3's
one-feature/one-page convention into artifact identity or the saved manifest.

## Product and ownership boundaries

Start with **one unpublished Rust library** with focused modules for document
composition, evidence, manifests, and verification. Add a small runner API to the
same package. Separate packages only when dependencies or consumers justify them.

| Framework owns                                               | Adopting project owns                                      |
| ------------------------------------------------------------ | ---------------------------------------------------------- |
| Registration, scoped identities, and execution results       | Which capabilities deserve documentation                   |
| Claim/check records and artifact reference accounting        | Meaningful assertions and fixture preparation              |
| Checked values, typed inline references, and document blocks | Public API calls, CLI execution, or application automation |
| Artifact inventory, hashes, and declared producer profiles   | Rendering, capture, encoding, and format semantics         |
| Deterministic document composition and baseline comparison   | Documentation hierarchy, prose, branding, and hosting      |
| Read-only check and explicit update interfaces               | Review decisions, CI invocation, and optional agent skills |

Use ordinary Rust functions or closures. Projects may capture their existing
driver or construct it inside a scenario. Avoid a universal `World`, `Driver`,
`Renderer`, scheduling engine, or scenario language until a real integration
requires one. Rust is the first scenario-authoring language; a Rust runner may
exercise a non-Rust CLI, but that is not a second language SDK.

The core accepts produced output through a narrow artifact API: contract/local
ID, media type, bytes or a staged file, producer identity, comparison profile,
and links to checks. Streaming or staged files prevent arbitrary media from
requiring a complete in-memory copy. Small text artifacts can use byte helpers.
Format-specific metadata and validation belong to producers; accepting bytes
does not make the framework a validator for every possible file format.

The document model contains ordered blocks: headings, paragraphs, Markdown prose,
code, resources, and contributor notes, with room for lists and tables when the examples need them.
Paragraphs accept literal text and typed inline references to checked values,
production labels, or resources. The first presentation adapter renders these
blocks to ordinary Markdown and assets. Unknown formats can be linked as files;
inline display requires an explicitly supported presentation. Embedding text must
escape Markdown correctly, including content containing backticks. An invalid
reference or incompatible resource kind fails.
Resource links within raw Markdown still participate in final reference and
ownership validation; literal markup is not an escape from those checks.

Document composition observes completed execution. Appending a paragraph or
placing an already captured resource must not advance time, change input, or
reevaluate a mutable application value. Checks retain immutable observed values;
captures retain their original checkpoint. Rendering the finished document must
not rerun the application. If a later state matters, capture or check it again
explicitly. Source execution order and emitted block order are deterministic.

Only explicit reader-content calls emit reader content. Contributor notes are
authored explicitly too, but have a different audience. Actions and assertions
do not automatically become step lists, passing-test labels, or execution logs.
Authors choose how to explain the experience, can perform setup silently, and
can attach additional regression checks without changing the narrative. Failed
execution invalidates the whole candidate even if earlier paragraphs were built.

Long static passages can use a single multiline Markdown block, or an optional
included prose file. A compatibility template adapter can consume the same typed
records during N3 migration. Neither creates a second required authoring source
or an independently maintained list of resource references.

Keep the document blocks and verified record available independently of Markdown
rendering so an adopter can supply another formatter later. Do not build a plugin loader or
second site generator now. Custom formatters must declare their consumed
references so they cannot bypass ownership checks by silently dropping evidence.

N3 keeps its production input replay, explicit clock, control witnesses,
annotations, window composition, image encoding, and strict renderer policy as
its adapter. No graphics dependency belongs in the portable library's default
dependency graph. The application stays one application crate; a proven reusable
documentation dependency is a separate, justified boundary.

## What adoption could look like

A developer adds one documentation runner to an existing Cargo project, registers
a document function, then calls the runner locally and in CI. They do not
need to restructure their product or move their entire manual into the framework.

The following is an API sketch, not compiling code or an existing package:

```rust
fn configuration(doc: &mut Doc) -> Result<()> {
    doc.heading("Configure the service");
    doc.markdown("Set `port` to choose the listening port.");

    // Call the adopting project's real public API.
    let config = my_app::Config::parse(r#"{"port":8080}"#)?;
    let port = doc.expect_eq("explicit-port", config.port, 8080)?;
    doc.paragraph(("This example selects ", &port, "."));
    doc.code("configuration", Json(&config))?;
    Ok(())
}

fn main() -> ExitCode {
    doc_harness::run_cli([Document::new("configuration", configuration)])
}
```

The emitted page reads as ordinary documentation:

```markdown
# Configure the service

Set `port` to choose the listening port.

This example selects 8080.

    {"port":8080}
```

The paragraph consumes a checked-value handle directly. The framework records
that reference without requiring a second value name or a template placeholder.
The `code` convenience method registers and places one JSON resource in a single
operation. Only the port was asserted; showing the complete JSON does not label
all its fields as verified. A lower-level resource API returns reusable handles
when authors need a resource in several places or an explicit claim association.

For UI documentation, the same model can compose a sentence from a shortcut
handle, use that identity to replay input through the application adapter, and
place the resulting image directly. An illustrative fragment is:

```rust
let pan = app.shortcut_ref("navigation.pan")?;
doc.paragraph(("Hold ", &pan, " while dragging to pan the view."));
// The adapter replays the gesture; checks still assert the meaningful result.
pan_example(doc, app, &pan)?;
doc.image("after-pan", app.capture()?, "The view after panning.")?;
```

`pan_example` is a project helper, not a magical framework action. Its gesture,
intermediate assertions, and release behavior remain inspectable ordinary code.
The shortcut handle records its production identity and provenance; putting it
in a paragraph alone does not establish that it was exercised. The image helper
registers and places the captured resource, deriving media metadata from the
producer. The author supplies its identity and description once.

These signatures illustrate the authoring contract; the spike must establish
usable ownership, borrowing, error propagation, and diagnostics. Methods that
only append blocks may defer validation to finalization; checks and producers
that can fail immediately return errors. No error is silently discarded.
A second scenario should exercise malformed configuration and meaningful error
behavior. For a CLI guide, execute the actual CLI and check its exit status and
output; calling an internal parser alone would not demonstrate CLI behavior.

## Contributor notes and rendering modes

Internal notes are authored document content that can be retained and shown to
contributors. They are separate from Rust source comments, assertion diagnostics,
and application commands. Use them for rationale, limitations, review questions,
or an explanation of why a particular state is demonstrated. They do not count
as behavioral checks or convert proposed behavior into verified promises.
Notes reuse the same prose and typed-reference composition as paragraphs, so
they can refer to resources without a second rich-text authoring API.

Keep authoring small; illustrative additions to the existing model are:

```rust
doc.note("Keep the malformed-input case: recovery is part of this promise.");
let image = doc.image("after-pan", app.capture()?, "The view after panning.")?;
doc.note_on(&image, "Inspect label legibility when changing the capture scale.");
```

An ordinary note belongs to its authored location in the document. An anchored
note refers to an existing block, claim, or resource handle; it need not repeat a
path or resource name. Dangling and cross-run targets fail validation. Plain
paragraphs and notes do not need manually assigned IDs; an explicit stable anchor
is available when a consumer needs to refer to a particular passage across runs.
Do not promise persistent comment identity from line numbers or insertion order.

Execute once and derive audience-specific views from the same immutable document
and evidence. Rendering mode must not alter actions, checks, application state,
or the reader narrative. A note-only edit leaves reader output bytes unchanged.

| View                         | Content and publication boundary                                                                                                                                            |
| ---------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Reader, the default          | Reader prose and resources only. Omit note bodies, note metadata, and resources referenced exclusively by notes from the exported files and manifest.                       |
| Contributor, explicit opt-in | The same reader content with contributor annotations and their resources. A plain renderer can show callouts; a future viewer can toggle comment markers or a margin panel. |

Use separate output roots and inventories for the two views. Internal notes must
not be carried in the reader export as hidden HTML, comments, embedded JSON, or
unused files. A contributor viewer can include a toggle because its bundle was
explicitly generated with those records; a reader export has nothing to reveal.
The adopter decides where to publish each view and any access controls.
Each export identifies its audience in the manifest; comparing a contributor
result against a reader baseline fails as an incompatible view.

Validate all references before rendering, then compute the resource inventory
reachable from each view. A resource shared by reader prose and a note remains a
reader resource; one referenced only by a note is contributor-only. Neither is
an orphan. Linking contributor-only evidence to a reader-visible claim does not
promote it into reader output: presentation use determines visibility, and each
export's metadata includes only relationships within that audience. Complete
internal reference validation and behavioral checks still run.
If contributor output is retained as a baseline, compare it in its own
scope; its note changes do not create public artifact drift. Notes must not affect
reader numbering, anchors, asset identities, or ordering.

The first proof needs note records and reader/contributor export tests, with a
simple contributor rendering. An interactive comment UI, author accounts,
threads, notifications, and resolution workflows are later viewer features.
This is one document with two views, not a second maintained contributor guide.

## Future revision and diff views

Design for an optional viewer that compares two document/evidence bundles. A Git
integration could supply bundles from two revisions, while another integration
could use local baselines or build artifacts. Git selection and loading belong
outside scenario execution and outside the portable document model.

The existing boundaries should supply enough information: stable document and
resource identities, explicitly named anchors where needed, versioned manifests,
media types, content hashes, comparison profiles, and access to the retained
artifact bytes. Text can be compared as rendered text or structured blocks;
images can later use side-by-side, overlay, or difference views. Exact prose
block matching across revisions is not guaranteed for unanchored text. Changing
a resource's stable ID appears as removal/addition unless an explicit mapping is
supplied; changing its title or path while retaining the ID preserves identity.

A comparison viewer must work with the artifacts actually retained for each
revision. Missing historical captures are reported as unavailable; reconstructing
them requires a separate explicit run. A Git commit alone is not proof that its
documentation was executed or reviewed. Source revision remains optional run
metadata, preserving builds outside Git and avoiding commit-dependent output.

Preserve three small data and consumer boundaries:

- Composition produces an immutable document/evidence result.
- Audience rendering selects the applicable records and resources.
- A future revision source supplies two such results to a comparison viewer.

Do not add Git commands, repository lifecycle management, image-diff algorithms,
a revision-provider trait hierarchy, dynamic plugin loading, or a viewer SDK now.
Ordinary Rust modules and consumers of the versioned records suffice. Visual
diffs help human review; they do not alter exact check policy or approve a
baseline. Revision views follow the same audience rules as ordinary exports.

## Running the document

An initial project-owned `examples/docs.rs` runner could expose:

```sh
cargo run --example docs -- check
cargo run --example docs -- build --out target/docs-review
cargo run --example docs -- update
```

| Command  | Proposed behavior                                                                             |
| -------- | --------------------------------------------------------------------------------------------- |
| `check`  | Execute, validate, and compare against the retained baseline; never change it                 |
| `build`  | Execute and write a validated candidate to an explicit review directory; do not accept it     |
| `update` | Execute, validate, and deliberately replace owned baseline output; never imply human approval |

An example or dedicated runner package works with an existing public library
API; these are separate crates and cannot access private application types.
For private integrations such as N3's, keep an in-crate module, test, or gated
application runner. Use development dependencies or explicit feature gates where
appropriate to keep documentation-only dependencies out of shipped runtime code.
A normal Rust test can invoke the same check API. A future `cargo` subcommand may
discover and launch the configured runner; it cannot execute arbitrary project
Rust callbacks without compiling them. Distribution convenience follows a useful
SDK, rather than dictating its architecture.

## Authoring ergonomics is an acceptance criterion

Aim for a readable account of the experience with little framework bookkeeping.
Line count is a useful symptom, not the objective. Count the full authored cost:
document functions, prose, helper definitions, fixtures, registration, and
adapter configuration. Moving hundreds of lines into a one-use helper does not
establish a reduction in complexity.

The current [navigation scenario](../src/documentation/scenarios/navigation.rs)
and [editing scenario](../src/documentation/scenarios/editing.rs) are each over
900 lines; [gizmo](../src/documentation/scenarios/gizmo.rs) exceeds 800. These
files include multiple states and regression cases, not just one screenshot's
setup. Preserve their relevant coverage while addressing the actual repetition:

| Authoring cost observed in N3                                                        | Proposed response                                                                 | Behavior that must remain visible                                                      |
| ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Capture names repeated in scenarios and templates                                    | Register and embed in one call; reuse typed resource handles                      | Which state was captured and where the resource appears                                |
| Double-click event sequences repeated in editing, axis locks, and numeric transforms | One tested adapter gesture helper with explicit timing policy                     | Both press/release pairs and ordering through production input                         |
| Repeated cloning and comparison of document, selection, revision, and dirty state    | Named project-specific observation/check helpers with field-level failure details | Exactly which fields must stay unchanged; avoid accidental whole-application snapshots |
| Repeated callout, capture, and clear sequences                                       | Scoped presentation helpers where lifetimes match                                 | Presentation cannot alter focus, held input, or application state                      |
| Repeated input matrices and setup                                                    | Ordinary functions and data-driven cases with distinct case IDs                   | Every mode and recovery path still executes and fails independently                    |

Keep high-level intent and important intermediate checkpoints readable in the
document function. Factor mechanical event construction and established invariant
checks into the owning adapter. An abbreviated `drag` helper is useful for a
completed drag; a release-order example needs explicit held and released phases.
Do not hide those phases or replace production input with a final-state mutation.

Prose should remain comfortable to edit: whole paragraphs and multiline Markdown
are first-class. Authors should not construct an object for every word or sentence.
For dynamic inline content, prototype ordinary tuple/sequence composition first;
consider a small typed formatting macro only if the examples establish a clear
benefit. Ordinary `format!` or `Display` conversion must not silently discard a
checked value's provenance and then present it as a verified inline reference.
Keep Rust for control flow; do not introduce an assertion or scenario DSL just to
save punctuation.

Evaluate ergonomics on two tasks before settling the API: write the small
non-graphical guide from scratch, and port a representative section of a large
N3 scenario with its narrative. Review all author-written files together. Check
that each durable ID is declared once, adding a paragraph or resource needs no
parallel registry edit, and a behavioral failure identifies the document, claim,
expected/actual result, and relevant helper/case rather than an opaque boolean.
Compare the code and generated guide side by side; retain the same checks,
intermediate states, and resource content. Record duplication and maintenance
cost as well as total lines, with no arbitrary line-count target.

## Drift, determinism, and publication rules

The portable default is exact comparison of the complete declared output set.
Text and JSON producers serialize deterministically. N3 continues its exact
renderer-specific checks. There is no automatic fallback to skipping an artifact,
accepting a new baseline, or relaxing image comparison after a failure.

An adopter may eventually need a named, versioned normalization or semantic
comparison policy. Add that only for a demonstrated use case, record it per
artifact, and expose it during review. Do not ship image tolerance or a general
custom-comparator system in the first proof. Volatile output should first be
controlled through fixtures, clocks, seeds, and explicit adapter policy.

Separate the stable baseline manifest from diagnostic run metadata. The former
contains a schema version, contract/check/reference identities, artifact paths,
media types, sizes, hashes, and declared producer/comparison profiles. The latter
can record the source revision, actual host details, timings, and local paths
without making every unrelated commit or run timestamp create document drift.
Unknown schema versions or incompatible required profiles fail visibly. A profile
is a declared compatibility contract, not proof of every environmental input.

Minimum failure rules:

- A required scenario, assertion, producer, or document-composition failure fails the run.
  A panic also prevents baseline publication; publishing a passed subset is not
  a successful complete check.
- Duplicate IDs, unsafe relative paths, duplicate output ownership, unresolved
  references, wrong artifact kinds, missing files, and changed bytes fail.
- Every reader-facing evidence artifact must be referenced by a document.
  Supporting evidence retained outside the reader's page must be explicitly
  associated with a claim in the manifest. Generated pages, navigation, and the
  manifest have declared structural roles and ownership; they are not treated as
  evidence attachments. None of these roles permits arbitrary orphaned output.
- Contributor-note references participate in validation and ownership for the
  contributor export. Reader exports contain only their own reachable inventory;
  omitting contributor records must not skip behavioral checks or hide failures.
- Checks cover only the explicitly owned output root. The framework neither
  scans nor takes control of the adopter's whole documentation site. A generated
  subtree can coexist with handwritten pages. Local references outside that
  subtree require a declared site root or asset mapping; remote link reachability
  is a separate concern. N3 retains its stricter guide-only link policy.
- An obsolete owned output is reported, not silently deleted. Renames and
  retirements require explicit reconciliation. Unknown files and symlink escapes
  must never be overwritten or removed by baseline update.
- All execution and validation complete before baseline writes. Stage candidate
  output separately. Concurrent update and interrupted/I/O-failed publication
  must have a tested recovery policy before adoption; never describe sequential
  file writes as an atomic transaction.

The runner executes project code, so `check` can promise a read-only baseline,
not a universally side-effect-free application. Projects isolate their own
fixtures, settings, filesystem effects, and external services. The core does not
implicitly introduce network calls, capture user state, or publish a website.

Generation, human review, baseline update, and deployment are separate actions.
The first release candidate can use ordinary Git diffs and existing viewers for
review; a mandatory hosted review UI or stored approval system is unnecessary.

## Incubation and implementation sequence

All stages stay in this repository. Timings and public API stability remain open.
Do not change the existing N3 guide baseline merely to make extraction convenient.

| Stage                                                      | Deliverable                                                                                                                                        | Exit evidence                                                                                                                                                             |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1. Prove the contract and authoring model without graphics | An ignored standalone Cargo spike with a tiny core and independently compiled configuration library/CLI example                                    | One executable document composes readable prose, checked values, JSON, and a transcript without a separate template; authoring costs and deliberate failures are reviewed |
| 2. Establish a maintained boundary                         | Promote only the proven core into one unpublished package, provisionally `crates/doc-harness/`, with the small consumer as an example/test fixture | Dependency graph contains no N3, egui, wgpu, or window system; focused failure tests pass; maintained code does not depend on ignored files                               |
| 3. Bridge one N3 guide                                     | Adapt one existing scenario and its bindings/captures to the same core; compare through N3's existing publication format                           | Identical behavior, Markdown, media, and legacy manifest; existing renderer receipts remain valid; `just verify` passes                                                   |
| 4. Migrate and test real adoption                          | Move the remaining shared lifecycle behind the adapter, and trial one capability in an independently existing Rust project                         | Full N3 native guide and Linux receipt checks pass; adopter adds a scenario without changing core internals or creating a renderer                                        |
| 5. Prepare the future repository                           | Package the SDK, runner instructions, executable getting-started guide, extension example, and optional workflow skill                             | Relocation works without N3 paths, private assets, or its CLI; package API, license/provenance, and schema policy have been reviewed                                      |

Stage 1 is a portability experiment, not evidence of external demand or a stable
SDK. Stage 4's independent project trial is needed to validate adoption rather
than merely demonstrate a second fixture we designed ourselves. A release or
repository move is a later decision, not a side effect of these stages.

Use `spikes/executable-docs/` for Stage 1 after confirming it is ignored. Its own
Cargo project must not alter N3's manifest, lockfile, production imports, or
verification recipes. Keep its guide, review captures, and findings together.
The first implementation task is deliberately bounded:

1. Implement document-function registration, named equality/condition checks,
   checked values, and text/JSON resources.
2. Implement the minimal document composer used by the example: Markdown blocks,
   typed inline references, direct resource placement, and contributor notes.
   Render reader and contributor Markdown/assets to separate owned directories
   with audience-specific manifests; a plain note callout is enough initially.
3. Expose the same runner through a Rust test and `check`, `build`, and `update`.
4. Demonstrate failures for changed behavior, an invalid/cross-run reference, an altered
   or missing artifact, duplicate identity, orphan output, and failed generation
   leaving the retained baseline unchanged.
5. Read the generated guide and perform a deliberate feature change: update the
   prose and assertions, inspect the candidate diff, then update the baseline.
   Confirm that checks alone do not emit reader prose, presentation does not
   change application state, and resources/values retain their observed state
   after later application changes. Record authoring friction and revise the
   proposed API before promoting it.

The example should include a multi-paragraph explanation, a failure section, a
checked inline value, a resource reused through its handle, and a shared helper.
Those cases make the syntax and ownership costs reviewable before extraction.
Add a note attached to a resource and one contributor-only resource. Verify note
inclusion in contributor output, complete exclusion from the reader bundle,
reader byte stability after a note-only edit, and rejection of invalid note
targets or mismatched export audiences. No interactive viewer or Git integration
is required for this stage.

For Stage 3, use a bridge formatter to preserve N3's current manifest and paths
while checking the new structured records separately. A shared-core extraction
must not force a new baseline format merely because the portable SDK has one.
Any intentional N3 output change gets its own explained migration and follows
the normal regeneration and visual review workflow.

The bridge started with [the hand-tool scenario](../src/documentation/scenarios/hand_tool.rs)
and its former template, now removed. Together they exercised
production input, held/released state, control and shortcut bindings, stills, and
animation. This checks substantially more than copying one screenshot through
the new artifact API.

Then author that same guide directly in the document function, preserving the
reader output and assertions. Before broader migration, also port one substantial
section of the larger navigation or editing guide to assess the ergonomics under
real complexity. A compatibility adapter is a transition aid; it must not leave
every author maintaining both a text template and document-building code.

The migration must not create two permanent artifact ownership or verification
paths. Initially compare old and candidate behavior; once parity is established,
make the existing `just docs` and `just verify` commands use the shared lifecycle.
N3's capture and semantic input assertions remain required throughout.

Measure cold/warm documentation build time, run time, and dependency impact on
both consumers before making performance or convenience claims. Serial execution
is enough initially; concurrency and caching wait for measured need.

## Stage 1 findings (2026-10-04)

The ignored `spikes/executable-docs/` workspace now implements and exercises the
first stage. Its `README.md` gives reproducible commands; `VALIDATION.md` records
the checks, rendered review, and authoring assessment; `PORTABILITY.md` records
the independent rebuild. These local files are deliberately not linked as
repository-hosted documentation because ignored experiments are not published.
N3's application, Cargo manifests, lockfile, guide, and recipes are unchanged.

The prototype has two local packages: a small `executable-docs` SDK and an
independently compiled configuration library/CLI. A normal Rust function writes
multiline Markdown, paragraphs with typed checked values, JSON and command
transcripts, and contributor notes. It exercises the actual CLI through valid
save, invalid replacement preserving every output byte, and successful retry.
The same function is called from the runner and a regression test. No template,
renderer, agent skill, or graphics dependency is needed.

The working syntax uses `Doc::new`, `expect_eq`/`require`, `paragraph`/`markdown`,
`resource`/`code`, `note`/`note_on`, and `finish`. Tuples compose literal text and
typed handles; `text`, `json`, `text_code`, and `json_code` cover common resource
cases. Exact captured bytes can retain application-defined producer/profile
metadata through `Artifact`. These names are experimental. The examples earlier
in this RFC remain design sketches, not an alternative implemented API.

Both audiences render from one frozen execution. Reader output excludes notes,
their relationships, and exclusive resources throughout pages, assets, and
manifests. Independent tests compare the complete reader bundle before and after
note insertion, including annotations attached to existing checks. A successful
check retains an ID/kind/pass record; actual/expected payload is exposed to a
reader only when public prose references the checked value. Check IDs are public
contract metadata. A private diagnostic resource referenced only by a note is
entirely absent from the reader bundle.

The runner exposes read-only `check`, new-directory `build --out`, and explicit
`update`. It renders all selected audiences before writing. Each audience owns a
separate inventory and filesystem transaction. It rejects unknown files,
orphaned resources, invalid/cross-run handles, unsafe paths, wrong resource kinds,
duplicate IDs, audience/schema mismatch, and incompatible producer/profile
changes for retained resource IDs. New resources may introduce new producers.
Retired files require explicit reconciliation. Staging, locking, adjacent rename,
rollback, and explicit backup recovery are tested. Publication is not atomic
across audiences or guaranteed crash-atomic; a postpublication cleanup error
reports that the valid candidate was already published.

Validation includes 46 prototype tests, a standalone rebuild outside N3, exact
comparison of both relocated exports, and browser inspection of the rendered
reader and contributor views. N3's full native `just verify` gate also passed,
including its required exact documentation replay. The initial contributor rendering exposed an
ambiguous “note on 2” label; the implementation now names the check instead.
A copied consumer then changed its default port from 8080 to 8181. The old
assertion failed with document/check IDs and actual/expected values, preserving
baseline bytes and modification times. Updating the prose and assertion produced
a candidate diff; review, explicit update, and a subsequent exact check completed
the change cycle. The main demonstration retains its original default.

The authoring cost is still material:

| Measured scope, including comments and blank lines | Lines |
| -------------------------------------------------- | ----: |
| Narrative/scenario function                        |   140 |
| Supporting consumer helpers                        |    74 |
| Documentation runner                               |    22 |
| Documentation regression tests                     |    48 |
| Consumer Cargo configuration                       |    13 |
| Complete documentation integration                 |   297 |
| Example application and its own tests              |   300 |
| Complete consumer                                  |   597 |

This is evidence that one authoring source can cover prose, execution, evidence,
and notes. It is not evidence of a percentage reduction against N3's larger
scenarios. No complexity was excluded merely by moving it into helpers. Repeated
transcript registration/placement and sparse boolean-failure context are concrete
remaining friction. Preserve explicit behavioral assertions while refining those
operations with the next real consumer; do not solve them with a large DSL yet.

Fresh-target compilation in the relocated workspace took about 8.0 seconds; a
warm build of both documentation views took about 0.58 seconds. These are single
local observations with a warm registry cache, not performance claims. The copy
used the host's Rust 1.92.0 rather than N3's inherited 1.98.1 and produced the same
19 files / 25,335 bytes, including ownership markers. This proves relocation on
this Mac, not Windows/Linux portability or a supported minimum Rust version.

Keep these limitations visible before promoting an API:

- Public prose block IDs are positional; note insertion cannot renumber them,
  but public paragraph insertion can. Document/check/resource IDs are authored
  and stable. Durable block identity/source spans remain a future diff concern.
- Raw authored HTML is rejected; reference definitions resolve within one
  Markdown block; local fragments target generated block IDs. External links
  receive basic syntax checks, not network availability checks.
- The filesystem lifecycle assumes cooperative local writers; it is not a
  security boundary against a process racing filesystem operations.
- The non-GUI example was created alongside the SDK. It proves mechanics, not
  independent demand, adoption success, or ergonomics for a thousand-line guide.

At the end of Stage 1, the next proposed work was the maintained lifecycle
boundary and two N3 authoring trials. The subsequent user authorization and
results are recorded below. Git adapters, interactive comments/diffs, plugin
infrastructure, publication, and the repository move remain deferred.

## Maintained implementation findings (2026-10-04)

The local implementation now includes:

- [An unpublished SDK](../crates/doc-harness/README.md) with Rust-authored prose,
  checked values, observed bindings, frozen resources, contributor notes, strict
  audience filtering, a structured manifest, and a project-owned runner.
- One filesystem lifecycle used by the SDK and N3. Dedicated trees preserve N3's
  existing file format; recorded trees retain explicit ownership metadata. Exact
  comparisons, safe output paths, deliberate updates, failure recovery, and
  incompatible producer profiles have regression coverage.
- [The hand-tool](../src/documentation/scenarios/hand_tool.rs) and the entire
  [navigation scenario](../src/documentation/scenarios/navigation.rs) author
  their narratives beside real replay. Their old templates are removed. All
  production input and behavioral assertions remain in place. The other 26
  features keep one legacy narrative source each.
- Reader/contributor candidate builds from one frozen execution. N3's legacy
  format supports private prose notes and public captured media; private captured
  resources fail before publication. Full SDK exports support contributor-only
  resources and exclude their files and metadata from the reader bundle.
- [Two non-GUI integrations](../crates/doc-example-config/README.md): a real
  configuration CLI and the existing serde_json library's public parsing/error/
  serialization APIs. The latter is our integration, not independent maintainer
  adoption or upstream documentation.

Small helpers address demonstrated repetition: `resource_code` registers and
places a captured resource, typed handles retain reuse, and the expression-list
form of `prose!` removes nested tuples when a paragraph has many text/binding
parts. The review follow-up added a named inline form described below, without
introducing template files or a procedural macro. N3's `Guide` wraps the existing capture calls so
registration cannot require a second capture or clock advance.

Authoring is more unified but is not shorter in the two N3 migrations. Counts
include comments, blank lines, and all helpers in each scenario file:

| Scope                                                 |           Before | After |
| ----------------------------------------------------- | ---------------: | ----: |
| Hand tool: scenario plus separate narrative           |   279 + 36 = 315 |   350 |
| Navigation: scenario plus separate narrative          | 954 + 88 = 1,042 | 1,158 |
| Shared N3 Guide adapter, including focused regression |                0 |   242 |

The adapter is additional shared code. Before the topology follow-up, the SDK had 2,706 lines
under `src/` and 2,304 under `tests/`, including comments, blank lines, and inline tests. Its README,
preview example, and package configuration are additional. These are not hidden
from the cost assessment. The example package
also reports complete narrative, helper, runner, and test counts. Preserving
exact legacy Markdown whitespace adds composition overhead; collecting observed
labels and returning typed captures also costs syntax. The benefit demonstrated
here is local narrative/evidence ownership, checked references, and one drift
workflow. Do not advertise a line-count reduction. Continue evaluating author
comprehension and repeated setup before adding more abstractions or migrating
all remaining guides.

The minimum package toolchain is Rust 1.95. At the initial extraction, the maintained SDK and consumers
were copied outside N3 and passed 70 tests plus two README doctests on Rust 1.95.0
and 1.98.1. Both real guide commands checked reader/contributor baselines, and
the preview example ran in each copy. A locally built `.crate` also passed Cargo
package verification and its unpacked tests. The copy's lockfile contains 39
packages, with retained versions unchanged; no N3, egui, wgpu, or winit package is
in that graph. The SDK includes its MIT license; the README documents schema
version 1, compatibility limits, provenance, artifact producers, and recovery.
No package was published and no repository was created.

The architecture, contributor instructions, and existing docs-driven development
skill now describe the actual boundary. The skill remains optional for SDK users.
`just doc-framework-test` is part of native `just verify` and the existing CI
checks. Native/web product routing remains unchanged; N3 depends on the framework
only on native targets.

The initial native `just verify` passed with 954 N3 tests, 70 SDK/consumer tests, two README
doctests, tooling checks, and measurement checks. The existing manual CPU timing
probe remains ignored; no guide capture was skipped. The final framework gate
also builds API documentation with warnings denied. `just web-check` passed,
and the WebAssembly dependency graph excludes the SDK.

The initial native guide and pinned Linux lavapipe receipt each passed all 28 features
and 136 generated artifacts. All 138 retained guide/receipt files remain
byte-identical to the pre-extraction inventory. Linux used the already-built
pinned image through the existing runner's read-only mounts and renderer policy
after Docker's base-image metadata lookup stalled. The complete Linux replay
preceded the isolated filesystem case-alias guard; the final source then compiled
and passed all 70 SDK/consumer tests and two doctests in that same image. That
guard changes candidate destinations, not replay or rendering. A missing-CLI
diagnostic differed under emulation; resolving the executable before launching
it restored the explicit failure contract without weakening checks or changing
guide output. The corrected consumer also passed its tests and both guide checks
on native Rust 1.95.0 and 1.98.1 outside N3. Reader/contributor HTML, captured
stills, and the existing hand-tool animation were reviewed locally. This does not
establish physical-device input recognition or independent developer acceptance.

The remaining adoption gate is work with an independent project owner using
that project's own behavior and authoring needs. Longer-term schema/API
stability, Windows filesystem behavior, a public name and release, Git/diff
viewers, interactive comments, plugin infrastructure, and the eventual repository
move have not been established by this local implementation. The finished local
code is a reviewable starting point for those decisions.

### Review follow-up: correctness and authoring

Independent review reproduced five missing failure cases: typed Markdown could
expose authored HTML by changing inline-code context, JSON ordering could depend
on an adopter's `serde_json/preserve_order` feature, artifact filenames containing
entities could render broken links, unclosed Markdown containers could swallow
later blocks, and an ownership marker could declare nonexistent files without
failing checks. The fixes track generated HTML spans and validate the completed
composition, encode typed destinations, recursively canonicalize JSON objects,
and validate complete ownership with a narrowly scoped manifest-backed retirement
exception. Each case has behavioral regression coverage.

Named inline prose keeps a sentence intact while retaining typed references:
`prose!("Hold {pan} over {viewport}.", pan = &pan, viewport = &viewport)?`.
It accepts named fields and escaped braces, without formatting modifiers.
Missing, duplicate, unused, or malformed bindings fail. Expressions run once;
substitution retains the original typed parts and their ownership/visibility
checks. This is an inline authoring convenience, not a second narrative source.
Ordinary paragraph, heading, and embed blocks remain the default for new pages;
exact `markdown_parts` composition supports the established N3 output contract.

The hand-tool guide now composes its explanation outside its exhaustive replay
matrix. Its inputs, captures, and 17 assertion sites remain in their original
order. The complete file decreased from 356 to 350 lines, still above its original
315-line scenario-plus-template total. The improvement is continuous readable
prose and clearer separation of explanation from coverage, not a claim that the
framework solves thousand-line replay setup.

`heading_with_id` and labeled `Block::link` references give important sections
deliberate identities while keeping positional IDs as the default. They do not
add a Git implementation or change N3's legacy fragment anchors. N3's imported
legacy claims remain numbered, and paragraph-attached contributor comments need
an explicit output-format decision before that adapter can support them.

Artifact production is independent of the application, but the current exporter
targets Markdown. A public structured document view is deferred until a concrete
alternate publisher exercises audience filtering, inline references, frozen
observations, and resources together. The existing private document model is the
extension boundary; a plugin registry or a handful of unfiltered getters would
not establish that contract.

Follow-up validation passed native `just verify`, including 954 N3 tests and
exact guide replay, 90 SDK/consumer tests, three README doctests, strict API docs,
tooling, and measurement checks. All 138 retained guide/receipt files remain
byte-identical. Fresh portable copies passed the 90 tests, three doctests, guide
checks and API docs on Rust 1.95.0 and 1.98.1; the rebuilt local archive also
passed independent unpacked tests. Feature-unified consumers produced identical
JSON, page, and manifest bytes with `preserve_order` enabled or disabled. The
cached pinned Linux image passed final-source N3 compilation, all 90 portable
tests, and three doctests without changing source or baseline bytes. That focused
Linux run did not repeat the earlier full GPU replay.

### Project topology and configuration follow-up

The maintained SDK now exposes `ExportLayout` and `RunnerConfig`. Page/resource
prefixes, exact page paths, audience baseline destinations, candidate audience
directories, and the base for relative filesystem paths belong to the adopter.
Direct `runner::run` calls support existing tests and build tools; the CLI parser
is optional. Existing defaults remain compatible. No repository discovery or
required source/fixture/site directory was added.

Document finishing is independent of eventual export placement. Full export
validates the chosen topology, page-relative links, audience visibility, resource
use, and collisions before publication. Typed resource links rebase automatically;
explicit resource paths remain bundle-relative. The manifest and ownership
filenames, privacy rules, and strict comparison remain protocol contracts.

The maintained `configured` example uses nested pages, moved resources, external
retained output, and custom audience directory names. A disposable adoption proof
also copied the SDK to `vendor/document-engine` with a separate consumer under
`tools/narrative-author`, no root Cargo workspace or Git repository, and an
external Cargo target. Rust 1.95 built that arrangement. Invocation from an
unrelated working directory and relocation of the entire source project produced
identical Reader and Contributor exports while preserving unrelated files. This
proves those integration shapes; it remains our test, not adoption by independent
maintainers. The current API and path semantics are documented in the SDK README.

## Positioning and alternatives

Executable documentation is an established family of ideas. The following
primary sources were checked on 2026-10-04:

| Precedent                                                  | Existing overlap                                                                           | Consequence for this proposal                                                   |
| ---------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| [Cucumber](https://cucumber.io/docs/)                      | Executable plain-text scenarios, behavior verification, and documentation                  | Do not claim to invent intent expressed through executable examples             |
| [Concordion](https://concordion.org/)                      | Natural-language living documentation, inline checks, screenshot and storyboard extensions | Documentation plus assertions and pictures is not itself a new category         |
| [Insta](https://insta.rs/docs/quickstart/)                 | Rust snapshots, candidate output, and explicit review workflows                            | Reuse established review ideas; a snapshot does not establish narrative meaning |
| [mdBook](https://rust-lang.github.io/mdBook/cli/test.html) | Testing Rust examples within a book                                                        | Existing documentation publishers are potential consumers of generated Markdown |

Our proposed contribution is a Rust-first packaging of the workflow N3 uses:
the reader's guide, selected behavioral promises, observed bindings, and retained
artifacts share one checked contract. Whether this combination is distinctive
and easier to adopt is a product hypothesis to test, not an established novelty
claim. Public material should spell out "docs-driven development" to avoid
confusion with domain-driven design.

An extracted screenshot crate would miss most of that value. A skill-only
distribution would explain the method without enforcing drift checks. A new site
generator would add publishing constraints the adopter did not request. Extending
an existing snapshot library may reduce comparison/review work, but would still
need the narrative, identity, binding, and output-ownership layer; evaluate that
choice during the spike rather than committing to reimplement every utility.

## Decisions to revisit after the first proof

- Which inline syntax best preserves readable paragraphs, typed provenance, and
  useful Rust diagnostics with the least bookkeeping.
- How much page composition the two consumers actually need before introducing
  shared fragments, multiple scenarios per page, or custom formatters.
- Which publication/recovery strategy works for a dedicated output subtree on
  supported hosts, and how an adopter explicitly reconciles retired files.
- Whether a separate Cargo subcommand saves enough setup to warrant packaging.
- Which real project will provide the independent adoption trial, and which
  environment-dependent evidence it requires.
- Public name, minimum Rust version, distribution/license details, and schema
  compatibility promises after the package boundary has been exercised.

The maintained implementation now supports N3 and two non-GUI integration
shapes. Use the measured authoring costs and remaining adoption gate to guide
the next trial; keep speculative viewer and plugin features outside the core.
