//! Shading choices reuse the view pie's cancel zones, edge avoidance, tracing,
//! and painter. Input lifecycle and command dispatch belong to the host.
use egui::{Context, Id, Pos2, Rect, Vec2};

use super::pie::{Item, Layout, RADIUS};
use crate::{controls::Control, render::shading::ShadingMode, shortcuts::Command};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Wireframe,
    Solid,
    MaterialPreview,
}

impl Action {
    pub const ALL: [Self; 3] = [Self::Wireframe, Self::Solid, Self::MaterialPreview];

    pub fn mode(self) -> ShadingMode {
        match self {
            Self::Wireframe => ShadingMode::Wireframe,
            Self::Solid => ShadingMode::Solid,
            Self::MaterialPreview => ShadingMode::MaterialPreview,
        }
    }

    pub fn command(self) -> Command {
        Command::SetShading(self.mode())
    }

    pub fn control(self) -> Control {
        match self {
            Self::Wireframe => Control::PieWireframe,
            Self::Solid => Control::PieSolid,
            Self::MaterialPreview => Control::PieMaterialPreview,
        }
    }

    fn offset(self) -> Vec2 {
        match self {
            Self::Wireframe => -Vec2::X,
            Self::Solid => Vec2::X,
            Self::MaterialPreview => Vec2::Y,
        }
    }

    fn tooltip(self) -> &'static str {
        match self {
            Self::Wireframe => {
                "Show polygon boundaries, including rear edges, without filled surfaces."
            }
            Self::Solid => "Show shaded surfaces with neutral viewport lighting.",
            Self::MaterialPreview => {
                "Preview materials and textures with studio lighting. Native meshes keep their solid appearance."
            }
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

    /// Keep painted layers and tooltip state independent from other instances.
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

    pub fn hovered(&self, point: Pos2) -> Option<Action> {
        if !self.layout.accepts(point) {
            return None;
        }
        Action::ALL
            .into_iter()
            .find(|action| self.item_rect(*action).contains(point))
            .or_else(|| {
                let delta = self.layout.normalized_delta(point - self.center());
                // The unoccupied upper wedge cancels. Rendered will need its
                // own scene-lighting contract before it becomes a choice here.
                if delta.x.abs() < delta.y.abs() {
                    return (delta.y > 0.0).then_some(Action::MaterialPreview);
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
            sector_half_angle: std::f32::consts::FRAC_PI_4,
            tooltip: Some(action.tooltip()),
        });
        self.layout.paint(
            ctx,
            "n3.shading_pie",
            self.instance,
            Control::ShadingPie,
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
    fn cards_and_occupied_wedges_choose_modes_while_top_and_center_cancel() {
        let pie = open(viewport().center(), viewport()).unwrap();
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
                let pie = open(anchor, viewport).unwrap();
                assert_eq!(pie.anchor(), anchor);
                assert_eq!(pie.hovered(anchor), None);
                for action in Action::ALL {
                    assert!(viewport.contains_rect(pie.item_rect(action)));
                }
                for (index, action) in Action::ALL.into_iter().enumerate() {
                    for other in &Action::ALL[index + 1..] {
                        assert!(!pie.item_rect(action).intersects(pie.item_rect(*other)));
                    }
                }
            }
        }
        assert!(open(Pos2::ZERO, Rect::ZERO).is_none());
    }

    #[test]
    fn painting_witnesses_three_choices_without_taking_input_and_marks_current_mode() {
        let ctx = Context::default();
        controls::enable(&ctx);
        let pie = open(viewport().center(), viewport()).unwrap();
        let pointer = pie.item_rect(Action::Wireframe).center();
        let mut frames = Vec::new();
        for current in Action::ALL.map(Action::mode) {
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
        assert_ne!(frames[1], frames[2]);
    }
}
