use serde::{Deserialize, Serialize, de};
use serde_json::{Map, Value, value::RawValue};
use std::collections::BTreeMap;

pub(super) type RawValues = BTreeMap<String, Box<RawValue>>;

pub const MAX_SETTINGS_BYTES: usize = 1024 * 1024;
pub const DEFAULT_VIEW_DURATION_MS: u32 = 120;
pub const MAX_VIEW_DURATION_MS: u32 = 1000;

pub(super) const KEYS: [&str; 12] = [
    "viewport.showGrid",
    "viewport.showEdges",
    "navigation.preciseScrollZoom",
    "navigation.animateViews",
    "navigation.viewDurationMs",
    "navigation.returnTo3D",
    "snapping.enabled",
    "snapping.mode",
    "snapping.stepCm",
    "display.lengthUnit",
    "appearance.theme",
    "appearance.accentColor",
];

/// The user's preference. System follows the host appearance, including a
/// change while the application is open; it is never saved as a resolved value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResolvedTheme {
    #[default]
    Light,
    Dark,
}

impl ThemeMode {
    pub const fn resolve(self, system: ResolvedTheme) -> ResolvedTheme {
        match self {
            Self::System => system,
            Self::Light => ResolvedTheme::Light,
            Self::Dark => ResolvedTheme::Dark,
        }
    }
}

/// A user-selected opaque RGB accent. JSON uses a compact, editable CSS-style
/// hex color; rendering decides how to adapt it for each theme and state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccentColor([u8; 3]);

impl AccentColor {
    pub const DEFAULT: Self = Self([0x25, 0x63, 0xEB]);

    pub const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self([red, green, blue])
    }

    pub const fn rgb(self) -> [u8; 3] {
        self.0
    }
}

impl Default for AccentColor {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Serialize for AccentColor {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!(
            "#{:02X}{:02X}{:02X}",
            self.0[0], self.0[1], self.0[2]
        ))
    }
}

impl<'de> Deserialize<'de> for AccentColor {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let bytes = value.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
            return Err(de::Error::custom("expected an opaque #RRGGBB color"));
        }
        let component = |start| u8::from_str_radix(&value[start..start + 2], 16).unwrap();
        Ok(Self::new(component(1), component(3), component(5)))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReturnTo3D {
    Perspective,
    Orientation,
    #[default]
    Both,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapMode {
    #[default]
    Auto,
    Fixed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayUnit {
    #[serde(rename = "mm")]
    Millimeters,
    #[default]
    #[serde(rename = "cm")]
    Centimeters,
    #[serde(rename = "m")]
    Meters,
    #[serde(rename = "in")]
    Inches,
    #[serde(rename = "ft")]
    Feet,
}

/// Typed runtime preferences. JSON parsing goes through `SettingsFile`, which
/// rejects ambiguous or invalid data before it can become an applied snapshot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Settings {
    #[serde(rename = "viewport.showGrid")]
    pub show_grid: bool,
    #[serde(rename = "viewport.showEdges")]
    pub show_edges: bool,
    #[serde(rename = "navigation.preciseScrollZoom")]
    pub precise_scroll_zoom: bool,
    #[serde(rename = "navigation.animateViews")]
    pub animate_views: bool,
    #[serde(rename = "navigation.viewDurationMs")]
    pub view_duration_ms: u32,
    #[serde(rename = "navigation.returnTo3D")]
    pub return_3d: ReturnTo3D,
    #[serde(rename = "snapping.enabled")]
    pub snap_enabled: bool,
    #[serde(rename = "snapping.mode")]
    pub snap_mode: SnapMode,
    #[serde(rename = "snapping.stepCm")]
    pub snap_step_cm: f64,
    #[serde(rename = "display.lengthUnit")]
    pub display_unit: DisplayUnit,
    #[serde(rename = "appearance.theme")]
    pub theme: ThemeMode,
    #[serde(rename = "appearance.accentColor")]
    pub accent_color: AccentColor,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_grid: true,
            show_edges: true,
            precise_scroll_zoom: false,
            animate_views: true,
            view_duration_ms: DEFAULT_VIEW_DURATION_MS,
            return_3d: ReturnTo3D::Both,
            snap_enabled: true,
            snap_mode: SnapMode::Auto,
            snap_step_cm: 1.0,
            display_unit: DisplayUnit::Centimeters,
            theme: ThemeMode::System,
            accent_color: AccentColor::DEFAULT,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.view_duration_ms > MAX_VIEW_DURATION_MS {
            return Err("navigation.viewDurationMs must be between 0 and 1000".into());
        }
        if !self.snap_step_cm.is_finite() || self.snap_step_cm <= 0.0 {
            return Err("snapping.stepCm must be finite and greater than zero".into());
        }
        Ok(())
    }

    /// Decode a strict JSON object with built-in defaults for omitted keys.
    /// Unknown data is ignored here; the controller retains it for file updates.
    pub fn from_json(json: &str) -> Result<Self, String> {
        Ok(SettingsFile::parse(Some(json.as_bytes()), &Self::default())?.effective)
    }

    /// Canonical, readable JSON for the known preferences. Controller writes
    /// additionally preserve unknown data already present in the user's file.
    pub fn to_json(&self) -> Result<String, String> {
        let bytes = SettingsFile::encode(&self.raw_values()?)?;
        String::from_utf8(bytes).map_err(|error| error.to_string())
    }

    pub(super) fn values(&self) -> Result<Map<String, Value>, String> {
        self.validate()?;
        match serde_json::to_value(self).map_err(|error| error.to_string())? {
            Value::Object(values) => Ok(values),
            _ => unreachable!("Settings serializes as an object"),
        }
    }

    pub(super) fn raw_values(&self) -> Result<RawValues, String> {
        self.values()?
            .into_iter()
            .map(|(key, value)| {
                serde_json::value::to_raw_value(&value)
                    .map(|value| (key, value))
                    .map_err(|error| error.to_string())
            })
            .collect()
    }

    fn from_values(values: &RawValues, defaults: &Self) -> Result<Self, String> {
        fn field<T: de::DeserializeOwned>(
            values: &RawValues,
            key: &str,
            default: T,
        ) -> Result<T, String> {
            match values.get(key) {
                Some(value) => serde_json::from_str(value.get())
                    .map_err(|error| format!("Invalid setting {key}: {error}")),
                None => Ok(default),
            }
        }
        let settings = Self {
            show_grid: field(values, "viewport.showGrid", defaults.show_grid)?,
            show_edges: field(values, "viewport.showEdges", defaults.show_edges)?,
            precise_scroll_zoom: field(
                values,
                "navigation.preciseScrollZoom",
                defaults.precise_scroll_zoom,
            )?,
            animate_views: field(values, "navigation.animateViews", defaults.animate_views)?,
            view_duration_ms: field(
                values,
                "navigation.viewDurationMs",
                defaults.view_duration_ms,
            )?,
            return_3d: field(values, "navigation.returnTo3D", defaults.return_3d)?,
            snap_enabled: field(values, "snapping.enabled", defaults.snap_enabled)?,
            snap_mode: field(values, "snapping.mode", defaults.snap_mode)?,
            snap_step_cm: field(values, "snapping.stepCm", defaults.snap_step_cm)?,
            display_unit: field(values, "display.lengthUnit", defaults.display_unit)?,
            theme: field(values, "appearance.theme", defaults.theme)?,
            accent_color: field(values, "appearance.accentColor", defaults.accent_color)?,
        };
        settings.validate()?;
        Ok(settings)
    }
}

/// Retain the original keys (including unknown fields and omitted defaults),
/// independently of the effective typed values used by the application.
pub(super) struct SettingsFile {
    pub values: RawValues,
    pub effective: Settings,
}

impl SettingsFile {
    pub fn parse(bytes: Option<&[u8]>, defaults: &Settings) -> Result<Self, String> {
        let values = match bytes {
            None => RawValues::new(),
            Some(bytes) => {
                if bytes.len() > MAX_SETTINGS_BYTES {
                    return Err("Settings JSON exceeds the 1 MiB limit".into());
                }
                let mut deserializer = serde_json::Deserializer::from_slice(bytes);
                let StrictObject(values) = StrictObject::deserialize(&mut deserializer)
                    .map_err(|error| format!("Invalid settings JSON: {error}"))?;
                deserializer
                    .end()
                    .map_err(|error| format!("Invalid settings JSON: {error}"))?;
                for value in values.values() {
                    validate_tree(value, 1)?;
                }
                values
            }
        };
        let effective = Settings::from_values(&values, defaults)?;
        Ok(Self { values, effective })
    }

    pub fn encode(values: &RawValues) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(values).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        if bytes.len() > MAX_SETTINGS_BYTES {
            return Err("Settings JSON exceeds the 1 MiB limit after formatting".into());
        }
        Ok(bytes)
    }
}

/// Preserve unknown JSON values verbatim, including numeric precision beyond
/// the known settings' f64 range. Duplicate keys are rejected rather than taking
/// serde_json's usual last-value-wins interpretation.
struct StrictObject(RawValues);

impl<'de> Deserialize<'de> for StrictObject {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = StrictObject;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object without duplicate keys")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = RawValues::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom(format!("Duplicate JSON key: {key}")));
                    }
                    values.insert(key, map.next_value()?);
                }
                Ok(StrictObject(values))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}

fn validate_tree(value: &RawValue, depth: usize) -> Result<(), String> {
    if depth > 128 {
        return Err("Settings JSON exceeds the nesting limit".into());
    }
    let invalid = |error| format!("Invalid settings JSON: {error}");
    match value.get().as_bytes().first() {
        Some(b'{') => {
            let StrictObject(values) = serde_json::from_str(value.get()).map_err(invalid)?;
            for child in values.values() {
                validate_tree(child, depth + 1)?;
            }
        }
        Some(b'[') => {
            let values: Vec<Box<RawValue>> = serde_json::from_str(value.get()).map_err(invalid)?;
            for child in values {
                validate_tree(&child, depth + 1)?;
            }
        }
        _ => {} // RawValue has already checked scalar JSON syntax without rounding it.
    }
    Ok(())
}
