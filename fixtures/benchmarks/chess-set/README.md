# Chess Set benchmark fixture

[Chess Set by Riley Queen](https://polyhaven.com/a/chess_set), distributed by
Poly Haven under [CC0-1.0](LICENSE.md), is N3's first external benchmark scene.
The recognizable board and pieces make missing geometry and placement mistakes
easy to spot. The bounded 1K export supplies overlapping geometry and multiple
materials without a large texture download.

Use [`chess-set-1k.glb`](chess-set-1k.glb) for both native and browser runs.
It is a self-contained derivative of the unchanged upstream
[`source/chess_set_1k.gltf`](source/chess_set_1k.gltf) and its referenced resources.
The browser's single-file input does not resolve sibling files from the source
glTF bundle.

| Property                             | Value                                                                                                                  |
| ------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| Source bundle                        | 6,983,829 bytes across 11 files (6.66 MiB)                                                                             |
| Packed GLB                           | 6,962,072 bytes (6.64 MiB)                                                                                             |
| Scene                                | One scene; 33 mesh nodes: board and 32 pieces                                                                          |
| Geometry                             | 49,150 vertices across primitives; 76,920 triangles; 37 primitives                                                     |
| Materials                            | Three metallic/roughness materials, all double-sided                                                                   |
| Images                               | Nine 1024 × 1024 JPEGs: base color, normal, and packed ARM for each material                                           |
| Decoded image storage                | 37,748,736 bytes (36 MiB) of base-level RGBA8; excludes mipmaps, retained encoded bytes, and other CPU/GPU allocations |
| Extensions, animation, skins, morphs | None                                                                                                                   |

These are imported scene counts, not authored editable topology. N3 places this
as one asset instance with 33 internal mesh nodes. Repeating instances is a
separate workload and must state whether resources are shared. The original ARM
images have no `occlusionTexture` references; this fixture preserves that choice.

## Integrity and reproduction

[`provenance.json`](provenance.json) pins source URLs, sizes, upstream MD5 values,
and SHA-256 digests, plus the derived GLB's digest. Downloaded October 2, 2026.
The upstream URLs are mutable, so the recorded hashes identify the exact snapshot.
The geometry download URL contains `8k`; it is the shared geometry buffer used by
the selected 1K export, not an 8K texture download.

Keep `source/` byte-identical to upstream. [`pack.py`](pack.py) uses only Python's
standard library and first verifies every source digest. It embeds the existing
geometry and JPEG bytes, adds aligned image buffer views, and serializes a GLB.
It does not optimize, merge, reorder, resize, recompress, or change scene semantics.
The source bundle and derivative together occupy about 13.3 MiB.

From the repository root:

```sh
# Read-only: verify source hashes and exact GLB reproducibility.
python3 fixtures/benchmarks/chess-set/pack.py

# Explicitly regenerate the same pinned derivative.
python3 fixtures/benchmarks/chess-set/pack.py --write
```

Changing the asset or recipe requires a deliberate provenance update; the recipe
fails if its output differs from the recorded derivative. Source resources are
excluded from repository formatting.

## Validation and workload

On October 2, 2026, the official Khronos `gltf-validator` **2.0.0-dev.3.10**, running
on Node.js **24.14.0**, checked both the external glTF and packed GLB with all
resources loaded, no issue suppression, and untruncated reports. Both returned
**zero errors, 37 warnings, two information messages, and zero hints**:

- Each primitive omits tangents while using a normal map. The 37
  `MESH_PRIMITIVE_GENERATED_TANGENT_SPACE` warnings mean consumers must generate
  tangent space; visual differences across implementations remain possible.
- Two `UNUSED_OBJECT` messages identify unused secondary UV attributes.

These upstream properties are preserved, not repaired during packing. Full
acquisition and validator receipts live in ignored `.cache/review/chess-set/`.
The importer regression compares the external-file and embedded-byte paths,
including evaluated geometry, materials, and decoded images.

```sh
just measure-viewport --host native --profile release \
  --mode editor \
  --input fixtures/benchmarks/chess-set/chess-set-1k.glb \
  --output .cache/measure/chess-native-editor-orbit-01.json

just measure-viewport --host web --profile release \
  --mode editor \
  --input fixtures/benchmarks/chess-set/chess-set-1k.glb \
  --output .cache/measure/chess-web-editor-orbit-01.json
```

Repeat with `--mode viewport` and `--mode renderer`, using fresh mode-specific
output names, to distinguish editor and UI work from the production scene renderer.
Every mode preserves the full editor layout's scene rectangle; renderer mode
excludes editor projection and feedback. Only editor and viewport modes accept
`--selected`.

The current harness measures **Solid** orbit or stationary frames. Importing and
retaining textures does not establish Material Preview/PBR performance. An
explicit Material Preview workload remains separate work. Likewise this asset
does not cover dense authored mesh editing. See the
[measurement guide](../../../docs/architecture/viewport-measurement.md) for
browser setup, report validity, repeatability, and remaining instrumentation limits.
