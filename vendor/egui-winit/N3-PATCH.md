# Local egui-winit compatibility patch

Source: `egui-winit` 0.36.2 from crates.io, repository commit
`49682f8baa058bf49e011035cfbd6e825f88a5ef`, path `crates/egui-winit` in
[emilk/egui](https://github.com/emilk/egui).
Upstream source and package metadata are retained. The upstream Apache-2.0 and
MIT license texts are included alongside them.

The published adapter implements `egui::DroppedFile::bytes`, while egui 0.36.2
requires `bytes_async` on `wasm32`. This prevents the unmodified adapter from
compiling for browsers even without its native clipboard feature.

The change in `src/dropped_file.rs` retains the existing filesystem method on
native targets and implements the browser trait method with an explicit
unsupported-path error. A native path has no browser `File` handle; N3's browser
wrapper supplies selected/dropped bytes through its own file adapter. This does
not synthesize bytes or touch browser storage.

`src/lib.rs` translates Command modifiers using egui's runtime operating system
on `wasm32`. Browser builds have no compile-time macOS target, even on a Mac;
N3 sets the context's OS before constructing the adapter. Native builds retain
the original compile-time macOS decision. This preserves Command on Mac browsers
and Control on Windows/Linux browsers without introducing new bindings.

The normalized manifest's two license include paths point at the local copies.
`Cargo.toml.orig` and `.cargo_vcs_info.json` preserve upstream provenance. The
published package's development lockfile is omitted; N3's root `Cargo.lock`
remains authoritative. Remove the Cargo patch and this directory when an upstream
release supports the current browser trait contract and passes N3's checks.
