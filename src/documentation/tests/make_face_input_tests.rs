//! Make Face uses ordinary input ownership and the shared edit lifecycle.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{
    camera::View,
    document::{Document, Geometry, PrimitiveKind},
    editor::{Editor, Tool},
    shortcuts::viewport_focus_id,
};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use std::time::Duration;

fn setup(s: &mut Session<'_>, editing: bool) -> u64 {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Circle).unwrap();
    s.state.editor = Editor::new(document).unwrap();
    s.state.editor.select_object(id).unwrap();
    s.state.editor.set_tool(Tool::View);
    s.state.animate_views = false;
    s.state.set_view(View::Front);
    s.ctx
        .memory_mut(|memory| memory.request_focus(viewport_focus_id()));
    s.settle().unwrap();
    s.hover(Control::Viewport).unwrap();
    if editing {
        s.shortcut("edit.confirm").unwrap();
        s.shortcut("selection.all").unwrap();
        assert!(s.state.editor.edit_mode);
        assert!(s.state.editor.can_make_face());
    }
    id
}

fn faces(s: &Session<'_>, object: u64) -> usize {
    s.state
        .editor
        .document
        .eval_object(object)
        .unwrap()
        .faces
        .len()
}

fn key(pressed: bool, repeat: bool, modifiers: Modifiers) -> Event {
    Event::Key {
        key: Key::F,
        physical_key: Some(Key::F),
        pressed,
        repeat,
        modifiers,
    }
}

fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

#[test]
fn make_face_requires_edit_selection_and_accepts_one_undo_step() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    let id = setup(&mut s, false);
    let baseline = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    s.shortcut("mesh.make-face").unwrap();
    assert_eq!(s.state.editor.document, baseline);
    assert!(!s.state.editor.edit_mode);
    s.shortcut("edit.confirm").unwrap();
    s.shortcut("mesh.make-face").unwrap();
    assert!(s.state.editor.selected_vertices.is_empty());
    s.shortcut("selection.next").unwrap();
    assert_eq!(s.state.editor.selected_vertices.len(), 1);
    s.shortcut("mesh.make-face").unwrap();
    assert_eq!(s.state.editor.document, baseline);
    assert_eq!(s.state.editor.revision, revision);

    s.shortcut("selection.all").unwrap();
    let selected = s.state.editor.selected_vertices.clone();
    s.shortcut("mesh.make-face").unwrap();
    assert_eq!(faces(&s, id), 1);
    assert!(matches!(
        s.state.editor.document.objects[0].geometry,
        Geometry::Mesh(_)
    ));
    assert_eq!(s.state.editor.selected_vertices, selected);
    let filled = s.state.editor.document.clone();
    let filled_revision = s.state.editor.revision;
    s.shortcut("mesh.make-face").unwrap();
    assert_eq!(s.state.editor.document, filled);
    assert_eq!(s.state.editor.revision, filled_revision);
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, baseline);
    assert!(s.state.editor.edit_mode);
    assert_eq!(s.state.editor.selected_vertices, selected);
    s.redo().unwrap();
    assert_eq!(s.state.editor.document, filled);
    s.undo().unwrap();
    let undone_revision = s.state.editor.revision;
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, baseline);
    assert_eq!(
        s.state.editor.revision, undone_revision,
        "one Make Face is one undo step"
    );
}

#[test]
fn make_face_resolves_mode_and_selection_changes_earlier_in_one_input_batch() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    let id = setup(&mut s, false);
    let baseline = s.state.editor.document.clone();
    s.extra_layout_pass = true;
    let events = ["edit.confirm", "selection.all", "mesh.make-face"]
        .into_iter()
        .flat_map(|id| {
            [
                s.shortcut_event(id, true).unwrap(),
                s.shortcut_event(id, false).unwrap(),
            ]
        })
        .collect();
    s.frame(events, Duration::ZERO).unwrap();
    s.settle().unwrap();
    assert!(s.state.editor.edit_mode);
    assert_eq!(faces(&s, id), 1);
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, baseline);
}

#[test]
fn make_face_ignores_modified_keys_and_key_repeat_before_or_after_an_edit() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    let id = setup(&mut s, true);
    let baseline = s.state.editor.document.clone();
    for modifiers in [
        Modifiers::SHIFT,
        Modifiers::ALT,
        Modifiers::CTRL,
        Modifiers::MAC_CMD,
    ] {
        s.frame(
            vec![key(true, false, modifiers), key(false, false, modifiers)],
            Duration::ZERO,
        )
        .unwrap();
        s.modifiers_changed(Modifiers::NONE).unwrap();
        assert_eq!(
            s.state.editor.document, baseline,
            "modified F is not Make Face"
        );
    }
    // egui derives repeat from held-key state. Start with modified F, then
    // release the modifier while F remains held: repeats must not become edits.
    s.frame(vec![key(true, false, Modifiers::SHIFT)], Duration::ZERO)
        .unwrap();
    s.modifiers_changed(Modifiers::NONE).unwrap();
    s.frame(vec![key(true, true, Modifiers::NONE)], Duration::ZERO)
        .unwrap();
    assert_eq!(faces(&s, id), 0, "a repeat edge cannot start the command");
    s.frame(vec![key(false, false, Modifiers::NONE)], Duration::ZERO)
        .unwrap();
    s.shortcut_down("mesh.make-face").unwrap();
    assert_eq!(faces(&s, id), 1);
    let filled = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    s.frame(vec![key(true, true, Modifiers::NONE)], Duration::ZERO)
        .unwrap();
    assert_eq!(s.state.editor.document, filled);
    assert_eq!(s.state.editor.revision, revision);
    s.shortcut_up("mesh.make-face").unwrap();
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, baseline);
}

#[test]
fn make_face_respects_actual_text_and_popup_ownership() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    let id = setup(&mut s, true);
    let baseline = s.state.editor.document.clone();
    let selection = s.state.editor.selected_vertices.clone();
    s.click_path(&[Control::N3Menu, Control::Preferences])
        .unwrap();
    // Setup uses instant camera changes; enable animation through the real UI
    // so its duration field is editable before exercising text ownership.
    s.reveal_preferences_control(Control::AnimateViews).unwrap();
    s.click(Control::AnimateViews).unwrap();
    s.reveal_preferences_control(Control::Duration).unwrap();
    let slider = s.trace.get(Control::Duration).unwrap().rect;
    let spacing = &s.ctx.global_style().spacing;
    s.click_at(egui::pos2(
        slider.left() + spacing.slider_width + 2.0 * spacing.item_spacing.x,
        slider.center().y,
    ))
    .unwrap();
    let field = s.ctx.memory(|memory| memory.focused());
    assert!(field.is_some() && field != Some(viewport_focus_id()));
    s.frame(
        vec![Event::PointerMoved(s.state.viewport.center())],
        Duration::ZERO,
    )
    .unwrap();
    s.shortcut("mesh.make-face").unwrap();
    assert_eq!(s.state.editor.document, baseline);
    assert_eq!(s.state.editor.selected_vertices, selection);
    assert_eq!(s.ctx.memory(|memory| memory.focused()), field);
    s.click(Control::PreferencesClose).unwrap();
    s.right_click(Control::Viewport).unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    assert!(s.trace.get(Control::MakeFace).unwrap().enabled);
    s.shortcut("mesh.make-face").unwrap();
    assert!(egui::Popup::is_any_open(&s.ctx));
    assert_eq!(s.state.editor.document, baseline);
    s.click(Control::MakeFace).unwrap();
    assert_eq!(
        faces(&s, id),
        1,
        "the context menu reaches the same semantic action"
    );
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, baseline);
}

#[test]
fn make_face_never_finishes_a_marquee_or_mouse_navigation_gesture() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    for held in [
        PointerButton::Primary,
        PointerButton::Middle,
        PointerButton::Secondary,
    ] {
        let mut s = Session::new(&mut capture).unwrap();
        setup(&mut s, true);
        let baseline = s.state.editor.document.clone();
        let selection = s.state.editor.selected_vertices.clone();
        let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(50.0, -75.0);
        let end = start + egui::vec2(60.0, -35.0);
        s.frame(
            vec![Event::PointerMoved(start), button(start, held, true)],
            Duration::ZERO,
        )
        .unwrap();
        s.frame(vec![Event::PointerMoved(end)], Duration::from_millis(30))
            .unwrap();
        if held == PointerButton::Primary {
            assert!(s.state.editor.is_pointer_interacting());
        } else {
            assert!(s.state.mouse_navigation_active());
        }
        let pose = s.state.camera.view_projection(s.state.aspect());
        s.shortcut("mesh.make-face").unwrap();
        assert_eq!(s.state.editor.document, baseline);
        assert_eq!(s.state.editor.selected_vertices, selection);
        assert_eq!(s.state.camera.view_projection(s.state.aspect()), pose);
        if held == PointerButton::Primary {
            assert!(s.state.editor.is_pointer_interacting());
        } else {
            assert!(s.state.mouse_navigation_active());
        }
        s.frame(vec![button(end, held, false)], Duration::ZERO)
            .unwrap();
        s.undo().unwrap();
        assert_eq!(s.state.editor.document, baseline);
    }
}

#[test]
fn make_face_preserves_armed_released_and_numeric_transform_sessions() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    setup(&mut s, true);
    let baseline = s.state.editor.document.clone();
    let selection = s.state.editor.selected_vertices.clone();
    s.shortcut("tool.move").unwrap();
    s.shortcut("transform.axis-x").unwrap();
    s.shortcut("mesh.make-face").unwrap();
    assert_eq!(s.state.editor.transform_axis, Some(0));
    assert_eq!(s.state.editor.document, baseline);
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(60.0, -85.0);
    s.drag_at(start, egui::vec2(90.0, 0.0)).unwrap();
    assert!(s.state.editor.has_transform_session());
    assert!(!s.state.editor.is_pointer_interacting());
    let preview = s.state.editor.document.clone();
    assert_ne!(preview, baseline);
    s.shortcut("mesh.make-face").unwrap();
    assert!(s.state.editor.has_transform_session());
    assert_eq!(s.state.editor.transform_axis, Some(0));
    assert_eq!(s.state.editor.document, preview);
    assert_eq!(s.state.editor.selected_vertices, selection);

    s.frame(
        vec![
            Event::Key {
                key: Key::Num1,
                physical_key: Some(Key::Num1),
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
            Event::Text("1".into()),
            Event::Key {
                key: Key::Num1,
                physical_key: Some(Key::Num1),
                pressed: false,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ],
        Duration::ZERO,
    )
    .unwrap();
    assert_eq!(s.state.editor.numeric_text(), Some("1"));
    let typed = s.state.editor.document.clone();
    s.shortcut("mesh.make-face").unwrap();
    assert!(s.state.editor.has_transform_session());
    assert_eq!(s.state.editor.numeric_text(), Some("1"));
    assert_eq!(s.state.editor.document, typed);
    assert_eq!(s.state.editor.selected_vertices, selection);
    s.shortcut("cancel").unwrap();
    assert_eq!(s.state.editor.document, baseline);
    s.undo().unwrap();
    assert_eq!(
        s.state.editor.document, baseline,
        "ignored Make Face attempts add no history"
    );
}
