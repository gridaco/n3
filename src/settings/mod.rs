//! Global preferences, separate from documents and platform filesystem effects.
//! The controller owns merge policy; native hosts supply the storage boundary.

mod controller;
mod model;

pub use controller::{SettingsController, SettingsStore};
pub use model::{
    AccentColor, DEFAULT_VIEW_DURATION_MS, DisplayUnit, MAX_SETTINGS_BYTES, MAX_VIEW_DURATION_MS,
    ResolvedTheme, ReturnTo3D, Settings, SnapMode, ThemeMode,
};

#[cfg(test)]
mod tests;
