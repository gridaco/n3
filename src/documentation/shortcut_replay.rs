//! Resolve semantic bindings into native-style events. Rebinding an action must
//! update both the guide's keycaps and the input illustrated in its captures.
use super::{Result, Session};
use crate::input::bindings::{self, Binding, BindingInput, Trigger};
use egui::{Event, Modifiers};

impl Session<'_> {
    pub fn shortcut(&mut self, id: &str) -> Result<bool> {
        let opened = self.shortcut_down(id)?;
        Ok(self.shortcut_up(id)? || opened)
    }

    pub fn shortcut_down(&mut self, id: &str) -> Result<bool> {
        self.shortcut_edge(id, true)
    }

    pub fn shortcut_up(&mut self, id: &str) -> Result<bool> {
        self.shortcut_edge(id, false)
    }

    pub fn shortcut_label(&self, id: &str) -> Result<String> {
        Ok(bindings::binding(id)?.label())
    }

    pub fn shortcut_is_down(&self, id: &str) -> Result<bool> {
        Ok(match bindings::binding(id)?.input {
            BindingInput::Key(key) => self.input.key_down(key),
            BindingInput::Number(number) => self.number_keys_down.contains(&number),
            BindingInput::Modifier(modifier) => modifier.active(self.input.modifiers()),
        })
    }

    fn observe_shortcut(&mut self, id: &str) -> Result<Binding> {
        let binding = bindings::binding(id)?;
        self.shortcut_bindings
            .insert(id.to_owned(), binding.label());
        Ok(binding)
    }

    fn shortcut_edge(&mut self, id: &str, pressed: bool) -> Result<bool> {
        let binding = self.observe_shortcut(id)?;
        let modifiers = edge_modifiers(binding, pressed);
        match binding.input {
            BindingInput::Key(key) => {
                let open = self.key(key, pressed, modifiers)?;
                if !pressed && modifiers != Modifiers::NONE {
                    // A modified tap accepts its key-up while the chord is
                    // still held. Releasing modifiers first cancels that tap.
                    self.modifiers_changed(Modifiers::NONE)?;
                }
                Ok(open)
            }
            BindingInput::Number(number) => self.number_key(number, pressed, modifiers),
            BindingInput::Modifier(_) => {
                self.modifiers_changed(modifiers)?;
                Ok(false)
            }
        }
    }

    /// For same-frame key-order scenarios. Number bindings must use the normal
    /// helpers so the physical top-row/numpad metadata cannot be dropped.
    pub fn shortcut_event(&self, id: &str, pressed: bool) -> Result<Event> {
        key_event(bindings::binding(id)?, pressed)
    }
}

fn edge_modifiers(binding: Binding, pressed: bool) -> Modifiers {
    match binding.input {
        BindingInput::Modifier(modifier) if pressed => binding.modifiers | modifier.modifiers(),
        _ if pressed || binding.trigger == Trigger::Tap => binding.modifiers,
        _ => Modifiers::NONE,
    }
}

fn key_event(binding: Binding, pressed: bool) -> Result<Event> {
    let modifiers = edge_modifiers(binding, pressed);
    match binding.input {
        BindingInput::Key(key) => Ok(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        }),
        BindingInput::Modifier(_) => Ok(Event::ModifiersChanged(modifiers)),
        BindingInput::Number(_) => {
            Err("Use shortcut_down/up to retain number-key source metadata".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rebound_action_updates_both_spelling_and_replayed_input() {
        let mut binding = bindings::binding("selection.duplicate").unwrap();
        binding.input = BindingInput::Key(egui::Key::B);
        assert_eq!(binding.key_parts(), ["Command", "B"]);
        assert!(matches!(key_event(binding, true).unwrap(), Event::Key {
            key: egui::Key::B, modifiers, pressed: true, ..
        } if modifiers.command || modifiers.mac_cmd));
    }

    #[test]
    fn modified_taps_release_the_key_before_the_chord_modifiers() {
        let mut tap = bindings::binding("view.planar").unwrap();
        tap.modifiers = Modifiers::SHIFT;
        for pressed in [true, false] {
            assert!(matches!(
                key_event(tap, pressed).unwrap(),
                Event::Key {
                    modifiers: Modifiers::SHIFT,
                    ..
                }
            ));
        }
        let mut hold = bindings::binding("navigation.orbit").unwrap();
        hold.modifiers |= Modifiers::SHIFT;
        assert!(
            matches!(key_event(hold, true).unwrap(), Event::ModifiersChanged(m)
            if m.alt && m.shift)
        );
        assert!(matches!(
            key_event(hold, false).unwrap(),
            Event::ModifiersChanged(Modifiers::NONE)
        ));
    }
}
