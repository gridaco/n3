use super::{Result, Session};
use crate::{
    controls::Control,
    document::{Document, Geometry, PrimitiveKind},
    document_io,
    units::LengthUnit,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(document: &Document) -> Result<Self> {
        let directory = std::env::temp_dir().join(format!(
            "n3-length-units-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let path = directory.join("Length example.n3.json");
        document_io::save(&path, document, None, false)?;
        Ok(Self(path))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(directory) = self.0.parent() {
            let _ = std::fs::remove_dir(directory);
        }
    }
}

fn choose(s: &mut Session<'_>, control: Control, unit: LengthUnit) -> Result<()> {
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click(Control::LengthUnitMenu)?;
    s.require(
        s.trace.get(control)?.parents == [Control::PreferencesWindow, Control::LengthUnitMenu],
        "Display units are actual options in Preferences",
    )?;
    s.click(control)?;
    s.close_preferences()?;
    s.require(
        s.state.display_unit == unit && !s.state.show_preferences,
        &format!(
            "The chosen display unit becomes active and remains after closing Preferences (requested={unit:?}, actual={:?}, open={})",
            s.state.display_unit,
            s.state.show_preferences,
        ),
    )
}

fn width(s: &Session<'_>) -> Result<f64> {
    match &s.state.editor.document.objects[0].geometry {
        Geometry::Primitive(primitive) => Ok(primitive.size[0]),
        _ => Err("The length example must remain a live primitive".into()),
    }
}

fn type_width(s: &mut Session<'_>, text: &str) -> Result<()> {
    s.click(Control::PrimitiveX)?;
    s.shortcut("selection.all")?;
    s.frame(vec![egui::Event::Text(text.into())], Duration::ZERO)?;
    s.shortcut("edit.confirm")?;
    Ok(())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube)?;
    let object = &mut document.objects[0];
    object.name = "Thin panel".into();
    let Geometry::Primitive(primitive) = &mut object.geometry else {
        return Err("The inserted fixture is not a primitive".into());
    };
    primitive.size = [0.1, 180.0, 60.0];
    let scratch = Scratch::new(&document)?;
    s.state
        .install_document(scratch.0.clone(), document.clone())?;
    s.settle()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let row = s
        .state
        .layer_row_rect(id)
        .ok_or("The panel has no Layers row")?;
    s.click_at(row.center())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(Control::ViewRight)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.witness(Control::Inspector)?;
    s.require(
        s.trace.get(Control::PrimitiveX).is_ok(),
        "The selected primitive has a Width field",
    )?;
    s.witness(Control::PrimitiveX)?;
    s.witness(Control::PrimitiveY)?;
    s.require(
        s.state.display_unit == LengthUnit::Centimeters && !s.state.is_dirty(),
        "Opening the centimeter document starts clean with centimeters as its display unit",
    )?;
    let camera = s.state.camera.view_projection(1.0);
    let revision = s.state.editor.revision;
    let selected = s.state.editor.selected_objects.clone();
    let ruler = s
        .state
        .ruler_2d_model
        .as_ref()
        .ok_or("The Right view has no rulers")?;
    let centimeter_scale = ruler.horizontal.pixels_per_unit;
    let origin = ruler.horizontal.origin;
    let ranges: Vec<_> = ruler
        .horizontal
        .ranges
        .iter()
        .map(|span| (span.start, span.end))
        .collect();
    for (control, unit) in [
        (Control::LengthMillimeters, LengthUnit::Millimeters),
        (Control::LengthCentimeters, LengthUnit::Centimeters),
        (Control::LengthMeters, LengthUnit::Meters),
        (Control::LengthInches, LengthUnit::Inches),
        (Control::LengthFeet, LengthUnit::Feet),
    ] {
        s.click(Control::PrimitiveX)?;
        choose(s, control, unit)?;
        let ruler = s
            .state
            .ruler_2d_model
            .as_ref()
            .ok_or("Changing units hid the rulers")?;
        let ruler_scale = ruler.horizontal.pixels_per_unit;
        let ruler_origin = ruler.horizontal.origin;
        let ruler_ranges: Vec<_> = ruler
            .horizontal
            .ranges
            .iter()
            .map(|span| (span.start, span.end))
            .collect();
        s.require(
            s.state.editor.document == document
                && s.state.editor.revision == revision
                && !s.state.is_dirty()
                && s.state.editor.selected_objects == selected
                && s.state.camera.view_projection(1.0) == camera
                && ruler.unit == unit
                && (ruler_scale - centimeter_scale * unit.to_centimeters(1.0)).abs() < 1e-6
                && ruler_origin == origin
                && ruler_ranges == ranges,
            &format!("Display-unit changes rescale ruler values without changing geometry, dirty state, selection, camera, ruler origin or selection extents (unit={unit:?}, document={}, revision={}/{revision}, dirty={}, selection={}, camera={}, ruler_scale={ruler_scale}/{}, ruler_origin={ruler_origin:?}/{origin:?}, ruler_ranges={ruler_ranges:?}/{ranges:?})",
                s.state.editor.document == document,
                s.state.editor.revision,
                s.state.is_dirty(),
                s.state.editor.selected_objects == selected,
                s.state.camera.view_projection(1.0) == camera,
                centimeter_scale * unit.to_centimeters(1.0)),
        )?;
        if unit == LengthUnit::Millimeters {
            s.require(
                unit.from_centimeters(0.1) == 1.0,
                "A 0.1 cm width is displayed as 1 mm",
            )?;
            s.hover(Control::PrimitiveX)?;
            s.capture_tutorial("length-units-millimeters")?;
        }
        if unit == LengthUnit::Meters {
            s.require(
                unit.from_centimeters(180.0) == 1.8,
                "A 180 cm height is displayed as 1.8 m",
            )?;
            s.hover(Control::PrimitiveY)?;
            s.capture_tutorial("length-units-meters")?;
        }
    }
    // Focusing and switching units must not introduce a history entry.
    s.undo()?;
    s.require(
        s.state.editor.document == document && s.state.editor.revision == revision && !s.state.is_dirty(),
        "Switching units from a focused length field preserves exact geometry and revision without an undo entry",
    )?;

    choose(s, Control::LengthMeters, LengthUnit::Meters)?;
    type_width(s, "1 ft")?;
    s.require(
        (width(s)? - 30.48).abs() < 1e-10 && s.state.is_dirty(),
        "An explicit feet suffix previews immediately and commits exactly 30.48 canonical centimeters on Enter",
    )?;
    let changed = s.state.editor.document.clone();
    s.undo()?;
    choose(s, Control::LengthMillimeters, LengthUnit::Millimeters)?;
    choose(s, Control::LengthMeters, LengthUnit::Meters)?;
    s.redo()?;
    s.require(
        s.state.editor.document == changed,
        "Display-unit changes preserve the existing redo branch",
    )?;
    s.undo()?;
    type_width(s, "1.8")?;
    s.require(
        width(s)? == 180.0,
        "An unsuffixed 1.8 input uses the active meters display and stores 180 centimeters",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == document && !s.state.is_dirty(),
        "Each completed length edit is one undo step; display-only changes add none",
    )?;

    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(18.0, -18.0);
    s.click_at(empty)?;
    s.shortcut("selection.all")?;
    s.shortcut("tool.move")?;
    s.shortcut("nudge.up")?;
    s.shortcut("nudge.fast-up")?;
    s.require(
        s.state.display_unit == LengthUnit::Meters
            && s.state.editor.document.objects[0].transform.translation == [0.0, 11.0, 0.0],
        "Arrow movement remains 1 cm or 10 cm even when Properties and rulers display meters",
    )?;
    s.shortcut("history.undo")?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document && !s.state.is_dirty(),
        "Undo restores the canonical centimeter positions",
    )?;

    let mut json: serde_json::Value =
        serde_json::from_str(&document.to_json()?).map_err(|e| e.to_string())?;
    s.require(
        json["version"] == 1 && json["length_unit"] == "cm",
        "Native version-1 JSON explicitly declares canonical centimeters",
    )?;
    json.as_object_mut().unwrap().remove("length_unit");
    s.require(
        Document::from_json(&json.to_string())? == document,
        "Legacy version-1 JSON defaults to centimeters without changing coordinates",
    )?;
    json["length_unit"] = "m".into();
    s.require(
        Document::from_json(&json.to_string()).is_err(),
        "A different canonical JSON length unit is rejected instead of silently rescaling geometry",
    )?;

    choose(s, Control::LengthFeet, LengthUnit::Feet)?;
    s.state.install_document(scratch.0.clone(), document)?;
    s.settle()?;
    s.require(
        s.state.display_unit == LengthUnit::Feet,
        "Opening a document preserves the user's display-unit preference",
    )?;
    choose(s, Control::LengthFeet, LengthUnit::Feet)?;
    s.click_path(&[Control::N3Menu, Control::FileMenu, Control::New])?;
    s.require(
        s.state.editor.document.objects.is_empty() && s.state.display_unit == LengthUnit::Feet,
        "New preserves the display-unit preference while geometry remains canonical centimeters",
    )?;
    Ok(())
}
