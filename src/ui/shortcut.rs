//! Shortcut presentation only. The input catalog remains the physical keymap.
use crate::input::bindings::{Binding, BindingInput, HeldModifier};
use crate::keyboard_input::NumberKey;
use egui::{Key, os::OperatingSystem};

/// Factual Unicode reference, independent of the platform display policy below.
/// A character's scalar value is its code point; coverage refers to bundled Inter.
#[cfg(test)]
struct SymbolReference {
    pub name: &'static str,
    pub symbols: &'static [(char, bool)],
}

#[cfg(test)]
const SYMBOL_REFERENCE: &[SymbolReference] = &[
    SymbolReference {
        name: "Command",
        symbols: &[('⌘', true)],
    },
    SymbolReference {
        name: "Shift",
        symbols: &[('⇧', true)],
    },
    SymbolReference {
        name: "Option / Alt",
        symbols: &[('⌥', true)],
    },
    SymbolReference {
        name: "Control",
        symbols: &[('⌃', true)],
    },
    SymbolReference {
        name: "Caps Lock",
        symbols: &[('⇪', true)],
    },
    SymbolReference {
        name: "Return / Enter",
        symbols: &[('↩', true), ('⏎', true)],
    },
    SymbolReference {
        name: "Keypad Enter",
        symbols: &[('⌤', false)],
    },
    SymbolReference {
        name: "Tab",
        symbols: &[('⇥', true)],
    },
    SymbolReference {
        name: "Backtab",
        symbols: &[('⇤', true)],
    },
    SymbolReference {
        name: "Backspace",
        symbols: &[('⌫', true)],
    },
    SymbolReference {
        name: "Forward Delete",
        symbols: &[('⌦', true)],
    },
    SymbolReference {
        name: "Escape",
        symbols: &[('⎋', true)],
    },
    SymbolReference {
        name: "Space",
        symbols: &[('␣', true)],
    },
    SymbolReference {
        name: "Left Arrow",
        symbols: &[('←', true)],
    },
    SymbolReference {
        name: "Up Arrow",
        symbols: &[('↑', true)],
    },
    SymbolReference {
        name: "Right Arrow",
        symbols: &[('→', true)],
    },
    SymbolReference {
        name: "Down Arrow",
        symbols: &[('↓', true)],
    },
    SymbolReference {
        name: "Home",
        symbols: &[('↖', true)],
    },
    SymbolReference {
        name: "End",
        symbols: &[('↘', true)],
    },
    SymbolReference {
        name: "Page Up",
        symbols: &[('⇞', true)],
    },
    SymbolReference {
        name: "Page Down",
        symbols: &[('⇟', true)],
    },
    // ⊞ is U+229E SQUARED PLUS, sometimes used as a Windows/Super substitute.
    // It is not the Windows logo and is not available in the bundled Inter.
    SymbolReference {
        name: "Windows / Super substitute",
        symbols: &[('⊞', false)],
    },
    SymbolReference {
        name: "Fn / function keys",
        symbols: &[],
    },
];

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Presentation {
    pub visual: String,
    pub descriptive: String,
}

/// Explicit OS input makes every display policy testable on any host. The
/// catalog's command/mac_cmd flags both express semantic command matching.
pub(crate) fn format(binding: Binding, os: OperatingSystem) -> Presentation {
    let mac = os.is_mac();
    let command = binding.modifiers.command || binding.modifiers.mac_cmd;
    let control = binding.modifiers.ctrl || (!mac && command);
    let alt_key = matches!(binding.input, BindingInput::Modifier(HeldModifier::Alt));
    let mut visual = Vec::new();
    let mut descriptive = Vec::new();
    let mut push = |enabled, symbol: &str, name: &str, short_name: &str| {
        if enabled {
            visual.push(if mac { symbol } else { short_name }.to_owned());
            descriptive.push(name.to_owned());
        }
    };
    push(control, "⌃", "Control", "Ctrl");
    push(
        binding.modifiers.alt && !alt_key,
        "⌥",
        if mac { "Option" } else { "Alt" },
        "Alt",
    );
    push(binding.modifiers.shift, "⇧", "Shift", "Shift");
    push(mac && command, "⌘", "Command", "Command");
    let (key_visual, key_descriptive) = match binding.input {
        BindingInput::Key(key) => (key_label(key, os).to_owned(), key_name(key, os).to_owned()),
        BindingInput::Number(NumberKey::TopRow(digit)) => (digit.to_string(), digit.to_string()),
        BindingInput::Number(NumberKey::Numpad(digit)) => {
            (format!("Numpad {digit}"), format!("Numpad {digit}"))
        }
        BindingInput::Modifier(HeldModifier::Alt) => (
            if mac { "⌥" } else { "Alt" }.into(),
            if mac { "Option" } else { "Alt" }.into(),
        ),
    };
    visual.push(key_visual);
    descriptive.push(key_descriptive);
    Presentation {
        visual: visual.join(if mac { "" } else { "+" }),
        descriptive: descriptive.join(" + "),
    }
}

/// Visual choices are deliberate conventions, not inferred from glyph coverage.
pub(crate) fn key_label(key: Key, os: OperatingSystem) -> &'static str {
    match key {
        Key::ArrowLeft => "←",
        Key::ArrowUp => "↑",
        Key::ArrowRight => "→",
        Key::ArrowDown => "↓",
        Key::Escape => "Esc",
        Key::Enter if os.is_mac() => "⏎",
        Key::Tab if os.is_mac() => "⇥",
        Key::Backspace if os.is_mac() => "⌫",
        Key::Delete if os.is_mac() => "⌦",
        Key::PageUp => "Page Up",
        Key::PageDown => "Page Down",
        Key::SuperLeft | Key::SuperRight if os.is_mac() => "⌘",
        Key::SuperLeft | Key::SuperRight => super_name(os),
        Key::ShiftLeft | Key::ShiftRight => {
            if os.is_mac() {
                "⇧"
            } else {
                "Shift"
            }
        }
        Key::ControlLeft | Key::ControlRight => {
            if os.is_mac() {
                "⌃"
            } else {
                "Ctrl"
            }
        }
        Key::AltLeft | Key::AltRight => {
            if os.is_mac() {
                "⌥"
            } else {
                "Alt"
            }
        }
        // Space, Home, End and function keys deliberately retain legible names.
        _ => key.symbol_or_name(),
    }
}

fn super_name(os: OperatingSystem) -> &'static str {
    if os.is_mac() {
        "Command"
    } else if os == OperatingSystem::Windows {
        "Windows"
    } else {
        "Super"
    }
}

/// Descriptive English is suitable for accessibility, help and documentation.
pub(crate) fn key_name(key: Key, os: OperatingSystem) -> &'static str {
    match key {
        Key::ArrowLeft => "Left Arrow",
        Key::ArrowUp => "Up Arrow",
        Key::ArrowRight => "Right Arrow",
        Key::ArrowDown => "Down Arrow",
        Key::Enter if os.is_mac() => "Return",
        Key::Delete if os.is_mac() => "Forward Delete",
        Key::PageUp => "Page Up",
        Key::PageDown => "Page Down",
        Key::SuperLeft => match os {
            OperatingSystem::Mac | OperatingSystem::IOS => "Left Command",
            OperatingSystem::Windows => "Left Windows",
            _ => "Left Super",
        },
        Key::SuperRight => match os {
            OperatingSystem::Mac | OperatingSystem::IOS => "Right Command",
            OperatingSystem::Windows => "Right Windows",
            _ => "Right Super",
        },
        Key::AltLeft if os.is_mac() => "Left Option",
        Key::AltRight if os.is_mac() => "Right Option",
        Key::ShiftLeft => "Left Shift",
        Key::ShiftRight => "Right Shift",
        Key::ControlLeft => "Left Control",
        Key::ControlRight => "Right Control",
        Key::AltLeft => "Left Alt",
        Key::AltRight => "Right Alt",
        Key::Comma => "Comma",
        Key::Period => "Period",
        Key::Slash => "Slash",
        Key::Backslash => "Backslash",
        Key::Backtick => "Backtick",
        Key::Colon => "Colon",
        Key::Semicolon => "Semicolon",
        Key::Quote => "Quote",
        Key::Minus => "Minus",
        Key::Plus => "Plus",
        Key::Equals => "Equals",
        Key::Pipe => "Pipe",
        Key::Questionmark => "Question Mark",
        Key::Exclamationmark => "Exclamation Mark",
        Key::OpenBracket => "Open Bracket",
        Key::CloseBracket => "Close Bracket",
        Key::OpenCurlyBracket => "Open Curly Bracket",
        Key::CloseCurlyBracket => "Close Curly Bracket",
        _ => key.name(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::bindings::{BINDINGS, required};
    use egui::Modifiers;

    #[test]
    fn platform_policies_resolve_semantic_command_and_modifier_order() {
        for (id, mac, other) in [
            ("selection.duplicate", "⌘D", "Ctrl+D"),
            ("document.save-as", "⇧⌘S", "Ctrl+Shift+S"),
            ("view.xray", "⌥Z", "Alt+Z"),
            ("navigation.orbit", "⌥", "Alt"),
            ("selection.previous", "⇧⇥", "Shift+Tab"),
        ] {
            assert_eq!(format(required(id), OperatingSystem::Mac).visual, mac);
            for os in [
                OperatingSystem::Windows,
                OperatingSystem::Nix,
                OperatingSystem::Unknown,
                OperatingSystem::Android,
            ] {
                assert_eq!(format(required(id), os).visual, other);
            }
            assert_eq!(format(required(id), OperatingSystem::IOS).visual, mac);
        }
        let mut binding = required("document.save");
        binding.modifiers = Modifiers {
            ctrl: true,
            alt: true,
            shift: true,
            ..Modifiers::MAC_CMD
        };
        assert_eq!(format(binding, OperatingSystem::Mac).visual, "⌃⌥⇧⌘S");
        assert_eq!(
            format(binding, OperatingSystem::Windows).visual,
            "Ctrl+Alt+Shift+S"
        );
        for modifiers in [
            Modifiers::COMMAND,
            Modifiers::MAC_CMD,
            Modifiers::CTRL | Modifiers::COMMAND,
        ] {
            binding.modifiers = modifiers;
            assert_eq!(format(binding, OperatingSystem::Nix).visual, "Ctrl+S");
        }
    }

    #[test]
    fn key_policy_and_fallbacks_preserve_readability_and_physical_origin() {
        for (key, mac, pc) in [
            (Key::Enter, "⏎", "Enter"),
            (Key::Tab, "⇥", "Tab"),
            (Key::Backspace, "⌫", "Backspace"),
            (Key::Delete, "⌦", "Delete"),
            (Key::Escape, "Esc", "Esc"),
            (Key::Space, "Space", "Space"),
            (Key::Home, "Home", "Home"),
            (Key::End, "End", "End"),
            (Key::PageUp, "Page Up", "Page Up"),
            (Key::PageDown, "Page Down", "Page Down"),
            (Key::F6, "F6", "F6"),
            (Key::BrowserBack, "BrowserBack", "BrowserBack"),
            (Key::ArrowLeft, "←", "←"),
            (Key::ArrowUp, "↑", "↑"),
            (Key::ArrowRight, "→", "→"),
            (Key::ArrowDown, "↓", "↓"),
        ] {
            assert_eq!(key_label(key, OperatingSystem::Mac), mac);
            for os in [OperatingSystem::Windows, OperatingSystem::Nix] {
                assert_eq!(key_label(key, os), pc);
            }
        }
        assert_eq!(
            key_label(Key::SuperLeft, OperatingSystem::Windows),
            "Windows"
        );
        assert_eq!(key_label(Key::SuperRight, OperatingSystem::Nix), "Super");
        assert_eq!(
            format(required("numpad.front"), OperatingSystem::Mac).visual,
            "Numpad 1"
        );
        assert_eq!(
            format(required("view.front"), OperatingSystem::Mac).visual,
            "2"
        );
    }

    #[test]
    fn descriptive_names_and_formatting_do_not_change_bindings() {
        assert_eq!(
            format(required("document.save-as"), OperatingSystem::Mac).descriptive,
            "Shift + Command + S"
        );
        assert_eq!(
            format(required("document.save-as"), OperatingSystem::Nix).descriptive,
            "Control + Shift + S"
        );
        assert_eq!(
            format(required("edit.confirm"), OperatingSystem::Mac).descriptive,
            "Return"
        );
        assert_eq!(
            format(required("cancel"), OperatingSystem::Mac).descriptive,
            "Escape"
        );
        assert_eq!(
            format(required("preferences.open"), OperatingSystem::Mac).descriptive,
            "Command + Comma"
        );
        assert_eq!(
            format(required("nudge.left"), OperatingSystem::Windows).descriptive,
            "Left Arrow"
        );
        for binding in BINDINGS {
            let original = (
                binding.label(),
                binding.key_parts(),
                binding.command,
                binding.input,
                binding.modifiers,
            );
            for os in [
                OperatingSystem::Mac,
                OperatingSystem::Windows,
                OperatingSystem::Nix,
            ] {
                let presented = format(*binding, os);
                assert!(!presented.visual.is_empty() && !presented.descriptive.is_empty());
                assert_eq!(
                    original,
                    (
                        binding.label(),
                        binding.key_parts(),
                        binding.command,
                        binding.input,
                        binding.modifiers
                    )
                );
                if let BindingInput::Key(key) = binding.input {
                    assert!(binding.matches_key(key, binding.modifiers));
                }
            }
        }
    }

    #[test]
    fn factual_reference_matches_inter_without_fallback_fonts() {
        let ctx = egui::Context::default();
        let mut definitions = egui::FontDefinitions::empty();
        definitions.font_data.insert(
            "inter".into(),
            egui::FontData::from_static(include_bytes!(
                "../../assets/fonts/inter/InterVariable.ttf"
            ))
            .into(),
        );
        definitions
            .families
            .insert(egui::FontFamily::Proportional, vec!["inter".into()]);
        ctx.set_fonts(definitions);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().fonts_mut(|fonts| {
                // has_glyph can misreport symbols in the replacement glyph's
                // own face in egui 0.36. Compare atlas glyphs without fallbacks.
                let font = egui::FontId::proportional(crate::theme::text::UI_BODY_13);
                let missing = fonts
                    .layout_no_wrap("\u{10ffff}".into(), font.clone(), egui::Color32::WHITE)
                    .rows[0]
                    .glyphs[0]
                    .uv_rect;
                for reference in SYMBOL_REFERENCE {
                    for &(symbol, covered) in reference.symbols {
                        let sheet =
                            include_str!("../../docs/architecture/keyboard-presentation.md");
                        assert!(
                            sheet.contains(symbol),
                            "Reference sheet is missing {symbol}"
                        );
                        assert!(
                            sheet.contains(&format!("U+{:04X}", u32::from(symbol))),
                            "Reference sheet is missing {symbol}'s code point"
                        );
                        let galley = fonts.layout_no_wrap(
                            symbol.to_string(),
                            font.clone(),
                            egui::Color32::WHITE,
                        );
                        assert_eq!(
                            galley.rows[0].glyphs[0].uv_rect != missing,
                            covered,
                            "{} U+{:04X}",
                            reference.name,
                            u32::from(symbol)
                        );
                    }
                }
            });
        });
        output.textures_delta.clear();
    }
}
