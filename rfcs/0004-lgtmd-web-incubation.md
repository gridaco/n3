# RFC 0004: Incubate lgtmd's Rust and web contracts together

Status: **Implemented and extracted; release deferred**

Date: 2026-10-13

The portable Rust SDK and TypeScript integration were incubated directly in N3,
without a submodule, while N3 remained a real consumer. The independently verified
SDK, Node bridge, Rust/React examples, and development harness now live in the
[lgtmd repository](https://github.com/lgtmd/lgtmd). N3 consumes an exact
Git revision and retains its own adapter, replay, renderer, and guide artifacts.

The preferred eventual N3 integration remains a crates.io dependency once the
package passes its release gates. Extraction and dependency pinning do not
publish a package or authorize a release. The incubation findings and original
sequence below preserve the reasoning and historical verification evidence;
SDK-owned development commands now run in the lgtmd repository.

## Implementation record

- `crates/lgtmd` now permits explanation-only documents, separate frozen
  observations, and typed code resource views. N3 retains its own nonempty replay
  fact requirement and unchanged guide artifacts.
- Schema-v1 frozen input and coarse compilation live in `lgtmd::transport`.
  Native round trips preserve exports. JSON resources use a shared Rust encoder,
  including integer-like keys and fractional numbers registered from Node.
- `packages/lgtmd` supplies Node 24 ESM authoring, native assertion wrappers,
  project-owned async preparation/cleanup, supplied context, and optional
  `lgtmd/playwright`. Ordinary callback return values are ignored; assertions
  remain explicit. Tagged Markdown automatically separates complete blocks.
- The selected bridge is `crates/lgtmd-node`. A WASM probe compiled the same
  prose/notes/JSON/image input and produced equal exports, but its filesystem
  publication failed as unsupported. The native route reuses the existing strict
  Rust store rather than implementing another filesystem contract in Node.
- The Playwright adapter requires a sole guarded `docsReporter`, a completed
  receipt, and successful runner-process exit before publishing. Imported custom
  reporter instances are supported; built-in reporter-name delegation, retries,
  repeatEach greater than one, shards, and multiple/dependency projects are not.
- [Three complete React/Vite examples](https://github.com/lgtmd/lgtmd/blob/main/examples/lgtmd-web/README.md) include all project
  setup, cleanup, application code, prose, observations, checks, downloads,
  screenshots and contributor notes. React components and ordinary Vite scripts
  model a real developer application; lgtmd is a development dependency. Browser
  captures use the production bundle on isolated ports. No N3 runtime is required.
- The independent web gate passed on Rust 1.95, Node 24.14.0 and macOS arm64,
  including real Chromium replay and exact repeated exports, Cargo packaging and
  an extracted consumer, and npm installation/core/Playwright execution with
  Rust absent from PATH. The final gate passed all 51 Node tests. Linux x64 CI
  is configured but has not run here.
- Full native N3 verification passed against both the working SDK and its actual
  packaged source in a disposable N3 copy, without authoring changes: 954 tests
  passed, one pre-existing test remained ignored, and the required measurement
  and real-renderer parity checks passed. Exact guide and consumer baselines are
  unchanged. This rehearses package-content compatibility; registry installation
  remains unverified until an explicitly authorized release.

The web example gate builds a fresh temporary candidate and checks a second
execution against it. This establishes repeated-run determinism, not a retained
visual regression baseline or cross-platform pixel identity. The separately
reviewed local Reader/Contributor trees and screenshots are disposable evidence;
projects choose and explicitly accept their own baselines using `update`.

See the [implemented architecture](../docs/architecture/executable-documents.md#node-and-web-integration)
and [Node SDK contract](https://github.com/lgtmd/lgtmd/blob/main/packages/lgtmd/README.md). The original sequence below
records the reasoning and later extraction/release gates.

## Outcome and rationale

Incubation exercised major authoring, compiler, and host integration changes
through both Rust and JavaScript consumers. Extraction changes manifests,
dependency location, and development wiring while retaining N3's authoring API
and guide output. Future SDK development proceeds in its own repository, with
N3 compatibility checked explicitly when updating the pinned dependency.

Readiness applies to a declared initial support scope. Incubation cannot promise
that no future breaking change will ever be needed. Package contents, clean
installation, supported runtimes, diagnostics, and compatibility are explicit
exit gates; moving files alone does not establish production readiness.

[RFC 0003](0003-executable-documentation-framework.md) records the existing Rust
framework. The [implemented architecture](../docs/architecture/executable-documents.md)
and [SDK README](https://github.com/lgtmd/lgtmd/blob/main/crates/lgtmd/README.md) remain authoritative for current behavior.

## Initial support scope

- Rust authoring through `lgtmd`, with N3 and the existing CLI/library consumers.
- TypeScript and JavaScript authoring from Node.js, initially on the repository's
  Node 24 development baseline. Pin actual package/tool versions when implemented.
- One optional Playwright Test integration, composed with a project's own test
  fixtures, assertions, configuration, and browser lifecycle.
- Reader and Contributor Markdown/resource exports through the same Rust
  compiler, with explicit build, check, and baseline acceptance behavior.

The first web integration documents a web application from Node. Running the SDK
inside the application browser is a separate support target. The framework must
also work with supplied context and ordinary async code without Playwright.

The project owns preparation, mock state, seeds, services, credentials, APIs, and
cleanup. An optional preparation callback returns context and cleanup; it carries
no assumptions about a database, frontend framework, server count, or deployment.
Application-owned fixtures may provide that context instead. DOM queries remain
subject to application changes, and the author must assert the behavior described.

## Proposed source layout

Create directories only when their implementation lands:

```text
crates/lgtmd/                 Rust SDK and shared document processing
crates/doc-example-config/    Existing portable Rust consumers and baselines
crates/lgtmd-node/            Selected coarse compiler/store host bridge
packages/lgtmd/               TypeScript SDK, Node host, optional Playwright adapter
examples/lgtmd-web/           Small independent web fixture and authored documents
fixtures/lgtmd/               Shared semantic inputs, exports, and failure cases
```

Keep N3's existing `web/` devoted to the N3 application. Use a private npm workspace
for the SDK and example when both exist, with explicit package scripts and a
lockfile. Its dependencies serve this incubation; the application remains Rust.
The Playwright adapter can be an optional `lgtmd/playwright` entry point with a
peer dependency rather than a requirement for every SDK consumer.

If a native bridge proves preferable, create only the host package it actually
requires instead of maintaining an unused WASM package. Keep Rust processing in
the existing crate unless a demonstrated dependency boundary requires a split.

Public APIs receive content, context, paths, and options explicitly. They must
not discover an N3 checkout or assume these source directories. Repository-only
verification may know the payload layout and must prove it in a disposable
independent workspace, following the existing Rust standalone gate.

## Three small documents that exercise the contract

Use one neutral example project. Existing interface sketches are design input;
their declarations and hand-authored previews are not runtime evidence.

| Document                      | What the author writes                                                                                                                                | What it must establish                                                                                                                 |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Explain record names          | Ordinary paragraphs, headings, a reused prose helper, and a targeted contributor note.                                                                | Useful documentation can exist without an executed check; it makes no verification claim.                                              |
| Describe values and resources | Application-owned async preparation, an observation, an explicit assertion, JSON shown as code and linked as a download, and a private text resource. | Values and buffers freeze at registration; preparation and cleanup have honest failure semantics; non-image resources are first class. |
| Filter records in a browser   | Existing Playwright fixtures and assertions, filter/clear interactions, a screenshot, prose, and a note on the illustrated passage.                   | Documentation adds to a normal browser test without owning the application or requiring action wrappers.                               |

Start with a tiny deterministic records fixture whose data is supplied by the
example. A production backend is unnecessary to demonstrate the contract.
Include an already-prepared context path and ordinary helper reuse in these
documents, rather than adding more demo applications.

Implemented authoring shape; the complete runnable example includes setup and finalization:

```ts
test("Filter records", async ({ page, doc }) => {
  doc.page({ id: "filter-records", title: "Find a record" });
  const search = page.getByRole("searchbox", { name: "Search records" });
  const results = page.getByRole("list", { name: "Records" });

  await search.fill("Alpha");
  await expect(results.getByRole("listitem"))
    .toHaveText(["Alpha contract", "Alpha renewal"]);

  const image = doc.assets.image("matches", await results.screenshot());
  const explanation = doc.md`
    Search by name to narrow the list.

    ${image.embed("The two matching records")}
  `;
  doc.noteOn(explanation)`The project fixture owns the supplied records.`;

  await search.fill("");
  await expect(results.getByRole("listitem"))
    .toHaveText(["Alpha contract", "Alpha renewal", "Beta proposal"]);
});
```

The runnable example must include the test extension, navigation/setup fixture,
reporter or finalizer configuration, and application implementation omitted from
this sketch. Review the complete authoring cost, including helpers and fixture
configuration. Avoid IDs on every paragraph, duplicate resource declarations,
mandatory wrapping of existing assertions, or parallel prose templates.

## Contracts to settle during incubation

### 1. Native authoring and execution

Rust and TypeScript keep idiomatic APIs over the same document semantics. Prose
segments, typed interpolations, resource presentations, and targeted notes retain
their meaning across the language boundary. Ordinary functions provide reuse.
Specify indentation handling, escaping, block placement, and reference failures
through examples before freezing the TypeScript surface.

Separate authored explanation, frozen observation, explicitly executed check,
and host run outcome. A passed browser test must not manufacture an SDK check.
Named check wrappers are optional and must preserve native failures, including a
failure caught later by author code. Unwrapped assertions remain host outcomes.

Before incubation, `Doc::finish` required successful checks and observations
were coupled to equality checks. The implementation deliberately separates those
semantics, with matching tests in both languages. N3 retains its stricter
behavioral requirements in its adapter.
Do not add fake `require(true)` calls to make generic documents pass.

### 2. Frozen input and one compiler operation

Use one coarse operation, conceptually
`compile(frozenDocuments, exportOptions) -> exports and diagnostics`.
Native SDKs execute application code and freeze its results before calling it.
The compiler receives no browser object, callback, promise, or application seed.

Define a versioned input containing complete ordered content, typed references,
observations/check outcomes, notes, document/variant identities, immutable
resources, and useful source context. Current Rust handle ownership is
process-local; transport identity must be explicit and validated. Keep literal
Markdown and interpolated values distinguishable. Specify numeric and JSON
limits, null versus absence, invalid values, byte ownership, and unknown-version
errors rather than relying on lossy serialization defaults.

Both language paths use the same Rust validation and export logic. A private
Rust model can evolve behind the versioned boundary. Do not expose Rust struct
layout or reconstruct authored content from the current inventory manifest.
Compare identical canonical fixtures across both frontends.

Complete Markdown pages, resources, and their receipts are sufficient for this
milestone. The canonical interactive viewer remains future work. Its eventual
structured output contract is not claimed stable by this milestone; preserve
document structure internally so another exporter can be added without changing
how authors write their documents.

### 3. Host lifecycle and acceptance

Authoring may compile a preview before cleanup, but accepting a successful output
requires the relevant host run to finish successfully. Standalone preparation
owns partial failure before returning context; after successful acquisition,
framework-managed cleanup runs once. Preserve primary and cleanup errors together.
An adapter using project fixtures leaves their cleanup to the existing runner.

Playwright has dependency-ordered fixture teardown and distinct test/worker
lifetimes. Use its fixture and reporter extension points, with regressions proving
when output becomes eligible. A doc fixture's own teardown alone is insufficient
proof that the entire run succeeded. Relevant worker/global teardown errors,
timeouts, interruption, and reporter failures must also block acceptance.
Prove the final success signal against the completed runner process. Candidate
staging plus a post-process finalizer may be necessary; one reporter callback
cannot establish that every other reporter completed successfully. Select and
test that mechanism during the binding and real-execution stages below.
See [fixture execution order](https://playwright.dev/docs/test-fixtures#execution-order)
and the [reporter lifecycle](https://playwright.dev/docs/api/class-reporter).

Identify document, project variant, and execution attempt separately. Worker or
retry arrival order must not determine which evidence wins. Start the example
with one project and retries disabled; require an explicit rejection of unsupported
modes. Before broader support, test collision handling and final-attempt policy.
A retry success must retain its flaky status rather than becoming a clean first
pass; [Playwright exposes that distinction](https://playwright.dev/docs/test-retries#retries).
Transient run IDs and host diagnostics must not introduce nondeterminism into
canonical document bytes.

### 4. Resources, privacy, and publication

Freeze mutable values and resource bytes when registered. Reader and Contributor
exports reuse one completed run. Private notes and exclusively private resources
must stay out of Reader text, resource files, paths, and metadata; adding notes
must preserve Reader anchors and bytes. Existing check identifiers are public
metadata even when their values are hidden. Keep that policy explicit rather
than silently claiming every evidence identifier is private.

Configuration selects output roots and layouts. Build a fresh review candidate;
check compares without accepting; update is explicit. Preserve strict ownership,
path separation, failure recovery, and producer/profile compatibility. Reuse the
Rust storage implementation where the selected host permits it; otherwise prove
the Node host against the same filesystem contract before claiming parity.
Publishing multiple audiences is currently not one atomic transaction.

Use fixed resource bytes for cross-language conformance. Browser screenshots have
their own pinned capture environment and explicit reviewed baseline. Do not claim
cross-platform pixel identity, add hidden tolerances, or weaken N3's exact checks.

## Binding and tooling decision

Evaluate WASM first for pure processing, using a real frozen document with prose,
notes, JSON, and image bytes. Compare a coarse native bridge if startup, resource
transfer, diagnostics, packaging, or filesystem lifecycle makes it preferable.
Choose one initial shipping route behind the TypeScript facade. Neither route
should require crossing a process/binding boundary for every paragraph.

WASM has target-specific deployment and loading behavior; a Node integration does
not establish browser compatibility. Use the
[wasm-bindgen deployment guide](https://wasm-bindgen.github.io/wasm-bindgen/reference/deployment.html)
when implementing the probe. Include actual host publication in the assessment;
a pure compilation demo alone does not settle the installation or storage design.

Local development may build Rust artifacts. Installing the eventual npm package
must not require a Rust compiler. Pin the supported Node/module format and browser
test dependencies. Make browser/dependency installation an explicit setup step;
checks should not silently fetch a browser or depend on Docker.

## Implementation sequence and acceptance gates

1. **Authoring examples and semantics.** Make the three complete source examples
   reviewable, including setup and cleanup. Agree on explanation-only pages,
   observations, named checks, interpolation, and errors. Measure full authoring
   overhead. No production packaging or broad framework support is needed here.
2. **Shared model and binding.** Implement the minimal versioned frozen input,
   coarse compilation boundary, and selected bridge. Exercise both Rust and
   TypeScript through common conformance cases. Preserve N3 native guide output.
3. **Real web execution.** Run the three examples, compose with real Playwright
   fixtures, and prove post-run acceptance. Failure cases include mutable inputs,
   foreign/missing references, bad schema, swallowed check failure, teardown
   failure, timeout, and unsupported retry/variant configuration. Inspect the
   generated prose, stills, and contributor notes in a browser preview.
4. **Independent and packaged verification.** Copy only the portable Rust/JS
   sources, examples, conformance fixtures, and tooling to a temporary workspace.
   Resolve its own dependencies and run it without N3 source or its renderer.
   Run clean consumers against a Cargo package and npm tarball, exercising both
   supported entry points and shipped binding assets. The npm consumer check
   runs with Rust unavailable. Document the actual supported platform matrix.
5. **Extraction rehearsal and release readiness.** Use the packaged Rust source
   in a disposable N3 checkout without changing its authoring code; run native
   verification and exact guide comparison. Review licenses, package metadata,
   installation, and version/compatibility policy. Then move the payload to
   lgtmd, verify it there, and request authorization for an exact release version.
   After publication, N3 can switch to crates.io and repeat its locked checks.

Retain the existing `just doc-framework-test`, standalone Rust smoke, and N3
`just verify` gates. Add an explicit portable web gate and setup documentation
when implemented, with a CI lane supplying the pinned browser environment. Run
the same Node/browser checks locally; the independent gate must not launch N3.
Optional modes cannot silently reduce the required checks for their declared scope.

Cargo's packaging workflow checks the archive and builds from its packaged
contents; use that alongside installed-consumer tests, as described in the
[Cargo package reference](https://doc.rust-lang.org/cargo/commands/cargo-package.html).
Before publication, the N3 rehearsal establishes package-content compatibility;
only the later registry installation verifies actual crates.io consumption.

## Deferred work and remaining decisions

Defer a full canonical viewer, browser-hosted SDK execution, broad runner adapters,
backend orchestration, automatic seeding conventions, cursor/key compositing,
AI-assisted scenarios, Git/image diffs, and a public plugin protocol. Keep these
possible through clear host/content boundaries, without reserving speculative
APIs or implementing a framework around hypothetical extensions.

The examples and probe have settled the initial TypeScript signatures, wire
encoding, native bridge, shared Rust publication, and strict single-attempt
Playwright scope. Broader platform verification, distributed release binaries,
version policy, and the initial release version remain later work. The intended exit is direct published-package
consumption from N3; use a submodule only if evidence creates a concrete need.
