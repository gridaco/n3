//! Zero-size production probes: no clocks, TLS, allocations, or recorder.
use super::Stage;

#[derive(Default)]
pub(crate) struct FrameProbe {
    _private: (),
}
impl FrameProbe {
    #[inline(always)]
    pub(crate) fn end(&mut self, _: Stage) {}
    #[inline(always)]
    pub(crate) fn mesh_upload(&mut self, _: usize) {}
    #[inline(always)]
    pub(crate) fn resized(&mut self) {}
    #[inline(always)]
    pub(crate) fn ui_jobs(&mut self, _: usize) {}
    #[inline(always)]
    pub(crate) fn pixel_scale(&mut self, _: f32) {}
    #[inline(always)]
    pub(crate) fn egui_pass(&mut self) {}
    #[inline(always)]
    pub(crate) fn egui_tessellate(&mut self) {}
    #[inline(always)]
    pub(crate) fn egui_texture_update(&mut self) {}
    #[inline(always)]
    pub(crate) fn egui_composite(&mut self) {}
    #[inline(always)]
    pub(crate) fn scene_render(&mut self) {}
    #[inline(always)]
    pub(crate) fn scene_size(&mut self, _: [u32; 2]) {}
    #[inline(always)]
    pub(crate) fn surface_size(&mut self, _: [u32; 2]) {}
    #[inline(always)]
    pub(crate) fn editor_feedback(&mut self) {}
}
pub(crate) struct ProjectionProbe;
impl ProjectionProbe {
    #[inline(always)]
    pub(crate) fn finish(self, _: usize) {}
}
#[inline(always)]
pub(crate) fn projection_started() -> ProjectionProbe {
    ProjectionProbe
}
#[inline(always)]
pub(crate) fn geometry_rebuilt() {}
#[inline(always)]
pub(crate) fn editor_prepared() {}
