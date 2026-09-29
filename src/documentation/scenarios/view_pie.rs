use super::{Result, Session};
use crate::controls::Control;
use glam::{Mat4, Vec3};
use std::time::Duration;

fn matrix(s: &Session<'_>) -> Mat4 {
    s.state.camera.view_projection(s.state.aspect())
}

fn open(s: &mut Session<'_>, anchor: egui::Pos2) -> Result<()> {
    s.frame(vec![egui::Event::PointerMoved(anchor)], Duration::ZERO)?;
    s.frame(vec![s.shortcut_event("view.pie", true)?], Duration::ZERO)?;
    s.frame(Vec::new(), Duration::ZERO)?;
    s.require(
        s.state.view_pie_active() && s.shortcut_is_down("view.pie")?,
        "Holding the view-pie key over the viewport opens the view pie",
    )?;
    s.witness(Control::ViewPie)
}

fn hover_choice(s: &mut Session<'_>, control: Control) -> Result<()> {
    s.witness(control)?;
    let choice = s.trace.get(control)?;
    s.require(
        choice.enabled && choice.parents == [Control::ViewPie],
        "The enabled choice is part of the actual view pie",
    )?;
    let point = s.trace.get(control)?.rect.center();
    s.frame(vec![egui::Event::PointerMoved(point)], Duration::ZERO)?;
    Ok(())
}

fn release(s: &mut Session<'_>) -> Result<()> {
    s.frame(vec![s.shortcut_event("view.pie", false)?], Duration::ZERO)?;
    s.frame(Vec::new(), Duration::ZERO)?;
    s.require(
        !s.state.view_pie_active(),
        "Releasing the view-pie key closes the view pie",
    )
}

fn disabled_selection(s: &mut Session<'_>, anchor: egui::Pos2) -> Result<()> {
    let before = matrix(s);
    open(s, anchor)?;
    let choice = s.trace.get(Control::PieSelection)?;
    let point = choice.rect.center();
    s.require(
        !choice.enabled,
        "Frame selection is disabled when the current editing context has no selection",
    )?;
    s.frame(vec![egui::Event::PointerMoved(point)], Duration::ZERO)?;
    release(s)?;
    s.require(
        matrix(s) == before,
        "Releasing over disabled Frame selection leaves the camera unchanged",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("cube-quads.obj")?;
    s.witness(Control::Viewport)?;
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(18.0, -18.0);
    s.click_at(empty)?;
    s.shortcut("selection.next")?;
    s.require(
        s.state.editor.selected_objects.len() == 1,
        "The pie example starts with one selected cube",
    )?;
    let document = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    let tool = s.state.editor.tool;
    let dirty = s.state.is_dirty();
    let anchor = s.state.viewport.center();

    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;

    let before = matrix(s);
    open(s, anchor)?;
    for (control, x, y) in [
        (Control::PieTop, 0, -1),
        (Control::PieFront, -1, -1),
        (Control::PieBack, 1, -1),
        (Control::PieLeft, -1, 0),
        (Control::PieRight, 1, 0),
        (Control::PieSelection, 1, 1),
        (Control::PieBottom, 0, 1),
    ] {
        s.witness(control)?;
        let position = s.trace.get(control)?.rect.center() - anchor;
        let in_direction = |coordinate: f32, sign: i32| match sign {
            0 => coordinate.abs() < 1.0,
            -1 => coordinate < 0.0,
            _ => coordinate > 0.0,
        };
        s.require(
            in_direction(position.x, x) && in_direction(position.y, y),
            "Every live pie choice occupies its documented compass position",
        )?;
    }
    hover_choice(s, Control::PieFront)?;
    s.require(
        matrix(s) == before && !s.state.camera.is_transitioning(),
        "Hovering a pie choice while the view-pie key is held does not change the camera",
    )?;
    s.capture_tutorial("view-pie-open")?;
    s.frame(vec![egui::Event::PointerMoved(anchor)], Duration::ZERO)?;
    release(s)?;
    s.require(
        matrix(s) == before,
        "Returning to the opening point before release cancels without changing the view",
    )?;

    open(s, anchor)?;
    let southwest = egui::pos2(
        s.trace.get(Control::PieLeft)?.rect.center().x,
        s.trace.get(Control::PieBottom)?.rect.center().y,
    );
    s.frame(vec![egui::Event::PointerMoved(southwest)], Duration::ZERO)?;
    release(s)?;
    s.require(
        matrix(s) == before,
        "The empty southwest slot has no camera action",
    )?;

    open(s, anchor)?;
    hover_choice(s, Control::PieRight)?;
    s.frame(vec![s.shortcut_event("cancel", true)?], Duration::ZERO)?;
    s.require(
        !s.state.view_pie_active()
            && matrix(s) == before
            && s.state.editor.selected_objects == selected,
        "Cancel cancels the pie without changing the camera or deselecting the object",
    )?;
    s.frame(vec![s.shortcut_event("cancel", false)?], Duration::ZERO)?;
    release(s)?;
    s.require(
        matrix(s) == before,
        "Releasing the view-pie key after Cancel cannot commit the cancelled choice",
    )?;

    for (control, direction) in [
        (Control::PieTop, Vec3::Y),
        (Control::PieFront, Vec3::Z),
        (Control::PieBack, Vec3::NEG_Z),
        (Control::PieLeft, Vec3::NEG_X),
        (Control::PieRight, Vec3::X),
        (Control::PieBottom, Vec3::NEG_Y),
    ] {
        let before = matrix(s);
        open(s, anchor)?;
        hover_choice(s, control)?;
        s.require(
            matrix(s) == before,
            "A direction choice waits for the held key to be released",
        )?;
        release(s)?;
        s.frame(
            Vec::new(),
            Duration::from_millis(s.state.view_duration_ms.into()),
        )?;
        s.require(
            !s.state.camera.is_transitioning()
                && s.state.camera.is_orthographic()
                && s.state.camera.direction_in_view(direction).abs_diff_eq(Vec3::Z, 1e-5),
            "Releasing over the choice reaches its named orthographic direction with the current transition settings",
        )?;
    }

    // Begin from a displaced view so the selection-fit action must recenter it.
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(60.0, -70.0);
    let end = start + egui::vec2(95.0, -40.0);
    let middle = |position, pressed| egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Middle,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(vec![middle(start, true)], Duration::ZERO)?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    s.frame(vec![middle(end, false)], Duration::ZERO)?;
    let points = s.state.editor.selection_points(s.state.z_up)?;
    let (min, max) = points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    );
    let center = (min + max) * 0.5;
    s.require(
        matrix(s).project_point3(center).truncate().length() > 0.01,
        "The real middle-button pan leaves the selected cube away from screen center",
    )?;
    let direction = s.state.camera.direction_in_view(Vec3::Z);
    open(s, anchor)?;
    hover_choice(s, Control::PieSelection)?;
    release(s)?;
    s.require(
        matrix(s).project_point3(center).truncate().length() < 1e-5
            && points.iter().all(|point| {
                let point = matrix(s).project_point3(*point);
                point.x.abs() < 1.0 && point.y.abs() < 1.0 && (0.0..=1.0).contains(&point.z)
            })
            && s.state.camera.is_orthographic()
            && s.state
                .camera
                .direction_in_view(Vec3::Z)
                .abs_diff_eq(direction, 1e-5),
        "The southeast choice fits the selected cube while retaining direction and projection",
    )?;

    s.require(
        s.state.editor.selected_objects == selected && s.state.editor.tool == tool,
        "Using the pie preserves the selected object and persistent tool",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.selected_vertices.is_empty(),
        "Vertex edit mode starts with no selected components",
    )?;
    disabled_selection(s, anchor)?;
    s.shortcut("selection.next")?;
    let points = s.state.editor.selection_points(s.state.z_up)?;
    let vertices = s.state.editor.selected_vertices.clone();
    s.require(
        points.len() == 1,
        "The vertex pie example selects one visible vertex",
    )?;
    open(s, anchor)?;
    hover_choice(s, Control::PieSelection)?;
    release(s)?;
    s.require(
        matrix(s).project_point3(points[0]).truncate().length() < 1e-5
            && s.state.editor.edit_mode
            && s.state.editor.selected_vertices == vertices
            && s.state.editor.selected_objects == selected,
        "In vertex edit mode the pie frames the selected component and retains that selection",
    )?;
    s.shortcut("edit.leave")?;
    s.shortcut("cancel")?;
    s.require(
        !s.state.editor.edit_mode && s.state.editor.selected_objects.is_empty(),
        "The final object-mode example has no selection",
    )?;
    disabled_selection(s, anchor)?;

    s.right_click(Control::Viewport)?;
    let before = matrix(s);
    s.frame(vec![s.shortcut_event("view.pie", true)?], Duration::ZERO)?;
    s.require(
        !s.state.view_pie_active() && matrix(s) == before,
        "An existing viewport popup keeps the view-pie key and prevents the pie from opening",
    )?;
    s.frame(vec![s.shortcut_event("view.pie", false)?], Duration::ZERO)?;
    s.shortcut("cancel")?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.selected_objects.is_empty()
            && s.state.editor.tool == tool
            && s.state.is_dirty() == dirty,
        "Using or cancelling the pie creates no document undo step or dirty-state change",
    )?;
    Ok(())
}
