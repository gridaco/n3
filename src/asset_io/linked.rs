//! Linked scene references are host-resolved snapshots, outside edit history.
//! The document owns placement and source identity, not decoded resources. Load
//! resolves sources once; reopening sees current source bytes. There is no live
//! refresh, source rewrite, or embedded glTF package in the native JSON format.
use crate::{document::Document, scene::SceneAsset};
use std::{collections::BTreeMap, sync::Arc};

type Result<T> = std::result::Result<T, String>;
pub(super) const MAX_LINKED_SOURCES: usize = 64;
pub(super) const MAX_LINKED_PAYLOAD_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct LoadedDocument {
    /// Native sources are absolute paths; browser sources have snapshot identity.
    pub document: Document,
    /// Keys match `AssetInstance::source`. Duplicates share decoded resources.
    pub assets: BTreeMap<String, Arc<SceneAsset>>,
    /// Failed links remain authored objects with these recoverable diagnostics.
    pub diagnostics: Vec<String>,
    /// Exact bytes read for optimistic save-conflict detection; imports have none.
    pub saved_bytes: Option<Vec<u8>>,
}

/// Charge dominant retained decoded payloads across unique sources. Each source
/// also has its own parser/evaluator budgets; this prevents many individually
/// valid assets from multiplying retained buffers without a document-wide bound.
/// This is a payload budget, not a claim about total process/GPU memory. The
/// unified evaluator and renderer separately bound placed instances and draws.
pub(super) fn decoded_payload_bytes(asset: &SceneAsset) -> Result<usize> {
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

pub(super) fn validate_resource_cache_with_budget(
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
