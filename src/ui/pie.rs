//! Shared geometry and visual treatment for held-key pies. Hosts retain input
//! ownership and each menu supplies only its actions and directional policy.
use egui::{Align2, Context, FontId, Id, LayerId, Order, Pos2, Rect, Stroke, StrokeKind, Vec2};
use std::{cell::Cell, time::Duration};

use crate::{
    controls::{self, Control},
    theme,
};

pub(super) const CARD_SIZE: Vec2 = Vec2::new(100.0, 30.0);
const CARD_PADDING_X: f32 = theme::space::XL;
const PADDING: f32 = theme::space::LG;
pub(super) const DEAD_ZONE: f32 = 24.0;
// Radius of the opening between the inward-facing card edges.
pub(super) const RADIUS: Vec2 = Vec2::splat(theme::size::STEP_32);

#[derive(Clone, Debug)]
pub(super) struct Layout {
    // Retain the original press position even when edge avoidance moves the pie.
    anchor: Pos2,
    bounds: Rect,
    pub center: Pos2,
    radius: Vec2,
    card_size: Vec2,
    cards: Vec<(Control, Vec2)>,
    scale: f32,
    hovered_since: Cell<Option<(Control, f64)>>,
    pointing_angle: Cell<f32>,
}

pub(super) struct Item {
    pub control: Control,
    pub offset: Vec2,
    pub enabled: bool,
    pub hovered: bool,
    pub selected: bool,
    pub sector_half_angle: f32,
    pub tooltip: Option<&'static str>,
}

impl Layout {
    pub fn measure_cards(
        ctx: &Context,
        controls: impl Iterator<Item = Control>,
    ) -> Vec<(Control, Vec2)> {
        ctx.fonts_mut(|fonts| {
            controls
                .map(|control| {
                    let text = fonts.layout_no_wrap(
                        control.label().to_owned(),
                        FontId::proportional(theme::text::UI_BODY_13),
                        egui::Color32::WHITE,
                    );
                    (
                        control,
                        Vec2::new(
                            CARD_SIZE.x.max(text.size().x + CARD_PADDING_X * 2.0),
                            CARD_SIZE.y,
                        ),
                    )
                })
                .collect()
        })
    }

    /// The caller supplies the usable layout, clipping, and selection bounds.
    pub fn open(
        anchor: Pos2,
        bounds: Rect,
        radius: Vec2,
        cards: Vec<(Control, Vec2)>,
    ) -> Option<Self> {
        if !anchor.is_finite()
            || !bounds.contains(anchor)
            || !bounds.is_finite()
            || !bounds.width().is_finite()
            || !bounds.height().is_finite()
            || bounds.width() <= 0.0
            || bounds.height() <= 0.0
        {
            return None;
        }
        let card_size = cards
            .iter()
            .fold(CARD_SIZE, |size, (_, card)| size.max(*card));
        let full_size = (radius + card_size) * 2.0;
        // Scale margins together with the pie, including for very small windows.
        let scale = ((bounds.width() / (full_size.x + PADDING * 2.0))
            .min(bounds.height() / (full_size.y + PADDING * 2.0)))
        .min(1.0);
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let half_size = (full_size * 0.5 + Vec2::splat(PADDING)) * scale;
        let safe = Rect::from_min_max(bounds.min + half_size, bounds.max - half_size);
        let center = Pos2::new(
            anchor.x.clamp(safe.left(), safe.right().max(safe.left())),
            anchor.y.clamp(safe.top(), safe.bottom().max(safe.top())),
        );
        Some(Self {
            anchor,
            bounds,
            center,
            radius,
            card_size,
            cards,
            scale,
            hovered_since: Cell::new(None),
            pointing_angle: Cell::new(0.0),
        })
    }

    #[cfg(test)]
    pub fn anchor(&self) -> Pos2 {
        self.anchor
    }

    pub fn item_rect(&self, offset: Vec2, control: Control) -> Rect {
        let size = self
            .cards
            .iter()
            .find(|(id, _)| *id == control)
            .map_or(CARD_SIZE, |(_, size)| *size);
        // Horizontal cards grow outward from their inner edge on the circle,
        // including diagonals. Only the top/bottom cards use a vertical edge.
        let outward = if offset.x == 0.0 {
            Vec2::new(0.0, offset.y.signum() * size.y * 0.5)
        } else {
            Vec2::new(offset.x.signum() * size.x * 0.5, 0.0)
        };
        Rect::from_center_size(
            self.center + (offset.normalized() * self.radius + outward) * self.scale,
            size * self.scale,
        )
    }

    /// Both the original press location and the displayed center cancel. This
    /// also prevents an unintentional choice near the layout boundary.
    pub fn accepts(&self, point: Pos2) -> bool {
        point.is_finite()
            && self.bounds.contains(point)
            && point.distance(self.anchor) > DEAD_ZONE
            && point.distance(self.center) > DEAD_ZONE * self.scale
    }

    pub fn normalized_delta(&self, delta: Vec2) -> Vec2 {
        delta / (self.radius * self.scale)
    }

    pub fn paint(
        &self,
        ctx: &Context,
        layer: &'static str,
        instance: Option<Id>,
        group: Control,
        items: &[Item],
        pointer: Option<Pos2>,
    ) {
        let center = self.center;
        ctx.set_cursor_icon(if items.iter().any(|item| item.hovered) {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Default
        });
        // The editor keeps its canonical identities. Independent consumers
        // supply an instance ID so their painted layers and tooltip Areas do
        // not share egui state with another copy of the same pie.
        let layer_id = LayerId::new(
            Order::Foreground,
            instance.unwrap_or_else(|| Id::new(layer)),
        );
        let painter = ctx.layer_painter(layer_id).with_clip_rect(self.bounds);
        let visuals = ctx.global_style().visuals.clone();
        let muted_color = visuals
            .widgets
            .noninteractive
            .fg_stroke
            .color
            .gamma_multiply(0.5);
        let center_radius = DEAD_ZONE * self.scale;
        let ring_width = theme::size::MD * self.scale;
        let ring_radius = center_radius - ring_width * 0.5;
        // egui's circle stroke is outside its radius, whereas an open path
        // stroke is centered. Use centered paths for both parts of the ring.
        let ring = (0..64)
            .map(|step| {
                center + Vec2::angled(std::f32::consts::TAU * step as f32 / 64.0) * ring_radius
            })
            .collect();
        painter.add(egui::epaint::PathShape::closed_line(
            ring,
            Stroke::new(ring_width, muted_color),
        ));
        if let Some(pointer) = pointer {
            let direction = pointer - center;
            if direction.is_finite() && direction.length_sq() > 0.0 {
                self.pointing_angle.set(direction.y.atan2(direction.x));
            }
        }
        // Direction feedback is independent of whether a sector has a choice.
        // At the exact center, retain the last angle instead of jumping.
        let angle = self.pointing_angle.get();
        let half = items
            .first()
            .map_or(std::f32::consts::FRAC_PI_8, |item| item.sector_half_angle);
        let points = (0..=16)
            .map(|step| {
                let a = angle - half + 2.0 * half * step as f32 / 16.0;
                center + Vec2::angled(a) * ring_radius
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(ring_width, visuals.selection.stroke.color),
        ));
        // A painted label keeps the held-key menu's pointer ownership unchanged.
        // Its surface keeps the name readable over the scene.
        let title = painter.layout_no_wrap(
            group.label().to_owned(),
            FontId::proportional(theme::text::UI_BODY_13 * self.scale),
            visuals.weak_text_color(),
        );
        let title_pos = center
            - Vec2::new(
                title.size().x * 0.5,
                center_radius + theme::space::SM * self.scale + title.size().y,
            );
        let title_rect = Rect::from_min_size(title_pos, title.size())
            .expand2(Vec2::new(theme::space::SM, theme::space::XS) * self.scale);
        painter.rect_filled(
            title_rect,
            f32::from(theme::radius::SM) * self.scale,
            visuals.window_fill(),
        );
        painter.galley(title_pos, title, visuals.weak_text_color());
        let bounds =
            Rect::from_center_size(center, (self.radius + self.card_size) * 2.0 * self.scale);
        controls::record(
            ctx,
            group,
            group.label(),
            bounds
                .union(Rect::from_center_size(
                    self.anchor,
                    Vec2::splat(DEAD_ZONE * 2.0),
                ))
                .intersect(self.bounds),
            true,
        );
        controls::scope(ctx, group, || {
            for item in items {
                let control = item.control;
                let rect = self.item_rect(item.offset, control);
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
        let now = ctx.input(|input| input.time);
        let delay = ctx.global_style().interaction.tooltip_delay as f64;
        if let Some(item) = items
            .iter()
            .find(|item| item.hovered && item.tooltip.is_some())
        {
            let since = match self.hovered_since.get() {
                Some((control, since)) if control == item.control => since,
                _ => {
                    self.hovered_since.set(Some((item.control, now)));
                    now
                }
            };
            let remaining = delay - (now - since);
            if remaining > 0.0 {
                ctx.request_repaint_after(Duration::from_secs_f64(remaining));
            } else {
                // This is sector hover, not widget hover: moving within the
                // same sector keeps the timer, even far outside its card.
                egui::Tooltip::always_open(
                    ctx.clone(),
                    layer_id,
                    instance.map_or_else(
                        || Id::new((layer, item.control.id())),
                        |id| id.with(item.control.id()),
                    ),
                    self.item_rect(item.offset, item.control),
                )
                .show(|ui| {
                    ui.label(item.tooltip.unwrap());
                });
            }
        } else {
            self.hovered_since.set(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_names_are_painted_clear_of_the_cancel_ring_and_choices() {
        for visuals in [egui::Visuals::light(), egui::Visuals::dark()] {
            let ctx = Context::default();
            ctx.set_visuals(visuals);
            for group in [Control::ViewPie, Control::ShadingPie] {
                for size in [Vec2::new(1000.0, 800.0), Vec2::new(200.0, 120.0)] {
                    let viewport = Rect::from_min_size(Pos2::ZERO, size);
                    for anchor in [
                        viewport.center(),
                        viewport.left_top(),
                        viewport.right_bottom(),
                    ] {
                        let layout = Layout::open(anchor, viewport, RADIUS, vec![]).unwrap();
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(viewport),
                                ..Default::default()
                            },
                            |ui| layout.paint(ui.ctx(), "pie-title-test", None, group, &[], None),
                        );
                        output.textures_delta.clear();
                        let title = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::Shape::Text(text)
                                    if text.galley.job.text == group.label() =>
                                {
                                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                                }
                                _ => None,
                            })
                            .expect("The held menu must paint its canonical name");
                        let surface = title
                            .expand2(Vec2::new(theme::space::SM, theme::space::XS) * layout.scale);
                        assert!(viewport.contains_rect(surface));
                        assert!(surface.bottom() < layout.center.y - DEAD_ZONE * layout.scale);
                        for offset in [
                            Vec2::new(0.0, -1.0),
                            Vec2::new(-1.0, -1.0),
                            Vec2::new(1.0, -1.0),
                            Vec2::new(-1.0, 0.0),
                            Vec2::new(1.0, 0.0),
                            Vec2::new(-1.0, 1.0),
                            Vec2::new(1.0, 1.0),
                            Vec2::new(0.0, 1.0),
                        ] {
                            assert!(!surface.intersects(layout.item_rect(offset, group)));
                        }
                    }
                }
            }
        }
    }
}
