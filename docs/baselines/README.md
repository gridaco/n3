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
stills and animation playback before accepting it. Checks never write artifacts,
skip captures or apply visual tolerances. Renderer-specific hashes establish
exact reproducibility within each profile, not equality between Metal and Linux.

See the [pipeline contract](../architecture/documentation-pipeline.md) for
ownership and the [development guide](../development.md#verification-and-git-hooks)
for the native and container verification boundaries.
