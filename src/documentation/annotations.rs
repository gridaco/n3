//! Paint-only callouts anchored to controls observed in the current UI pass.
//! A missing target fails the tutorial instead of pointing at a stale coordinate.
use crate::{
    controls::{Control, Trace},
    theme,
};
use egui::{Color32, Pos2, Rect, Stroke, Vec2};

#[derive(Clone)]
pub struct Annotation {
    pub target: Control,
    pub caption: String,
}

pub fn paint(
    ctx: &egui::Context,
    viewport: Rect,
    trace: &Trace,
    notes: &[Annotation],
) -> Result<(), String> {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("n3.documentation.annotations"),
    ));
    for note in notes {
        let target = trace.get(note.target)?;
        if !target.rect.is_finite() || !target.rect.is_positive() {
            return Err(format!(
                "Annotation target {} has no visible bounds",
                note.target.id()
            ));
        }
        let text = format!("{}\n{}", target.label, note.caption);
        let galley = painter.layout(
            text,
            egui::FontId::proportional(crate::theme::text::GUIDE_CUE_13),
            Color32::WHITE,
            230.0,
        );
        let cue_padding = theme::space::LG + theme::space::XS;
        let rect = placement(
            viewport,
            target.rect,
            galley.size() + Vec2::splat(cue_padding * 2.0),
        );
        let accent = Color32::from_rgb(255, 207, 72);
        painter.rect_stroke(
            target.rect.expand(theme::space::SM),
            theme::radius::MD,
            Stroke::new(2.0, accent),
            egui::StrokeKind::Outside,
        );
        let tip = Pos2::new(
            target.rect.center().x.clamp(rect.left(), rect.right()),
            target.rect.center().y.clamp(rect.top(), rect.bottom()),
        );
        painter.line_segment([target.rect.center(), tip], Stroke::new(1.0, accent));
        painter.rect_filled(rect, theme::radius::LG, Color32::from_rgb(29, 34, 43));
        painter.rect_stroke(
            rect,
            theme::radius::LG,
            Stroke::new(1.0, accent),
            egui::StrokeKind::Inside,
        );
        painter.galley(rect.min + Vec2::splat(cue_padding), galley, Color32::WHITE);
    }
    Ok(())
}

fn placement(viewport: Rect, target: Rect, size: Vec2) -> Rect {
    let bounds = viewport.shrink(theme::space::XL);
    let size = size.min(bounds.size().max(Vec2::splat(1.0)));
    let clearance = theme::space::XL + theme::space::XS;
    let mut origin = egui::pos2(target.left() - size.x - clearance, target.top());
    if origin.x < bounds.left() {
        origin = egui::pos2(target.left(), target.bottom() + clearance);
    }
    origin.x = origin
        .x
        .clamp(bounds.left(), (bounds.right() - size.x).max(bounds.left()));
    origin.y = origin
        .y
        .clamp(bounds.top(), (bounds.bottom() - size.y).max(bounds.top()));
    Rect::from_min_size(origin, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callouts_follow_live_bounds_and_stay_inside_the_viewport() {
        let viewport = Rect::from_min_max(egui::pos2(180.0, 40.0), egui::pos2(760.0, 680.0));
        let a = Rect::from_min_size(egui::pos2(650.0, 80.0), Vec2::splat(40.0));
        let b = a.translate(egui::vec2(-200.0, 250.0));
        let first = placement(viewport, a, egui::vec2(230.0, 70.0));
        let second = placement(viewport, b, egui::vec2(230.0, 70.0));
        assert!(viewport.contains_rect(first) && viewport.contains_rect(second));
        assert_ne!(first, second);
    }
}
