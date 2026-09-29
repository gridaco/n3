use super::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct MemoryState {
    bytes: Option<Vec<u8>>,
    writes: usize,
    read_error: bool,
    write_error: bool,
    race: Option<Option<Vec<u8>>>,
}

#[derive(Clone, Default)]
struct MemoryStore(Rc<RefCell<MemoryState>>);

impl MemoryStore {
    fn replace(&self, json: Option<&str>) {
        self.0.borrow_mut().bytes = json.map(|json| json.as_bytes().to_vec());
    }

    fn bytes(&self) -> Option<Vec<u8>> {
        self.0.borrow().bytes.clone()
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&self.bytes().unwrap()).unwrap()
    }

    fn writes(&self) -> usize {
        self.0.borrow().writes
    }
}

impl SettingsStore for MemoryStore {
    fn read(&mut self) -> Result<Option<Vec<u8>>, String> {
        let state = self.0.borrow();
        if state.read_error {
            Err("Simulated read failure".into())
        } else {
            Ok(state.bytes.clone())
        }
    }

    fn compare_and_swap(
        &mut self,
        expected: Option<&[u8]>,
        replacement: &[u8],
    ) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        if let Some(bytes) = state.race.take() {
            state.bytes = bytes;
        }
        if state.write_error {
            return Err("Simulated write failure".into());
        }
        if state.bytes.as_deref() != expected {
            return Err("Settings changed before replacement".into());
        }
        state.bytes = Some(replacement.to_vec());
        state.writes += 1;
        Ok(())
    }
}

fn controller(json: Option<&str>) -> (SettingsController<MemoryStore>, MemoryStore) {
    let store = MemoryStore::default();
    store.replace(json);
    (
        SettingsController::new(store.clone(), Settings::default()).unwrap(),
        store,
    )
}

#[test]
fn default_json_has_exactly_the_supported_flat_keys_and_roundtrips() {
    let settings = Settings::default();
    let text = settings.to_json().unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        value,
        json!({
            "viewport.showGrid": true,
            "viewport.showEdges": true,
            "navigation.preciseScrollZoom": false,
            "navigation.animateViews": true,
            "navigation.viewDurationMs": 120,
            "navigation.returnTo3D": "both",
            "snapping.enabled": true,
            "snapping.mode": "auto",
            "snapping.stepCm": 1.0,
            "display.lengthUnit": "cm",
            "appearance.theme": "system",
            "appearance.accentColor": "#2563EB",
        })
    );
    assert_eq!(Settings::from_json(&text).unwrap(), settings);
    assert_eq!(Settings::from_json("{}").unwrap(), settings);
    assert!(text.ends_with('\n'));
}

#[test]
fn partial_files_use_defaults_and_all_enum_variants_have_defined_json() {
    for (name, expected) in [
        ("perspective", ReturnTo3D::Perspective),
        ("orientation", ReturnTo3D::Orientation),
        ("both", ReturnTo3D::Both),
    ] {
        let text = json!({"navigation.returnTo3D": name}).to_string();
        let settings = Settings::from_json(&text).unwrap();
        assert_eq!(settings.return_3d, expected);
        assert_eq!(settings.view_duration_ms, DEFAULT_VIEW_DURATION_MS);
    }
    for (name, expected) in [("auto", SnapMode::Auto), ("fixed", SnapMode::Fixed)] {
        let text = json!({"snapping.mode": name}).to_string();
        assert_eq!(Settings::from_json(&text).unwrap().snap_mode, expected);
    }
    for (name, expected) in [
        ("mm", DisplayUnit::Millimeters),
        ("cm", DisplayUnit::Centimeters),
        ("m", DisplayUnit::Meters),
        ("in", DisplayUnit::Inches),
        ("ft", DisplayUnit::Feet),
    ] {
        let text = json!({"display.lengthUnit": name}).to_string();
        assert_eq!(Settings::from_json(&text).unwrap().display_unit, expected);
    }
    for (name, expected) in [
        ("system", ThemeMode::System),
        ("light", ThemeMode::Light),
        ("dark", ThemeMode::Dark),
    ] {
        let text = json!({"appearance.theme": name}).to_string();
        assert_eq!(Settings::from_json(&text).unwrap().theme, expected);
    }
    assert_eq!(
        Settings::from_json(r##"{"appearance.accentColor":"#0a7BcD"}"##)
            .unwrap()
            .accent_color,
        AccentColor::new(0x0A, 0x7B, 0xCD)
    );
    for duration in [0, MAX_VIEW_DURATION_MS] {
        let text = json!({"navigation.viewDurationMs": duration}).to_string();
        assert_eq!(
            Settings::from_json(&text).unwrap().view_duration_ms,
            duration
        );
    }
    for step in [f64::from_bits(1), 0.001, f64::MAX] {
        let text = json!({"snapping.stepCm": step}).to_string();
        assert_eq!(Settings::from_json(&text).unwrap().snap_step_cm, step);
    }
}

#[test]
fn theme_resolution_uses_live_system_value_only_when_requested() {
    assert_eq!(ResolvedTheme::default(), ResolvedTheme::Light);
    assert_eq!(
        ThemeMode::System.resolve(ResolvedTheme::Light),
        ResolvedTheme::Light
    );
    assert_eq!(
        ThemeMode::System.resolve(ResolvedTheme::Dark),
        ResolvedTheme::Dark
    );
    for system in [ResolvedTheme::Light, ResolvedTheme::Dark] {
        assert_eq!(ThemeMode::Light.resolve(system), ResolvedTheme::Light);
        assert_eq!(ThemeMode::Dark.resolve(system), ResolvedTheme::Dark);
    }
}

#[test]
fn accent_color_normalizes_hex_without_losing_rgb_channels() {
    let parsed = Settings::from_json(r##"{"appearance.accentColor":"#0a7BcD"}"##).unwrap();
    assert_eq!(parsed.accent_color.rgb(), [0x0A, 0x7B, 0xCD]);
    let written: Value = serde_json::from_str(&parsed.to_json().unwrap()).unwrap();
    assert_eq!(written["appearance.accentColor"], "#0A7BCD");
    assert_eq!(
        Settings::from_json(&parsed.to_json().unwrap()).unwrap(),
        parsed
    );
}

#[test]
fn malformed_or_invalid_known_values_are_rejected_with_no_coercion() {
    for source in [
        "",
        "null",
        "[]",
        "true",
        "{} {}",
        "{\"viewport.showGrid\":true,}",
        "{//comment\n}",
        "{\"snapping.stepCm\":NaN}",
        "{\"snapping.stepCm\":1e500}",
    ] {
        assert!(Settings::from_json(source).is_err(), "{source}");
    }
    for (key, value) in [
        ("viewport.showGrid", json!(1)),
        ("viewport.showEdges", json!("false")),
        ("navigation.preciseScrollZoom", Value::Null),
        ("navigation.animateViews", json!([])),
        ("navigation.viewDurationMs", json!(-1)),
        ("navigation.viewDurationMs", json!(1.25)),
        ("navigation.viewDurationMs", json!(1001)),
        ("navigation.viewDurationMs", json!(u64::MAX)),
        ("navigation.returnTo3D", json!("Both")),
        ("snapping.enabled", json!({})),
        ("snapping.mode", json!("adaptive")),
        ("snapping.stepCm", json!(0)),
        ("snapping.stepCm", json!(-0.1)),
        ("display.lengthUnit", json!("centimeters")),
        ("appearance.theme", json!("automatic")),
        ("appearance.theme", json!(1)),
        ("appearance.accentColor", json!("blue")),
        ("appearance.accentColor", json!("#1234")),
        ("appearance.accentColor", json!("#12345678")),
        ("appearance.accentColor", json!("#12GG56")),
        ("appearance.accentColor", Value::Null),
    ] {
        let error = Settings::from_json(&json!({key: value}).to_string()).unwrap_err();
        assert!(error.contains(key), "{key}: {error}");
    }
}

#[test]
fn duplicates_are_rejected_in_known_unknown_nested_and_escaped_keys() {
    for source in [
        r#"{"viewport.showGrid":true,"viewport.showGrid":false}"#,
        r#"{"future":1,"future":2}"#,
        r#"{"future":{"a":1,"a":2}}"#,
        r#"{"future":[{"a":1,"a":2}]}"#,
        r#"{"future":1,"futur\u0065":2}"#,
    ] {
        assert!(
            Settings::from_json(source)
                .unwrap_err()
                .contains("Duplicate")
        );
    }
}

#[test]
fn input_and_output_size_limits_fail_without_writes() {
    let source = format!("{{\"future\":\"{}\"}}", "x".repeat(MAX_SETTINGS_BYTES));
    let (mut control, store) = controller(Some(&source));
    assert!(
        control
            .sync(&Settings::default())
            .unwrap_err()
            .contains("1 MiB")
    );
    assert_eq!(store.writes(), 0);
    assert_eq!(store.bytes().unwrap(), source.as_bytes());

    // Valid input can fit while adding a UI setting would exceed the same budget.
    let source = format!("{{\"future\":\"{}\"}}", "x".repeat(MAX_SETTINGS_BYTES - 24));
    assert!(source.len() < MAX_SETTINGS_BYTES);
    store.replace(Some(&source));
    let mut local = control.sync(&Settings::default()).unwrap();
    local.show_grid = false;
    assert!(control.sync(&local).unwrap_err().contains("formatting"));
    assert_eq!(store.writes(), 0);
    assert_eq!(store.bytes().unwrap(), source.as_bytes());
}

#[test]
fn missing_files_stay_missing_until_edit_or_explicit_creation() {
    let (mut control, store) = controller(None);
    let defaults = Settings::default();
    assert_eq!(control.sync(&defaults).unwrap(), defaults);
    assert_eq!(control.reload().unwrap(), defaults);
    assert_eq!(store.bytes(), None);
    assert_eq!(control.ensure_file(&defaults).unwrap(), defaults);
    assert_eq!(store.json().as_object().unwrap().len(), 12);
    assert_eq!(store.writes(), 1);
    let unchanged = store.bytes();
    control.ensure_file(&defaults).unwrap();
    assert_eq!(store.bytes(), unchanged);
    assert_eq!(store.writes(), 1);

    let (mut control, store) = controller(None);
    let mut local = defaults;
    local.show_edges = false;
    assert_eq!(control.sync(&local).unwrap(), local);
    assert_eq!(store.writes(), 1);
    assert_eq!(store.json().as_object().unwrap().len(), 12);
}

#[test]
fn unchanged_files_are_not_reformatted_and_partial_omissions_survive_ui_edits() {
    let source = "{ \"viewport.showGrid\": false, \"future.flag\": [true,null] }\n";
    let (mut control, store) = controller(Some(source));
    let mut local = control.sync(&Settings::default()).unwrap();
    assert!(!local.show_grid);
    control.sync(&local).unwrap();
    control.ensure_file(&local).unwrap();
    assert_eq!(store.bytes().unwrap(), source.as_bytes());
    assert_eq!(store.writes(), 0);
    local.show_edges = false;
    control.sync(&local).unwrap();
    let json = store.json();
    assert_eq!(json.as_object().unwrap().len(), 3);
    assert_eq!(json["future.flag"], json!([true, null]));
    assert_eq!(json["viewport.showGrid"], false);
    assert_eq!(json["viewport.showEdges"], false);
}

#[test]
fn disjoint_local_and_external_edits_merge_with_latest_unknown_data() {
    let (mut control, store) = controller(Some(r#"{"future":{"version":1}}"#));
    let mut local = control.sync(&Settings::default()).unwrap();
    local.show_grid = false;
    local.snap_mode = SnapMode::Fixed;
    store.replace(Some(
        r#"{"navigation.viewDurationMs":250,"future":{"version":2},"new":null}"#,
    ));
    let effective = control.sync(&local).unwrap();
    assert!(!effective.show_grid);
    assert_eq!(effective.snap_mode, SnapMode::Fixed);
    assert_eq!(effective.view_duration_ms, 250);
    assert_eq!(store.json()["future"], json!({"version":2}));
    assert_eq!(store.json()["new"], Value::Null);
    assert_eq!(store.writes(), 1);
    assert_eq!(control.sync(&effective).unwrap(), effective);
    assert_eq!(store.writes(), 1);
}

#[test]
fn theme_and_accent_persist_merge_and_reset_without_touching_unknown_keys() {
    let (mut control, store) = controller(Some(
        r#"{"appearance.theme":"dark","future.panel":{"layout":"compact"}}"#,
    ));
    let mut local = control.reload().unwrap();
    assert_eq!(local.theme, ThemeMode::Dark);
    local.accent_color = AccentColor::new(0xA1, 0xB2, 0xC3);
    // An external theme edit is disjoint from the local accent edit.
    store.replace(Some(
        r#"{"appearance.theme":"light","future.panel":{"layout":"compact"}}"#,
    ));
    let merged = control.sync(&local).unwrap();
    assert_eq!(merged.theme, ThemeMode::Light);
    assert_eq!(merged.accent_color, AccentColor::new(0xA1, 0xB2, 0xC3));
    assert_eq!(store.json()["future.panel"], json!({"layout":"compact"}));
    assert_eq!(store.json()["appearance.accentColor"], "#A1B2C3");

    let mut restarted = SettingsController::new(store.clone(), Settings::default()).unwrap();
    assert_eq!(restarted.reload().unwrap(), merged);

    let mut reset = merged.clone();
    reset.accent_color = AccentColor::DEFAULT;
    let reset = control.sync(&reset).unwrap();
    assert_eq!(reset.accent_color, AccentColor::DEFAULT);
    assert_eq!(store.json()["appearance.accentColor"], "#2563EB");
    assert_eq!(restarted.reload().unwrap(), reset);

    let mut local_conflict = reset;
    local_conflict.accent_color = AccentColor::new(0x11, 0x22, 0x33);
    store.replace(Some(
        r##"{"appearance.theme":"light","appearance.accentColor":"#445566"}"##,
    ));
    assert!(
        control
            .sync(&local_conflict)
            .unwrap_err()
            .contains("appearance.accentColor")
    );
    assert_eq!(store.json()["appearance.accentColor"], "#445566");
}

#[test]
fn unknown_json_numbers_and_nested_values_survive_writes_without_rounding() {
    let source = r#"{
        "future.long": 1234567890123456789012345678901234567890,
        "future.huge": 1e500,
        "future.nested": {"samples":[1e-9999, -0.000, null],"label":"\u0061"}
    }"#;
    let (mut control, store) = controller(Some(source));
    let mut local = control.sync(&Settings::default()).unwrap();
    local.show_grid = false;
    control.sync(&local).unwrap();
    let bytes = store.bytes().unwrap();
    let written = std::str::from_utf8(&bytes).unwrap();
    assert!(written.contains("1234567890123456789012345678901234567890"));
    assert!(written.contains("1e500"));
    assert!(written.contains(r#"{"samples":[1e-9999, -0.000, null],"label":"\u0061"}"#));
    assert_eq!(Settings::from_json(written).unwrap(), local);
}

#[test]
fn invalid_utf8_and_excessive_nesting_are_not_loaded_or_overwritten() {
    let (mut control, store) = controller(None);
    store.0.borrow_mut().bytes = Some(vec![b'{', 0xff, b'}']);
    assert!(control.reload().is_err());
    let source = format!("{{\"future\":{}0{}}}", "[".repeat(140), "]".repeat(140));
    store.replace(Some(&source));
    assert!(control.reload().unwrap_err().contains("nesting limit"));
    assert!(control.ensure_file(&Settings::default()).is_err());
    assert_eq!(store.bytes().unwrap(), source.as_bytes());
    assert_eq!(store.writes(), 0);
}

#[test]
fn two_controllers_merge_disjoint_changes_and_conflict_only_on_divergent_values() {
    let (mut first, store) = controller(None);
    let mut second = SettingsController::new(store.clone(), Settings::default()).unwrap();
    let mut one = first.sync(&Settings::default()).unwrap();
    let mut two = second.sync(&Settings::default()).unwrap();
    one.view_duration_ms = 200;
    first.sync(&one).unwrap();
    two.show_grid = false;
    two = second.sync(&two).unwrap();
    assert_eq!(two.view_duration_ms, 200);
    one = first.sync(&one).unwrap();
    assert!(!one.show_grid);
    one.view_duration_ms = 400;
    two.view_duration_ms = 600;
    first.sync(&one).unwrap();
    assert!(
        second
            .sync(&two)
            .unwrap_err()
            .contains("navigation.viewDurationMs")
    );
    let resolved = second.reload().unwrap();
    assert_eq!(resolved.view_duration_ms, 400);
    assert!(!resolved.show_grid);
}

#[test]
fn same_key_conflicts_keep_the_entire_baseline_and_recover_without_losing_pending_edits() {
    let (mut control, store) = controller(None);
    let mut local = control.sync(&Settings::default()).unwrap();
    local.view_duration_ms = 250;
    local.show_grid = false;
    let external = r#"{"navigation.viewDurationMs":400,"viewport.showEdges":false}"#;
    store.replace(Some(external));
    assert!(
        control
            .sync(&local)
            .unwrap_err()
            .contains("navigation.viewDurationMs")
    );
    assert_eq!(store.bytes().unwrap(), external.as_bytes());
    assert_eq!(store.writes(), 0);
    // Remove just the conflicting file edit. Both pending local values still apply.
    store.replace(Some(r#"{"viewport.showEdges":false}"#));
    let effective = control.sync(&local).unwrap();
    assert_eq!(effective.view_duration_ms, 250);
    assert!(!effective.show_grid && !effective.show_edges);
    assert_eq!(store.writes(), 1);
}

#[test]
fn identical_concurrent_edits_converge_without_rewrite() {
    let (mut control, store) = controller(None);
    let local = Settings {
        view_duration_ms: 300,
        ..Settings::default()
    };
    let external = "{\"navigation.viewDurationMs\":300}";
    store.replace(Some(external));
    assert_eq!(control.sync(&local).unwrap(), local);
    assert_eq!(store.bytes().unwrap(), external.as_bytes());
    assert_eq!(store.writes(), 0);
}

#[test]
fn equivalent_accent_hex_casing_converges_without_a_false_conflict() {
    let (mut control, store) = controller(None);
    control.reload().unwrap();
    let local = Settings {
        accent_color: AccentColor::new(0xAB, 0xCD, 0xEF),
        ..Settings::default()
    };
    let external = r##"{"appearance.accentColor":"#abcdef"}"##;
    store.replace(Some(external));
    assert_eq!(control.sync(&local).unwrap(), local);
    assert_eq!(store.bytes().unwrap(), external.as_bytes());
    assert_eq!(store.writes(), 0);
}

#[test]
fn external_key_removal_and_file_deletion_restore_defaults_without_recreating_file() {
    let (mut control, store) = controller(Some(
        r#"{"viewport.showGrid":false,"navigation.viewDurationMs":300}"#,
    ));
    let local = control.sync(&Settings::default()).unwrap();
    store.replace(Some(r#"{"viewport.showGrid":false}"#));
    let local = control.sync(&local).unwrap();
    assert_eq!(local.view_duration_ms, DEFAULT_VIEW_DURATION_MS);
    assert!(!local.show_grid);
    store.replace(None);
    assert_eq!(control.sync(&local).unwrap(), Settings::default());
    assert_eq!(store.bytes(), None);
    assert_eq!(store.writes(), 0);
}

#[test]
fn removal_can_conflict_with_a_pending_same_key_edit() {
    let (mut control, store) = controller(Some(r#"{"navigation.viewDurationMs":300}"#));
    let mut local = control.sync(&Settings::default()).unwrap();
    local.view_duration_ms = 500;
    store.replace(None);
    assert!(
        control
            .ensure_file(&local)
            .unwrap_err()
            .contains("Settings conflict")
    );
    assert_eq!(store.bytes(), None);
    assert_eq!(store.writes(), 0);
    let effective = control.reload().unwrap();
    assert_eq!(effective, Settings::default());
    control.ensure_file(&effective).unwrap();
    assert_eq!(store.writes(), 1);
}

#[test]
fn reload_explicitly_adopts_file_after_conflict_and_uses_custom_defaults() {
    let defaults = Settings {
        show_grid: false,
        ..Settings::default()
    };
    let store = MemoryStore::default();
    let mut control = SettingsController::new(store.clone(), defaults.clone()).unwrap();
    let mut local = control.reload().unwrap();
    assert_eq!(local, defaults);
    local.view_duration_ms = 300;
    store.replace(Some(r#"{"navigation.viewDurationMs":600}"#));
    assert!(control.sync(&local).is_err());
    let file = control.reload().unwrap();
    assert_eq!(file.view_duration_ms, 600);
    assert!(!file.show_grid);
    assert_eq!(control.sync(&file).unwrap(), file);
    assert_eq!(store.writes(), 0);
}

#[test]
fn malformed_files_never_overwrite_or_advance_baseline_and_can_be_repaired() {
    let (mut control, store) = controller(None);
    let local = Settings {
        show_grid: false,
        ..Settings::default()
    };
    for malformed in [
        "{",
        r#"{"navigation.viewDurationMs":5000}"#,
        r#"{"appearance.theme":"automatic"}"#,
        r##"{"appearance.accentColor":"#GG0000"}"##,
        r#"{"a":1,"a":2}"#,
    ] {
        store.replace(Some(malformed));
        assert!(control.sync(&local).is_err());
        assert!(control.reload().is_err());
        assert!(control.ensure_file(&local).is_err());
        assert_eq!(store.bytes().unwrap(), malformed.as_bytes());
        assert_eq!(store.writes(), 0);
    }
    store.replace(Some(r#"{"display.lengthUnit":"mm"}"#));
    let effective = control.sync(&local).unwrap();
    assert!(!effective.show_grid);
    assert_eq!(effective.display_unit, DisplayUnit::Millimeters);
    assert_eq!(store.writes(), 1);
}

#[test]
fn io_failure_and_cas_race_preserve_pending_edits_for_retry() {
    let (mut control, store) = controller(None);
    let local = Settings {
        show_grid: false,
        ..Settings::default()
    };
    store.0.borrow_mut().read_error = true;
    assert!(control.sync(&local).unwrap_err().contains("read failure"));
    store.0.borrow_mut().read_error = false;
    store.0.borrow_mut().write_error = true;
    assert!(control.sync(&local).unwrap_err().contains("write failure"));
    assert_eq!(store.bytes(), None);
    store.0.borrow_mut().write_error = false;
    let raced = br#"{"viewport.showEdges":false}"#.to_vec();
    store.0.borrow_mut().race = Some(Some(raced.clone()));
    assert!(
        control
            .sync(&local)
            .unwrap_err()
            .contains("before replacement")
    );
    assert_eq!(store.bytes().unwrap(), raced);
    assert_eq!(store.writes(), 0);
    let effective = control.sync(&local).unwrap();
    assert!(!effective.show_grid && !effective.show_edges);
    assert_eq!(store.writes(), 1);
}

#[test]
fn invalid_local_values_and_defaults_cannot_be_serialized_or_written() {
    for step in [0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let invalid = Settings {
            snap_step_cm: step,
            ..Settings::default()
        };
        assert!(invalid.to_json().is_err());
        let store = MemoryStore::default();
        assert!(SettingsController::new(store.clone(), invalid.clone()).is_err());
        let mut control = SettingsController::new(store.clone(), Settings::default()).unwrap();
        assert!(control.sync(&invalid).is_err());
        assert_eq!(store.writes(), 0);
    }
    let invalid = Settings {
        view_duration_ms: 1001,
        ..Settings::default()
    };
    assert!(invalid.to_json().is_err());
}
