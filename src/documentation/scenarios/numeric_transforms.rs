use super::{Result, Session, pointer};
use crate::{
    controls::Control,
    document::{Document, Geometry, PrimitiveKind},
    editor::Editor,
};
use glam::{DQuat, DVec3};
use std::time::Duration;

fn key_event(key: egui::Key, pressed: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn tap(s: &mut Session<'_>, key: egui::Key) -> Result<()> {
    s.key(key, true, egui::Modifiers::NONE)?;
    s.key(key, false, egui::Modifiers::NONE)?;
    Ok(())
}

fn text_key(character: char) -> Result<egui::Key> {
    Ok(match character {
        '0' => egui::Key::Num0,
        '1' => egui::Key::Num1,
        '2' => egui::Key::Num2,
        '3' => egui::Key::Num3,
        '4' => egui::Key::Num4,
        '5' => egui::Key::Num5,
        '6' => egui::Key::Num6,
        '7' => egui::Key::Num7,
        '8' => egui::Key::Num8,
        '9' => egui::Key::Num9,
        '-' => egui::Key::Minus,
        '.' => egui::Key::Period,
        _ => return Err("The numeric tutorial uses only decimal input".into()),
    })
}

fn type_number(s: &mut Session<'_>, text: &str) -> Result<()> {
    for character in text.chars() {
        let key = text_key(character)?;
        // Native input can carry both representations of one keystroke. Drive
        // the real resolver with both; it must not append the character twice.
        s.frame(
            vec![
                key_event(key, true),
                egui::Event::Text(character.to_string()),
            ],
            Duration::ZERO,
        )?;
        s.frame(vec![key_event(key, false)], Duration::ZERO)?;
        s.settle()?;
    }
    Ok(())
}

fn rotation_matches(s: &Session<'_>, expected: DQuat) -> bool {
    let actual = DQuat::from_array(s.state.editor.document.objects[0].transform.rotation);
    actual.dot(expected).abs() > 1.0 - 1e-10
}

fn double_click(s: &mut Session<'_>) -> Result<()> {
    let point = s.state.viewport_ui_rect.left_bottom() + egui::vec2(35.0, -85.0);
    s.frame(
        vec![egui::Event::PointerMoved(point)],
        Duration::from_millis(500),
    )?;
    s.frame(vec![pointer(point, true)], Duration::ZERO)?;
    s.frame(vec![pointer(point, false)], Duration::from_millis(20))?;
    s.frame(Vec::new(), Duration::from_millis(80))?;
    s.frame(vec![pointer(point, true)], Duration::ZERO)?;
    s.frame(vec![pointer(point, false)], Duration::from_millis(20))?;
    s.settle()
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube)?;
    document.objects[0].name = "Numeric example".into();
    let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
        return Err("The numeric example starts with a live primitive".into());
    };
    primitive.size = [8.0, 4.0, 3.0];
    s.state.editor = Editor::new(document.clone())?;
    s.settle()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(Control::ViewFront)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    let row = s
        .state
        .layer_row_rect(id)
        .ok_or("The numeric example has no Layers row")?;
    s.click_at(row.center())?;
    s.click_at(s.state.viewport.center())?;
    s.require(
        s.state.editor.selected_object == Some(id),
        "The numeric example starts with one live primitive selected",
    )?;

    // Keep the real tutorial pointer away from the centered numeric display.
    // Expire the selection click cue before typing so the amount and subsequent
    // key cues remain readable without hiding or altering any input evidence.
    let resting_pointer = s.state.viewport.right_bottom() + egui::vec2(-40.0, -110.0);
    s.frame(
        vec![egui::Event::PointerMoved(resting_pointer)],
        crate::doc_input::CUE_LIFETIME,
    )?;
    s.shortcut("tool.rotate")?;
    // Preserve event order even when the axis key and both characters arrive
    // in the same native input batch. View-number shortcuts must not run.
    // Compare at a fixed aspect: inspector layout may reflow during the
    // tutorial, but number entry must not change the camera itself.
    let camera = s.state.camera.view_projection(1.0);
    s.frame(
        vec![
            s.shortcut_event("transform.axis-z", true)?,
            key_event(egui::Key::Num9, true),
            egui::Event::Text("9".into()),
            key_event(egui::Key::Num0, true),
            egui::Event::Text("0".into()),
        ],
        Duration::ZERO,
    )?;
    s.frame(
        vec![
            s.shortcut_event("transform.axis-z", false)?,
            key_event(egui::Key::Num9, false),
            key_event(egui::Key::Num0, false),
        ],
        Duration::ZERO,
    )?;
    s.settle()?;
    let expected_rotation = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
    s.require(
        rotation_matches(s, expected_rotation)
            && s.state.editor.numeric_text() == Some("90")
            && s.state.editor.document.objects[0].geometry == document.objects[0].geometry
            && s.state.camera.view_projection(1.0) == camera
            && s.state.editor.has_transform_session(),
        "Rotate, the Z-axis lock, and paired native key/text events for 9 and 0 preview exactly ninety degrees around Z without changing the view or primitive parameters",
    )?;
    s.witness(Control::ToolRotate)?;
    s.witness(Control::MoveAxisLock)?;
    s.witness(Control::TransformValue)?;
    s.witness(Control::RotationZ)?;
    s.capture_tutorial("numeric-rotate")?;
    tap(s, egui::Key::Backspace)?;
    s.require(rotation_matches(s, DQuat::from_rotation_z(9.0_f64.to_radians())), "Backspace changes the same rotation preview from ninety to nine degrees without deleting the object")?;
    type_number(s, "0")?;
    let rotated = s.state.editor.document.clone();
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.document == rotated
            && !s.state.editor.has_transform_session()
            && s.state.editor.transform_axis.is_none()
            && !s.state.editor.edit_mode,
        "Confirm confirms one numeric rotation without cascading into vertex edit mode",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == document,
        "One Undo restores the complete numeric rotation and its edits",
    )?;
    s.redo()?;
    s.require(
        s.state.editor.document == rotated,
        "Redo restores the exact accepted rotation",
    )?;
    s.undo()?;

    s.shortcut("tool.move")?;
    s.shortcut("transform.axis-x")?;
    type_number(s, "-0.125")?;
    let translation = s.state.editor.document.objects[0].transform.translation;
    let camera_unchanged = s.state.camera.view_projection(1.0) == camera;
    s.require(
        s.state.editor.snapping.enabled
            && translation == [-0.125, 0.0, 0.0]
            && camera_unchanged,
        &format!(
            "Signed fractional Move input bypasses grid snapping and the decimal key does not toggle the view while numeric input owns it (snapping={}, translation={translation:?}, camera_unchanged={camera_unchanged})",
            s.state.editor.snapping.enabled
        ),
    )?;
    s.witness(Control::ToolMove)?;
    s.witness(Control::PositionX)?;
    s.capture_tutorial("numeric-move")?;
    s.shortcut("cancel")?;
    s.require(
        s.state.editor.document == document && !s.state.editor.has_transform_session(),
        "Cancel discards the exact numeric movement and clears its session",
    )?;

    s.shortcut("tool.scale")?;
    s.shortcut("transform.axis-x")?;
    type_number(s, "1.5")?;
    s.require(
        s.state.editor.document.objects[0].transform.scale == [1.5, 1.0, 1.0]
            && s.state.editor.document.objects[0].geometry == document.objects[0].geometry,
        "Scale, the X-axis lock, and 1.5 scale only the chosen object axis by a factor without baking its primitive",
    )?;
    s.witness(Control::ToolScale)?;
    s.witness(Control::ScaleX)?;
    s.capture_tutorial("numeric-scale")?;
    for _ in 0..3 {
        tap(s, egui::Key::Backspace)?;
    }
    let before_invalid = s.state.editor.document.clone();
    type_number(s, "0")?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.numeric_error().is_some()
            && s.state.editor.has_transform_session()
            && s.state.editor.document == before_invalid,
        "A zero scale multiplier reports an error, keeps the last valid preview, and cannot be confirmed",
    )?;
    tap(s, egui::Key::Backspace)?;
    type_number(s, "1.5")?;
    s.require(
        s.state.editor.numeric_error().is_none()
            && s.state.editor.document.objects[0].transform.scale == [1.5, 1.0, 1.0],
        "Backspace and corrected input repair an invalid value within the same session",
    )?;
    let scaled = s.state.editor.document.clone();
    double_click(s)?;
    s.require(
        s.state.editor.document == scaled
            && !s.state.editor.has_transform_session()
            && !s.state.editor.edit_mode,
        "A viewport double-click confirms numeric scaling once without changing edit mode",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == document,
        "One Undo restores the entire numeric scale session",
    )?;

    s.shortcut("tool.move")?;
    s.shortcut("transform.axis-x")?;
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(45.0, -75.0);
    let end = start + egui::vec2(37.0, 0.0);
    s.frame(
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
        Duration::ZERO,
    )?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    let dragged = s.state.editor.document.objects[0].transform.translation[0];
    s.require(
        dragged > 0.0 && dragged != 2.0 && s.state.editor.is_pointer_interacting(),
        "The numeric replacement check starts with an uncommitted pointer movement",
    )?;
    type_number(s, "2")?;
    s.require(s.state.editor.document.objects[0].transform.translation == [2.0, 0.0, 0.0], "Typing after a held drag replaces the total amount from the original session baseline instead of adding to its preview")?;
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.require(
        s.state.editor.document.objects[0].transform.translation == [2.0, 0.0, 0.0],
        "The retired pointer release cannot overwrite the numeric preview",
    )?;
    s.shortcut("transform.axis-y")?;
    s.require(
        s.state.editor.document.objects[0].transform.translation == [0.0, 2.0, 0.0],
        "Changing numeric movement from X to Y reuses the amount against the original baseline",
    )?;
    s.shortcut("transform.axis-y")?;
    s.require(s.state.editor.document == document && s.state.editor.has_transform_session(), "Toggling the numeric axis off restores the baseline while leaving the session explicitly finishable")?;
    s.shortcut("cancel")?;
    s.require(
        s.state.editor.document == document && !s.state.editor.has_transform_session(),
        "Cancelling an already-restored numeric session adds no geometry change",
    )?;
    s.require(
        DVec3::from_array(s.state.editor.document.objects[0].transform.scale) == DVec3::ONE,
        "All numeric tutorial previews leave the original object scale after undo or cancel",
    )?;
    Ok(())
}
