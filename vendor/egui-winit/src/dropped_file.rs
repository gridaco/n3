use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct NativeFile {
    path: PathBuf,
}

impl From<PathBuf> for NativeFile {
    fn from(path: PathBuf) -> Self {
        Self { path }
    }
}

impl egui::DroppedFile for NativeFile {
    fn path(&self) -> &Path {
        &self.path
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.path).map_err(|err| err.to_string())
    }

    #[cfg(target_arch = "wasm32")]
    fn bytes_async(
        &self,
    ) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Vec<u8>, String>> + '_>> {
        // Winit provides a native path, not a browser File. Browser hosts must
        // supply selected/dropped file bytes through their browser file adapter.
        Box::pin(async { Err("Native file paths cannot be read in a browser".into()) })
    }
}
