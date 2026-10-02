# Keep argument values out of shell source: recipes forward quoted positional args.

set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments := true

root := justfile_directory()
export N3_ROOT := root
export N3_INVOCATION_DIR := invocation_directory()
export CARGO_HOME := env("CARGO_HOME", root / ".cache/cargo")
export CARGO_TARGET_DIR := env("CARGO_TARGET_DIR", root / "target")

# List available commands.
default:
    @just --list

# Install contributor tools and activate the repository's Git hook.
setup: tools-install hooks-install

# Cache the pinned formatter through npx without package lifecycle scripts.
tools-install:
    python3 tools/format_docs.py --prepare

# Use repository-owned hooks through local Git configuration.
hooks-install:
    python3 tools/git_hooks.py install

# Build and locally sign build/N3.app without launching it.
build:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --locked
    bundle="$N3_ROOT/build/N3.app"
    mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources/licenses" "$N3_ROOT/.cache/review"
    cp "$N3_ROOT/assets/logo/N3.icns" "$bundle/Contents/Resources/N3.icns"
    cp "$N3_ROOT/assets/fonts/inter/LICENSE.txt" "$bundle/Contents/Resources/licenses/Inter.txt"
    cp "$N3_ROOT/assets/fonts/lucide/LICENSE" "$bundle/Contents/Resources/licenses/Lucide.txt"
    cp "$CARGO_TARGET_DIR/debug/n3" "$bundle/Contents/MacOS/n3.next"
    mv -f "$bundle/Contents/MacOS/n3.next" "$bundle/Contents/MacOS/n3"
    cat > "$bundle/Contents/Info.plist" <<'PLIST'
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>n3</string>
    <key>CFBundleIdentifier</key><string>dev.n3.editor</string>
    <key>CFBundleName</key><string>N3</string>
    <key>CFBundleDisplayName</key><string>N3</string>
    <key>CFBundleIconFile</key><string>N3.icns</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>0.0.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
    </dict></plist>
    PLIST
    # Ad-hoc signing is local only; this is not a distributable release.
    codesign --force --sign - "$bundle"
    # The bundle is rebuilt in place; refresh its directory timestamp so
    # Finder and Launch Services notice changes to the icon and metadata.
    touch "$bundle"

# Run an occasional asset/tooling task (currently: icon).
tools task:
    #!/usr/bin/env bash
    set -euo pipefail
    case "$1" in
        icon) swift "$N3_ROOT/tools/macos_icon.swift" "$N3_ROOT/assets/logo/n3-logo-01-crisp.svg" "$N3_ROOT/assets/logo/N3.icns" ;;
        *) echo 'Usage: just tools icon' >&2; exit 2 ;;
    esac

# Launch with the sample, a model path, or --empty. Paths are relative to your cwd.
run model="": build
    #!/usr/bin/env bash
    set -euo pipefail
    case "$1" in
        --empty) set -- ;;
        "") set -- "$N3_ROOT/fixtures/obj/bracket.obj" ;;
        --*) echo 'Usage: just run [MODEL | --empty]; use just build to build only' >&2; exit 2 ;;
        /*) ;;
        *) set -- "$N3_INVOCATION_DIR/$1" ;;
    esac
    if [[ $# -gt 0 && ! -f "$1" ]]; then
        printf 'Model does not exist: %s\n' "$1" >&2
        exit 2
    fi
    open -n "$N3_ROOT/build/N3.app" --stdout "$N3_ROOT/.cache/review/session.log" --stderr "$N3_ROOT/.cache/review/stderr.log" --args "$@"

# Run Cargo with the same repository-local dependency and build directories.
cargo +args:
    @cargo "$@"

# Check the application without building the native bundle.
check *args:
    cargo check --locked "$@"

# Install the optional browser target and the matching local WASM glue generator.
web-setup:
    python3 tools/web.py setup

# Check browser compilation without linking or generating the static site.
web-check:
    cargo check --locked --target wasm32-unknown-unknown --lib

# Lint the browser library with warnings denied, independently of native lint.
web-lint:
    cargo clippy --locked --target wasm32-unknown-unknown --lib -- -D warnings

# Check browser wrapper lifecycle and concurrency without building WASM.
web-test:
    node --test tools/tests/web_wrapper.test.mjs

# Build the shared editor as WASM and write a self-contained site to build/web.
web-build *args:
    python3 tools/web.py build "$@"

# Optional browser gate: lint Rust and generate runnable WASM/glue output.
web-verify: web-test web-lint web-build

# Build and serve the browser baseline locally (supports --port and --no-open).
web *args: web-build
    python3 tools/web.py serve "$@"

# Opt-in real-host viewport measurements; raw evidence stays in ignored output.
measure-viewport *args:
    python3 -m tools.benchmark.measure "$@"

# Repeat real host sessions and aggregate or compare their frame evidence.
benchmark-viewport *args:
    python3 -m tools.benchmark "$@"

# Install an isolated optional Playwright/Chromium runner for viewport measurements.
measure-browser-setup:
    python3 -m tools.benchmark.measure setup-browser

# Verify benchmark collection, reporting, browser policy, and Rust recorder contracts.
# This needs no browser installation and records no performance baseline.
benchmark-tools-test:
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/benchmark/tests -t . -p 'test_*.py'
    node --test tools/benchmark/tests/*.test.mjs

# Exercise feature-gated recorder contracts and exact renderer composition parity.
benchmark-rust-test:
    just cargo test --locked --features viewport-measure --lib measurement::
    just cargo test --locked --features viewport-measure --lib scene_only_presentation_matches_egui_pixels_and_restores_texture_after_resize

# Compile and lint the opt-in browser recorder without launching a browser.
benchmark-web-check:
    cargo clippy --locked --target wasm32-unknown-unknown --features viewport-measure --lib -- -D warnings

# Complete benchmark correctness gate; does not collect timings or install Chromium.
benchmark-test: benchmark-tools-test benchmark-rust-test

# Browse production UI components, or capture their deterministic internal evidence.
workbench *args:
    cargo run --locked -- --workbench "$@"

# Format Rust and authored documentation/configuration.
fmt: fmt-rust fmt-docs

# Format Rust source.
fmt-rust:
    cargo fmt

# Check all supported formatting without changing files.
fmt-check: fmt-rust-check fmt-docs-check

fmt-rust-check:
    cargo fmt --check

# Format authored sources; generated guides remain owned by their pipeline.
fmt-docs:
    python3 tools/format_docs.py --write

fmt-docs-check:
    python3 tools/format_docs.py --check

# Check all targets with warnings denied.
lint:
    cargo clippy --locked --all-targets -- -D warnings

# Run the complete suite, including exact docs replay, on the native host.
test *args:
    cargo test --locked "$@"

# Open the local guide preview, check it, or intentionally regenerate it.
docs mode="serve" *args:
    #!/usr/bin/env bash
    set -euo pipefail
    case "$1" in
        serve) shift; exec python3 "$N3_ROOT/tools/docs_preview.py" "$@" ;;
        check|update)
            if [[ $# -ne 1 ]]; then
                echo 'check/update do not accept extra arguments' >&2
                exit 2
            fi
            ;;
        *) echo 'Usage: just docs [serve [--no-open] [--port PORT] | check | update]' >&2; exit 2 ;;
    esac
    exec cargo run --locked -- --docs "$1"

# Test the local preview server without a browser or GPU.
docs-preview-test:
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -p 'test_docs_preview.py'

# Exercise Git hook behavior in temporary repositories without pushing anything.
hooks-test:
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -p 'test_git_hooks.py'

# Check the opt-in CI runner's filesystem and command boundaries.
runner-test:
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -p 'test_ci_runner.py'

# Run all development tooling regressions on the native host.
tools-test: web-test benchmark-tools-test
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -p 'test_*.py'

# Opt in to the pinned Ubuntu CI environment; requires Docker.
ci *args:
    python3 tools/ci_runner.py ci "$@"

# Explicit Ubuntu CI documentation replay/update; requires Docker.
ci-docs mode="check":
    python3 tools/ci_runner.py docs "$1"

# Explicit Ubuntu CI test run; native `just test` remains the default.
ci-test *args:
    python3 tools/ci_runner.py test "$@"

# The macOS lane owns bundle/signing and native integration, not general checks.
ci-macos: build
    codesign --verify --deep --strict "$N3_ROOT/build/N3.app"
    plutil -lint "$N3_ROOT/build/N3.app/Contents/Info.plist"
    test "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' "$N3_ROOT/build/N3.app/Contents/Info.plist")" = N3.icns
    cmp "$N3_ROOT/assets/logo/N3.icns" "$N3_ROOT/build/N3.app/Contents/Resources/N3.icns"
    swift "$N3_ROOT/tools/check_macos_bundle_icon.swift" "$N3_ROOT/build/N3.app"
    cargo test --locked native::

# Focused real-Metal regressions for development on a Mac with an available GPU.
test-metal:
    cargo test --locked documentation::capture::tests::

# Native development and pre-push checks. CI infrastructure is opt-in.
verify: fmt-check lint tools-test test benchmark-rust-test
