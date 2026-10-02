//! Native linked-source snapshots and serialization-only reference rebasing.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use crate::{
    asset_io::linked::{
        LoadedDocument, MAX_LINKED_PAYLOAD_BYTES, MAX_LINKED_SOURCES, decoded_payload_bytes,
    },
    document::{AssetInstance, Document, Geometry, Object, Transform},
};

type Result<T> = std::result::Result<T, String>;

/// Open and import use the same decoding result. The host decides whether to
/// replace the active document or append its objects through one editor edit.
pub(crate) fn load(path: &Path) -> Result<LoadedDocument> {
    if crate::asset_io::is_scene_path(path) {
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
        (super::load_obj_path(path)?, None)
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
            } else if !crate::asset_io::is_scene_path(Path::new(&instance.source)) {
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
