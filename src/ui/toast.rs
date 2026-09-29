//! Transient, non-modal feedback. The queue belongs to the workspace, never the
//! document or undo history. Time comes from egui so replay and native use agree.
use std::{collections::VecDeque, time::Duration};

use egui::{Align, Align2, Context, Layout, Rect, RichText, Sense};

use crate::{
    shortcuts::Command,
    theme::{self, Palette},
};

use super::{controls, controls::Control, lucide};

const MAX_TOASTS: usize = 3;
const MAX_PENDING_TOASTS: usize = 32;
const DEFAULT_DURATION: Duration = Duration::from_secs(6);
const TOAST_WIDTH: f32 = theme::size::STEP_60 + theme::size::STEP_20;
const MIN_TOAST_HEIGHT: f32 = theme::size::STEP_24;

#[allow(dead_code)] // The shared feedback vocabulary includes future producers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn label(self) -> &'static str {
        match self {
            Self::Info => "Info",
            Self::Success => "Success",
            Self::Warning => "Warning",
            Self::Error => "Error",
        }
    }
}

pub(crate) struct Toast {
    message: String,
    kind: ToastKind,
    action: Option<(String, Command)>,
    duration: Option<Duration>,
}

impl Toast {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToastKind::Info,
            action: None,
            duration: Some(DEFAULT_DURATION),
        }
    }

    pub(crate) fn kind(mut self, kind: ToastKind) -> Self {
        self.kind = kind;
        self
    }

    /// Actions use the application's existing command routing and ownership.
    pub(crate) fn action(mut self, label: impl Into<String>, command: Command) -> Self {
        self.action = Some((label.into(), command));
        self
    }

    #[allow(dead_code)] // Producers may opt into a different reading duration.
    pub(crate) fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    #[allow(dead_code)] // For producer-managed progress or persistent feedback.
    pub(crate) fn persistent(mut self) -> Self {
        self.duration = None;
        self
    }

    fn same_message(&self, other: &Self) -> bool {
        self.kind == other.kind && self.message == other.message && self.action == other.action
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ToastId(u64);

struct Entry {
    id: ToastId,
    toast: Toast,
    remaining: Option<Duration>,
    last_time: Option<f64>,
    paused: bool,
    dismiss_id: Option<egui::Id>,
    action_id: Option<egui::Id>,
}

impl Entry {
    fn advance(&mut self, now: f64, paused: bool) {
        // The previous state owned the elapsed interval. In an on-demand UI,
        // the event that starts a hover must not erase seconds before it, and
        // the event ending a hover must not charge the paused interval.
        if !self.paused
            && let Some(last) = self.last_time
            && let Some(remaining) = &mut self.remaining
        {
            *remaining = remaining.saturating_sub(Duration::from_secs_f64((now - last).max(0.0)));
        }
        self.last_time = Some(now);
        self.paused = paused;
    }

    fn owns_focus(&self, focused: Option<egui::Id>) -> bool {
        focused.is_some() && (focused == self.dismiss_id || focused == self.action_id)
    }

    fn expired(&self) -> bool {
        self.remaining.is_some_and(|remaining| remaining.is_zero())
    }
}

#[derive(Default)]
pub(crate) struct Toaster {
    entries: VecDeque<Entry>,
    next_id: u64,
    visible_ids: Vec<ToastId>,
    stack_rect: Option<Rect>,
    focus_request: Option<ToastId>,
    return_focus: Option<egui::Id>,
    focused_entry: Option<ToastId>,
    focused_widget: Option<egui::Id>,
    restore_requested: bool,
}

impl Toaster {
    /// Repeated active or pending messages refresh rather than multiply.
    /// At most three are visible; arrivals wait in FIFO order. A burst may hold
    /// 35 entries total: three display candidates and a 32-message backlog.
    /// On overflow only the oldest backlog item is dropped. A small viewport
    /// can keep some display candidates waiting too.
    /// Visible readers are never evicted. These IDs are transient, not durable
    /// operation records; producers must retain important state elsewhere.
    pub(crate) fn push(&mut self, toast: Toast) -> ToastId {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.toast.same_message(&toast))
        {
            entry.remaining = toast.duration;
            entry.last_time = None;
            entry.toast = toast;
            return entry.id;
        }
        if self.entries.len() == MAX_TOASTS + MAX_PENDING_TOASTS {
            self.entries.remove(MAX_TOASTS);
        }
        self.next_id = self.next_id.checked_add(1).expect("Toast IDs exhausted");
        let id = ToastId(self.next_id);
        self.entries.push_back(Entry {
            id,
            remaining: toast.duration,
            toast,
            last_time: None,
            paused: true,
            dismiss_id: None,
            action_id: None,
        });
        id
    }

    /// Replace content in place and restart its reading duration.
    #[allow(dead_code)] // Producer-owned progress updates can use the same ID.
    pub(crate) fn update(&mut self, id: ToastId, toast: Toast) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        entry.remaining = toast.duration;
        entry.last_time = None;
        entry.toast = toast;
        if self.focused_entry == Some(id) {
            self.focus_request = Some(id);
        }
        true
    }

    #[allow(dead_code)] // Producers may finish their own persistent notification.
    pub(crate) fn dismiss(&mut self, id: ToastId) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        self.entries.remove(index);
        self.visible_ids.retain(|visible| *visible != id);
        if self.focused_entry == Some(id) {
            self.restore_requested = true;
        }
        if self.focus_request == Some(id) {
            self.focus_request = None;
        }
        true
    }

    #[allow(dead_code)] // A workspace reset may clear its transient feedback.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.visible_ids.clear();
        self.stack_rect = None;
        self.focus_request = None;
        self.restore_requested |= self.focused_entry.is_some();
    }

    /// Called after input dispatch. Focus enters the first toast on the next
    /// layout pass, preferring its action over dismissal. Entry never happens
    /// automatically just because a notification arrived.
    pub(crate) fn focus(&mut self, ctx: &Context) -> bool {
        let Some(first) = self.entries.front() else {
            return false;
        };
        let focused = ctx.memory(|memory| memory.focused());
        if !self.entries.iter().any(|entry| entry.owns_focus(focused)) {
            self.return_focus = Some(focused.unwrap_or_else(crate::shortcuts::viewport_focus_id));
        }
        self.focus_request = Some(first.id);
        ctx.request_repaint();
        true
    }

    fn restore_focus(&mut self, ctx: &Context) {
        let return_to = self
            .return_focus
            .take()
            .unwrap_or_else(crate::shortcuts::viewport_focus_id);
        if return_to == crate::shortcuts::viewport_focus_id() {
            crate::shortcuts::claim_viewport_input(ctx);
        } else {
            ctx.memory_mut(|memory| {
                memory.move_focus(egui::FocusDirection::None);
                memory.request_focus(return_to);
            });
        }
        self.focus_request = None;
        self.focused_entry = None;
        self.focused_widget = None;
        self.restore_requested = false;
    }

    /// `enabled = false` hides and suspends feedback during menus, dialogs and
    /// gestures. Hover or keyboard focus pauses the visible stack and holds new
    /// arrivals in the queue, keeping targets stable while someone reads. Queued
    /// duration begins on display; layout retries do not consume extra time.
    pub(crate) fn show(
        &mut self,
        ctx: &Context,
        bounds: Rect,
        palette: Palette,
        enabled: bool,
    ) -> Vec<Command> {
        let now = ctx.input(|input| input.time);
        let focused_before = ctx.memory(|memory| memory.focused());
        let focused_entry = self
            .entries
            .iter()
            .find(|entry| entry.owns_focus(focused_before))
            .map(|entry| entry.id);
        if self.restore_requested {
            // Do not steal focus if another UI has claimed it since a producer
            // dismissed the previously focused notification.
            if focused_before.is_none() || focused_before == self.focused_widget {
                self.restore_focus(ctx);
            } else {
                self.restore_requested = false;
                self.return_focus = None;
            }
        } else if (focused_entry.is_some()
            // egui can surrender newly acquired focus before our first chance
            // to install its Escape filter. Preserve the prior toast owner for
            // this one event, without taking focus from another widget.
            || (focused_before.is_none() && self.focused_widget.is_some()))
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.restore_focus(ctx);
        } else if focused_entry.is_none() && self.focus_request.is_none() {
            self.return_focus = None;
        }

        // First account for the interval that just ended. Current hover or
        // visibility determines the next interval, not the elapsed one.
        for entry in &mut self.entries {
            entry.advance(now, true);
        }
        self.entries.retain(|entry| !entry.expired());
        self.visible_ids
            .retain(|id| self.entries.iter().any(|entry| entry.id == *id));

        let bounds = bounds.shrink(theme::space::XL);
        let enabled = enabled
            && bounds.width() >= theme::size::STEP_24
            && bounds.height() >= MIN_TOAST_HEIGHT;
        if !enabled || self.entries.is_empty() {
            self.stack_rect = None;
            if self.entries.is_empty() {
                self.focus_request = None;
                self.return_focus = None;
            }
            return Vec::new();
        }

        let gap = theme::space::LG;
        let capacity = (((bounds.height() + gap) / (MIN_TOAST_HEIGHT + gap)) as usize)
            .clamp(1, self.entries.len().min(MAX_TOASTS));
        let held = self.focus_request.is_some()
            || self
                .entries
                .iter()
                .any(|entry| entry.owns_focus(ctx.memory(|memory| memory.focused())))
            || self.stack_rect.is_some_and(|rect| {
                ctx.pointer_hover_pos()
                    .is_some_and(|point| rect.contains(point))
            });
        let visible_count = if held && !self.visible_ids.is_empty() {
            self.visible_ids.len().min(capacity)
        } else {
            capacity
        };
        self.visible_ids = self
            .entries
            .iter()
            .take(visible_count)
            .map(|entry| entry.id)
            .collect();
        let toast_height = ((bounds.height() - gap * (visible_count - 1) as f32)
            / visible_count as f32)
            .min(theme::size::STEP_60);
        let width = bounds.width().min(TOAST_WIDTH);
        let newest_id = self.visible_ids[visible_count - 1];
        let focus_request = self.focus_request.map(|id| {
            if self.visible_ids.contains(&id) {
                id
            } else {
                self.visible_ids[0]
            }
        });
        let mut focused_removed = false;
        let mut focus_requested = false;
        let mut sizing_pass = false;
        let mut completed = Vec::new();
        let mut commands = Vec::new();

        let area = egui::Area::new(egui::Id::new("n3.toasts"))
            .order(egui::Order::Middle)
            .movable(false)
            .sense(Sense::click())
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(bounds.right_bottom())
            .constrain_to(bounds)
            .default_size(egui::vec2(width, bounds.height()))
            .show(ctx, |ui| {
                ui.set_clip_rect(bounds);
                ui.spacing_mut().item_spacing.y = gap;
                sizing_pass = ui.is_sizing_pass();
                // Stack arrivals upward so existing targets retain their
                // bottom offset. Reading a stack holds further promotions.
                for entry in self.entries.iter_mut().take(visible_count).rev() {
                    let is_newest = entry.id == newest_id;
                    let request_focus = focus_request == Some(entry.id);
                    let mut focused = false;
                    let mut close = false;
                    let mut action = false;
                    let frame = egui::Frame::popup(ui.style())
                        .fill(palette.popover)
                        .stroke(egui::Stroke::new(theme::space::PX, palette.border))
                        .inner_margin(theme::space::margin(theme::space::XL));
                    let inset = frame.total_margin();
                    let content_width = width - inset.sum().x;
                    let action_height = if entry.toast.action.is_some() {
                        theme::size::STEP_6 + gap
                    } else {
                        0.0
                    };
                    let body_height =
                        (toast_height - inset.sum().y - theme::size::STEP_6 - gap - action_height)
                            .max(theme::space::PX);
                    let response = ui
                        .push_id(entry.id, |ui| {
                            frame.show(ui, |ui| {
                                ui.set_width(content_width);
                                ui.horizontal(|ui| {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        // egui counts stroke/expansion in the
                                        // measured button frame. Fill-only
                                        // feedback keeps this target stationary.
                                        let visuals = ui.visuals_mut();
                                        for widget in [
                                            &mut visuals.widgets.inactive,
                                            &mut visuals.widgets.open,
                                            &mut visuals.widgets.hovered,
                                            &mut visuals.widgets.active,
                                        ] {
                                            widget.bg_stroke = egui::Stroke::NONE;
                                            widget.expansion = 0.0;
                                        }
                                        let dismiss = ui
                                            .add_sized(
                                                egui::vec2(
                                                    theme::size::STEP_6,
                                                    theme::size::STEP_6,
                                                ),
                                                egui::Button::new(
                                                    lucide::Icon::X.text(theme::text::BASE),
                                                )
                                                .frame_when_inactive(false),
                                            )
                                            .on_hover_text("Dismiss notification");
                                        dismiss.widget_info(|| {
                                            egui::WidgetInfo::labeled(
                                                egui::WidgetType::Button,
                                                dismiss.enabled(),
                                                Control::ToastDismiss.label(),
                                            )
                                        });
                                        entry.dismiss_id = Some(dismiss.id);
                                        if request_focus && entry.toast.action.is_none() {
                                            dismiss.request_focus();
                                            focus_requested = true;
                                        }
                                        close = dismiss.clicked();
                                        focused |= ctx.memory(|memory| memory.has_focus(dismiss.id));
                                        if is_newest {
                                            controls::record(
                                                ctx,
                                                Control::ToastDismiss,
                                                Control::ToastDismiss.label(),
                                                dismiss.rect,
                                                dismiss.enabled(),
                                            );
                                        }
                                        let color = match entry.toast.kind {
                                            ToastKind::Error => palette.destructive,
                                            ToastKind::Warning => ui.visuals().warn_fg_color,
                                            ToastKind::Info | ToastKind::Success => {
                                                palette.muted_foreground
                                            }
                                        };
                                        ui.with_layout(
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        theme::strong(entry.toast.kind.label())
                                                            .size(theme::text::XS)
                                                            .color(color),
                                                    )
                                                    .truncate(),
                                                )
                                                .on_hover_text(format!(
                                                    "{}: focus notifications. Escape: return to the previous control.",
                                                    controls::shortcut_label("notifications.focus")
                                                ));
                                            },
                                        );
                                    });
                                });
                                egui::ScrollArea::vertical()
                                    .id_salt("message")
                                    .max_height(body_height)
                                    .auto_shrink([false, true])
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&entry.toast.message)
                                                    .color(palette.popover_foreground),
                                            )
                                            .wrap(),
                                        );
                                    });
                                if let Some((label, _)) = &entry.toast.action {
                                    let response = ui.add(
                                        egui::Button::new(
                                            RichText::new(label).color(palette.primary),
                                        )
                                        .truncate(),
                                    );
                                    entry.action_id = Some(response.id);
                                    if request_focus {
                                        response.request_focus();
                                        focus_requested = true;
                                    }
                                    focused |= ctx.memory(|memory| memory.has_focus(response.id));
                                    action = response.clicked();
                                    if is_newest {
                                        controls::record(
                                            ctx,
                                            Control::ToastAction,
                                            label,
                                            response.rect,
                                            response.enabled(),
                                        );
                                    }
                                } else {
                                    entry.action_id = None;
                                }
                            })
                        })
                        .inner
                        .response;
                    if close || action {
                        completed.push(entry.id);
                        focused_removed |= focused;
                    }
                    if action && let Some((_, command)) = entry.toast.action {
                        commands.push(command);
                    }
                    let _ = response; // The containing area owns hover for the stack.
                }
            });
        self.stack_rect = Some(area.response.rect.intersect(bounds));
        let focused = ctx.memory(|memory| memory.focused());
        self.focused_entry = self
            .entries
            .iter()
            .find(|entry| entry.owns_focus(focused))
            .map(|entry| entry.id);
        self.focused_widget = self.focused_entry.and(focused);
        if let Some(id) = self.focused_widget {
            ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        escape: true,
                        ..Default::default()
                    },
                );
            });
        }
        let paused = self.focused_entry.is_some()
            || sizing_pass
            || self.stack_rect.is_some_and(|rect| {
                ctx.pointer_hover_pos()
                    .is_some_and(|point| rect.contains(point))
            });
        for entry in self.entries.iter_mut().take(visible_count) {
            entry.paused = paused;
            if !paused && let Some(remaining) = entry.remaining {
                ctx.request_repaint_after(remaining);
            }
        }
        if focus_requested && !sizing_pass {
            self.focus_request = None;
        }
        controls::record(
            ctx,
            Control::ToastStack,
            Control::ToastStack.label(),
            area.response.rect.intersect(bounds),
            true,
        );
        self.entries.retain(|entry| !completed.contains(&entry.id));
        self.visible_ids.retain(|id| !completed.contains(id));
        if focused_removed {
            self.restore_focus(ctx);
        }
        if !completed.is_empty() {
            ctx.request_repaint();
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_bounds_and_duplicate_refresh_preserve_other_messages() {
        let mut toaster = Toaster::default();
        let first = toaster.push(Toast::new("first"));
        toaster.push(Toast::new("second").kind(ToastKind::Success));
        toaster.push(Toast::new("third"));
        toaster.entries[1].advance(0.0, false);
        toaster.entries[1].advance(5.0, false);
        toaster.push(Toast::new("second").kind(ToastKind::Success));
        assert_eq!(toaster.entries.len(), 3);
        assert_eq!(toaster.entries[1].remaining, Some(DEFAULT_DURATION));
        toaster.push(Toast::new("fourth"));
        assert_eq!(toaster.entries.len(), 4);
        assert_eq!(toaster.entries.front().unwrap().id, first);
        assert_eq!(toaster.entries.back().unwrap().toast.message, "fourth");
    }

    #[test]
    fn distinct_actions_and_severities_are_not_coalesced() {
        let mut toaster = Toaster::default();
        toaster.push(Toast::new("Ready").action("Continue", Command::Frame));
        toaster.push(Toast::new("Ready").action("Continue", Command::SelectAll));
        toaster.push(
            Toast::new("Ready")
                .kind(ToastKind::Warning)
                .action("Continue", Command::SelectAll),
        );
        assert_eq!(toaster.entries.len(), 3);
    }

    #[test]
    fn ids_support_update_persistence_dismiss_and_clear() {
        let mut toaster = Toaster::default();
        let id = toaster.push(Toast::new("Working").persistent());
        assert_eq!(id, toaster.push(Toast::new("Working").persistent()));
        toaster.entries[0].advance(0.0, false);
        toaster.entries[0].advance(3600.0, false);
        assert_eq!(toaster.entries[0].remaining, None);
        assert!(!toaster.entries[0].expired());
        assert!(toaster.update(id, Toast::new("Finished").kind(ToastKind::Success)));
        assert_eq!(toaster.entries[0].id, id);
        assert_eq!(toaster.entries[0].remaining, Some(DEFAULT_DURATION));
        assert!(toaster.dismiss(id));
        assert!(!toaster.dismiss(id));
        assert!(!toaster.update(id, Toast::new("Too late")));
        let later = toaster.push(Toast::new("Next"));
        assert_ne!(later, id);
        toaster.clear();
        assert!(toaster.entries.is_empty());
        assert!(!toaster.update(later, Toast::new("Too late")));
    }

    #[test]
    fn burst_overflow_only_drops_backlog_and_never_display_candidates() {
        let mut toaster = Toaster::default();
        let ids: Vec<_> = (0..50)
            .map(|index| toaster.push(Toast::new(format!("Message {index}"))))
            .collect();
        assert_eq!(toaster.entries.len(), MAX_TOASTS + MAX_PENDING_TOASTS);
        assert_eq!(
            toaster
                .entries
                .iter()
                .take(3)
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            ids[..3]
        );
        assert_eq!(toaster.entries[3].id, ids[18]);
        assert_eq!(toaster.entries.back().unwrap().id, ids[49]);
        assert!(!toaster.dismiss(ids[3]));
    }

    #[test]
    fn expiry_uses_replay_time_and_suspension_preserves_remaining_time() {
        let mut toaster = Toaster::default();
        toaster.push(Toast::new("hello").duration(Duration::from_secs(4)));
        let entry = &mut toaster.entries[0];
        entry.advance(10.0, false);
        entry.advance(11.0, false);
        assert_eq!(entry.remaining, Some(Duration::from_secs(3)));
        entry.advance(11.0, false); // Same frame's layout retry.
        assert_eq!(entry.remaining, Some(Duration::from_secs(3)));
        entry.advance(12.0, true);
        assert_eq!(entry.remaining, Some(Duration::from_secs(2)));
        entry.advance(90.0, true);
        entry.advance(91.0, false);
        assert_eq!(entry.remaining, Some(Duration::from_secs(2)));
        entry.advance(93.0, false);
        assert!(entry.expired());
    }

    fn frame(
        toaster: &mut Toaster,
        ctx: &Context,
        bounds: Rect,
        time: f64,
        events: Vec<egui::Event>,
        enabled: bool,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(bounds),
            time: Some(time),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            controls::begin_pass(ui.ctx());
            ui.add(
                egui::TextEdit::singleline(&mut String::new())
                    .id(egui::Id::new("toast.test.return")),
            );
            commands.extend(toaster.show(
                ui.ctx(),
                bounds,
                Palette::from_context(ui.ctx(), Default::default()),
                enabled,
            ));
            if ui.ctx().current_pass_index() == 0 {
                ui.ctx().request_discard("Exercise toast layout retries");
            }
        });
        output.textures_delta.clear(); // Layout/input test without a renderer.
        commands
    }

    fn setup() -> (Toaster, Context, Rect) {
        let ctx = Context::default();
        crate::ui::workspace_ui::configure_context(&ctx);
        controls::enable(&ctx);
        (
            Toaster::default(),
            ctx,
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0)),
        )
    }

    #[test]
    fn action_fires_once_and_dismissal_does_not_fire_it() {
        for control in [Control::ToastAction, Control::ToastDismiss] {
            let (mut toaster, ctx, bounds) = setup();
            toaster.push(Toast::new("Use Move").action("Select Move", Command::Frame));
            for _ in 0..3 {
                frame(&mut toaster, &ctx, bounds, 0.0, vec![], true);
            }
            let position = controls::snapshot(&ctx).controls[&control].rect.center();
            let mut commands = Vec::new();
            for (index, pressed) in [true, false].into_iter().enumerate() {
                commands.extend(frame(
                    &mut toaster,
                    &ctx,
                    bounds,
                    0.1 + index as f64 * 0.1,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    true,
                ));
            }
            assert_eq!(
                commands.len(),
                usize::from(control == Control::ToastAction),
                "click {control:?} at {position:?}; {:?}",
                controls::snapshot(&ctx)
            );
            assert!(toaster.entries.is_empty());
            assert!(frame(&mut toaster, &ctx, bounds, 0.3, vec![], true).is_empty());
        }
    }

    #[test]
    fn hidden_time_is_suspended_and_expiry_removes_the_notification() {
        let (mut toaster, ctx, bounds) = setup();
        toaster.push(Toast::new("Finished").duration(Duration::from_secs(2)));
        frame(&mut toaster, &ctx, bounds, 0.0, vec![], true);
        frame(&mut toaster, &ctx, bounds, 1.0, vec![], true);
        assert_eq!(toaster.entries[0].remaining, Some(Duration::from_secs(1)));
        frame(&mut toaster, &ctx, bounds, 1.0, vec![], false);
        frame(&mut toaster, &ctx, bounds, 100.0, vec![], false);
        frame(&mut toaster, &ctx, bounds, 200.0, vec![], true);
        assert_eq!(toaster.entries[0].remaining, Some(Duration::from_secs(1)));
        frame(&mut toaster, &ctx, bounds, 201.0, vec![], true);
        assert!(toaster.entries.is_empty());
        assert!(controls::snapshot(&ctx).controls.is_empty());
    }

    fn key(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn keyboard_entry_escape_and_action_restore_the_previous_control() {
        let (mut toaster, ctx, bounds) = setup();
        let origin = egui::Id::new("toast.test.return");
        toaster.push(Toast::new("Ready").action("Continue", Command::Frame));
        frame(&mut toaster, &ctx, bounds, 0.0, vec![], true);
        ctx.memory_mut(|memory| memory.request_focus(origin));
        assert!(toaster.focus(&ctx));
        frame(&mut toaster, &ctx, bounds, 0.1, vec![], true);
        let action_id = toaster.entries[0].action_id.unwrap();
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(action_id));
        // Reproduce egui surrendering focus before a newly focused button can
        // install its filter; Escape must still restore the actual origin.
        ctx.memory_mut(|memory| memory.set_focus_lock_filter(action_id, Default::default()));
        frame(
            &mut toaster,
            &ctx,
            bounds,
            0.2,
            vec![key(egui::Key::Escape, true)],
            true,
        );
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(origin));
        assert_eq!(toaster.entries.len(), 1);
        frame(
            &mut toaster,
            &ctx,
            bounds,
            0.3,
            vec![key(egui::Key::Escape, false)],
            true,
        );
        assert!(toaster.focus(&ctx));
        frame(&mut toaster, &ctx, bounds, 0.4, vec![], true);
        let commands = frame(
            &mut toaster,
            &ctx,
            bounds,
            0.5,
            vec![key(egui::Key::Enter, true)],
            true,
        );
        assert_eq!(commands, vec![Command::Frame]);
        assert!(toaster.entries.is_empty());
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(origin));
    }

    #[test]
    fn focused_updates_and_producer_dismissal_restore_focus() {
        let (mut toaster, ctx, bounds) = setup();
        let origin = egui::Id::new("toast.test.return");
        let id = toaster.push(
            Toast::new("Working")
                .persistent()
                .action("Inspect", Command::Frame),
        );
        frame(&mut toaster, &ctx, bounds, 0.0, vec![], true);
        ctx.memory_mut(|memory| memory.request_focus(origin));
        toaster.focus(&ctx);
        frame(&mut toaster, &ctx, bounds, 0.1, vec![], true);
        assert!(toaster.update(id, Toast::new("Finished")));
        frame(&mut toaster, &ctx, bounds, 0.2, vec![], true);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            toaster.entries[0].dismiss_id
        );
        assert!(toaster.dismiss(id));
        frame(&mut toaster, &ctx, bounds, 0.3, vec![], true);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(origin));
        assert!(!toaster.focus(&ctx));
    }

    #[test]
    fn hovered_stack_holds_arrivals_and_pending_time_starts_on_display() {
        let (mut toaster, ctx, bounds) = setup();
        toaster.push(Toast::new("First"));
        frame(&mut toaster, &ctx, bounds, 0.0, vec![], true);
        let dismiss = controls::snapshot(&ctx).controls[&Control::ToastDismiss].rect;
        frame(
            &mut toaster,
            &ctx,
            bounds,
            1.0,
            vec![egui::Event::PointerMoved(dismiss.center())],
            true,
        );
        let second = toaster.push(Toast::new("Second"));
        frame(&mut toaster, &ctx, bounds, 60.0, vec![], true);
        assert_eq!(toaster.visible_ids.len(), 1);
        assert_eq!(
            controls::snapshot(&ctx).controls[&Control::ToastDismiss].rect,
            dismiss
        );
        assert_eq!(toaster.entries[0].remaining, Some(Duration::from_secs(5)));
        assert_eq!(toaster.entries[1].remaining, Some(DEFAULT_DURATION));
        frame(
            &mut toaster,
            &ctx,
            bounds,
            61.0,
            vec![egui::Event::PointerMoved(egui::pos2(5.0, 5.0))],
            true,
        );
        assert_eq!(toaster.visible_ids.len(), 2);
        assert_eq!(toaster.visible_ids[1], second);
        frame(&mut toaster, &ctx, bounds, 63.0, vec![], true);
        assert_eq!(toaster.entries[0].remaining, Some(Duration::from_secs(3)));
        assert_eq!(toaster.entries[1].remaining, Some(Duration::from_secs(4)));
    }

    #[test]
    fn long_messages_fit_narrow_viewport_and_pause_on_hover() {
        let (mut toaster, ctx, _) = setup();
        let bounds = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 300.0));
        for i in 0..3 {
            toaster.push(Toast::new(format!("{i}: {}", "Long message ".repeat(100))));
        }
        for _ in 0..3 {
            frame(&mut toaster, &ctx, bounds, 0.0, vec![], true);
        }
        let trace = controls::snapshot(&ctx);
        trace.validate().unwrap();
        let stack = trace.controls[&Control::ToastStack].rect;
        let dismiss = trace.controls[&Control::ToastDismiss].rect;
        assert!(bounds.contains_rect(stack));
        assert!(
            bounds.contains_rect(dismiss),
            "bounds={bounds:?} dismiss={dismiss:?} stack={stack:?}"
        );
        frame(
            &mut toaster,
            &ctx,
            bounds,
            1.0,
            vec![egui::Event::PointerMoved(dismiss.center())],
            true,
        );
        frame(&mut toaster, &ctx, bounds, 40.0, vec![], true);
        assert_eq!(toaster.entries[1].remaining, Some(Duration::from_secs(5)));
        assert_eq!(toaster.entries[2].remaining, Some(DEFAULT_DURATION));
        frame(&mut toaster, &ctx, bounds, 80.0, vec![], false);
        assert!(controls::snapshot(&ctx).controls.is_empty());
        assert_eq!(toaster.entries[1].remaining, Some(Duration::from_secs(5)));
    }
}
