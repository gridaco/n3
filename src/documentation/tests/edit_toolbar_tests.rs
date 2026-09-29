use super::{Capture, Control, HEIGHT, Session, WIDTH};

#[test]
fn edit_mode_has_a_separate_exit_bar_without_resizing_the_tools() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut s = Session::new(&mut capture).unwrap();
    s.click(Control::InsertMenu).unwrap();
    s.click(Control::InsertCube).unwrap();
    s.click(Control::ToolMove).unwrap();
    let document = s.state.editor.document.clone();
    let revision = s.state.editor.revision;
    let toolbar = s.trace.get(Control::ViewportToolbar).unwrap().rect;
    assert!(s.trace.get(Control::EditToolbar).is_err());
    assert!(s.trace.get(Control::LeaveEdit).is_err());

    s.shortcut("edit.confirm").unwrap();
    assert!(s.state.editor.edit_mode);
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.revision, revision);
    assert_eq!(s.trace.get(Control::ViewportToolbar).unwrap().rect, toolbar);
    let edit_bar = s.trace.get(Control::EditToolbar).unwrap().rect;
    let exit = s.trace.get(Control::LeaveEdit).unwrap();
    assert!((s.state.viewport.bottom() - edit_bar.bottom() - crate::theme::space::XL).abs() < 1.0);
    assert!((edit_bar.center().x - s.state.viewport_ui_rect.center().x).abs() < 1.0);
    assert!(edit_bar.left() > toolbar.right());
    assert!(edit_bar.contains_rect(exit.rect));
    assert_eq!(exit.parents, [Control::EditToolbar]);
    assert!(exit.enabled);
    if let Ok(snap) = s.trace.get(Control::SnapFeedback) {
        assert!(snap.rect.top() > toolbar.bottom());
    }

    s.click(Control::LeaveEdit).unwrap();
    assert!(!s.state.editor.edit_mode);
    assert_eq!(s.state.editor.document, document);
    assert_eq!(s.state.editor.revision, revision);
    assert!(s.trace.get(Control::EditToolbar).is_err());
    assert!(s.trace.get(Control::LeaveEdit).is_err());
    assert_eq!(s.trace.get(Control::ViewportToolbar).unwrap().rect, toolbar);
}
