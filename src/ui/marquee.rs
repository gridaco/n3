//! Stateless marquee presentation shared by the viewport and timeline.
//!
//! Hosts own pointer intent, drag thresholds, selection, and commit/cancel.
//! This module only projects two screen points into a clipped rectangle and
//! paints it. Two consumers do not yet justify a universal gesture controller.

use crate::theme;
use egui::{Color32, Painter, Pos2, Rect, Stroke, StrokeKind};

pub(crate) fn rectangle(start: Pos2, current: Pos2, clip: Rect) -> Rect {
    Rect::from_two_pos(start, current).intersect(clip)
}

pub(crate) fn paint(painter: &Painter, rect: Rect) {
    if !rect.is_positive() || !rect.is_finite() {
        return;
    }
    painter.rect_filled(
        rect,
        theme::radius::NONE,
        Color32::from_rgba_unmultiplied(116, 173, 246, 25),
    );
    painter.rect_stroke(
        rect,
        theme::radius::NONE,
        Stroke::new(1.0, Color32::from_rgb(116, 173, 246)),
        StrokeKind::Inside,
    );
}
