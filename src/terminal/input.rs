//! Classic VT/xterm input encoding. The view owns focus and once-per-frame
//! delivery; this module only turns already-owned input into PTY bytes.
//!
//! Printable text follows egui's logical Text events, preserving keyboard layouts
//! and UTF-8. Physical keys are never used to reconstruct ordinary text. macOS
//! Option retains native text composition; Alt prefixes text with ESC elsewhere.
//! Cmd remains available to the host clipboard, while physical Control emits
//! terminal control bytes. This is not the application's shortcut catalog.
use alacritty_terminal::term::TermMode;
use egui::{Event, ImeEvent, Key, Modifiers, PointerButton, os::OperatingSystem};

#[derive(Default)]
pub(super) struct InputEncoder {
    composing: bool,
}

impl InputEncoder {
    pub fn reset(&mut self) {
        self.composing = false;
    }

    pub fn encode(
        &mut self,
        events: &[Event],
        modifiers: Modifiers,
        mode: TermMode,
        os: OperatingSystem,
    ) -> Vec<u8> {
        let mut output = Vec::new();
        let mut modifiers = modifiers;
        // Some integrations mirror IME commits as Text. Suppress one matching
        // Text per commit within this batch; ordinary repeated text remains valid.
        let mut mirrored: Vec<&str> = events
            .iter()
            .filter_map(|event| match event {
                Event::Ime(ImeEvent::Commit(text)) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        let ime_frame = self.composing
            || !mirrored.is_empty()
            || events.iter().any(|event| {
                matches!(event, Event::Ime(ImeEvent::Preedit { text, .. }) if !text.is_empty())
            });
        for event in events {
            match event {
                Event::ModifiersChanged(current) => modifiers = *current,
                Event::Key {
                    key,
                    pressed: true,
                    modifiers: current,
                    ..
                } => {
                    modifiers = *current;
                    // The composition window owns its Enter/arrows/backspace.
                    // egui-winit normally omits these keys, but other consumers
                    // may include both the physical key and the final commit.
                    if !ime_frame {
                        output.extend(key_bytes(*key, *current, mode, os));
                    }
                }
                Event::Text(text) => {
                    if let Some(index) = mirrored.iter().position(|commit| *commit == text) {
                        mirrored.remove(index);
                        continue;
                    }
                    if !self.composing && !command_owned(modifiers, os) && !control(modifiers, os) {
                        if modifiers.alt && !os.is_mac() {
                            output.push(0x1b);
                        }
                        output.extend_from_slice(text.as_bytes());
                    }
                }
                Event::Ime(ImeEvent::Preedit { text, .. }) => self.composing = !text.is_empty(),
                Event::Ime(ImeEvent::Commit(text)) => {
                    self.composing = false;
                    output.extend_from_slice(text.as_bytes());
                }
                Event::Paste(text) => {
                    // egui-winit maps plain Ctrl+V to Paste on non-macOS hosts.
                    // Ctrl+Shift+V and explicit menu paste remain clipboard input.
                    if !os.is_mac() && control(modifiers, os) && !modifiers.shift {
                        output.push(0x16);
                    } else {
                        output.extend(paste_bytes(text, mode));
                    }
                }
                Event::Copy | Event::Cut => {
                    // On macOS Cmd+C is clipboard; Ctrl+C reaches Key above.
                    // Elsewhere egui-winit reports Ctrl+C/X as these semantic
                    // events, so recover their terminal meaning from modifiers.
                    if !os.is_mac() && control(modifiers, os) && !modifiers.shift {
                        output.push(if matches!(event, Event::Copy) {
                            0x03
                        } else {
                            0x18
                        });
                    }
                }
                Event::WindowFocused(false) => self.reset(),
                _ => {}
            }
        }
        output
    }
}

fn command_owned(modifiers: Modifiers, os: OperatingSystem) -> bool {
    modifiers.mac_cmd || (os.is_mac() && modifiers.command)
}

fn control(modifiers: Modifiers, os: OperatingSystem) -> bool {
    modifiers.ctrl || (!os.is_mac() && modifiers.command)
}

fn parameter(modifiers: Modifiers, os: OperatingSystem) -> u8 {
    1 + u8::from(modifiers.shift)
        + 2 * u8::from(modifiers.alt)
        + 4 * u8::from(control(modifiers, os))
}

fn key_bytes(key: Key, modifiers: Modifiers, mode: TermMode, os: OperatingSystem) -> Vec<u8> {
    if command_owned(modifiers, os)
        || (!os.is_mac()
            && control(modifiers, os)
            && modifiers.shift
            && matches!(key, Key::C | Key::V | Key::X))
    {
        return Vec::new();
    }
    let param = parameter(modifiers, os);
    let cursor = match key {
        Key::ArrowUp => Some('A'),
        Key::ArrowDown => Some('B'),
        Key::ArrowRight => Some('C'),
        Key::ArrowLeft => Some('D'),
        Key::Home => Some('H'),
        Key::End => Some('F'),
        _ => None,
    };
    if let Some(final_byte) = cursor {
        return if param != 1 {
            format!("\x1b[1;{param}{final_byte}").into_bytes()
        } else if mode.contains(TermMode::APP_CURSOR) {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        };
    }
    let numbered = match key {
        Key::Insert => Some(2),
        Key::Delete => Some(3),
        Key::PageUp => Some(5),
        Key::PageDown => Some(6),
        _ => None,
    };
    if let Some(number) = numbered {
        return tilde(number, param);
    }
    let function = match key {
        Key::F1 => Some(1),
        Key::F2 => Some(2),
        Key::F3 => Some(3),
        Key::F4 => Some(4),
        Key::F5 => Some(5),
        Key::F6 => Some(6),
        Key::F7 => Some(7),
        Key::F8 => Some(8),
        Key::F9 => Some(9),
        Key::F10 => Some(10),
        Key::F11 => Some(11),
        Key::F12 => Some(12),
        Key::F13 => Some(13),
        Key::F14 => Some(14),
        Key::F15 => Some(15),
        Key::F16 => Some(16),
        Key::F17 => Some(17),
        Key::F18 => Some(18),
        Key::F19 => Some(19),
        Key::F20 => Some(20),
        Key::F21 => Some(21),
        Key::F22 => Some(22),
        Key::F23 => Some(23),
        Key::F24 => Some(24),
        _ => None,
    };
    if let Some(function) = function {
        // F13-F24 are the conventional shifted F1-F12 xterm sequences.
        let param = if function > 12 {
            ((param - 1) | 1) + 1
        } else {
            param
        };
        let number = (function - 1) % 12;
        return if number < 4 {
            let final_byte = char::from(b'P' + number);
            if param == 1 {
                format!("\x1bO{final_byte}").into_bytes()
            } else {
                format!("\x1b[1;{param}{final_byte}").into_bytes()
            }
        } else {
            tilde([15, 17, 18, 19, 20, 21, 23, 24][number as usize - 4], param)
        };
    }
    if key == Key::Tab && modifiers.shift {
        return if param == 2 {
            b"\x1b[Z".to_vec()
        } else {
            format!("\x1b[1;{param}Z").into_bytes()
        };
    }
    let byte = match key {
        // Classic terminal input has no distinct Shift+Enter code. Both send CR;
        // Alt+Enter prefixes ESC. Extended keyboard protocols are not negotiated.
        Key::Enter => Some(b'\r'),
        Key::Backspace => Some(if control(modifiers, os) { 0x08 } else { 0x7f }),
        Key::Tab => Some(b'\t'),
        Key::Escape => Some(0x1b),
        _ if control(modifiers, os) => control_byte(key),
        _ => None,
    };
    let Some(byte) = byte else {
        return Vec::new();
    };
    let mut output = Vec::with_capacity(2);
    if modifiers.alt {
        output.push(0x1b);
    }
    output.push(byte);
    output
}

fn tilde(number: u8, param: u8) -> Vec<u8> {
    if param == 1 {
        format!("\x1b[{number}~").into_bytes()
    } else {
        format!("\x1b[{number};{param}~").into_bytes()
    }
}

fn control_byte(key: Key) -> Option<u8> {
    let name = key.name().as_bytes();
    if name.len() == 1 && name[0].is_ascii_alphabetic() {
        return Some(name[0].to_ascii_uppercase() - b'A' + 1);
    }
    match key {
        Key::Space | Key::Num2 | Key::Backtick => Some(0x00),
        Key::OpenBracket | Key::OpenCurlyBracket | Key::Num3 => Some(0x1b),
        Key::Backslash | Key::Pipe | Key::Num4 => Some(0x1c),
        Key::CloseBracket | Key::CloseCurlyBracket | Key::Num5 => Some(0x1d),
        Key::Num6 => Some(0x1e),
        Key::Slash | Key::Minus | Key::Num7 => Some(0x1f),
        Key::Questionmark | Key::Num8 => Some(0x7f),
        _ => None,
    }
}

fn paste_bytes(text: &str, mode: TermMode) -> Vec<u8> {
    // Clipboard contents cannot terminate bracketed paste or inject terminal
    // escapes/control keys. Preserve Unicode, tabs and newlines; normalize CRLF.
    let clean: String = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect();
    if clean.is_empty() {
        return Vec::new();
    }
    if mode.contains(TermMode::BRACKETED_PASTE) {
        [
            b"\x1b[200~".as_slice(),
            clean.as_bytes(),
            b"\x1b[201~".as_slice(),
        ]
        .concat()
    } else {
        clean.replace('\n', "\r").into_bytes()
    }
}

pub(super) fn focus_report(mode: TermMode, focused: bool) -> Vec<u8> {
    if !mode.contains(TermMode::FOCUS_IN_OUT) {
        return Vec::new();
    }
    if focused {
        b"\x1b[I".to_vec()
    } else {
        b"\x1b[O".to_vec()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum MouseAction {
    Press(PointerButton),
    Release(PointerButton),
    Move(Option<PointerButton>),
    WheelUp,
    WheelDown,
}

/// Grid coordinates are zero based. The view chooses when Shift bypasses mouse
/// reporting for local selection, and keeps drag/release ownership outside cells.
pub(super) fn mouse_report(
    mode: TermMode,
    action: MouseAction,
    column: usize,
    row: usize,
    modifiers: Modifiers,
) -> Vec<u8> {
    if !mode.intersects(TermMode::MOUSE_MODE) {
        return Vec::new();
    }
    let release = matches!(action, MouseAction::Release(_));
    let button = |button: PointerButton| match button {
        PointerButton::Primary => Some(0),
        PointerButton::Middle => Some(1),
        PointerButton::Secondary => Some(2),
        _ => None,
    };
    let code = match action {
        MouseAction::Press(b) | MouseAction::Release(b) => button(b),
        MouseAction::Move(held) => {
            let reports_motion = mode.contains(TermMode::MOUSE_MOTION)
                || (held.is_some() && mode.contains(TermMode::MOUSE_DRAG));
            if !reports_motion {
                return Vec::new();
            }
            held.map_or(Some(3), button).map(|b| b + 32)
        }
        MouseAction::WheelUp => Some(64),
        MouseAction::WheelDown => Some(65),
    };
    let Some(mut code) = code else {
        return Vec::new();
    };
    let Some(column) = column.checked_add(1) else {
        return Vec::new();
    };
    let Some(row) = row.checked_add(1) else {
        return Vec::new();
    };
    if release && !mode.contains(TermMode::SGR_MOUSE) {
        code = 3;
    }
    code +=
        u8::from(modifiers.shift) * 4 + u8::from(modifiers.alt) * 8 + u8::from(modifiers.ctrl) * 16;
    if mode.contains(TermMode::SGR_MOUSE) {
        let final_byte = if release { 'm' } else { 'M' };
        return format!("\x1b[<{code};{column};{row}{final_byte}").into_bytes();
    }
    let utf8 = mode.contains(TermMode::UTF8_MOUSE);
    let limit = if utf8 { 2015 } else { 223 };
    if column > limit || row > limit {
        return Vec::new();
    }
    let mut output = vec![0x1b, b'[', b'M', code + 32];
    for coordinate in [column, row] {
        let coordinate = (coordinate + 32) as u32;
        if utf8 {
            let mut buffer = [0; 4];
            output.extend_from_slice(
                char::from_u32(coordinate)
                    .unwrap()
                    .encode_utf8(&mut buffer)
                    .as_bytes(),
            );
        } else {
            output.push(coordinate as u8);
        }
    }
    output
}

/// Positive lines mean wheel up. Ordinary screen scrollback stays in the view;
/// application mouse mode takes precedence over alternate-screen arrow scrolling.
pub(super) fn wheel_report(
    mode: TermMode,
    lines: i32,
    column: usize,
    row: usize,
    modifiers: Modifiers,
) -> Vec<u8> {
    let bytes = if mode.intersects(TermMode::MOUSE_MODE) {
        mouse_report(
            mode,
            if lines > 0 {
                MouseAction::WheelUp
            } else {
                MouseAction::WheelDown
            },
            column,
            row,
            modifiers,
        )
    } else if mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL) {
        key_bytes(
            if lines > 0 {
                Key::ArrowUp
            } else {
                Key::ArrowDown
            },
            Modifiers::NONE,
            mode,
            OperatingSystem::Unknown,
        )
    } else {
        Vec::new()
    };
    // Bound one synthetic wheel event; native wheel deltas are much smaller.
    bytes.repeat(lines.unsigned_abs().min(256) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn encode(
        events: &[Event],
        modifiers: Modifiers,
        mode: TermMode,
        os: OperatingSystem,
    ) -> Vec<u8> {
        InputEncoder::default().encode(events, modifiers, mode, os)
    }

    #[test]
    fn logical_utf8_text_is_not_duplicated_by_key_events_and_repeats_survive() {
        assert_eq!(
            encode(
                &[key(Key::A, Modifiers::NONE), Event::Text("å한🙂".into())],
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            "å한🙂".as_bytes()
        );
        assert_eq!(
            encode(
                &[Event::Text("a".into()), Event::Text("a".into())],
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Nix
            ),
            b"aa"
        );
        let mut released = key(Key::Enter, Modifiers::NONE);
        if let Event::Key { pressed, .. } = &mut released {
            *pressed = false;
        }
        assert!(
            encode(
                &[released],
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Mac
            )
            .is_empty()
        );
    }

    #[test]
    fn physical_control_is_distinct_from_command_and_clipboard_events() {
        for (key_value, byte) in [
            (Key::C, 3),
            (Key::D, 4),
            (Key::Z, 26),
            (Key::J, 10),
            (Key::M, 13),
            (Key::Space, 0),
            (Key::OpenBracket, 27),
            (Key::Backslash, 28),
            (Key::CloseBracket, 29),
            (Key::Num6, 30),
            (Key::Minus, 31),
        ] {
            assert_eq!(
                key_bytes(
                    key_value,
                    Modifiers::CTRL,
                    TermMode::NONE,
                    OperatingSystem::Mac
                ),
                [byte]
            );
            assert!(
                key_bytes(
                    key_value,
                    Modifiers::COMMAND,
                    TermMode::NONE,
                    OperatingSystem::Mac
                )
                .is_empty()
            );
        }
        let ctrl = Modifiers {
            ctrl: true,
            command: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            encode(
                &[Event::Copy, Event::Cut, Event::Paste("clipboard".into())],
                ctrl,
                TermMode::NONE,
                OperatingSystem::Nix
            ),
            [3, 24, 22]
        );
        let ctrl_shift = Modifiers {
            shift: true,
            ..ctrl
        };
        assert_eq!(
            encode(
                &[Event::Copy, Event::Paste("clipboard".into())],
                ctrl_shift,
                TermMode::NONE,
                OperatingSystem::Nix
            ),
            b"clipboard"
        );
        assert!(key_bytes(Key::C, ctrl_shift, TermMode::NONE, OperatingSystem::Nix).is_empty());
        assert_eq!(
            encode(
                &[Event::Copy, Event::Paste("clipboard".into())],
                Modifiers::COMMAND,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"clipboard"
        );
    }

    #[test]
    fn special_keys_application_cursor_and_xterm_modifiers_are_encoded() {
        for (key_value, expected) in [
            (Key::Enter, "\r"),
            (Key::Backspace, "\x7f"),
            (Key::Tab, "\t"),
            (Key::Escape, "\x1b"),
            (Key::ArrowUp, "\x1b[A"),
            (Key::Home, "\x1b[H"),
            (Key::End, "\x1b[F"),
            (Key::Insert, "\x1b[2~"),
            (Key::Delete, "\x1b[3~"),
            (Key::PageUp, "\x1b[5~"),
            (Key::PageDown, "\x1b[6~"),
            (Key::F1, "\x1bOP"),
            (Key::F12, "\x1b[24~"),
        ] {
            assert_eq!(
                key_bytes(
                    key_value,
                    Modifiers::NONE,
                    TermMode::NONE,
                    OperatingSystem::Mac
                ),
                expected.as_bytes()
            );
        }
        assert_eq!(
            key_bytes(
                Key::ArrowLeft,
                Modifiers::NONE,
                TermMode::APP_CURSOR,
                OperatingSystem::Mac
            ),
            b"\x1bOD"
        );
        assert_eq!(
            key_bytes(
                Key::ArrowRight,
                Modifiers::CTRL,
                TermMode::APP_CURSOR,
                OperatingSystem::Mac
            ),
            b"\x1b[1;5C"
        );
        assert_eq!(
            key_bytes(
                Key::Tab,
                Modifiers::SHIFT,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\x1b[Z"
        );
        assert_eq!(
            key_bytes(
                Key::Delete,
                Modifiers::CTRL,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\x1b[3;5~"
        );
        assert_eq!(
            key_bytes(
                Key::F1,
                Modifiers::SHIFT,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\x1b[1;2P"
        );
        assert_eq!(
            key_bytes(
                Key::F13,
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\x1b[1;2P"
        );
        assert_eq!(
            key_bytes(
                Key::F24,
                Modifiers::CTRL,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\x1b[24;6~"
        );
        assert_eq!(
            key_bytes(
                Key::Enter,
                Modifiers::SHIFT,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\r"
        );
        assert_eq!(
            key_bytes(
                Key::Enter,
                Modifiers::ALT,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\x1b\r"
        );
        assert!(
            key_bytes(
                Key::F35,
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Mac
            )
            .is_empty()
        );
    }

    #[test]
    fn paste_normalizes_newlines_and_cannot_inject_control_or_close_brackets() {
        let text = "one\r\ntwo\rthree\n\x1b[201~\x03\u{009b}done\t✓";
        assert_eq!(
            paste_bytes(text, TermMode::BRACKETED_PASTE),
            "\x1b[200~one\ntwo\nthree\n[201~done\t✓\x1b[201~".as_bytes()
        );
        assert_eq!(paste_bytes("a\r\nb\nc", TermMode::NONE), b"a\rb\rc");
        assert!(paste_bytes("\x1b\x03", TermMode::BRACKETED_PASTE).is_empty());
    }

    #[test]
    fn ime_preedit_is_local_and_commit_is_delivered_once_without_enter_or_mirrored_text() {
        let mut encoder = InputEncoder::default();
        assert!(
            encoder
                .encode(
                    &[
                        Event::Ime(ImeEvent::Preedit {
                            text: "ㅎ".into(),
                            active_range_chars: Some(0..1)
                        }),
                        key(Key::Enter, Modifiers::NONE)
                    ],
                    Modifiers::NONE,
                    TermMode::NONE,
                    OperatingSystem::Mac
                )
                .is_empty()
        );
        assert_eq!(
            encoder.encode(
                &[
                    key(Key::Enter, Modifiers::NONE),
                    Event::Ime(ImeEvent::Commit("한".into())),
                    Event::Text("한".into())
                ],
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            "한".as_bytes()
        );
        assert_eq!(
            encoder.encode(
                &[Event::Text("한".into()), Event::Text("한".into())],
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            "한한".as_bytes()
        );
        assert_eq!(
            encode(
                &[
                    Event::Text("字".into()),
                    Event::Ime(ImeEvent::Commit("字".into()))
                ],
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Nix
            ),
            "字".as_bytes()
        );
        encoder.composing = true;
        encoder.reset();
        assert_eq!(
            encoder.encode(
                &[key(Key::Enter, Modifiers::NONE)],
                Modifiers::NONE,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            b"\r"
        );
    }

    #[test]
    fn option_text_preserves_macos_composition_and_alt_text_is_meta_elsewhere() {
        assert_eq!(
            encode(
                &[key(Key::B, Modifiers::ALT), Event::Text("∫".into())],
                Modifiers::ALT,
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            "∫".as_bytes()
        );
        assert_eq!(
            encode(
                &[key(Key::B, Modifiers::ALT), Event::Text("b".into())],
                Modifiers::ALT,
                TermMode::NONE,
                OperatingSystem::Nix
            ),
            b"\x1bb"
        );
        assert_eq!(
            key_bytes(
                Key::C,
                Modifiers {
                    ctrl: true,
                    alt: true,
                    ..Modifiers::NONE
                },
                TermMode::NONE,
                OperatingSystem::Mac
            ),
            [0x1b, 3]
        );
    }

    #[test]
    fn mouse_reports_follow_requested_protocol_motion_and_coordinate_limits() {
        let mode = TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE;
        assert_eq!(
            mouse_report(
                mode,
                MouseAction::Press(PointerButton::Primary),
                4,
                2,
                Modifiers::CTRL
            ),
            b"\x1b[<16;5;3M"
        );
        assert_eq!(
            mouse_report(
                mode,
                MouseAction::Release(PointerButton::Primary),
                4,
                2,
                Modifiers::NONE
            ),
            b"\x1b[<0;5;3m"
        );
        assert_eq!(
            mouse_report(
                mode,
                MouseAction::Move(Some(PointerButton::Primary)),
                4,
                2,
                Modifiers::NONE
            ),
            b"\x1b[<32;5;3M"
        );
        assert!(mouse_report(mode, MouseAction::Move(None), 4, 2, Modifiers::NONE).is_empty());
        assert_eq!(
            mouse_report(
                TermMode::MOUSE_MOTION | TermMode::SGR_MOUSE,
                MouseAction::Move(None),
                4,
                2,
                Modifiers::NONE
            ),
            b"\x1b[<35;5;3M"
        );
        assert_eq!(
            mouse_report(
                TermMode::MOUSE_REPORT_CLICK,
                MouseAction::Press(PointerButton::Secondary),
                0,
                0,
                Modifiers::NONE
            ),
            b"\x1b[M\x22!!"
        );
        assert_eq!(
            mouse_report(
                TermMode::MOUSE_REPORT_CLICK,
                MouseAction::Release(PointerButton::Secondary),
                0,
                0,
                Modifiers::NONE
            ),
            b"\x1b[M\x23!!"
        );
        assert!(
            mouse_report(
                TermMode::MOUSE_REPORT_CLICK,
                MouseAction::Press(PointerButton::Primary),
                223,
                0,
                Modifiers::NONE
            )
            .is_empty()
        );
        assert!(
            !mouse_report(
                mode,
                MouseAction::Press(PointerButton::Primary),
                300,
                0,
                Modifiers::NONE
            )
            .is_empty()
        );
        assert_eq!(
            mouse_report(
                TermMode::MOUSE_REPORT_CLICK | TermMode::UTF8_MOUSE,
                MouseAction::Press(PointerButton::Primary),
                300,
                0,
                Modifiers::NONE
            ),
            "\x1b[M \u{014d}!".as_bytes()
        );
        assert!(
            mouse_report(
                mode,
                MouseAction::Press(PointerButton::Primary),
                usize::MAX,
                0,
                Modifiers::NONE
            )
            .is_empty()
        );
    }

    #[test]
    fn focus_and_wheel_reports_only_run_for_requested_application_modes() {
        assert!(focus_report(TermMode::NONE, true).is_empty());
        assert_eq!(focus_report(TermMode::FOCUS_IN_OUT, true), b"\x1b[I");
        assert_eq!(focus_report(TermMode::FOCUS_IN_OUT, false), b"\x1b[O");
        assert!(wheel_report(TermMode::NONE, 3, 0, 0, Modifiers::NONE).is_empty());
        assert_eq!(
            wheel_report(
                TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL | TermMode::APP_CURSOR,
                -2,
                0,
                0,
                Modifiers::NONE
            ),
            b"\x1bOB\x1bOB"
        );
        assert_eq!(
            wheel_report(
                TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE,
                2,
                0,
                0,
                Modifiers::NONE
            ),
            b"\x1b[<64;1;1M\x1b[<64;1;1M"
        );
        assert!(
            wheel_report(
                TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL,
                0,
                0,
                0,
                Modifiers::NONE
            )
            .is_empty()
        );
    }
}
