//! Native bounded filesystem access. The portable decoders only see the resolver contract.
use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use crate::asset_io::resources::{ResourceResolver, validate_path};

type Result<T> = std::result::Result<T, String>;

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
