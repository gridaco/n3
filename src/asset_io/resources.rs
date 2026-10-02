//! Bounded host resource access for import adapters. No model opens files.
use std::path::{Component, Path};

type Result<T> = std::result::Result<T, String>;

pub(crate) const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_RESOURCE_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;

/// A host may replace filesystem access with an in-memory asset package. The
/// loader validates and percent-decodes URI paths before invoking this boundary.
pub(crate) trait ResourceResolver {
    fn read(&self, relative_path: &str, maximum_bytes: usize) -> Result<Vec<u8>>;
}

pub(crate) fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains(['\\', ':', '?', '#', '\0'])
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err("Asset resources must be relative paths inside the asset directory; network and parent paths are not allowed.".into());
    }
    Ok(())
}
