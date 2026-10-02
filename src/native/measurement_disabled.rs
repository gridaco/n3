//! Zero-cost ordinary-host counterpart to the optional measurement adapter.
use crate::{measurement::FrameProbe, workspace_ui::WorkspaceUi};
use winit::window::{Window, WindowAttributes};

pub(super) struct Host;

impl Host {
    pub(super) fn from_environment() -> Result<Self, String> {
        Ok(Self)
    }

    pub(super) fn configured(&self) -> bool {
        false
    }

    pub(super) fn active(&self) -> bool {
        false
    }

    pub(super) fn window_attributes(&self, attributes: WindowAttributes) -> WindowAttributes {
        attributes
    }

    pub(super) fn set_adapter(&mut self, _: &wgpu::AdapterInfo) {}

    #[inline(always)]
    pub(super) fn without_ui(&self) -> bool {
        false
    }

    #[inline(always)]
    pub(super) fn prepare_without_ui(&self, _: &mut WorkspaceUi, _: &mut FrameProbe) {}

    #[inline(always)]
    pub(super) fn paint_without_ui(
        &self,
        _: &mut crate::render::workspace::WorkspaceRenderer,
        _: crate::render::workspace::FrameTarget<'_>,
        _: &mut WorkspaceUi,
        _: std::collections::BTreeMap<crate::document::AssetInstance, crate::scene_view::SceneView>,
        _: f32,
        _: &mut FrameProbe,
    ) {
        unreachable!("measurement mode is disabled")
    }

    pub(super) fn begin_frame(
        &mut self,
        _: &mut WorkspaceUi,
        _: &Window,
        _: &wgpu::SurfaceConfiguration,
    ) -> Result<FrameProbe, String> {
        Ok(FrameProbe::default())
    }

    pub(super) fn finish_frame(
        &mut self,
        _: FrameProbe,
        _: &WorkspaceUi,
        _: bool,
    ) -> Result<bool, String> {
        Ok(false)
    }
}
