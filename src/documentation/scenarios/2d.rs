use super::{Result, Session};
use crate::{
    controls::Control,
    document::{Geometry, PrimitiveKind},
};
use std::time::Duration;

fn type_value(s: &mut Session<'_>, control: Control, text: &str) -> Result<()> {
    s.click(control)?;
    s.shortcut("selection.all")?;
    s.frame(vec![egui::Event::Text(text.into())], Duration::ZERO)?;
    s.shortcut("edit.confirm")?;
    Ok(())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.require(
        !s.state.is_planar_navigation()
            && s.state.ruler_2d_model.is_none()
            && s.trace.controls.contains_key(&Control::EmptyStateInsert),
        "The new document begins in 3D without a 2D ruler",
    )?;
    s.click(Control::NavigationPlanar)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.require(
        s.state.is_planar_navigation()
            && s.state.camera.is_orthographic()
            && s.state.ruler_2d_model.is_some()
            && s.state.editor.document.objects.is_empty()
            && !s.trace.controls.contains_key(&Control::EmptyStateInsert),
        "Entering 2D shows its ruler and keeps an empty canvas clear",
    )?;
    s.capture_image("2d-empty")?;
    s.click(Control::InsertMenu)?;
    s.click(Control::InsertCircle)?;
    let id = s
        .state
        .editor
        .selected_object
        .ok_or("The Circle was not selected")?;
    let original = s.state.editor.document.clone();
    let evaluated = original.eval_object(id)?;
    s.require(
        s.state.is_planar_navigation()
            && matches!(&original.objects[0].geometry, Geometry::Primitive(p)
                if p.kind == PrimitiveKind::Circle && !p.fill && p.segments == 32 && p.size == [2., 2., 1.])
            && evaluated.vertices.len() == 32
            && evaluated.edges.len() == 32
            && evaluated.faces.is_empty(),
        "Circle starts as an unfilled 32-vertex loop of radius 1 cm and keeps the 2D view",
    )?;
    s.witness(Control::CircleRadius)?;
    s.witness(Control::CircleVertices)?;
    s.witness(Control::CircleFill)?;
    s.witness(Control::Ruler2DHorizontal)?;
    s.witness(Control::Ruler2DVertical)?;
    s.capture_image("2d-overview")?;

    type_value(s, Control::CircleRadius, "1.5")?;
    s.require(
        matches!(&s.state.editor.document.objects[0].geometry, Geometry::Primitive(p)
            if p.kind == PrimitiveKind::Circle && p.size == [3., 3., 1.] && !p.fill && p.segments == 32),
        "Radius changes the live Circle without changing its fill or vertex count",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == original,
        "One Undo restores the radius",
    )?;
    type_value(s, Control::CircleVertices, "24")?;
    s.require(
        matches!(&s.state.editor.document.objects[0].geometry, Geometry::Primitive(p)
            if p.kind == PrimitiveKind::Circle && p.segments == 24 && !p.fill && p.size == [2., 2., 1.])
            && s.state.editor.document.eval_object(id)?.edges.len() == 24,
        "Vertices changes the closed loop's segment count while retaining the Circle recipe",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == original,
        "One Undo restores the vertex count",
    )?;

    s.click(Control::CircleFill)?;
    let filled = s.state.editor.document.eval_object(id)?;
    s.require(
        matches!(&s.state.editor.document.objects[0].geometry, Geometry::Primitive(p)
            if p.kind == PrimitiveKind::Circle && p.fill && p.segments == 32 && p.size == [2., 2., 1.])
            && filled.vertices == evaluated.vertices
            && filled.faces.len() == 1
            && filled.edges.is_empty(),
        "Fill adds one polygon without replacing the Circle or its vertices",
    )?;
    s.capture_image("2d-circle-filled")?;
    s.click(Control::CircleFill)?;
    s.require(
        s.state.editor.document == original,
        "Turning Fill off restores the same unfilled Circle",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document.eval_object(id)?.faces.len() == 1,
        "Undo restores Fill",
    )?;
    s.undo()?;
    s.require(
        s.state.editor.document == original,
        "A second Undo returns to the original unfilled recipe",
    )?;

    let visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    s.require(
        visible.len() == 32,
        "Every unfilled Circle vertex is available for editing",
    )?;
    let center = (visible
        .iter()
        .fold(egui::Vec2::ZERO, |sum, (_, p)| sum + p.to_vec2())
        / visible.len() as f32)
        .to_pos2();
    let outline = visible
        .iter()
        .max_by(|a, b| a.1.x.total_cmp(&b.1.x))
        .ok_or("Circle has no outline")?
        .1;
    s.click_at(center)?;
    s.require(
        s.state.editor.selected_objects.is_empty(),
        "The unfilled Circle's empty center does not select the object",
    )?;
    s.frame(Vec::new(), Duration::from_millis(500))?;
    s.click_at(outline)?;
    s.require(
        s.state.editor.selected_object == Some(id),
        "Clicking the outline selects the Circle",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.edit_mode && s.state.editor.document == original,
        "Entering vertex edit mode preserves the Circle recipe",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        !s.state.editor.edit_mode && s.state.editor.document == original,
        "Leaving without a vertex edit retains Radius, Vertices, and Fill",
    )?;
    Ok(())
}
