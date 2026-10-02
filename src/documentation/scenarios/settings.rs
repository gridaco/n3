//! The real preferences UI and controller, with in-memory host storage. Never
//! read or mutate the user's actual settings while generating documentation.
use super::{Result, Session};
use crate::{
    controls::Control,
    settings::{
        AccentColor, DisplayUnit, ResolvedTheme, Settings, SettingsController, SettingsStore,
        ThemeMode,
    },
};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Clone, Default)]
struct MemoryStore(Rc<RefCell<Option<Vec<u8>>>>);

impl SettingsStore for MemoryStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.0.borrow().clone())
    }
    fn compare_and_swap(&mut self, expected: Option<&[u8]>, bytes: &[u8]) -> Result<()> {
        if self.0.borrow().as_deref() != expected {
            return Err("Settings changed during the write".into());
        }
        *self.0.borrow_mut() = Some(bytes.to_vec());
        Ok(())
    }
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("bracket.obj")?;
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let defaults = Settings::default();
    s.require(
        defaults.theme == ThemeMode::System
            && ThemeMode::System.resolve(ResolvedTheme::Light) == ResolvedTheme::Light
            && ThemeMode::System.resolve(ResolvedTheme::Dark) == ResolvedTheme::Dark,
        "System is the default and resolves against the host appearance without rewriting the preference",
    )?;
    let example = defaults.to_json()?;
    s.require(
        Settings::from_json(&example)? == defaults,
        "The documented JSON contains the actual validated settings defaults",
    )?;
    s.value("settings-defaults", example.trim_end());

    let store = MemoryStore::default();
    let mut controller = SettingsController::new(store.clone(), defaults.clone())?;
    s.state.apply_user_settings(&controller.reload()?);
    s.require(
        store.0.borrow().is_none(),
        "An absent user settings file uses defaults without creating files during read",
    )?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    let window = s.trace.get(Control::PreferencesWindow)?.rect;
    let title = s.trace.get(Control::PreferencesTitle)?.rect;
    let close = s.trace.get(Control::PreferencesClose)?.rect;
    s.require(
        window.contains_rect(title)
            && window.contains_rect(close)
            && (title.left() - window.left() - (window.right() - close.right())).abs() < 2.0
            && (title.center().y - close.center().y).abs() < 2.0
            && close.left() > title.right(),
        "The Preferences title and close button align at opposite ends of the header",
    )?;
    s.click(Control::PreferencesClose)?;
    // Exercise the application shortcut through normal keyboard routing. The
    // semantic action opens the existing window, including from hidden UI.
    s.state.show_ui = false;
    for _ in 0..2 {
        s.shortcut("preferences.open")?;
        s.require(
            s.state.show_ui && s.state.show_preferences,
            "The Preferences shortcut opens Preferences from hidden UI and remains open on repeated use",
        )?;
    }

    s.witness(Control::SettingsJson)?;
    s.witness(Control::ReloadSettings)?;
    s.witness(Control::AccentColor)?;
    s.witness(Control::ZUp)?;
    s.require(
        s.trace.get(Control::ZUp)?.parents == [Control::PreferencesWindow],
        "Z-up is shown under Viewport in Preferences",
    )?;
    s.witness(Control::ResetAccent)?;
    s.click(Control::Grid)?;
    s.require(
        s.state.settings_ready_for_sync(&s.ctx),
        "A completed Preferences click is ready for persistence without requiring a viewport click",
    )?;
    s.state
        .apply_user_settings(&controller.sync(&s.state.user_settings())?);
    s.click(Control::AppearanceThemeMenu)?;
    s.require(
        s.trace.get(Control::ThemeLight)?.parents
            == [Control::PreferencesWindow, Control::AppearanceThemeMenu],
        "Light is an actual Appearance choice in Preferences",
    )?;
    s.click(Control::ThemeLight)?;
    s.state
        .apply_user_settings(&controller.sync(&s.state.user_settings())?);
    s.settle()?;
    s.require(
        s.state.user_settings().theme == ThemeMode::Light && s.ctx.theme() == egui::Theme::Light,
        "Choosing Light resolves the application UI to light and saves the preference",
    )?;
    // The choice, rather than the host's current appearance, determines the
    // guide pixels. Both examples use the same real Preferences window.
    s.wait(Duration::from_millis(250))?;
    s.capture_image("settings-preferences")?;

    s.click(Control::AppearanceThemeMenu)?;
    s.require(
        s.trace.get(Control::ThemeDark)?.parents
            == [Control::PreferencesWindow, Control::AppearanceThemeMenu],
        "Dark is an actual Appearance choice in Preferences",
    )?;
    s.click(Control::ThemeDark)?;
    s.state
        .apply_user_settings(&controller.sync(&s.state.user_settings())?);
    s.settle()?;
    s.require(
        s.state.user_settings().theme == ThemeMode::Dark && s.ctx.theme() == egui::Theme::Dark,
        "Choosing Dark resolves the application UI to dark and saves the preference",
    )?;
    let style = s.ctx.style_of(s.ctx.theme());
    let visuals = &style.visuals;
    let palette = crate::theme::Palette::from_context(&s.ctx, AccentColor::DEFAULT);
    s.require(
        [visuals.panel_fill, visuals.window_fill, visuals.widgets.hovered.bg_fill,
            visuals.window_stroke.color, palette.workbench_viewport,
            palette.titlebar].into_iter().all(|color| color.r() == color.g() && color.g() == color.b())
            && palette.titlebar == palette.sidebar,
        "Dark uses neutral gray surfaces with a matching titlebar while accent colors remain separate",
    )?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("settings-dark")?;

    let saved = store
        .0
        .borrow()
        .clone()
        .ok_or("UI change did not create settings")?;
    s.require(
        !Settings::from_json(std::str::from_utf8(&saved).map_err(|e| e.to_string())?)?.show_grid,
        "Changing Grid through Preferences writes the same setting used by JSON",
    )?;
    s.require(
        Settings::from_json(std::str::from_utf8(&saved).map_err(|e| e.to_string())?)?.theme
            == ThemeMode::Dark,
        "The selected explicit theme is present in settings.json",
    )?;
    let mut restarted = SettingsController::new(store.clone(), defaults)?;
    s.require(
        restarted.reload()? == s.state.user_settings(),
        "A fresh settings controller restores preferences from the saved text",
    )?;
    // Emulate an agent changing the persisted text. The same controller merges
    // external edits in the native host; no illustrated camera action is faked.
    let mut external: serde_json::Value =
        serde_json::from_slice(&saved).map_err(|e| e.to_string())?;
    external["navigation.viewDurationMs"] = 240.into();
    external["display.lengthUnit"] = "mm".into();
    external["appearance.accentColor"] = "#C3547B".into();
    external["future.extension"] = serde_json::json!({"keep": true});
    *store.0.borrow_mut() = Some(external.to_string().into_bytes());
    s.state
        .apply_user_settings(&controller.sync(&s.state.user_settings())?);
    s.settle()?;
    s.require(
        s.state.view_duration_ms == 240
            && s.state.user_settings().display_unit == DisplayUnit::Millimeters
            && s.state.user_settings().accent_color == AccentColor::new(0xC3, 0x54, 0x7B)
            && s.state.user_settings().theme == ThemeMode::Dark,
        "External JSON changes update existing preferences without changing geometry",
    )?;
    s.click(Control::ResetAccent)?;
    s.require(
        s.state.accent_color == AccentColor::DEFAULT,
        "Reset accent applies the default through its visible Preferences button",
    )?;
    s.state
        .apply_user_settings(&controller.sync(&s.state.user_settings())?);
    s.require(
        s.state.user_settings().accent_color == AccentColor::DEFAULT,
        "Reset accent restores the default through the same persisted preference",
    )?;
    s.reveal_preferences_control(Control::AnimateViews)?;
    s.click(Control::AnimateViews)?;
    s.state
        .apply_user_settings(&controller.sync(&s.state.user_settings())?);
    let merged: serde_json::Value =
        serde_json::from_slice(store.0.borrow().as_ref().unwrap()).map_err(|e| e.to_string())?;
    s.require(
        merged["navigation.animateViews"] == false
            && merged["navigation.viewDurationMs"] == 240
            && merged["appearance.theme"] == "dark"
            && merged["appearance.accentColor"] == "#2563EB"
            && merged["future.extension"]["keep"] == true,
        "UI updates preserve disjoint external changes and unknown JSON keys",
    )?;

    // Dark was an explicit appearance example. Restore the canonical Light
    // presentation through the real UI before illustrating settings recovery.
    s.reveal_preferences_control(Control::AppearanceThemeMenu)?;
    s.click(Control::AppearanceThemeMenu)?;
    s.click(Control::ThemeLight)?;
    s.state
        .apply_user_settings(&controller.sync(&s.state.user_settings())?);
    s.settle()?;
    s.require(
        s.state.user_settings().theme == ThemeMode::Light && s.ctx.theme() == egui::Theme::Light,
        "The settings recovery example returns to the guide's Light presentation",
    )?;

    // Appearance stays in view for its edits; now scroll the same real window
    // to exercise the file controls below the fold.
    let open_before_scroll = s.trace.get(Control::SettingsJson)?.rect;
    s.hover(Control::PreferencesWindow)?;
    s.scroll(0.0, -600.0, true, egui::Modifiers::NONE)?;
    s.wait(Duration::from_millis(350))?;
    let open_after_scroll = s.trace.get(Control::SettingsJson)?.rect;
    s.click(Control::SettingsJson)?;
    let requested_open = std::mem::take(&mut s.state.request_open_settings);
    s.require(
        requested_open,
        &format!("Open settings.json emits a native host request rather than opening files in the UI (before={open_before_scroll:?}, after={open_after_scroll:?}, window={:?})", s.trace.get(Control::PreferencesWindow)?.rect),
    )?;
    controller.ensure_file(&s.state.user_settings())?;

    let valid_bytes = store.0.borrow().clone();
    *store.0.borrow_mut() = Some(b"{ incomplete".to_vec());
    let before_invalid = s.state.user_settings();
    s.hover(Control::PreferencesWindow)?;
    s.scroll(0.0, -12.0, false, egui::Modifiers::NONE)?;
    s.click(Control::ReloadSettings)?;
    let requested_reload = std::mem::take(&mut s.state.request_reload_settings);
    s.require(
        requested_reload,
        "Reload settings emits an explicit host request",
    )?;
    s.state.settings_error = controller.reload().err();
    s.require(
        s.state.settings_error.is_some()
            && s.state.user_settings() == before_invalid
            && store.0.borrow().as_deref() == Some(b"{ incomplete".as_slice()),
        "Invalid JSON leaves active preferences and file contents intact and supplies a visible error",
    )?;
    s.settle()?;
    // The Units section makes Preferences scroll at the tutorial window size.
    // Bring the recovery controls and their error into view before capturing
    // or clicking them; a recorded offscreen control is not user-reachable.
    s.hover(Control::PreferencesWindow)?;
    s.scroll(0.0, -8.0, false, egui::Modifiers::NONE)?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("settings-invalid-json")?;
    *store.0.borrow_mut() = valid_bytes;
    s.click(Control::ReloadSettings)?;
    let requested_reload = std::mem::take(&mut s.state.request_reload_settings);
    s.require(
        requested_reload,
        "The user can retry Reload after repairing their settings file",
    )?;
    s.state.apply_user_settings(&controller.reload()?);
    s.state.settings_error = None;
    s.settle()?;
    s.require(
        s.state.editor.document == document && s.state.editor.revision == revision,
        "Settings reload, persistence, and recovery leave document geometry and edit history untouched",
    )?;
    s.state.host_capabilities = crate::workspace_ui::HostCapabilities::BROWSER;
    s.settle()?;
    s.require(
        s.trace.get(Control::SettingsJson).is_err()
            && s.trace.get(Control::ReloadSettings).is_ok(),
        "Hosts without external settings-file access omit that control from the same Preferences window",
    )?;
    s.click(Control::ReloadSettings)?;
    let requested_reload = std::mem::take(&mut s.state.request_reload_settings);
    s.require(
        requested_reload,
        "Reload remains available through the shared control when settings use browser storage",
    )?;
    s.click(Control::PreferencesClose)?;
    Ok(())
}
