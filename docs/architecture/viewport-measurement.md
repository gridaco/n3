# Viewport measurement

This contributor tool starts the investigation in [issue #2](https://github.com/gridaco/n3/issues/2).
It builds the real native or WASM application with the optional `viewport-measure`
feature, runs a bounded workload through the production rendering path, and saves
raw frame samples and summaries. Ordinary builds do not enable the feature.
Measurement output and compiled browser artifacts belong in ignored local storage.

The [benchmark package](../../tools/benchmark/README.md) documents code ownership,
test layers, and producer/consumer contract checks. `just benchmark-test` verifies
the measurement tooling without collecting a performance baseline or installing
a browser; ordinary `just verify` and CI include those correctness checks.

The single-run tool supplies raw evidence; the repeated benchmark tool controls
run order and produces machine-readable and plain-text comparisons. It does not
establish that browser overhead is
unavoidable, identify all causes of a slowdown, or approve a renderer rewrite.
Follow the [platform performance contract](platform-boundaries.md): retain each
host's execution policy and use evidence to distinguish a browser integration
mistake from a shared rendering cost.

## Build, run, measure

Run from the repository root. The Python tool uses the repository's Cargo cache
and respects `CARGO_HOME` and `CARGO_TARGET_DIR`. The optional browser target and
matching glue generator are installed through `just web-setup`. Install the separate
automated browser once with `python3 -m tools.benchmark.measure setup-browser`
(or `just measure-browser-setup`). This pins Playwright 1.63.0 in `.cache/viewport-tools/`
and its full Chromium browser in `.cache/viewport-browsers/`. It installs no global
browser and is not part of the native verifier.

```sh
python3 -m tools.benchmark.measure --host native --profile release \
  --mode editor \
  --input fixtures/benchmarks/chess-set/chess-set-1k.glb \
  --output .cache/measure/native-editor-release-orbit-01.json

python3 -m tools.benchmark.measure --host web --profile release \
  --mode editor \
  --input fixtures/benchmarks/chess-set/chess-set-1k.glb \
  --output .cache/measure/web-editor-release-orbit-01.json
```

The native command launches a separate editor process, measures, writes its
report, and exits. The browser command defaults to isolated automated headless
Chromium: it opens the harness, waits for the imported editor, presses **Run**,
saves the report, and closes its own browser/context. It uses a fresh temporary
profile, never the user's Chrome profile or an attached browser session. The
command succeeds only after a complete report and successful runner shutdown.
Timeout or interruption requests cleanup of the owned runner and browser.

Use `--browser headed` for the same automated loop with a visible isolated window.
Use `--browser manual` only for an explicitly chosen manual session: open the
printed localhost URL, press **Run**, and keep the page visible and focused.
`--open` opens the default browser only with `--browser manual`; it is rejected
for automated runs. Runtime timeout is controlled by `--timeout` (default 600
seconds; compilation time is excluded). Every invocation needs a fresh output
path. Existing reports are never overwritten.

Automation uses the `chromium` channel's full browser for both headed and new
headless mode; setup omits the separate headless shell with `--no-shell`.
[Playwright's browser documentation](https://playwright.dev/docs/browsers#chromium-new-headless-mode)
explains that distinction and the browser versions required by each Playwright
release. Device scale defaults to 2; `--device-scale-factor` changes the automated
context's scale while `--width`/`--height` still request physical pixels. Manual
sessions use the actual browser/device scale.

The runner sets Chromium's process device scale as well as the context's scale.
In the initial macOS headless check, context emulation alone reported DPR 2 but
`ResizeObserver.devicePixelContentBoxSize` still reported DPR 1. winit uses that
physical-size observation. The process flag makes the observations agree; it is
recorded in launch metadata, and a canvas-size mismatch still fails the run.

Headless rendering is a distinct test environment, not proof of native display
behavior or ordinary headed browser performance. Record browser mode/version,
launch configuration, and the actual GPU adapter/backend before comparing runs.
The runner does not silently switch to software rendering flags when GPU setup
fails. [Chrome's headless GPU testing guidance](https://developer.chrome.com/blog/supercharge-web-ai-testing)
illustrates why GPU availability must be checked rather than inferred from a
successful browser launch. Platform-specific GPU setup needs a recorded experiment;
do not copy Linux GPU flags into every host or equate a software adapter with a
hardware result.

The local server binds only `127.0.0.1`. A per-run token gates configuration,
selected input bytes, generated WASM/glue, and report submission. Only the file
explicitly selected with `--input` is exposed; a report cannot choose its output
path. Host/origin checks and a body-size limit constrain report submission. The
model stays local and is not copied into the repository or generated site. Treat
the printed URL as a temporary local capability and do not share it.

Each invocation compiles with `--locked --features viewport-measure`. Choose
`--profile dev` for a development-build comparison or `--profile release` for an
optimized-build comparison. Both hosts use that same named Cargo profile; the
ordinary browser `web` profile is not used here. The current `dev` profile uses
application optimization level 1 and dependency level 2; release uses Cargo's
release settings unless explicitly overridden. Reports record the command,
profile, Rust compiler, build flags, profile overrides, UTC timestamp, revision,
dirty state, and a fingerprint of tracked and nonignored untracked source files.
WASM builds use `build/measure-web/<profile>/`, separate from `build/web/`.

Available workload controls:

| Option                  | Default       | Meaning                                                                                                               |
| ----------------------- | ------------- | --------------------------------------------------------------------------------------------------------------------- |
| `--frames`              | `180`         | Retained frames after warm-up, maximum 10,000                                                                         |
| `--warmup`              | `60`          | Discarded frames before retained samples, maximum 10,000                                                              |
| `--workload`            | `orbit`       | `orbit` advances the shared camera operation by a fixed angle per rendered frame; `stationary` holds the camera still |
| `--mode`                | `editor`      | Complete `editor`, UI-free `viewport`, or `renderer` without editor projection and feedback; see mode contracts below |
| `--selected`            | off           | Select the first document object in `editor` or `viewport`; rejected in `renderer` mode                               |
| `--width`, `--height`   | `1280`, `800` | Requested physical dimensions of the complete editor surface, not CSS pixels or the inner 3D viewport                 |
| `--no-stage-timing`     | off           | Disable optional CPU stage and projection timing; execution counters remain enabled to verify mode exclusions         |
| `--browser`             | `headless`    | Automated `headless` or `headed` Chromium; `manual` explicitly opts out of automation                                 |
| `--device-scale-factor` | `2`           | Automated browser scale, finite and greater than zero up to 4; manual browser scale is observed instead               |

Both hosts reset to isolated default preferences and fixed solid shading, edges,
grid, light theme, and camera state. They do not modify the user's preferences.
Fresh automated runs start with the [FPS meter](frame-meter.md) disabled. Reports
record its actual initial enabled state and each retained frame's state; a change
invalidates the capture. Meter-on and meter-off captures are different UI
workloads and must not be pooled as repeats. Older reports without this field
have unknown meter state and cannot be mixed with observed-state reports.
The same harness accepts authored mesh inputs such as OBJ and self-contained GLB
scene assets. Linked asset source resolution and other document dependencies need
separate workload definition; the input hash identifies only the selected file,
not an asset dependency graph. Prefer self-contained fixtures for cross-host runs.

## Repeated benchmark workflow

Use `just benchmark-viewport` for an optimization investigation. It wraps the
existing measurement path, builds once per requested host, and runs each repeat
in a fresh native process or isolated browser. Cases run sequentially; their order
is shuffled within each round with a recorded seed. This reduces systematic
ordering bias without running competing workloads simultaneously.
The defaults are five repeats, seed `0`, and the `release` Cargo profile.
`--profile`, `--seed`, and `--no-stage-timing` make those choices explicit when
needed; the workload controls above retain their single-run meanings.

```sh
just benchmark-viewport run \
  --input fixtures/benchmarks/chess-set/chess-set-1k.glb \
  --hosts native web --modes editor viewport renderer \
  --repeats 5 --frames 180 --warmup 60 --browser headed \
  --output .cache/benchmark/chess-baseline
```

The suite defaults to headed Chromium. Headless and visible Chromium have shown
different frame pacing locally, including with little measured CPU work. Treat
browser mode as part of the environment rather than assuming that two executions
of the same build are interchangeable. The single-run command retains its
headless default for focused automated checks.

The output directory preserves raw JSON reports, runner logs, a run manifest,
and a frozen copy of each built native/browser artifact, alongside
`summary.json` (`n3.viewport-suite.v1`), and `summary.txt`. Keep these together: the summary is a lens over
the measurements, not a replacement for the samples or their provenance. A source
fingerprint identifies a revision of the local code but does not preserve that
code; retain its patch separately when necessary. Output directories must be
fresh so another investigation cannot overwrite its evidence. An output directory
inside the repository must be ignored; external temporary directories are also
supported.

The suite checks source identity before and after each session and hashes its
owned artifacts so another build cannot replace the code used by later repeats.
Raw suite reports carry execution-order metadata; their manifest records successful
completion and the report hash. Offline aggregation checks that receipt too, so a
JSON file left by a failed shutdown cannot become accepted evidence later. Keep
suite reports with their manifest when moving or archiving the output.

The manifest retains failed and partial attempts. The suite does not silently
retry them or include failed runner processes in its aggregate. Contamination,
runner failure, or an incompatible comparison produces a nonzero exit status;
inspect the retained evidence before deciding whether to rerun.

Regenerate an aggregate from existing raw reports, or compare two completed
aggregates after a focused change:

```sh
just benchmark-viewport report .cache/measure/chess-*.json \
  --output .cache/benchmark/chess-imported

just benchmark-viewport compare \
  .cache/benchmark/chess-baseline/summary.json \
  .cache/benchmark/chess-candidate/summary.json \
  --output .cache/benchmark/chess-comparison
```

Inspect rejected or contaminated reports before using a comparison. The tool
checks report completeness, the recorded mode contract, validity flags, and compatibility of
the measured workload and environment. Different modes and hosts remain separate
cases. A source change is intentional for a baseline/candidate comparison;
changes in input, dimensions, shading, camera path, instrumentation, or browser
environment can make its timing delta ambiguous.
The comparison command pairs the same host and mode across baseline and
candidate. Native/web or editor/renderer results can be read alongside each
other, but are not treated as interchangeable repeats of one case.

Frames within an orbit are correlated observations, not independent repetitions.
The aggregate summarizes each run first, then reports the median and range across
repeated runs, with median absolute deviation (MAD) describing their spread.
Within-run p95 and maximum frame times retain tail and interruption
evidence. It does not pool all frames into a larger apparent sample size, claim
statistical significance, or enforce a performance pass/fail threshold. Five
repeats are a starting point, not a guarantee that a small difference is real.
Slow samples remain in the data. A numerical outlier is not, by itself, grounds
for discarding a run. A zero-duration stage can reflect clock quantization;
percentage changes from a zero baseline are reported as undefined.

Use the report to choose the next instrument. CPU frame time, frame-start cadence,
and named stages answer different questions; execution counters confirm which
work happened. A repeated stage cost suggests a focused profile, while a long
frame-start interval with little CPU work suggests scheduling or presentation
investigation. Neither alone measures GPU execution or input-to-display latency.

The [methodology references](../../TESTING.md#benchmark-methodology) cover process
isolation, warm-up, repeated runs, randomized ordering, and retaining raw data.
N3 keeps a small orchestration/report layer around its real application: timing
the entire command would include startup and loading, while an isolated function
loop would not exercise the production host's frame scheduling. These references
guide the method; N3 does not claim their statistical analyses or use them as
interchangeable runtime harnesses.

## Measurement modes

The modes isolate executed work, rather than changing the visibility of controls:

| Mode               | Included work                                                                                                                            | Excluded work                                                                                       |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `editor` (default) | Complete production editor frame: egui, editor projection and selection preparation, 3D feedback, scene rendering, and final composition | Nothing from the normal measured frame path                                                         |
| `viewport`         | Editor projection and selection preparation, 3D feedback, production scene renderer, and direct scene composition                        | egui frame/layout, widget and overlay painting, tessellation, texture updates, and egui composition |
| `renderer`         | Scene/resource synchronization, camera updates, production scene renderer, and direct scene composition                                  | egui execution, editor projection/picking preparation, and editor interaction feedback              |

The normal **Hide UI** action is not a benchmark mode. It omits controls but still
runs egui, viewport interaction, projection, overlay painting, tessellation, and
composition. It also enlarges the scene area, changing the rendering workload.

All modes first load the input and establish the full editor layout before the
measured window. They capture that layout's scene rectangle and retain its physical
size, aspect ratio, device scale, and camera path. UI-free modes render into that
same rectangle with blank outer margins; they do not expand to fill the surface.
This is intentional: removing panels must not simultaneously increase the pixel
workload when comparing modes. It isolates execution differences under matched
scene dimensions, rather than representing a full-window renderer application.
A full-window no-UI workload would be useful for that separate question, but
would need its own label and dimensions; it is not currently an option.
The preparation frame is not a retained sample. The UI-free paths use the same
production renderer and caches, with direct scene composition replacing egui's
texture composition. They retain the host's surface and frame scheduling policy.
Renderer mode preserves base edges, grid, shading, lighting, and camera settings;
the omitted feedback is editor highlights, edit selection, and transform gizmos.

Choose `--mode viewport` or `--mode renderer` in the commands above and use a fresh
output name identifying the mode. Repeat every mode under the same conditions.
`--selected` is meaningful for `editor` and `viewport`; renderer mode rejects it
because selection feedback is outside that mode's contract.

Per-frame counters record actual entry into egui passes, tessellation, texture
updates, egui composition, editor preparation, scene rendering, and direct scene
composition, plus editor feedback updates. `validity.mode_contract_satisfied`
checks those counts against the requested mode even with `--no-stage-timing`.
A mode-contract failure causes the
tool to fail; a mode label or rounded zero duration is not proof of exclusion.
Other validity flags, such as lost focus, remain in the saved report so contaminated
runs can be inspected and excluded from comparison.

These are related workloads, not an additive accounting equation. Subtracting
renderer time from editor time does not precisely measure "UI cost": composition,
feedback, scheduling, and CPU/GPU overlap can change. Use the named stages and
execution counters to locate work before tracing or optimizing it.

## What the report establishes

The versioned `n3.viewport-measure.v2` JSON includes the retained samples in
original order and median, nearest-rank p95/p99, minimum, and maximum summaries.
It records scene counts, selection, renderer/adapter metadata supplied by the
host, mesh revision, dimensions, CPU stages, and narrowly scoped cache/upload
counters. The selected model's filename, byte length, and SHA-256 identify the
input without recording its absolute path. The source fingerprint distinguishes local changes but cannot reconstruct them;
preserve the reviewed patch alongside local evidence when needed.

`cpu_frame_ms` is elapsed host CPU time around the measured frame path.
`frame_start_interval_ms` is the interval between consecutive observed frame
starts. Surface acquisition, queue submission, and presentation API durations are
CPU observations; they do not measure when the GPU finishes or the display shows
a frame. A long API call can include waiting. A short call does not prove the GPU
is fast. `editor_projection` names the projection/occlusion work independently of
its entry point. In `editor` mode this nested duration is included within UI
timing; in `viewport` mode editor preparation owns it. Do not add it again to
either enclosing stage or total frame time. Skipped stage durations are `null`,
not zero. Disabling stage timing makes the CPU stage map and projection duration
`null`; raw execution and projection/cache counters remain present. Upload byte
counts cover the instrumented base mesh buffers, not all GPU traffic. Schema v1
reports used the misleading `projection_within_ui` name and lack mode contracts;
keep them as historical evidence instead of combining their fields with v2 data.

The fixed camera path exercises shared navigation and real rendering without
synthetic device delivery. It measures rendering throughput under that workload,
not OS/DOM event latency, physical trackpad responsiveness, or orbit input
coalescing. `stationary` deliberately requests frames: it is **not an idle
wakeup or power benchmark**. Both hosts retain their normal surface and scheduling
policies while the measurement asks for another frame.

Browser metadata includes user agent, device pixel ratio, CSS/physical canvas
sizes, viewport size, visibility, and the automation mode/configuration. JavaScript polls the result every 250 ms;
this is recorded as harness overhead. Browser loading records selected-byte
fetch/read duration separately from the synchronous `load_file` call. Neither
number establishes time to first fully rendered frame; native loading duration
is currently unavailable. Loading precedes the measured warm-up and frame sample
window.

Browser clock precision depends on browser policy and isolation. The initial local
browser smoke showed durations quantized around 0.1 ms. A zero stage duration can
mean the work fell below timer resolution; it does not establish that the stage
was free. Keep this uncertainty and instrumentation overhead visible when
comparing small native and browser stages.

Actual display refresh, presentation timestamps, and GPU timestamps are recorded
as unavailable rather than inferred from a nominal refresh rate or CPU frame
intervals. The first slice does not provide async GPU timestamp/readback support,
idle-event-loop sampling, full allocation profiling, or input latency traces.
Those are follow-up instruments, not reasons to add per-frame blocking GPU waits.

## Interpreting and extending evidence

### Workloads and fixture selection

Organize steady-state performance cases by internal representation and work:
authored topology, static material rendering, many placed objects, and animated
deformation. Do not multiply every performance case by every supported file
extension. Format adapters still need their own correctness tests. Parsing,
decoding, resource resolution, and initial upload need separate loading benchmarks
when those costs are under investigation.

The [authoring boundary](authoring-and-interchange.md) matters here. OBJ loads into
editable authored topology; glTF/GLB loads into immutable `SceneAsset` resources
placed in the same document. Both participate in the common projection/occlusion
cache, but their geometry preparation, rendering, and editing capabilities are
different. A GLB-only suite would miss authored vertex-edit and polygon-edge work.
Optimizations to shared code should transfer when the evaluated workload is
equivalent; similar silhouettes or triangle counts alone do not establish that.
Keep format dispatch in `asset_io`, with performance policy below that boundary.

The maintained corpus starts with the following fixtures and remaining workloads:

| Workload                 | Fixture direction                                                                                                                           | Current coverage                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Dense editable topology  | A reproducibly generated authored document, initially around 25,000 vertices, with a larger optional case                                   | The current orbit harness supports this representation; a public dense fixture still needs to be added |
| Static imported geometry | Poly Haven [Chess Set](../../fixtures/benchmarks/chess-set/README.md), 6,962,072-byte GLB, 49,150 vertices, 76,920 triangles, 33 mesh nodes | Pinned CC0 1K source and reproducible GLB; use the existing Solid orbit/stationary harness             |
| Static textured scene    | Existing Khronos [Water Bottle](../../fixtures/gltf/README.md), 8,966,700 bytes, 2,549 vertices, 4,510 triangles, four embedded images      | Already pinned with CC0 model data; an explicit Material Preview workload is still needed              |
| Many objects/draws       | Deterministic placements of existing geometry, with shared-resource versus unique-resource cases stated explicitly                          | Future workload; triangle count alone cannot stand in for object/draw count                            |
| Skinning and morphs      | Existing Simple Skin and Animated Morph Cube for correctness, then bounded replication or one larger licensed asset for scale               | Animation starts paused in the current harness; timed playback is a future workload                    |

In particular, the current session fixes Solid shading. Textures may be loaded and
resident, but those frames do **not** benchmark Material Preview/PBR shading.
Current upload counters cover base mesh buffers, not imported texture traffic.
Before using textured or animated fixtures for performance conclusions, add an
explicit shared workload setting, appropriate counters, and matching host reports.

Suggested acquisition budgets for the initial maintained corpus are at most
10 MiB per external asset and 25 MiB total, with 1K/2K textures and a recorded
decoded memory footprint. These are selection guidelines, not implemented import
limits. Increase computational load with explicit geometry/instance counts;
reserve larger downloads and 4K/8K texture stress for optional cases. Record source
revision or immutable download identity, license, checksums, authored and evaluated
counts, materials, image dimensions, and required extensions. Preserve upstream
files; any derived fixture needs its own reproducible recipe and provenance.

The [testing index](../../TESTING.md#external-reference-suites-and-tools) keeps
authoritative links to importer validators, graphics conformance suites, and
[fixture sources](../../TESTING.md#fixture-sources-and-provenance). Choose a bounded
subset for the missing workload. Conformance and source validity complement the
editor's performance evidence; they do not supply its camera, editing, or
frame-time benchmark.

### Comparing runs

First inspect validity flags and raw samples. Exclude runs with frame errors,
focus losses, visibility changes, viewport/DPR changes, or unexpected geometry
revision changes. The browser records contamination detected between retained
frames as well. A successfully saved report can still be unsuitable for comparison.
Verify the actual surface and inner viewport dimensions from the report; matching
requested dimensions alone is insufficient, particularly across different display
scales. Keep the canvas visible and avoid resizing or interacting during a run.

Collect multiple warmed repetitions on the same machine, power state, display,
input hash, profile, physical viewport dimensions, quality settings, selected
state, mode, and workload. For a deliberate mode comparison, change only the mode
and verify the reported scene rectangle and camera matrices match. Compare
distributions and raw frame pacing, not just a single
average or FPS estimate. Run paired instrumented/uninstrumented repetitions before
attributing a small difference to a renderer change. Keep adapter/backend and
browser versions explicit; the same `wgpu` code does not prove identical work,
synchronization, limits, or presentation behavior across hosts.

Use stage/counter evidence to choose a focused next trace. For example, repeated
projection work under camera-only orbit can direct investigation toward shared CPU
work; it does not by itself explain every host difference. If evidence isolates a
browser timing, scheduling, focus, or synchronization mistake, fix that adapter
and repeat the same workload. If a cost is shared, treat the optimization as
separate renderer work and preserve native capabilities. Classify a browser limit
as unavoidable only after a controlled experiment establishes that constraint.

A Three.js comparison also needs the same imported geometry, physical pixels,
material/shading, edges, selection feedback, camera path, and presentation
conditions. An editor and a minimal scene viewer do different work. A perceived
difference is a useful report, but an unmatched comparison cannot identify its
cause or acceptable magnitude.

Tooling regressions run with `just tools-test`.
They cover host/profile builds, artifact preservation, input metadata, isolated
browser setup, runner failure/timeout/cleanup, report completion, and the actual
localhost server's token/path/origin/output boundaries. Runtime native and
browser measurements remain necessary: a parser test or WASM compilation is not
measurement evidence.
