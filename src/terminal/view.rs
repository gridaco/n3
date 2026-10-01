use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{TermMode, cell::Flags};
use alacritty_terminal::vte::ansi::{Color, CursorShape};
use egui::{FontId, Id, Pos2, Rect, Sense, Stroke, Vec2};

use super::{
    SessionStatus, TerminalSession,
    colors::resolve_color,
    input::{self, InputEncoder},
};
use crate::theme::{self, Palette};

/// Stable caller identity and interaction bookkeeping; the session owns output.
pub(crate) struct TerminalView {
    id: Id,
    input_frame: Option<u64>,
    keyboard_frame: Option<u64>,
    wheel_remainder: f32,
    encoder: InputEncoder,
    reported_focus: bool,
    selecting: bool,
    mouse_button: Option<egui::PointerButton>,
    preedit: String,
}

pub(crate) struct TerminalViewResponse {
    pub rect: Rect,
    pub content_rect: Rect,
    pub notice_rect: Rect,
    pub focused: bool,
    pub columns: usize,
    pub rows: usize,
    pub display_offset: usize,
    pub status: String,
    /// Process creation remains a native-host effect, outside UI layout passes.
    pub start_requested: bool,
}

impl TerminalView {
    pub(crate) fn new(id: Id) -> Self {
        Self {
            id,
            input_frame: None,
            keyboard_frame: None,
            wheel_remainder: 0.0,
            encoder: InputEncoder::default(),
            reported_focus: false,
            selecting: false,
            mouse_button: None,
            preedit: String::new(),
        }
    }

    pub(crate) fn focus_id(&self) -> Id {
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
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            );
        });
    }
    pub(crate) fn surrender_focus(&self, ctx: &egui::Context) {
        ctx.memory_mut(|memory| memory.surrender_focus(self.focus_id()));
    }

    /// Hidden panels still release terminal protocol focus and transient gestures.
    pub(crate) fn deactivate(&mut self, ctx: &egui::Context, session: &mut TerminalSession) {
        self.surrender_focus(ctx);
        if self.reported_focus && session.is_interactive() {
            session.send_input(input::focus_report(session.mode(), false));
        }
        self.reported_focus = false;
        self.encoder.reset();
        self.preedit.clear();
        self.selecting = false;
        self.mouse_button = None;
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut TerminalSession,
    ) -> TerminalViewResponse {
        let available = ui.available_size().max(Vec2::ZERO);
        let (rect, _) = ui.allocate_exact_size(available, Sense::hover());
        let visible = rect.intersect(ui.clip_rect());
        let palette = Palette::from_context(ui.ctx(), crate::settings::AccentColor::DEFAULT);
        session.set_default_colors(
            palette.foreground.to_array()[..3].try_into().unwrap(),
            palette.background.to_array()[..3].try_into().unwrap(),
        );
        session.poll(ui.ctx());
        let painter = ui.painter().with_clip_rect(visible);
        painter.rect_filled(rect, 0, palette.background);
        let padding = theme::space::LG;
        let status = session.status_text();
        let restart = matches!(
            session.status(),
            SessionStatus::Exited { .. } | SessionStatus::Failed(_)
        );
        let notice = painter.layout(
            status.clone(),
            FontId::proportional(theme::text::XS),
            palette.muted_foreground,
            (rect.width() - padding * 2.0 - if restart { 80.0 } else { 0.0 }).max(1.0),
        );
        let notice_height = (notice.size().y + padding * 2.0).max(theme::ROW_HEIGHT);
        let notice_rect = Rect::from_min_max(
            rect.min,
            Pos2::new(
                rect.right(),
                (rect.top() + notice_height).min(rect.bottom()),
            ),
        );
        painter
            .with_clip_rect(notice_rect.intersect(visible))
            .galley(
                notice_rect.left_top() + Vec2::splat(padding),
                notice,
                palette.muted_foreground,
            );
        let mut start_requested = false;
        if restart && notice_rect.width() > 100.0 {
            let button_rect = Rect::from_min_max(
                egui::pos2(rect.right() - 76.0, notice_rect.top() + 2.0),
                egui::pos2(rect.right() - padding, notice_rect.bottom() - 2.0),
            );
            start_requested = ui.put(button_rect, egui::Button::new("Restart")).clicked();
        }
        painter.hline(
            rect.x_range(),
            notice_rect.bottom(),
            Stroke::new(1.0, palette.border),
        );
        let content_rect = Rect::from_min_max(
            Pos2::new(rect.left(), notice_rect.bottom()),
            rect.right_bottom(),
        );
        let inner = content_rect.shrink(padding);
        let font = FontId::monospace(theme::text::UI_BODY_13);
        let cell_size = ui.fonts_mut(|fonts| {
            Vec2::new(
                fonts.glyph_width(&font, 'M'),
                fonts.row_height(&font).ceil(),
            )
        });
        let columns = (inner.width().max(0.0) / cell_size.x).floor() as usize;
        let rows = (inner.height().max(0.0) / cell_size.y).floor() as usize;
        if columns >= 2 && rows >= 1 {
            let pixels = cell_size * ui.ctx().pixels_per_point();
            session.set_cell_size(pixels.x.round() as u16, pixels.y.round() as u16);
            session.resize(columns, rows);
            start_requested |= matches!(session.status(), SessionStatus::Dormant);
        }
        let response = ui.interact(
            content_rect.intersect(visible),
            self.focus_id(),
            Sense::click_and_drag(),
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Other,
                true,
                format!("Terminal. {status}\n{}", session.visible_text()),
            )
        });
        let competing = egui::Popup::is_any_open(ui.ctx())
            || ui.memory(|memory| memory.top_modal_layer().is_some());
        let window_focused = ui.input(|input| input.focused);
        let pressed_here =
            response.contains_pointer() && ui.input(|input| input.pointer.any_pressed());
        if !competing && (response.clicked() || pressed_here) {
            self.request_focus(ui.ctx());
        }
        if !window_focused || response.clicked_elsewhere() {
            self.surrender_focus(ui.ctx());
        }
        let interactive = session.is_interactive();
        let can_scroll = response.hovered() && !competing && window_focused;
        let frame = ui.ctx().cumulative_frame_nr();
        if self.input_frame != Some(frame) {
            self.input_frame = Some(frame);
            let (events, modifiers) = ui.input(|input| (input.events.clone(), input.modifiers));
            let mode = session.mode();
            let focused = self.is_focused(ui.ctx()) && !competing && window_focused;
            if focused != self.reported_focus {
                if interactive {
                    session.send_input(input::focus_report(mode, focused));
                }
                self.reported_focus = focused;
            }
            if !focused {
                self.encoder.reset();
                self.preedit.clear();
            }
            if !window_focused || competing {
                self.selecting = false;
                self.mouse_button = None;
            }
            if can_scroll {
                let lines: f32 = events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::MouseWheel { unit, delta, .. } => Some(
                            delta.y
                                * match unit {
                                    egui::MouseWheelUnit::Point => 1.0 / cell_size.y,
                                    egui::MouseWheelUnit::Line => 1.0,
                                    egui::MouseWheelUnit::Page => session.rows() as f32,
                                },
                        ),
                        _ => None,
                    })
                    .sum();
                self.wheel_remainder += lines;
                let lines = self.wheel_remainder.trunc() as i32;
                self.wheel_remainder -= lines as f32;
                if lines != 0 {
                    let (column, row) = pointer_cell(
                        ui.input(|input| input.pointer.hover_pos())
                            .unwrap_or(inner.min),
                        inner,
                        cell_size,
                        session,
                    );
                    if interactive
                        && !modifiers.shift
                        && mode.intersects(TermMode::MOUSE_MODE | TermMode::ALT_SCREEN)
                    {
                        session
                            .send_input(input::wheel_report(mode, lines, column, row, modifiers));
                    } else {
                        session.scroll_lines(lines);
                    }
                }
            }
            if !competing && window_focused {
                self.pointer_events(&events, ui, session, inner, cell_size, mode);
            }
            if focused {
                self.keyboard_frame = Some(frame);
                self.request_focus(ui.ctx());
                // A live terminal owns Tab and Escape as data. Fixture preview
                // keeps ordinary egui traversal and Escape-to-leave behavior.
                ui.memory_mut(|memory| {
                    memory.set_focus_lock_filter(
                        self.focus_id(),
                        egui::EventFilter {
                            tab: interactive,
                            horizontal_arrows: true,
                            vertical_arrows: true,
                            escape: true,
                        },
                    )
                });
                let mut keyboard_events = Vec::new();
                for event in events {
                    if let egui::Event::Ime(ime) = &event {
                        match ime {
                            egui::ImeEvent::Preedit { text, .. } => self.preedit = text.clone(),
                            egui::ImeEvent::Commit(_) => self.preedit.clear(),
                            _ => {}
                        }
                    }
                    if matches!(event, egui::Event::Copy)
                        && (ui.ctx().os() == egui::os::OperatingSystem::Mac || modifiers.shift)
                    {
                        if let Some(text) = session.term.lock().selection_to_string() {
                            ui.ctx().copy_text(text);
                        }
                        continue;
                    }
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } = event
                        && (!interactive
                            || modifiers.shift
                                && matches!(key, egui::Key::PageUp | egui::Key::PageDown))
                    {
                        match key {
                            egui::Key::PageUp => session.scroll_lines(session.rows() as i32),
                            egui::Key::PageDown => session.scroll_lines(-(session.rows() as i32)),
                            egui::Key::Home if !interactive => {
                                session.scroll_to(session.history_size())
                            }
                            egui::Key::End if !interactive => session.scroll_to(0),
                            egui::Key::Escape if !interactive => self.surrender_focus(ui.ctx()),
                            _ => {}
                        }
                        continue;
                    }
                    keyboard_events.push(event);
                }
                if interactive {
                    let bytes =
                        self.encoder
                            .encode(&keyboard_events, modifiers, mode, ui.ctx().os());
                    if !bytes.is_empty() {
                        session.scroll_to(0);
                        session.term.lock().selection = None;
                        session.send_input(bytes);
                    }
                }
            }
        }
        if self.keyboard_frame == Some(frame) {
            ui.input_mut(|input| input.events.retain(|event| {
                !matches!(event, egui::Event::Key { key, .. } if interactive || *key != egui::Key::Tab)
                    && !matches!(event, egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Copy | egui::Event::Cut | egui::Event::Ime(_))
            }));
        }
        if can_scroll {
            ui.input_mut(|input| {
                input.events.retain(|event| {
                    !matches!(event, egui::Event::MouseWheel { .. } | egui::Event::Zoom(_))
                });
                input.smooth_scroll_delta = Vec2::ZERO;
            });
        }
        let focused = self.is_focused(ui.ctx());
        let content_painter = painter.with_clip_rect(inner.intersect(visible));
        let cursor_rect = paint_contents(
            &content_painter,
            inner.min,
            cell_size,
            &font,
            session,
            TerminalAppearance {
                palette,
                dark: ui.visuals().dark_mode,
                focused,
            },
        );
        if interactive && focused && !competing {
            ui.output_mut(|output| {
                output.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: content_rect,
                    cursor_rect,
                    should_interrupt_composition: false,
                })
            });
            if !self.preedit.is_empty() {
                let galley =
                    painter.layout_no_wrap(self.preedit.clone(), font.clone(), palette.foreground);
                let preedit_rect = Rect::from_min_size(cursor_rect.min, galley.size());
                content_painter.rect_filled(preedit_rect, 0, palette.background);
                content_painter.galley(preedit_rect.min, galley, palette.foreground);
                content_painter.hline(
                    preedit_rect.x_range(),
                    preedit_rect.bottom(),
                    Stroke::new(1.0, palette.foreground),
                );
            }
        }
        if session.history_size() > 0 && content_rect.height() > 0.0 {
            let track = Rect::from_min_max(
                Pos2::new(rect.right() - 4.0, content_rect.top()),
                content_rect.right_bottom(),
            );
            let total = (session.history_size() + session.rows()) as f32;
            let thumb_height = (track.height() * session.rows() as f32 / total)
                .max(12.0)
                .min(track.height());
            let fraction = 1.0 - session.display_offset() as f32 / session.history_size() as f32;
            let thumb = Rect::from_min_size(
                track.min + Vec2::new(0.0, fraction * (track.height() - thumb_height)),
                Vec2::new(3.0, thumb_height),
            );
            painter.rect_filled(thumb, 1, palette.border);
        }
        if focused {
            painter.rect_stroke(
                rect,
                0,
                Stroke::new(1.0, ui.visuals().selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }
        TerminalViewResponse {
            rect: visible,
            content_rect: content_rect.intersect(visible),
            notice_rect: notice_rect.intersect(visible),
            focused,
            columns: session.columns(),
            rows: session.rows(),
            display_offset: session.display_offset(),
            status,
            start_requested,
        }
    }

    fn pointer_events(
        &mut self,
        events: &[egui::Event],
        ui: &egui::Ui,
        session: &mut TerminalSession,
        inner: Rect,
        size: Vec2,
        mode: TermMode,
    ) {
        for event in events {
            let (pos, modifiers, button, pressed) = match event {
                egui::Event::PointerButton {
                    pos,
                    modifiers,
                    button,
                    pressed,
                } => (*pos, *modifiers, Some(*button), Some(*pressed)),
                egui::Event::PointerMoved(pos)
                    if self.selecting
                        || self.mouse_button.is_some()
                        || mode.contains(TermMode::MOUSE_MOTION) =>
                {
                    (*pos, ui.input(|i| i.modifiers), None, None)
                }
                _ => continue,
            };
            let inside = inner.intersect(ui.clip_rect()).contains(pos)
                && ui.ctx().layer_id_at(pos) == Some(ui.layer_id());
            // Ownership is decided on press. A Shift change during an app drag
            // must still send its release; a local selection stays local.
            let reporting = self.mouse_button.is_some()
                || (!self.selecting
                    && session.is_interactive()
                    && mode.intersects(TermMode::MOUSE_MODE)
                    && !modifiers.shift);
            if !inside && !self.selecting && self.mouse_button.is_none() {
                continue;
            }
            let (column, row) = pointer_cell(pos, inner, size, session);
            if reporting {
                let action = match (button, pressed) {
                    (Some(button), Some(true)) if inside => {
                        self.mouse_button = Some(button);
                        input::MouseAction::Press(button)
                    }
                    (Some(button), Some(false)) if self.mouse_button == Some(button) => {
                        self.mouse_button = None;
                        input::MouseAction::Release(button)
                    }
                    (None, None) => input::MouseAction::Move(self.mouse_button),
                    _ => continue,
                };
                session.send_input(input::mouse_report(mode, action, column, row, modifiers));
            } else {
                let point = Point::new(
                    Line(row as i32 - session.display_offset() as i32),
                    Column(column),
                );
                let side = if ((pos.x - inner.left()) / size.x).fract() < 0.5 {
                    Side::Left
                } else {
                    Side::Right
                };
                match (button, pressed) {
                    (Some(egui::PointerButton::Primary), Some(true)) if inside => {
                        self.selecting = true;
                        session.term.lock().selection =
                            Some(Selection::new(SelectionType::Simple, point, side));
                    }
                    (None, None) | (Some(egui::PointerButton::Primary), Some(false))
                        if self.selecting =>
                    {
                        if let Some(selection) = session.term.lock().selection.as_mut() {
                            selection.update(point, side);
                        }
                        if pressed == Some(false) {
                            self.selecting = false;
                        }
                    }
                    _ => {}
                }
                if pressed == Some(false) {
                    self.mouse_button = None;
                }
            }
        }
    }
}

fn pointer_cell(pos: Pos2, inner: Rect, size: Vec2, session: &TerminalSession) -> (usize, usize) {
    (
        ((pos.x - inner.left()) / size.x)
            .floor()
            .max(0.0)
            .min((session.columns() - 1) as f32) as usize,
        ((pos.y - inner.top()) / size.y)
            .floor()
            .max(0.0)
            .min((session.rows() - 1) as f32) as usize,
    )
}

struct TerminalAppearance {
    palette: Palette,
    dark: bool,
    focused: bool,
}

fn paint_contents(
    painter: &egui::Painter,
    origin: Pos2,
    size: Vec2,
    font: &FontId,
    session: &TerminalSession,
    appearance: TerminalAppearance,
) -> Rect {
    let TerminalAppearance {
        palette,
        dark,
        focused,
    } = appearance;
    let term = session.term.lock();
    let content = term.renderable_content();
    let cursor = content.cursor;
    let cursor_rect = Rect::from_min_size(
        origin
            + Vec2::new(
                cursor.point.column.0 as f32 * size.x,
                (cursor.point.line.0 + content.display_offset as i32) as f32 * size.y,
            ),
        size,
    );
    for indexed in content.display_iter {
        let cell = indexed.cell;
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        let row = indexed.point.line.0 + content.display_offset as i32;
        let columns = if cell.flags.contains(Flags::WIDE_CHAR) {
            2.0
        } else {
            1.0
        };
        let cell_rect = Rect::from_min_size(
            origin + Vec2::new(indexed.point.column.0 as f32 * size.x, row as f32 * size.y),
            Vec2::new(size.x * columns, size.y),
        );
        if !painter.clip_rect().intersects(cell_rect) {
            continue;
        }
        let mut fg = cell.fg;
        if cell.flags.contains(Flags::BOLD)
            && let Color::Named(named) = fg
        {
            fg = Color::Named(named.to_bright());
        }
        let mut foreground = resolve_color(fg, palette, dark, content.colors);
        let mut background = resolve_color(cell.bg, palette, dark, content.colors);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut foreground, &mut background);
        }
        if cell.flags.contains(Flags::DIM) {
            foreground = foreground.gamma_multiply(0.65);
        }
        let selected = content
            .selection
            .as_ref()
            .is_some_and(|selection| selection.contains(indexed.point));
        if selected {
            background = palette.secondary;
        }
        if indexed.point == cursor.point
            && focused
            && cursor.shape == CursorShape::Block
            && content.display_offset == 0
        {
            background = palette.foreground;
            foreground = palette.background;
        }
        if background != palette.background {
            painter.rect_filled(cell_rect, 0, background);
        }
        if cell.flags.contains(Flags::HIDDEN) || (cell.c == ' ' && cell.zerowidth().is_none()) {
            continue;
        }
        let mut text = cell.c.to_string();
        if let Some(combining) = cell.zerowidth() {
            text.extend(combining);
        }
        let format = egui::TextFormat {
            font_id: font.clone(),
            color: foreground,
            italics: cell.flags.contains(Flags::ITALIC),
            underline: if cell.flags.intersects(Flags::ALL_UNDERLINES) {
                Stroke::new(1.0, foreground)
            } else {
                Stroke::NONE
            },
            strikethrough: if cell.flags.contains(Flags::STRIKEOUT) {
                Stroke::new(1.0, foreground)
            } else {
                Stroke::NONE
            },
            ..Default::default()
        };
        let galley = painter.layout_job(egui::text::LayoutJob::single_section(text, format));
        painter
            .with_clip_rect(cell_rect.intersect(painter.clip_rect()))
            .galley(cell_rect.min, galley, foreground);
    }
    if cursor.shape != CursorShape::Hidden
        && content.display_offset == 0
        && painter.clip_rect().intersects(cursor_rect)
    {
        match if focused {
            cursor.shape
        } else {
            CursorShape::HollowBlock
        } {
            CursorShape::Beam => {
                painter.vline(
                    cursor_rect.left(),
                    cursor_rect.y_range(),
                    Stroke::new(1.5, palette.foreground),
                );
            }
            CursorShape::Underline => {
                painter.hline(
                    cursor_rect.x_range(),
                    cursor_rect.bottom() - 1.0,
                    Stroke::new(1.5, palette.foreground),
                );
            }
            CursorShape::HollowBlock => {
                painter.rect_stroke(
                    cursor_rect,
                    0,
                    Stroke::new(1.0, palette.foreground),
                    egui::StrokeKind::Inside,
                );
            }
            _ => {}
        }
    }
    cursor_rect
}
