# Geometry fixtures

The four small geometric OBJ fixtures were authored directly for n3 on
September 28, 2026 and use the repository's MIT license. Suzanne is a downloaded
reference mesh with separate upstream attribution below. No textures or material
libraries are included.

The files are canonical, trackable repository fixtures. N3's application source
is maintained under `src/`; see the [opening-models guide](../docs/guide/opening-models.md)
for importing these fixtures. Dependency caches, builds, application bundles,
and local review output remain ignored under `.cache/`, `target/`, and `build/`.
Generated user-guide media lives in `docs/guide/` and is checked with the source.
The fixtures exercise importing and display; they do not define the editor's
canonical document format.

| File                       | Positions |              Polygon faces | Expected triangles | Unique polygon edges | Source extent X/Y/Z            |
| -------------------------- | --------: | -------------------------: | -----------------: | -------------------: | ------------------------------ |
| `obj/cube-quads.obj`       |         8 |                    6 quads |                 12 |                   12 | 2 / 2 / 2                      |
| `obj/bracket.obj`          |        12 | 6 quads + 2 concave n-gons |                 20 |                   18 | 3 / 3 / 1                      |
| `obj/concave-ngon.obj`     |         6 |            1 concave n-gon |                  4 |                    6 | 2 / 2 / 0                      |
| `obj/negative-indices.obj` |         4 |                     1 quad |                  2 |                    4 | 2 / 2 / 0                      |
| `obj/suzanne.obj`          |     2,012 |            3,936 triangles |              3,936 |                5,946 | 2.673828 / 1.924805 / 1.626465 |

Each file contains one named object. Closed solids use outward face winding.
The two flat fixtures face +Z. The negative-index fixture includes a non-unit
authored normal to exercise normalization; the other original fixtures omit
normals to exercise flat face normals. Polygon-edge overlays retain authored edges
without exposing triangulation diagonals.

The viewer centers loaded geometry and scales its largest extent to 2.0 for
display, preserving the original extents as statistics. Position counts refer to
authored OBJ `v` records; display buffers duplicate vertices as needed. The
viewer accepts supported simple polygons, including concave faces, and rejects
detected crossing boundaries or degenerate geometry. Warped polygons are
triangulated by projection with a warning that their rendered surface may differ
from the exporting application. They are no longer rejected solely for being
non-planar. OBJ materials/textures, standalone lines, and standalone points are
ignored with notices.

Loading a fixture replaces the displayed model only after a successful load.
These expected counts and behaviors describe the fixtures and viewer contract;
native visual and interaction validation is ongoing.

## Suzanne provenance

[`obj/suzanne.obj`](obj/suzanne.obj) is an unchanged copy of
[Kivy's `examples/3Drendering/monkey.obj`](https://github.com/kivy/kivy/blob/9bd35c595760fe6909e6889b9aefe659a57f9eae/examples/3Drendering/monkey.obj),
downloaded September 28, 2026. Its header identifies a Blender 2.65 export and
names the object Suzanne. This is Kivy's triangulated export, not Blender's
original primitive topology, and serves as a reference fixture rather than the
n3 mascot.

- Upstream revision: `9bd35c595760fe6909e6889b9aefe659a57f9eae`.
- Size: 248,311 bytes.
- SHA-256: `a8539c2ce2cea7dfd31f3d3c20b16997dde2a959b0683d89d1406bea79653171`.
- Kivy repository license: MIT, copyright 2010–2025 Kivy Team and other
  contributors. The [upstream license](licenses/kivy-MIT.txt) is retained from
  the same revision; the OBJ header contains no separate asset license.

The mesh includes authored normals and a `monkey.mtl` reference. That material
library is not included: the viewer loads the geometry and reports its usual
materials-ignored notice. The source OBJ is preserved without stripping that
reference or changing its coordinates.

Open it in N3 from the repository root, or drag the file onto its viewport:

```sh
just run fixtures/obj/suzanne.obj
```

## glTF scene fixtures

[glTF fixtures](gltf/README.md) cover embedded PBR textures, external-resource
skinning, and morph animation through the read-only scene viewer. Their upstream
revision, licenses, and exact bytes are recorded separately from the OBJ fixtures.

## Benchmark scenes

[Chess Set](benchmarks/chess-set/README.md) is a bounded CC0 Poly Haven scene for
native/browser viewport measurements. It includes the unchanged 1K source bundle,
a reproducibly packed GLB, and source/derivative checksums. Benchmark workload and
validation limits are documented alongside the fixture.
