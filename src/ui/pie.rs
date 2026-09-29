//! Shared geometry and visual treatment for held-key pies. Hosts retain input
//! ownership and each menu supplies only its actions and directional policy.
use egui::{Align2, Context, FontId, Id, LayerId, Order, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::{
    controls::{self, Control},
    theme,
};

const CARD_SIZE: Vec2 = Vec2::new(100.0, 30.0);
const PADDING: f32 = theme::space::LG;
pub(super) const DEAD_ZONE: f32 = 24.0;

#[derive(Clone, Debug)]
pub(super) struct Layout {
    // Retain the original press position even when edge avoidance moves the pie.
    anchor: Pos2,
    viewport: Rect,
    pub center: Pos2,
    radius: Vec2,
    scale: f32,
}

pub(super) struct Item {
    pub control: Control,
    pub offset: Vec2,
    pub enabled: bool,
    pub hovered: bool,
    pub selected: bool,
}

impl Layout {
    pub fn open(anchor: Pos2, viewport: Rect, radius: Vec2) -> Option<Self> {
        if !anchor.is_finite()
            || !viewport.contains(anchor)
            || !viewport.is_finite()
            || !viewport.width().is_finite()
            || !viewport.height().is_finite()
            || viewport.width() <= 0.0
            || viewport.height() <= 0.0
        {
            return None;
        }
        let full_size = radius * 2.0 + CARD_SIZE;
        // Scale margins together with the pie, including for very small windows.
        let scale = ((viewport.width() / (full_size.x + PADDING * 2.0))
            .min(viewport.height() / (full_size.y + PADDING * 2.0)))
        .min(1.0);
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let half_size = (full_size * 0.5 + Vec2::splat(PADDING)) * scale;
        let safe = Rect::from_min_max(viewport.min + half_size, viewport.max - half_size);
        let center = Pos2::new(
            anchor.x.clamp(safe.left(), safe.right().max(safe.left())),
            anchor.y.clamp(safe.top(), safe.bottom().max(safe.top())),
        );
        Some(Self {
            anchor,
            viewport,
            center,
            radius,
            scale,
        })
    }

    #[cfg(test)]
    pub fn anchor(&self) -> Pos2 {
        self.anchor
    }

    pub fn item_rect(&self, offset: Vec2) -> Rect {
        Rect::from_center_size(
            self.center + offset * self.radius * self.scale,
            CARD_SIZE * self.scale,
        )
    }

    /// Both the original press location and the displayed center cancel. This
    /// also prevents an unintentional choice when opening near a viewport edge.
    pub fn accepts(&self, point: Pos2) -> bool {
        point.is_finite()
            && self.viewport.contains(point)
            && point.distance(self.anchor) > DEAD_ZONE
            && point.distance(self.center) > DEAD_ZONE * self.scale
    }

    pub fn normalized_delta(&self, delta: Vec2) -> Vec2 {
        delta / (self.radius * self.scale)
    }

    pub fn paint(&self, ctx: &Context, layer: &'static str, group: Control, items: &[Item]) {
        let center = self.center;
        ctx.set_cursor_icon(if items.iter().any(|item| item.hovered) {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Default
        });
        let painter = ctx
            .layer_painter(LayerId::new(Order::Foreground, Id::new(layer)))
            .with_clip_rect(self.viewport);
        let visuals = ctx.global_style().visuals.clone();
        let line = Stroke::new(
            self.scale,
            visuals
                .widgets
                .noninteractive
                .fg_stroke
                .color
                .gamma_multiply(0.5),
        );
        let center_radius = DEAD_ZONE * self.scale;
        for item in items {
            let target = self.item_rect(item.offset).center();
            let direction = (target - center).normalized();
            painter.line_segment([center + direction * center_radius, target], line);
        }
        painter.circle_filled(center, center_radius, visuals.window_fill());
        painter.circle_stroke(center, center_radius, line);
        let cross = 3.0 * self.scale;
        painter.line_segment([center - Vec2::X * cross, center + Vec2::X * cross], line);
        painter.line_segment([center - Vec2::Y * cross, center + Vec2::Y * cross], line);
        if self.anchor.distance(center) > 1.0 {
            painter.circle_stroke(self.anchor, DEAD_ZONE, line);
        }
        let bounds = Rect::from_center_size(center, (self.radius * 2.0 + CARD_SIZE) * self.scale);
        controls::record(
            ctx,
            group,
            group.label(),
            bounds
                .union(Rect::from_center_size(
                    self.anchor,
                    Vec2::splat(DEAD_ZONE * 2.0),
                ))
                .intersect(self.viewport),
            true,
        );
        controls::scope(ctx, group, || {
            for item in items {
                let control = item.control;
                let rect = self.item_rect(item.offset);
                let fill = if item.hovered {
                    visuals.selection.bg_fill
                } else {
                    visuals.window_fill()
                };
                let stroke = if item.hovered || item.selected {
                    visuals.selection.stroke
                } else {
                    visuals.window_stroke
                };
                painter.rect(
                    rect,
                    f32::from(theme::radius::MD) * self.scale,
                    fill,
                    Stroke::new(stroke.width * self.scale, stroke.color),
                    StrokeKind::Inside,
                );
                let text_color = if !item.enabled {
                    visuals.weak_text_color()
                } else if item.selected {
                    visuals.selection.stroke.color
                } else {
                    visuals.text_color()
                };
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    control.label(),
                    FontId::proportional(theme::text::UI_BODY_13 * self.scale),
                    text_color,
                );
                controls::record(ctx, control, control.label(), rect, item.enabled);
            }
        });
    }
}
