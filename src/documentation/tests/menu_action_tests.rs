//! Action menus share current capabilities and the ordinary command boundary.
use super::{Capture, Control, HEIGHT, Session, WIDTH};
use crate::{
    document::{Document, PrimitiveKind},
    editor::{Editor, EditorAccess},
    shortcuts::{Command, HostEffect},
};

fn click_with_layout_retry(s: &mut Session<'_>, control: Control) {
    s.hover(control).unwrap();
    s.pointer_button(egui::PointerButton::Primary, true)
        .unwrap();
    // Retry the release carrying the click, rather than the preceding hover.
    s.extra_layout_pass = true;
    s.pointer_button(egui::PointerButton::Primary, false)
        .unwrap();
    s.settle().unwrap();
}

#[test]
fn context_actions_follow_selection_and_duplicate_once_across_layout_retry() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    s.state.editor.deselect();
    s.settle().unwrap();
    let original = s.state.editor.document.clone();

    s.right_click(Control::Viewport).unwrap();
    for control in [
        Control::DuplicateSelection,
        Control::DeleteSelection,
        Control::FrameSelection,
    ] {
        let row = s.trace.get(control).unwrap();
        assert!(!row.enabled);
        assert_eq!(row.parents, [Control::ViewportMenu]);
    }
    assert!(s.trace.get(Control::MakeFace).is_err());
    s.click(Control::SelectAll).unwrap();
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(
        s.state.editor.selected_objects.len(),
        original.objects.len()
    );
    assert_eq!(s.state.editor.document, original);

    s.right_click(Control::Viewport).unwrap();
    for control in [
        Control::DuplicateSelection,
        Control::DeleteSelection,
        Control::FrameSelection,
    ] {
        assert!(s.trace.get(control).unwrap().enabled);
    }
    click_with_layout_retry(&mut s, Control::DuplicateSelection);
    assert!(!egui::Popup::is_any_open(&s.ctx));
    assert_eq!(
        s.state.editor.document.objects.len(),
        original.objects.len() * 2,
        "A retried menu click dispatches Duplicate exactly once"
    );
    s.undo().unwrap();
    assert_eq!(s.state.editor.document, original);
    assert!(
        !s.state.editor.undo(),
        "A menu command creates exactly one history entry"
    );

    // A command presented as enabled must recheck its capability at dispatch.
    s.state.editor.deselect();
    s.state.dispatch(Command::DuplicateSelection, &s.ctx, false);
    assert_eq!(s.state.editor.document, original);
}

#[test]
fn make_face_visibility_and_eligibility_refresh_in_the_open_context_menu() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    let mut document = Document::default();
    let object = document.insert_primitive(PrimitiveKind::Circle).unwrap();
    s.state.editor = Editor::new(document).unwrap();
    s.state.editor.select_object(object).unwrap();
    s.ctx
        .memory_mut(|memory| memory.request_focus(crate::shortcuts::viewport_focus_id()));
    s.settle().unwrap();
    s.hover(Control::Viewport).unwrap();
    s.shortcut("edit.confirm").unwrap();
    assert!(s.state.editor.edit_mode);
    assert!(s.state.editor.selected_vertices.is_empty());
    s.right_click(Control::Viewport).unwrap();
    assert!(!s.trace.get(Control::MakeFace).unwrap().enabled);
    assert!(!s.trace.get(Control::DuplicateSelection).unwrap().enabled);

    // Fixture selection changes isolate capability refresh from picking policy.
    let vertices: Vec<_> = s
        .state
        .editor
        .document
        .eval_object(object)
        .unwrap()
        .vertices
        .iter()
        .take(3)
        .map(|vertex| vertex.id)
        .collect();
    for count in [2, 3] {
        s.state.editor.selected_vertices = vertices[..count].iter().copied().collect();
        s.settle().unwrap();
        assert!(egui::Popup::is_any_open(&s.ctx));
        assert_eq!(s.trace.get(Control::MakeFace).unwrap().enabled, count == 3);
        assert!(s.trace.get(Control::DeleteSelection).unwrap().enabled);
        assert!(s.trace.get(Control::FrameSelection).unwrap().enabled);
    }
}

#[test]
fn file_menu_actions_preserve_host_effects_and_read_only_capabilities() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.load_fixture("bracket.obj").unwrap();
    let original = s.state.editor.document.clone();

    for (control, effect) in [
        (Control::Open, HostEffect::Open),
        (Control::Import, HostEffect::Import),
    ] {
        s.click_path(&[Control::N3Menu, Control::FileMenu]).unwrap();
        assert!(s.take_host_effects().is_empty());
        click_with_layout_retry(&mut s, control);
        assert_eq!(s.take_host_effects(), [effect]);
        assert!(!egui::Popup::is_any_open(&s.ctx));
        assert_eq!(s.state.editor.document, original);
    }

    s.state.editor.set_access(EditorAccess::ReadOnly);
    s.click_path(&[Control::N3Menu, Control::FileMenu]).unwrap();
    for control in [Control::New, Control::Open] {
        assert!(s.trace.get(control).unwrap().enabled);
    }
    for control in [Control::Import, Control::Save, Control::SaveAs] {
        assert!(!s.trace.get(control).unwrap().enabled);
    }
    assert_eq!(s.state.editor.document, original);
}
