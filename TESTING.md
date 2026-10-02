# Testing N3

This is the contributor index for verification, benchmark tooling, reference
suites, and test assets. Setup and environment details live in
[development](docs/development.md); change and review rules live in
[AGENTS.md](AGENTS.md). The [justfile](justfile) defines the executable commands.

## Choose the evidence you need

| Task                               | Entry point                      | What it establishes                                                                                      |
| ---------------------------------- | -------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Verify an implementation change    | `just verify`                    | Native formatting, Clippy, tooling regressions, and the complete Rust suite including exact guide replay |
| Run Rust behavior tests            | `just test`                      | The native Rust suite, including guide checks; accepts Cargo test arguments for focused runs             |
| Check generated user documentation | `just docs check`                | Read-only replay and exact artifact verification on the native renderer                                  |
| Check the browser build            | `just web-verify`                | Node wrapper regressions, WASM Clippy, and static-site generation; does not launch a browser             |
| Check development tooling          | `just tools-test`                | Python tooling tests and Node wrapper tests                                                              |
| Check real Metal capture           | `just test-metal`                | Focused capture regressions on a Mac with an available GPU                                               |
| Reproduce CI environments          | `just ci`, `just ci-macos`       | Explicit Ubuntu Docker checks, or macOS bundle/signing and native integration checks, respectively       |
| Measure the viewport               | `just measure-viewport --help`   | Discover the opt-in native/browser build-run-measure workflow and its workload options                   |
| Repeat and compare measurements    | `just benchmark-viewport --help` | Run a controlled suite, aggregate raw reports, and compare a baseline with a candidate                   |
| Verify benchmark correctness       | `just benchmark-test`            | Recorder, evidence admission, statistics, lifecycle, browser policy, and renderer parity contracts       |

`just verify` remains the native development gate. Browser installation and
performance measurements are separate, optional workflows. See
[verification and hooks](docs/development.md#verification-and-git-hooks) for
the full environment and CI boundaries.

`just benchmark-web-check` compiles and lints the optional WASM measurement
adapter; the WASM CI lane includes this check alongside the ordinary browser
build. It does not install or launch a browser.

Compilation, replay, device interaction, and performance measurements answer
different questions. Report which evidence was collected. In particular, a
successful WASM build does not prove browser input behavior, and a short CPU
submission time does not measure GPU completion or display latency.

## Browser runtime and performance

The [browser development guide](docs/development.md#browser-build) owns runtime
interaction checks, embedding, and host limitations. Native and web share one
feature UI and user guide.

The [viewport measurement guide](docs/architecture/viewport-measurement.md) owns
setup, commands, report fields, workload definitions, fixture selection, and
comparison methodology. Its `editor`, `viewport`, and `renderer` modes measure the
complete app, editor viewport without egui, and scene renderer without editor
projection/feedback, respectively. They preserve the scene dimensions and record
execution counters to verify exclusions; **Hide UI** is not an isolation mode.
Its Playwright runner launches a fresh isolated Chromium
profile, supports headless and headed execution, and saves browser/GPU diagnostics
with raw samples. This currently automates the measurement loop, not every editor
interaction. The [Playwright browser documentation](https://playwright.dev/docs/browsers)
explains browser installation and execution modes; N3's tooling owns its version
pin and local cache paths.

Use the [repeated benchmark workflow](docs/architecture/viewport-measurement.md#repeated-benchmark-workflow)
for performance investigations. It runs fresh application/browser instances in
seeded shuffled rounds and writes raw reports, logs, and CLI/JSON summaries. The
suite defaults to a visible isolated browser because headless mode can change
frame pacing. It summarizes independent runs without treating every frame as an
independent repetition. Results are descriptive evidence, not a statistical
significance claim or a performance CI gate.

The implementation lives in [`tools/benchmark/`](tools/benchmark/README.md), with
its own tests and wire-contract fixtures. These correctness tests run in ordinary
verification and CI; timing collection stays opt-in.

Benchmark internal workloads: authored topology, scene rendering, object count,
or deformation. Keep format decoding correctness and loading performance distinct
from steady-state rendering. The
[authoring and interchange contract](docs/architecture/authoring-and-interchange.md)
explains why editable meshes and immutable scene assets need different coverage
even when an optimization uses shared code. Each platform retains its own
[capabilities and performance policy](docs/architecture/platform-boundaries.md).

## External reference suites and tools

These are resources to select from for a concrete investigation. Listing an
upstream suite here does not mean N3 runs or passes it in CI. Keep selected cases
and tool versions reproducible, and record their purpose alongside the result.

| Resource                                                                             | Use in N3                                                                                                                                      |
| ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| [Khronos glTF Asset Generator](https://github.com/KhronosGroup/glTF-Asset-Generator) | Targeted importer cases covering accessors, buffers, materials, primitives, animation, and other glTF semantics                                |
| [Khronos glTF Validator](https://github.com/KhronosGroup/glTF-Validator)             | Validate source assets independently of N3; valid input does not establish correct rendered output                                             |
| [WebGPU Conformance Test Suite](https://github.com/gpuweb/cts)                       | Investigate behavior defined by the graphics API; the suite can run inside WPT or standalone, and does not provide an editor performance score |
| [wgpu testing guide](https://github.com/gfx-rs/wgpu/blob/trunk/docs/testing.md)      | Find upstream validation, GPU, shader, and benchmark coverage when reducing a problem to wgpu or naga                                          |

Use the [executable documentation pipeline](docs/architecture/documentation-pipeline.md)
for N3's feature behavior and rendered guide evidence. Upstream conformance tests
complement that application-specific contract.

## Benchmark methodology

These primary references explain the practices used by N3's benchmark tooling.
They are useful beyond any one renderer optimization; they are not dependencies
of the suite or evidence that N3 implements each framework's statistical model.

| Resource                                                                                                                                  | Practice to apply                                                                                                                       |
| ----------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| [pyperf runner](https://pyperf.readthedocs.io/en/stable/runner.html) and [analysis](https://pyperf.readthedocs.io/en/stable/analyze.html) | Fresh worker processes, explicit warm-up, repeated measurements, raw samples and metadata, and visible noise rather than a single score |
| [Google Benchmark user guide](https://google.github.io/benchmark/user_guide.html)                                                         | Random interleaving to reduce ordering bias, repeated-run aggregates, and machine-readable results                                      |
| [Criterion analysis](https://bheisler.github.io/criterion.rs/book/analysis.html)                                                          | Separate warm-up, measurement, and comparison; inspect outliers and distinguish small changes from noise                                |

N3's harness measures the real application's retained frame window. Whole-command
timing includes startup/import/shutdown, and a function microbenchmark has a
different execution model. Use those tools for a matching question rather than
substituting them for end-to-end frame measurements.

## Fixture sources and provenance

| Source                                                                                      | Role                                                                                                            |
| ------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| [N3 geometry fixtures](fixtures/README.md)                                                  | Canonical small geometry and import cases, with expected topology and source attribution                        |
| [N3 glTF fixtures](fixtures/gltf/README.md)                                                 | The existing pinned Khronos subset, license records, checksums, and validator evidence                          |
| [Chess Set benchmark fixture](fixtures/benchmarks/chess-set/README.md)                      | Pinned CC0 Poly Haven scene, reproducible GLB, and source/derivative validation for native/browser measurements |
| [Khronos glTF Sample Assets](https://github.com/KhronosGroup/glTF-Sample-Assets)            | Curated representative and feature-focused models; licensing is per asset, not uniformly CC0                    |
| [Poly Haven](https://polyhaven.com/) and its [asset license](https://polyhaven.com/license) | CC0 models, textures, and environment maps when the existing corpus lacks a needed workload                     |

Reuse existing fixtures first. Keep canonical assets and attribution in
`fixtures/`; keep disposable experiments and review output in ignored local
storage. Preserve upstream files and record exact revisions, bytes, and checksums.
Derived fixtures need a reproducible recipe and their own provenance. The
[benchmark fixture policy](docs/architecture/viewport-measurement.md#workloads-and-fixture-selection)
defines the initial workload plan and suggested size budgets.

When adopting another suite or tool, add its authoritative link here and explain
which layer it tests. Put detailed instructions beside the owning workflow;
keep execution receipts and temporary results out of this index.
