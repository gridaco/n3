//! Native open/import dispatch and linked resources for the authored document.
pub(crate) mod document;
pub(crate) mod gltf;
mod linked;
pub(crate) mod obj;
mod resources;

pub(crate) use gltf::load_path as load_scene_path;
pub(crate) use linked::{LoadedDocument, load, validate_resource_cache};
pub(crate) use resources::{FileResolver, ResourceResolver};
use std::path::Path;

pub(crate) fn is_scene_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("gltf") || extension.eq_ignore_ascii_case("glb")
    })
}
pub(crate) fn is_supported_path(path: &Path) -> bool {
    is_scene_path(path)
        || path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("obj") || extension.eq_ignore_ascii_case("json")
        })
}
