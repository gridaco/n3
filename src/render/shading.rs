//! Viewport display policy, independent of authored geometry and materials.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShadingMode {
    #[default]
    Solid,
    Wireframe,
    // TODO: Add material preview and rendered modes with the future material
    // and lighting pipeline; neither is an alternate name for this solid view.
}
