# Developing N3

The application is a single Rust package named `n3`, with private implementation
modules. Read [AGENTS.md](../AGENTS.md) for development principles and iteration
guidance. Start at [the architecture map](architecture/architecture.md) and
[the milestone review](architecture/milestone-review.md). The old viewer-only
spike has been removed; the root package is the maintained source tree.

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
for checks; only `ci-docs update` can write the guide. No generated app bundle,
cache, or review capture belongs in Git.

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

The current guide baseline uses native macOS Metal. Local development is verified
against that baseline. Ubuntu CI still needs a separately validated Linux media
baseline strategy before the first push: its lavapipe pixels must not be compared
as if they were Metal pixels. The explicit CI commands currently fail on this
renderer mismatch; they do not skip media checks or silently regenerate anything.
Resolving that CI gate must not become a Docker requirement for local iteration.

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

`just verify` runs Oxfmt and rustfmt checks, Clippy with warnings denied, all
Python tooling tests, and all Rust tests on the host. The Rust suite includes
required documentation replay and exact artifact comparisons. `just test`
forwards optional Cargo test arguments; `just tools-test` runs the Python suite.
Checks never update the guide, including when a renderer or driver changes.

The optional [CI image](../tools/ci/Dockerfile) fixes Ubuntu 24.04 on
`linux/amd64`, a 2026-09-28 APT snapshot, and Mesa lavapipe software Vulkan with
`sse2` CPU capabilities. It requests `N3_DOCS_RENDERER=lavapipe` explicitly;
normal developer commands select the native backend without that setting.
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
`src/documentation/scenarios/` and one Markdown template in `docs/templates/`.
Registration lives in `src/documentation/features.rs`. User-facing state and
behavior come from the real workspace, semantic commands, native-navigation
adapter, and scene renderer. Assertions verify the intended outcome.

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
in another terminal after changing a scenario or template, then refresh the
browser. `just docs update` regenerates through the native renderer;
`just docs check` verifies there without writing. Neither requires Docker.
The small preview uses a vendored Docsify runtime, so it needs no network access,
Node installation, or package-manager setup. It is a local reading surface, not
a choice of framework for the eventual documentation website.

Review changed text, stills, and animated clips before accepting regenerated
outputs. `check` does not rewrite or approve anything. The normal test suite
replays all scenarios and checks the complete generated tree, including animation
bytes and the manifest. Missing, orphaned, duplicate, and unreferenced artifacts
fail. A generation failure writes no new baseline.

Templates render application shortcuts with `{{shortcut:semantic-id}}` as
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

## User settings

Global preferences live at `~/Library/Application Support/N3/settings.json` on
macOS. Preferences can open that file in a text editor. See the
[user guide](guide/settings.md) for keys and recovery, and the
[settings boundary](architecture/user-settings.md) before changing persistence.
Settings tests and guide replay use isolated stores; they must never load or
modify the current user's file.

## Documents and imports

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
