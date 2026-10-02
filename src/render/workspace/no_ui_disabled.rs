//! Ordinary builds do not allocate or compile the measurement compositor.
pub(super) struct ScenePresentation;
impl ScenePresentation {
    pub(super) fn new(_: wgpu::TextureFormat) -> Self {
        Self
    }
    pub(super) fn invalidate(&mut self) {}
}
