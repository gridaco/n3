//! Browser runtime hooks. Guide tracing is absent, so these calls retain no state.
use super::Control;

#[inline]
pub fn begin_pass(_ctx: &egui::Context) {}

#[inline]
pub fn record(
    _ctx: &egui::Context,
    _control: Control,
    _label: &str,
    _rect: egui::Rect,
    _enabled: bool,
) {
}

#[inline]
pub fn scope<R>(_ctx: &egui::Context, _parent: Control, draw: impl FnOnce() -> R) -> R {
    draw()
}
