# glTF integration fixtures

A small, unmodified subset of the official
[Khronos glTF Sample Assets](https://github.com/KhronosGroup/glTF-Sample-Assets),
pinned to commit
[`f36bfdabd1031c3cf6689a50570b8cdf3678b49c`](https://github.com/KhronosGroup/glTF-Sample-Assets/tree/f36bfdabd1031c3cf6689a50570b8cdf3678b49c)
and downloaded on September 30, 2026. These complement tiny N3-authored fixtures;
their presence does not itself establish viewer support.

| Fixture and entry point                                              | Purpose                                                                                                              | Source author      |
| -------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- | ------------------ |
| [Water Bottle](WaterBottle/glTF-Binary/WaterBottle.glb)              | Static GLB; metallic/roughness material, four embedded texture images, normals, tangents, and UVs                    | Microsoft, 2017    |
| [Simple Skin](SimpleSkin/glTF/SimpleSkin.gltf)                       | External glTF buffers; two-joint skin with animated rotation, weights and inverse bind matrices; no authored normals | Marco Hutter, 2017 |
| [Animated Morph Cube](AnimatedMorphCube/glTF/AnimatedMorphCube.gltf) | External glTF buffer; two morph targets and animated weights                                                         | Microsoft, 2017    |

All three use triangle primitives and require no extensions. Useful structural
expectations for integration tests:

| Fixture             | Nodes | Vertices | Triangles | Animation                            |
| ------------------- | ----: | -------: | --------: | ------------------------------------ |
| Water Bottle        |     1 |    2,549 |     4,510 | None                                 |
| Simple Skin         |     3 |       10 |         8 | One clip targeting joint rotation    |
| Animated Morph Cube |     1 |       24 |        12 | One clip targeting two morph weights |

## Integrity and licensing

[`provenance.json`](provenance.json) records the upstream path, exact byte size,
and SHA-256 of every copied file, including the original license and metadata.
Model data is **CC0-1.0**. The upstream license and metadocumentation are
**CC-BY-4.0**, attributed to the Khronos glTF Sample Assets contributors and the
named model authors. Read each model's original terms:

- [Water Bottle license](WaterBottle/LICENSE.md) and [metadata](WaterBottle/metadata.json)
- [Simple Skin license](SimpleSkin/LICENSE.md) and [metadata](SimpleSkin/metadata.json)
- [Animated Morph Cube license](AnimatedMorphCube/LICENSE.md) and [metadata](AnimatedMorphCube/metadata.json)

The upstream terms exclude logos and associated trademarks. N3 does not relicense
these assets under the application's source-code license. These files remain
byte-identical to upstream; the subset omits upstream screenshots, alternate
exports, and generated README files. Relative links in preserved metadata may
therefore refer to omitted upstream material. Use the pinned source above when
consulting the full model directory.

Keep external buffers beside their glTF entry points. Do not flatten directories,
optimize models, rewrite metadata, or format upstream files in place. A changed
fixture needs a deliberate source/provenance update.

Initial acquisition checks verified every recorded digest, GLB header and chunk
lengths, JSON decoding, declared external buffer availability/length, and the
structural counts above.

## Official validator evidence

On September 30, 2026, all three fixtures were checked with Khronos's official
[`gltf-validator` npm package](https://github.com/KhronosGroup/glTF-Validator/tree/main/node),
pinned to **2.0.0-dev.3.10**, using Node.js **24.14.0**. All external resources
were loaded and checked. No issues were suppressed, no severities were changed,
and reports were not truncated.

| Fixture             | Errors | Warnings | Information | Hints |
| ------------------- | -----: | -------: | ----------: | ----: |
| Water Bottle        |      0 |        0 |           0 |     5 |
| Simple Skin         |      0 |        0 |           0 |     1 |
| Animated Morph Cube |      0 |        0 |           1 |    10 |

Every hint is `BUFFER_VIEW_TARGET_MISSING`: upstream omits the optional vertex/
index buffer target hint. Animated Morph Cube also reports `UNUSED_MESH_TANGENT`
because its material has no normal map. These upstream properties remain intact.

The package was downloaded with `npm pack gltf-validator@2.0.0-dev.3.10
--ignore-scripts` into an ignored local tool cache; it is not a project dependency.
Its archive SHA-256 is
`4e03dbdc3bc0d1342afd5d2d7ae341a7e3474502ccb872b02dbbaddaee0fdec6`,
and its npm SHA-512 integrity was also verified before execution. The run used
the official `validateBytes` API with `maxIssues: 0`, `writeTimestamp: false`,
explicit `gltf`/`glb` format, and an `externalResourceFunction` resolving files
relative to each fixture's entry point. The local review script and complete
reports are in ignored `.cache/review/gltf-validator/`; the table above is the
maintained result for the source revision and hashes in `provenance.json`.

Format validation does not prove N3's rendered output. Importer/evaluator tests,
the executable scene-viewer guide, and visual review supply separate evidence.
