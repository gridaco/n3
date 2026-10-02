//! Shared anatomy for action menus. Hosts own opening and command dispatch;
//! rows consume live action state and resolve shortcuts from the input catalog.
use crate::{
    input::{actions::ActionId, bindings::Binding},
    theme,
};
use egui::{AtomExt as _, Button, InnerResponse, Response, Ui};

use super::{
    controls::{self, Control},
    lucide::Icon,
    shortcut::{self, Presentation},
};
mod keyboard;
pub(crate) use keyboard::finish_frame;

/// A shortcut presenter retains the typed binding until it is rendered.
/// Platform notation and descriptive names come from the shared formatter;
/// menu definitions never author shortcut strings.
pub(crate) struct Shortcut(Binding);

impl Shortcut {
    pub(crate) fn for_action(action: ActionId) -> Option<Self> {
        action.shortcut().map(Self::for_binding)
    }

    pub(crate) fn for_binding(binding: Binding) -> Self {
        Self(binding)
    }

    pub(crate) fn presentation(&self, os: egui::os::OperatingSystem) -> Presentation {
        shortcut::format(self.0, os)
    }
}

pub(crate) use crate::input::actions::ActionState;

pub(crate) struct Item {
    action: ActionId,
    state: ActionState,
    control: Option<Control>,
    label: Option<&'static str>,
}

impl Item {
    pub(crate) fn new(action: ActionId, state: ActionState) -> Self {
        Self {
            action,
            state,
            control: None,
            label: None,
        }
    }

    /// Use a contextual label and witness, while retaining the action's command
    /// identity and shortcut (for example, "Hide 2D ruler").
    pub(crate) fn control(mut self, control: Control) -> Self {
        self.control = Some(control);
        self
    }

    /// Contextual presentation keeps the action and its shortcut authoritative.
    #[allow(dead_code)] // Contextual labels are part of the shared menu contract.
    pub(crate) fn label(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
    }

    pub(crate) fn show(self, ui: &mut Ui) -> Option<Response> {
        if !self.state.visible {
            return None;
        }
        let control = self
            .control
            .unwrap_or_else(|| control_for_action(self.action));
        let label = self.label.unwrap_or_else(|| {
            self.control
                .map_or_else(|| self.action.label(), Control::label)
        });
        let mut atoms = egui::Atoms::new(());
        if let Some(checked) = self.state.checked {
            // A fixed check column prevents the label moving when toggled.
            atoms.push_right(
                egui::RichText::new(if checked { "✓" } else { "" })
                    .atom_size(egui::Vec2::splat(ui.spacing().icon_width)),
            );
        } else if let Some(icon) = icon_for_action(self.action) {
            atoms.push_right(icon.text(ui.spacing().icon_width));
        }
        atoms.push_right(label);
        let mut button = Button::new(atoms).min_size(egui::vec2(theme::size::STEP_52, 0.0));
        let shortcut =
            Shortcut::for_action(self.action).map(|hint| hint.presentation(ui.ctx().os()));
        if let Some(shortcut) = &shortcut {
            button = button.shortcut_text(shortcut.visual.clone());
        }
        // Stable action identity keeps keyboard focus on the same command when
        // a neighboring row becomes hidden as the workspace context changes.
        let id = ui.make_persistent_id(self.action.id());
        let response = ui
            .scope_builder(egui::UiBuilder::new().id(id), |ui| {
                ui.add_enabled(self.state.enabled, button)
            })
            .inner;
        // Decorative icons and shortcut text are not part of the action name.
        response.widget_info(|| match self.state.checked {
            Some(checked) => egui::WidgetInfo::selected(
                egui::WidgetType::Checkbox,
                response.enabled(),
                checked,
                label,
            ),
            None => egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label),
        });
        if let Some(shortcut) = &shortcut {
            ui.ctx().accesskit_node_builder(response.id, |node| {
                node.set_keyboard_shortcut(shortcut.descriptive.clone());
            });
        }
        controls::record(ui.ctx(), control, label, response.rect, response.enabled());
        keyboard::row(ui, &response, None);
        Some(if let Some(help) = self.action.help() {
            response.on_hover_text(help)
        } else {
            response
        })
    }
}

pub(crate) fn dropdown(button: Button<'_>) -> egui::containers::menu::MenuButton<'_> {
    egui::containers::menu::MenuButton::from_button(button)
        .config(egui::containers::menu::MenuConfig::new().style(theme::menu_style))
}

pub(crate) fn context(response: &Response) -> egui::Popup<'_> {
    egui::Popup::context_menu(response).style(theme::menu_style)
}

/// Typed value dropdowns share navigation without inventing application actions.
pub(crate) fn value_dropdown<R>(
    ui: &mut Ui,
    combo: egui::ComboBox,
    control: Control,
    draw: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<Option<R>> {
    let mut menu_id = None;
    let result = combo
        .popup_style(theme::menu_style.into())
        .show_ui(ui, |ui| {
            menu_id = Some(egui::containers::menu::find_menu_root(ui).id);
            let ctx = ui.ctx().clone();
            keyboard::begin(ui, None);
            let value = controls::scope(&ctx, control, || draw(ui));
            keyboard::finish(ui, None);
            value
        });
    if let Some(id) = menu_id {
        keyboard::return_to(ui.ctx(), id, result.response.id);
    }
    // A closed value trigger is a button, not a text-edit session. Let Escape
    // dismiss its containing dialog after focus returns from the popup. An
    // open popup consumes its own Escape before this point.
    if result.inner.is_none()
        && result.response.lost_focus()
        && !keyboard::handled_this_frame(ui.ctx())
        && ui
            .stack()
            .iter()
            .any(|stack| stack.kind() == Some(egui::UiKind::Window))
        && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        ui.close_kind(egui::UiKind::Window);
    }
    result
}

pub(crate) fn selectable_value<T: PartialEq>(
    ui: &mut Ui,
    current: &mut T,
    value: T,
    label: impl Into<egui::WidgetText>,
) -> Response {
    let selected = *current == value;
    let response = ui.selectable_value(current, value, label);
    value_row(ui, &response, selected);
    response
}

pub(crate) fn selectable_label(
    ui: &mut Ui,
    selected: bool,
    label: impl Into<egui::WidgetText>,
) -> Response {
    let response = ui.selectable_label(selected, label);
    value_row(ui, &response, selected);
    response
}

fn value_row(ui: &mut Ui, response: &Response, selected: bool) {
    keyboard::row(ui, response, None);
    if selected {
        keyboard::prefer_initial(ui, response);
    }
    if response.clicked() {
        if response.clicked_by(egui::PointerButton::Primary) {
            response.surrender_focus();
            keyboard::pointer_value_activation(ui.ctx());
        }
        ui.close();
    }
}

pub(crate) fn content<R>(ui: &mut Ui, control: Control, draw: impl FnOnce(&mut Ui) -> R) -> R {
    content_with_parent(ui, control, None, draw)
}

fn content_with_parent<R>(
    ui: &mut Ui,
    control: Control,
    parent: Option<keyboard::Parent>,
    draw: impl FnOnce(&mut Ui) -> R,
) -> R {
    theme::menu_style(ui.style_mut());
    ui.set_min_width(theme::size::STEP_52);
    let ctx = ui.ctx().clone();
    keyboard::begin(ui, parent);
    let result = controls::scope(&ctx, control, || draw(ui));
    keyboard::finish(ui, parent);
    result
}

pub(crate) fn submenu<R>(
    ui: &mut Ui,
    control: Control,
    draw: impl FnOnce(&mut Ui) -> R,
) -> (Response, Option<InnerResponse<R>>) {
    use egui::containers::menu::{MenuConfig, MenuState, SubMenu, find_menu_root};
    let parent_id = find_menu_root(ui).id;
    let button = controls::submenu_button(control);
    let was_open = MenuState::from_id(ui.ctx(), parent_id, |s| {
        s.open_item == Some(SubMenu::id_from_widget_id(ui.next_auto_id()))
    });
    let inactive = ui.visuals().widgets.inactive;
    if was_open {
        ui.visuals_mut().widgets.inactive = ui.visuals().widgets.open;
    }
    let response = ui.add(button.button);
    ui.visuals_mut().widgets.inactive = inactive;
    let child_id = SubMenu::id_from_widget_id(response.id);
    keyboard::row(ui, &response, Some(child_id));
    let parent = Some(keyboard::Parent {
        menu: parent_id,
        row: response.id,
    });
    let config = MenuConfig::new().style(theme::menu_style);
    let popup = if keyboard::uses_keyboard(ui.ctx()) {
        // Native SubMenu's hover heuristics otherwise override arrow navigation
        // when the pointer remains on an old row. Use the same native Popup and
        // IDs while keyboard navigation owns submenu selection.
        if response.clicked() {
            if !was_open {
                MenuState::mark_shown(ui.ctx(), child_id);
            }
            MenuState::from_id(ui.ctx(), parent_id, |s| {
                s.open_item = (!was_open).then_some(child_id)
            });
        }
        let open = MenuState::from_id(ui.ctx(), parent_id, |s| s.open_item == Some(child_id));
        let frame = egui::Frame::menu(ui.style());
        let mut anchor = response.clone();
        anchor.interact_rect = anchor
            .interact_rect
            .expand2(egui::vec2(0.0, frame.total_margin().sum().y / 2.0));
        let popup = egui::Popup::from_response(&anchor)
            .id(child_id)
            .open(open)
            .align(egui::emath::RectAlign::RIGHT_START)
            .gap(frame.total_margin().sum().x / 2.0 + 2.0)
            .layout(egui::Layout::top_down_justified(egui::Align::Min))
            .style(theme::menu_style)
            .frame(frame)
            .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
            .info(
                egui::UiStackInfo::new(egui::UiKind::Menu)
                    .with_tag_value(MenuConfig::MENU_CONFIG_TAG, config),
            )
            .show(|ui| content_with_parent(ui, control, parent, draw));
        if popup.as_ref().is_some_and(|p| p.response.should_close()) {
            ui.close();
        }
        popup
    } else {
        button.sub_menu.config(config).show(ui, &response, |ui| {
            content_with_parent(ui, control, parent, draw)
        })
    };
    controls::record(
        ui.ctx(),
        control,
        control.label(),
        response.rect,
        response.enabled(),
    );
    (response, popup)
}

pub(crate) fn separator(ui: &mut Ui) -> Response {
    controls::menu_separator(ui)
}

fn icon_for_action(action: ActionId) -> Option<Icon> {
    match action {
        ActionId::InsertCube => Some(Icon::Box),
        ActionId::InsertCylinder => Some(Icon::Cylinder),
        ActionId::InsertCone => Some(Icon::Cone),
        ActionId::InsertTorus => Some(Icon::Torus),
        ActionId::InsertPlane => Some(Icon::RectangleHorizontal),
        ActionId::InsertCircle => Some(Icon::Circle),
        // TODO: Replace the generic icons in one dedicated icon pass.
        ActionId::InsertSphere | ActionId::InsertPolyhedron => Some(Icon::ScanBox),
        _ => None,
    }
}

fn control_for_action(action: ActionId) -> Control {
    match action {
        ActionId::New => Control::New,
        ActionId::Open => Control::Open,
        ActionId::Import => Control::Import,
        ActionId::Save => Control::Save,
        ActionId::SaveAs => Control::SaveAs,
        ActionId::Preferences => Control::Preferences,
        ActionId::AnimationPanel => Control::AnimationPanelToggle,
        ActionId::TerminalPanel => Control::TerminalPanelToggle,
        ActionId::CloseToolDock => Control::ToolDockClose,
        ActionId::AnimationPlayback => Control::ScenePlay,
        ActionId::SelectAll => Control::SelectAll,
        ActionId::Duplicate => Control::DuplicateSelection,
        ActionId::Delete => Control::DeleteSelection,
        ActionId::MakeFace => Control::MakeFace,
        ActionId::FrameAll => Control::Frame,
        ActionId::FrameSelection => Control::FrameSelection,
        ActionId::LocalView => Control::LocalViewMenu,
        ActionId::Xray => Control::Xray,
        ActionId::Ruler2D => Control::Ruler2DViewToggle,
        ActionId::Edges => Control::Edges,
        ActionId::ViewPerspective => Control::ViewPerspective,
        ActionId::ViewFront => Control::ViewFront,
        ActionId::ViewRight => Control::ViewRight,
        ActionId::ViewBack => Control::ViewBack,
        ActionId::ViewLeft => Control::ViewLeft,
        ActionId::ViewTop => Control::ViewTop,
        ActionId::ViewBottom => Control::ViewBottom,
        ActionId::InsertCube => Control::InsertCube,
        ActionId::InsertCylinder => Control::InsertCylinder,
        ActionId::InsertCone => Control::InsertCone,
        ActionId::InsertTorus => Control::InsertTorus,
        ActionId::InsertPlane => Control::InsertPlane,
        ActionId::InsertCircle => Control::InsertCircle,
        ActionId::InsertSphere => Control::InsertSphere,
        ActionId::InsertPolyhedron => Control::InsertPolyhedron,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_rows_paint_action_shortcuts_on_the_right_without_hover_gaps() {
        fn collect_text(shape: &egui::Shape, texts: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::Shape::Text(text) => {
                    // Compare typographic line boxes, not ink bounds: a comma
                    // correctly sits below other glyphs on the same line.
                    texts.push((
                        text.galley.job.text.clone(),
                        text.galley.rect.translate(text.pos.to_vec2()),
                    ))
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect_text(shape, texts);
                    }
                }
                _ => {}
            }
        }

        for os in [
            egui::os::OperatingSystem::Mac,
            egui::os::OperatingSystem::Windows,
            egui::os::OperatingSystem::Nix,
        ] {
            let ctx = egui::Context::default();
            ctx.set_os(os);
            ctx.enable_accesskit();
            super::super::workspace_ui::configure_context(&ctx);
            let actions = [
                (ActionId::SelectAll, true),
                (ActionId::Delete, false),
                (ActionId::Preferences, true),
                (ActionId::SaveAs, true),
                (ActionId::Xray, true),
            ];
            let mut rows = Vec::new();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                rows.clear();
                ui.set_width(320.0);
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                    content(ui, Control::ViewportMenu, |ui| {
                        for (action, enabled) in actions {
                            rows.push(
                                Item::new(
                                    action,
                                    ActionState {
                                        enabled,
                                        ..Default::default()
                                    },
                                )
                                .show(ui)
                                .unwrap(),
                            );
                        }
                    });
                });
            });
            let mut texts = Vec::new();
            for shape in &output.shapes {
                collect_text(&shape.shape, &mut texts);
            }
            output.textures_delta.clear();
            for ((action, enabled), row) in actions.into_iter().zip(&rows) {
                assert_eq!(row.enabled(), enabled);
                let label = texts
                    .iter()
                    .find(|(text, _)| text == action.label())
                    .expect("The action label must be painted independently of its shortcut")
                    .1;
                let expected_hint = shortcut::format(action.shortcut().unwrap(), ctx.os()).visual;
                let hint = texts
                    .iter()
                    .filter(|(text, _)| text == &expected_hint)
                    // Delete is both the action label and its shortcut; locate the
                    // rightmost painted text instead of mistaking the two copies.
                    .max_by(|(_, a), (_, b)| a.left().total_cmp(&b.left()))
                    .expect("The canonical shortcut must be painted, including on disabled rows")
                    .1;
                assert!(
                    hint.left() > label.right(),
                    "Shortcut must follow a separate label column: {action:?}, label={label:?}, hint={hint:?}"
                );
                assert!(
                    hint.center().x > row.rect.center().x,
                    "Shortcut must occupy the right side"
                );
                assert!(row.rect.contains(hint.center()));
                assert!(
                    (hint.center().y - label.center().y).abs() < 2.0,
                    "Shortcut and label must align at the actual menu text size"
                );
                let tree = output.platform_output.accesskit_update.as_ref().unwrap();
                let node = &tree
                    .nodes
                    .iter()
                    .find(|(id, _)| *id == row.id.accesskit_id())
                    .unwrap()
                    .1;
                assert_eq!(node.label(), Some(action.label()));
                let descriptive = shortcut::format(action.shortcut().unwrap(), os).descriptive;
                assert_eq!(node.keyboard_shortcut(), Some(descriptive.as_str()));
            }
            for pair in rows.windows(2) {
                assert_eq!(
                    pair[0].rect.bottom(),
                    pair[1].rect.top(),
                    "Adjacent menu rows must touch"
                );
            }
        }
    }

    #[test]
    fn hidden_rows_leave_neighbor_identity_stable_and_allocate_no_space() {
        let ctx = egui::Context::default();
        let mut open_id = None;
        let mut first_top = 0.0;
        for show_new in [true, false] {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                content(ui, Control::FileMenu, |ui| {
                    let new = Item::new(
                        ActionId::New,
                        ActionState {
                            visible: show_new,
                            ..Default::default()
                        },
                    )
                    .show(ui);
                    let open = Item::new(ActionId::Open, ActionState::default())
                        .show(ui)
                        .unwrap();
                    if let Some(new) = new {
                        first_top = new.rect.top();
                        open_id = Some(open.id);
                    } else {
                        assert_eq!(open_id, Some(open.id));
                        assert_eq!(open.rect.top(), first_top);
                    }
                });
            });
            output.textures_delta.clear();
        }
    }
}
