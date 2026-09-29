//! Shading choices reuse the view pie's cancel zones, edge avoidance, tracing,
//! and painter. Input lifecycle and command dispatch belong to the host.
use egui::{Context, Pos2, Rect, Vec2};

use super::pie::{Item, Layout};
use crate::{controls::Control, render::shading::ShadingMode, shortcuts::Command};

const RADIUS: Vec2 = Vec2::new(105.0, 75.0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Wireframe,
    Solid,
}

impl Action {
    pub const ALL: [Self; 2] = [Self::Wireframe, Self::Solid];

    pub fn mode(self) -> ShadingMode {
        match self {
            Self::Wireframe => ShadingMode::Wireframe,
            Self::Solid => ShadingMode::Solid,
        }
    }

    pub fn command(self) -> Command {
        Command::SetShading(self.mode())
    }

    pub fn control(self) -> Control {
        match self {
            Self::Wireframe => Control::PieWireframe,
            Self::Solid => Control::PieSolid,
        }
    }

    fn offset(self) -> Vec2 {
        match self {
            Self::Wireframe => -Vec2::X,
            Self::Solid => Vec2::X,
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

    pub fn hovered(&self, point: Pos2) -> Option<Action> {
        if !self.layout.accepts(point) {
            return None;
        }
        Action::ALL
            .into_iter()
            .find(|action| self.item_rect(*action).contains(point))
            .or_else(|| {
                let delta = self.layout.normalized_delta(point - self.center());
                // Only the left/right wedges choose a mode. Empty vertical
                // directions cancel, leaving deliberate room for future modes.
                if delta.x.abs() < delta.y.abs() {
                    return None;
                }
                Some(if delta.x < 0.0 {
                    Action::Wireframe
                } else {
                    Action::Solid
                })
            })
    }

    pub fn paint(&self, ctx: &Context, pointer: Option<Pos2>, current: ShadingMode) {
        let hovered = pointer.and_then(|point| self.hovered(point));
        let items = Action::ALL.map(|action| Item {
            control: action.control(),
            offset: action.offset(),
            enabled: true,
            hovered: hovered == Some(action),
            selected: action.mode() == current,
        });
        self.layout
            .paint(ctx, "n3.shading_pie", Control::ShadingPie, &items);
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
    fn cards_and_horizontal_wedges_choose_modes_while_other_directions_cancel() {
        let pie = Pie::open(viewport().center(), viewport()).unwrap();
        for action in Action::ALL {
            let card = pie.item_rect(action);
            for point in [
                card.center(),
                card.left_center(),
                card.right_center(),
                pie.center() + action.offset() * RADIUS * 2.0,
            ] {
                assert_eq!(pie.hovered(point), Some(action));
            }
        }
        for point in [
            pie.anchor(),
            pie.center() + Vec2::Y * 100.0,
            pie.center() - Vec2::Y * 100.0,
            Pos2::new(-1.0, 0.0),
            Pos2::new(f32::NAN, 0.0),
        ] {
            assert_eq!(pie.hovered(point), None);
        }
    }

    #[test]
    fn clamped_pie_keeps_cards_inside_and_the_original_anchor_cancels() {
        for viewport in [
            viewport(),
            Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(200.0, 120.0)),
        ] {
            for anchor in [
                viewport.left_top(),
                viewport.right_bottom(),
                viewport.center(),
            ] {
                let pie = Pie::open(anchor, viewport).unwrap();
                assert_eq!(pie.anchor(), anchor);
                assert_eq!(pie.hovered(anchor), None);
                for action in Action::ALL {
                    assert!(viewport.contains_rect(pie.item_rect(action)));
                }
                assert!(
                    !pie.item_rect(Action::Solid)
                        .intersects(pie.item_rect(Action::Wireframe))
                );
            }
        }
        assert!(Pie::open(Pos2::ZERO, Rect::ZERO).is_none());
    }

    #[test]
    fn painting_witnesses_two_choices_without_taking_input_and_marks_current_mode() {
        let ctx = Context::default();
        controls::enable(&ctx);
        let pie = Pie::open(viewport().center(), viewport()).unwrap();
        let pointer = pie.item_rect(Action::Wireframe).center();
        let mut frames = Vec::new();
        for current in [ShadingMode::Solid, ShadingMode::Wireframe] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport()),
                    events: vec![egui::Event::PointerMoved(pointer)],
                    ..Default::default()
                },
                |root_ui| {
                    let ctx = root_ui.ctx();
                    controls::begin_pass(ctx);
                    let events = ctx.input(|input| input.events.clone());
                    pie.paint(ctx, Some(pointer), current);
                    assert_eq!(ctx.input(|input| input.events.clone()), events);
                },
            );
            output.textures_delta.clear();
            assert_eq!(
                output.platform_output.cursor_icon,
                egui::CursorIcon::PointingHand
            );
            frames.push(output.shapes);
            let trace = controls::snapshot(&ctx);
            trace.validate().unwrap();
            for action in Action::ALL {
                let observed = trace.get(action.control()).unwrap();
                assert_eq!(observed.parents, vec![Control::ShadingPie]);
                assert_eq!(observed.rect, pie.item_rect(action));
                assert!(observed.enabled);
            }
        }
        assert_ne!(
            frames[0], frames[1],
            "The current shading mode needs a visible state"
        );
    }
}
