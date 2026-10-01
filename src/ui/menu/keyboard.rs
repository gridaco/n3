//! Focus policy for menus; egui still owns widgets, activation and popups.
use egui::containers::menu::{MenuState, find_menu_root};
use egui::{Context, Event, Id, Key, Modifiers, Response, Ui};

#[derive(Clone, Copy)]
pub(super) struct Parent {
    pub menu: Id,
    pub row: Id,
}

#[derive(Clone, Copy)]
struct Row {
    id: Id,
    submenu: Option<Id>,
}

#[derive(Clone, Default)]
struct Focus {
    rows: Vec<Row>,
    last_pass: u64,
    return_to: Option<Id>,
    trigger: Option<Id>,
    initialized: bool,
    focused: Option<Id>,
    preferred: Option<Id>,
}

#[derive(Clone, Default)]
struct InputMode {
    frame: Option<u64>,
    handled_frame: Option<u64>,
    keyboard: bool,
    // The current menu's rows and original focus, for dismissal by an outside click.
    restore: Option<(Vec<Id>, Id)>,
}

fn mode_id() -> Id {
    Id::new("n3.menu.input-mode")
}
fn focus_id(menu: Id) -> Id {
    menu.with("n3.menu.focus")
}

pub(super) fn uses_keyboard(ctx: &Context) -> bool {
    ctx.data(|data| {
        data.get_temp::<InputMode>(mode_id())
            .unwrap_or_default()
            .keyboard
    })
}

pub(super) fn begin(ui: &Ui, parent: Option<Parent>) {
    let ctx = ui.ctx();
    let frame = ctx.cumulative_frame_nr();
    let pointer_used = ctx.input(|i| {
        i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed() || i.pointer.any_released()
    });
    let key_used = ctx.input(|i| {
        i.events.iter().any(|event| {
            matches!(
                event,
                Event::Key {
                    key: Key::ArrowUp
                        | Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::Tab
                        | Key::Home
                        | Key::End
                        | Key::Escape
                        | Key::Enter
                        | Key::Space,
                    pressed: true,
                    ..
                }
            )
        })
    });
    ctx.data_mut(|data| {
        let mode = data.get_temp_mut_or_default::<InputMode>(mode_id());
        if mode.frame != Some(frame) {
            mode.frame = Some(frame);
            if pointer_used {
                mode.keyboard = false;
            }
            if key_used {
                mode.keyboard = true;
            }
        }
    });
    // egui queues spatial navigation before UI runs. Menus use row order and
    // must not subsequently move focus to an unrelated panel or parent menu.
    ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
    let id = focus_id(find_menu_root(ui).id);
    let pass = ctx.cumulative_pass_nr();
    let focused = ui.memory(|memory| memory.focused());
    let keyboard = uses_keyboard(ctx);
    let return_to = ctx.data_mut(|data| {
        let state = data.get_temp_mut_or_default::<Focus>(id);
        if state.last_pass + 1 < pass || state.return_to.is_none() {
            state.return_to = Some(focused.unwrap_or(crate::shortcuts::viewport_focus_id()));
            state.initialized = false;
        }
        state.last_pass = pass;
        state.rows.clear();
        state.preferred = None;
        if keyboard {
            state.trigger.or(state.return_to).unwrap()
        } else {
            state.return_to.unwrap()
        }
    });
    if parent.is_none() {
        ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<InputMode>(mode_id()).restore =
                Some((Vec::new(), return_to))
        });
    }
}

pub(super) fn row(ui: &Ui, response: &Response, submenu: Option<Id>) {
    if !response.enabled() || ui.is_sizing_pass() {
        return;
    }
    let id = focus_id(find_menu_root(ui).id);
    ui.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<Focus>(id).rows.push(Row {
            id: response.id,
            submenu,
        });
        if let Some((rows, _)) = &mut data.get_temp_mut_or_default::<InputMode>(mode_id()).restore {
            rows.push(response.id);
        }
    });
    if !uses_keyboard(ui.ctx())
        && response.hovered()
        && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
    {
        response.request_focus();
    }
    if response.has_focus() && uses_keyboard(ui.ctx()) {
        response.scroll_to_me(None);
    }
}

pub(super) fn prefer_initial(ui: &Ui, response: &Response) {
    if response.enabled() && !ui.is_sizing_pass() {
        ui.ctx().data_mut(|data| {
            data.get_temp_mut_or_default::<Focus>(focus_id(find_menu_root(ui).id))
                .preferred = Some(response.id)
        });
    }
}

pub(super) fn return_to(ctx: &Context, menu: Id, target: Id) {
    let keyboard = uses_keyboard(ctx);
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Focus>(focus_id(menu))
            .trigger = Some(target);
        if keyboard
            && let Some((_, restore)) =
                &mut data.get_temp_mut_or_default::<InputMode>(mode_id()).restore
        {
            *restore = target;
        }
    });
}

pub(super) fn pointer_value_activation(ctx: &Context) {
    // Pointer selection keeps the native dropdown's unfocused result. Keeping
    // keyboard focus on its trigger would intercept the next document shortcut.
    ctx.data_mut(|data| data.get_temp_mut_or_default::<InputMode>(mode_id()).restore = None);
}

pub(super) fn finish(ui: &mut Ui, parent: Option<Parent>) {
    if ui.will_parent_close() {
        return;
    }
    let menu = find_menu_root(ui).id;
    let ctx = ui.ctx().clone();
    let mut state = ctx.data(|data| data.get_temp::<Focus>(focus_id(menu)).unwrap_or_default());
    // Closing needs no measured rows: Escape must also work during a child's
    // first, invisible sizing pass, without bubbling to the root popup.
    let close = ui.input_mut(|i| {
        if i.consume_key(Modifiers::NONE, Key::Escape) {
            Some(Key::Escape)
        } else if i.consume_key(Modifiers::NONE, Key::ArrowLeft) {
            Some(Key::ArrowLeft)
        } else {
            None
        }
    });
    if let Some(key) = close {
        if claim_navigation(&ctx) {
            if let Some(parent) = parent {
                MenuState::from_id(&ctx, parent.menu, |s| s.open_item = None);
                ui.memory_mut(|m| m.request_focus(parent.row));
            } else if key == Key::Escape {
                ui.close();
                ui.memory_mut(|m| m.request_focus(state.trigger.or(state.return_to).unwrap()));
            }
            ctx.request_repaint();
        }
        return;
    }
    if ui.is_sizing_pass() || !ui.is_visible() {
        return;
    }
    if state.rows.is_empty() {
        return;
    }
    let focused = ui.memory(|memory| memory.focused());
    let index = state.rows.iter().position(|row| Some(row.id) == focused);
    let deepest = MenuState::is_deepest_open_sub_menu(&ctx, menu);
    if index.is_none() && !deepest {
        return;
    }
    // A click outside may already have focused a text field drawn before this
    // popup. Keep that deliberate transfer while native dismissal completes.
    let focus_is_external = focused.is_some_and(|id| {
        ctx.data(|data| {
            data.get_temp::<InputMode>(mode_id())
                .and_then(|mode| mode.restore)
                .is_none_or(|(rows, _)| !rows.contains(&id))
        })
    });
    if state.initialized && index.is_none() && focus_is_external && focused != state.focused {
        return;
    }
    state.initialized = true;
    let mut index = index
        .or_else(|| {
            state
                .rows
                .iter()
                .position(|row| Some(row.id) == state.preferred)
        })
        .unwrap_or(0);
    if focused != Some(state.rows[index].id) {
        ui.memory_mut(|memory| memory.request_focus(state.rows[index].id));
    }
    let keys = ui.input_mut(|i| {
        let mut keys = Vec::new();
        for key in [
            Key::ArrowDown,
            Key::ArrowUp,
            Key::Home,
            Key::End,
            Key::ArrowRight,
        ] {
            if i.consume_key(Modifiers::NONE, key) {
                keys.push(key);
            }
        }
        if i.consume_key(Modifiers::SHIFT, Key::Tab) {
            keys.push(Key::ArrowUp);
        }
        if i.consume_key(Modifiers::NONE, Key::Tab) {
            keys.push(Key::ArrowDown);
        }
        keys
    });
    if !keys.is_empty() && claim_navigation(&ctx) {
        for key in keys {
            match key {
                Key::ArrowDown | Key::ArrowUp | Key::Home | Key::End => {
                    index = match key {
                        Key::ArrowDown => (index + 1) % state.rows.len(),
                        Key::ArrowUp => (index + state.rows.len() - 1) % state.rows.len(),
                        Key::Home => 0,
                        _ => state.rows.len() - 1,
                    };
                    MenuState::from_id(&ctx, menu, |s| s.open_item = None);
                    ui.memory_mut(|m| m.request_focus(state.rows[index].id));
                }
                Key::ArrowRight => {
                    if let Some(child) = state.rows[index].submenu {
                        // The native menu state expires unseen children. Keep
                        // this requested child alive until the next layout.
                        MenuState::mark_shown(&ctx, child);
                        MenuState::from_id(&ctx, menu, |s| s.open_item = Some(child));
                    }
                }
                _ => {}
            }
        }
        ctx.request_repaint();
    }
    let focus = ui.memory(|m| m.focused());
    state.focused = focus;
    if let Some(id) = focus.filter(|id| state.rows.iter().any(|row| row.id == *id)) {
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                id,
                egui::EventFilter {
                    tab: true,
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    escape: true,
                },
            )
        });
    }
    ctx.data_mut(|data| data.insert_temp(focus_id(menu), state));
}

fn claim_navigation(ctx: &Context) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| {
        let mode = data.get_temp_mut_or_default::<InputMode>(mode_id());
        if mode.handled_frame == Some(frame) {
            return false;
        }
        mode.handled_frame = Some(frame);
        true
    })
}

pub(super) fn handled_this_frame(ctx: &Context) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|data| {
        data.get_temp::<InputMode>(mode_id())
            .unwrap_or_default()
            .handled_frame
            == Some(frame)
    })
}

/// Restore the previous owner only if dismissal did not focus another control.
pub(crate) fn finish_frame(ctx: &Context) {
    if egui::Popup::is_any_open(ctx) {
        return;
    }
    let restore = ctx.data_mut(|data| {
        data.remove_by_type::<Focus>();
        data.get_temp_mut_or_default::<InputMode>(mode_id())
            .restore
            .take()
    });
    if let Some((rows, target)) = restore {
        ctx.memory_mut(|m| {
            if m.focused().is_none_or(|id| rows.contains(&id)) {
                m.request_focus(target);
            }
        });
    }
}
