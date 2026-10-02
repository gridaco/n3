//! Live application action availability, shared by menu presentation and dispatch.
use super::*;
use crate::input::actions::ActionState;

impl WorkspaceUi {
    /// Recompute from current state; menu declarations never cache capabilities.
    pub fn action_state(&self, action: ActionId) -> ActionState {
        use ActionId::*;
        let idle = !self.editor.has_transform_session();
        let navigable = !self.editor.blocks_navigation();
        let selected = self.can_frame_selection();
        let mut state = ActionState::default();
        state.enabled = match action {
            New | Open => idle,
            Import | Save | SaveAs => idle && self.editor.can_edit(),
            Preferences => true,
            AnimationPanel => {
                state.checked = Some(self.animation_panel_is_open());
                !self.editor.is_interacting() && !self.mouse_navigation.wants_input()
            }
            TerminalPanel => {
                state.visible = self.host_capabilities.local_terminal;
                state.checked = Some(self.tool_dock.active == Some(ToolDockPanel::Terminal));
                state.visible
                    && !self.editor.is_interacting()
                    && !self.mouse_navigation.wants_input()
            }
            CloseToolDock => {
                self.tool_dock.active.is_some()
                    && !self.editor.is_interacting()
                    && !self.mouse_navigation.wants_input()
            }
            AnimationPlayback => self.can_toggle_animation_playback(),
            SelectAll => idle,
            Duplicate => self.editor.can_duplicate_selection(),
            Delete => idle && self.editor.can_edit() && selected,
            MakeFace => {
                state.visible = self.editor.edit_mode;
                self.editor.can_make_face()
            }
            FrameAll => navigable,
            FrameSelection => navigable && selected,
            LocalView => {
                state.checked = Some(self.is_local_view());
                !self.editor.is_interacting()
                    && (self.is_local_view() || !self.editor.selected_objects.is_empty())
            }
            Xray => {
                state.checked = Some(self.editor.xray_enabled());
                navigable && self.editor.numeric_text().is_none()
            }
            Ruler2D => {
                state.checked = Some(self.show_2d_ruler);
                navigable && self.is_planar_navigation()
            }
            Edges => {
                state.checked = Some(self.show_edges);
                navigable
            }
            ViewPerspective | ViewFront | ViewRight | ViewBack | ViewLeft | ViewTop
            | ViewBottom => navigable,
            InsertCube | InsertCylinder | InsertCone | InsertTorus | InsertPlane | InsertCircle
            | InsertSphere | InsertPolyhedron => {
                self.editor.can_edit() && !self.editor.is_interacting()
            }
        };
        state
    }

    /// Menus choose an action, then leave egui before it executes. Native host
    /// effects and layout retries follow the same path as keyboard commands.
    pub(super) fn menu_action(
        &mut self,
        ui: &mut egui::Ui,
        action: ActionId,
        control: Option<Control>,
    ) -> Option<egui::Response> {
        let mut state = self.action_state(action);
        if control == Some(Control::Ruler2DHide) {
            state.checked = None;
        }
        let mut item = menu::Item::new(action, state);
        if let Some(control) = control {
            item = item.control(control);
        }
        let response = item.show(ui)?;
        if response.clicked() {
            let command = action.command();
            if !self.pending_ui_commands.contains(&command) {
                self.pending_ui_commands.push(command);
            }
            ui.close();
            ui.ctx()
                .memory_mut(|memory| memory.request_focus(shortcuts::viewport_focus_id()));
        }
        Some(response)
    }
}
