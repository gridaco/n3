//! Bounded host resource access for import adapters. No model opens files.
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;

pub(crate) const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_RESOURCE_BYTES: usize = 256 * 1024 * 1024;
pub(crate) const MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;

/// A host may replace filesystem access with an in-memory asset package. The
/// loader validates and percent-decodes URI paths before invoking this boundary.
pub(crate) trait ResourceResolver {
    fn read(&self, relative_path: &str, maximum_bytes: usize) -> Result<Vec<u8>>;
}

pub(crate) struct FileResolver {
    root: PathBuf,
}
impl FileResolver {
    pub(crate) fn new(directory: &Path) -> Result<Self> {
        let root = directory
            .canonicalize()
            .map_err(|error| format!("Cannot open asset directory: {error}"))?;
        if !root.is_dir() {
            return Err("The asset resource root is not a directory.".into());
        }
        Ok(Self { root })
    }
}
impl ResourceResolver for FileResolver {
    fn read(&self, relative_path: &str, maximum_bytes: usize) -> Result<Vec<u8>> {
        validate_path(relative_path)?;
        let mut path = self.root.clone();
        for component in Path::new(relative_path).components() {
            let Component::Normal(name) = component else {
                continue;
            };
            path.push(name);
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                format!("Cannot read asset resource {relative_path:?}: {error}")
            })?;
            if metadata.file_type().is_symlink() {
                return Err("Asset resources cannot follow symbolic links.".into());
            }
        }
        read_file(&path, maximum_bytes)
    }
}

pub(crate) fn read_file(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| format!("Cannot read asset: {error}"))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("Assets and resources must be regular files, not symbolic links.".into());
    }
    if metadata.len() > maximum_bytes as u64 {
        return Err("Asset resource exceeds its byte budget.".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| error.to_string())?
        .take(maximum_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum_bytes {
        return Err("Asset resource exceeds its byte budget.".into());
    }
    Ok(bytes)
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
