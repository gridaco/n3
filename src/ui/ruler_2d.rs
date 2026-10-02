//! Read-only 2D ruler strips, including animated alignment within 2D navigation.
#[cfg(test)]
use super::scalar_ticks::MAX_TICKS;
use super::scalar_ticks::{self, nice_step};
use crate::{
    camera::Camera, document::DisplayFrame, object_feedback::SELECTED_COLOR,
    orientation::display_rotation, theme, units::LengthUnit,
};
use egui::{Color32, FontId, Pos2, Rect, Stroke, Ui};
use glam::{DMat4, DVec3, Vec3};

pub const THICKNESS: f32 = theme::size::STEP_5;
// Minimum distance between numbered graduations; smaller values allow denser rulers.
const MIN_MAJOR_TICK_SPACING: f64 = 72.0;
const FONT_SIZE: f32 = theme::text::RULER_10;
const LABEL_INSET: f32 = theme::space::XS;
const LABEL_SEPARATION: f32 = theme::space::SM;

/// Space for viewport controls and input below the overlaid 2D ruler strips.
/// The scene continues to render and project against the full viewport.
pub fn content_rect(viewport: Rect) -> Rect {
    Rect::from_min_max(viewport.min + egui::Vec2::splat(THICKNESS), viewport.max)
}

#[derive(Clone, Debug)]
pub struct Tick {
    /// Absolute logical screen coordinate along this ruler.
    pub position: f32,
    #[allow(dead_code)] // Preserve the measured value alongside its formatted label.
    pub value: f64,
    /// Each tick is a labeled graduation.
    pub label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    /// Unclipped projected endpoints in absolute logical screen coordinates.
    pub start: f32,
    pub end: f32,
}

#[derive(Clone, Debug)]
pub struct AxisRuler {
    pub rect: Rect,
    pub ticks: Vec<Tick>,
    pub ranges: Vec<Span>,
    /// Projected document-world zero, possibly outside the visible strip.
    pub origin: f64,
    /// Positive pixels per displayed unit: labels increase to the right or down.
    pub pixels_per_unit: f64,
}

#[derive(Clone, Debug)]
pub struct Ruler2DModel {
    pub horizontal: AxisRuler,
    pub vertical: AxisRuler,
    pub unit: LengthUnit,
}

pub fn eligible(camera: &Camera, z_up: bool) -> bool {
    !camera.is_transitioning()
        && camera.is_orthographic()
        && crate::move_input::nudge_delta(camera, z_up, None, 1, 0, 1.0).is_some()
}

impl Ruler2DModel {
    /// `groups` contains one already-oriented display-space point group per
    /// selected object, or one combined group of selected edit-mode vertices.
    pub fn new(
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
        frame: &DisplayFrame,
        groups: &[Vec<Vec3>],
    ) -> Option<Self> {
        Self::build(
            viewport,
            camera,
            z_up,
            frame,
            groups,
            LengthUnit::Centimeters,
        )
    }

    /// Source coordinates are centimeters. The display unit changes ruler values
    /// and tick spacing; it never changes the projected origin or selected spans.
    pub fn new_in_unit(
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
        frame: &DisplayFrame,
        groups: &[Vec<Vec3>],
        unit: LengthUnit,
    ) -> Option<Self> {
        if unit == LengthUnit::Centimeters {
            Self::new(viewport, camera, z_up, frame, groups)
        } else {
            Self::build(viewport, camera, z_up, frame, groups, unit)
        }
    }

    fn build(
        viewport: Rect,
        camera: &Camera,
        z_up: bool,
        frame: &DisplayFrame,
        groups: &[Vec<Vec3>],
        unit: LengthUnit,
    ) -> Option<Self> {
        // The owner controls visibility from navigation mode. Orthographic
        // screen-plane distances remain valid during an animated axis change;
        // use the live projection so zero and selection spans follow the scene.
        if !camera.is_orthographic()
            || !viewport.is_finite()
            || viewport.width() <= THICKNESS
            || viewport.height() <= THICKNESS
            || !frame.scale.is_finite()
            || frame.scale <= 0.0
            || frame.center.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let matrix = camera
            .view_projection(viewport.width() / viewport.height())
            .as_dmat4();
        let zero = display_rotation(z_up)
            .as_dmat4()
            .transform_point3(frame.world_to_display(DVec3::ZERO));
        let (origin_x, origin_y) = project(matrix, viewport, zero)?;
        let centimeters_per_unit = unit.to_centimeters(1.0);
        let scale_x = matrix.row(0).truncate().length()
            * f64::from(viewport.width())
            * 0.5
            * frame.scale
            * centimeters_per_unit;
        let scale_y = matrix.row(1).truncate().length()
            * f64::from(viewport.height())
            * 0.5
            * frame.scale
            * centimeters_per_unit;
        if !scale_x.is_finite() || !scale_y.is_finite() || scale_x <= 0.0 || scale_y <= 0.0 {
            return None;
        }
        let content = content_rect(viewport);
        let mut horizontal = AxisRuler {
            rect: Rect::from_min_max(
                egui::pos2(content.left(), viewport.top()),
                content.right_top(),
            ),
            ticks: Vec::new(),
            ranges: Vec::new(),
            origin: origin_x,
            pixels_per_unit: scale_x,
        };
        let mut vertical = AxisRuler {
            rect: Rect::from_min_max(
                egui::pos2(viewport.left(), content.top()),
                content.left_bottom(),
            ),
            ticks: Vec::new(),
            ranges: Vec::new(),
            origin: origin_y,
            pixels_per_unit: scale_y,
        };
        horizontal.ticks = horizontal.make_ticks(content.left(), content.right());
        vertical.ticks = vertical.make_ticks(content.top(), content.bottom());
        for points in groups {
            let mut bounds = [
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ];
            let valid = !points.is_empty()
                && points.iter().all(|point| {
                    let Some((x, y)) = project(matrix, viewport, point.as_dvec3()) else {
                        return false;
                    };
                    bounds[0] = bounds[0].min(x);
                    bounds[1] = bounds[1].max(x);
                    bounds[2] = bounds[2].min(y);
                    bounds[3] = bounds[3].max(y);
                    true
                });
            if valid && bounds.iter().all(|value| (*value as f32).is_finite()) {
                horizontal.ranges.push(Span {
                    start: bounds[0] as f32,
                    end: bounds[1] as f32,
                });
                vertical.ranges.push(Span {
                    start: bounds[2] as f32,
                    end: bounds[3] as f32,
                });
            }
        }
        merge(&mut horizontal.ranges);
        merge(&mut vertical.ranges);
        Some(Self {
            horizontal,
            vertical,
            unit,
        })
    }
}

impl AxisRuler {
    /// Plan a projected linear axis independently of camera and document state.
    pub(crate) fn make_ticks(&self, start: f32, end: f32) -> Vec<Tick> {
        scalar_ticks::plan(
            f64::from(start),
            f64::from(end),
            self.origin,
            self.pixels_per_unit,
            MIN_MAJOR_TICK_SPACING,
            format_value,
        )
        .into_iter()
        .map(|tick| Tick {
            position: tick.position,
            value: tick.value,
            label: Some(tick.label),
        })
        .collect()
    }
}

fn project(matrix: DMat4, viewport: Rect, point: DVec3) -> Option<(f64, f64)> {
    let clip = matrix * point.extend(1.0);
    if !clip.is_finite() || clip.w <= 0.0 {
        return None;
    }
    let x =
        f64::from(viewport.left()) + (clip.x / clip.w + 1.0) * 0.5 * f64::from(viewport.width());
    let y =
        f64::from(viewport.top()) + (1.0 - clip.y / clip.w) * 0.5 * f64::from(viewport.height());
    (x.is_finite() && y.is_finite()).then_some((x, y))
}

fn format_value(value: f64, step: f64) -> String {
    if value == 0.0 {
        return "0".into();
    }
    let magnitude = value.abs();
    let decimals = decimal_places(step);
    if !(0.001..1.0e6).contains(&magnitude) {
        let precision =
            (magnitude.log10().floor() - step.log10().floor()).clamp(0.0, 15.0) as usize;
        format!("{value:.precision$e}")
    } else {
        let formatted = format!("{value:.decimals$}");
        if decimals == 0 {
            formatted
        } else {
            formatted
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_owned()
        }
    }
}

fn decimal_places(step: f64) -> usize {
    let mut decimals = (-step.log10().floor()).clamp(0.0, 15.0) as usize;
    while decimals < 15 {
        let scaled = step * 10.0_f64.powi(decimals as i32);
        if (scaled - scaled.round()).abs() <= scaled.abs() * 1e-10 {
            break;
        }
        decimals += 1;
    }
    decimals
}

fn merge(ranges: &mut Vec<Span>) {
    ranges.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut merged: Vec<Span> = Vec::with_capacity(ranges.len());
    for span in ranges.drain(..) {
        if let Some(previous) = merged.last_mut()
            && span.start <= previous.end
        {
            previous.end = previous.end.max(span.end);
        } else {
            merged.push(span);
        }
    }
    *ranges = merged;
}

/// Paint only: no widgets, hit testing, focus, or event consumption.
pub fn paint(ui: &Ui, model: &Ruler2DModel) {
    let visuals = ui.visuals();
    let background = visuals.panel_fill;
    let muted = visuals.weak_text_color();
    let border = visuals.widgets.noninteractive.bg_stroke.color;
    let corner = Rect::from_min_max(
        egui::pos2(model.vertical.rect.left(), model.horizontal.rect.top()),
        egui::pos2(model.horizontal.rect.left(), model.vertical.rect.top()),
    );
    let corner_painter = ui
        .painter()
        .with_clip_rect(ui.clip_rect().intersect(corner));
    corner_painter.rect_filled(corner, theme::radius::NONE, background);
    corner_painter.text(
        corner.center(),
        egui::Align2::CENTER_CENTER,
        model.unit.symbol(),
        FontId::monospace(FONT_SIZE),
        muted,
    );
    corner_painter.line_segment(
        [corner.right_top(), corner.right_bottom()],
        Stroke::new(1.0, border),
    );
    corner_painter.line_segment(
        [corner.left_bottom(), corner.right_bottom()],
        Stroke::new(1.0, border),
    );
    for (axis, horizontal) in [(&model.horizontal, true), (&model.vertical, false)] {
        let painter = ui
            .painter()
            .with_clip_rect(ui.clip_rect().intersect(axis.rect));
        painter.rect_filled(axis.rect, theme::radius::NONE, background);
        let (minimum, maximum) = if horizontal {
            (axis.rect.left(), axis.rect.right())
        } else {
            (axis.rect.top(), axis.rect.bottom())
        };
        let mut selected_labels: Vec<(Span, std::sync::Arc<egui::Galley>)> = Vec::new();
        for span in &axis.ranges {
            if span.end < minimum || span.start > maximum {
                continue;
            }
            let start = span.start.max(minimum);
            let end = span.end.min(maximum);
            let rect = if horizontal {
                Rect::from_min_max(
                    egui::pos2(start, axis.rect.top()),
                    egui::pos2(end, axis.rect.bottom()),
                )
            } else {
                Rect::from_min_max(
                    egui::pos2(axis.rect.left(), start),
                    egui::pos2(axis.rect.right(), end),
                )
            };
            painter.rect_filled(
                rect,
                theme::radius::NONE,
                SELECTED_COLOR.gamma_multiply(0.18),
            );
            for (position, at_start) in [(span.start, true), (span.end, false)] {
                if span.start == span.end && !at_start {
                    continue;
                }
                if !(minimum..=maximum).contains(&position) {
                    continue;
                }
                let step = nice_step(0.25 / axis.pixels_per_unit);
                let value = (f64::from(position) - axis.origin) / axis.pixels_per_unit;
                if !step.is_finite() || step <= 0.0 || !value.is_finite() {
                    continue;
                }
                let rounded = (value / step).round() * step;
                let label = format_value(if rounded.is_finite() { rounded } else { value }, step);
                let label = if label.contains('.') && !label.contains('e') {
                    label.trim_end_matches('0').trim_end_matches('.').to_owned()
                } else {
                    label
                };
                let galley =
                    painter.layout_no_wrap(label, FontId::monospace(FONT_SIZE), SELECTED_COLOR);
                let width = galley.size().x;
                let Some(label_span) =
                    endpoint_label_span(position, width, at_start, minimum, maximum)
                else {
                    continue;
                };
                if selected_labels
                    .iter()
                    .all(|(placed, _)| !overlaps(*placed, label_span, LABEL_SEPARATION))
                {
                    selected_labels.push((label_span, galley));
                }
            }
        }
        for tick in &axis.ticks {
            let length = theme::size::STEP_1;
            let (outer, inner) = if horizontal {
                (
                    egui::pos2(tick.position, axis.rect.bottom() - length),
                    egui::pos2(tick.position, axis.rect.bottom()),
                )
            } else {
                (
                    egui::pos2(axis.rect.right() - length, tick.position),
                    egui::pos2(axis.rect.right(), tick.position),
                )
            };
            painter.line_segment([outer, inner], Stroke::new(1.0, muted));
            if let Some(label) = &tick.label {
                let width = painter
                    .layout_no_wrap(label.clone(), FontId::monospace(FONT_SIZE), muted)
                    .size()
                    .x;
                let start = tick.position - width * 0.5;
                let span = Span {
                    start,
                    end: start + width,
                };
                let opacity = selected_labels
                    .iter()
                    .fold(1.0_f32, |opacity, (selected, _)| {
                        opacity.min(label_opacity_near_poi(span, *selected))
                    });
                let color = Color32::from_rgba_unmultiplied(
                    muted.r(),
                    muted.g(),
                    muted.b(),
                    (opacity * 255.0).round() as u8,
                );
                let galley =
                    painter.layout_no_wrap(label.clone(), FontId::monospace(FONT_SIZE), color);
                paint_label(&painter, axis.rect, horizontal, span, galley, color);
            }
        }
        for (span, galley) in selected_labels {
            paint_label(
                &painter,
                axis.rect,
                horizontal,
                span,
                galley,
                SELECTED_COLOR,
            );
        }
        let edge = if horizontal {
            [axis.rect.left_bottom(), axis.rect.right_bottom()]
        } else {
            [axis.rect.right_top(), axis.rect.right_bottom()]
        };
        painter.line_segment(edge, Stroke::new(1.0, border));
    }
}

fn overlaps(a: Span, b: Span, padding: f32) -> bool {
    a.start < b.end + padding && b.start < a.end + padding
}

/// Keep selection values on the unfilled side of their endpoint. If there is
/// insufficient ruler space, omit the value rather than place it on the fill.
fn endpoint_label_span(
    position: f32,
    width: f32,
    at_start: bool,
    minimum: f32,
    maximum: f32,
) -> Option<Span> {
    let gap = theme::space::XS;
    let start = if at_start {
        position - gap - width
    } else {
        position + gap
    };
    (start >= minimum + gap && start + width <= maximum - gap).then_some(Span {
        start,
        end: start + width,
    })
}

/// Regular labels remain visible, but recede smoothly near a selection value.
fn label_opacity_near_poi(label: Span, poi: Span) -> f32 {
    let gap = (poi.start - label.end).max(label.start - poi.end).max(0.0);
    0.18 + 0.82 * (gap / theme::size::STEP_6).clamp(0.0, 1.0)
}

fn paint_label(
    painter: &egui::Painter,
    rect: Rect,
    horizontal: bool,
    span: Span,
    galley: std::sync::Arc<egui::Galley>,
    color: Color32,
) {
    if horizontal {
        painter.galley(
            egui::pos2(span.start, rect.top() + LABEL_INSET),
            galley,
            color,
        );
    } else {
        painter.add(
            egui::epaint::TextShape::new(
                Pos2::new(rect.left() + LABEL_INSET, span.end),
                galley,
                color,
            )
            .with_angle(-std::f32::consts::FRAC_PI_2),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Transition, View};
    use std::time::Duration;

    fn viewport() -> Rect {
        Rect::from_min_size(egui::pos2(40.0, 70.0), egui::vec2(800.0, 600.0))
    }

    fn screen(camera: &Camera, viewport: Rect, point: Vec3) -> Pos2 {
        let ndc = camera
            .view_projection(viewport.width() / viewport.height())
            .project_point3(point);
        egui::pos2(
            viewport.left() + (ndc.x + 1.0) * viewport.width() * 0.5,
            viewport.top() + (1.0 - ndc.y) * viewport.height() * 0.5,
        )
    }

    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 0.001, "{a} differs from {b}");
    }

    #[test]
    fn selection_values_stay_outside_filled_ranges_and_fade_nearby_graduations() {
        let left = endpoint_label_span(101.0, 20.0, true, 0.0, 300.0).unwrap();
        let right = endpoint_label_span(201.0, 20.0, false, 0.0, 300.0).unwrap();
        assert!(left.end < 101.0 && right.start > 201.0);
        assert!(endpoint_label_span(8.0, 20.0, true, 0.0, 300.0).is_none());
        assert!(endpoint_label_span(292.0, 20.0, false, 0.0, 300.0).is_none());

        let nearby = Span {
            start: 76.0,
            end: 94.0,
        };
        let distant = Span {
            start: 20.0,
            end: 38.0,
        };
        assert!(label_opacity_near_poi(nearby, left) < 1.0);
        assert_eq!(label_opacity_near_poi(distant, left), 1.0);
        assert!(label_opacity_near_poi(left, left) > 0.0);
    }

    #[test]
    fn intermediate_major_steps_keep_fractional_labels() {
        assert_eq!(nice_step(1.2), 1.5);
        assert_eq!(format_value(1.5, 1.5), "1.5");
        assert_eq!(format_value(-10.5, 1.5), "-10.5");
        assert_eq!(format_value(3.0, 1.5), "3");
        assert_eq!(format_value(0.15, 0.15), "0.15");
    }

    #[test]
    fn display_units_convert_values_without_moving_origin_or_selection_in_any_view() {
        let frame = DisplayFrame {
            center: [2.0, -3.0, 4.0],
            scale: 0.25,
        };
        for view in [
            View::Front,
            View::Right,
            View::Top,
            View::Back,
            View::Left,
            View::Bottom,
        ] {
            let mut camera = Camera::default();
            camera.set_view(view);
            camera.pan(23.0, -17.0, viewport().height());
            camera.zoom(0.7);
            for z_up in [false, true] {
                let groups = vec![
                    vec![Vec3::new(-0.75, -0.5, -0.25), Vec3::ZERO],
                    vec![Vec3::new(0.25, 0.5, 0.75), Vec3::ONE],
                ];
                let centimeter =
                    Ruler2DModel::new(viewport(), &camera, z_up, &frame, &groups).unwrap();
                assert_eq!(centimeter.unit, LengthUnit::Centimeters);
                for unit in LengthUnit::ALL {
                    let model =
                        Ruler2DModel::new_in_unit(viewport(), &camera, z_up, &frame, &groups, unit)
                            .unwrap();
                    assert_eq!(model.unit, unit);
                    for (actual, source) in [
                        (&model.horizontal, &centimeter.horizontal),
                        (&model.vertical, &centimeter.vertical),
                    ] {
                        assert_eq!(actual.rect, source.rect);
                        assert_eq!(actual.origin, source.origin);
                        assert_eq!(actual.ranges, source.ranges);
                        close(
                            actual.pixels_per_unit,
                            source.pixels_per_unit * unit.to_centimeters(1.0),
                        );
                        assert!(!actual.ticks.is_empty());
                        assert!(actual.ticks.len() <= MAX_TICKS);
                        for tick in &actual.ticks {
                            let measured_centimeters =
                                (f64::from(tick.position) - source.origin) / source.pixels_per_unit;
                            close(unit.to_centimeters(tick.value), measured_centimeters);
                            assert!(tick.position.is_finite() && tick.value.is_finite());
                            if let Some(label) = &tick.label {
                                assert!(label.parse::<f64>().unwrap().is_finite());
                            }
                        }
                        assert!(
                            actual
                                .ranges
                                .iter()
                                .all(|span| { span.start.is_finite() && span.end.is_finite() })
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn fractional_centimeters_remain_fractional_and_each_unit_is_painted_in_the_corner() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        camera.zoom(2.0);
        let frame = DisplayFrame::default();
        let groups = [vec![Vec3::ZERO, Vec3::new(0.125, 0.25, 0.0)]];
        let model = Ruler2DModel::new(viewport(), &camera, false, &frame, &groups).unwrap();
        let right = model.horizontal.ranges[0].end;
        close(
            (f64::from(right) - model.horizontal.origin) / model.horizontal.pixels_per_unit,
            0.125,
        );
        assert!(
            model
                .horizontal
                .ticks
                .iter()
                .any(|tick| { tick.label.as_ref().is_some_and(|label| label.contains('.')) })
        );
        for unit in LengthUnit::ALL {
            let model =
                Ruler2DModel::new_in_unit(viewport(), &camera, false, &frame, &groups, unit)
                    .unwrap();
            close(
                (f64::from(model.horizontal.ranges[0].end) - model.horizontal.origin)
                    / model.horizontal.pixels_per_unit,
                unit.from_centimeters(0.125),
            );
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(900.0, 750.0))),
                    ..Default::default()
                },
                |root_ui| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(root_ui, |ui| paint(ui, &model));
                },
            );
            output.textures_delta.clear();
            let symbol = output.shapes.iter().find(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == unit.symbol())
            }).expect("display-unit symbol must be painted by the actual ruler");
            assert_eq!(symbol.clip_rect.width(), THICKNESS);
            assert_eq!(symbol.clip_rect.height(), THICKNESS);
            assert_eq!(symbol.clip_rect.left_top(), viewport().left_top());
            assert_eq!(
                symbol.clip_rect.right_bottom(),
                content_rect(viewport()).left_top()
            );
        }
    }

    #[test]
    fn all_six_views_measure_document_units_from_source_zero_in_both_up_modes() {
        let frame = DisplayFrame {
            center: [2.0, -3.0, 4.0],
            scale: 0.25,
        };
        for (view, y_right, y_up, z_right, z_up) in [
            (View::Front, DVec3::X, DVec3::Y, DVec3::X, DVec3::Z),
            (View::Right, DVec3::NEG_Z, DVec3::Y, DVec3::Y, DVec3::Z),
            (View::Back, DVec3::NEG_X, DVec3::Y, DVec3::NEG_X, DVec3::Z),
            (View::Left, DVec3::Z, DVec3::Y, DVec3::NEG_Y, DVec3::Z),
            (View::Top, DVec3::X, DVec3::NEG_Z, DVec3::X, DVec3::Y),
            (View::Bottom, DVec3::X, DVec3::Z, DVec3::X, DVec3::NEG_Y),
        ] {
            let mut camera = Camera::default();
            camera.set_view(view);
            for (z_up, right, up) in [(false, y_right, y_up), (true, z_right, z_up)] {
                let rect = viewport();
                let model = Ruler2DModel::new(rect, &camera, z_up, &frame, &[]).unwrap();
                let display = |point| {
                    display_rotation(z_up).transform_point3(frame.world_to_display(point).as_vec3())
                };
                let zero = screen(&camera, rect, display(DVec3::ZERO));
                let right = screen(&camera, rect, display(right * 2.0));
                let down = screen(&camera, rect, display(-up * 3.0));
                close(model.horizontal.origin, f64::from(zero.x));
                close(model.vertical.origin, f64::from(zero.y));
                close(
                    (f64::from(right.x) - model.horizontal.origin)
                        / model.horizontal.pixels_per_unit,
                    2.0,
                );
                close(
                    (f64::from(down.y) - model.vertical.origin) / model.vertical.pixels_per_unit,
                    3.0,
                );
                assert_eq!(model.horizontal.rect.top(), rect.top());
                assert_eq!(model.vertical.rect.left(), rect.left());
                assert_eq!(model.horizontal.rect.bottom(), content_rect(rect).top());
                assert_eq!(model.vertical.rect.right(), content_rect(rect).left());
                assert_eq!(model.horizontal.rect.height(), THICKNESS);
                assert_eq!(model.vertical.rect.width(), THICKNESS);
                for axis in [&model.horizontal, &model.vertical] {
                    assert!(axis.ranges.is_empty());
                    assert!(!axis.ticks.is_empty());
                    assert!(
                        axis.ticks
                            .windows(2)
                            .all(|pair| pair[0].position < pair[1].position
                                && pair[0].value < pair[1].value)
                    );
                    for tick in &axis.ticks {
                        close(
                            f64::from(tick.position),
                            axis.origin + tick.value * axis.pixels_per_unit,
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn pan_and_zoom_update_origin_and_scale_without_reframing_source_offsets() {
        let frame = DisplayFrame {
            center: [1.0e12, -2.0e12, 3.0e12],
            scale: 1.0e-12,
        };
        let mut camera = Camera::default();
        camera.set_view(View::Back);
        let rect = viewport();
        let before = Ruler2DModel::new(rect, &camera, true, &frame, &[]).unwrap();
        camera.pan(37.0, -23.0, rect.height());
        let panned = Ruler2DModel::new(rect, &camera, true, &frame, &[]).unwrap();
        close(panned.horizontal.origin - before.horizontal.origin, 37.0);
        close(panned.vertical.origin - before.vertical.origin, -23.0);
        assert_eq!(
            panned.horizontal.pixels_per_unit,
            before.horizontal.pixels_per_unit
        );
        camera.zoom(std::f32::consts::LN_2);
        let zoomed = Ruler2DModel::new(rect, &camera, true, &frame, &[]).unwrap();
        close(
            zoomed.horizontal.pixels_per_unit / panned.horizontal.pixels_per_unit,
            2.0,
        );
        close(
            zoomed.vertical.pixels_per_unit / panned.vertical.pixels_per_unit,
            2.0,
        );
        let zero =
            display_rotation(true).transform_point3(frame.world_to_display(DVec3::ZERO).as_vec3());
        let expected = screen(&camera, rect, zero);
        close(zoomed.horizontal.origin, f64::from(expected.x));
        close(zoomed.vertical.origin, f64::from(expected.y));
    }

    #[test]
    fn selection_groups_merge_only_overlap_and_preserve_gaps_points_and_offscreen_extents() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        let rect = viewport();
        let frame = DisplayFrame::default();
        let groups = vec![
            vec![Vec3::new(-0.5, 0.0, 0.0), Vec3::ZERO],
            vec![Vec3::new(0.25, 0.0, 0.0), Vec3::new(0.5, 0.0, 0.0)],
            vec![Vec3::new(0.4, 0.0, 0.0), Vec3::new(0.75, 0.0, 0.0)],
        ];
        let model = Ruler2DModel::new(rect, &camera, false, &frame, &groups).unwrap();
        assert_eq!(model.horizontal.ranges.len(), 2);
        close(
            f64::from(model.horizontal.ranges[0].start),
            f64::from(screen(&camera, rect, groups[0][0]).x),
        );
        close(
            f64::from(model.horizontal.ranges[0].end),
            f64::from(screen(&camera, rect, Vec3::ZERO).x),
        );
        close(
            f64::from(model.horizontal.ranges[1].end),
            f64::from(screen(&camera, rect, groups[2][1]).x),
        );
        let gap = model.horizontal.ranges[1].start - model.horizontal.ranges[0].end;
        close(f64::from(gap), model.horizontal.pixels_per_unit * 0.25);
        assert_eq!(
            model.vertical.ranges,
            [Span {
                start: rect.center().y,
                end: rect.center().y
            }]
        );
        let combined = vec![groups.into_iter().flatten().collect()];
        let combined = Ruler2DModel::new(rect, &camera, false, &frame, &combined).unwrap();
        assert_eq!(
            combined.horizontal.ranges.len(),
            1,
            "A selected-vertex group has one combined extent"
        );
        let single = Ruler2DModel::new(rect, &camera, false, &frame, &[vec![Vec3::ZERO]]).unwrap();
        assert_eq!(
            single.horizontal.ranges,
            [Span {
                start: rect.center().x,
                end: rect.center().x
            }]
        );
        let outside = Ruler2DModel::new(
            rect,
            &camera,
            false,
            &frame,
            &[vec![Vec3::X * 100.0, Vec3::X * 200.0]],
        )
        .unwrap();
        assert!(
            outside.horizontal.ranges[0].start > rect.right(),
            "Only painting clips the underlying interval"
        );
    }

    #[test]
    fn animated_orthographic_ruler_tracks_live_projection_and_selection() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        camera.set_view_with_transition(View::Right, Transition::default());
        camera.advance_transition(Duration::from_millis(60));
        let rect = viewport();
        let points = vec![Vec3::ZERO, Vec3::X, Vec3::Y];
        let model = Ruler2DModel::new(
            rect,
            &camera,
            false,
            &DisplayFrame::default(),
            std::slice::from_ref(&points),
        )
        .unwrap();
        let matrix = camera
            .view_projection(rect.width() / rect.height())
            .as_dmat4();
        let projected: Vec<_> = points
            .iter()
            .map(|point| project(matrix, rect, point.as_dvec3()).unwrap())
            .collect();
        assert!((model.horizontal.origin - projected[0].0).abs() < 1e-5);
        assert!((model.vertical.origin - projected[0].1).abs() < 1e-5);
        assert!(
            (f64::from(model.horizontal.ranges[0].end)
                - projected
                    .iter()
                    .map(|point| point.0)
                    .fold(f64::NEG_INFINITY, f64::max))
            .abs()
                < 1e-4
        );
        assert!(
            (f64::from(model.vertical.ranges[0].start)
                - projected
                    .iter()
                    .map(|point| point.1)
                    .fold(f64::INFINITY, f64::min))
            .abs()
                < 1e-4
        );
        assert!(
            camera.is_transitioning(),
            "Measurement must not settle the camera"
        );
    }

    #[test]
    fn settled_alignment_excludes_oblique_animation_and_cancelled_projection_blends() {
        let mut camera = Camera::default();
        assert!(!eligible(&camera, false));
        camera.set_view(View::Front);
        assert!(eligible(&camera, false));
        camera.orbit(0.001, 0.0);
        assert!(!eligible(&camera, false));
        camera.set_view(View::Front);
        camera.set_view_with_transition(View::Right, Transition::default());
        assert!(
            !eligible(&camera, false),
            "An aligned starting pose still has a running transition"
        );
        camera.finish_transition();
        assert!(eligible(&camera, false));
        camera.set_view(View::Perspective);
        camera.set_view_with_transition(
            View::Front,
            Transition::Animated {
                duration: Duration::from_secs(1),
            },
        );
        camera.advance_transition(Duration::from_millis(500));
        camera.cancel_transition();
        assert!(!eligible(&camera, false));
        assert!(
            Ruler2DModel::new(viewport(), &camera, false, &DisplayFrame::default(), &[]).is_none()
        );
    }

    #[test]
    fn extreme_units_and_invalid_inputs_never_create_nonfinite_or_unbounded_ticks() {
        let mut camera = Camera::default();
        camera.set_view(View::Front);
        for scale in [1.0e-300, 1.0e-100, 1.0, 1.0e100, 1.0e300] {
            for rect in [
                viewport(),
                Rect::from_min_size(Pos2::ZERO, egui::vec2(1.0e8, 600.0)),
            ] {
                let frame = DisplayFrame {
                    center: [0.0; 3],
                    scale,
                };
                let model = Ruler2DModel::new(rect, &camera, false, &frame, &[]).unwrap();
                for (axis, minimum, maximum) in [
                    (&model.horizontal, rect.left(), rect.right()),
                    (&model.vertical, rect.top(), rect.bottom()),
                ] {
                    assert!(axis.ticks.len() <= MAX_TICKS);
                    assert!(axis.ticks.iter().all(|tick| tick.position.is_finite()
                        && tick.value.is_finite()
                        && (minimum..=maximum).contains(&tick.position)));
                }
            }
        }
        for frame in [
            DisplayFrame {
                center: [f64::NAN, 0.0, 0.0],
                scale: 1.0,
            },
            DisplayFrame {
                center: [0.0; 3],
                scale: 0.0,
            },
            DisplayFrame {
                center: [f64::MAX; 3],
                scale: f64::MAX,
            },
        ] {
            assert!(Ruler2DModel::new(viewport(), &camera, false, &frame, &[]).is_none());
        }
        assert!(
            Ruler2DModel::new(
                Rect::from_min_size(Pos2::ZERO, egui::vec2(0.5, 100.0)),
                &camera,
                false,
                &DisplayFrame::default(),
                &[]
            )
            .is_none()
        );
    }
}
