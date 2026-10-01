//! Linked scene references are host-resolved snapshots, outside edit history.
//! The document owns placement and source identity, not decoded resources. Load
//! resolves sources once; reopening sees current source bytes. There is no live
//! refresh, source rewrite, or embedded glTF package in the native JSON format.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::{
    document::{AssetInstance, Document, Geometry, Object, Transform},
    scene::SceneAsset,
};

type Result<T> = std::result::Result<T, String>;
const MAX_LINKED_SOURCES: usize = 64;
const MAX_LINKED_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct LoadedDocument {
    /// Asset sources are absolute in memory, independent of later Save As paths.
    pub document: Document,
    /// Keys match `AssetInstance::source`. Duplicates share decoded resources.
    pub assets: BTreeMap<String, Arc<SceneAsset>>,
    /// Failed links remain authored objects with these recoverable diagnostics.
    pub diagnostics: Vec<String>,
    /// Exact bytes read for optimistic save-conflict detection; imports have none.
    pub saved_bytes: Option<Vec<u8>>,
}

/// Open and import use the same decoding result. The host decides whether to
/// replace the active document or append its objects through one editor edit.
pub(crate) fn load(path: &Path) -> Result<LoadedDocument> {
    if super::is_scene_path(path) {
        let source = absolute_reference(path)?;
        let asset = Arc::new(super::load_scene_path(Path::new(&source))?);
        if decoded_payload_bytes(&asset)? > MAX_LINKED_PAYLOAD_BYTES {
            return Err("Asset exceeds the 512 MiB linked payload budget.".into());
        }
        let name = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("Imported asset")
            .to_owned();
        let document = Document {
            objects: vec![Object {
                id: 1,
                name,
                transform: Transform::default(),
                geometry: Geometry::Asset(AssetInstance {
                    source: source.clone(),
                    scene: asset.default_scene,
                }),
            }],
            ..Document::default()
        };
        document.validate()?;
        return Ok(LoadedDocument {
            document,
            assets: BTreeMap::from([(source, asset)]),
            diagnostics: Vec::new(),
            saved_bytes: None,
        });
    }

    let is_obj = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("obj"));
    let (document, saved_bytes) = if is_obj {
        (super::obj::load_path(path)?, None)
    } else {
        // One read supplies both the decoded document and the conflict baseline.
        let text = super::document::read_text(path)?;
        (Document::from_json(&text)?, Some(text.into_bytes()))
    };
    let base = absolute_parent(path)?;
    resolve(document, &base, saved_bytes)
}

fn resolve(
    mut document: Document,
    directory: &Path,
    saved_bytes: Option<Vec<u8>>,
) -> Result<LoadedDocument> {
    let mut assets = BTreeMap::new();
    let mut diagnostics = Vec::new();
    let mut attempted = BTreeSet::new();
    let mut retained_payload = 0usize;
    for object in &mut document.objects {
        let Geometry::Asset(instance) = &mut object.geometry else {
            continue;
        };
        let path = directory.join(&instance.source);
        instance.source = absolute_reference(&path)?;
        if attempted.insert(instance.source.clone()) {
            let result = if attempted.len() > MAX_LINKED_SOURCES {
                Err(format!(
                    "Document exceeds the {MAX_LINKED_SOURCES} linked-source loading limit."
                ))
            } else if !super::is_scene_path(Path::new(&instance.source)) {
                Err("Linked assets must be glTF or GLB files.".to_owned())
            } else {
                super::load_scene_path(Path::new(&instance.source))
            };
            match result {
                Ok(asset) => {
                    let bytes = decoded_payload_bytes(&asset)?;
                    if let Some(total) = retained_payload
                        .checked_add(bytes)
                        .filter(|total| *total <= MAX_LINKED_PAYLOAD_BYTES)
                    {
                        retained_payload = total;
                        assets.insert(instance.source.clone(), Arc::new(asset));
                    } else {
                        diagnostics.push(format!(
                            "Cannot load linked asset {:?}: document exceeds the 512 MiB linked payload budget.",
                            instance.source
                        ));
                    }
                }
                Err(error) => diagnostics.push(format!(
                    "Cannot load linked asset {:?}: {error}",
                    instance.source
                )),
            }
        }
        if let Some(asset) = assets.get(&instance.source)
            && let Err(error) = asset.evaluate(instance.scene, None)
        {
            diagnostics.push(format!(
                "Cannot evaluate linked object {:?}, scene {}: {error}",
                object.name, instance.scene
            ));
        }
    }
    document.validate()?;
    Ok(LoadedDocument {
        document,
        assets,
        diagnostics,
        saved_bytes,
    })
}

/// Charge dominant retained decoded payloads across unique sources. Each source
/// also has its own parser/evaluator budgets; this prevents many individually
/// valid assets from multiplying retained buffers without a document-wide bound.
/// This is a payload budget, not a claim about total process/GPU memory. The
/// unified evaluator and renderer separately bound placed instances and draws.
fn decoded_payload_bytes(asset: &SceneAsset) -> Result<usize> {
    let mut bytes = asset.cached_frame_payload_bytes()?;
    let mut add = |count: usize, size: usize| {
        bytes = bytes.saturating_add(count.saturating_mul(size));
    };
    for image in &asset.images {
        add(image.rgba8.capacity(), 1);
    }
    for mesh in &asset.meshes {
        for primitive in &mesh.primitives {
            add(
                primitive.vertices.capacity(),
                std::mem::size_of::<crate::scene::SceneVertex>(),
            );
            add(primitive.indices.len(), std::mem::size_of::<u32>());
            for morph in &primitive.morphs {
                add(morph.positions.capacity(), std::mem::size_of::<[f64; 3]>());
                add(morph.normals.capacity(), std::mem::size_of::<[f32; 3]>());
                add(morph.tangents.capacity(), std::mem::size_of::<[f32; 3]>());
            }
            for influences in &primitive.influences {
                add(
                    influences.capacity(),
                    std::mem::size_of::<([u16; 4], [f32; 4])>(),
                );
            }
        }
    }
    for clip in &asset.animations {
        for channel in &clip.channels {
            add(channel.times.capacity(), std::mem::size_of::<f32>());
            add(channel.values.capacity(), std::mem::size_of::<f64>());
            add(channel.in_tangents.capacity(), std::mem::size_of::<f64>());
            add(channel.out_tangents.capacity(), std::mem::size_of::<f64>());
        }
    }
    for skin in &asset.skins {
        add(
            skin.inverse_bind.capacity(),
            std::mem::size_of::<glam::DMat4>(),
        );
    }
    Ok(bytes)
}

/// Append must validate the union, including resources retained for Undo after
/// an object was deleted. Reject before publishing any document/cache changes.
pub(crate) fn validate_resource_cache(assets: &BTreeMap<String, Arc<SceneAsset>>) -> Result<()> {
    validate_resource_cache_with_budget(assets, MAX_LINKED_PAYLOAD_BYTES)
}

fn validate_resource_cache_with_budget(
    assets: &BTreeMap<String, Arc<SceneAsset>>,
    maximum_payload: usize,
) -> Result<()> {
    if assets.len() > MAX_LINKED_SOURCES {
        return Err(format!(
            "Document exceeds the {MAX_LINKED_SOURCES} linked-source loading limit."
        ));
    }
    let mut total = 0usize;
    for asset in assets.values() {
        total = total
            .checked_add(decoded_payload_bytes(asset)?)
            .filter(|bytes| *bytes <= maximum_payload)
            .ok_or("Document exceeds the 512 MiB linked payload budget.")?;
    }
    Ok(())
}

/// Source normalization affects the newly loaded snapshot only. Existing parent
/// directories are resolved to avoid interpreting `..` across a symlink wrongly.
/// The final file is not canonicalized: a symlink asset remains subject to the
/// loader's regular-file policy. Missing parents retain their anchored path.
fn absolute_reference(path: &Path) -> Result<String> {
    let mut absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    if let (Some(parent), Some(name)) = (absolute.parent(), absolute.file_name())
        && let Ok(parent) = fs::canonicalize(parent)
    {
        absolute = parent.join(name);
    }
    absolute
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Asset paths must be valid UTF-8 to persist in an N3 document.".into())
}

fn absolute_parent(path: &Path) -> Result<PathBuf> {
    let absolute = std::path::absolute(path).map_err(|error| error.to_string())?;
    let parent = absolute
        .parent()
        .ok_or("Document path needs a directory.")?;
    fs::canonicalize(parent).map_err(|error| format!("Cannot resolve document directory: {error}"))
}

pub(super) fn snapshot_for_save(document: &Document, destination: &Path) -> Result<Document> {
    let mut snapshot = document.clone();
    // Documents without links keep the existing persistence behavior, including
    // its useful destination/create errors, without requiring path rebasing.
    if !snapshot
        .objects
        .iter()
        .any(|object| matches!(object.geometry, Geometry::Asset(_)))
    {
        return Ok(snapshot);
    }
    let directory = absolute_parent(destination)?;
    for object in &mut snapshot.objects {
        let Geometry::Asset(instance) = &mut object.geometry else {
            continue;
        };
        let source = Path::new(&instance.source);
        if !source.is_absolute() {
            return Err("Resolve relative asset references before saving the document.".into());
        }
        if let Some(relative) = relative_to(source, &directory) {
            instance.source = relative
                .to_str()
                .ok_or("Asset paths must be valid UTF-8.")?
                .to_owned();
        }
    }
    Ok(snapshot)
}

/// Both paths are absolute. Preserve source components (including unresolved
/// missing-directory `..`) and fall back to an absolute reference across roots.
fn relative_to(source: &Path, directory: &Path) -> Option<PathBuf> {
    let source: Vec<_> = source.components().collect();
    let directory: Vec<_> = directory.components().collect();
    let common = source
        .iter()
        .zip(&directory)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0
        || source[common..]
            .iter()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
    {
        return None;
    }
    let mut relative = PathBuf::new();
    for _ in &directory[common..] {
        relative.push("..");
    }
    for component in &source[common..] {
        relative.push(component.as_os_str());
    }
    Some(relative)
}

#[cfg(test)]
mod tests;
