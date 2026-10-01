use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{
    camera::View, document::PrimitiveKind, editor::Tool, scroll_input::ScrollPhase,
    shortcuts::viewport_focus_id,
};
use egui::{Key, Modifiers};
use std::time::Duration;

#[test]
fn batched_locked_double_click_commits_and_unlocks_without_selecting_or_entering_edit() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for on_surface in [false, true] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        s.state.editor.convert_selected().unwrap();
        tap(&mut s, Key::X, Modifiers::NONE);
        let original = s.state.editor.document.clone();
        let selected = s.state.editor.selected_objects.clone();
        let position = if on_surface {
            s.state.viewport.center()
        } else {
            s.empty_viewport_point().unwrap()
        };
        // Exactly two clicks: a third click after confirmation legitimately
        // belongs to ordinary selection again.
        for click in 0..2 {
            s.extra_layout_pass = true;
            s.frame(
                vec![
                    egui::Event::PointerMoved(position),
                    super::pointer(position, true),
                    super::pointer(position, false),
                ],
                Duration::from_millis(30),
            )
            .unwrap();
            assert_eq!(s.state.editor.selected_objects, selected);
            assert!(!s.state.editor.edit_mode);
            assert_eq!(s.state.editor.transform_axis, (click == 0).then_some(0));
            assert_eq!(s.state.editor.document, original);
        }
        assert!(!s.state.editor.has_transform_session());
    }
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.state.editor.deselect();
    tap(&mut s, Key::X, Modifiers::NONE);
    let center = s.state.viewport.center();
    s.frame(
        vec![
            egui::Event::PointerMoved(center),
            super::pointer(center, true),
            super::pointer(center, false),
        ],
        Duration::from_millis(30),
    )
    .unwrap();
    assert!(s.state.editor.selected_objects.is_empty());
    assert_eq!(s.state.editor.transform_axis, Some(0));
}

fn tap(s: &mut Session<'_>, key: Key, modifiers: Modifiers) {
    s.key(key, true, modifiers).unwrap();
    s.key(key, false, Modifiers::NONE).unwrap();
}

fn setup(s: &mut Session<'_>) {
    s.state.editor.insert(PrimitiveKind::Cube).unwrap();
    s.state.editor.set_tool(crate::editor::Tool::Move);
    s.state.camera.set_view(View::Front);
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.settle().unwrap();
}

fn drag_locked(s: &mut Session<'_>, dx: f32) {
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(65.0, -65.0);
    s.drag_at(start, egui::vec2(dx, 0.0)).unwrap();
    assert!(s.state.editor.has_transform_session());
    assert!(s.state.editor.is_interacting());
    assert!(!s.state.editor.is_pointer_interacting());
}

#[test]
fn released_locked_moves_and_arrows_share_one_confirmation_and_undo() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let baseline = s.state.editor.document.clone();
    tap(&mut s, Key::X, Modifiers::NONE);
    // Produce a visible Auto-snapped preview before keyboard nudges.
    drag_locked(&mut s, 120.0);
    let first_drag = s.state.editor.document.objects[0].transform.translation[0];
    assert!(first_drag > 0.0);
    s.extra_layout_pass = true;
    s.key(Key::ArrowRight, true, Modifiers::NONE).unwrap();
    s.key(Key::ArrowRight, true, Modifiers::SHIFT).unwrap();
    s.key(Key::ArrowRight, false, Modifiers::NONE).unwrap();
    tap(&mut s, Key::ArrowLeft, Modifiers::NONE);
    // Pointer Auto may leave a fractional coordinate. Keyboard nudges retain
    // their own one-centimeter destination grid before continuing this session.
    assert_eq!(
        s.state.editor.document.objects[0].transform.translation[0],
        (first_drag + 10.0).round(),
    );
    drag_locked(&mut s, -12.0);
    let preview = s.state.editor.document.clone();
    assert_ne!(preview, baseline);
    assert_eq!(s.state.editor.transform_axis, Some(0));
    tap(&mut s, Key::Enter, Modifiers::NONE);
    assert_eq!(s.state.editor.document, preview);
    assert_eq!(s.state.editor.transform_axis, None);
    assert!(!s.state.editor.is_interacting());
    assert!(!s.state.editor.edit_mode);
    tap(&mut s, Key::Z, Modifiers::COMMAND);
    assert_eq!(s.state.editor.document, baseline);
    tap(&mut s, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(s.state.editor.document, preview);
    tap(&mut s, Key::Z, Modifiers::COMMAND);
    assert_eq!(s.state.editor.document, baseline);
    tap(&mut s, Key::Z, Modifiers::COMMAND);
    assert!(s.state.editor.document.objects.is_empty());
}

#[test]
fn every_paused_transform_allows_navigation_and_resumes_one_object_or_vertex_transaction() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for (tool, binding) in [
        (Tool::Move, "tool.move"),
        (Tool::Rotate, "tool.rotate"),
        (Tool::Scale, "tool.scale"),
    ] {
        for edit in [false, true] {
            let mut s = Session::new(&mut capture).unwrap();
            setup(&mut s);
            s.state.animate_views = false;
            if edit {
                s.shortcut("edit.confirm").unwrap();
                // Select the complete evaluated cube as fixture setup so every
                // transform remains valid independently of face visibility.
                s.state.editor.selected_vertices = s
                    .state
                    .editor
                    .document
                    .eval_object(s.state.editor.selected_object.unwrap())
                    .unwrap()
                    .vertices
                    .iter()
                    .map(|vertex| vertex.id)
                    .collect();
            }
            s.shortcut(binding).unwrap();
            s.shortcut("view.perspective").unwrap();
            s.shortcut("transform.axis-x").unwrap();
            let baseline = s.state.editor.document.clone();
            let selection = s.state.editor.selected_objects.clone();
            let vertices = s.state.editor.selected_vertices.clone();
            let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(140.0, -180.0);
            let end = start + egui::vec2(70.0, -20.0);
            s.frame(
                vec![
                    egui::Event::PointerMoved(start),
                    super::pointer(start, true),
                ],
                Duration::ZERO,
            )
            .unwrap();
            s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
                .unwrap();
            assert!(
                s.state.editor.is_pointer_interacting(),
                "{tool:?}, edit={edit}"
            );
            let preview = s.state.editor.document.clone();
            assert_ne!(preview, baseline, "{tool:?}, edit={edit}");
            let held_pose = s.state.camera.view_projection(1.0);
            s.pinch(0.2).unwrap();
            s.trackpad_scroll(25.0, -18.0, Modifiers::NONE, ScrollPhase::Started)
                .unwrap();
            s.trackpad_scroll(0.0, 0.0, Modifiers::NONE, ScrollPhase::Ended)
                .unwrap();
            assert_eq!(s.state.camera.view_projection(1.0), held_pose);
            assert_eq!(s.state.editor.document, preview);
            s.frame(vec![super::pointer(end, false)], Duration::ZERO)
                .unwrap();
            assert!(!s.state.editor.is_pointer_interacting());
            assert!(s.state.editor.has_transform_session());

            for navigation in 0..6 {
                let pose = s.state.camera.view_projection(1.0);
                match navigation {
                    0 | 1 => {
                        let modifiers = if navigation == 1 {
                            Modifiers::SHIFT
                        } else {
                            Modifiers::NONE
                        };
                        s.trackpad_scroll(25.0, -18.0, modifiers, ScrollPhase::Started)
                            .unwrap();
                        s.trackpad_scroll(0.0, 0.0, modifiers, ScrollPhase::Ended)
                            .unwrap();
                        s.modifiers_changed(Modifiers::NONE).unwrap();
                    }
                    2 => s.pinch(0.15).unwrap(),
                    3 => s.scroll(0.0, 1.0, false, Modifiers::NONE).unwrap(),
                    4 => {
                        let button = |pos, pressed| egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Secondary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        };
                        s.frame(
                            vec![egui::Event::PointerMoved(start), button(start, true)],
                            Duration::ZERO,
                        )
                        .unwrap();
                        s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
                            .unwrap();
                        assert!(s.state.mouse_navigation_active());
                        // Escape belongs to this camera gesture, not the paused
                        // transform underneath it. The next release stays inert.
                        s.shortcut("cancel").unwrap();
                        assert!(!s.state.mouse_navigation_active());
                        s.frame(vec![button(end, false)], Duration::ZERO).unwrap();
                    }
                    _ => {
                        let held = if edit {
                            "navigation.pan"
                        } else {
                            "navigation.orbit"
                        };
                        s.shortcut_down(held).unwrap();
                        s.drag_at(start, egui::vec2(35.0, -22.0)).unwrap();
                        s.shortcut_up(held).unwrap();
                    }
                }
                assert_ne!(
                    s.state.camera.view_projection(1.0),
                    pose,
                    "{tool:?}, edit={edit}, navigation={navigation}"
                );
                assert_eq!(
                    s.state.editor.document, preview,
                    "Navigation cannot rewrite the preview"
                );
                assert!(s.state.editor.has_transform_session());
                assert_eq!(s.state.editor.transform_axis, Some(0));
                assert_eq!(s.state.editor.selected_objects, selection);
                assert_eq!(s.state.editor.selected_vertices, vertices);
                assert_eq!(s.state.editor.edit_mode, edit);
            }

            // A new drag captures the changed camera while retaining the same
            // operation baseline. Applying still creates exactly one undo step.
            s.extra_layout_pass = true;
            s.drag_at(start, egui::vec2(45.0, -18.0)).unwrap();
            let combined = s.state.editor.document.clone();
            assert_ne!(combined, preview, "{tool:?}, edit={edit}");
            let navigated_pose = s.state.camera.view_projection(1.0);
            s.shortcut("edit.confirm").unwrap();
            assert!(!s.state.editor.has_transform_session());
            assert_eq!(s.state.editor.edit_mode, edit);
            s.shortcut("history.undo").unwrap();
            assert_eq!(s.state.editor.document, baseline);
            assert_eq!(s.state.camera.view_projection(1.0), navigated_pose);
            s.shortcut("history.redo").unwrap();
            assert_eq!(s.state.editor.document, combined);

            // Cancelling another paused preview preserves the navigated camera
            // and adds no transaction, so Undo still reaches the first baseline.
            s.shortcut("transform.axis-x").unwrap();
            s.drag_at(start, egui::vec2(45.0, -18.0)).unwrap();
            assert_ne!(s.state.editor.document, combined);
            s.pinch(0.1).unwrap();
            let cancelled_pose = s.state.camera.view_projection(1.0);
            s.shortcut("cancel").unwrap();
            assert_eq!(s.state.editor.document, combined);
            assert_eq!(s.state.camera.view_projection(1.0), cancelled_pose);
            s.shortcut("history.undo").unwrap();
            assert_eq!(s.state.editor.document, baseline);
        }
    }
}

#[test]
fn released_locked_move_cancels_on_escape_or_focus_loss_without_creating_history() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for lose_focus in [false, true] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s);
        let baseline = s.state.editor.document.clone();
        let selected = s.state.editor.selected_objects.clone();
        tap(&mut s, Key::X, Modifiers::NONE);
        drag_locked(&mut s, 24.0);
        tap(&mut s, Key::ArrowUp, Modifiers::NONE);
        assert_ne!(s.state.editor.document, baseline);
        if lose_focus {
            s.frame(vec![egui::Event::WindowFocused(false)], Duration::ZERO)
                .unwrap();
            s.frame(vec![egui::Event::WindowFocused(true)], Duration::ZERO)
                .unwrap();
            s.ctx
                .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
        } else {
            tap(&mut s, Key::Escape, Modifiers::NONE);
        }
        assert_eq!(s.state.editor.document, baseline);
        assert_eq!(s.state.editor.selected_objects, selected);
        assert_eq!(s.state.editor.transform_axis, None);
        assert!(!s.state.editor.is_interacting());
        // There is no Move undo entry; the only previous edit was insertion.
        tap(&mut s, Key::Z, Modifiers::COMMAND);
        assert!(s.state.editor.document.objects.is_empty());
        tap(&mut s, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(s.state.editor.document, baseline);
    }
}

#[test]
fn held_enter_layout_retry_and_pointer_release_commit_once_without_edit_cascade() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    s.state.editor.convert_selected().unwrap();
    let baseline = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    tap(&mut s, Key::X, Modifiers::NONE);
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(70.0, -70.0);
    // A real document preview must cross the default snapping threshold.
    let end = start + egui::vec2(120.0, 0.0);
    s.frame(
        vec![
            egui::Event::PointerMoved(start),
            super::pointer(start, true),
        ],
        Duration::ZERO,
    )
    .unwrap();
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)
        .unwrap();
    assert!(s.state.editor.is_pointer_interacting());
    let preview = s.state.editor.document.clone();
    assert_ne!(preview, baseline);
    s.extra_layout_pass = true;
    s.key(Key::Enter, true, Modifiers::NONE).unwrap();
    s.key(Key::Enter, true, Modifiers::NONE).unwrap();
    s.frame(vec![super::pointer(end, false)], Duration::ZERO)
        .unwrap();
    s.key(Key::Enter, false, Modifiers::NONE).unwrap();
    assert_eq!(s.state.editor.document, preview);
    assert_eq!(s.state.editor.selected_objects, selected);
    assert_eq!(s.state.editor.transform_axis, None);
    assert!(!s.state.editor.is_interacting());
    assert!(!s.state.editor.edit_mode);
    tap(&mut s, Key::Z, Modifiers::COMMAND);
    assert_eq!(s.state.editor.document, baseline);
    tap(&mut s, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(s.state.editor.document, preview);
}

#[test]
fn pending_move_blocks_document_actions_allows_navigation_and_preferences_keeps_ownership() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let baseline = s.state.editor.document.clone();
    tap(&mut s, Key::X, Modifiers::NONE);
    drag_locked(&mut s, 35.0);
    let preview = s.state.editor.document.clone();
    // Explanatory errors can wrap the status bar and change viewport aspect.
    // Compare the camera independently of that legitimate layout change.
    let selection = s.state.editor.selected_objects.clone();
    tap(&mut s, Key::D, Modifiers::COMMAND);
    assert_eq!(s.state.editor.document, preview);
    assert_eq!(s.state.editor.selected_objects, selection);
    assert!(s.state.editor.has_transform_session());
    let planar = s.state.is_planar_navigation();
    // Period belongs to numeric transform input while an axis is armed. Camera
    // UI remains available, without stealing this decimal point from its owner.
    tap(&mut s, Key::Period, Modifiers::NONE);
    assert_eq!(s.state.is_planar_navigation(), planar);
    assert_eq!(s.state.editor.document, preview);
    assert!(s.state.editor.has_transform_session());
    for (key, modifiers) in [
        (Key::S, Modifiers::COMMAND),
        (Key::S, Modifiers::COMMAND | Modifiers::SHIFT),
        (Key::N, Modifiers::COMMAND),
        (Key::O, Modifiers::COMMAND),
        (Key::Q, Modifiers::COMMAND),
    ] {
        s.state.error = None;
        assert!(!s.key(key, true, modifiers).unwrap());
        s.key(key, false, Modifiers::NONE).unwrap();
        assert!(
            s.state.error.is_some(),
            "pending {key:?} requires an explanation"
        );
        assert!(!s.state.request_new && !s.state.request_save && !s.state.request_save_as);
        assert_eq!(s.state.editor.document, preview);
        assert!(s.state.editor.has_transform_session());
    }
    for control in [Control::New, Control::Open, Control::Save, Control::SaveAs] {
        if s.trace.get(Control::N3Menu).is_ok() && s.trace.get(Control::FileMenu).is_err() {
            s.click(Control::N3Menu).unwrap();
        }
        if s.trace.get(control).is_err() {
            s.click(Control::FileMenu).unwrap();
        }
        let target = s.trace.get(control).unwrap();
        assert!(
            !target.enabled,
            "{control:?} must not accept a Move preview"
        );
        assert!(!s.click_at(target.rect.center()).unwrap());
        assert_eq!(s.state.editor.document, preview);
        assert!(s.state.editor.has_transform_session());
    }
    if egui::Popup::is_any_open(&s.ctx) {
        s.key(Key::Escape, true, Modifiers::NONE).unwrap();
        s.key(Key::Escape, false, Modifiers::NONE).unwrap();
    }
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert!(s.trace.get(Control::EditToolbar).is_err());
    let insert = s.trace.get(Control::InsertMenu).unwrap();
    assert!(
        !insert.enabled,
        "Insertion must not accept a preview implicitly"
    );
    assert!(!s.click_at(insert.rect.center()).unwrap());
    assert_eq!(s.state.editor.document, preview);
    assert!(s.state.editor.has_transform_session());
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert!(!s.state.request_new && !s.state.request_save);
    s.click(Control::N3Menu).unwrap();
    assert!(s.trace.get(Control::ViewMenu).unwrap().enabled);
    s.click(Control::ViewMenu).unwrap();
    assert!(!s.trace.get(Control::LocalViewMenu).unwrap().enabled);
    tap(&mut s, Key::Escape, Modifiers::NONE);
    assert!(
        egui::Popup::is_any_open(&s.ctx),
        "Escape closes only the View submenu"
    );
    assert!(s.trace.get(Control::LocalViewMenu).is_err());
    assert_eq!(s.state.editor.document, preview);
    tap(&mut s, Key::Escape, Modifiers::NONE);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert!(s.trace.get(Control::Projection).unwrap().enabled);
    let orthographic = s.state.camera.is_orthographic();
    s.click(Control::Projection).unwrap();
    assert_ne!(s.state.camera.is_orthographic(), orthographic);
    assert_eq!(s.state.editor.document, preview);
    let gizmo = s.trace.get(Control::Gizmo).unwrap();
    assert!(gizmo.enabled);
    let pose = s.state.camera.view_projection(1.0);
    let disk_body = gizmo.rect.left_top() + egui::vec2(35.0, 35.0);
    s.drag_at(disk_body, egui::vec2(-100.0, 35.0)).unwrap();
    assert_ne!(s.state.camera.view_projection(1.0), pose);
    assert_eq!(s.state.editor.document, preview);
    assert!(s.state.editor.has_transform_session());

    s.state.animate_views = true;
    s.state.view_duration_ms = 120;
    s.click(Control::AxisY).unwrap();
    assert!(s.state.camera.is_transitioning());
    s.extra_layout_pass = true;
    s.frame(vec![], Duration::from_millis(60)).unwrap();
    assert!(s.state.camera.is_transitioning());
    assert!(s.state.editor.has_transform_session());
    s.frame(vec![], Duration::from_millis(60)).unwrap();
    assert!(!s.state.camera.is_transitioning());
    assert_eq!(s.state.editor.document, preview);
    let display_frame = s.state.editor.frame;
    s.shortcut("view.frame").unwrap();
    assert_eq!(s.state.editor.frame, display_frame);
    assert_eq!(s.state.editor.document, preview);
    assert!(s.state.editor.has_transform_session());
    assert_eq!(s.state.editor.transform_axis, Some(0));

    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    assert!(s.state.show_preferences);
    assert_eq!(s.state.editor.document, preview);
    tap(&mut s, Key::Escape, Modifiers::NONE);
    assert!(!s.state.show_preferences);
    assert_eq!(s.state.editor.document, preview);
    assert!(s.state.editor.has_transform_session());
    assert_eq!(s.state.editor.transform_axis, Some(0));
    // The next viewport Escape belongs to the move rather than Preferences.
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    tap(&mut s, Key::Escape, Modifiers::NONE);
    assert_eq!(s.state.editor.document, baseline);
    assert_eq!(s.state.editor.transform_axis, None);

    // A blocked Save notice belongs to the preview. Undo cancels the preview
    // without consuming an earlier edit or leaving a stale Apply instruction.
    tap(&mut s, Key::X, Modifiers::NONE);
    tap(&mut s, Key::ArrowRight, Modifiers::NONE);
    tap(&mut s, Key::S, Modifiers::COMMAND);
    assert!(s.state.error.is_some());
    tap(&mut s, Key::Z, Modifiers::COMMAND);
    assert_eq!(s.state.editor.document, baseline);
    assert!(!s.state.editor.has_transform_session());
    assert!(s.state.error.is_none());
}

#[test]
fn held_arrows_form_one_undo_step_and_new_presses_start_new_steps() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    let original = s.state.editor.document.clone();
    s.extra_layout_pass = true;
    s.key(Key::ArrowRight, true, Modifiers::NONE).unwrap();
    s.key(Key::ArrowRight, true, Modifiers::NONE).unwrap();
    s.key(Key::ArrowRight, true, Modifiers::SHIFT).unwrap();
    assert_eq!(
        s.state.editor.document.objects[0].transform.translation,
        [12.0, 0.0, 0.0]
    );
    s.key(Key::ArrowRight, false, Modifiers::NONE).unwrap();
    let held = s.state.editor.document.clone();
    tap(&mut s, Key::ArrowRight, Modifiers::NONE);
    assert_eq!(
        s.state.editor.document.objects[0].transform.translation,
        [13.0, 0.0, 0.0]
    );
    tap(&mut s, Key::Z, Modifiers::COMMAND);
    assert_eq!(s.state.editor.document, held);
    tap(&mut s, Key::Z, Modifiers::COMMAND);
    assert_eq!(s.state.editor.document, original);
    tap(&mut s, Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(s.state.editor.document, held);
}

#[test]
fn fields_popups_and_hand_tool_keep_move_shortcuts_out_of_the_document() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s);
    tap(&mut s, Key::X, Modifiers::NONE);
    assert_eq!(s.state.editor.transform_axis, Some(0));
    let original = s.state.editor.document.clone();
    s.key(Key::Space, true, Modifiers::NONE).unwrap();
    tap(&mut s, Key::Z, Modifiers::NONE);
    tap(&mut s, Key::ArrowUp, Modifiers::SHIFT);
    assert_eq!(s.state.editor.transform_axis, Some(0));
    assert_eq!(s.state.editor.document, original);
    s.key(Key::Space, false, Modifiers::NONE).unwrap();

    tap(&mut s, Key::I, Modifiers::SHIFT);
    assert!(egui::Popup::is_any_open(&s.ctx));
    tap(&mut s, Key::Y, Modifiers::NONE);
    tap(&mut s, Key::ArrowDown, Modifiers::NONE);
    assert_eq!(s.state.editor.transform_axis, Some(0));
    assert_eq!(s.state.editor.document, original);
    tap(&mut s, Key::Escape, Modifiers::NONE);
    assert_eq!(s.state.editor.transform_axis, Some(0));

    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2.0 * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    assert!(
        s.ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| id != viewport_focus_id())
    );
    tap(&mut s, Key::Z, Modifiers::NONE);
    s.key(Key::ArrowUp, true, Modifiers::NONE).unwrap();
    assert_eq!(s.state.editor.transform_axis, Some(0));
    assert_eq!(s.state.editor.document, original);
    // A hold that began in a field cannot start nudging after focus transfers.
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.key(Key::ArrowUp, true, Modifiers::NONE).unwrap();
    assert_eq!(s.state.editor.document, original);
    s.key(Key::ArrowUp, false, Modifiers::NONE).unwrap();
    s.reveal_preferences_control(Control::PreferencesClose)
        .unwrap();
    s.click(Control::PreferencesClose).unwrap();
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    tap(&mut s, Key::Escape, Modifiers::NONE);
    assert_eq!(s.state.editor.transform_axis, None);
    assert!(!s.state.editor.selected_objects.is_empty());
    for tool in [Key::E, Key::R] {
        tap(&mut s, tool, Modifiers::NONE);
        tap(&mut s, Key::X, Modifiers::NONE);
        tap(&mut s, Key::ArrowUp, Modifiers::NONE);
        // Rotate and Scale now accept axis locks, but Move's centimeter
        // nudges must never leak into either tool.
        assert_eq!(s.state.editor.transform_axis, Some(0));
        assert_eq!(s.state.editor.document, original);
    }
}
