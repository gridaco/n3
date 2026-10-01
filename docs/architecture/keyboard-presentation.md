# Keyboard presentation

`src/ui/shortcut.rs` owns shortcut presentation. It consumes `Binding`,
`BindingInput`, and egui modifiers from the existing input catalog, with an
explicit `egui::os::OperatingSystem` argument. Menus pass `ctx.os()`; tests can
exercise every policy on any host. The formatter does not register, remap, or
dispatch shortcuts. `menu::Shortcut` accepts an action or a typed binding.

## Factual symbol reference

This is a Unicode and font reference, independent of what N3 displays. Coverage
is verified against `assets/fonts/inter/InterVariable.ttf` (Inter 4.1), with all
fallback fonts removed. The matching test table lives in `ui::shortcut` and
checks the rasterized glyph against the missing glyph. Code points are Unicode
scalar values, not Lucide icon-font codepoints.

| Key or concept             | Recognized symbol(s)         | Unicode code point(s) | Bundled Inter coverage |
| -------------------------- | ---------------------------- | --------------------- | ---------------------- |
| Command                    | ⌘                            | U+2318                | Yes                    |
| Shift                      | ⇧                            | U+21E7                | Yes                    |
| Option / Alt               | ⌥                            | U+2325                | Yes                    |
| Control                    | ⌃                            | U+2303                | Yes                    |
| Caps Lock                  | ⇪                            | U+21EA                | Yes                    |
| Return / Enter             | ↩, ⏎                         | U+21A9, U+23CE        | Both                   |
| Apple keypad Enter         | ⌤                            | U+2324                | No                     |
| Tab                        | ⇥                            | U+21E5                | Yes                    |
| Backtab                    | ⇤                            | U+21E4                | Yes                    |
| Backspace                  | ⌫                            | U+232B                | Yes                    |
| Forward Delete             | ⌦                            | U+2326                | Yes                    |
| Escape                     | ⎋                            | U+238B                | Yes                    |
| Space                      | ␣                            | U+2423                | Yes                    |
| Left Arrow                 | ←                            | U+2190                | Yes                    |
| Up Arrow                   | ↑                            | U+2191                | Yes                    |
| Right Arrow                | →                            | U+2192                | Yes                    |
| Down Arrow                 | ↓                            | U+2193                | Yes                    |
| Home                       | ↖                            | U+2196                | Yes                    |
| End                        | ↘                            | U+2198                | Yes                    |
| Page Up                    | ⇞                            | U+21DE                | Yes                    |
| Page Down                  | ⇟                            | U+21DF                | Yes                    |
| Windows / Super substitute | ⊞                            | U+229E                | No                     |
| Fn, function keys          | No dedicated symbol selected | —                     | Render as text         |

U+229E is SQUARED PLUS, sometimes used as a Windows/Super substitute; it is not
the Windows logo. Caps Lock, Fn, and keypad Enter are reference concepts, not new
bindings. egui's logical `Key::Enter` does not distinguish Return from keypad
Enter, so N3 does not invent that distinction for display.

## Platform display policy

| Input                     | macOS               | Windows / Linux     | Rationale                                                       |
| ------------------------- | ------------------- | ------------------- | --------------------------------------------------------------- |
| Semantic command modifier | ⌘                   | Ctrl                | Resolve the catalog's command to its platform meaning           |
| Physical Control modifier | ⌃                   | Ctrl                | Distinct from Command on macOS; deduplicate Ctrl elsewhere      |
| Shift modifier            | ⇧                   | Shift               | Familiar macOS menu symbol; explicit name elsewhere             |
| Alt modifier              | ⌥                   | Alt                 | Option on macOS; Alt elsewhere                                  |
| Return / Enter            | ⏎                   | Enter               | Supported Return mark; familiar text elsewhere                  |
| Tab                       | ⇥                   | Tab                 | Familiar macOS key mark                                         |
| Shift + Tab               | ⇧⇥                  | Shift+Tab           | Preserve the actual combination instead of substituting Backtab |
| Backspace                 | ⌫                   | Backspace           | Distinguish backward deletion from forward deletion             |
| Forward Delete            | ⌦                   | Delete              | Retain the two distinct input keys                              |
| Directional arrows        | ← ↑ → ↓             | ← ↑ → ↓             | Compact, readily recognizable direction                         |
| Escape                    | Esc                 | Esc                 | Familiar readable abbreviation                                  |
| Space                     | Space               | Space               | Avoid the less familiar open-box symbol                         |
| Home / End                | Home / End          | Home / End          | Names communicate navigation intent clearly                     |
| Page Up / Page Down       | Page Up / Page Down | Page Up / Page Down | Prefer recognizable names over uncommon marks                   |
| Function keys             | F1…F35              | F1…F35              | Preserve the key's existing name                                |
| Physical Super key        | ⌘                   | Windows / Super     | Distinct from semantic command; no logo substitute              |
| Unsupported or other key  | Existing key text   | Existing key text   | Keep a readable fallback; never drop a key                      |

macOS modifiers appear in Control, Option, Shift, Command order, with no plus
separators: `⌘D`, `⇧⌘S`, `⌥Z`. Windows/Linux use Control, Alt, Shift order with
plus separators: `Ctrl+D`, `Ctrl+Shift+S`, `Alt+Z`. Unknown/Android platforms use
the explicit Windows/Linux naming policy; iOS follows egui's Apple policy.
Numpad digits retain the `Numpad` prefix so top-row and numpad bindings remain
distinct. A held Option/Alt input appears once, even when its modifier flag is
also present.

The input catalog currently uses `Modifiers::MAC_CMD` for portable commands.
Its matcher treats `command || mac_cmd` as semantic command; the presenter follows
that same interpretation. On Windows/Linux, Ctrl may be present both physically
and semantically and appears once. The Windows logo key is represented separately
by physical `Key::SuperLeft/Right`; it is never inferred from `command`.

These are N3's display choices, informed by
[Apple's menu modifier symbols and Return notation](https://support.apple.com/en-us/102650)
and [modifier ordering guidance](https://developer.apple.com/design/human-interface-guidelines/keyboards/),
[Microsoft's keyboard interaction guidance](https://learn.microsoft.com/en-us/windows/win32/uxguide/inter-keyboard),
and [GNOME's shortcut notation](https://help.gnome.org/gnome-help/keyboard-nav.html).
Those sources show relevant conventions; applications vary in which nonmodifier
keys they symbolize. Font coverage alone does not select a display representation.

## Descriptive presentation and accessibility

`Presentation` contains `visual` and `descriptive` strings. `key_label` and
`key_name` offer the same distinction for individual egui keys. For example,
`⇧⌘S` has the descriptive name `Shift + Command + S`; `Ctrl+Shift+S` has
`Control + Shift + S`; `⌘,` has `Command + Comma`. Arrows receive full direction
names. The macOS Return and Forward Delete names remain explicit.

Menus use native `Button::shortcut_text` for right-aligned muted visual hints,
preserve the action's accessible name, and set the AccessKit node's
`keyboard_shortcut` to the descriptive string. Disabled items retain their hint
and disabled metadata. Existing keyboard focus, navigation, and activation are
unchanged. This supplies accessibility metadata when AccessKit is active;
screen-reader behavior requires testing with the native platform adapter.

`Binding::key_parts` and `Binding::label` continue to provide the existing
descriptive, macOS-oriented guide and replay labels. Documentation does not
replace its key names with menu glyphs. New platform-specific descriptive UI can
use the formatter's `descriptive` field without changing that guide contract.

Tests cover platform policies, modifier order and deduplication, key fallbacks,
font coverage without fallback faces, descriptive accessibility nodes, menu
spacing at the actual text size, and unchanged catalog matching. The executable
workspace guide witnesses the File menu and Save as shortcut; existing menu
keyboard replay remains the navigation regression gate.
