//! Held-key navigation bindings and ordered modifier input. No picking or camera
//! work lives here: the viewport resolves a semantic tool at an eligible press.
use super::{
    bindings::{self, BindingInput},
    navigation_input::NavigationTool,
};
use egui::{Event, Modifiers};

#[derive(Default)]
pub struct HeldNavigation {
    space: bool,
    alt_down: bool,
    alt_allowed: bool,
}

impl HeldNavigation {
    pub fn reset(&mut self) {
        // Retain physical Alt observation: cancelling or changing focus must
        // not reinterpret the same still-held modifier as a new key press.
        self.block();
    }

    pub fn modifiers(&mut self, modifiers: Modifiers, keys_available: bool) {
        let binding = bindings::required("navigation.orbit");
        let BindingInput::Modifier(trigger) = binding.input else {
            unreachable!("orbit hold requires a modifier binding");
        };
        let down = trigger.active(modifiers);
        if down && !self.alt_down {
            self.alt_allowed = keys_available && binding.matches_modifiers(modifiers);
        }
        self.alt_down = down;
        if !down || !keys_available || !binding.matches_modifiers(modifiers) {
            self.alt_allowed = false;
        }
    }

    pub fn space(&mut self, pressed: bool, repeat: bool, modifiers: Modifiers, available: bool) {
        if !pressed {
            self.space = false;
        } else if !repeat
            && available
            && bindings::required("navigation.pan").matches_modifiers(modifiers)
        {
            self.space = true;
        }
    }

    /// A field or popup cannot transfer an already-held key into navigation.
    pub fn block(&mut self) {
        self.space = false;
        self.alt_allowed = false;
    }

    pub fn held(&self, tool: NavigationTool) -> bool {
        match tool {
            NavigationTool::Pan => self.space,
            NavigationTool::Orbit => self.alt_allowed,
        }
    }

    pub fn preferred(&self) -> Option<NavigationTool> {
        if self.space {
            Some(NavigationTool::Pan)
        } else if self.alt_allowed {
            Some(NavigationTool::Orbit)
        } else {
            None
        }
    }
}

pub enum OrderedInput {
    Modifiers(Modifiers),
    Event(Event),
}

/// Preserve modifier transitions in egui's native event order. In particular,
/// releasing Option between two PointerMoved events stops orbit at that gap;
/// the final frame's modifier state must not rewrite earlier movement.
pub fn ordered_input(events: Vec<Event>, final_modifiers: Modifiers) -> Vec<OrderedInput> {
    let mut output = Vec::with_capacity(events.len() + 1);
    for event in events {
        match &event {
            Event::ModifiersChanged(modifiers) => {
                output.push(OrderedInput::Modifiers(*modifiers));
                continue;
            }
            Event::Key { modifiers, .. }
            | Event::PointerButton { modifiers, .. }
            | Event::MouseWheel { modifiers, .. } => {
                output.push(OrderedInput::Modifiers(*modifiers));
            }
            _ => {}
        }
        output.push(OrderedInput::Event(event));
    }
    output.push(OrderedInput::Modifiers(final_modifiers));
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_priority_and_consumed_alt_require_a_new_press() {
        let mut input = HeldNavigation::default();
        input.modifiers(Modifiers::ALT, false);
        input.modifiers(Modifiers::ALT, true);
        assert_eq!(input.preferred(), None);
        input.modifiers(Modifiers::NONE, true);
        input.modifiers(Modifiers::ALT, true);
        assert_eq!(input.preferred(), Some(NavigationTool::Orbit));
        input.space(true, false, Modifiers::ALT, true);
        assert_eq!(input.preferred(), Some(NavigationTool::Pan));
        assert!(input.held(NavigationTool::Orbit));
        input.space(false, false, Modifiers::ALT, true);
        assert_eq!(input.preferred(), Some(NavigationTool::Orbit));
        input.block();
        input.modifiers(Modifiers::ALT, true);
        assert_eq!(input.preferred(), None);
    }

    #[test]
    fn modifier_release_stays_between_pointer_movements_in_one_batch() {
        let events = ordered_input(
            vec![
                Event::ModifiersChanged(Modifiers::ALT),
                Event::PointerMoved(egui::pos2(1., 2.)),
                Event::ModifiersChanged(Modifiers::NONE),
                Event::PointerMoved(egui::pos2(3., 4.)),
            ],
            Modifiers::NONE,
        );
        assert!(
            matches!(events.as_slice(), [OrderedInput::Modifiers(a), OrderedInput::Event(_),
            OrderedInput::Modifiers(b), OrderedInput::Event(_), OrderedInput::Modifiers(_)]
            if a.alt && !b.alt)
        );
    }
}
