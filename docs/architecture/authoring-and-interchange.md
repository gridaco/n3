# Authoring and interchange

N3 owns its authoring semantics. External formats enter through adapters; their
schemas do not define what an N3 object is or how editing works. Imported scenes
also have an N3-owned representation, but that representation serves viewing and
evaluation; it is not the future canonical authoring document.

## Three representations

| Representation                        | What it preserves                                                                                                     | What it owns                                                                         |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ |
| Authored document (`model::document`) | Primitive recipes, object transforms, stable identities, polygon boundaries, loose edges, and placed asset references | Editable intent and document validation; mutations pass through the editor lifecycle |
| Imported scene (`scene::SceneAsset`)  | The supported hierarchy, geometry, materials, cameras, lights, skins, morphs, and clips                               | Immutable viewing data in N3 units, independent of the source parser                 |
| Evaluated output                      | Geometry and scene state needed for a particular view or time                                                         | Derived results and caches; never a competing editable source                        |

A procedural cylinder illustrates the difference. Its radius, height, and segment
count describe something the user can still adjust. Evaluating it produces mesh
topology; rendering or delivering it to another application may produce triangles.
Those triangles do not retain the cylinder's recipe. Importing a triangulated
cylinder cannot silently recover the original parameters or their intended
relationships. This is a representation boundary, not a commitment to a particular
future procedural-operation system.

For current primitives, merely entering vertex edit mode retains the recipe. An
effective edit materializes a mesh through the ordinary transaction, and returning
to the edit-session baseline restores the recipe. See
[edit transactions](edit-transactions.md). Imported asset contents do not offer this editing lifecycle. Their placement
objects do: moving, rotating, scaling, duplicating, and deleting a linked instance
use the same editor transactions as native geometry, without changing the source
material, mesh, or animation data.

## Serialization is a separate concern

The authored `Document` is the semantic authority. Versioned, readable `.n3.json`
is its current canonical persistence contract. JSON syntax is not the reason that
polygon identity, units, or primitive behavior exist. Pure `from_json` / `to_json`
codecs remain alongside the document for now; file access and overwrite protection
belong to `asset_io::document`. This separation does not require a second schema
hierarchy or additional crates.

OBJ import maps supported source geometry into an authored document. glTF/GLB
loading maps supported source semantics into immutable scene resources and places
an `AssetInstance` in the authored document. The instance stores a source reference
and scene index; its object owns placement and stable identity. `.n3.json` saves
that reference, not an embedded scene package. Neither path defines a universal
interchange format for N3. Future export must state which authored information it evaluates,
preserves, or loses; a matching silhouette is not proof of lossless interchange.

Making an imported asset's internal geometry editable is a separate conversion feature. Pausing or
disabling playback does not establish editable topology: UV seams and hard normals
may split render vertices, several nodes may instance one mesh, and a displayed
pose may already include skinning or morphs. Conversion must choose connectivity,
instance ownership, attribute/material preservation, and a pose policy explicitly.
Those choices remain open. The unified editor enables asset placement without
claiming editable source topology or animation authoring.

## Adapter and evaluation boundary

`src/asset_io/` owns format dispatch, native document persistence, OBJ parsing,
resource resolution, and the `gltf/` adapter. Format extensions, accessor layouts,
URI rules, decoder types, and source-unit conversion stop here. Adapters reject
unsupported required semantics and report supported-profile limitations rather
than quietly claiming complete format support.

`src/scene/` owns the internal scene model, its validation, and explicit-time
evaluation. Spatial values arrive in centimeters. The scene model and evaluator
must not need glTF parser objects, file paths, resource URIs, or filesystem access.
Construction validates N3 invariants independently of any decoder. Merely copying
format structs into our own structs would not establish this boundary if their
validation, units, or behavior still depended on the parser.

The current runtime contract deliberately stays small: a node forest, render
primitives, transform and morph-weight curves, skins, and a metallic-roughness
material profile. Index handles identify resources within one immutable asset;
they are not persistent editable topology IDs. Curve values and Hermite tangents
have their own internal arrays rather than exposing an accessor's packed layout.
These are supported evaluation capabilities, not limits on the future authored
document. A future authoring graph can evaluate into this contract, or a concrete
feature can justify extending it and updating affected adapters.

Scene evaluation produces world-space results; render adapters handle display
normalization and GPU resources. Playback and exposure remain transient view
state, outside both source data and editor history.
Decoded scenes and evaluated frames are shared outside history; undo restores
placed references and transforms without copying their resource payloads.
The [scene viewer contract](scene-viewer.md) describes loading, recovery, caching,
and the currently supported rendering profile.

Owning these representations does not mean inventing new shading physics. N3
follows standard metallic-roughness PBR semantics within an explicit supported
profile. Adapters map standard material data to that profile; renderer limitations
remain documented. Supporting material playback or display does not commit N3 to
material authoring.

## Priorities, not a roadmap

The authoring foundation starts with N3's own mesh topology and editing semantics.
Animation authoring is a later concern. Material authoring is much further away
and may remain outside the product. These priorities do not specify delivery dates,
an operation graph, a future animation schema, or an eventual all-purpose document
model. Extend the authored model when a concrete editing capability needs it;
do not promote the imported scene model into that role by convenience.
