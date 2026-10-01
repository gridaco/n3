//! Painting and live, visible identities use the same geometry as hit testing.
use super::*;
use crate::ui::scalar_ticks;
use egui::{Align2, Color32, FontId, Stroke, StrokeKind};

pub(super) fn time_label(value: f64, step: f64) -> String {
    if value == 0.0 {
        return "0 s".into();
    }
    if value.abs() >= 1e7 || step < 1e-6 {
        return format!("{value:.3e} s");
    }
    let decimals = (-step.log10().floor()).max(0.0) as usize
        + usize::from(step * 10f64.powf(-step.log10().floor()) % 1.0 > 0.001);
    format!("{value:.precision$} s", precision = decimals.min(7))
}

impl Timeline {
    #[expect(
        clippy::too_many_arguments,
        reason = "Shared visible geometry and immutable presentation inputs"
    )]
    pub(super) fn paint(
        &self,
        ui: &mut Ui,
        id: Id,
        rect: Rect,
        ruler: Rect,
        keys: Rect,
        rows: &[RowLayout],
        markers: &[Marker],
        data: &Data,
        host: &HostState,
        cap: &Capabilities,
        out: &mut Output,
    ) {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        let muted = ui.visuals().weak_text_color();
        let line = ui.visuals().widgets.noninteractive.bg_stroke;
        let accent = ui.visuals().selection.stroke.color;
        let text = ui.visuals().text_color();
        let font = FontId::proportional(theme::text::SMALL_UI_11);
        let range = self.visible.unwrap();
        let hover_enabled = !self.is_marquee_active() && host.valid() && cap.inspect;
        let prepared = self.prepared.as_ref().unwrap();
        painter.rect_stroke(rect, theme::radius::SM, line, StrokeKind::Inside);
        painter.line_segment(
            [
                egui::pos2(rect.left(), ruler.bottom()),
                ruler.right_bottom(),
            ],
            line,
        );
        painter.line_segment(
            [egui::pos2(keys.left(), rect.top()), keys.left_bottom()],
            line,
        );
        // Native pointer capture also marks this surface as an input owner for
        // hosts composing it alongside unrelated controls or a viewport. The
        // transport's native buttons and fields are excluded from both surfaces.
        let body = Rect::from_min_max(egui::pos2(rect.left(), keys.top()), rect.max);
        let canvas = ui.interact(body, id.with("canvas"), egui::Sense::click_and_drag());
        canvas.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, "Read-only timeline")
        });
        out.observations.push(Observation {
            target: Target::Canvas,
            id: canvas.id,
            rect: rect.intersect(ui.clip_rect()),
            enabled: true,
        });
        let ruler_response = ui.interact(ruler, id.with("ruler"), egui::Sense::click_and_drag());
        ruler_response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Other,
                host.valid() && cap.seek,
                "Time ruler",
            )
        });
        out.observations.push(Observation {
            target: Target::Ruler,
            id: ruler_response.id,
            rect: ruler.intersect(ui.clip_rect()),
            enabled: host.valid() && cap.seek,
        });
        // Selection spans the label and keys. Paint it beneath the grid and
        // markers so navigation never erases the timestamp structure.
        let responses: Vec<_> = rows
            .iter()
            .map(|row| {
                let track = &data.tracks[row.track];
                let response = ui.interact(
                    row.label,
                    id.with(Target::Row(track.id).name()),
                    egui::Sense::hover(),
                );
                let selected = self.selected_tracks.contains(&track.id);
                if selected || (hover_enabled && response.hovered()) {
                    let color = if selected {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals()
                            .widgets
                            .hovered
                            .weak_bg_fill
                            .gamma_multiply(0.35)
                    };
                    painter.rect_filled(row.label.union(row.keys), 0.0, color);
                }
                response
            })
            .collect();
        let scale = f64::from(keys.width()) / range.duration();
        let origin = f64::from(keys.left()) - range.start * scale;
        let ticks = scalar_ticks::plan(
            f64::from(keys.left()),
            f64::from(keys.right()),
            origin,
            scale,
            64.0,
            time_label,
        );
        let axis = painter.with_clip_rect(ruler.intersect(ui.clip_rect()));
        for tick in ticks {
            painter.line_segment(
                [
                    egui::pos2(tick.position, keys.top()),
                    egui::pos2(tick.position, keys.bottom()),
                ],
                Stroke::new(1.0, line.color.gamma_multiply(0.45)),
            );
            axis.line_segment(
                [
                    egui::pos2(tick.position, ruler.bottom() - 4.0),
                    egui::pos2(tick.position, ruler.bottom()),
                ],
                line,
            );
            let galley = axis.layout_no_wrap(tick.label, font.clone(), muted);
            let half = galley.size().x * 0.5;
            let x = if ruler.width() >= galley.size().x {
                tick.position
                    .clamp(ruler.left() + half, ruler.right() - half)
            } else {
                tick.position
            };
            axis.galley(
                egui::pos2(x - half, ruler.center().y - galley.size().y * 0.5 - 2.0),
                galley,
                muted,
            );
        }
        for (row, response) in rows.iter().zip(responses) {
            let track = &data.tracks[row.track];
            let target = Target::Row(track.id);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Other, true, &track.label)
            });
            let has_children = prepared.has_children(row.track);
            let label_x = row.label.left()
                + 8.0
                + (row.depth as f32 * 12.0).min((row.label.width() - 40.0).max(0.0));
            if row.depth > 0 {
                let guide = painter.with_clip_rect(row.label);
                let x = label_x - 6.0;
                guide.line_segment(
                    [egui::pos2(x, row.label.top()), egui::pos2(x, row.center)],
                    Stroke::new(1.0, line.color.gamma_multiply(0.65)),
                );
                guide.line_segment(
                    [egui::pos2(x, row.center), egui::pos2(label_x, row.center)],
                    Stroke::new(1.0, line.color.gamma_multiply(0.65)),
                );
            }
            if has_children {
                let text: egui::WidgetText =
                    crate::ui::lucide::Icon::ChevronRight.text(11.0).into();
                let galley = text.into_galley(ui, None, 11.0, egui::FontSelection::Default);
                let pos = egui::pos2(label_x + 5.0, row.center) - galley.size() * 0.5;
                let angle = if self.collapsed.contains(&track.id) {
                    0.0
                } else {
                    std::f32::consts::FRAC_PI_2
                };
                painter.with_clip_rect(row.label).add(
                    egui::epaint::TextShape::new(pos, galley, muted)
                        .with_angle_and_anchor(angle, Align2::CENTER_CENTER),
                );
            }
            let label_rect = Rect::from_min_max(
                egui::pos2(label_x + 14.0, row.label.top()),
                row.label.max - egui::vec2(6.0, 0.0),
            );
            let mut job = egui::text::LayoutJob::simple(
                track.label.clone(),
                if has_children {
                    FontId::new(
                        theme::text::SMALL_UI_11,
                        crate::ui::typography::semibold_family(),
                    )
                } else {
                    font.clone()
                },
                if row.depth == 0 { text } else { muted },
                label_rect.width().max(0.0),
            );
            job.wrap = egui::text::TextWrapping::truncate_at_width(label_rect.width().max(0.0));
            let galley = ui.painter().layout_job(job);
            let pos = egui::pos2(label_rect.left(), row.center - galley.size().y * 0.5);
            painter.with_clip_rect(label_rect).galley(
                pos,
                galley,
                if row.depth == 0 { text } else { muted },
            );
            if hover_enabled {
                response.on_hover_text(&track.label);
            }
            painter.line_segment(
                [row.label.left_bottom(), row.keys.right_bottom()],
                Stroke::new(1.0, line.color.gamma_multiply(0.3)),
            );
            out.observations.push(Observation {
                target,
                id: id.with(Target::Row(track.id).name()),
                rect: row.label.intersect(ui.clip_rect()),
                enabled: true,
            });
        }
        for marker in markers {
            let track = &data.tracks[marker.track];
            let ordered = prepared.key_indices(marker.track);
            let first = &track.keys[ordered[marker.indices.start]];
            let last = &track.keys[ordered[marker.indices.end - 1]];
            let count = marker.indices.len();
            let target = if count == 1 {
                Target::Key {
                    track: track.id,
                    key: first.id,
                }
            } else {
                Target::Cluster {
                    track: track.id,
                    first: first.id,
                    last: last.id,
                }
            };
            let response = ui.interact(marker.rect, id.with(target.name()), egui::Sense::hover());
            let selected = ordered[marker.indices.clone()].iter().any(|&k| {
                self.selected_keys.contains(&KeyRef {
                    track: track.id,
                    key: track.keys[k].id,
                })
            });
            let color = if selected || (hover_enabled && response.hovered()) {
                accent
            } else {
                muted
            };
            if count == 1 {
                let center = marker.center;
                let r = MARKER * 0.5;
                painter
                    .with_clip_rect(marker.rect)
                    .add(egui::Shape::convex_polygon(
                        vec![
                            center - Vec2::Y * r,
                            center + Vec2::X * r,
                            center + Vec2::Y * r,
                            center - Vec2::X * r,
                        ],
                        color,
                        Stroke::NONE,
                    ));
            } else {
                painter.rect_filled(
                    marker.rect.shrink2(egui::vec2(0.0, 1.0)),
                    theme::radius::SM,
                    color.gamma_multiply(0.35),
                );
                // A capsule is an aggregate, never a fabricated timestamp/key.
                painter.line_segment(
                    [
                        marker.rect.left_center() + Vec2::X * 3.0,
                        marker.rect.right_center() - Vec2::X * 3.0,
                    ],
                    Stroke::new(2.0, color),
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Other,
                    host.valid() && cap.inspect,
                    if count == 1 {
                        format!("{}: key at {} seconds", track.label, first.time)
                    } else {
                        format!("{}: cluster of {count} keys", track.label)
                    },
                )
            });
            if hover_enabled {
                response.on_hover_ui(|ui| {
                ui.label(&track.label);
                if count==1 {
                    ui.label(format!("{} s",first.time));
                    if let Some(metadata)=&first.metadata {ui.label(metadata);}
                } else {
                    ui.label(format!("{count} distinct keys · {}…{} s",first.time,last.time));
                    ui.weak("Click selects all underlying keys for inspection. Zoom in to distinguish them.");
                }
            });
            }
            let bounds = marker.rect.intersect(ui.clip_rect());
            if bounds.is_positive() {
                out.observations.push(Observation {
                    target,
                    id: id.with(
                        if count == 1 {
                            Target::Key {
                                track: track.id,
                                key: first.id,
                            }
                        } else {
                            Target::Cluster {
                                track: track.id,
                                first: first.id,
                                last: last.id,
                            }
                        }
                        .name(),
                    ),
                    rect: bounds,
                    enabled: host.valid() && cap.inspect,
                });
            }
        }
        if host.accepted_time.is_finite() {
            let x = navigation::x_at(range, keys.left(), keys.width(), host.accepted_time);
            if x >= keys.left() && x <= keys.right() {
                painter.line_segment(
                    [egui::pos2(x, ruler.top()), egui::pos2(x, keys.bottom())],
                    Stroke::new(1.0, accent),
                );
                painter
                    .with_clip_rect(ruler)
                    .add(egui::Shape::convex_polygon(
                        vec![
                            egui::pos2(x - 6.0, ruler.top() + 1.0),
                            egui::pos2(x + 6.0, ruler.top() + 1.0),
                            egui::pos2(x + 6.0, ruler.top() + 6.0),
                            egui::pos2(x, ruler.top() + 10.0),
                            egui::pos2(x - 6.0, ruler.top() + 6.0),
                        ],
                        accent,
                        Stroke::NONE,
                    ));
            }
        }
        if let Some(gesture) = self.box_select.as_ref().filter(|gesture| gesture.dragging) {
            marquee::paint(
                &painter.with_clip_rect(keys),
                marquee::rectangle(gesture.start, gesture.current, keys),
            );
        }
        if data.tracks.is_empty() {
            painter.text(
                keys.center(),
                Align2::CENTER_CENTER,
                "No tracks",
                FontId::proportional(theme::text::UI_BODY_13),
                muted,
            );
        } else if prepared.total_keys == 0 {
            painter.text(
                keys.center(),
                Align2::CENTER_CENTER,
                "No keys",
                FontId::proportional(theme::text::UI_BODY_13),
                muted,
            );
        }
        if !host.valid() {
            painter.rect_filled(body, 0.0, Color32::from_black_alpha(24));
        }
    }
}
