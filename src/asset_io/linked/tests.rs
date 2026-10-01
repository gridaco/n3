use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "n3-linked-test-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn triangle(directory: &Path, name: &str, width: f32) -> PathBuf {
    fs::create_dir_all(directory).unwrap();
    let positions = [0_f32, 0., 0., width, 0., 0., 0., 1., 0.];
    let bytes: Vec<_> = positions.into_iter().flat_map(f32::to_le_bytes).collect();
    fs::write(directory.join(format!("{name}.bin")), &bytes).unwrap();
    let json = serde_json::json!({
        "asset": {"version": "2.0"},
        "buffers": [{"uri": format!("{name}.bin"), "byteLength": bytes.len()}],
        "bufferViews": [{"buffer": 0, "byteLength": bytes.len()}],
        "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3,
            "type": "VEC3", "min": [0,0,0], "max": [width,1,0]}],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
        "nodes": [{"mesh": 0}], "scenes": [{"nodes": [0]}], "scene": 0
    });
    let path = directory.join(format!("{name}.gltf"));
    fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    path
}

fn reference(document: &Document) -> &AssetInstance {
    let Geometry::Asset(instance) = &document.objects[0].geometry else {
        panic!("expected an asset instance");
    };
    instance
}

fn width(asset: &SceneAsset) -> f64 {
    let bounds = asset
        .evaluate(asset.default_scene, None)
        .unwrap()
        .bounds
        .unwrap();
    bounds.max.x - bounds.min.x
}

#[test]
fn import_places_one_authored_object_and_keeps_source_resources_separate() {
    let scratch = Scratch::new();
    let path = triangle(&scratch.0.join("assets"), "Triangle", 1.);
    let original = fs::read(&path).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.document.objects.len(), 1);
    assert_eq!(loaded.document.objects[0].name, "Triangle");
    assert_eq!(loaded.document.objects[0].transform, Transform::default());
    let instance = reference(&loaded.document);
    assert!(Path::new(&instance.source).is_absolute());
    assert_eq!(instance.scene, 0);
    assert_eq!(loaded.assets.len(), 1);
    assert_eq!(width(&loaded.assets[&instance.source]), 100.);
    assert!(loaded.saved_bytes.is_none());
    assert!(loaded.diagnostics.is_empty());
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn save_and_save_as_rebase_only_the_written_snapshot() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0.join("project/assets"), "Triangle", 1.);
    let loaded = load(&source).unwrap();
    let original_document = loaded.document.clone();
    let destination = scratch.0.join("project/model.n3.json");
    let bytes = super::super::document::save(&destination, &loaded.document, None, false).unwrap();
    let serialized = Document::from_json(std::str::from_utf8(&bytes).unwrap()).unwrap();
    assert_eq!(reference(&serialized).source, "assets/Triangle.gltf");
    let reopened = load(&destination).unwrap();
    assert_eq!(reopened.document, original_document);
    assert_eq!(reopened.saved_bytes, Some(bytes.clone()));

    fs::create_dir(scratch.0.join("elsewhere")).unwrap();
    let save_as = scratch.0.join("elsewhere/copy.n3.json");
    let second = super::super::document::save(&save_as, &loaded.document, None, false).unwrap();
    let serialized = Document::from_json(std::str::from_utf8(&second).unwrap()).unwrap();
    assert_eq!(
        reference(&serialized).source,
        "../project/assets/Triangle.gltf"
    );
    assert_eq!(load(&save_as).unwrap().document, original_document);
    assert_eq!(loaded.document, original_document);
    assert_eq!(fs::read(destination).unwrap(), bytes);
}

#[test]
fn relocating_document_and_its_asset_package_preserves_relative_links() {
    let scratch = Scratch::new();
    let folder = scratch.0.join("original package");
    let source = triangle(&folder.join("resources"), "Triangle", 1.5);
    let loaded = load(&source).unwrap();
    let path = folder.join("model.n3.json");
    super::super::document::save(&path, &loaded.document, None, false).unwrap();
    let relocated = scratch.0.join("relocated package");
    fs::rename(&folder, &relocated).unwrap();
    let reopened = load(&relocated.join("model.n3.json")).unwrap();
    let instance = reference(&reopened.document);
    assert_eq!(
        Path::new(&instance.source),
        relocated.join("resources/Triangle.gltf")
    );
    assert_eq!(width(&reopened.assets[&instance.source]), 150.);
    assert!(reopened.diagnostics.is_empty());
}

#[test]
fn missing_dependency_keeps_the_authored_reference_and_remains_saveable() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0.join("assets"), "Triangle", 1.);
    let loaded = load(&source).unwrap();
    let path = scratch.0.join("model.n3.json");
    let saved = super::super::document::save(&path, &loaded.document, None, false).unwrap();
    fs::remove_file(source.with_extension("bin")).unwrap();
    let missing = load(&path).unwrap();
    assert_eq!(missing.document, loaded.document);
    assert!(missing.assets.is_empty());
    assert_eq!(missing.diagnostics.len(), 1);
    assert!(missing.diagnostics[0].contains("Cannot load linked asset"));
    let resaved = super::super::document::save(
        &path,
        &missing.document,
        missing.saved_bytes.as_deref(),
        false,
    )
    .unwrap();
    assert_eq!(resaved, saved);
    assert!(
        load(&source).is_err(),
        "A new broken import must still fail"
    );
}

#[test]
fn missing_source_and_missing_parent_are_retained_as_recoverable_links() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0.join("assets"), "Triangle", 1.);
    let mut document = load(&source).unwrap().document;
    let Geometry::Asset(instance) = &mut document.objects[0].geometry else {
        unreachable!()
    };
    instance.source = scratch
        .0
        .join("missing/folder/Triangle.gltf")
        .to_str()
        .unwrap()
        .into();
    let path = scratch.0.join("model.n3.json");
    super::super::document::save(&path, &document, None, false).unwrap();
    let missing = load(&path).unwrap();
    assert_eq!(missing.document, document);
    assert!(missing.assets.is_empty());
    assert_eq!(missing.diagnostics.len(), 1);
}

#[test]
fn load_is_a_snapshot_and_reopen_uses_current_source_bytes() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0.join("assets"), "Triangle", 1.);
    let loaded = load(&source).unwrap();
    let key = reference(&loaded.document).source.clone();
    let path = scratch.0.join("model.n3.json");
    super::super::document::save(&path, &loaded.document, None, false).unwrap();
    triangle(source.parent().unwrap(), "Triangle", 2.);
    let changed_bytes = fs::read(&source).unwrap();
    assert_eq!(width(&loaded.assets[&key]), 100.);
    let reopened = load(&path).unwrap();
    assert_eq!(width(&reopened.assets[&key]), 200.);
    assert_eq!(reopened.document, loaded.document);
    super::super::document::save(
        &path,
        &reopened.document,
        reopened.saved_bytes.as_deref(),
        false,
    )
    .unwrap();
    assert_eq!(fs::read(&source).unwrap(), changed_bytes);
}

#[test]
fn duplicate_references_share_one_source_and_invalid_scene_is_diagnostic() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0.join("assets"), "Triangle", 1.);
    let mut document = load(&source).unwrap().document;
    let mut second = document.objects[0].clone();
    second.id = 2;
    second.name = "Second instance".into();
    let Geometry::Asset(instance) = &mut second.geometry else {
        unreachable!()
    };
    instance.scene = 42;
    document.objects.push(second);
    let path = scratch.0.join("model.n3.json");
    super::super::document::save(&path, &document, None, false).unwrap();
    let loaded = load(&path).unwrap();
    assert_eq!(loaded.assets.len(), 1);
    assert_eq!(loaded.document.objects.len(), 2);
    assert_eq!(loaded.diagnostics.len(), 1);
    assert!(loaded.diagnostics[0].contains("Second instance"));
    assert!(loaded.diagnostics[0].contains("42"));
}

#[test]
fn native_save_conflicts_still_compare_exact_bytes_after_reference_rebasing() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0.join("assets"), "Triangle", 1.);
    let loaded = load(&source).unwrap();
    let path = scratch.0.join("model.n3.json");
    super::super::document::save(&path, &loaded.document, None, false).unwrap();
    let loaded = load(&path).unwrap();
    fs::write(&path, b"external change").unwrap();
    let error = super::super::document::save(
        &path,
        &loaded.document,
        loaded.saved_bytes.as_deref(),
        false,
    )
    .unwrap_err();
    assert!(error.contains("changed on disk"));
    assert_eq!(fs::read(path).unwrap(), b"external change");
}

#[test]
fn obj_still_imports_authored_polygons_without_linked_resources() {
    let scratch = Scratch::new();
    let path = scratch.0.join("triangle.obj");
    fs::write(&path, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    let loaded = load(&path).unwrap();
    let Geometry::Mesh(mesh) = &loaded.document.objects[0].geometry else {
        panic!("OBJ must remain authored mesh geometry");
    };
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.faces.len(), 1);
    assert!(loaded.assets.is_empty());
    assert!(loaded.diagnostics.is_empty());
    assert!(loaded.saved_bytes.is_none());
}

#[test]
fn unresolved_relative_references_cannot_be_rebased_from_an_unknown_origin() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0.join("assets"), "Triangle", 1.);
    let mut document = load(&source).unwrap().document;
    let Geometry::Asset(instance) = &mut document.objects[0].geometry else {
        unreachable!()
    };
    instance.source = "assets/Triangle.gltf".into();
    let path = scratch.0.join("model.n3.json");
    assert!(
        super::super::document::save(&path, &document, None, false)
            .unwrap_err()
            .contains("Resolve relative")
    );
    assert!(!path.exists());
}

#[test]
fn append_checks_all_retained_sources_including_undo_resources() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0, "Triangle", 1.);
    let loaded = load(&source).unwrap();
    let asset = loaded.assets.values().next().unwrap().clone();
    let mut resources: BTreeMap<_, _> = (0..MAX_LINKED_SOURCES)
        .map(|index| (format!("source-{index}.gltf"), asset.clone()))
        .collect();
    validate_resource_cache(&resources).unwrap();
    resources.insert("one-more.gltf".into(), asset);
    assert!(
        validate_resource_cache(&resources)
            .unwrap_err()
            .contains("linked-source")
    );
    assert_eq!(
        resources.len(),
        MAX_LINKED_SOURCES + 1,
        "Validation never evicts undo resources"
    );
}

#[test]
fn shared_mesh_instances_charge_cached_evaluation_without_reevaluating_it() {
    let scratch = Scratch::new();
    let source = triangle(&scratch.0, "Triangle", 1.);
    let loaded = load(&source).unwrap();
    let single = loaded.assets.values().next().unwrap();
    let single_bytes = decoded_payload_bytes(single).unwrap();
    let mut data = (***single).clone();
    data.nodes.push(data.nodes[0].clone());
    data.nodes.push(data.nodes[0].clone());
    data.scenes[0].roots = vec![0, 1, 2];
    data.scenes.push(crate::scene::SceneDefinition {
        name: "Empty".into(),
        roots: Vec::new(),
    });
    let repeated = Arc::new(SceneAsset::new(data).unwrap());
    assert_eq!(
        repeated.meshes.len(),
        single.meshes.len(),
        "The source mesh is still shared"
    );
    assert!(decoded_payload_bytes(&repeated).unwrap() > single_bytes);
    let resources = BTreeMap::from([("instanced.gltf".into(), repeated.clone())]);
    assert!(
        validate_resource_cache_with_budget(&resources, single_bytes).is_err(),
        "A tiny source cannot hide its larger cached instantiated pose from the host budget"
    );

    let empty = repeated.evaluate(1, None).unwrap();
    assert!(empty.draws.is_empty());
    let before = repeated.cached_frame_payload_bytes().unwrap();
    decoded_payload_bytes(&repeated).unwrap();
    assert_eq!(repeated.cached_frame_payload_bytes().unwrap(), before);
    assert!(
        Arc::ptr_eq(&empty, &repeated.evaluate(1, None).unwrap()),
        "Budget inspection must not replace the current cached scene with the default pose"
    );
}
