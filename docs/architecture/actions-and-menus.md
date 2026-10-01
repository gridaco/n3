# Actions and menus

Application action menus share a typed catalog and an egui presenter. The naming
follows the useful parts of [shadcn's menu anatomy](https://ui.shadcn.com/docs/components/base/context-menu):
trigger, content, item, shortcut, submenu, and separator. Context and dropdown
menus use the same content and items, with different opening behavior.

## Authorities

- `src/input/actions.rs` owns `ActionId`, the stable semantic ID, default label,
  help text, and the existing `Command` that executes it. The current catalog
  covers File, View, Insert, and context-menu actions. New consumers can use the
  same catalog without introducing another command implementation.
- `src/input/bindings.rs` remains the only physical keymap. An action resolves its
  primary press binding by command identity; the first matching binding is the
  displayed primary shortcut, and aliases continue to work. An unbound action
  displays no hint. Menu authors do not supply shortcut strings or key codes.
- `WorkspaceUi::action_state` in `src/ui/action_state.rs` derives visible, enabled,
  and checked state from the current workspace. Metadata is stable; capability
  state is recomputed for presentation and checked again at dispatch.
- `WorkspaceUi::dispatch` executes semantic commands through the existing editor
  and native-host boundaries. Input routing still owns text focus, popups, held
  keys, and active gestures. Showing a shortcut is not registering a new binding.
- `src/ui/controls.rs` owns witnessed UI identities for accessibility and guide
  paths. Default action labels delegate to the catalog. A contextual witness can
  retain a label such as “Hide 2D ruler” while sharing the same action and binding.

Visibility is presentation policy, separate from eligibility. For example, Make
Face is absent in object mode and disabled until the edit selection is suitable.
The 2D ruler action stays visible under View but is disabled in 3D. Frame selection
requires a selection and an available camera. The dispatcher still validates
geometry and observes input ownership; an enabled item does not prevalidate a
candidate edit or promise that every geometry operation will succeed.

## Authoring menus

Use `src/ui/menu.rs`:

| Part                        | N3 API                                                           |
| --------------------------- | ---------------------------------------------------------------- |
| Dropdown trigger            | `menu::dropdown(button)`                                         |
| Context trigger             | `menu::context(response)`                                        |
| Content/group scope         | `menu::content(ui, control, draw)`                               |
| Submenu trigger and content | `menu::submenu(ui, control, draw)`                               |
| Action or checked action    | `menu::Item::new(action, live_state)`                            |
| Shortcut hint               | `menu::Shortcut::for_action(action)`; automatically used by Item |
| Divider                     | `menu::separator(ui)`                                            |
| Typed value dropdown        | `menu::value_dropdown(ui, combo, control, draw)`                 |
| Value item                  | `menu::selectable_value` or `menu::selectable_label`             |

Within `WorkspaceUi`, author ordinary commands with the shared helper:

```rust
menu::content(ui, Control::ViewportMenu, |ui| {
    self.menu_action(ui, ActionId::SelectAll, None);
    self.menu_action(ui, ActionId::Duplicate, None);
    menu::separator(ui);
    self.menu_action(ui, ActionId::Preferences, Some(Control::ViewportPreferences));
});
```

The optional control is an existing contextual label/witness override. It never
changes the command or its shortcut. Most items need only the action ID.
`menu_action` queries current availability and queues a clicked command for the
host's ordinary post-egui dispatch. Layout retries cannot execute it twice, and
Open/Import host effects are preserved. Do not mutate documents or open dialogs
from a menu closure.

`Item` uses native egui buttons and popup ownership. It handles disabled and
hidden states, checked markers, semantic icons, help, and right-aligned muted
shortcut hints. Widget IDs use action identity so hiding an earlier row does not
transfer keyboard focus to another command. `Shortcut` retains a structured
`Binding` until rendering and consumes `ui::shortcut::format(binding, ctx.os())`.
The formatter supplies compact platform notation and separate descriptive names;
the native accessibility node receives the latter as its keyboard shortcut.
See [keyboard presentation](keyboard-presentation.md) for the factual symbol
reference, display policy, font coverage, and rationale. Menu definitions do not
choose symbols or manually author shortcut strings.

Menu rows use the shared eight-point horizontal padding, zero inter-item gap,
and full-width dividers. Submenus use Lucide ChevronRight. Parameter selections
such as theme, units, and primitive type remain ordinary typed value controls;
they share popup styling and do not need synthetic application action IDs.

### Keyboard ownership

`menu/keyboard.rs` provides one focus policy for action menus: initial focus on
the first enabled item, wrapping Up/Down and Tab traversal, Home/End, Right/Left
submenu navigation, and one-level Escape. Hidden and disabled rows do not enter
the focus order. Typed value dropdowns initially focus their selected value and
return to their trigger after keyboard dismissal. Selecting a value by pointer
clears menu focus, preserving native pointer behavior and subsequent document
shortcuts. Outside clicks preserve focus on the clicked control. Enter/Space
activation, focus feedback, and accessibility nodes remain native egui widget
behavior. Pointer movement resumes ordinary menu hover.

egui 0.36 does not initialize mouse-opened menu focus or provide directional
submenu navigation. Its generic spatial focus can leave a menu, so the wrapper
replaces that traversal with row order. It cancels the prequeued spatial move and
handles navigation once per frame across layout retries. Native SubMenu hover
heuristics remain in pointer mode; keyboard mode uses the same native Popup IDs,
frame, and placement, preventing a stationary pointer from switching submenus.

`menu::finish_frame` restores the previous focus owner after dismissal unless an
outside click focused another control. Programmatic Insert uses this same policy;
call sites must not add their own first-item or Escape focus workarounds.

## Extending and checking

Add a catalog action, its live capability policy, and its command implementation.
Add a binding only when a new shortcut is explicitly intended. Add menu entries
by `ActionId`. Keep control IDs stable when moving existing commands so executable
guides keep witnessing the same controls.

Catalog tests check identities, command associations, intended bound/unbound
status, and primary aliases. Menu tests inspect rendered shortcut text, row
boundaries, disabled state, and identity when another row disappears. Application
replay covers selection/mode changes, popup input ownership, deferred host effects,
and one command execution across layout retries. Regenerate and inspect guide
captures when menu appearance changes, then run `just verify`.
