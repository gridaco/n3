//! Native I/O port. Filesystem policy and synchronous bounded reads stay here;
//! format decoding and retained-resource accounting are shared with browser input.
pub(crate) mod document;
mod linked;
mod resources;

use std::path::Path;

use super::resources::MAX_FILE_BYTES;
use crate::{document::Document, scene::SceneAsset};

pub(crate) use linked::load;
pub(crate) use resources::FileResolver;
use resources::read_file;

type Result<T> = std::result::Result<T, String>;

pub(crate) fn load_scene_path(path: &Path) -> Result<SceneAsset> {
    let bytes = read_file(path, MAX_FILE_BYTES)?;
    let resolver = super::FileResolver::new(
        path.parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new(".")),
    )?;
    super::gltf::load(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("glTF scene"),
        &bytes,
        &resolver,
    )
}

pub(crate) fn load_obj_path(path: &Path) -> Result<Document> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported object");
    super::obj::parse_obj(&document::read_text(path)?, name)
}
