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

For host-dependent drift, the CI runner reports host and container architecture,
glibc, CPU feature flags and Mesa's effective capabilities. Two diagnostic-only
CPU profiles can compare imported scene inputs, baked lighting and color transfer:

```sh
N3_CI_PROBE_CPU_CAPS=sse2 python3 tools/ci_runner.py test imported_render_fingerprints -- --nocapture
N3_CI_PROBE_CPU_CAPS=nosse python3 tools/ci_runner.py test imported_render_fingerprints -- --nocapture
```

The override accepts only that exact test command; full CI, guide commands and
other tests retain SSE2. `N3_CI_FAILURE_ARTIFACTS=1` additionally exports a failed
guide check's generated output to ignored
`.cache/ci/linux-amd64/home/docs-failure-artifacts/`, clearing prior diagnostic
output before the run. Export preserves the original failure and cannot update
the canonical guide or receipt. Review and paired host evidence are required
before changing a renderer baseline.

See the [pipeline contract](../architecture/documentation-pipeline.md) for
ownership and the [development guide](../development.md#verification-and-git-hooks)
for the native and container verification boundaries.
