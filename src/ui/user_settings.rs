//! Map persistent preferences to runtime behavior; no storage or host APIs here.
use super::WorkspaceUi;
use crate::{
    navigation_state::PlanarExit,
    settings::{DisplayUnit, ReturnTo3D, Settings, SnapMode},
    snapping::StepPolicy,
    units::LengthUnit,
};

impl WorkspaceUi {
    /// A value snapshot, independent of document data and transient UI state.
    pub fn user_settings(&self) -> Settings {
        Settings {
            show_grid: self.show_grid,
            show_edges: self.show_edges,
            precise_scroll_zoom: self.precise_scroll_zoom,
            animate_views: self.animate_views,
            view_duration_ms: self.view_duration_ms,
            return_3d: match self.return_3d {
                PlanarExit::PerspectiveOnly => ReturnTo3D::Perspective,
                PlanarExit::OrientationOnly => ReturnTo3D::Orientation,
                PlanarExit::OrientationAndPerspective => ReturnTo3D::Both,
            },
            snap_enabled: self.editor.snapping.enabled,
            snap_mode: match self.editor.snap_policy {
                StepPolicy::Adaptive { .. } => SnapMode::Auto,
                StepPolicy::Fixed => SnapMode::Fixed,
            },
            snap_step_cm: self.editor.snapping.step_cm,
            display_unit: match self.display_unit {
                LengthUnit::Millimeters => DisplayUnit::Millimeters,
                LengthUnit::Centimeters => DisplayUnit::Centimeters,
                LengthUnit::Meters => DisplayUnit::Meters,
                LengthUnit::Inches => DisplayUnit::Inches,
                LengthUnit::Feet => DisplayUnit::Feet,
            },
            theme: self.theme_mode,
            accent_color: self.accent_color,
        }
    }

    /// The controller supplies a validated snapshot. Applying preferences is not
    /// an edit: it must not resize geometry, alter selection, or add undo history.
    pub fn apply_user_settings(&mut self, settings: &Settings) {
        self.show_grid = settings.show_grid;
        self.show_edges = settings.show_edges;
        self.precise_scroll_zoom = settings.precise_scroll_zoom;
        self.animate_views = settings.animate_views;
        if !settings.animate_views {
            self.camera.finish_transition();
        }
        self.view_duration_ms = settings.view_duration_ms;
        self.return_3d = match settings.return_3d {
            ReturnTo3D::Perspective => PlanarExit::PerspectiveOnly,
            ReturnTo3D::Orientation => PlanarExit::OrientationOnly,
            ReturnTo3D::Both => PlanarExit::OrientationAndPerspective,
        };
        self.editor.snapping.enabled = settings.snap_enabled;
        self.editor.snapping.step_cm = settings.snap_step_cm;
        self.editor.snap_policy = match settings.snap_mode {
            SnapMode::Auto => StepPolicy::default(),
            SnapMode::Fixed => StepPolicy::Fixed,
        };
        self.display_unit = match settings.display_unit {
            DisplayUnit::Millimeters => LengthUnit::Millimeters,
            DisplayUnit::Centimeters => LengthUnit::Centimeters,
            DisplayUnit::Meters => LengthUnit::Meters,
            DisplayUnit::Inches => LengthUnit::Inches,
            DisplayUnit::Feet => LengthUnit::Feet,
        };
        self.theme_mode = settings.theme;
        self.accent_color = settings.accent_color;
    }

    /// Don't change the units or movement policy underneath an active gesture
    /// or partially typed value. Hosts retry at the next idle interaction point.
    pub fn settings_ready_for_sync(&self, ctx: &egui::Context) -> bool {
        // Keyboard focus can belong to a checkbox or button after Tab. Only a
        // text editor needs to retain an unfinished typed value across frames.
        let editing_text = ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        !self.editor.is_interacting()
            && self.property_session.is_none()
            && !self.mouse_navigation_active()
            && !self.temporary_navigation_active()
            && !self.pie_owns_input()
            && !editing_text
            && !egui::Popup::is_any_open(ctx)
            && !ctx.egui_is_using_pointer()
            && !ctx.input(|input| input.pointer.any_down() || !input.keys_down.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, PrimitiveKind};
    use crate::settings::{AccentColor, ResolvedTheme, ThemeMode};

    #[test]
    fn system_theme_tracks_host_changes_and_explicit_theme_does_not() {
        let mut ui = WorkspaceUi::new(egui::TextureId::User(0));
        let ctx = egui::Context::default();
        assert_eq!(ui.theme_mode, ThemeMode::System);

        ui.set_system_theme(ResolvedTheme::Dark);
        ui.sync_theme(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        assert_eq!(
            crate::theme::Palette::new(ui.resolved_theme(), ui.accent_color).workbench_viewport,
            crate::theme::Palette::new(ResolvedTheme::Dark, ui.accent_color).workbench_viewport
        );

        ui.set_system_theme(ResolvedTheme::Light);
        ui.sync_theme(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Light);

        ui.theme_mode = ThemeMode::Dark;
        ui.sync_theme(&ctx);
        ui.set_system_theme(ResolvedTheme::Light);
        ui.sync_theme(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
        ctx.set_theme(egui::Theme::Light);
        ui.sync_theme(&ctx);
        assert_eq!(ctx.theme(), egui::Theme::Dark);
    }

    #[test]
    fn keyboard_focus_on_buttons_does_not_block_saving_but_text_and_held_keys_do() {
        let ui = WorkspaceUi::new(egui::TextureId::User(0));
        let ctx = egui::Context::default();
        let text_id = egui::Id::new("settings-test-text");
        let button_id = egui::Id::new("settings-test-button");
        let mut value = String::new();
        ctx.run_ui(egui::RawInput::default(), |root_ui| {
            egui::CentralPanel::default().show(root_ui, |ui| {
                ui.add(egui::TextEdit::singleline(&mut value).id(text_id));
            });
        })
        .textures_delta
        .clear();
        ctx.memory_mut(|memory| memory.request_focus(text_id));
        assert!(!ui.settings_ready_for_sync(&ctx));
        ctx.memory_mut(|memory| memory.request_focus(button_id));
        assert!(ui.settings_ready_for_sync(&ctx));
        ctx.input_mut(|input| {
            input.keys_down.insert(egui::Key::Period);
        });
        assert!(!ui.settings_ready_for_sync(&ctx));
        ctx.input_mut(|input| input.keys_down.clear());
        assert!(ui.settings_ready_for_sync(&ctx));
    }

    #[test]
    fn settings_roundtrip_survive_new_and_do_not_edit_the_document() {
        let mut ui = WorkspaceUi::new(egui::TextureId::User(0));
        assert_eq!(ui.user_settings(), Settings::default());
        ui.editor.insert(PrimitiveKind::Cube).unwrap();
        let document = ui.editor.document.clone();
        let revision = ui.editor.revision;
        let selection = ui.editor.selected_objects.clone();
        let settings = Settings {
            show_grid: false,
            show_edges: false,
            precise_scroll_zoom: true,
            animate_views: false,
            view_duration_ms: 320,
            return_3d: ReturnTo3D::Orientation,
            snap_enabled: false,
            snap_mode: SnapMode::Fixed,
            snap_step_cm: 0.25,
            display_unit: DisplayUnit::Millimeters,
            theme: ThemeMode::Dark,
            accent_color: AccentColor::new(0xA1, 0xB2, 0xC3),
        };
        ui.apply_user_settings(&settings);
        assert_eq!(ui.user_settings(), settings);
        assert_eq!(ui.editor.document, document);
        assert_eq!(ui.editor.revision, revision);
        assert_eq!(ui.editor.selected_objects, selection);
        assert!(ui.editor.undo());
        assert!(ui.editor.document.objects.is_empty());
        assert!(!ui.editor.undo(), "Preferences do not insert an undo step");
        ui.install_document("import.obj".into(), document).unwrap();
        assert_eq!(ui.user_settings(), settings);
        ui.new_document();
        assert_eq!(ui.user_settings(), settings);
        assert_eq!(ui.editor.document, Document::default());
    }
}
