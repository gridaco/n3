# Developing N3

The application is a single Rust package named `n3`, with private implementation
modules. The workspace also maintains the portable `executable-docs` SDK and its
consumer examples under `crates/`; the root package remains the default Cargo
target. Read [AGENTS.md](../AGENTS.md) for development principles and iteration
guidance. [Testing](../TESTING.md) indexes verification workflows, reference suites,
and benchmark resources. Start at [the architecture map](architecture/architecture.md) and
[the milestone review](architecture/milestone-review.md). The old viewer-only
spike has been removed; the root package holds the maintained application.

## Local commands

Install Rust, `just`, and Python 3. Native app development on macOS also needs
the command-line developer tools. Docker is optional for reproducing Ubuntu CI. From the
repository root:

```sh
just run --empty
just build
just check
just cargo run --locked -- --check examples/cube.n3.json
just test
just verify
```

The editor initially opens at 1440 × 900 logical points, capped to 90% of the
primary monitor's full dimensions. This leaves desktop margins; winit does not
expose the usable work area excluding the Dock and menu bar. The usual minimum
is 820 × 500 points, reduced when necessary to fit the capped launch size.
Executable guide captures keep their own fixed dimensions.

For focused component development, run `just workbench`. The internal
[UI workbench skill](../.agents/skills/ui-workbench/SKILL.md) owns the agent workflow
for fixture cases, deterministic evidence and component boundaries.

Rust 1.98.1 and `just` 1.46 are the pinned development tools. The repository's
`rust-toolchain.toml` selects Rust with Clippy and rustfmt without changing the
global toolchain. `Cargo.toml` retains Rust 1.95 as the minimum supported
version; the development pin and minimum serve different roles. The root
`justfile` serves local development first: `just test`, `just docs check/update`,
and `just verify` use the native toolchain and renderer. On macOS that is Metal;
on Linux it is an available Vulkan adapter. No Docker daemon or CI environment
marker is required. A missing native capture adapter still fails visibly.

`just cargo ARGS...` forwards other Cargo commands to the native host. Native
recipes use repository-local Cargo/cache directories and respect `CARGO_HOME`
and `CARGO_TARGET_DIR` overrides. After dependencies are available, set
`CARGO_NET_OFFLINE=true` for offline native runs. Dependency versions live in
`Cargo.lock`; verification uses `--locked`.

For an explicit Ubuntu CI reproduction, use `just ci`, `just ci-test [ARGS...]`,
or `just ci-docs check/update`. Only these commands require Docker. Their runner
uses separate caches under `.cache/ci/linux-amd64` and mounts source read-only
for checks. `ci-docs update` writes the separate Linux receipt and ignored review
captures while keeping the native guide read-only. No generated app bundle,
cache, or review capture belongs in Git.

Hosted Ubuntu CI runs two independent jobs: `just ci checks` performs formatting,
Clippy, tooling tests, and all Rust tests except the full-guide comparison;
`just ci guide` runs that exact comparison. Both discover the current Rust test
inventory and reject a missing or ignored guide test, overlapping partitions, or omitted
tests. `just ci` still runs the complete sequence in one container, and native
`just verify`, `just test`, and the pre-push hook always retain the full suite.

Hosted jobs cache dependency downloads and compiled build output under keys
that include their platform, pinned toolchain/environment, Cargo manifests, and
vendored dependency source.
Incremental compilation output, generated guide artifacts, renderer receipts,
diagnostics, and account files are excluded. Pull requests can restore caches;
only successful checks, native, and WASM jobs on pushes to `main` save them. The guide
job restores the shared Ubuntu cache without publishing it. Cache hits still
execute every check, including the exact guide replay. The macOS job also caches
the pinned `just` installation while retaining its ordinary installation command.

Containers retain the invoking user's numeric UID/GID so writable cache and
review files keep their host ownership. The runner mounts isolated passwd/group
entries read-only from its cache; the terminal backend needs a resolvable account
even when its controlled shell is explicitly configured.

## Browser build

The browser target uses the same package with an additional WASM library entry
point. The native application, documentation renderer, terminal backend, and
filesystem host remain target-specific. Both hosts use the same workspace and
user guide; host capabilities omit or disable unsupported controls.
Read [platform boundaries and performance](architecture/platform-boundaries.md)
before extending the port or changing host ownership.

For opt-in native/browser CPU-stage and frame-pacing measurements, use
[`just measure-viewport`](architecture/viewport-measurement.md). It builds the
production hosts with contributor instrumentation and records raw samples;
ordinary builds do not include the recorder or measurement exports.

```sh
just web-setup
just web-check
just web-verify
just web-build
just web-build --profile release
just web
```

`web-setup` installs the `wasm32-unknown-unknown` target for the repository's Rust
toolchain and the matching `wasm-bindgen-cli` version, 0.2.129, under
`.cache/web-tools`. `web-check` checks the library for that target with the locked
dependency graph. `web-build` defaults to the `web` development Cargo profile;
`--profile release` selects the optimized production build. Both write the static
application to ignored `build/web/`, including `pkg/n3.js`, `pkg/n3_bg.wasm`, and
the bundled font licenses.
It requires no Node, bundler, React installation, Docker, or application server.

`web-verify` runs the wrapper lifecycle regressions with Node.js 24, Clippy with
warnings denied for the WASM library, and the complete static-site build. A
dedicated Ubuntu WASM CI job runs the same wrapper tests and lint, builds the
release artifact, and checks the optional measurement adapter. It supplements the
native jobs without requiring a browser GPU on that runner.

The pinned `egui-winit` 0.36.2 adapter requires narrow WASM compatibility patches
for its dropped-file trait and browser OS modifier mapping. The maintained source, upstream
licenses, and exact change are recorded in
[vendor/egui-winit/N3-PATCH.md](../vendor/egui-winit/N3-PATCH.md). Native behavior
is preserved; browser file contents enter through N3's selected-byte adapter.

`just web` builds and serves that directory on localhost port 8000, then opens
the browser. To select another port or leave browser opening to another tool:

```sh
just web --no-open --port 8001
```

Keep the server running while using the application; Ctrl-C stops it. A desktop
browser with WebGPU enabled and available is required. Remote hosting requires
HTTPS; opening the generated HTML as a local `file:` URL is not the supported
launch path. The output is static; module and WASM URLs resolve relative to the
site, including when it is hosted under a repository subpath.

Target compilation and browser runtime checks are separate from `just verify`.
The native verification command continues to check shared behavior and the
strict documentation baseline. Browser runtime evidence must additionally record
successful WebGPU initialization, rendering, interaction, file round-tripping,
and lifecycle behavior on the browser actually tested. These host checks use the
same feature contract; they do not create a second guide or media baseline.

Include a fresh-load input check before switching focus away from the canvas or
using controls in an embedding page. Move the pointer into the viewport, use
Shift+I to open Insert, create an object, click empty space to deselect, and
double-click the object to enter edit mode. Check held Z and Space through the
shared shading and navigation behavior, then verify focus loss cancels held
input and returning to the canvas restores ordinary interaction. Repeat with
multiple objects to check selection changes. Using wrapper buttons first can
hide a startup focus bug by causing an extra blur/focus transition.

The browser host initializes egui focus from the current window after asynchronous
WebGPU setup; the canvas may already have received its initial focus event before
the input adapter exists. Keep subsequent focus events ordered through the usual
adapter. The local server disables HTTP caching, but an already-open tab retains
its loaded WASM until reloaded after a rebuild.

### GitHub Pages

The canonical repository publishes the web app through
[the CI workflow](../.github/workflows/ci.yml). Its WASM job packages only
`build/web/` as a GitHub Pages artifact. The deployment job runs for a push to
`gridaco/n3`'s `main` branch after all four CI lanes succeed. Pull requests and
forks build and verify without publishing. Failed verification leaves the
previous deployment in place.

GitHub Pages uses the **GitHub Actions** source and the `github-pages` environment.
Only the deployment job receives Pages and OIDC write permissions. Main CI runs
finish before their successor starts, so a new push cannot interrupt an active
deployment. To retry a failed deployment, rerun the failed job while its artifact
is retained; otherwise rerun the entire main CI run to rebuild the artifact.

Generated WASM, JavaScript glue, and site copies stay in ignored `build/` and
Actions artifacts. No deployment branch or generated commits are needed. The
published site tracks verified main development; it is not a versioned release.
See GitHub's [custom Pages workflows](https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages)
for the hosting contract. A new fork must explicitly configure its own site and
adjust the canonical-repository deployment conditions before publishing.

### Browser gestures

The browser host captures wheel and WebKit gesture events on its canvas, then
passes them through [browser_navigation](../src/input/browser_navigation.rs)
into the shared [navigation router](../src/input/navigation_events.rs). The
router retains viewport, focus, popup, and active-edit ownership. Canvas capture
prevents the browser's default page zoom for an editor pinch; it does not install
page-wide gesture handlers. Ordinary scrolling over egui controls remains UI
input rather than camera input.

Browsers encode trackpad pinch as a wheel event with `ctrlKey`, or as WebKit's
cumulative `scale` and `rotation`. The wheel flag identifies the zoom event; it
must not become a synthetic held keyboard Control modifier. A real Control-wheel
combination is indistinguishable from pinch and also zooms. WebKit samples become
relative zoom and rotation deltas; events from the same gesture must not also
reach the camera through winit's wheel path.

Pixel deltas use CSS pixels converted to egui logical points, independently of
the display's backing-pixel ratio. Line deltas retain wheel-line units; page
deltas use the canvas's CSS height. Browsers provide no reliable wheel-device
identity: unmodified pixel scrolling uses the shared precise-scroll policy,
including when a high-resolution mouse produces it. The adapter does not infer
finger or momentum start/end phases from timing. WebKit exposes rotation;
Chromium's pinch-wheel events contain no twist information.

The shared navigation scenario exercises these conversions and ownership rules.
DOM delivery, browser default prevention, and physical trackpad feel require
browser testing; native replay alone cannot establish them.

### Embedding

The framework-neutral [wrapper](../web/n3.js) exports
`mountN3(container, { onState, onError })`. It creates and sizes the canvas, awaits
GPU startup, and returns a controller:

| Method                   | Behavior                                                                                    |
| ------------------------ | ------------------------------------------------------------------------------------------- |
| `command(id)`            | Dispatch a semantic action, such as `insert.cube` or `history.undo`, then focus the canvas. |
| `open(file, { append })` | Read a browser `File` and confirm replacement when needed; `append` defaults to `false`.    |
| `download(filename)`     | Request a document download; the default name is `Untitled.n3.json`.                        |
| `documentJSON()`         | Return serialized document text.                                                            |
| `snapshot()`             | Return the parsed status object.                                                            |
| `destroy()`              | Stop the editor, disconnect observation, and remove listeners and the canvas.               |

The controller exposes its `canvas`. `onState` receives status changes including
object and selection counts, dirty state, dimensions, and errors; callbacks run
after the Rust state borrow is released. A JavaScript or React application may
own surrounding UI while Rust retains workspace behavior and document editing.
This experimental bridge does not expose arbitrary document mutation, GPU
resources, or a stable UI-independent editor API.

```js
import { mountN3 } from "/n3/n3.js";

const editor = await mountN3(document.querySelector("#editor"), {
  onState: (state) => console.log(state.objects, state.dirty),
  onError: (error) => console.error(error),
});
```

Serve `n3.js` and its generated `pkg/` together under `/n3/` in this example.
Give the container an explicit usable size. Preserve the wrapper's file
confirmation and teardown behavior when extending it.

The lower-level generated `start(canvas)` export returns a `WebApp` handle and
initializes the GPU asynchronously. The canvas emits `n3-ready` or `n3-error`;
methods require a ready application. Its methods are `command(id)`,
`load_file(name, bytes, append)`, `document_json()`, `new_document()`,
`settings_json()`, `snapshot()`, `resize(width, height)`, and `destroy()`.
Direct callers must resolve unsaved changes before replacing the document.

Mounting is asynchronous: unmounting before startup completes must still destroy
the eventual controller. The baseline supports one editor lifetime per WASM
module; destroying it does not establish that the module can mount again.
Direct React effects that remount, including development StrictMode, therefore
require additional lifecycle work. Ordinary React mounting and removal can use
an iframe, which gives each frame its own module lifetime:

```jsx
export function N3Panel() {
  return (
    <iframe
      title="N3 editor"
      src="/n3/index.html"
      style={{ width: "100%", height: "80vh", border: 0 }}
    />
  );
}
```

A cross-frame command/state bridge is not implemented. Direct mounting provides
that integration in a persistent page; multiple editors within one module need
a separate lifecycle design and verification.

### Browser host constraints

The host accepts selected bytes for `.n3.json`, OBJ, and self-contained glTF/GLB.
Parsing and unit conversion remain in `asset_io`. Selected filenames are labels,
not directory access: external buffers, textures, and linked source files need a
resource resolution workflow. External asset resources fail explicitly; native
documents preserve unresolved references for recovery. Saving does not embed
imported assets or make those references portable.

Downloads cannot confirm durable storage or promise an in-place overwrite or
native save-conflict detection. The dirty indicator therefore remains set after
a download request. The wrapper confirms destructive replacement and requests a
`beforeunload` warning, subject to
[browser restrictions](https://developer.mozilla.org/en-US/docs/Web/API/Window/beforeunload_event);
this is not a recovery store.

Preferences use the shared typed controller and an origin-local store, separate
from documents and history. A different host or port has a different store.
Storage can be [denied, cleared, or temporary](https://developer.mozilla.org/en-US/docs/Web/API/Window/localStorage);
failures remain visible without preventing editing. The read-before-write merge
check is optimistic and has no atomic cross-tab lock.
Local edits, storage events from other tabs, and focus recovery trigger merges
at safe input boundaries. Successful synchronization has no periodic polling;
storage failures retry with a bounded backoff. The native filesystem store keeps
its independent synchronization policy.

- WebGPU requires a [secure context](https://developer.mozilla.org/en-US/docs/Web/API/WebGPU_API)
  and a compatible browser, device, and policy. There is no WebGL fallback;
  initialization failures remain visible.
- The host cannot launch a local shell/PTY or open a settings file in an external
  editor. Those controls are omitted from the shared UI.
- Browser-reserved shortcuts may not reach the canvas. Focused-workspace input
  must preserve surrounding page controls. The gesture adapter above maps the
  events the browser exposes without claiming native phase or momentum parity.
- The egui adapter uses an internal clipboard fallback; external clipboard
  integration requires a browser adapter.
- The application runs on the main thread. Large decoding and geometry work can
  interrupt responsiveness; workers and shared memory require separate work.
- Touch, IME, accessibility, physical trackpads, and browser/GPU coverage remain
  review gates. A desktop smoke test does not establish these capabilities.
- One canvas is the presentation unit. Native multi-window delivery and multiple
  editors in one module remain outside the baseline.

## Formatting

Native contributor formatting additionally uses Node.js 24/npm with `npx`.
`tools/format_docs.py` holds the single Oxfmt version pin and invokes that
version through `npx`. Run `just tools-install` to warm its npm cache, or
`just setup` to do that and activate the Git hook. The default cache is
`.cache/npm`; set `npm_config_cache` to use another writable location. The
canonical CI image includes Node/npm and uses the same wrapper. No repository
`package.json`, `package-lock.json`, or local `node_modules` installation is
needed. The application and local guide preview do not require Node.

`just fmt` runs rustfmt and Oxfmt; `just fmt-check` checks both without writing.
The separate `fmt-rust` / `fmt-rust-check` and `fmt-docs` / `fmt-docs-check` recipes
are useful during focused work. Most Oxfmt input is Markdown; it also handles
the repository's supported configuration and browser-preview sources.

Oxfmt options and exclusions live in `.oxfmtrc.json`. `TODO.md` is deliberately
excluded from formatting but remains repository-owned and eligible for Git.
Generated guides, vendored preview code, the Cargo lockfile, and example
documents are also excluded. The wrapper's `--check` and `--write` modes format
authored sources; for `.md.in` templates it sends Markdown through Oxfmt's CLI
stdin, verifies that semantic bindings and their order survive, and validates
all templates before writing any of them. After formatting changes a guide
template, use `just docs update` and review the generated text; never format
generated pages independently.

## Verification and Git hooks

The published guide baseline uses native macOS Metal. Local development compares
every artifact directly against that guide. Ubuntu CI replays the same scenarios
with the pinned lavapipe renderer and checks every artifact's byte length and
SHA-256 against `docs/baselines/linux-vulkan-lavapipe.json`. The receipt also pins
the entire native guide, and generated Markdown must match its published text.
Missing receipts, stale native guides, inventory changes and byte drift fail;
there are no image tolerances, skipped captures or automatic updates.

After deliberately updating and reviewing the native guide, run
`just ci-docs update` to generate the Linux review tree under
`.cache/docs/linux-vulkan-lavapipe/` and update its receipt. Inspect that text,
stills and animation playback, then run `just ci-docs check`. Only this explicit
Ubuntu reproduction requires Docker; native development remains independent.

`just setup` installs contributor dependencies and configures
`core.hooksPath=.githooks` in local Git configuration, shared by the clone's linked
worktrees. `just hooks-install` can be run separately and refuses to replace
another configured hooks directory or disable existing active default hooks.
Each clone needs setup.

The pre-push hook checks the commit being pushed: non-deletion refs must resolve
to the current `HEAD`, with no staged, unstaged, or nonignored untracked changes.
It runs `just verify` and checks that the checkout did not change during the run.
Switch to the branch to be verified before pushing it. Deletion-only pushes have
no source to verify. The hook never rewrites files or updates generated baselines.

`just verify` runs Oxfmt and rustfmt checks, Clippy with warnings denied, Python
tooling tests, JavaScript wrapper regressions, and all Rust tests on the host. The Rust suite includes
required documentation replay and exact artifact comparisons. `just test`
forwards optional Cargo test arguments; `just tools-test` runs the Python and
JavaScript suites.
Checks never update the guide, including when a renderer or driver changes.

The optional [CI image](../tools/ci/Dockerfile) fixes Ubuntu 24.04 on
`linux/amd64`, a 2026-09-28 APT snapshot, and Mesa lavapipe software Vulkan with
`sse2` CPU capabilities and 256-bit vectors, with its shader cache disabled. This
selects accurate sqrt for sRGB conversion and prevents cached 128-bit code from
bypassing that choice. The [baseline contract](baselines/README.md) records the
cross-host evidence and diagnostic commands. It requests `N3_DOCS_RENDERER=lavapipe` explicitly;
normal developer commands select the native backend without that setting.
The opt-in container uses one Rust test thread. Default parallel execution
reproduced allocator aborts and a segmentation fault among GPU-backed tests on
both the committed native baseline and the browser port. The underlying fault
is not established; serial test scheduling is the contained CI policy. All
tests and exact captures still execute, and native `just verify` retains the
host's ordinary concurrency. Explicit `ci-test` arguments can override test
thread count for diagnosis.
Updating a rendering profile requires reviewing its generated output. A Docker
build alone does not establish that the guide agrees with its baseline.

Hosted macOS CI separately runs `just ci-macos`: build the app, verify its ad-hoc
signature, bundled icon, and metadata, and run focused `native::` tests. The
repository assembles the `.app` itself in `just build`; Cargo builds its executable
and the recipe supplies `Info.plist`, resources, and local ad-hoc signing. The
macOS icon is checked in at `assets/logo/N3.icns` and regenerated from the SVG
source with `just tools icon` when the logo changes. General CI stays on Ubuntu.
`just test-metal` is a focused capture check on a Mac with GPU access;
`just test` runs the complete native suite.

Hook tests use temporary repositories and a stub verifier to check rejection and
failure propagation without pushing or changing this checkout. `just hooks-test`
runs them directly. The CI workflow becomes live when the repository is pushed.

## Documentation is executable

Every user-guide feature has one registered Rust scenario under
`src/documentation/scenarios/` and exactly one narrative source. Registration
lives in `src/documentation/features.rs`: `template: None` means the scenario
authors the guide through the N3 `Guide` adapter; `Some(...)` selects its retained
Markdown template under `docs/templates/`. The `hand-tool` and `navigation`
features now author their prose in Rust alongside the replay. The remaining
26 features still use templates. Do not maintain both forms for one feature.

User-facing state and behavior come from the real workspace, semantic commands,
navigation adapters, and scene renderer. Assertions verify the intended outcome.
The portable SDK owns document composition, frozen evidence, and shared artifact
publication; it does not own N3's input or renderer. See
[executable documents](architecture/executable-documents.md) for the maintained
boundary and [the SDK README](../crates/doc-harness/README.md) for its API.

```sh
just docs
just docs update
just docs check
```

`just docs` starts a local guide preview and opens it in the default browser.
It requires Python 3, serves only on `127.0.0.1`, and chooses an available port.
The terminal prints the URL; Ctrl-C stops the server. To choose a port or leave
browser opening to another tool:

```sh
just docs serve --no-open --port 3030
```

The preview reads the checked-in Markdown and WebP media without rebuilding the
application, running GPU captures, or changing baselines. Run `just docs update`
in another terminal after intentionally changing the scenario's narrative or
its retained template, then refresh the browser. `just docs update` regenerates through the native renderer;
`just docs check` verifies there without writing. Neither requires Docker.
The small preview uses a vendored Docsify runtime, so it needs no network access,
Node installation, or package-manager setup. It is a local reading surface, not
a choice of framework for the eventual documentation website.

To inspect both audiences before updating the canonical baseline, build a fresh
candidate in ignored local storage:

```sh
just docs build --out .cache/docs-candidate
```

This performs one complete replay, then writes `reader/` and `contributor/`
subtrees from the same completed sessions and captured bytes. The two
Rust-authored guides add contributor notes; retained template pages are identical
across audiences. The destination must be new and separate from `docs/guide` and
`docs/baselines`. This command does not accept a baseline or change the roles of
`just docs check/update`.

Review changed text, stills, and animated clips before accepting regenerated
outputs. `check` does not rewrite or approve anything. The normal test suite
replays all scenarios and checks the complete generated tree, including animation
bytes and the manifest. Missing, orphaned, duplicate, and unreferenced artifacts
fail. A generation failure writes no new baseline.

Rust-authored guides take witnessed controls and canonical shortcut labels from
the `Guide` adapter, and insert typed handles into their authored Markdown.
Captures remain `Session` operations; the adapter registers their existing bytes
without replaying the action or advancing the clock. The current migrations keep
the published guide text, media paths, media bytes, and `manifest.txt` format.

Retained templates render application shortcuts with `{{shortcut:semantic-id}}` as
`<kbd>` keys from the canonical input bindings, and witnessed menu paths with
`{{control:id}}` as inline `<code>`. Literal text-entry keys and gesture modifiers
use `{{key:Name}}` and `{{modifier:name}}`. Do not copy remappable keys into prose
or scenario values. Replay illustrated shortcuts through `Session::shortcut`,
`shortcut_down`, and `shortcut_up`; raw keys remain appropriate for literal input
and explicit input-ownership probes.

Asserted values, stills, and clips use `{{value:name}}`, `{{image:name}}`, and
`{{animation:name}}`.
Callouts follow live control bounds and fail when their target disappears.
See [authoring and replay contracts](architecture/documentation-pipeline.md) for
timing, input ownership, animation examples, and verification boundaries.

The manifest records the renderer profile that generated the guide; the full
adapter and driver identity is reported at execution time. Exact byte checks
require the same renderer, and driver changes can still cause pixel drift. A
profile mismatch gives a specific diagnostic. Baselines stay strict: no hidden
visual tolerance, automatic update, or skipped images. The scenarios explicitly select macOS styling,
so the guide still depicts the Mac application when replay runs on Ubuntu.
Synthetic replay proves application behavior, not physical trackpad feel,
Finder delivery, macOS dialogs, or artistic approval.

`just docs-preview-test` checks the preview server's routes and browser asset
contract using Python's standard library. It also runs as part of `just verify`;
it does not replace the Rust replay and exact artifact checks.

For focused framework work, run `just doc-framework-test`. It checks the maintained
SDK and the config CLI and serde_json consumers without starting N3's renderer.
This focused gate also runs inside `just verify`.
The [consumer README](../crates/doc-example-config/README.md) documents direct Cargo
commands, package-local baselines, and candidate review. These examples exercise
two integration shapes; the serde_json example is our use of an existing library,
not adoption by its maintainers. N3 adapter or scenario changes still require
the native replay and `just verify`.

## User settings

Global preferences live at `~/Library/Application Support/N3/settings.json` on
macOS. Preferences can open that file in a text editor. See the
[user guide](guide/settings.md) for keys and recovery, and the
[settings boundary](architecture/user-settings.md) before changing persistence.
Settings tests and guide replay use isolated stores; they must never load or
modify the current user's file.

## Documents and imports

`.gltf` and `.glb` open as linked asset placements in an ordinary N3 document.
Native geometry and imported placements share selection, transforms, duplication,
deletion and Undo. Source meshes, materials and animation remain immutable;
decoded resources and playback stay outside authored history. See the
[imported asset contract](architecture/scene-viewer.md) for its format profile,
rendering, animation and resource limits. Unsupported required extensions fail;
unsupported optional extensions appear in Properties diagnostics. Saving
`.n3.json` preserves source references and placement; editing asset contents,
converting them to authored meshes, and glTF/GLB export are not implemented.

File Open and startup replace the current document. File Import adds objects to
it. Dropping `.n3.json` opens that document; dropping OBJ/glTF/GLB imports into the
current document. OBJ geometry becomes editable, while imported scene placements
retain their linked sources. `--read-only` explicitly disables document editing
independently of input format.

```sh
just run fixtures/gltf/WaterBottle/glTF-Binary/WaterBottle.glb
just cargo run --locked -- --check fixtures/gltf/SimpleSkin/glTF/SimpleSkin.gltf
```

The `--check` command validates and evaluates the default scene without creating
a window; it does not establish GPU appearance or full format conformance.

N3 documents use versioned JSON (`.n3.json`) with stable object and vertex IDs.
Canonical lengths are centimeters; fractional values remain valid. Procedural
geometry is preserved until an effective vertex edit or explicit conversion.
Returning exactly to the edit-session baseline restores its parameters.
GPU buffers, render triangles,
selection, navigation, open menus, and unfinished sessions are not document data.

OBJ is an import format. Its raw coordinate numbers are interpreted as
centimeters without guessing physical scale. Display framing normalizes derived
render coordinates without rewriting source geometry. Y-up is the default;
Z-up only changes presentation.

The importer preserves OBJ position indices and polygon boundaries without
welding coincident vertices. Nonempty object/group sections become objects;
shared positions across sections appear in each object's mesh. Positive and
negative indices are supported. Concave polygons are triangulated for display.
Warped faces use projected triangulation with a notice; degenerate or crossing
projected boundaries are rejected.

Materials, textures, UVs, authored normals, standalone lines/points, and
free-form geometry are not retained. Normals are generated for display. Missing
material files do not block supported mesh import. OBJ export and lossless OBJ
roundtripping remain unimplemented.

Import limits: 64 MiB input, 1,000,000 coordinate records per kind, 2,000,000 total
face corners, 4,096 corners per face, 10,000 object/group records, and a polygon
work budget of 50 million (sum of squared polygon corner counts). These limits
are safety bounds, not a large-scene performance claim.

## Changing the editor

Keep canonical data, preview sessions, committed history, derived render data,
and UI state distinct. The current Editor still uses egui for picking and
painting; do not treat it as a ready external kernel API. A UI or keyboard
shortcut should dispatch an existing semantic action when applicable.

Add behavioral assertions at the appropriate core/input boundary and update the
owning guide scenario for visible interactions. Use production input events for
what a tutorial illustrates; direct state setup may prepare a fixture but must
not stand in for the behavior being demonstrated. Keep experiments and speculative
UX proposals separate from implemented contracts.

Promotion is a source-layout milestone. It does not imply publication, release,
performance qualification, full OBJ support, or final interaction acceptance.
