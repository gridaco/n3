use super::icons::Icon;
use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use glam::Vec3;

use crate::{
    camera::Camera,
    controls::{self, Control, shortcut_label},
    navigation_input::{NavigationInput, NavigationMotion},
    orientation::display_rotation,
    theme::{self, AXIS_COLORS},
};

const SIZE: f32 = 100.0;
const NAVIGATION_HEIGHT: f32 = 26.0;
const INSET: f32 = theme::space::XL_2;
const ARM: f32 = 30.0;
const HANDLE_RADIUS: f32 = 9.0;
const AXIS_CONTROLS: [Control; 6] = [
    Control::AxisX,
    Control::AxisNegX,
    Control::AxisY,
    Control::AxisNegY,
    Control::AxisZ,
    Control::AxisNegZ,
];

/// Logical viewport coordinates; native camera input excludes this same region.
pub fn bounds(viewport: Rect) -> Rect {
    let disk = disk_bounds(viewport);
    Rect::from_min_size(disk.min, egui::vec2(SIZE, SIZE + NAVIGATION_HEIGHT))
}

fn disk_bounds(viewport: Rect) -> Rect {
    Rect::from_min_size(
        egui::pos2(viewport.right() - INSET - SIZE, viewport.top() + INSET),
        Vec2::splat(SIZE),
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GizmoAction {
    Snap(Vec3),
    Orbit,
    Navigation(bool),
    ToggleProjection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GizmoResponse {
    pub open_preferences: bool,
    pub action: Option<GizmoAction>,
    /// Keep Planar ownership while an axis click is holding a paused snap.
    pub primary_held: bool,
}

#[derive(Clone, Copy)]
struct Handle {
    index: usize,
    direction: Vec3,
    center: Pos2,
    depth: f32,
}

fn handles(camera: &Camera, z_up: bool, center: Pos2) -> [Handle; 6] {
    let rotation = display_rotation(z_up);
    let mut handles = std::array::from_fn(|index| {
        let axis = [Vec3::X, Vec3::Y, Vec3::Z][index / 2];
        let direction = rotation.transform_vector3(if index % 2 == 0 { axis } else { -axis });
        let view = camera.direction_in_view(direction);
        Handle {
            index,
            direction,
            center: center + egui::vec2(view.x, -view.y) * ARM,
            depth: view.z,
        }
    });
    // Paint rear handles first. Picking uses this same order in reverse.
    handles.sort_by(|a, b| a.depth.total_cmp(&b.depth));
    handles
}

fn hit_handle(handles: &[Handle; 6], pointer: Pos2) -> Option<Handle> {
    handles
        .iter()
        .rev()
        .copied()
        .find(|handle| handle.center.distance(pointer) <= HANDLE_RADIUS + 3.0)
}

fn snap_direction(handle: Handle) -> Vec3 {
    // Looking straight down an axis overlaps both ends. Clicking the visible
    // end again flips to the hidden end, keeping all six views reachable.
    if handle.depth > 0.9999 {
        -handle.direction
    } else {
        handle.direction
    }
}

pub fn show(
    ui: &mut Ui,
    viewport: Rect,
    camera: &mut Camera,
    z_up: bool,
    planar: bool,
) -> GizmoResponse {
    let rect = disk_bounds(viewport);
    let response = ui.interact(rect, ui.id().with("axis_gizmo"), Sense::click_and_drag());
    controls::record(
        ui.ctx(),
        Control::Gizmo,
        Control::Gizmo.label(),
        bounds(viewport).intersect(viewport),
        response.enabled(),
    );
    let ctx = ui.ctx().clone();
    let menu_state_id = response.id.with("context_menu_was_open");
    let blocked_drag_id = response.id.with("context_menu_blocked_primary");
    let was_open = response.context_menu_opened()
        || ctx.data(|data| data.get_temp::<bool>(menu_state_id).unwrap_or(false));
    let secondary_id = response.id.with("secondary_click_or_drag");
    let mut secondary = ctx
        .data_mut(|data| data.remove_temp::<NavigationInput>(secondary_id))
        .unwrap_or_default();
    let mut context_click = false;
    if !ui.input(|input| input.focused) || egui::Popup::is_any_open(&ctx) {
        secondary.cancel();
    } else {
        for event in ui.input(|input| input.events.clone()) {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    ..
                } => {
                    if pressed {
                        let eligible = response.enabled()
                            && response.contains_pointer()
                            && rect.intersect(viewport).contains(pos);
                        secondary.queue_press(egui::PointerButton::Secondary, eligible, pos);
                    } else {
                        secondary.queue_release(egui::PointerButton::Secondary, pos);
                    }
                }
                egui::Event::PointerMoved(pos) => secondary.queue_motion(pos),
                egui::Event::PointerGone
                | egui::Event::WindowFocused(false)
                | egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } => secondary.cancel(),
                _ => {}
            }
        }
        context_click = secondary
            .flush()
            .iter()
            .any(|motion| matches!(motion, NavigationMotion::ContextClick(_)));
    }
    ctx.data_mut(|data| data.insert_temp(secondary_id, secondary));
    let mut output = GizmoResponse::default();
    let open_command = if context_click {
        Some(egui::SetOpenCommand::Bool(true))
    } else if response.clicked_by(egui::PointerButton::Primary) {
        Some(egui::SetOpenCommand::Bool(false))
    } else {
        None
    };
    // Override egui's time-based secondary click with the same distance-only
    // policy as the viewport. A right-drag never becomes a preferences menu.
    super::menu::context(&response)
        .open_memory(open_command)
        .show(|ui| {
            super::menu::content(ui, Control::Gizmo, |ui| {
                if super::menu::Item::new(
                    crate::input::actions::ActionId::Preferences,
                    crate::input::actions::ActionState::default(),
                )
                .control(Control::GizmoPreferences)
                .show(ui)
                .is_some_and(|response| response.clicked())
                {
                    output.open_preferences = true;
                    ui.close();
                }
            });
        });
    let menu_open = response.context_menu_opened();
    let primary_down = ui.input(|input| input.pointer.primary_down());
    let blocked_primary = ctx.data(|data| data.get_temp::<bool>(blocked_drag_id).unwrap_or(false))
        || ((was_open || menu_open) && primary_down);
    ctx.data_mut(|data| {
        data.insert_temp(menu_state_id, menu_open);
        data.insert_temp(blocked_drag_id, blocked_primary && primary_down);
    });
    // Closing the menu on the gizmo must not turn that same press into an orbit
    // or a snap, even if the pointer leaves the menu before being released.
    let allow_primary = !was_open && !menu_open && !blocked_primary;
    let first_pass = ui.ctx().current_pass_index() == 0;
    output.primary_held = allow_primary && response.is_pointer_button_down_on();
    if first_pass
        && allow_primary
        && response.is_pointer_button_down_on()
        && ui.input(|input| input.pointer.button_pressed(egui::PointerButton::Primary))
    {
        // Hold the currently visible pose as soon as the user takes control;
        // animated handles must not move away while a click becomes a drag.
        camera.cancel_transition();
    }
    let dragging = allow_primary
        && response.dragged_by(egui::PointerButton::Primary)
        && ui.input(|i| i.focused);
    if first_pass && dragging {
        let delta = response.drag_delta();
        if delta.is_finite() && delta.length_sq() > 0.0 {
            camera.orbit(delta.x, delta.y);
            output.action = Some(GizmoAction::Orbit);
        }
    }
    let axes = handles(camera, z_up, rect.center());
    let hovered = allow_primary
        .then(|| response.hover_pos().and_then(|pos| hit_handle(&axes, pos)))
        .flatten();
    if first_pass
        && allow_primary
        && response.clicked_by(egui::PointerButton::Primary)
        && let Some(handle) = response
            .interact_pointer_pos()
            .and_then(|pos| hit_handle(&axes, pos))
    {
        // The controller records the Free return orientation before applying
        // this request, including when axis transitions are instantaneous.
        output.action = Some(GizmoAction::Snap(snap_direction(handle)));
        // The controller applies the snap after painting; refresh projection feedback.
        ui.ctx().request_repaint();
    }
    let painter = ui.painter().with_clip_rect(rect.intersect(viewport));
    // Overlay neutrals follow the active egui theme. Axis colors remain
    // semantic and intentionally do not change with the accent preference.
    let visuals = ui.visuals();
    let background = visuals.window_fill();
    painter.circle_filled(
        rect.center(),
        45.0,
        if response.hovered() || dragging {
            visuals.widgets.hovered.bg_fill
        } else {
            background
        },
    );
    for handle in &axes {
        let color = AXIS_COLORS[handle.index / 2];
        let stroke = if handle.depth < 0.0 {
            color.gamma_multiply(0.4)
        } else {
            color.gamma_multiply(0.8)
        };
        painter.line_segment([rect.center(), handle.center], Stroke::new(1.5, stroke));
    }
    painter.circle_filled(rect.center(), 3.0, visuals.weak_text_color());
    controls::scope(ui.ctx(), Control::Gizmo, || {
        for handle in &axes {
            // Only expose a clickable target when picking its center selects
            // this handle; the opposite axis is hidden in an aligned view.
            if hit_handle(&axes, handle.center).is_some_and(|hit| hit.index == handle.index)
                && rect.intersect(viewport).contains(handle.center)
            {
                let control = AXIS_CONTROLS[handle.index];
                controls::record(
                    ui.ctx(),
                    control,
                    control.label(),
                    Rect::from_center_size(handle.center, Vec2::splat(2.0 * (HANDLE_RADIUS + 3.0)))
                        .intersect(rect.intersect(viewport)),
                    response.enabled(),
                );
            }
        }
    });
    for handle in &axes {
        let color = AXIS_COLORS[handle.index / 2];
        let positive = handle.index % 2 == 0;
        let hovered = !dragging && hovered.is_some_and(|hover| hover.index == handle.index);
        let radius = HANDLE_RADIUS + if hovered { 1.0 } else { 0.0 };
        let fill = if positive { color } else { background };
        painter.circle_filled(handle.center, radius, fill);
        painter.circle_stroke(
            handle.center,
            radius,
            Stroke::new(
                if hovered { 2.0 } else { 1.0 },
                if hovered { visuals.text_color() } else { color },
            ),
        );
        painter.text(
            handle.center,
            Align2::CENTER_CENTER,
            AXIS_CONTROLS[handle.index].label(),
            FontId::proportional(if positive {
                theme::text::XS
            } else {
                theme::text::GIZMO_BACK_AXIS_10
            }),
            if positive {
                Color32::from_rgb(25, 30, 38)
            } else {
                color
            },
        );
    }
    if dragging {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    } else if let Some(handle) = hovered {
        let opposite = handle.depth > 0.9999;
        let label = AXIS_CONTROLS[if opposite {
            handle.index ^ 1
        } else {
            handle.index
        }]
        .label();
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(format!(
                "Click: view from {label} · orthographic{}\nDrag: orbit the camera",
                if opposite { " · opposite side" } else { "" }
            ));
    } else if allow_primary {
        response
            .on_hover_cursor(egui::CursorIcon::Grab)
            .on_hover_text("Drag to orbit the camera · right-click for preferences");
    }
    let toggle_rect = Rect::from_min_size(
        egui::pos2(rect.left(), rect.bottom() + theme::space::SM),
        egui::vec2(68.0, 22.0),
    );
    controls::scope(&ctx, Control::Gizmo, || {
        ui.add_enabled_ui(allow_primary, |ui| {
            for (index, control, selected, tooltip) in [
                (0, Control::NavigationPlanar, planar, "Planar navigation"),
                (
                    1,
                    Control::NavigationFree,
                    !planar,
                    "Return to 3D using Gizmo preferences",
                ),
            ] {
                let button_rect = Rect::from_min_size(
                    toggle_rect.min + egui::vec2(index as f32 * 34.0, 0.0),
                    egui::vec2(34.0, toggle_rect.height()),
                );
                let response = ui
                    .push_id(control.id(), |ui| {
                        let left = if index == 0 {
                            theme::radius::MD
                        } else {
                            theme::radius::NONE
                        };
                        let right = if index == 1 {
                            theme::radius::MD
                        } else {
                            theme::radius::NONE
                        };
                        ui.put(
                            button_rect,
                            egui::Button::new(control.label())
                                .selected(selected)
                                .corner_radius(egui::CornerRadius {
                                    nw: left,
                                    sw: left,
                                    ne: right,
                                    se: right,
                                }),
                        )
                    })
                    .inner
                    .on_hover_text(format!(
                        "{tooltip} · tap {} to toggle",
                        shortcut_label("view.planar")
                    ));
                controls::record(
                    &ctx,
                    control,
                    control.label(),
                    response.rect.intersect(viewport),
                    response.enabled(),
                );
                if first_pass && response.clicked() {
                    output.action = Some(GizmoAction::Navigation(index == 0));
                }
            }
            let orthographic = camera.is_orthographic();
            let (icon, current, next) = if orthographic {
                (Icon::Orthographic, "Orthographic", "Perspective")
            } else {
                (Icon::Perspective, "Perspective", "Orthographic")
            };
            let button_rect = Rect::from_min_size(
                egui::pos2(toggle_rect.right() + theme::space::MD, toggle_rect.top()),
                egui::vec2(26.0, 22.0),
            );
            let response = ui
                .push_id(Control::Projection.id(), |ui| {
                    ui.put(
                        button_rect,
                        egui::Button::new("").corner_radius(theme::radius::MD),
                    )
                })
                .inner
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(format!(
                    "{current} projection · Click for {next}\n{} — toggle projection",
                    shortcut_label("view.projection")
                ));
            response.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    response.enabled(),
                    format!("{current} projection; switch to {next}"),
                )
            });
            icon.paint(
                &ui.painter()
                    .with_clip_rect(response.rect.intersect(viewport)),
                Rect::from_center_size(response.rect.center(), Vec2::splat(18.0)),
                ui.style().interact(&response).fg_stroke.color,
            );
            controls::record(
                &ctx,
                Control::Projection,
                Control::Projection.label(),
                response.rect.intersect(viewport),
                response.enabled(),
            );
            if first_pass && response.clicked() {
                output.action = Some(GizmoAction::ToggleProjection);
            }
        });
    });
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Transition;

    fn pointer_frame(
        context: &egui::Context,
        camera: &mut Camera,
        events: Vec<egui::Event>,
    ) -> GizmoResponse {
        pointer_frame_with_transition(context, camera, events, Transition::Instant)
    }

    fn pointer_frame_with_transition(
        context: &egui::Context,
        camera: &mut Camera,
        events: Vec<egui::Event>,
        transition: Transition,
    ) -> GizmoResponse {
        pointer_frame_with_time(context, camera, events, transition, None)
    }

    fn pointer_frame_with_time(
        context: &egui::Context,
        camera: &mut Camera,
        events: Vec<egui::Event>,
        transition: Transition,
        time: Option<f64>,
    ) -> GizmoResponse {
        pointer_frame_with_navigation(context, camera, events, transition, time, false)
    }

    fn pointer_frame_with_navigation(
        context: &egui::Context,
        camera: &mut Camera,
        events: Vec<egui::Event>,
        transition: Transition,
        time: Option<f64>,
        planar: bool,
    ) -> GizmoResponse {
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut output = GizmoResponse::default();
        context
            .run_ui(
                egui::RawInput {
                    screen_rect: Some(viewport),
                    events,
                    time,
                    ..Default::default()
                },
                |root_ui| {
                    let context = root_ui.ctx().clone();
                    let ctx = &context;
                    controls::begin_pass(ctx);
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(root_ui, |ui| {
                            output = show(ui, viewport, camera, false, planar);
                            if let Some(GizmoAction::Snap(direction)) = output.action {
                                camera.look_from_with_transition(direction, transition);
                            }
                        });
                },
            )
            .textures_delta
            .clear();
        output
    }

    #[test]
    fn navigation_toggle_has_separate_bounds_and_requests_mode_without_touching_camera() {
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        assert_eq!(disk_bounds(viewport).center(), egui::pos2(734.0, 66.0));
        assert_eq!(bounds(viewport).min, disk_bounds(viewport).min);
        assert_eq!(bounds(viewport).height(), SIZE + NAVIGATION_HEIGHT);
        for (control, requested) in [
            (Control::NavigationPlanar, true),
            (Control::NavigationFree, false),
        ] {
            let ctx = egui::Context::default();
            controls::enable(&ctx);
            let mut camera = Camera::default();
            camera.look_from_with_transition(Vec3::X, Transition::default());
            let original = camera.view_projection(1.0);
            pointer_frame_with_navigation(
                &ctx,
                &mut camera,
                vec![],
                Transition::Instant,
                None,
                !requested,
            );
            let trace = controls::snapshot(&ctx);
            let target = trace.get(control).unwrap();
            assert_eq!(target.parents, [Control::Gizmo]);
            assert!(bounds(viewport).contains_rect(target.rect));
            assert!(!disk_bounds(viewport).intersects(target.rect));
            let pos = target.rect.center();
            for pressed in [true, false] {
                let output = pointer_frame_with_navigation(
                    &ctx,
                    &mut camera,
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    Transition::Instant,
                    None,
                    !requested,
                );
                assert_eq!(
                    output.action,
                    (!pressed).then_some(GizmoAction::Navigation(requested))
                );
                assert!(!output.open_preferences);
                assert!(camera.is_transitioning());
                assert_eq!(camera.view_projection(1.0), original);
            }
        }
    }

    #[test]
    fn dragging_from_navigation_toggle_never_captures_the_orbit_disk() {
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        for control in [
            Control::NavigationPlanar,
            Control::NavigationFree,
            Control::Projection,
        ] {
            let ctx = egui::Context::default();
            controls::enable(&ctx);
            let mut camera = Camera::default();
            let original = camera.view_projection(1.0);
            pointer_frame(&ctx, &mut camera, vec![]);
            let start = controls::snapshot(&ctx).get(control).unwrap().rect.center();
            let end = disk_bounds(viewport).center();
            for events in [
                vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                vec![egui::Event::PointerMoved(end)],
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            ] {
                assert_eq!(pointer_frame(&ctx, &mut camera, events).action, None);
                assert_eq!(camera.view_projection(1.0), original);
            }
        }
    }

    #[test]
    fn secondary_distance_policy_keeps_long_clicks_and_rejects_drag_menus() {
        use crate::pointer_policy::DRAG_THRESHOLD;
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let start = disk_bounds(viewport).center();
        for distance in [DRAG_THRESHOLD, DRAG_THRESHOLD + 0.5, 30.0] {
            let ctx = egui::Context::default();
            controls::enable(&ctx);
            let mut camera = Camera::default();
            camera.look_from_with_transition(Vec3::X, Transition::default());
            let pose = camera.view_projection(1.0);
            let button = |pressed| egui::Event::PointerButton {
                pos: start,
                button: egui::PointerButton::Secondary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            pointer_frame_with_time(&ctx, &mut camera, vec![], Transition::Instant, Some(0.0));
            pointer_frame_with_time(
                &ctx,
                &mut camera,
                vec![egui::Event::PointerMoved(start), button(true)],
                Transition::Instant,
                Some(0.1),
            );
            pointer_frame_with_time(
                &ctx,
                &mut camera,
                vec![egui::Event::PointerMoved(start + egui::vec2(distance, 0.0))],
                Transition::Instant,
                Some(5.0),
            );
            pointer_frame_with_time(
                &ctx,
                &mut camera,
                vec![egui::Event::PointerMoved(start), button(false)],
                Transition::Instant,
                Some(10.0),
            );
            pointer_frame_with_time(&ctx, &mut camera, vec![], Transition::Instant, Some(10.1));
            assert_eq!(
                controls::snapshot(&ctx)
                    .get(Control::GizmoPreferences)
                    .is_ok(),
                distance <= DRAG_THRESHOLD,
                "distance {distance}",
            );
            assert!(camera.is_transitioning());
            assert!(camera.view_projection(1.0).abs_diff_eq(pose, 1e-6));
        }
    }

    #[test]
    fn context_menu_preserves_animation_and_dismissal_cannot_become_a_drag() {
        use std::time::Duration;
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let rect = disk_bounds(viewport);
        let mode = Transition::Animated {
            duration: Duration::from_millis(250),
        };
        for choose_preferences in [true, false] {
            let context = egui::Context::default();
            controls::enable(&context);
            let mut camera = Camera::default();
            camera.look_from(Vec3::Z);
            camera.look_from_with_transition(Vec3::X, mode);
            camera.advance_transition(Duration::from_millis(75));
            let pose = camera.view_projection(1.0);
            pointer_frame_with_transition(&context, &mut camera, vec![], mode);
            for pressed in [true, false] {
                assert!(
                    !pointer_frame_with_transition(
                        &context,
                        &mut camera,
                        vec![
                            egui::Event::PointerMoved(rect.center()),
                            egui::Event::PointerButton {
                                pos: rect.center(),
                                button: egui::PointerButton::Secondary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            },
                        ],
                        mode,
                    )
                    .open_preferences
                );
                assert!(camera.is_transitioning());
                assert!(camera.view_projection(1.0).abs_diff_eq(pose, 1e-5));
            }
            pointer_frame_with_transition(&context, &mut camera, vec![], mode);
            let trace = controls::snapshot(&context);
            trace.validate().unwrap();
            let item = trace.get(Control::GizmoPreferences).unwrap();
            assert_eq!(item.parents, [Control::Gizmo]);
            let start = if choose_preferences {
                item.rect.center()
            } else {
                [
                    rect.left_top() + egui::vec2(4.0, 4.0),
                    rect.left_bottom() + egui::vec2(4.0, -4.0),
                    rect.right_bottom() - egui::vec2(4.0, 4.0),
                ]
                .into_iter()
                .find(|point| !item.rect.expand(10.0).contains(*point))
                .unwrap()
            };
            pointer_frame_with_transition(
                &context,
                &mut camera,
                vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                mode,
            );
            let end = if choose_preferences {
                start
            } else {
                start + egui::vec2(-180.0, 40.0)
            };
            pointer_frame_with_transition(
                &context,
                &mut camera,
                vec![egui::Event::PointerMoved(end)],
                mode,
            );
            let opened = pointer_frame_with_transition(
                &context,
                &mut camera,
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                mode,
            );
            assert_eq!(opened.open_preferences, choose_preferences);
            assert!(camera.is_transitioning());
            assert!(camera.view_projection(1.0).abs_diff_eq(pose, 1e-5));
        }
    }

    #[test]
    fn animated_axis_clicks_take_time_and_pointer_press_interrupts_in_place() {
        use std::time::Duration;
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut camera = Camera::default();
        camera.look_from(Vec3::Z);
        let mode = Transition::Animated {
            duration: Duration::from_millis(250),
        };
        let x_handle = disk_bounds(viewport).center() + egui::vec2(ARM, 0.0);
        let original = camera.view_projection(1.0);
        pointer_frame_with_transition(&context, &mut camera, vec![], mode);
        for pressed in [true, false] {
            pointer_frame_with_transition(
                &context,
                &mut camera,
                vec![
                    egui::Event::PointerMoved(x_handle),
                    egui::Event::PointerButton {
                        pos: x_handle,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                mode,
            );
        }
        assert!(camera.is_transitioning());
        assert!(camera.view_projection(1.0).abs_diff_eq(original, 1e-5));
        assert!(camera.advance_transition(Duration::from_millis(100)));
        let halfway = camera.view_projection(1.0);
        assert!(!halfway.abs_diff_eq(original, 1e-4));
        let center = disk_bounds(viewport).center();
        pointer_frame_with_transition(
            &context,
            &mut camera,
            vec![
                egui::Event::PointerMoved(center),
                egui::Event::PointerButton {
                    pos: center,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            mode,
        );
        assert!(!camera.is_transitioning());
        assert!(!camera.advance_transition(Duration::from_secs(1)));
        assert!(camera.view_projection(1.0).abs_diff_eq(halfway, 1e-5));
    }

    #[test]
    fn dragging_the_body_or_axis_handle_orbits_outside_bounds_without_snapping() {
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let center = disk_bounds(viewport).center();
        for from_axis_handle in [false, true] {
            let context = egui::Context::default();
            let mut camera = Camera::default();
            camera.pan(30.0, -10.0, viewport.height());
            camera.zoom(0.2);
            let mut expected = camera.clone();
            let start = if from_axis_handle {
                handles(&camera, false, center)
                    .iter()
                    .find(|h| h.index == 0)
                    .unwrap()
                    .center
            } else {
                center + egui::vec2(-30.0, -30.0)
            };
            pointer_frame(&context, &mut camera, vec![]);
            pointer_frame(
                &context,
                &mut camera,
                vec![
                    egui::Event::PointerMoved(start),
                    egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
            let mut position = start;
            for delta in [egui::vec2(30.0, 15.0), egui::vec2(-200.0, 25.0)] {
                position += delta;
                pointer_frame(
                    &context,
                    &mut camera,
                    vec![egui::Event::PointerMoved(position)],
                );
                expected.orbit(delta.x, delta.y);
                assert!(
                    camera
                        .view_projection(1.0)
                        .abs_diff_eq(expected.view_projection(1.0), 1e-5)
                );
            }
            assert!(!bounds(viewport).contains(position));
            pointer_frame(
                &context,
                &mut camera,
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            pointer_frame(
                &context,
                &mut camera,
                vec![egui::Event::PointerMoved(center)],
            );
            assert!(
                camera
                    .view_projection(1.0)
                    .abs_diff_eq(expected.view_projection(1.0), 1e-5)
            );
            assert!(!camera.is_orthographic(), "releasing a drag must not snap");
        }
    }

    #[test]
    fn source_axes_follow_the_same_y_up_and_z_up_transform_as_the_mesh() {
        let mut camera = Camera::default();
        camera.look_from(Vec3::Z);
        for z_up in [false, true] {
            let axes = handles(&camera, z_up, Pos2::ZERO);
            let get = |index| axes.iter().find(|h| h.index == index).unwrap();
            assert!((get(0).center - egui::pos2(ARM, 0.0)).length() < 1e-5);
            let vertical = if z_up { 4 } else { 2 };
            assert!((get(vertical).center - egui::pos2(0.0, -ARM)).length() < 1e-5);
            assert_eq!(get(vertical).direction, Vec3::Y);
        }
    }

    #[test]
    fn aligned_handles_pick_the_front_and_can_flip_to_the_hidden_side() {
        for direction in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
            for z_up in [false, true] {
                let mut camera = Camera::default();
                camera.look_from(direction);
                let axes = handles(&camera, z_up, Pos2::ZERO);
                let handle = hit_handle(&axes, Pos2::ZERO).expect("aligned axis at center");
                assert!(handle.depth > 0.9999);
                assert!((snap_direction(handle) + direction).length() < 1e-5);
            }
        }
    }

    #[test]
    fn egui_pointer_clicks_snap_and_flip_the_camera() {
        let context = egui::Context::default();
        let viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut camera = Camera::default();
        camera.look_from(Vec3::Z);
        let mut frame = |events| {
            // The shared harness applies the snap request as the controller
            // does, after it has the opportunity to remember the Free view.
            pointer_frame(&context, &mut camera, events);
        };
        frame(vec![]);
        // From +Z, +X is at the right end. After snapping it is at the center.
        for pos in [
            disk_bounds(viewport).center() + egui::vec2(ARM, 0.0),
            disk_bounds(viewport).center(),
        ] {
            frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
        assert!((camera.direction_in_view(-Vec3::X) - Vec3::Z).length() < 1e-5);
        assert!(camera.is_orthographic());
    }
}
