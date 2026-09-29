//! Shared logical-point click versus drag policy.

pub const DRAG_THRESHOLD: f32 = 4.0;

pub fn crossed_drag_threshold(start: egui::Pos2, position: egui::Pos2) -> bool {
    start.is_finite() && position.is_finite() && start.distance(position) > DRAG_THRESHOLD
}
