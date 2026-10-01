//! Viewport display policy, independent of authored geometry and materials.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadingMode {
    #[default]
    Solid,
    Wireframe,
    MaterialPreview,
    // TODO: A Rendered mode needs an explicit scene-lighting/output contract;
    // Material Preview is the current supported realtime material inspection.
}
