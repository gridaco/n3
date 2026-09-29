//! Paint-only tutorial cues derived from the same events delivered to the real UI.

use std::{collections::BTreeMap, collections::VecDeque, sync::LazyLock, time::Duration};

use crate::{
    keyboard_input::{NumberKey, NumberKeyEvent},
    pointer_policy::crossed_drag_threshold,
};
use egui::{Color32, CursorIcon, Event, Key, Modifiers, PointerButton, Pos2, Stroke};

// Keep the overlay's timing, geometry, and colors together.
pub const CUE_LIFETIME: Duration = Duration::from_millis(600);
const MAX_TRAIL_POINTS: usize = 48;
const TRAIL_STEP: f32 = 2.0;
const CURSOR_SCALE: f32 = 1.0;
const RING_RADIUS: f32 = 13.0;
const HUD_MARGIN: f32 = crate::theme::space::XL + crate::theme::space::XS;
const HUD_GAP: f32 = crate::theme::space::SM + crate::theme::space::PX;
const HUD_PADDING: egui::Vec2 = egui::vec2(
    crate::theme::space::LG + crate::theme::space::PX,
    crate::theme::space::MD,
);
const FONT_SIZE: f32 = crate::theme::text::GUIDE_CUE_13;
const INK: Color32 = Color32::from_rgb(18, 22, 29);
const PAPER: Color32 = Color32::from_rgb(250, 252, 255);
const ACCENT: Color32 = Color32::from_rgb(255, 207, 72);
const BUTTONS: [PointerButton; 5] = [
    PointerButton::Primary,
    PointerButton::Secondary,
    PointerButton::Middle,
    PointerButton::Extra1,
    PointerButton::Extra2,
];

// Hand contours are authored around the palm's event hotspot at (0, 0).
// Open fingers and folded knuckles remain legible at the native cursor scale.
const OPEN_HAND: &[[f32; 2]] = &[
    [-5., 15.],
    [-5., 12.],
    [-9., 7.],
    [-13., 1.],
    [-14., -2.],
    [-13., -4.],
    [-11., -4.],
    [-7., 0.],
    [-7., -12.],
    [-6., -14.],
    [-4., -14.],
    [-3., -12.],
    [-3., -4.],
    [-2., -4.],
    [-2., -16.],
    [-1., -18.],
    [1., -18.],
    [2., -16.],
    [2., -4.],
    [3., -4.],
    [3., -14.],
    [4., -16.],
    [6., -16.],
    [7., -14.],
    [7., -3.],
    [8., -3.],
    [8., -10.],
    [9., -12.],
    [11., -12.],
    [12., -10.],
    [12., 2.],
    [10., 8.],
    [7., 12.],
    [7., 15.],
];
const CLOSED_HAND: &[[f32; 2]] = &[
    [-5., 14.],
    [-5., 11.],
    [-9., 7.],
    [-13., 2.],
    [-13., -2.],
    [-11., -4.],
    [-8., -2.],
    [-8., -7.],
    [-7., -9.],
    [-4., -9.],
    [-3., -7.],
    [-3., -10.],
    [-1., -11.],
    [2., -11.],
    [3., -9.],
    [4., -10.],
    [7., -10.],
    [8., -7.],
    [10., -8.],
    [12., -6.],
    [12., 3.],
    [10., 8.],
    [7., 11.],
    [7., 14.],
];
static OPEN_HAND_TRIANGLES: LazyLock<Vec<u32>> = LazyLock::new(|| hand_triangles(OPEN_HAND));
static CLOSED_HAND_TRIANGLES: LazyLock<Vec<u32>> = LazyLock::new(|| hand_triangles(CLOSED_HAND));

fn hand_triangles(contour: &[[f32; 2]]) -> Vec<u32> {
    let coordinates: Vec<_> = contour
        .iter()
        .flatten()
        .map(|value| f64::from(*value))
        .collect();
    earcutr::earcut(&coordinates, &[], 2)
        .expect("Authored cursor contours are simple polygons")
        .into_iter()
        .map(|index| index as u32)
        .collect()
}

#[derive(Clone, Copy)]
struct Press {
    origin: Pos2,
    dragged: bool,
}

struct Recent<T> {
    value: T,
    remaining: Duration,
}

impl<T> Recent<T> {
    fn new(value: T) -> Self {
        Self {
            value,
            remaining: CUE_LIFETIME,
        }
    }
}

struct Release {
    position: Pos2,
    button: PointerButton,
    dragged: bool,
}

/// Egui's logical digit is identical for top-row and numpad input. Retain the
/// physical identity alongside it, just as the real shortcut dispatcher does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CueKey {
    Logical(Key),
    Number(NumberKey),
}

impl CueKey {
    fn from_event(index: usize, key: Key, pressed: bool, numbers: &[NumberKeyEvent]) -> Self {
        numbers
            .iter()
            .find(|event| {
                event.event_index == index
                    && event.pressed == pressed
                    && event.key.egui_key() == Some(key)
            })
            .map(|event| Self::Number(event.key))
            .unwrap_or_else(|| {
                // Scripted logical-only digits have the same top-row fallback
                // as shortcuts. A later physical top-row release must still
                // release this key instead of leaving a second identity held.
                (0..=9)
                    .map(NumberKey::TopRow)
                    .find(|number| number.egui_key() == Some(key))
                    .map_or(Self::Logical(key), Self::Number)
            })
    }

    fn logical(self) -> Option<Key> {
        match self {
            Self::Logical(key) => Some(key),
            Self::Number(key) => key.egui_key(),
        }
    }
}

pub struct VirtualInput {
    focused: bool,
    position: Option<Pos2>,
    modifiers: Modifiers,
    buttons: [Option<Press>; 5],
    keys: BTreeMap<CueKey, Modifiers>,
    trail: VecDeque<Pos2>,
    recent_pointer: Option<Recent<Release>>,
    recent_keys: Option<Recent<Vec<String>>>,
    recent_gesture: Option<Recent<String>>,
}

impl Default for VirtualInput {
    fn default() -> Self {
        Self {
            focused: true,
            position: None,
            modifiers: Modifiers::NONE,
            buttons: [None; 5],
            keys: BTreeMap::new(),
            trail: VecDeque::new(),
            recent_pointer: None,
            recent_keys: None,
            recent_gesture: None,
        }
    }
}

impl VirtualInput {
    pub fn advance(&mut self, elapsed: Duration) {
        expire(&mut self.recent_pointer, elapsed);
        expire(&mut self.recent_keys, elapsed);
        expire(&mut self.recent_gesture, elapsed);
        if !self.any_pressed() && self.recent_pointer.is_none() {
            self.trail.clear();
        }
    }

    pub fn observe(
        &mut self,
        events: &[Event],
        numbers: &[NumberKeyEvent],
        modifiers: Modifiers,
        focused: bool,
    ) {
        if !focused {
            self.clear_focus();
            return;
        }
        self.focused = true;
        self.modifiers = modifiers;
        for (index, event) in events.iter().enumerate() {
            match event {
                Event::WindowFocused(false) => self.clear_focus(),
                Event::WindowFocused(true) => self.focused = true,
                _ if !self.focused => {}
                Event::ModifiersChanged(modifiers) => self.modifiers = *modifiers,
                Event::PointerMoved(pos) => self.move_pointer(*pos),
                Event::PointerGone => {
                    self.position = None;
                    self.trail.clear();
                    self.recent_pointer = None;
                    // Like egui, leaving the window does not synthesize a button release.
                }
                Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers,
                } => {
                    self.modifiers = *modifiers;
                    if !pos.is_finite() {
                        continue;
                    }
                    self.move_pointer(*pos);
                    if *pressed {
                        if !self.any_pressed() {
                            self.trail.clear();
                            self.trail.push_back(*pos);
                            self.recent_pointer = None;
                        }
                        self.buttons[*button as usize].get_or_insert(Press {
                            origin: *pos,
                            dragged: false,
                        });
                    } else if let Some(press) = self.buttons[*button as usize].take() {
                        self.recent_pointer = Some(Recent::new(Release {
                            position: *pos,
                            button: *button,
                            dragged: press.dragged,
                        }));
                    }
                }
                Event::Key {
                    key,
                    pressed,
                    modifiers,
                    ..
                } => {
                    self.modifiers = *modifiers;
                    let key = CueKey::from_event(index, *key, *pressed, numbers);
                    if *pressed {
                        self.keys.entry(key).or_insert(*modifiers);
                        self.recent_keys = None;
                    } else if let Some(press_modifiers) = self.keys.get(&key) {
                        let labels = chord(
                            merge_modifiers(*press_modifiers, *modifiers),
                            self.keys.keys().copied(),
                        );
                        self.recent_keys = Some(Recent::new(labels));
                        self.keys.remove(&key);
                    }
                }
                Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } => {
                    self.modifiers = *modifiers;
                    if delta.is_finite() && *delta != egui::Vec2::ZERO {
                        self.recent_gesture = Some(Recent::new(format!(
                            "Scroll {:+.1}, {:+.1} {unit:?}",
                            delta.x, delta.y
                        )));
                    }
                }
                Event::Zoom(scale) if scale.is_finite() && *scale > 0.0 && *scale != 1.0 => {
                    self.recent_gesture = Some(Recent::new(format!("Pinch {scale:.2}×")));
                }
                Event::Rotate(angle) if angle.is_finite() && *angle != 0.0 => {
                    self.recent_gesture = Some(Recent::new(format!("Rotate {angle:+.2} rad")));
                }
                _ => {}
            }
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn position(&self) -> Option<Pos2> {
        self.position
    }

    pub fn is_pressed(&self, button: PointerButton) -> bool {
        self.buttons[button as usize].is_some()
    }

    pub fn key_down(&self, key: Key) -> bool {
        self.keys.keys().any(|held| held.logical() == Some(key))
    }

    pub fn summary(&self, cursor: CursorIcon) -> String {
        let position = self
            .position
            .map_or_else(|| "none".to_owned(), |p| format!("{:.1},{:.1}", p.x, p.y));
        let pointer = self.recent_pointer.as_ref().map_or_else(
            || "none".to_owned(),
            |cue| {
                format!(
                    "{}@{:.1},{:.1}/{}ms",
                    release_label(&cue.value),
                    cue.value.position.x,
                    cue.value.position.y,
                    cue.remaining.as_millis()
                )
            },
        );
        let keys = self.recent_keys.as_ref().map_or_else(
            || "none".to_owned(),
            |cue| format!("{}/{}ms", cue.value.join("+"), cue.remaining.as_millis()),
        );
        let gesture = self.recent_gesture.as_ref().map_or_else(
            || "none".to_owned(),
            |cue| format!("{}/{}ms", cue.value, cue.remaining.as_millis()),
        );
        format!(
            "focused={}; pointer={position}; cursor={cursor:?}; held_mouse=[{}]; held_keys=[{}]; released_mouse={pointer}; released_keys={keys}; gesture={gesture}; trail={}",
            self.focused,
            self.mouse_labels().join(","),
            chord(self.modifiers, self.keys.keys().copied()).join("+"),
            self.trail.len(),
        )
    }

    pub fn paint(&self, ctx: &egui::Context, viewport: egui::Rect, cursor: CursorIcon) {
        if !self.focused {
            return;
        }
        // A painter on the top layer creates no widgets, input regions, or focus targets.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Debug,
            egui::Id::new("documentation-virtual-input"),
        ));
        let fade = self.recent_pointer.as_ref().map_or(1.0, |cue| {
            cue.remaining.as_secs_f32() / CUE_LIFETIME.as_secs_f32()
        });
        if self.trail.len() > 1 {
            let points: Vec<_> = self.trail.iter().copied().collect();
            painter.add(egui::Shape::line(
                points.clone(),
                Stroke::new(5.0, INK.linear_multiply(fade)),
            ));
            painter.add(egui::Shape::line(
                points,
                Stroke::new(2.5, ACCENT.linear_multiply(fade)),
            ));
        }
        if let Some(cue) = &self.recent_pointer {
            let radius = RING_RADIUS + 7.0 * (1.0 - fade);
            painter.circle_stroke(
                cue.value.position,
                radius,
                Stroke::new(4.0, INK.linear_multiply(fade)),
            );
            painter.circle_stroke(
                cue.value.position,
                radius,
                Stroke::new(2.0, ACCENT.linear_multiply(fade)),
            );
        }
        if let Some(position) = self.position {
            if self.any_pressed() {
                painter.circle_stroke(position, RING_RADIUS, Stroke::new(4.0, INK));
                painter.circle_stroke(position, RING_RADIUS, Stroke::new(2.0, ACCENT));
            }
            paint_cursor(&painter, position, cursor, self.any_pressed());
        }
        if viewport.is_finite() && viewport.is_positive() {
            self.paint_hud(&painter.with_clip_rect(viewport), viewport, cursor);
        }
    }

    fn paint_hud(&self, painter: &egui::Painter, viewport: egui::Rect, cursor: CursorIcon) {
        let mut caps: Vec<(String, bool)> = chord(self.modifiers, self.keys.keys().copied())
            .into_iter()
            .map(|label| (label, true))
            .collect();
        if self.keys.is_empty()
            && let Some(cue) = &self.recent_keys
        {
            caps.push((format!("{} released", cue.value.join(" + ")), false));
        }
        caps.extend(self.mouse_labels().into_iter().map(|label| (label, true)));
        if let Some(cue) = &self.recent_pointer {
            caps.push((release_label(&cue.value), false));
        }
        if let Some(cue) = &self.recent_gesture {
            caps.push((cue.value.clone(), false));
        }
        if self.position.is_some() {
            let label = match cursor {
                CursorIcon::Grab => Some("Grab"),
                CursorIcon::Grabbing => Some("Grabbing"),
                CursorIcon::Move => Some("Move"),
                CursorIcon::AllScroll => Some("Orbit"),
                CursorIcon::PointingHand => Some("Pointer"),
                CursorIcon::Text | CursorIcon::VerticalText => Some("Text"),
                _ => None,
            };
            if let Some(label) = label {
                caps.push((label.to_owned(), false));
            }
        }
        let mut x = viewport.left() + HUD_MARGIN;
        let bottom = viewport.bottom() - HUD_MARGIN;
        let right = viewport.right() - HUD_MARGIN;
        for (label, active) in caps {
            let available = right - x - 2.0 * HUD_PADDING.x;
            if available < FONT_SIZE {
                break;
            }
            let text = painter.layout(
                label,
                egui::FontId::proportional(FONT_SIZE),
                if active { INK } else { PAPER },
                available,
            );
            let size = text.size() + 2.0 * HUD_PADDING;
            let rect = egui::Rect::from_min_size(egui::pos2(x, bottom - size.y), size);
            painter.rect_filled(
                rect,
                crate::theme::radius::MD,
                if active { ACCENT } else { INK },
            );
            painter.rect_stroke(
                rect,
                crate::theme::radius::MD,
                Stroke::new(1.0, PAPER),
                egui::StrokeKind::Inside,
            );
            painter.galley(rect.min + HUD_PADDING, text, PAPER);
            x += size.x + HUD_GAP;
        }
    }

    fn any_pressed(&self) -> bool {
        self.buttons.iter().any(Option::is_some)
    }

    fn move_pointer(&mut self, position: Pos2) {
        if !position.is_finite() {
            return;
        }
        self.position = Some(position);
        if self.any_pressed() {
            for press in self.buttons.iter_mut().flatten() {
                press.dragged |= crossed_drag_threshold(press.origin, position);
            }
            if self
                .trail
                .back()
                .is_none_or(|last| last.distance(position) >= TRAIL_STEP)
            {
                self.trail.push_back(position);
                if self.trail.len() > MAX_TRAIL_POINTS {
                    self.trail.pop_front();
                }
            }
        }
    }

    fn mouse_labels(&self) -> Vec<String> {
        BUTTONS
            .into_iter()
            .filter_map(|button| {
                self.buttons[button as usize].map(|press| {
                    format!(
                        "{} {}",
                        button_name(button),
                        if press.dragged { "drag" } else { "down" }
                    )
                })
            })
            .collect()
    }

    fn clear_focus(&mut self) {
        *self = Self {
            focused: false,
            ..Self::default()
        };
    }
}

fn expire<T>(cue: &mut Option<Recent<T>>, elapsed: Duration) {
    if let Some(current) = cue {
        current.remaining = current.remaining.saturating_sub(elapsed);
        if current.remaining.is_zero() {
            *cue = None;
        }
    }
}

fn button_name(button: PointerButton) -> &'static str {
    match button {
        PointerButton::Primary => "Left",
        PointerButton::Secondary => "Right",
        PointerButton::Middle => "Middle",
        PointerButton::Extra1 => "Mouse 4",
        PointerButton::Extra2 => "Mouse 5",
    }
}

fn release_label(release: &Release) -> String {
    format!(
        "{} {}",
        button_name(release.button),
        if release.dragged {
            "drag released"
        } else {
            "released"
        }
    )
}

fn merge_modifiers(a: Modifiers, b: Modifiers) -> Modifiers {
    Modifiers {
        alt: a.alt || b.alt,
        ctrl: a.ctrl || b.ctrl,
        shift: a.shift || b.shift,
        mac_cmd: a.mac_cmd || b.mac_cmd,
        command: a.command || b.command,
    }
}

fn chord(modifiers: Modifiers, keys: impl Iterator<Item = CueKey>) -> Vec<String> {
    let mut labels = Vec::new();
    for (down, label) in [
        (modifiers.ctrl, "Control"),
        (modifiers.alt, "Option"),
        (modifiers.shift, "Shift"),
        (
            modifiers.mac_cmd || (modifiers.command && !modifiers.ctrl),
            "Command",
        ),
    ] {
        if down {
            labels.push(label.to_owned());
        }
    }
    labels.extend(keys.map(|key| match key {
        CueKey::Number(NumberKey::Numpad(digit)) => format!("Numpad {digit}"),
        CueKey::Number(NumberKey::TopRow(digit)) => digit.to_string(),
        CueKey::Logical(key) => key.name().to_owned(),
    }));
    labels
}

fn paint_cursor(painter: &egui::Painter, position: Pos2, icon: CursorIcon, pressed: bool) {
    if icon == CursorIcon::None {
        return;
    }
    let fill = if pressed { ACCENT } else { PAPER };
    if matches!(icon, CursorIcon::Move | CursorIcon::AllScroll) {
        // Four arrows share the event hotspot at the center of the move cursor.
        let mut lines = Vec::with_capacity(12);
        for direction in [egui::Vec2::X, -egui::Vec2::X, egui::Vec2::Y, -egui::Vec2::Y] {
            let tip = direction * 11.0;
            let side = egui::vec2(-direction.y, direction.x) * 4.0;
            lines.push([egui::Vec2::ZERO, tip]);
            lines.push([tip - direction * 4.0 + side, tip]);
            lines.push([tip - direction * 4.0 - side, tip]);
        }
        for (width, color) in [(4.0, INK), (2.0, fill)] {
            for line in &lines {
                painter.line_segment(
                    line.map(|offset| position + offset * CURSOR_SCALE),
                    Stroke::new(width, color),
                );
            }
        }
        return;
    }
    if matches!(icon, CursorIcon::Grab | CursorIcon::Grabbing) {
        paint_hand(painter, position, icon == CursorIcon::Grabbing, fill);
        return;
    }
    if matches!(icon, CursorIcon::Text | CursorIcon::VerticalText) {
        let mut lines = [
            [egui::vec2(0.0, -10.0), egui::vec2(0.0, 10.0)],
            [egui::vec2(-4.0, -10.0), egui::vec2(4.0, -10.0)],
            [egui::vec2(-4.0, 10.0), egui::vec2(4.0, 10.0)],
        ];
        if icon == CursorIcon::VerticalText {
            for point in lines.iter_mut().flatten() {
                *point = egui::vec2(point.y, point.x);
            }
        }
        for line in lines {
            let points = line.map(|offset| position + offset * CURSOR_SCALE);
            painter.line_segment(points, Stroke::new(4.0, INK));
            painter.line_segment(points, Stroke::new(2.0, fill));
        }
        return;
    }
    // The first vertex is the event's exact hotspot. Triangles fill the concave arrow.
    let points = [
        (0.0, 0.0),
        (0.0, 22.0),
        (6.0, 17.0),
        (10.0, 26.0),
        (14.0, 24.0),
        (10.0, 15.0),
        (18.0, 15.0),
    ]
    .map(|(x, y)| position + egui::vec2(x, y) * CURSOR_SCALE);
    let mut mesh = egui::Mesh::default();
    for point in points {
        mesh.colored_vertex(point, fill);
    }
    for [a, b, c] in [[0, 1, 2], [0, 2, 5], [0, 5, 6], [2, 3, 4], [2, 4, 5]] {
        mesh.add_triangle(a, b, c);
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.add(egui::Shape::closed_line(
        points.to_vec(),
        Stroke::new(2.0, INK),
    ));
}

fn paint_hand(painter: &egui::Painter, position: Pos2, closed: bool, fill: Color32) {
    let (contour, triangles) = if closed {
        (CLOSED_HAND, &*CLOSED_HAND_TRIANGLES)
    } else {
        (OPEN_HAND, &*OPEN_HAND_TRIANGLES)
    };
    let points: Vec<_> = contour
        .iter()
        .map(|[x, y]| position + egui::vec2(*x, *y) * CURSOR_SCALE)
        .collect();
    let mut mesh = egui::Mesh::default();
    for point in &points {
        mesh.colored_vertex(*point, fill);
    }
    mesh.indices.clone_from(triangles);
    painter.add(egui::Shape::mesh(mesh));
    painter.add(egui::Shape::closed_line(points, Stroke::new(2.0, INK)));
    if closed {
        for x in [-3.0, 2.0, 7.0] {
            painter.line_segment(
                [
                    position + egui::vec2(x, -6.0) * CURSOR_SCALE,
                    position + egui::vec2(x, -1.0) * CURSOR_SCALE,
                ],
                Stroke::new(1.2, INK),
            );
        }
    }
    let crease = if closed {
        [[-8., -1.], [-3., 4.], [1., 4.]]
    } else {
        [[-4., 5.], [0., 3.], [5., 4.]]
    };
    painter.add(egui::Shape::line(
        crease
            .map(|[x, y]| position + egui::vec2(x, y) * CURSOR_SCALE)
            .to_vec(),
        Stroke::new(1.2, INK),
    ));
}

#[cfg(test)]
#[path = "tests/doc_input_tests.rs"]
mod tests;
