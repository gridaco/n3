//! Pure view-pie geometry and painting. The host owns the held key, pointer
//! lifecycle, cancellation, and dispatch; painting never consumes an event.
use std::f32::consts::FRAC_PI_4;

use egui::{Context, Pos2, Rect, Vec2};

#[cfg(test)]
use super::pie::DEAD_ZONE;
use super::pie::{Item, Layout};

use crate::{camera::View, controls::Control, shortcuts::Command};

const RADIUS: Vec2 = Vec2::new(105.0, 75.0);

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
}

#[derive(Clone, Debug)]
pub struct Pie {
    layout: Layout,
}

impl Pie {
    pub fn open(anchor: Pos2, viewport: Rect) -> Option<Self> {
        Some(Self {
            layout: Layout::open(anchor, viewport, RADIUS)?,
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
        self.layout.item_rect(action.offset())
    }

    /// Cards are direct targets; the surrounding space uses the nearest of
    /// eight normalized ellipse directions. Southwest deliberately has no action.
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
        });
        self.layout
            .paint(ctx, "n3.view_pie", Control::ViewPie, &items);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls;

    fn viewport() -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 800.0))
    }

    #[test]
    fn all_sectors_and_card_rectangles_match_the_seven_actions() {
        let pie = Pie::open(viewport().center(), viewport()).unwrap();
        for action in Action::ALL {
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
        let pie = Pie::open(viewport().center(), viewport()).unwrap();
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
                let pie = Pie::open(anchor, viewport).unwrap();
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
        let original = Pie::open(anchor, viewport()).unwrap();
        let smaller = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 200.0));
        let resized = Pie::open(anchor, smaller).unwrap();
        assert_ne!(resized.center(), original.center());
        assert_eq!(resized.anchor(), anchor);
        assert_eq!(resized.hovered(anchor, true), None);
        for action in Action::ALL {
            assert!(smaller.contains_rect(resized.item_rect(action)));
        }
        assert!(Pie::open(anchor, Rect::ZERO).is_none());
        assert!(Pie::open(Pos2::new(-1.0, -1.0), viewport()).is_none());
        assert!(Pie::open(Pos2::ZERO, Rect::NOTHING).is_none());
        assert!(Pie::open(Pos2::new(f32::NAN, 0.0), viewport()).is_none());
        assert!(
            Pie::open(
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
        let pie = Pie::open(viewport().center(), viewport()).unwrap();
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
        let pie = Pie::open(viewport().center(), viewport()).unwrap();
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
}
