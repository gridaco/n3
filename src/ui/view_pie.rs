//! Pure view-pie geometry and painting. The host owns the held key, pointer
//! lifecycle, cancellation, and dispatch; painting never consumes an event.
use std::f32::consts::FRAC_PI_4;

use egui::{Context, Id, Pos2, Rect, Vec2};

#[cfg(test)]
use super::pie::DEAD_ZONE;
use super::pie::{Item, Layout, RADIUS};

use crate::{camera::View, controls::Control, shortcuts::Command};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Top,
    Front,
    Back,
    Left,
    Right,
    Selection,
    Bottom,
}

impl Action {
    pub const ALL: [Self; 7] = [
        Self::Top,
        Self::Front,
        Self::Back,
        Self::Left,
        Self::Right,
        Self::Selection,
        Self::Bottom,
    ];

    pub fn command(self) -> Command {
        match self {
            Self::Top => Command::View(View::Top),
            Self::Front => Command::View(View::Front),
            Self::Back => Command::View(View::Back),
            Self::Left => Command::View(View::Left),
            Self::Right => Command::View(View::Right),
            Self::Selection => Command::FrameSelection,
            Self::Bottom => Command::View(View::Bottom),
        }
    }

    pub fn control(self) -> Control {
        match self {
            Self::Top => Control::PieTop,
            Self::Front => Control::PieFront,
            Self::Back => Control::PieBack,
            Self::Left => Control::PieLeft,
            Self::Right => Control::PieRight,
            Self::Selection => Control::PieSelection,
            Self::Bottom => Control::PieBottom,
        }
    }

    fn offset(self) -> Vec2 {
        match self {
            Self::Top => Vec2::new(0.0, -1.0),
            Self::Front => Vec2::new(-1.0, -1.0),
            Self::Back => Vec2::new(1.0, -1.0),
            Self::Left => Vec2::new(-1.0, 0.0),
            Self::Right => Vec2::new(1.0, 0.0),
            Self::Selection => Vec2::new(1.0, 1.0),
            Self::Bottom => Vec2::new(0.0, 1.0),
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::Selection => {
                "Center and fit the selected objects or vertices without changing the viewing direction."
            }
            Self::Top => "View from above in orthographic projection.",
            Self::Front => "View from the front in orthographic projection.",
            Self::Back => "View from the back in orthographic projection.",
            Self::Left => "View from the left in orthographic projection.",
            Self::Right => "View from the right in orthographic projection.",
            Self::Bottom => "View from below in orthographic projection.",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Pie {
    layout: Layout,
    instance: Option<Id>,
}

impl Pie {
    pub fn open(ctx: &Context, anchor: Pos2, bounds: Rect) -> Option<Self> {
        Self::open_instance(ctx, None, anchor, bounds)
    }

    /// Independent consumers provide a stable ID; the host still owns the
    /// held key, cancellation and action dispatch.
    pub fn open_with_id(ctx: &Context, id: Id, anchor: Pos2, bounds: Rect) -> Option<Self> {
        Self::open_instance(ctx, Some(id), anchor, bounds)
    }

    fn open_instance(
        ctx: &Context,
        instance: Option<Id>,
        anchor: Pos2,
        bounds: Rect,
    ) -> Option<Self> {
        Some(Self {
            layout: Layout::open(
                anchor,
                bounds,
                RADIUS,
                Layout::measure_cards(ctx, Action::ALL.into_iter().map(Action::control)),
            )?,
            instance,
        })
    }

    #[cfg(test)]
    pub fn anchor(&self) -> Pos2 {
        self.layout.anchor()
    }

    pub fn center(&self) -> Pos2 {
        self.layout.center
    }

    pub fn item_rect(&self, action: Action) -> Rect {
        self.layout.item_rect(action.offset(), action.control())
    }

    /// Cards are direct targets; the surrounding space uses the nearest of
    /// eight circular directions. Southwest deliberately has no action.
    pub fn hovered(&self, point: Pos2, fit_enabled: bool) -> Option<Action> {
        if !self.layout.accepts(point) {
            return None;
        }
        let action = Action::ALL
            .into_iter()
            .find(|action| self.item_rect(*action).contains(point))
            .or_else(|| {
                let delta = self.layout.normalized_delta(point - self.center());
                let sector = (delta.y.atan2(delta.x) / FRAC_PI_4).round() as i32;
                match sector.rem_euclid(8) {
                    0 => Some(Action::Right),
                    1 => Some(Action::Selection),
                    2 => Some(Action::Bottom),
                    3 => None,
                    4 => Some(Action::Left),
                    5 => Some(Action::Front),
                    6 => Some(Action::Top),
                    7 => Some(Action::Back),
                    _ => unreachable!(),
                }
            })?;
        (action != Action::Selection || fit_enabled).then_some(action)
    }

    pub fn paint(&self, ctx: &Context, pointer: Option<Pos2>, fit_enabled: bool) {
        let hovered = pointer.and_then(|point| self.hovered(point, fit_enabled));
        let items = Action::ALL.map(|action| Item {
            control: action.control(),
            offset: action.offset(),
            enabled: action != Action::Selection || fit_enabled,
            hovered: hovered == Some(action),
            selected: false,
            sector_half_angle: std::f32::consts::FRAC_PI_8,
            tooltip: Some(action.tooltip()),
        });
        self.layout.paint(
            ctx,
            "n3.view_pie",
            self.instance,
            Control::ViewPie,
            &items,
            pointer,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls;

    fn open(anchor: Pos2, bounds: Rect) -> Option<Pie> {
        let ctx = Context::default();
        let mut pie = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            pie = Pie::open(ui.ctx(), anchor, bounds);
        })
        .textures_delta
        .clear();
        pie
    }

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))
    }

    #[test]
    fn all_sectors_and_card_rectangles_match_the_seven_actions() {
        let pie = open(viewport().center(), viewport()).unwrap();
        for action in Action::ALL {
            let card = pie.item_rect(action);
            let inward_edge = match action {
                Action::Top => card.center_bottom(),
                Action::Bottom => card.center_top(),
                Action::Front | Action::Left => card.right_center(),
                Action::Back | Action::Right | Action::Selection => card.left_center(),
            };
            assert!((inward_edge.distance(pie.center()) - RADIUS.x).abs() < 0.001);
            assert_eq!(
                pie.hovered(pie.item_rect(action).center(), true),
                Some(action)
            );
            let sector_point = pie.center() + action.offset() * RADIUS * 2.0;
            assert_eq!(pie.hovered(sector_point, true), Some(action));
            // Labels remain direct targets even near a geometric sector border.
            for point in [
                pie.item_rect(action).left_center(),
                pie.item_rect(action).right_center(),
            ] {
                assert_eq!(pie.hovered(point, true), Some(action));
            }
        }
        assert_eq!(
            pie.hovered(pie.center() + Vec2::new(-RADIUS.x, RADIUS.y), true),
            None
        );
        assert_eq!(
            pie.hovered(pie.center() + Vec2::new(-RADIUS.x, RADIUS.y) * 2.0, true),
            None
        );
    }

    #[test]
    fn dead_zone_disabled_selection_and_outside_viewport_cancel() {
        let pie = open(viewport().center(), viewport()).unwrap();
        for point in [
            pie.anchor(),
            pie.anchor() + Vec2::X * DEAD_ZONE,
            Pos2::new(-1.0, 400.0),
            Pos2::new(f32::NAN, 10.0),
        ] {
            assert_eq!(pie.hovered(point, true), None);
        }
        assert_eq!(
            pie.hovered(pie.anchor() + Vec2::X * (DEAD_ZONE + 1.0), true),
            Some(Action::Right)
        );
        assert_eq!(
            pie.hovered(pie.item_rect(Action::Selection).center(), false),
            None
        );
        assert_eq!(
            pie.hovered(pie.item_rect(Action::Front).center(), false),
            Some(Action::Front)
        );
    }

    #[test]
    fn edge_layouts_keep_cards_inside_without_moving_the_cancel_anchor() {
        for viewport in [
            viewport(),
            Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(200.0, 120.0)),
        ] {
            for anchor in [
                viewport.left_top(),
                viewport.right_top(),
                viewport.left_bottom(),
                viewport.right_bottom(),
                viewport.center(),
            ] {
                let pie = open(anchor, viewport).unwrap();
                assert_eq!(pie.anchor(), anchor);
                assert_eq!(pie.hovered(anchor, true), None);
                for (index, action) in Action::ALL.into_iter().enumerate() {
                    let rect = pie.item_rect(action);
                    assert!(
                        viewport.contains_rect(rect),
                        "{viewport:?} does not contain {rect:?}"
                    );
                    for other in &Action::ALL[index + 1..] {
                        assert!(
                            !rect.intersects(pie.item_rect(*other)),
                            "Pie labels must never overlap"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn opening_in_different_viewports_reflows_layout_and_rejects_invalid_geometry() {
        let anchor = Pos2::new(100.0, 80.0);
        let original = open(anchor, viewport()).unwrap();
        let smaller = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        let resized = open(anchor, smaller).unwrap();
        assert_ne!(resized.center(), original.center());
        assert_eq!(resized.anchor(), anchor);
        assert_eq!(resized.hovered(anchor, true), None);
        for action in Action::ALL {
            assert!(smaller.contains_rect(resized.item_rect(action)));
        }
        assert!(open(anchor, Rect::ZERO).is_none());
        assert!(open(Pos2::new(-1.0, -1.0), viewport()).is_none());
        assert!(open(Pos2::ZERO, Rect::NOTHING).is_none());
        assert!(open(Pos2::new(f32::NAN, 0.0), viewport()).is_none());
        assert!(
            open(
                Pos2::ZERO,
                Rect::from_min_max(Pos2::new(-f32::MAX, 0.0), Pos2::new(f32::MAX, 200.0))
            )
            .is_none()
        );
    }

    #[test]
    fn painting_records_catalog_controls_and_disabled_fit_without_taking_input() {
        let ctx = Context::default();
        controls::enable(&ctx);
        let pie = open(viewport().center(), viewport()).unwrap();
        let point = pie.item_rect(Action::Front).center();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport()),
                events: vec![egui::Event::PointerMoved(point)],
                ..Default::default()
            },
            |root_ui| {
                let context = root_ui.ctx().clone();
                let ctx = &context;
                controls::begin_pass(ctx);
                let events = ctx.input(|input| input.events.clone());
                pie.paint(ctx, Some(point), false);
                assert_eq!(ctx.input(|input| input.events.clone()), events);
            },
        );
        output.textures_delta.clear();
        assert!(!output.shapes.is_empty());
        assert_eq!(
            output.platform_output.cursor_icon,
            egui::CursorIcon::PointingHand
        );
        let trace = controls::snapshot(&ctx);
        trace.validate().unwrap();
        assert!(trace.get(Control::ViewPie).unwrap().parents.is_empty());
        for action in Action::ALL {
            let observed = trace.get(action.control()).unwrap();
            assert_eq!(observed.parents, vec![Control::ViewPie]);
            assert_eq!(observed.rect, pie.item_rect(action));
            assert_eq!(observed.label, action.control().label());
            assert_eq!(observed.enabled, action != Action::Selection);
        }
    }

    #[test]
    fn cancelled_and_disabled_targets_reset_the_cursor_without_taking_input() {
        let ctx = Context::default();
        let pie = open(viewport().center(), viewport()).unwrap();
        for point in [
            None,
            Some(pie.anchor()),
            Some(pie.item_rect(Action::Selection).center()),
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport()),
                    ..Default::default()
                },
                |root_ui| {
                    let context = root_ui.ctx().clone();
                    let ctx = &context;
                    ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                    pie.paint(ctx, point, false);
                },
            );
            output.textures_delta.clear();
            assert_eq!(
                output.platform_output.cursor_icon,
                egui::CursorIcon::Default
            );
        }
    }

    #[test]
    fn actions_only_dispatch_the_six_views_or_fit_selection() {
        for action in Action::ALL {
            match (action, action.command()) {
                (Action::Top, Command::View(View::Top))
                | (Action::Front, Command::View(View::Front))
                | (Action::Back, Command::View(View::Back))
                | (Action::Left, Command::View(View::Left))
                | (Action::Right, Command::View(View::Right))
                | (Action::Bottom, Command::View(View::Bottom))
                | (Action::Selection, Command::FrameSelection) => {}
                _ => panic!("Unexpected pie command"),
            }
        }
    }

    #[test]
    fn ring_segment_follows_pointer_angle_in_occupied_and_empty_sectors() {
        let ctx = Context::default();
        let pie = open(viewport().center(), viewport()).unwrap();
        for delta in [
            Vec2::new(100.0, -30.0),
            Vec2::new(100.0, 30.0),
            Vec2::new(-100.0, 100.0),
        ] {
            let pointer = pie.center() + delta;
            assert_eq!(
                pie.hovered(pointer, true),
                (delta.x > 0.0).then_some(Action::Right)
            );
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport()),
                    ..Default::default()
                },
                |ui| pie.paint(ui.ctx(), Some(pointer), true),
            );
            output.textures_delta.clear();
            let arcs: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) if !path.closed => Some(path),
                    _ => None,
                })
                .collect();
            assert_eq!(
                arcs.len(),
                1,
                "The direction marker remains present even in an empty sector"
            );
            let midpoint = arcs[0].points[arcs[0].points.len() / 2];
            assert!(((midpoint - pie.center()).normalized() - delta.normalized()).length() < 1e-5);
            let ring = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) if path.closed => Some(path),
                    _ => None,
                })
                .expect("The base ring uses the same path stroke geometry as its highlight");
            assert_eq!(ring.stroke.width, arcs[0].stroke.width);
            assert_eq!(ring.stroke.kind, arcs[0].stroke.kind);
            let radius = ring.points[0].distance(pie.center());
            assert!(
                arcs[0]
                    .points
                    .iter()
                    .all(|point| (point.distance(pie.center()) - radius).abs() < 0.001)
            );
        }
    }

    #[test]
    fn sector_help_waits_then_follows_the_active_choice_without_card_hover() {
        let ctx = Context::default();
        let pie = open(viewport().center(), viewport()).unwrap();
        let front = pie.center() + Vec2::new(-40.0, -40.0);
        let back = pie.center() + Vec2::new(40.0, -40.0);
        assert!(!pie.item_rect(Action::Front).contains(front));
        assert!(!pie.item_rect(Action::Back).contains(back));
        for (time, point, expected) in [
            (0.0, front, None),
            (0.2, front + Vec2::new(-5.0, -5.0), None),
            // egui measures a new tooltip Area on its first eligible frame.
            (0.75, front, None),
            (0.9, front, Some(Action::Front.tooltip())),
            (1.0, back, None),
            (1.2, back, None),
            (1.75, back, None),
            (1.9, back, Some(Action::Back.tooltip())),
            (2.0, pie.center(), None),
            (2.1, front, None),
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport()),
                    time: Some(time),
                    events: vec![egui::Event::PointerMoved(point)],
                    ..Default::default()
                },
                |ui| pie.paint(ui.ctx(), Some(point), true),
            );
            output.textures_delta.clear();
            let help = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.job.text.contains("orthographic projection") =>
                {
                    Some(text.galley.job.text.as_str())
                }
                _ => None,
            });
            assert_eq!(help, expected, "Tooltip mismatch at {time}");
            assert!(
                !output
                    .shapes
                    .iter()
                    .any(|shape| matches!(shape.shape, egui::Shape::LineSegment { .. })),
                "Pie choices have no connector lines"
            );
            for shape in &output.shapes {
                if let egui::Shape::Circle(circle) = &shape.shape {
                    assert_eq!(
                        circle.fill,
                        egui::Color32::TRANSPARENT,
                        "The center is a hollow ring"
                    );
                }
            }
        }
    }

    #[test]
    fn independent_instances_keep_bounds_and_delayed_tooltip_identity_across_layout_retries() {
        let ctx = Context::default();
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0));
        let left = Rect::from_min_size(Pos2::ZERO, Vec2::new(580.0, 800.0));
        let right = Rect::from_min_size(Pos2::new(620.0, 0.0), left.size());
        let ids = [Id::new("view-pie-left"), Id::new("view-pie-right")];
        let mut pies = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                time: Some(0.0),
                ..Default::default()
            },
            |ui| {
                pies = Some([
                    Pie::open_with_id(ui.ctx(), ids[0], left.left_top(), left).unwrap(),
                    Pie::open_with_id(ui.ctx(), ids[1], right.right_bottom(), right).unwrap(),
                ]);
            },
        );
        output.textures_delta.clear();
        let pies = pies.unwrap();
        for (pie, bounds) in pies.iter().zip([left, right]) {
            for action in Action::ALL {
                assert!(bounds.contains_rect(pie.item_rect(action)));
            }
            assert_eq!(pie.hovered(pie.anchor(), true), None);
        }
        let pointers = pies
            .each_ref()
            .map(|pie| pie.item_rect(Action::Front).center());
        let tooltip_layers = ids.map(|id| {
            egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Tooltip::tooltip_id(id.with(Action::Front.control().id()), 0),
            )
        });
        for (time, second_pointer) in [
            (0.0, None),
            (0.75, None),
            (0.9, None),
            (1.0, Some(pointers[1])),
            (1.2, Some(pointers[1])),
            (1.75, Some(pointers[1])),
            (1.9, Some(pointers[1])),
        ] {
            let mut passes = 0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(time),
                    events: vec![egui::Event::PointerMoved(pointers[0])],
                    ..Default::default()
                },
                |ui| {
                    let events = ui.ctx().input(|input| input.events.clone());
                    pies[0].paint(ui.ctx(), Some(pointers[0]), true);
                    pies[1].paint(ui.ctx(), second_pointer, true);
                    assert_eq!(ui.ctx().input(|input| input.events.clone()), events);
                    if passes == 0 {
                        ui.ctx().request_discard("exercise pie layout retry");
                    }
                    passes += 1;
                },
            );
            output.textures_delta.clear();
            assert!(passes > 1);
            let layers = ctx.memory(|memory| memory.layer_ids().collect::<Vec<_>>());
            if time >= 0.9 {
                assert!(layers.contains(&tooltip_layers[0]));
            }
            if time < 1.5 {
                assert!(!layers.contains(&tooltip_layers[1]));
            }
            if time >= 1.9 {
                assert!(layers.contains(&tooltip_layers[1]));
                assert_eq!(
                    output
                        .shapes
                        .iter()
                        .filter(|shape| matches!(
                            &shape.shape,
                            egui::Shape::Text(text) if text.galley.job.text == Action::Front.tooltip()
                        ))
                        .count(),
                    2,
                    "Both independently delayed tooltips must remain visible"
                );
            }
        }
    }
}
