# Benchmark tooling

This is N3's application-specific measurement tooling, not an adopted benchmark
framework. Playwright owns isolated browser automation. Python's standard library
owns orchestration and descriptive statistics. Rust `libtest`, Python `unittest`,
and Node's built-in test runner verify the tooling itself.

Use the root commands from the repository root:

```sh
just measure-viewport --help
just benchmark-viewport --help
just benchmark-test
```

The Python entry points are `python3 -m tools.benchmark.measure` for one capture
and `python3 -m tools.benchmark` for repeated runs, offline reports, and comparisons.
The [measurement guide](../../docs/architecture/viewport-measurement.md) owns usage,
workload definitions, methodology, and report interpretation.

## Ownership

| Location                                     | Responsibility                                                                                                       |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| `measure.py`                                 | Build one host, record source/environment metadata, launch a bounded capture, and verify process completion          |
| `transport.py`                               | Local capability server, bounded capture envelopes, explicit file allowlist, and exclusive report writes             |
| `suite.py`                                   | Build snapshots, fresh serial repetitions, order, lifecycle receipts, and source/artifact stability                  |
| `contracts.py`                               | Admit complete raw evidence, independently check observed facts, enforce compatibility, and validate suite receipts  |
| `statistics.py`                              | Pure within-run distributions and descriptive aggregates across runs                                                 |
| `reports.py`                                 | Group admitted runs, preserve exclusions, compare compatible cases, and render JSON/text summaries                   |
| `browser/`                                   | Contributor page, environment observations, isolated Playwright process, and runner policy                           |
| `tests/`                                     | Numeric oracles, invalid-evidence mutations, lifecycle/transport tests, CLI checks, and browser policy tests         |
| `tests/fixtures/`                            | Reviewed measurement wire-contract fixtures; these are test data, not performance baselines                          |
| [`src/measurement/`](../../src/measurement/) | Feature-gated in-process Rust recorder, mode contract, stage/counter serialization, and deterministic recorder tests |

The product wrapper stays in [`web/`](../../web/). Benchmark pages are physically
outside that directory and cannot enter the ordinary web bundle. Selected 3D assets
and their provenance stay in [`fixtures/benchmarks/`](../../fixtures/benchmarks/).
Builds, copied artifacts, reports, and logs stay in ignored output directories.

## Trust boundaries

Saving a transport envelope does not establish that its performance evidence is
valid. Raw records pass through `contracts.py` before aggregation. Claimed validity
flags cannot override contradictory counters, mode stages, dimensions, focus, or
visibility observations. Contamination remains visible; slow samples are retained.

Tests use explicit numeric expectations and adversarial records. They must not
derive every expected field or answer from the validator under test. The shared
wire-contract fixture is checked against the actual Rust serializer and Python
admission schema, so a producer/consumer naming change fails visibly. Change that
fixture only with a deliberate contract review; never regenerate it silently.

`just benchmark-tools-test` runs Python and Node checks without a browser or GPU.
`just benchmark-rust-test` checks recorder semantics and exact real-renderer pixel
parity for scene-only composition. `just benchmark-test` runs both. These checks
are included in `just verify` and the CI checks lane. They verify correctness, not
a timing threshold; ordinary verification never installs or launches Chromium.

`just benchmark-web-check` separately compiles and lints the optional browser
recorder. The WASM CI lane runs it alongside the ordinary browser build, without
requiring Chromium or GPU access. Fixture tests verify the pinned Chess sources
and exact GLB reproduction without rewriting them.

Real native/browser captures remain an explicit integration check after changes
to collection, launch, packaging, or host instrumentation. Use a short bounded
matrix to verify those paths, then repeat representative workloads when making a
performance claim. Passing unit tests does not prove display latency, input feel,
GPU completion time, or statistical significance.
