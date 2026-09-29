//! Shared object feedback for the viewport and Layers list.

pub const SELECTED_COLOR: egui::Color32 = crate::theme::OBJECT_SELECTED;
pub const HOVERED_COLOR: egui::Color32 = crate::theme::OBJECT_HOVERED;
pub const SELECTED_WIDTH: f32 = 2.0;
pub const HOVERED_WIDTH: f32 = 1.25;

pub fn color(selected: bool, hovered: bool) -> Option<egui::Color32> {
    if selected {
        Some(SELECTED_COLOR)
    } else if hovered {
        Some(HOVERED_COLOR)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_feedback_dominates_hover() {
        assert_eq!(color(false, false), None);
        assert_eq!(color(false, true), Some(HOVERED_COLOR));
        assert_eq!(color(true, false), Some(SELECTED_COLOR));
        assert_eq!(color(true, true), Some(SELECTED_COLOR));
    }
}
