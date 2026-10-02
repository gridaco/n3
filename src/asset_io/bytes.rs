//! Browser file snapshots enter through the same format adapters as native I/O.
//! No filesystem access, network fetch, or implicit resource discovery occurs.
use std::{collections::BTreeSet, path::Path, sync::Arc};

use sha2::{Digest, Sha256};

use super::{LoadedDocument, ResourceResolver};
use crate::document::{AssetInstance, Document, Geometry, Object, Transform};

struct EmbeddedResources;

impl ResourceResolver for EmbeddedResources {
    fn read(&self, relative_path: &str, _maximum_bytes: usize) -> Result<Vec<u8>, String> {
        Err(format!(
            "External resource {relative_path:?} is unavailable. Open a self-contained GLB or glTF with embedded resources in the browser."
        ))
    }
}

/// Decode one browser-selected file. Native JSON references remain recoverable
/// authored objects, but resolving a local package requires a future host API.
/// The caller installs or imports this candidate through the normal editor path.
pub(crate) fn load_bytes(name: &str, bytes: &[u8]) -> Result<LoadedDocument, String> {
    if bytes.len() as u64 > crate::document::MAX_BYTES {
        return Err("File exceeds the 64 MiB input limit".into());
    }
    let path = Path::new(name);
    if !super::is_supported_path(path) {
        return Err("Choose an N3 JSON, OBJ, glTF, or GLB file.".into());
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("The selected file needs a name.")?;
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("Imported object");
    if super::is_scene_path(path) {
        let asset = Arc::new(super::gltf::load(filename, bytes, &EmbeddedResources)?);
        // Filenames are not unique browser resources. Content identity prevents
        // a second selection named "model.glb" from replacing retained Undo data.
        // This is a reference only; .n3.json still never embeds imported payloads.
        let source = format!("browser-assets/{:x}/{filename}", Sha256::digest(bytes));
        let document = Document {
            objects: vec![Object {
                id: 1,
                name: stem.into(),
                transform: Transform::default(),
                geometry: Geometry::Asset(AssetInstance {
                    source: source.clone(),
                    scene: asset.default_scene,
                }),
            }],
            ..Document::default()
        };
        document.validate()?;
        let loaded = LoadedDocument {
            document,
            assets: [(source, asset)].into(),
            diagnostics: Vec::new(),
            saved_bytes: None,
        };
        super::validate_resource_cache(&loaded.assets)?;
        return Ok(loaded);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|error| format!("The selected text file is not valid UTF-8: {error}"))?;
    let is_obj = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"));
    let document = if is_obj {
        super::obj::parse_obj(text, stem)?
    } else {
        Document::from_json(text)?
    };
    let missing: BTreeSet<_> = document
        .objects
        .iter()
        .filter_map(|object| match &object.geometry {
            Geometry::Asset(instance) => Some(&instance.source),
            _ => None,
        })
        .collect();
    let diagnostics = missing
        .into_iter()
        .map(|source| {
            format!(
                "Linked asset {source:?} was retained but cannot be resolved from one browser file. Its source data is not embedded in N3 JSON."
            )
        })
        .collect();
    Ok(LoadedDocument {
        document,
        assets: Default::default(),
        diagnostics,
        saved_bytes: (!is_obj).then(|| bytes.to_vec()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_io::load_bytes;

    #[test]
    fn native_bytes_keep_recipes_exact_baseline_and_missing_links() {
        let mut document = Document::default();
        document
            .insert_primitive(crate::document::PrimitiveKind::Cube)
            .unwrap();
        document.objects.push(Object {
            id: 2,
            name: "Linked object".into(),
            transform: Transform::default(),
            geometry: Geometry::Asset(AssetInstance {
                source: "../assets/missing.glb".into(),
                scene: 3,
            }),
        });
        let bytes = format!("{}\n", document.to_json().unwrap()).into_bytes();
        let loaded = load_bytes("document.n3.json", &bytes).unwrap();
        assert_eq!(loaded.document, document);
        assert_eq!(loaded.saved_bytes, Some(bytes));
        assert!(loaded.assets.is_empty());
        assert_eq!(loaded.diagnostics.len(), 1);
        assert!(loaded.diagnostics[0].contains("../assets/missing.glb"));
    }

    #[test]
    fn obj_bytes_preserve_polygon_topology_and_document_units() {
        let loaded = load_bytes(
            "Plane.OBJ",
            b"v 0 0 0\nv 1.5 0 0\nv 1.5 2 0\nv 0 2 0\nf 1 2 3 4\n",
        )
        .unwrap();
        let Geometry::Mesh(mesh) = &loaded.document.objects[0].geometry else {
            panic!("OBJ must remain editable topology");
        };
        assert_eq!(loaded.document.objects[0].name, "Plane");
        assert_eq!(mesh.faces.len(), 1);
        assert_eq!(mesh.faces[0].vertices.len(), 4);
        assert_eq!(mesh.vertices[1].position, [1.5, 0.0, 0.0]);
        assert!(loaded.saved_bytes.is_none());
    }

    fn triangle_glb(width: f32) -> Vec<u8> {
        let positions = [[0.0f32, 0.0, 0.0], [width, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let binary: Vec<u8> = positions
            .iter()
            .flatten()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let mut json = serde_json::to_vec(&serde_json::json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": binary.len()}],
            "bufferViews": [{"buffer": 0, "byteLength": binary.len()}],
            "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3,
                "type": "VEC3", "min": [0, 0, 0], "max": [width, 1, 0]}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "nodes": [{"mesh": 0}],
            "scenes": [{"nodes": [0]}], "scene": 0
        }))
        .unwrap();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let length = 12 + 8 + json.len() + 8 + binary.len();
        let mut glb = Vec::new();
        for word in [
            0x46546c67u32,
            2,
            length as u32,
            json.len() as u32,
            0x4e4f534a,
        ] {
            glb.extend_from_slice(&word.to_le_bytes());
        }
        glb.extend(json);
        glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004e4942u32.to_le_bytes());
        glb.extend(binary);
        glb
    }

    #[test]
    fn glb_snapshots_decode_and_same_named_imports_have_distinct_identity() {
        let bytes = triangle_glb(1.0);
        let first = load_bytes("model.glb", &bytes).unwrap();
        let again = load_bytes("model.glb", &bytes).unwrap();
        let changed = load_bytes("model.glb", &triangle_glb(2.0)).unwrap();
        let first_key = first.assets.keys().next().unwrap();
        assert_eq!(first_key, again.assets.keys().next().unwrap());
        assert_ne!(first_key, changed.assets.keys().next().unwrap());
        assert!(first.saved_bytes.is_none());
        assert!(first.diagnostics.is_empty());
        let asset = &first.assets[first_key];
        assert_eq!(
            asset.meshes[0].primitives[0].vertices[1].position,
            [100.0, 0.0, 0.0]
        );
        assert!(asset.evaluate(asset.default_scene, None).is_ok());
    }

    #[test]
    fn external_gltf_resources_and_invalid_text_fail_without_host_access() {
        let gltf = br#"{"asset":{"version":"2.0"},"buffers":[{"uri":"data.bin","byteLength":4}],"scenes":[{"nodes":[]}],"scene":0}"#;
        assert!(
            load_bytes("scene.gltf", gltf)
                .unwrap_err()
                .contains("External resource \"data.bin\" is unavailable")
        );
        assert!(
            load_bytes("bad.n3.json", &[0xff])
                .unwrap_err()
                .contains("UTF-8")
        );
        assert!(load_bytes("other.txt", b"{}").is_err());
    }
}
