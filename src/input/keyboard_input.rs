//! Physical number-key metadata kept alongside egui's unchanged input events.

use std::collections::BTreeSet;

use egui::{Key, Modifiers};
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum NumberKey {
    TopRow(u8),
    Numpad(u8),
}

impl NumberKey {
    pub fn from_physical_key(key: PhysicalKey) -> Option<Self> {
        let PhysicalKey::Code(code) = key else {
            return None;
        };
        Some(match code {
            KeyCode::Digit0 => Self::TopRow(0),
            KeyCode::Digit1 => Self::TopRow(1),
            KeyCode::Digit2 => Self::TopRow(2),
            KeyCode::Digit3 => Self::TopRow(3),
            KeyCode::Digit4 => Self::TopRow(4),
            KeyCode::Digit5 => Self::TopRow(5),
            KeyCode::Digit6 => Self::TopRow(6),
            KeyCode::Digit7 => Self::TopRow(7),
            KeyCode::Digit8 => Self::TopRow(8),
            KeyCode::Digit9 => Self::TopRow(9),
            KeyCode::Numpad0 => Self::Numpad(0),
            KeyCode::Numpad1 => Self::Numpad(1),
            KeyCode::Numpad2 => Self::Numpad(2),
            KeyCode::Numpad3 => Self::Numpad(3),
            KeyCode::Numpad4 => Self::Numpad(4),
            KeyCode::Numpad5 => Self::Numpad(5),
            KeyCode::Numpad6 => Self::Numpad(6),
            KeyCode::Numpad7 => Self::Numpad(7),
            KeyCode::Numpad8 => Self::Numpad(8),
            KeyCode::Numpad9 => Self::Numpad(9),
            _ => return None,
        })
    }

    /// Logical event used by scripted inputs. Physical origin stays in metadata.
    #[allow(dead_code)] // Replay supplies logical keys alongside physical metadata.
    pub fn egui_key(self) -> Option<Key> {
        let (Self::TopRow(digit) | Self::Numpad(digit)) = self;
        [
            Key::Num0,
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ]
        .get(usize::from(digit))
        .copied()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NumberKeyEvent {
    /// Index of this key's egui Event::Key in the frame's RawInput.events.
    pub event_index: usize,
    pub key: NumberKey,
    pub pressed: bool,
    pub repeat: bool,
    pub modifiers: Modifiers,
}

#[derive(Default)]
pub struct NumberKeyInput {
    down: BTreeSet<NumberKey>,
    events: Vec<NumberKeyEvent>,
}

impl NumberKeyInput {
    pub fn record(
        &mut self,
        event_index: usize,
        key: NumberKey,
        pressed: bool,
        native_repeat: bool,
        modifiers: Modifiers,
    ) {
        let repeat = if pressed {
            let already_down = !self.down.insert(key);
            native_repeat || already_down
        } else {
            self.down.remove(&key);
            false
        };
        self.events.push(NumberKeyEvent {
            event_index,
            key,
            pressed,
            repeat,
            modifiers,
        });
    }

    pub fn take(&mut self) -> Vec<NumberKeyEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn reset(&mut self) {
        self.down.clear();
        self.events.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_digit_origin_survives_identical_logical_keys() {
        for (digit, top, pad) in [
            (0, KeyCode::Digit0, KeyCode::Numpad0),
            (1, KeyCode::Digit1, KeyCode::Numpad1),
            (2, KeyCode::Digit2, KeyCode::Numpad2),
            (3, KeyCode::Digit3, KeyCode::Numpad3),
            (4, KeyCode::Digit4, KeyCode::Numpad4),
            (5, KeyCode::Digit5, KeyCode::Numpad5),
            (6, KeyCode::Digit6, KeyCode::Numpad6),
            (7, KeyCode::Digit7, KeyCode::Numpad7),
            (8, KeyCode::Digit8, KeyCode::Numpad8),
            (9, KeyCode::Digit9, KeyCode::Numpad9),
        ] {
            assert_eq!(
                NumberKey::from_physical_key(PhysicalKey::Code(top)),
                Some(NumberKey::TopRow(digit))
            );
            assert_eq!(
                NumberKey::from_physical_key(PhysicalKey::Code(pad)),
                Some(NumberKey::Numpad(digit))
            );
            assert_eq!(
                NumberKey::TopRow(digit).egui_key(),
                NumberKey::Numpad(digit).egui_key()
            );
        }
        assert_eq!(
            NumberKey::from_physical_key(PhysicalKey::Code(KeyCode::KeyA)),
            None
        );
        assert_eq!(NumberKey::TopRow(10).egui_key(), None);
    }

    #[test]
    fn repeat_and_focus_reset_are_tracked_per_physical_source() {
        let mut input = NumberKeyInput::default();
        input.record(0, NumberKey::TopRow(3), true, false, Modifiers::NONE);
        input.record(1, NumberKey::Numpad(3), true, false, Modifiers::NONE);
        input.record(2, NumberKey::TopRow(3), true, false, Modifiers::NONE);
        assert_eq!(
            input
                .take()
                .iter()
                .map(|event| event.repeat)
                .collect::<Vec<_>>(),
            [false, false, true]
        );
        input.record(0, NumberKey::TopRow(3), false, false, Modifiers::NONE);
        input.record(1, NumberKey::Numpad(3), true, false, Modifiers::NONE);
        assert!(
            input.take()[1].repeat,
            "releasing the top row cannot release the numpad"
        );
        input.record(0, NumberKey::Numpad(3), true, false, Modifiers::NONE);
        input.reset();
        assert!(input.take().is_empty());
        input.record(0, NumberKey::Numpad(3), true, false, Modifiers::NONE);
        assert!(!input.take()[0].repeat);
    }
}
