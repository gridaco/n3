use super::artifacts::*;
use super::*;
#[test]
fn generated_documentation_is_current() {
    run("check").unwrap();
}
#[test]
fn guide_session_starts_from_stable_light_layout_pixels() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    let expected = SavedLayout::DOCUMENTATION;
    let layout = s.state.save_layout(&s.ctx);
    assert_eq!(s.ctx.theme(), egui::Theme::Light);
    assert!(s.state.show_ui);
    assert!((layout.hierarchy_width - expected.hierarchy_width).abs() <= 1.0);
    assert!((layout.inspector_width - expected.inspector_width).abs() <= 1.0);
    assert!(s.time > 0.0, "startup fade uses explicit scenario time");

    let initial = s.capture.read_frame().unwrap();
    assert_eq!((initial.width, initial.height), (WIDTH, HEIGHT));
    let time_before_settle = s.time;
    s.settle().unwrap();
    assert_eq!(s.time, time_before_settle, "layout settle is time-free");
    s.frame(Vec::new(), Duration::from_millis(500)).unwrap();
    s.settle().unwrap();
    let later = s.capture.read_frame().unwrap();
    assert!(
        initial.rgba == later.rgba,
        "idle guide pixels changed after startup; an egui overlay may still be fading in"
    );
}
#[test]
fn gizmo_context_menu_and_preferences_own_their_input() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    s.click(Control::AxisX).unwrap();
    s.frame(vec![], Duration::from_millis(30)).unwrap();
    let pose = s.state.camera.view_projection(s.state.aspect());
    s.right_click(Control::Gizmo).unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    assert!(
        s.state.camera.is_transitioning(),
        "Right-click must not interrupt the transition"
    );
    assert_eq!(pose, s.state.camera.view_projection(s.state.aspect()));
    let outside = s.state.viewport.center();
    assert!(
        !crate::navigation_events::viewport_accepts_pointer(&s.ctx, s.state.viewport, outside),
        "Popup dismissal must not also start native camera input"
    );
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert!(!s.state.show_preferences);
    assert_eq!(pose, s.state.camera.view_projection(s.state.aspect()));
    s.frame(
        vec![],
        Duration::from_millis(s.state.view_duration_ms.into()),
    )
    .unwrap();

    // Axis handles and the body share the same context-menu response.
    s.right_click(Control::AxisX).unwrap();
    s.click(Control::GizmoPreferences).unwrap();
    s.frame(vec![], crate::doc_input::CUE_LIFETIME).unwrap();
    s.settle().unwrap();
    assert!(s.state.show_preferences);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    let window = s.trace.get(Control::PreferencesWindow).unwrap().rect;
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect.center();
    for (name, point) in [
        ("window center", window.center()),
        ("window titlebar", window.min + egui::vec2(40., 10.)),
        ("duration slider", slider),
    ] {
        assert!(
            !crate::navigation_events::viewport_accepts_pointer(&s.ctx, s.state.viewport, point),
            "Preferences must block native camera input before a press/scroll: {name} at {point:?}"
        );
    }
    let uncovered = egui::pos2(
        s.state.viewport.right() - 180.,
        s.state.viewport.bottom() - 40.,
    );
    assert!(
        crate::navigation_events::viewport_accepts_pointer(&s.ctx, s.state.viewport, uncovered),
        "The nonmodal preferences window must allow navigation elsewhere"
    );
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.frame(vec![], crate::doc_input::CUE_LIFETIME).unwrap();
    s.settle().unwrap();
    assert!(
        crate::navigation_events::viewport_accepts_pointer(&s.ctx, s.state.viewport, slider),
        "Closing Preferences restores the underlying viewport"
    );

    s.right_click(Control::Gizmo).unwrap();
    let pose = s.state.camera.view_projection(s.state.aspect());
    s.click_at(uncovered).unwrap();
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(pose, s.state.camera.view_projection(s.state.aspect()));
    let start = s.trace.get(Control::Gizmo).unwrap().rect.center();
    let end = start + egui::vec2(-80., 40.);
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)
        .unwrap();
    s.frame(
        vec![egui::Event::PointerButton {
            pos: start,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    s.frame(
        vec![egui::Event::PointerButton {
            pos: end,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        Duration::ZERO,
    )
    .unwrap();
    s.settle().unwrap();
    assert!(
        !egui::Popup::is_any_open(&s.ctx),
        "Secondary dragging is not a context-menu click"
    );
    assert_eq!(pose, s.state.camera.view_projection(s.state.aspect()));
}
#[test]
fn tutorial_overlay_preserves_interaction_and_capture_state() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    s.hover(Control::AxisX).unwrap();
    assert_eq!(s.cursor, egui::CursorIcon::PointingHand);
    let geometry = |trace: &Trace| {
        trace
            .controls
            .iter()
            .map(|(id, item)| (*id, (item.rect, item.enabled)))
            .collect::<BTreeMap<_, _>>()
    };
    let before = geometry(&s.trace);
    let matrix = s.state.camera.view_projection(s.state.aspect());
    let focus = s.ctx.memory(|memory| memory.focused());
    let hovered = s.ctx.pointer_hover_pos();
    s.show_inputs = false;
    s.frame(vec![], Duration::ZERO).unwrap();
    let plain = s.capture.webp().unwrap();
    s.show_inputs = true;
    s.frame(vec![], Duration::ZERO).unwrap();
    assert_ne!(
        plain,
        s.capture.webp().unwrap(),
        "The overlay must actually be rendered"
    );
    assert_eq!(before, geometry(&s.trace));
    assert_eq!(matrix, s.state.camera.view_projection(s.state.aspect()));
    assert_eq!(focus, s.ctx.memory(|memory| memory.focused()));
    assert_eq!(hovered, s.ctx.pointer_hover_pos());
    assert_eq!(s.cursor, egui::CursorIcon::PointingHand);
    let time = s.time;
    let evidence = s.input.summary(s.cursor);
    s.capture_tutorial("hover-proof").unwrap();
    s.capture_tutorial("hover-replay").unwrap();
    assert_eq!(
        s.images["assets/hover-proof.webp"],
        s.images["assets/hover-replay.webp"]
    );
    assert_eq!(time, s.time);
    assert_eq!(evidence, s.input.summary(s.cursor));
    assert_eq!(hovered, s.ctx.pointer_hover_pos());
    s.click(Control::AxisX).unwrap();
    s.frame(
        vec![],
        Duration::from_millis(s.state.view_duration_ms.into()),
    )
    .unwrap();
    assert!(
        s.state.camera.direction_in_view(glam::Vec3::X).z > 0.9999,
        "The overlay cannot intercept the click it illustrates"
    );
}

#[test]
fn virtual_keys_obey_focus_repeat_and_layout_retry_rules() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let modifiers = egui::Modifiers::NONE;
    s.extra_layout_pass = true;
    s.number_key(NumberKey::TopRow(0), true, modifiers).unwrap();
    assert!(
        s.state.camera.is_orthographic(),
        "A layout retry must neither lose nor double a key"
    );
    assert!(s.input.key_down(egui::Key::Num0));
    s.number_key(NumberKey::TopRow(0), true, modifiers).unwrap();
    assert!(
        s.state.camera.is_orthographic(),
        "A held/repeated top-row 0 must not toggle twice"
    );
    let time = s.time;
    s.capture_tutorial("held-key-proof").unwrap();
    assert!(s.input.key_down(egui::Key::Num0));
    assert_eq!(time, s.time);
    s.number_key(NumberKey::TopRow(0), false, modifiers)
        .unwrap();
    s.number_key(NumberKey::TopRow(0), true, modifiers).unwrap();
    assert!(!s.state.camera.is_orthographic());
    s.number_key(NumberKey::TopRow(0), false, modifiers)
        .unwrap();

    s.click_path(&[Control::N3Menu, Control::ViewMenu]).unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    s.number_key(NumberKey::TopRow(0), true, modifiers).unwrap();
    assert!(
        !s.state.camera.is_orthographic(),
        "An open menu owns navigation keys"
    );
    s.number_key(NumberKey::TopRow(0), false, modifiers)
        .unwrap();
    s.click_path(&[Control::N3Menu, Control::ViewMenu]).unwrap();
    s.state
        .editor
        .commit("Focus test edit", |doc| {
            doc.objects[0].transform.translation[0] += 0.25;
            Ok(())
        })
        .unwrap();
    let edited_document = s.state.editor.document.clone();
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    // The standard egui slider places its numeric editor after the track.
    // Derive that location from the live widget and the active UI spacing.
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    assert!(
        s.ctx.egui_wants_keyboard_input(),
        "Exercise the slider's real numeric editor focus"
    );
    s.number_key(NumberKey::TopRow(0), true, modifiers).unwrap();
    assert!(
        !s.state.camera.is_orthographic(),
        "Focused editing owns navigation keys"
    );
    s.number_key(NumberKey::TopRow(0), false, modifiers)
        .unwrap();

    let command = egui::Modifiers {
        mac_cmd: true,
        command: true,
        ..modifiers
    };
    s.key(egui::Key::Z, true, command).unwrap();
    assert_eq!(
        s.state.editor.document, edited_document,
        "Text-field undo must not undo the document"
    );
    s.key(egui::Key::Z, false, command).unwrap();

    let command_shift = egui::Modifiers {
        mac_cmd: true,
        command: true,
        shift: true,
        ..modifiers
    };
    assert!(
        s.key(egui::Key::O, true, command_shift).unwrap(),
        "Command-O reaches the same Open effect boundary"
    );
    s.settle().unwrap();
    assert_eq!(s.input.modifiers(), command_shift);
    assert!(s.input.key_down(egui::Key::O));
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    assert!(!s.input.key_down(egui::Key::O));
    assert_eq!(s.input.modifiers(), modifiers);
}
#[test]
fn selection_keys_follow_real_viewport_field_and_popup_focus() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    s.state
        .editor
        .insert(document::PrimitiveKind::Cube)
        .unwrap();
    s.state.frame_all();
    s.settle().unwrap();
    let original = s.state.editor.document.clone();
    let first = original.objects[0].id;
    let second = original.objects[1].id;
    let viewport_id = crate::shortcuts::viewport_focus_id();
    let key = |key, pressed, modifiers| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers,
    };
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(18., -18.);
    s.frame(
        vec![egui::Event::PointerMoved(empty), pointer(empty, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(vec![pointer(empty, false)], Duration::ZERO)
        .unwrap();
    assert!(s.state.editor.selected_objects.is_empty());
    assert_eq!(s.ctx.memory(|memory| memory.focused()), Some(viewport_id));
    // No settling frame between the real click and the first Tab.
    s.extra_layout_pass = true;
    s.frame(
        vec![key(egui::Key::Tab, true, egui::Modifiers::NONE)],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.selected_object, Some(first));
    assert_eq!(s.ctx.memory(|memory| memory.focused()), Some(viewport_id));
    s.key(egui::Key::Tab, true, egui::Modifiers::NONE).unwrap();
    assert_eq!(
        s.state.editor.selected_object,
        Some(first),
        "Held Tab cannot advance again"
    );
    s.key(egui::Key::Tab, false, egui::Modifiers::NONE).unwrap();
    s.key(egui::Key::Tab, true, egui::Modifiers::NONE).unwrap();
    assert_eq!(s.state.editor.selected_object, Some(second));
    s.key(egui::Key::Tab, false, egui::Modifiers::NONE).unwrap();
    assert_eq!(s.state.editor.document, original);

    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    let field_id = s.ctx.memory(|memory| memory.focused());
    assert!(field_id.is_some() && field_id != Some(viewport_id));
    let command = egui::Modifiers {
        command: true,
        mac_cmd: true,
        ..egui::Modifiers::NONE
    };
    for (input, modifiers) in [
        (egui::Key::A, command),
        (egui::Key::Backspace, egui::Modifiers::NONE),
        (egui::Key::Delete, egui::Modifiers::NONE),
        (egui::Key::Tab, egui::Modifiers::NONE),
    ] {
        s.key(input, true, modifiers).unwrap();
        s.key(input, false, egui::Modifiers::NONE).unwrap();
        assert_eq!(
            s.state.editor.document, original,
            "Focused UI owns {input:?}"
        );
        assert_eq!(s.state.editor.selected_object, Some(second));
    }
    assert_ne!(
        s.ctx.memory(|memory| memory.focused()),
        field_id,
        "Tab still traverses UI controls"
    );
    assert_ne!(s.ctx.memory(|memory| memory.focused()), Some(viewport_id));
    // Merely hovering the scene cannot steal keyboard ownership.
    s.frame(vec![egui::Event::PointerMoved(empty)], Duration::ZERO)
        .unwrap();
    s.key(egui::Key::Tab, true, egui::Modifiers::NONE).unwrap();
    s.key(egui::Key::Tab, false, egui::Modifiers::NONE).unwrap();
    assert_eq!(s.state.editor.selected_object, Some(second));
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.click_at(empty).unwrap();
    s.key(egui::Key::A, true, command).unwrap();
    s.key(egui::Key::A, false, egui::Modifiers::NONE).unwrap();
    let selected = s.state.editor.selected_objects.clone();
    assert_eq!(selected.len(), 2);
    s.right_click(Control::Viewport).unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    for input in [
        egui::Key::Tab,
        egui::Key::Delete,
        egui::Key::Backspace,
        egui::Key::Escape,
    ] {
        s.key(input, true, egui::Modifiers::NONE).unwrap();
        s.key(input, false, egui::Modifiers::NONE).unwrap();
        assert_eq!(s.state.editor.document, original, "Menu owns {input:?}");
        assert_eq!(s.state.editor.selected_objects, selected);
    }
    assert!(!egui::Popup::is_any_open(&s.ctx));
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    s.key(egui::Key::Delete, true, egui::Modifiers::NONE)
        .unwrap();
    assert_eq!(s.state.editor.document, original);
    assert_eq!(s.state.editor.selected_objects, selected);
}

#[test]
fn escape_dismissal_and_key_repeat_do_not_leak_into_selection() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    let original = s.state.editor.document.clone();
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    assert!(s.ctx.egui_wants_keyboard_input());
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(!s.ctx.egui_wants_keyboard_input());
    assert!(
        s.state.show_preferences,
        "The focused field owns this Escape"
    );
    assert_eq!(s.state.editor.selected_object, Some(id));
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    s.extra_layout_pass = true;
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(!s.state.show_preferences);
    assert_eq!(
        s.state.editor.selected_object,
        Some(id),
        "Closing Preferences cannot also deselect on a layout retry"
    );
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    assert_eq!(
        s.state.editor.selected_object,
        Some(id),
        "A held Escape cannot unwind another context"
    );
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(s.state.editor.selected_object.is_none());
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn enter_toggle_respects_repeat_layout_retries_and_text_focus() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    let original = s.state.editor.document.clone();
    let original_tool = s.state.editor.tool;
    for editing in [true, false, true] {
        s.extra_layout_pass = true;
        s.key(egui::Key::Enter, true, egui::Modifiers::NONE)
            .unwrap();
        assert_eq!(s.state.editor.edit_mode, editing);
        s.key(egui::Key::Enter, true, egui::Modifiers::NONE)
            .unwrap();
        assert_eq!(
            s.state.editor.edit_mode, editing,
            "Holding Enter cannot toggle back"
        );
        s.key(egui::Key::Enter, false, egui::Modifiers::NONE)
            .unwrap();
        assert_eq!(s.state.editor.selected_object, Some(id));
        assert_eq!(s.state.editor.tool, original_tool);
        assert_eq!(s.state.editor.document, original);
    }
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    assert!(s.ctx.egui_wants_keyboard_input());
    s.key(egui::Key::Enter, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(
        s.state.editor.edit_mode,
        "Accepting a field must not also exit edit mode"
    );
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn object_feedback_clears_on_ui_ownership_and_never_changes_document_revision() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.state
        .editor
        .insert(document::PrimitiveKind::Cube)
        .unwrap();
    s.state.frame_all();
    s.state.editor.tool = crate::editor::Tool::View;
    s.settle().unwrap();
    let id = s.state.editor.selected_object.unwrap();
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let mesh_revision = s.state.mesh_revision;
    let dirty = s.state.is_dirty();
    let row = s.state.layer_row_rect(id).unwrap().center();
    s.extra_layout_pass = true;
    s.frame(vec![egui::Event::PointerMoved(row)], Duration::ZERO)
        .unwrap();
    assert_eq!(s.state.editor.hovered_object, Some(id));
    let surface = s.state.viewport.center();
    s.frame(vec![egui::Event::PointerMoved(surface)], Duration::ZERO)
        .unwrap();
    assert_eq!(s.state.editor.hovered_object, Some(id));
    s.frame(vec![egui::Event::PointerGone], Duration::ZERO)
        .unwrap();
    assert!(s.state.editor.hovered_object.is_none());
    s.frame(vec![egui::Event::PointerMoved(surface)], Duration::ZERO)
        .unwrap();
    assert_eq!(s.state.editor.hovered_object, Some(id));
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    assert!(s.state.editor.hovered_object.is_none());
    s.frame(vec![egui::Event::WindowFocused(true)], Duration::ZERO)
        .unwrap();
    s.click_path(&[Control::N3Menu, Control::ViewMenu]).unwrap();
    s.frame(vec![egui::Event::PointerMoved(surface)], Duration::ZERO)
        .unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    assert!(
        s.state.editor.hovered_object.is_none(),
        "An open popup owns hover"
    );
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let preference = s.trace.get(Control::Duration).unwrap().rect.center();
    s.frame(vec![egui::Event::PointerMoved(preference)], Duration::ZERO)
        .unwrap();
    assert!(
        s.state.editor.hovered_object.is_none(),
        "Preferences blocks hover through its window"
    );
    assert_eq!(s.state.editor.selected_object, Some(id));
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.revision, revision);
    assert_eq!(
        s.state.mesh_revision, mesh_revision,
        "Pointer feedback must not rebuild geometry"
    );
    assert_eq!(s.state.is_dirty(), dirty);
    s.state.new_document();
    s.settle().unwrap();
    assert_eq!(
        s.state.object_highlights(),
        crate::renderer::ObjectHighlights::default()
    );
    assert!(
        s.state.layer_row_rect(id).is_none(),
        "Replaced documents cannot leave stale row targets"
    );
}

#[test]
fn failed_scenario_cannot_update_any_existing_artifact() {
    let dir = std::env::temp_dir().join(format!(
        "n3-docs-failure-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("gizmo.md"), "previous verified guide").unwrap();
    let before = read_tree(&dir).unwrap();
    let result = execute("update", &dir, || {
        Err("Scenario assertion failed: disconnected button".into())
    });
    assert!(result.unwrap_err().contains("disconnected button"));
    assert_eq!(read_tree(&dir).unwrap(), before);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn shared_mouse_routing_applies_motion_once_and_cancels_click_after_escape() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("cube-quads.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    s.settle().unwrap();
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(30.0, -30.0);
    let end = start + egui::vec2(32.0, -12.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut expected = s.state.camera.clone();
    s.frame(
        vec![egui::Event::PointerMoved(start), button(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.extra_layout_pass = true;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    expected.orbit(32.0, -12.0);
    assert!(
        s.state
            .camera
            .view_projection(s.state.aspect())
            .abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5),
        "A layout retry must not orbit twice for the same raw pointer movement"
    );
    assert!(s.state.mouse_navigation_active());
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    assert!(!s.state.mouse_navigation_active());
    assert_eq!(s.state.editor.selected_object, Some(id));
    s.frame(vec![button(end, false)], Duration::ZERO).unwrap();
    s.settle().unwrap();
    assert!(
        s.trace.get(Control::ViewportMenu).is_err(),
        "Cancelled orbit release must not open a menu"
    );
    s.frame(
        vec![egui::Event::PointerMoved(start), button(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![
            egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            },
            button(start, false),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(
        s.trace.get(Control::ViewportMenu).is_err(),
        "Escape preceding release in one input batch cancels the context click"
    );
    assert_eq!(
        s.state.editor.selected_object,
        Some(id),
        "That Escape cannot also deselect"
    );
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    s.frame(
        vec![egui::Event::PointerMoved(start), button(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    let primary = |pressed| egui::Event::PointerButton {
        pos: end,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    s.frame(vec![primary(true), button(end, false)], Duration::ZERO)
        .unwrap();
    assert!(
        !s.state.editor.is_interacting(),
        "Primary press while orbit owns the pointer cannot start a box"
    );
    s.frame(vec![primary(false)], Duration::ZERO).unwrap();
    assert_eq!(
        s.state.editor.selected_object,
        Some(id),
        "Its later release cannot become an empty-space deselection"
    );
    // A deliberate held context click is distance-based, not a short time window.
    let matrix = s.state.camera.view_projection(s.state.aspect());
    s.frame(
        vec![egui::Event::PointerMoved(start), button(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(Vec::new(), Duration::from_secs(2)).unwrap();
    s.frame(vec![button(start, false)], Duration::ZERO).unwrap();
    s.settle().unwrap();
    assert!(s.trace.get(Control::ViewportMenu).is_ok());
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), matrix);
    s.click(Control::ViewportPreferences).unwrap();
    assert!(s.state.show_preferences);
    s.reveal_preferences_control(Control::Duration).unwrap();
    let preferences = s.trace.get(Control::Duration).unwrap().rect.center();
    let before = s.state.camera.view_projection(s.state.aspect());
    s.frame(
        vec![
            egui::Event::PointerMoved(preferences),
            button(preferences, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![egui::Event::PointerMoved(
            preferences + egui::vec2(20.0, 0.0),
        )],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(
        vec![button(preferences + egui::vec2(20.0, 0.0), false)],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(
        s.state.camera.view_projection(s.state.aspect()),
        before,
        "Pointer presses on Preferences cannot orbit through the window"
    );
}
#[test]
fn view_pie_routes_hold_release_without_leaking_input() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    let original = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let key = |key, pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let anchor = s.state.viewport.center();
    s.frame(
        vec![
            egui::Event::PointerMoved(anchor),
            key(egui::Key::Backtick, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.view_pie_active());
    assert_eq!(s.state.editor.hovered_object, None);
    let top = s.trace.get(Control::PieTop).unwrap().rect.center();
    let original_pose = s.state.camera.view_projection(s.state.aspect());
    s.state.scroll(15., 25., true, egui::Modifiers::NONE, None);
    s.state.scroll(0., 2., false, egui::Modifiers::NONE, None);
    s.state.pinch(0.5, None);
    s.state.trackpad_rotate(12.);
    assert_eq!(
        s.state.camera.view_projection(s.state.aspect()),
        original_pose
    );
    // Selection/deletion and view shortcuts cannot leak through the pie.
    s.number_key(NumberKey::Numpad(5), true, egui::Modifiers::NONE)
        .unwrap();
    s.number_key(NumberKey::Numpad(5), false, egui::Modifiers::NONE)
        .unwrap();
    s.frame(
        vec![
            key(egui::Key::Delete, true),
            key(egui::Key::Tab, true),
            key(egui::Key::Tab, false),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.view_pie_active());
    assert_eq!(s.state.editor.document, original);
    assert_eq!(
        s.state.camera.view_projection(s.state.aspect()),
        original_pose
    );
    // A primary press belongs to this menu until its pointer release,
    // including after the trigger key has already been released.
    s.frame(
        vec![egui::Event::PointerMoved(top), pointer(top, true)],
        Duration::ZERO,
    )
    .unwrap();
    s.extra_layout_pass = true;
    s.frame(vec![key(egui::Key::Backtick, false)], Duration::ZERO)
        .unwrap();
    assert!(!s.state.view_pie_active());
    assert!(s.state.camera.is_transitioning());
    assert!(!s.state.editor.is_interacting());
    s.frame(vec![pointer(top, false)], Duration::from_millis(120))
        .unwrap();
    assert!(s.state.camera.direction_in_view(glam::Vec3::Y).z > 0.999);
    assert_eq!(s.state.editor.selected_object, Some(id));
    assert_eq!(s.state.editor.document, original);
    assert_eq!(s.state.editor.revision, revision);
    assert!(!s.state.editor.edit_mode);

    // One event batch must anchor at the press location, then choose at
    // release location. Layout retries must not redispatch this command.
    s.state.animate_views = false;
    s.frame(Vec::new(), Duration::ZERO).unwrap();
    s.extra_layout_pass = true;
    s.frame(
        vec![
            egui::Event::PointerMoved(anchor),
            key(egui::Key::Backtick, true),
            egui::Event::PointerMoved(anchor + egui::vec2(105., 0.)),
            key(egui::Key::Backtick, false),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(!s.state.view_pie_active());
    assert!(s.state.camera.direction_in_view(glam::Vec3::X).z > 0.999);
    let pose = s.state.camera.view_projection(s.state.aspect());
    for target in [anchor, anchor + egui::vec2(-105., 75.)] {
        s.frame(
            vec![
                egui::Event::PointerMoved(anchor),
                key(egui::Key::Backtick, true),
            ],
            Duration::ZERO,
        )
        .unwrap();
        assert!(s.state.view_pie_active());
        s.frame(
            vec![
                egui::Event::PointerMoved(target),
                key(egui::Key::Backtick, false),
            ],
            Duration::ZERO,
        )
        .unwrap();
        assert!(!s.state.view_pie_active());
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
    }
    s.state.editor.escape();
    s.frame(
        vec![
            egui::Event::PointerMoved(anchor),
            key(egui::Key::Backtick, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    let disabled = s.trace.get(Control::PieSelection).unwrap();
    assert!(!disabled.enabled);
    let fit = disabled.rect.center();
    s.frame(
        vec![
            egui::Event::PointerMoved(fit),
            key(egui::Key::Backtick, false),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
    // Ordinary shortcuts resume on the next frame.
    s.number_key(NumberKey::TopRow(2), true, egui::Modifiers::NONE)
        .unwrap();
    s.number_key(NumberKey::TopRow(2), false, egui::Modifiers::NONE)
        .unwrap();
    assert!(s.state.camera.direction_in_view(glam::Vec3::Z).z > 0.999);
    // Held-tool precedence is identical for coalesced and separate events.
    s.frame(
        vec![
            egui::Event::PointerMoved(anchor),
            key(egui::Key::Space, true),
            key(egui::Key::Backtick, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(!s.state.view_pie_active());
    assert!(s.state.hand_tool_active());
    s.frame(vec![key(egui::Key::Backtick, false)], Duration::ZERO)
        .unwrap();
    s.frame(
        vec![key(egui::Key::Space, false), key(egui::Key::Backtick, true)],
        Duration::ZERO,
    )
    .unwrap();
    assert!(
        s.state.view_pie_active(),
        "A preceding Space release restores pie eligibility in the same batch"
    );
    s.frame(vec![key(egui::Key::Backtick, false)], Duration::ZERO)
        .unwrap();
}

#[test]
fn hand_drag_obeys_event_order_release_and_layout_retries() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    let original = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let tool = s.state.editor.tool;
    let space = |pressed| egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(60., -75.);
    let delta = egui::vec2(32., -12.);
    let end = start + delta;
    let mut expected = s.state.camera.clone();
    expected.pan(delta.x, delta.y, s.state.viewport.height());
    // One native input batch can contain both Space and the pointer press.
    s.frame(
        vec![
            egui::Event::PointerMoved(start),
            space(true),
            pointer(start, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert!(s.state.hand_tool_active() && s.state.mouse_navigation_active());
    assert_eq!(s.cursor, egui::CursorIcon::Grabbing);
    assert!(!s.state.editor.is_interacting());
    s.extra_layout_pass = true;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    assert!(
        s.state
            .camera
            .view_projection(s.state.aspect())
            .abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5),
        "The shared path pans once even when egui repeats layout"
    );
    // Space release precedes more movement while the primary button is held.
    s.frame(
        vec![space(false), egui::Event::PointerMoved(start)],
        Duration::ZERO,
    )
    .unwrap();
    assert!(!s.state.hand_tool_active() && !s.state.mouse_navigation_active());
    assert_ne!(s.cursor, egui::CursorIcon::Grabbing);
    assert!(
        s.state
            .camera
            .view_projection(s.state.aspect())
            .abs_diff_eq(expected.view_projection(s.state.aspect()), 1e-5)
    );
    s.frame(vec![pointer(start, false)], Duration::ZERO)
        .unwrap();
    assert_eq!(
        s.state.editor.selected_object,
        Some(id),
        "Released hand click must not deselect"
    );
    assert!(!s.state.editor.edit_mode);
    assert_eq!(s.state.editor.tool, tool);
    assert_eq!(s.state.editor.document, original);
    assert_eq!(s.state.editor.revision, revision);

    // A short hand click has no camera motion and cannot enter edit mode.
    for _ in 0..2 {
        s.frame(
            vec![
                space(true),
                pointer(start, true),
                space(false),
                pointer(start, false),
            ],
            Duration::ZERO,
        )
        .unwrap();
        assert_eq!(s.state.editor.selected_object, Some(id));
        assert!(!s.state.editor.edit_mode);
        assert!(!s.state.editor.is_interacting());
    }
    // Escape cancels a hand drag before its pointer-up without deselecting.
    s.frame(vec![space(true), pointer(start, true)], Duration::ZERO)
        .unwrap();
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    s.frame(vec![pointer(start, false), space(false)], Duration::ZERO)
        .unwrap();
    assert!(!s.state.hand_tool_active() && !s.state.mouse_navigation_active());
    assert_eq!(s.state.editor.selected_object, Some(id));
    // Selection commands cancel a gesture but do not release the held key.
    s.key(egui::Key::Space, true, egui::Modifiers::NONE)
        .unwrap();
    let command = egui::Modifiers {
        command: true,
        mac_cmd: true,
        ..egui::Modifiers::NONE
    };
    s.key(egui::Key::A, true, command).unwrap();
    s.key(egui::Key::A, false, egui::Modifiers::NONE).unwrap();
    assert!(s.state.hand_tool_active());
    assert_eq!(s.cursor, egui::CursorIcon::Grab);
    s.key(egui::Key::Space, false, egui::Modifiers::NONE)
        .unwrap();
    // Normal primary selection is restored after the override ends.
    s.frame(Vec::new(), Duration::from_secs(1)).unwrap();
    s.click_at(start).unwrap();
    assert!(s.state.editor.selected_objects.is_empty());
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn hand_space_respects_fields_popups_focus_loss_and_gizmo() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.editor.document.clone();
    let pose = s.state.camera.view_projection(s.state.aspect());
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    assert!(
        s.ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| id != crate::shortcuts::viewport_focus_id())
    );
    s.key(egui::Key::Space, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(!s.state.hand_tool_active());
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(30., -30.);
    s.click_at(empty).unwrap();
    // Holding a Space initially consumed by the field cannot activate on focus change.
    s.key(egui::Key::Space, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(!s.state.hand_tool_active());
    s.key(egui::Key::Space, false, egui::Modifiers::NONE)
        .unwrap();
    s.right_click(Control::Viewport).unwrap();
    s.key(egui::Key::Space, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(!s.state.hand_tool_active());
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    assert!(!s.state.hand_tool_active());
    s.key(egui::Key::Space, false, egui::Modifiers::NONE)
        .unwrap();
    s.click_at(empty).unwrap();
    s.key(egui::Key::Space, true, egui::Modifiers::NONE)
        .unwrap();
    assert!(s.state.hand_tool_active());
    assert_eq!(s.cursor, egui::CursorIcon::Grab);
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    s.frame(vec![egui::Event::WindowFocused(true)], Duration::ZERO)
        .unwrap();
    assert!(!s.state.hand_tool_active());
    // Key-up while unfocused may be absent; focus return must not latch Space.
    s.key(egui::Key::Space, false, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Space, true, egui::Modifiers::NONE)
        .unwrap();
    s.hover(Control::AxisX).unwrap();
    assert_eq!(
        s.cursor,
        egui::CursorIcon::PointingHand,
        "The gizmo keeps its own cursor"
    );
    assert!(!s.state.mouse_navigation_active());
    s.key(egui::Key::Space, false, egui::Modifiers::NONE)
        .unwrap();
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn number_navigation_respects_real_field_focus_repeat_and_active_edits() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let id = s.state.editor.document.objects[0].id;
    s.state.editor.select_object(id).unwrap();
    s.state.editor.tool = crate::editor::Tool::Move;
    s.settle().unwrap();
    let original = s.state.editor.document.clone();
    let start = s.trace.get(Control::TransformX).unwrap().rect.center();
    s.frame(
        vec![egui::Event::PointerMoved(start), pointer(start, true)],
        Duration::ZERO,
    )
    .unwrap();
    // Cross one centimeter so the navigation check holds a changed preview.
    let end = start + egui::vec2(120., 0.);
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    assert!(s.state.editor.is_transforming());
    let preview = s.state.editor.document.clone();
    assert_ne!(preview, original);
    let pose = s.state.camera.view_projection(s.state.aspect());
    for (key, modifiers) in [
        (NumberKey::TopRow(1), egui::Modifiers::NONE),
        (NumberKey::TopRow(2), egui::Modifiers::NONE),
        (NumberKey::TopRow(1), egui::Modifiers::SHIFT),
        (NumberKey::TopRow(2), egui::Modifiers::SHIFT),
        (NumberKey::Numpad(4), egui::Modifiers::NONE),
        (NumberKey::Numpad(5), egui::Modifiers::NONE),
    ] {
        s.number_key(key, true, modifiers).unwrap();
        s.number_key(key, false, egui::Modifiers::NONE).unwrap();
        assert!(
            s.state.editor.is_transforming(),
            "A view key cannot finish an edit"
        );
        assert_eq!(
            s.state.editor.document, preview,
            "A view key cannot discard an edit"
        );
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
        assert!(!s.state.camera.is_transitioning());
    }
    s.key(egui::Key::Escape, true, egui::Modifiers::NONE)
        .unwrap();
    s.key(egui::Key::Escape, false, egui::Modifiers::NONE)
        .unwrap();
    s.frame(vec![pointer(end, false)], Duration::ZERO).unwrap();
    assert_eq!(s.state.editor.document, original);

    s.extra_layout_pass = true;
    s.number_key(NumberKey::Numpad(5), true, egui::Modifiers::NONE)
        .unwrap();
    assert!(
        s.state.camera.is_orthographic(),
        "A layout retry cannot toggle twice"
    );
    s.number_key(NumberKey::Numpad(5), true, egui::Modifiers::NONE)
        .unwrap();
    assert!(
        s.state.camera.is_orthographic(),
        "A held number cannot toggle twice"
    );
    s.number_key(NumberKey::Numpad(5), false, egui::Modifiers::NONE)
        .unwrap();
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2. * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    assert!(
        s.ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| id != crate::shortcuts::viewport_focus_id())
    );
    let pose = s.state.camera.view_projection(s.state.aspect());
    for (key, modifiers) in [
        (NumberKey::TopRow(1), egui::Modifiers::NONE),
        (NumberKey::TopRow(1), egui::Modifiers::SHIFT),
        (NumberKey::TopRow(2), egui::Modifiers::SHIFT),
        (NumberKey::Numpad(4), egui::Modifiers::NONE),
        (NumberKey::Numpad(5), egui::Modifiers::NONE),
    ] {
        s.number_key(key, true, modifiers).unwrap();
        s.number_key(key, false, egui::Modifiers::NONE).unwrap();
        assert_eq!(
            s.state.camera.view_projection(s.state.aspect()),
            pose,
            "Fields own numeric input"
        );
        assert!(!s.state.camera.is_transitioning());
        assert_eq!(s.state.editor.document, original);
        assert_eq!(s.state.editor.selected_object, Some(id));
    }
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.right_click(Control::Viewport).unwrap();
    s.number_key(NumberKey::TopRow(2), true, egui::Modifiers::SHIFT)
        .unwrap();
    s.number_key(NumberKey::TopRow(2), false, egui::Modifiers::NONE)
        .unwrap();
    assert_eq!(
        s.state.camera.view_projection(s.state.aspect()),
        pose,
        "Popups own numeric input"
    );
    s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)
        .unwrap();
    s.number_key(NumberKey::Numpad(5), true, egui::Modifiers::NONE)
        .unwrap();
    assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn missing_or_unregistered_feature_sources_fail() {
    let expected = BTreeSet::from(["gizmo.rs".to_owned()]);
    assert!(check_inventory(&expected, &expected).is_ok());
    assert!(check_inventory(&expected, &BTreeSet::new()).is_err());
    assert!(
        check_inventory(
            &expected,
            &BTreeSet::from(["gizmo.rs".into(), "forgotten.rs".into()])
        )
        .is_err()
    );
}
#[test]
fn unobserved_controls_and_uncaptured_images_cannot_be_documented() {
    for template in [
        "{{control:frame}}",
        "{{control:removed.save}}",
        "{{image:fake}}",
        "{{value:guess}}",
        "{{invalid:frame}}",
        "{{control:frame",
        "{{shortcut:removed.action}}",
        "{{key:Imaginary}}",
        "{{modifier:hyper}}",
    ] {
        assert!(
            render_template(
                template,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &Artifacts::new(),
                &BTreeSet::new(),
            )
            .is_err(),
            "{template}"
        );
    }
}
#[test]
fn text_images_missing_files_and_orphans_all_fail_check() {
    let expected = BTreeMap::from([
        ("gizmo.md".into(), b"verified".to_vec()),
        ("assets/gizmo.webp".into(), vec![1, 2, 3]),
    ]);
    assert!(compare(&expected, &expected).is_ok());
    for key in expected.keys() {
        let mut changed = expected.clone();
        changed.get_mut(key).unwrap().push(0);
        assert!(compare(&expected, &changed).is_err());
        changed.remove(key);
        assert!(compare(&expected, &changed).is_err());
    }
    let mut orphan = expected.clone();
    orphan.insert("forgotten.md".into(), vec![]);
    assert!(compare(&expected, &orphan).is_err());
}
#[test]
fn animations_require_their_own_binding_and_participate_in_strict_drift_checks() {
    let path = "assets/orbit.webp".to_owned();
    let media = BTreeMap::from([(path.clone(), vec![1, 2, 3])]);
    let clips = BTreeSet::from([path.clone()]);
    assert!(
        render_template(
            "{{animation:orbit}}",
            &BTreeMap::new(),
            &BTreeMap::new(),
            &media,
            &clips
        )
        .is_ok()
    );
    assert!(
        render_template(
            "{{image:orbit}}",
            &BTreeMap::new(),
            &BTreeMap::new(),
            &media,
            &clips
        )
        .is_err()
    );
    assert!(
        render_template(
            "{{animation:orbit}}",
            &BTreeMap::new(),
            &BTreeMap::new(),
            &media,
            &BTreeSet::new()
        )
        .is_err()
    );
    assert!(
        render_template(
            "No media",
            &BTreeMap::new(),
            &BTreeMap::new(),
            &media,
            &clips
        )
        .is_err()
    );
    let mut changed = media.clone();
    changed.get_mut(&path).unwrap().push(4);
    assert!(compare(&media, &changed).is_err());
}
#[test]
fn renamed_or_reparented_controls_change_generated_guide() {
    let mut bindings = BTreeMap::from([(
        Control::AnimateViews,
        "Preferences → Animate axis views".into(),
    )]);
    let before = render_template(
        "{{control:help.animate}}",
        &bindings,
        &BTreeMap::new(),
        &Artifacts::new(),
        &BTreeSet::new(),
    )
    .unwrap();
    bindings.insert(Control::AnimateViews, "Settings → Motion".into());
    let after = render_template(
        "{{control:help.animate}}",
        &bindings,
        &BTreeMap::new(),
        &Artifacts::new(),
        &BTreeSet::new(),
    )
    .unwrap();
    assert_ne!(before, after);
    assert!(before.contains("<code>Preferences → Animate axis views</code>"));
    assert!(after.contains("<code>Settings → Motion</code>"));
}

#[test]
fn guide_keycaps_resolve_semantic_bindings_and_keep_number_sources_distinct() {
    use crate::input::bindings::binding;
    let output = render_template(
        "{{shortcut:selection.duplicate}} {{shortcut:view.front}} {{shortcut:numpad.front}} {{shortcut:view.pie}} {{key:Enter}} {{modifier:shift}}",
        &BTreeMap::new(), &BTreeMap::new(), &Artifacts::new(), &BTreeSet::new(),
    ).unwrap();
    for id in [
        "selection.duplicate",
        "view.front",
        "numpad.front",
        "view.pie",
    ] {
        for key in binding(id).unwrap().key_parts() {
            assert!(output.contains(&format!("<kbd>{key}</kbd>")));
        }
    }
    assert!(output.contains("<kbd>Enter</kbd>"));
    assert!(output.contains("<kbd>Shift</kbd>"));
    assert!(!output.contains("{{"));
}

#[test]
fn control_markup_escapes_html_and_markdown_table_delimiters() {
    let output = render_template(
        "| {{control:frame}} |",
        &BTreeMap::from([(Control::Frame, "View → <Frame> & A | B".into())]),
        &BTreeMap::new(),
        &Artifacts::new(),
        &BTreeSet::new(),
    )
    .unwrap();
    assert!(output.contains("<code>View → &lt;Frame&gt; &amp; A &#124; B</code>"));
}

#[test]
fn semantic_shortcut_replay_keeps_native_source_and_hold_lifecycle() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.show_ui;
    s.shortcut("ui.toggle").unwrap();
    assert_eq!(s.state.show_ui, !original);
    s.shortcut("ui.toggle").unwrap();
    assert_eq!(s.state.show_ui, original);
    s.shortcut_down("numpad.front").unwrap();
    assert!(s.shortcut_is_down("numpad.front").unwrap());
    assert!(!s.shortcut_is_down("view.front").unwrap());
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))
        .unwrap();
    assert!(s.state.camera.is_orthographic());
    s.shortcut_up("numpad.front").unwrap();
    assert!(!s.shortcut_is_down("numpad.front").unwrap());
    s.shortcut_down("navigation.orbit").unwrap();
    assert!(s.input.modifiers().alt);
    s.shortcut_up("navigation.orbit").unwrap();
    assert!(!s.input.modifiers().alt);
    assert!(s.shortcut_event("numpad.front", true).is_err());
    assert!(s.shortcut("removed.action").is_err());
    assert_eq!(
        s.shortcut_bindings["ui.toggle"],
        s.shortcut_label("ui.toggle").unwrap()
    );
}
#[test]
fn guide_links_stay_within_user_documentation() {
    let mut artifacts = BTreeMap::from([
        (
            "README.md".into(),
            b"[Navigation](navigation.md#orbit)".to_vec(),
        ),
        (
            "navigation.md".into(),
            b"[Guide](README.md) [Orbit](#orbit)".to_vec(),
        ),
    ]);
    validate_links(&artifacts).unwrap();
    for target in [
        "../../README.md",
        "../../TODO.md",
        "../../AGENTS.md",
        "../../CONTRIBUTING.md",
        "../development.md",
        "../architecture/documentation-pipeline.md",
        "../research/ux-ideas.md",
    ] {
        artifacts.insert(
            "navigation.md".into(),
            format!("[Repository document]({target})").into_bytes(),
        );
        let error = validate_links(&artifacts).unwrap_err();
        assert!(error.contains(target), "{error}");
    }
}

#[test]
fn duplicate_owners_unsafe_paths_unreferenced_images_and_broken_links_fail() {
    let mut artifacts = Artifacts::new();
    insert(&mut artifacts, "gizmo.md".into(), vec![]).unwrap();
    assert!(insert(&mut artifacts, "gizmo.md".into(), vec![99]).is_err());
    assert_eq!(
        artifacts["gizmo.md"],
        Vec::<u8>::new(),
        "Rejected duplicate owners cannot replace the original artifact"
    );
    for path in [
        "../escape",
        "/absolute",
        "assets/../escape",
        "",
        "assets\\escape",
        "assets//alias.webp",
        "assets/./alias.webp",
        "assets/",
    ] {
        assert!(validate_path(path).is_err());
    }
    assert!(
        render_template(
            "no image",
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::from([("assets/orphan.webp".into(), vec![])]),
            &BTreeSet::new(),
        )
        .is_err()
    );
    assert!(
        validate_links(&BTreeMap::from([(
            "gizmo.md".into(),
            b"[bad](missing.md)".to_vec()
        )]))
        .is_err()
    );
}
