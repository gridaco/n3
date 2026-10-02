//! Host open/import dispatch and linked resources for the authored document.
#[cfg(any(target_arch = "wasm32", test))]
mod bytes;
pub(crate) mod gltf;
mod linked;
#[cfg(not(target_arch = "wasm32"))]
mod native;
pub(crate) mod obj;
mod resources;

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) use bytes::load_bytes;
pub(crate) use linked::{LoadedDocument, validate_resource_cache};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::{FileResolver, document, load};
pub(crate) use resources::ResourceResolver;
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
