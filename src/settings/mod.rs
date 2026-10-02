//! Global preferences, separate from documents and platform filesystem effects.
//! The controller owns merge policy; native hosts supply the storage boundary.

mod controller;
mod model;

pub use controller::{SettingsController, SettingsStore};
#[allow(unused_imports)]
// Available to host stores; the controller also enforces this shared limit.
pub use model::MAX_SETTINGS_BYTES;
pub use model::{
    AccentColor, DEFAULT_VIEW_DURATION_MS, DisplayUnit, MAX_VIEW_DURATION_MS, ResolvedTheme,
    ReturnTo3D, Settings, SnapMode, ThemeMode,
};

#[cfg(test)]
mod tests;
