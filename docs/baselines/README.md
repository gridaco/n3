# Secondary documentation renderer baselines

The published user guide in `docs/guide/` uses macOS Metal. Ubuntu CI replays the
same scenarios with the pinned lavapipe renderer and verifies its output against
`linux-vulkan-lavapipe.json`, preserving the published guide.

The versioned receipt records each artifact's path, byte length and SHA-256,
including Markdown, the evidence manifest, stills and animations. Its canonical
guide digest frames the complete sorted tree's paths, lengths and bytes, so a
change to any published artifact makes the Linux receipt stale. The generated
Markdown and artifact inventory must also match the published guide exactly.

After an intentional native guide update and visual review:

```sh
just ci-docs update
just ci-docs check
```

The update writes Linux review output to ignored
`.cache/docs/linux-vulkan-lavapipe/` and this receipt. Inspect the generated text,
stills and animation playback before accepting it. Ordinary checks never write
artifacts, skip captures or apply visual tolerances. Renderer-specific hashes establish
exact reproducibility within each profile, not equality between Metal and Linux.

The pinned Linux renderer uses Mesa's `sse2` CPU capability profile, 256-bit
vectors and a disabled shader cache. Mesa's
[sRGB attachment conversion](https://gitlab.freedesktop.org/mesa/mesa/-/blob/mesa-25.2.8/src/gallium/auxiliary/gallivm/lp_bld_format_srgb.c#L245)
uses an [approximate SSE reciprocal-square-root path](https://gitlab.freedesktop.org/mesa/mesa/-/blob/mesa-25.2.8/src/gallium/auxiliary/gallivm/lp_bld_arit.c#L2641), which
produces different bytes on native x86 and emulated x86 hosts. Controlled probes
with identical uploaded inputs reproduced this difference, while CPU scene and
lighting inputs and explicit shader color transfer matched. With AVX masked by
the SSE2 profile, the eight-float conversion uses the ordinary sqrt fallback.
Mesa 25.2.8's [native-code cache key](https://gitlab.freedesktop.org/mesa/mesa/-/blob/mesa-25.2.8/src/gallium/drivers/llvmpipe/lp_screen.c#L825)
omits the vector-width override, so the cache must be disabled to prevent reuse
of a prior 128-bit shader.

The `nosse` diagnostic profile also removed the cross-host color difference,
but a complete 1920×1080 frame check exceeded its 45-second bound locally. The
selected wide SSE2 profile completed the same check in 2.01 seconds. These are
local measurements; full guide replay and exact artifact checks remain required.
This affects only the opt-in CI software renderer. Application shaders and native
rendering keep their existing behavior.

For host-dependent drift, the CI runner reports host and container architecture,
glibc, CPU feature flags and Mesa's effective capabilities. The default probe
exercises the production 256-bit profile; the two explicit overrides reproduce
historical 128-bit profiles, also with the shader cache disabled:

```sh
python3 tools/ci_runner.py test imported_render_fingerprints -- --nocapture
N3_CI_PROBE_CPU_CAPS=sse2 python3 tools/ci_runner.py test imported_render_fingerprints -- --nocapture
N3_CI_PROBE_CPU_CAPS=nosse python3 tools/ci_runner.py test imported_render_fingerprints -- --nocapture
```

RGB and alpha digests are separate because format packing may round an exact
alpha midpoint differently. The override accepts only that exact test command;
full CI, guide commands and other tests retain the wide SSE2 default.
`N3_CI_FAILURE_ARTIFACTS=1` additionally exports a failed
guide check's generated output to ignored
`.cache/ci/linux-amd64/home/docs-failure-artifacts/`, clearing prior diagnostic
output before the run. Export preserves the original failure and cannot update
the canonical guide or receipt. Review and paired host evidence are required
before changing a renderer baseline.

See the [pipeline contract](../architecture/documentation-pipeline.md) for
ownership and the [development guide](../development.md#verification-and-git-hooks)
for the native and container verification boundaries.
