//! Canonical keyboard bindings, shared by input routing and executable guides.
//! Context ownership and tap/hold lifecycles live in their input routers; this
//! table owns the physical inputs and their semantic command, not UI behavior.
use crate::{camera::View, editor::Tool, keyboard_input::NumberKey, shortcuts::Command};
use egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldModifier {
    Alt,
}

impl HeldModifier {
    pub fn modifiers(self) -> Modifiers {
        match self {
            Self::Alt => Modifiers::ALT,
        }
    }

    pub fn active(self, modifiers: Modifiers) -> bool {
        match self {
            Self::Alt => modifiers.alt,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingInput {
    Key(Key),
    Number(NumberKey),
    Modifier(HeldModifier),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Trigger {
    Press,
    Tap,
    Hold,
}

/// Proven keyboard owners, not a gesture or intent framework. Editor bindings
/// retain their existing ownership rules; timeline bindings are resolved only
/// by the timeline host's router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    Editor,
    Viewport,
    Timeline,
}

#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub id: &'static str,
    pub input: BindingInput,
    pub modifiers: Modifiers,
    pub(crate) command: Option<Command>,
    pub(crate) trigger: Trigger,
    pub(crate) over_text: bool,
    pub(crate) scope: Scope,
    // These existing allowances are intentional. Space wins over held Option;
    // Option orbit may coexist with Shift. New/Open/Quit also accept Shift.
    allow_shift: bool,
    allow_alt: bool,
}

impl Binding {
    const fn press(
        id: &'static str,
        input: BindingInput,
        modifiers: Modifiers,
        command: Command,
    ) -> Self {
        Self {
            id,
            input,
            modifiers,
            command: Some(command),
            trigger: Trigger::Press,
            over_text: false,
            scope: Scope::Editor,
            allow_shift: false,
            allow_alt: false,
        }
    }

    const fn application(mut self, allow_shift: bool) -> Self {
        self.over_text = true;
        self.allow_shift = allow_shift;
        self
    }

    const fn viewport(mut self) -> Self {
        self.scope = Scope::Viewport;
        self
    }

    const fn timeline(mut self) -> Self {
        self.scope = Scope::Timeline;
        self
    }

    const fn tap(mut self) -> Self {
        self.trigger = Trigger::Tap;
        self
    }

    const fn hold(
        id: &'static str,
        input: BindingInput,
        modifiers: Modifiers,
        allow_shift: bool,
        allow_alt: bool,
    ) -> Self {
        Self {
            id,
            input,
            modifiers,
            command: None,
            trigger: Trigger::Hold,
            over_text: false,
            scope: Scope::Editor,
            allow_shift,
            allow_alt,
        }
    }

    pub fn key(self) -> Option<Key> {
        match self.input {
            BindingInput::Key(key) => Some(key),
            _ => None,
        }
    }

    pub fn matches_key(self, key: Key, modifiers: Modifiers) -> bool {
        self.input == BindingInput::Key(key) && self.matches_modifiers(modifiers)
    }

    pub fn matches_number(self, key: NumberKey, modifiers: Modifiers) -> bool {
        self.input == BindingInput::Number(key) && self.matches_modifiers(modifiers)
    }

    pub fn matches_modifiers(self, modifiers: Modifiers) -> bool {
        let command = modifiers.command || modifiers.mac_cmd;
        let required_command = self.modifiers.command || self.modifiers.mac_cmd;
        command == required_command
            // egui exposes the platform command in addition to its physical
            // modifier. Preserve native command matching on both platforms.
            && (required_command || modifiers.ctrl == self.modifiers.ctrl)
            && (self.allow_shift || modifiers.shift == self.modifiers.shift)
            && (self.allow_alt || modifiers.alt == self.modifiers.alt)
    }

    pub fn key_parts(self) -> Vec<String> {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Control".into());
        }
        if self.modifiers.alt && !matches!(self.input, BindingInput::Modifier(HeldModifier::Alt)) {
            parts.push("Option".into());
        }
        if self.modifiers.shift {
            parts.push("Shift".into());
        }
        if self.modifiers.command || self.modifiers.mac_cmd {
            parts.push("Command".into());
        }
        parts.push(match self.input {
            BindingInput::Key(key) => match key {
                Key::Period => ".".into(),
                Key::Comma => ",".into(),
                Key::Slash => "/".into(),
                Key::Backslash => "\\".into(),
                Key::Backtick => "`".into(),
                _ => key.name().into(),
            },
            BindingInput::Number(NumberKey::TopRow(digit)) => digit.to_string(),
            BindingInput::Number(NumberKey::Numpad(digit)) => format!("Numpad {digit}"),
            BindingInput::Modifier(HeldModifier::Alt) => "Option".into(),
        });
        parts
    }

    pub fn label(self) -> String {
        self.key_parts().join("+")
    }
}

const SHIFT_COMMAND: Modifiers = Modifiers {
    shift: true,
    ..Modifiers::MAC_CMD
};

macro_rules! key {
    ($id:literal, $key:ident, $mods:expr, $command:expr) => {
        Binding::press($id, BindingInput::Key(Key::$key), $mods, $command)
    };
}
macro_rules! number {
    ($id:literal, $origin:ident, $number:literal, $mods:expr, $command:expr) => {
        Binding::press(
            $id,
            BindingInput::Number(NumberKey::$origin($number)),
            $mods,
            $command,
        )
    };
}

pub(crate) const BINDINGS: &[Binding] = &[
    key!("document.new", N, Modifiers::MAC_CMD, Command::New).application(true),
    key!("document.open", O, Modifiers::MAC_CMD, Command::Open).application(true),
    key!(
        "document.save",
        S,
        Modifiers::MAC_CMD,
        Command::Save { save_as: false }
    )
    .application(false),
    key!(
        "document.save-as",
        S,
        SHIFT_COMMAND,
        Command::Save { save_as: true }
    )
    .application(false),
    key!("document.quit", Q, Modifiers::MAC_CMD, Command::Quit).application(true),
    key!(
        "preferences.open",
        Comma,
        Modifiers::MAC_CMD,
        Command::OpenPreferences
    )
    .application(false),
    key!("insert.open", I, Modifiers::SHIFT, Command::InsertMenu),
    key!("history.undo", Z, Modifiers::MAC_CMD, Command::Undo),
    key!("history.redo", Z, SHIFT_COMMAND, Command::Redo),
    key!(
        "animation.play-pause",
        Space,
        Modifiers::NONE,
        Command::ToggleAnimationPlayback
    )
    .timeline(),
    key!(
        "ui.toggle",
        Backslash,
        Modifiers::MAC_CMD,
        Command::ToggleUi
    )
    .application(false),
    key!(
        "notifications.focus",
        F6,
        Modifiers::NONE,
        Command::FocusToasts
    ),
    key!("selection.all", A, Modifiers::MAC_CMD, Command::SelectAll),
    key!("mesh.make-face", F, Modifiers::NONE, Command::MakeFace),
    key!(
        "selection.duplicate",
        D,
        Modifiers::MAC_CMD,
        Command::DuplicateSelection
    ),
    key!(
        "selection.delete",
        Delete,
        Modifiers::NONE,
        Command::DeleteSelection
    ),
    key!(
        "selection.backspace",
        Backspace,
        Modifiers::NONE,
        Command::DeleteSelection
    ),
    key!(
        "selection.next",
        Tab,
        Modifiers::NONE,
        Command::CycleSelection { reverse: false }
    )
    .viewport(),
    key!(
        "selection.previous",
        Tab,
        Modifiers::SHIFT,
        Command::CycleSelection { reverse: true }
    )
    .viewport(),
    key!("tool.cursor", V, Modifiers::NONE, Command::Tool(Tool::View)),
    key!(
        "tool.cursor-alternate",
        Q,
        Modifiers::NONE,
        Command::Tool(Tool::View)
    ),
    key!("tool.move", W, Modifiers::NONE, Command::Tool(Tool::Move)),
    key!(
        "tool.move-alternate",
        G,
        Modifiers::NONE,
        Command::Tool(Tool::Move)
    ),
    key!(
        "tool.rotate",
        R,
        Modifiers::NONE,
        Command::Tool(Tool::Rotate)
    ),
    key!("tool.scale", E, Modifiers::NONE, Command::Tool(Tool::Scale)),
    key!(
        "tool.scale-alternate",
        S,
        Modifiers::NONE,
        Command::Tool(Tool::Scale)
    ),
    key!(
        "transform.axis-x",
        X,
        Modifiers::NONE,
        Command::ToggleTransformAxis(0)
    ),
    key!(
        "transform.axis-y",
        Y,
        Modifiers::NONE,
        Command::ToggleTransformAxis(1)
    ),
    key!(
        "transform.axis-z",
        Z,
        Modifiers::NONE,
        Command::ToggleTransformAxis(2)
    ),
    key!("edit.confirm", Enter, Modifiers::NONE, Command::Confirm),
    key!("edit.leave", Enter, Modifiers::SHIFT, Command::LeaveEdit),
    key!("cancel", Escape, Modifiers::NONE, Command::Escape),
    number!(
        "view.projection",
        TopRow,
        0,
        Modifiers::NONE,
        Command::ToggleProjection
    ),
    key!(
        "view.planar",
        Period,
        Modifiers::NONE,
        Command::TogglePlanarNavigation
    )
    .tap(),
    key!("view.2d-ruler", R, Modifiers::SHIFT, Command::ToggleRuler2D),
    key!("view.xray", Z, Modifiers::ALT, Command::ToggleXray),
    key!(
        "view.local",
        Slash,
        Modifiers::NONE,
        Command::ToggleLocalView
    ),
    number!(
        "view.perspective",
        TopRow,
        1,
        Modifiers::NONE,
        Command::View(View::Perspective)
    ),
    number!(
        "view.front",
        TopRow,
        2,
        Modifiers::NONE,
        Command::View(View::Front)
    ),
    number!(
        "view.right",
        TopRow,
        3,
        Modifiers::NONE,
        Command::View(View::Right)
    ),
    number!(
        "view.back",
        TopRow,
        4,
        Modifiers::NONE,
        Command::View(View::Back)
    ),
    number!(
        "view.left",
        TopRow,
        5,
        Modifiers::NONE,
        Command::View(View::Left)
    ),
    number!(
        "view.top",
        TopRow,
        6,
        Modifiers::NONE,
        Command::View(View::Top)
    ),
    number!(
        "view.bottom",
        TopRow,
        7,
        Modifiers::NONE,
        Command::View(View::Bottom)
    ),
    number!(
        "numpad.perspective",
        Numpad,
        0,
        Modifiers::NONE,
        Command::View(View::Perspective)
    ),
    number!(
        "numpad.front",
        Numpad,
        1,
        Modifiers::NONE,
        Command::View(View::Front)
    ),
    number!(
        "numpad.right",
        Numpad,
        3,
        Modifiers::NONE,
        Command::View(View::Right)
    ),
    number!(
        "numpad.top",
        Numpad,
        7,
        Modifiers::NONE,
        Command::View(View::Top)
    ),
    number!(
        "numpad.orbit-left",
        Numpad,
        4,
        Modifiers::NONE,
        Command::OrbitView {
            horizontal: -1.0,
            vertical: 0.0
        }
    ),
    number!(
        "numpad.orbit-right",
        Numpad,
        6,
        Modifiers::NONE,
        Command::OrbitView {
            horizontal: 1.0,
            vertical: 0.0
        }
    ),
    number!(
        "numpad.orbit-down",
        Numpad,
        2,
        Modifiers::NONE,
        Command::OrbitView {
            horizontal: 0.0,
            vertical: -1.0
        }
    ),
    number!(
        "numpad.orbit-up",
        Numpad,
        8,
        Modifiers::NONE,
        Command::OrbitView {
            horizontal: 0.0,
            vertical: 1.0
        }
    ),
    number!(
        "numpad.projection",
        Numpad,
        5,
        Modifiers::NONE,
        Command::ToggleProjection
    ),
    number!("view.frame", TopRow, 1, Modifiers::SHIFT, Command::Frame),
    number!(
        "view.frame-selection",
        TopRow,
        2,
        Modifiers::SHIFT,
        Command::FrameSelection
    ),
    Binding::hold(
        "navigation.pan",
        BindingInput::Key(Key::Space),
        Modifiers::NONE,
        false,
        true,
    ),
    Binding::hold(
        "navigation.orbit",
        BindingInput::Modifier(HeldModifier::Alt),
        Modifiers::ALT,
        true,
        false,
    ),
    Binding::hold(
        "shading.pie",
        BindingInput::Key(Key::Z),
        Modifiers::NONE,
        false,
        false,
    ),
    Binding::hold(
        "view.pie",
        BindingInput::Key(Key::Backtick),
        Modifiers::NONE,
        false,
        false,
    ),
    key!(
        "nudge.left",
        ArrowLeft,
        Modifiers::NONE,
        Command::Nudge {
            horizontal: -1,
            vertical: 0,
            fast: false,
            repeat: false
        }
    )
    .viewport(),
    key!(
        "nudge.right",
        ArrowRight,
        Modifiers::NONE,
        Command::Nudge {
            horizontal: 1,
            vertical: 0,
            fast: false,
            repeat: false
        }
    )
    .viewport(),
    key!(
        "nudge.up",
        ArrowUp,
        Modifiers::NONE,
        Command::Nudge {
            horizontal: 0,
            vertical: 1,
            fast: false,
            repeat: false
        }
    )
    .viewport(),
    key!(
        "nudge.down",
        ArrowDown,
        Modifiers::NONE,
        Command::Nudge {
            horizontal: 0,
            vertical: -1,
            fast: false,
            repeat: false
        }
    )
    .viewport(),
    key!(
        "nudge.fast-left",
        ArrowLeft,
        Modifiers::SHIFT,
        Command::Nudge {
            horizontal: -1,
            vertical: 0,
            fast: true,
            repeat: false
        }
    )
    .viewport(),
    key!(
        "nudge.fast-right",
        ArrowRight,
        Modifiers::SHIFT,
        Command::Nudge {
            horizontal: 1,
            vertical: 0,
            fast: true,
            repeat: false
        }
    )
    .viewport(),
    key!(
        "nudge.fast-up",
        ArrowUp,
        Modifiers::SHIFT,
        Command::Nudge {
            horizontal: 0,
            vertical: 1,
            fast: true,
            repeat: false
        }
    )
    .viewport(),
    key!(
        "nudge.fast-down",
        ArrowDown,
        Modifiers::SHIFT,
        Command::Nudge {
            horizontal: 0,
            vertical: -1,
            fast: true,
            repeat: false
        }
    )
    .viewport(),
];

pub fn binding(id: &str) -> Result<Binding, String> {
    BINDINGS
        .iter()
        .find(|binding| binding.id == id)
        .copied()
        .ok_or_else(|| format!("unknown input binding: {id}"))
}

/// Internal callers use fixed IDs, so a missing definition is a programming
/// error. Author-provided guide IDs instead use the fallible `binding` lookup.
pub(crate) fn required(id: &str) -> Binding {
    binding(id).expect("production input binding must exist")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_inputs_format_without_losing_physical_origin() {
        let mut ids = std::collections::BTreeSet::new();
        for binding in BINDINGS {
            assert!(ids.insert(binding.id), "duplicate {}", binding.id);
            assert!(!binding.label().is_empty());
        }
        assert_eq!(required("view.front").label(), "2");
        assert_eq!(required("numpad.front").label(), "Numpad 1");
        assert_eq!(required("navigation.orbit").key_parts(), ["Option"]);
        assert_eq!(required("view.planar").label(), ".");
        assert_eq!(required("preferences.open").key_parts(), ["Command", ","]);
        assert!(binding("missing.binding").is_err());
    }

    #[test]
    fn matching_preserves_command_aliases_and_held_modifier_policy() {
        let save = required("document.save");
        assert!(save.matches_key(Key::S, Modifiers::MAC_CMD));
        assert!(save.matches_key(Key::S, Modifiers::COMMAND));
        assert!(!save.matches_key(Key::S, SHIFT_COMMAND));
        assert!(!save.matches_key(Key::S, Modifiers::ALT | Modifiers::MAC_CMD));
        assert!(required("document.new").matches_key(Key::N, SHIFT_COMMAND));
        let pan = required("navigation.pan");
        assert!(pan.matches_key(Key::Space, Modifiers::ALT));
        assert!(!pan.matches_key(Key::Space, Modifiers::SHIFT));
        let orbit = required("navigation.orbit");
        assert!(orbit.matches_modifiers(Modifiers::ALT | Modifiers::SHIFT));
        assert!(!orbit.matches_modifiers(Modifiers::ALT | Modifiers::MAC_CMD));
        assert!(!required("view.front").matches_number(NumberKey::Numpad(2), Modifiers::NONE));
    }
}
