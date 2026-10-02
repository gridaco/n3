//! Wrapper state is observed every frame, but serialized only when it changes.
//! The comparison borrows strings and diagnostics from the live workspace.
use crate::{settings::ThemeMode, workspace_ui::WorkspaceUi};
use serde::Serialize;
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Status<'a> {
    ready: bool,
    objects: usize,
    selected: usize,
    dirty: bool,
    error: Option<Cow<'a, str>>,
    asset_diagnostics: Cow<'a, [String]>,
    settings_error: Option<Cow<'a, str>>,
    theme: &'static str,
    document_name: Option<Cow<'a, str>>,
    width: u32,
    height: u32,
}

impl<'a> Status<'a> {
    pub(super) fn read(state: &'a WorkspaceUi, ready: bool, size: [u32; 2]) -> Self {
        Self {
            ready,
            objects: state.editor.document.objects.len(),
            selected: state.editor.selected_objects.len(),
            dirty: state.is_dirty(),
            error: state.error.as_deref().map(Cow::Borrowed),
            asset_diagnostics: Cow::Borrowed(&state.asset_diagnostics),
            settings_error: state.settings_error.as_deref().map(Cow::Borrowed),
            // Preserve the established bridge values, independently of the
            // lowercase theme spelling in the settings persistence contract.
            theme: match state.theme_mode {
                ThemeMode::System => "System",
                ThemeMode::Light => "Light",
                ThemeMode::Dark => "Dark",
            },
            document_name: state.path.as_ref().map(|path| path.to_string_lossy()),
            width: size[0],
            height: size[1],
        }
    }

    pub(super) fn json(&self) -> String {
        serde_json::to_string(self).expect("Wrapper status contains only JSON strings and integers")
    }

    fn into_owned(self) -> Status<'static> {
        Status {
            ready: self.ready,
            objects: self.objects,
            selected: self.selected,
            dirty: self.dirty,
            error: self.error.map(|value| Cow::Owned(value.into_owned())),
            asset_diagnostics: Cow::Owned(self.asset_diagnostics.into_owned()),
            settings_error: self
                .settings_error
                .map(|value| Cow::Owned(value.into_owned())),
            theme: self.theme,
            document_name: self
                .document_name
                .map(|value| Cow::Owned(value.into_owned())),
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Default)]
pub(super) struct StatusCache {
    last: Option<Status<'static>>,
}

impl StatusCache {
    pub(super) fn update(&mut self, current: Status<'_>) -> Option<String> {
        if self.last.as_ref() == Some(&current) {
            return None;
        }
        let json = current.json();
        self.last = Some(current.into_owned());
        Some(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_preserves_bridge_schema_and_capitalized_theme() {
        let mut state = WorkspaceUi::new(egui::TextureId::User(0));
        state.theme_mode = ThemeMode::Dark;
        state.path = Some("bracket.n3.json".into());
        state.error = Some("An error".into());
        state.asset_diagnostics = vec!["An unresolved asset".into()];
        state.settings_error = Some("Storage unavailable".into());
        let status = Status::read(&state, true, [1280, 720]);
        let actual: serde_json::Value = serde_json::from_str(&status.json()).unwrap();
        assert_eq!(
            actual,
            serde_json::json!({
                "ready": true,
                "objects": 0,
                "selected": 0,
                "dirty": state.is_dirty(),
                "error": "An error",
                "assetDiagnostics": ["An unresolved asset"],
                "settingsError": "Storage unavailable",
                "theme": "Dark",
                "documentName": "bracket.n3.json",
                "width": 1280,
                "height": 720,
            })
        );
        state.theme_mode = ThemeMode::System;
        assert_eq!(Status::read(&state, false, [1, 1]).theme, "System");
        state.theme_mode = ThemeMode::Light;
        assert_eq!(Status::read(&state, false, [1, 1]).theme, "Light");
    }

    #[test]
    fn cache_emits_only_changes_and_observes_every_bridge_field() {
        let state = WorkspaceUi::new(egui::TextureId::User(0));
        let status = Status::read(&state, false, [1280, 720]);
        let mut cache = StatusCache::default();
        assert!(cache.update(status.clone()).is_some());
        for _ in 0..100 {
            assert!(cache.update(status.clone()).is_none());
        }
        let changes: [fn(&mut Status<'_>); 11] = [
            |value| value.ready = true,
            |value| value.objects = 1,
            |value| value.selected = 1,
            |value| value.dirty = !value.dirty,
            |value| value.error = Some(Cow::Borrowed("Error")),
            |value| value.asset_diagnostics = Cow::Owned(vec!["Notice".into()]),
            |value| value.settings_error = Some(Cow::Borrowed("Storage unavailable")),
            |value| value.theme = "Dark",
            |value| value.document_name = Some(Cow::Borrowed("other.n3.json")),
            |value| value.width = 640,
            |value| value.height = 480,
        ];
        for change in changes {
            let mut changed = status.clone();
            change(&mut changed);
            assert!(cache.update(changed.clone()).is_some());
            assert!(cache.update(changed).is_none());
            assert!(cache.update(status.clone()).is_some());
        }
    }
}
