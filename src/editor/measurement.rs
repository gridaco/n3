//! Reuse production viewport preparation without constructing an egui frame.
use super::{Camera, Editor, Projection, Rect};

impl Editor {
    pub(crate) fn prepare_viewport(
        &mut self,
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
    ) -> Result<(), String> {
        self.prepare(&Projection::new(viewport, camera, z_up)?)
    }
}
