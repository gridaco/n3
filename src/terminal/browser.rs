//! A browser owns no local process or PTY. Keep the Tool Dock contract without
//! compiling a native terminal dependency or presenting an executable prompt.
use crate::theme::{self, Palette};
use egui::{FontId, Id, Pos2, Rect, Sense, Stroke, Vec2};

const NOTICE: &str = "Local shells are unavailable in the browser.";

pub(crate) struct TerminalSession;

impl TerminalSession {
    pub(crate) fn placeholder() -> Self {
        Self
    }

    pub(crate) fn set_default_colors(&mut self, _foreground: [u8; 3], _background: [u8; 3]) {}

    pub(crate) fn poll(&mut self, _ctx: &egui::Context) {}
}

pub(crate) struct TerminalView {
    id: Id,
}

pub(crate) struct TerminalViewResponse {
    pub content_rect: Rect,
    pub notice_rect: Rect,
    pub focused: bool,
    pub status: String,
    pub start_requested: bool,
}

impl TerminalView {
    pub(crate) fn new(id: Id) -> Self {
        Self { id }
    }

    fn focus_id(&self) -> Id {
        self.id.with("contents")
    }

    pub(crate) fn is_focused(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|memory| memory.has_focus(self.focus_id()))
    }

    pub(crate) fn request_focus(&self, ctx: &egui::Context) {
        ctx.memory_mut(|memory| {
            memory.request_focus(self.focus_id());
            memory.set_focus_lock_filter(
                self.focus_id(),
                egui::EventFilter {
                    escape: true,
                    ..Default::default()
                },
            );
        });
    }

    pub(crate) fn deactivate(&mut self, ctx: &egui::Context, _session: &mut TerminalSession) {
        ctx.memory_mut(|memory| memory.surrender_focus(self.focus_id()));
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut TerminalSession,
    ) -> TerminalViewResponse {
        let (rect, _) = ui.allocate_exact_size(ui.available_size().max(Vec2::ZERO), Sense::hover());
        let visible = rect.intersect(ui.clip_rect());
        let palette = Palette::from_context(ui.ctx(), crate::settings::AccentColor::DEFAULT);
        let painter = ui.painter().with_clip_rect(visible);
        painter.rect_filled(rect, 0, palette.background);
        let padding = theme::space::LG;
        let notice = painter.layout(
            NOTICE.into(),
            FontId::proportional(theme::text::XS),
            palette.muted_foreground,
            (rect.width() - padding * 2.0).max(1.0),
        );
        let notice_rect = Rect::from_min_max(
            rect.min,
            Pos2::new(
                rect.right(),
                (rect.top() + (notice.size().y + padding * 2.0).max(theme::ROW_HEIGHT))
                    .min(rect.bottom()),
            ),
        );
        painter.galley(
            notice_rect.left_top() + Vec2::splat(padding),
            notice,
            palette.muted_foreground,
        );
        painter.hline(
            rect.x_range(),
            notice_rect.bottom(),
            Stroke::new(1.0, palette.border),
        );
        let content_rect = Rect::from_min_max(
            Pos2::new(rect.left(), notice_rect.bottom()),
            rect.right_bottom(),
        )
        .intersect(visible);
        let response = ui.interact(content_rect, self.focus_id(), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Other, true, format!("Terminal. {NOTICE}"))
        });
        let competing = egui::Popup::is_any_open(ui.ctx())
            || ui.memory(|memory| memory.top_modal_layer().is_some());
        if !competing && response.clicked() {
            self.request_focus(ui.ctx());
        }
        let focused = self.is_focused(ui.ctx()) && !competing;
        if !ui.input(|input| input.focused)
            || response.clicked_elsewhere()
            || (focused && ui.input(|input| input.key_pressed(egui::Key::Escape)))
        {
            self.deactivate(ui.ctx(), session);
        }
        // The shared dock claims viewport input for this frame, even when Escape
        // just relinquished terminal focus. This panel never dispatches commands.
        TerminalViewResponse {
            content_rect,
            notice_rect: notice_rect.intersect(visible),
            focused,
            status: NOTICE.into(),
            start_requested: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_terminal_never_requests_a_shell_and_escape_releases_focus() {
        let ctx = egui::Context::default();
        let mut view = TerminalView::new(Id::new("browser-terminal"));
        let mut session = TerminalSession::placeholder();
        session.set_default_colors([255; 3], [0; 3]);
        session.poll(&ctx);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            view.show(ui, &mut session);
        });
        output.textures_delta.clear();
        view.request_focus(&ctx);
        for escape in [false, true] {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 300.0))),
                events: if escape {
                    vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let output = view.show(ui, &mut session);
                assert_eq!(output.status, NOTICE);
                assert!(!output.start_requested);
                if !escape {
                    assert!(output.focused);
                }
                assert!(output.content_rect.is_positive());
                assert!(output.notice_rect.is_positive());
            });
            output.textures_delta.clear();
            assert_eq!(view.is_focused(&ctx), !escape);
        }
    }
}
