use super::{Result, Session, pointer, pointer_with_modifiers};
use crate::{
    controls::Control,
    doc_harness::ClipSpec,
    document::{Document, PrimitiveKind},
    editor::Editor,
    object_feedback::{HOVERED_COLOR, HOVERED_WIDTH, SELECTED_COLOR, SELECTED_WIDTH, color},
    orientation::display_rotation,
};
use glam::DVec3;
use std::{collections::BTreeSet, time::Duration};

fn hover_at(s: &mut Session<'_>, position: egui::Pos2) -> Result<()> {
    s.frame(vec![egui::Event::PointerMoved(position)], Duration::ZERO)?;
    s.settle()
}

fn row(s: &Session<'_>, id: u64) -> Result<egui::Pos2> {
    s.state
        .layer_row_rect(id)
        .filter(|rect| rect.is_positive())
        .map(|rect| rect.center())
        .ok_or_else(|| format!("Object {id} has no visible Layers row"))
}

fn surface(s: &Session<'_>, id: u64) -> Result<egui::Pos2> {
    let object = s
        .state
        .editor
        .document
        .objects
        .iter()
        .find(|object| object.id == id)
        .ok_or_else(|| format!("Object {id} is missing"))?;
    let center = s
        .state
        .editor
        .frame
        .world_to_display(DVec3::from_array(object.transform.translation));
    let matrix = (s.state.camera.view_projection(s.state.aspect())
        * display_rotation(s.state.z_up))
    .as_dmat4();
    let clip = matrix * center.extend(1.0);
    let ndc = clip.truncate() / clip.w;
    let viewport = s.state.viewport;
    let point = egui::pos2(
        viewport.left() + (ndc.x as f32 + 1.0) * 0.5 * viewport.width(),
        viewport.top() + (1.0 - ndc.y as f32) * 0.5 * viewport.height(),
    );
    if clip.w <= 0.0
        || !viewport.shrink(10.0).contains(point)
        || crate::axis_gizmo::bounds(viewport).contains(point)
    {
        return Err("The tutorial object's surface target is outside the usable viewport".into());
    }
    Ok(point)
}

fn animated_box_selection(
    s: &mut Session<'_>,
    bounds: egui::Rect,
    left: u64,
    both: &BTreeSet<u64>,
) -> Result<()> {
    s.click(Control::ToolView)?;
    s.click_at(row(s, left)?)?;
    hover_at(s, bounds.min)?;
    let document = s.state.editor.document.clone();
    let camera = s.state.camera.view_projection(s.state.aspect());
    s.capture_clip(
        "object-feedback-box-drag-and-release",
        ClipSpec::default(),
        |s| {
            s.callout(
                Control::ToolView,
                "Drag a box; selection waits until release.",
            )?;
            s.wait(Duration::from_millis(400))?;
            s.pointer_button(egui::PointerButton::Primary, true)?;
            s.wait(Duration::from_millis(200))?;
            s.move_pointer(bounds.max, Duration::from_millis(1200))?;
            s.require(
                s.state.editor.is_interacting()
                    && s.state.editor.selected_objects == BTreeSet::from([left])
                    && s.state.editor.hovered_object.is_none()
                    && s.state.object_highlights().hovered.is_none(),
                "The animated box holds the original selection and hides competing hover feedback",
            )?;
            s.wait(Duration::from_millis(500))?;
            s.pointer_button(egui::PointerButton::Primary, false)?;
            s.require(
                !s.state.editor.is_interacting()
                    && s.state.editor.selected_objects == *both
                    && s.state.object_highlights().selected == *both,
                "Releasing the animated box selects both intersected objects through the real UI",
            )?;
            s.callout(
                Control::ToolView,
                "Release to select the intersected objects.",
            )?;
            s.wait(Duration::from_millis(800))?;
            s.clear_callout()
        },
    )?;
    s.require(
        s.state.editor.document == document
            && s.state.camera.view_projection(s.state.aspect()) == camera,
        "The animated selection gesture changes no geometry and does not navigate the camera",
    )
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    // Scene setup supplies two separated meshes. Every subsequent selection and
    // hover comes from the real UI, using live row rectangles or projected geometry.
    let mut document = Document::default();
    let left = document.insert_primitive(PrimitiveKind::Cube)?;
    let right = document.insert_primitive(PrimitiveKind::Cube)?;
    for (id, name, x) in [(left, "Left cube", -2.3), (right, "Right cube", 2.3)] {
        let object = document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap();
        object.name = name.into();
        object.transform.translation[0] = x;
        document.convert_object(id)?;
    }
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let dirty = s.state.is_dirty();
    s.witness(Control::ObjectList)?;
    s.witness(Control::Viewport)?;
    let left_row = row(s, left)?;
    let right_row = row(s, right)?;
    let right_surface = surface(s, right)?;

    hover_at(s, left_row)?;
    let highlights = s.state.object_highlights();
    s.require(
        highlights.selected.is_empty()
            && highlights.hovered == Some(left)
            && s.state.editor.selected_object.is_none()
            && s.state.layer_row_color(left) == Some(HOVERED_COLOR),
        "Hovering an unselected Layers row highlights its object and row without selecting it",
    )?;
    s.click_at(left_row)?;
    let highlights = s.state.object_highlights();
    s.require(
        highlights.selected == BTreeSet::from([left])
            && highlights.hovered == Some(left)
            && s.state.editor.selected_object == Some(left)
            && s.state.layer_row_color(left) == Some(SELECTED_COLOR)
            && color(true, true) == Some(SELECTED_COLOR)
            && SELECTED_WIDTH > HOVERED_WIDTH,
        "Clicking a Layers row selects its object; the stronger selected style takes precedence over hover on that object",
    )?;

    hover_at(s, right_row)?;
    let highlights = s.state.object_highlights();
    s.require(
        highlights.selected == BTreeSet::from([left])
            && highlights.hovered == Some(right)
            && s.state.layer_row_color(left) == Some(SELECTED_COLOR)
            && s.state.layer_row_color(right) == Some(HOVERED_COLOR),
        "A selected object and a different hovered Layers object keep their distinct highlights simultaneously",
    )?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("object-feedback-layers")?;

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Edges])?;
    s.require(
        !s.state.show_edges && s.state.editor.selected_object == Some(left),
        "Turning Edges off hides the polygon overlay without changing object selection",
    )?;
    hover_at(s, right_surface)?;
    let highlights = s.state.object_highlights();
    s.require(
        highlights.selected == BTreeSet::from([left])
            && highlights.hovered == Some(right)
            && s.state.layer_row_color(left) == Some(SELECTED_COLOR)
            && s.state.layer_row_color(right) == Some(HOVERED_COLOR),
        "Hovering the right cube's visible surface highlights its matching Layers row while both object highlights remain independent of Edges",
    )?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("object-feedback-viewport")?;
    s.click_at(right_surface)?;
    s.require(
        s.state.editor.selected_object == Some(right)
            && s.state.layer_row_color(right) == Some(SELECTED_COLOR),
        "Clicking a visible surface selects the same object represented by its Layers row",
    )?;
    s.click_at(left_row)?;
    s.require(
        s.state.editor.selected_object == Some(left),
        "Clicking the other Layers row changes selection back to its object",
    )?;
    let empty = s.empty_viewport_point()?;
    hover_at(s, empty)?;
    let highlights = s.state.object_highlights();
    s.require(
        highlights.selected == BTreeSet::from([left])
            && highlights.hovered.is_none()
            && s.state.editor.hovered_object.is_none()
            && s.state.layer_row_color(right).is_none(),
        "Moving over empty viewport clears hover while retaining the selected object's highlight",
    )?;

    let eligible =
        s.state
            .editor
            .object_selection_bounds(s.state.viewport, &s.state.camera, s.state.z_up)?;
    let both = BTreeSet::from([left, right]);
    s.require(
        eligible.iter().map(|(id, _)| *id).collect::<BTreeSet<_>>() == both,
        "Both tutorial cubes have a visible vertex and projected bounds eligible for object box selection",
    )?;
    let combined = eligible
        .iter()
        .fold(egui::Rect::NOTHING, |bounds, (_, rect)| bounds.union(*rect));
    let bounds = egui::Rect::from_min_max(
        combined.min + egui::vec2(12.0, -12.0),
        combined.max + egui::vec2(-12.0, 12.0),
    );
    s.require(
        s.state.viewport.contains(bounds.min)
            && s.state.viewport.contains(bounds.max)
            && eligible
                .iter()
                .all(|(_, rect)| bounds.intersects(*rect) && !bounds.contains_rect(*rect)),
        "The tutorial box crosses both objects without fully containing either projected bounds",
    )?;
    animated_box_selection(s, bounds, left, &both)?;
    let camera = s.state.camera.view_projection(s.state.aspect());
    for tool in [
        Control::ToolView,
        Control::ToolMove,
        Control::ToolRotate,
        Control::ToolScale,
    ] {
        s.click_at(left_row)?;
        s.click(tool)?;
        hover_at(s, bounds.min)?;
        s.frame(vec![pointer(bounds.min, true)], Duration::ZERO)?;
        s.frame(
            vec![egui::Event::PointerMoved(right_surface)],
            Duration::ZERO,
        )?;
        s.require(
            s.state.editor.is_interacting()
                && s.state.editor.hovered_object.is_none()
                && s.state.object_highlights().hovered.is_none()
                && !s.state.editor.selected_objects.contains(&right)
                && s.state.layer_row_color(right).is_none(),
            "A held selection box suppresses hover over a visible unselected cube and its Layers row",
        )?;
        s.frame(vec![egui::Event::PointerMoved(bounds.max)], Duration::ZERO)?;
        let highlights = s.state.object_highlights();
        s.require(
            s.state.editor.is_interacting()
                && s.state.editor.selected_objects == BTreeSet::from([left])
                && s.state.editor.selected_object == Some(left)
                && highlights.selected == BTreeSet::from([left])
                && highlights.hovered.is_none()
                && s.state.editor.hovered_object.is_none()
                && s.state.layer_row_color(left) == Some(SELECTED_COLOR)
                && s.state.layer_row_color(right).is_none()
                && s.state.camera.view_projection(s.state.aspect()) == camera,
            "A held box preserves the original selection and suppresses hover in every tool without moving the camera",
        )?;
        if tool == Control::ToolView {
            s.capture_tutorial("object-feedback-box-selection")?;
        }
        s.frame(vec![pointer(bounds.max, false)], Duration::ZERO)?;
        s.settle()?;
        s.require(
            !s.state.editor.is_interacting()
                && s.state.editor.selected_objects == both
                && s.state.object_highlights().selected == both
                && s.state.layer_row_color(left) == Some(SELECTED_COLOR)
                && s.state.layer_row_color(right) == Some(SELECTED_COLOR),
            "Releasing the box selects both intersected visible objects and highlights their rows and outlines",
        )?;
        if tool == Control::ToolView {
            s.capture_tutorial("object-feedback-box-result")?;
        }
        hover_at(s, right_surface)?;
        s.require(
            s.state.editor.hovered_object == Some(right)
                && s.state.object_highlights().hovered == Some(right),
            "Hover feedback resumes over visible geometry after the selection box is released",
        )?;
    }
    let active = s.state.editor.selected_object;
    let end = empty + egui::vec2(22.0, -22.0);
    hover_at(s, empty)?;
    s.frame(vec![pointer(empty, true)], Duration::ZERO)?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    s.require(
        s.state.editor.is_interacting()
            && s.state.editor.selected_objects == both
            && s.state.editor.selected_object == active,
        "An unfinished empty-space box preserves the selection and active object until it is accepted",
    )?;
    s.shortcut("cancel")?;
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.settle()?;
    s.require(
        !s.state.editor.is_interacting()
            && s.state.editor.selected_objects == both
            && s.state.editor.selected_object == active
            && s.state.editor.document == document,
        "Cancel cancels an object box and preserves the complete previous selection and active object",
    )?;

    // Shift adds only when accepted; crossing part of the right object is enough.
    let right_bounds = eligible.iter().find(|(id, _)| *id == right).unwrap().1;
    let partial_right = egui::Rect::from_min_max(
        egui::pos2(right_bounds.center().x, right_bounds.top() - 12.0),
        right_bounds.max + egui::vec2(12.0, 12.0),
    );
    s.require(
        partial_right.intersects(right_bounds)
            && !partial_right.contains_rect(right_bounds)
            && eligible
                .iter()
                .all(|(id, rect)| *id == right || !partial_right.intersects(*rect)),
        "The additive tutorial box intersects only the right cube without containing it",
    )?;
    s.click_at(left_row)?;
    hover_at(s, partial_right.min)?;
    s.frame(
        vec![pointer_with_modifiers(
            partial_right.min,
            true,
            egui::Modifiers::SHIFT,
        )],
        Duration::ZERO,
    )?;
    s.frame(
        vec![egui::Event::PointerMoved(partial_right.max)],
        Duration::ZERO,
    )?;
    s.require(
        s.state.editor.is_interacting()
            && s.state.editor.selected_objects == BTreeSet::from([left]),
        "A held Shift-box preserves the original object selection",
    )?;
    s.frame(
        vec![pointer_with_modifiers(
            partial_right.max,
            false,
            egui::Modifiers::SHIFT,
        )],
        Duration::ZERO,
    )?;
    s.settle()?;
    s.require(
        !s.state.editor.is_interacting() && s.state.editor.selected_objects == both,
        "Releasing a Shift-box adds the intersected object to the previous selection",
    )?;

    hover_at(s, partial_right.min)?;
    s.frame(vec![pointer(partial_right.min, true)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerMoved(partial_right.max)],
        Duration::ZERO,
    )?;
    s.require(
        s.state.editor.is_interacting() && s.state.editor.selected_objects == both,
        "A replacement box leaves the multiple selection unchanged while held",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        !s.state.editor.is_interacting()
            && !s.state.editor.edit_mode
            && s.state.editor.selected_objects == BTreeSet::from([right]),
        "Confirm explicitly accepts the intersecting box without entering vertex editing",
    )?;
    s.frame(vec![pointer(partial_right.max, false)], Duration::ZERO)?;
    s.settle()?;
    s.require(
        s.state.editor.selected_objects == BTreeSet::from([right]),
        "Releasing the pointer after Confirm preserves the accepted box selection",
    )?;
    s.click_at(left_row)?;
    s.require(
        s.state.editor.selected_objects == BTreeSet::from([left]),
        "A plain Layers click replaces the multiple selection with that single object",
    )?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.edit_mode,
        "Confirm switches the selected mesh into vertex mode",
    )?;
    for position in [right_row, right_surface] {
        hover_at(s, position)?;
        let highlights = s.state.object_highlights();
        s.require(
            highlights.selected.is_empty()
                && highlights.hovered.is_none()
                && s.state.editor.selected_object == Some(left)
                && s.state.layer_row_color(left) == Some(SELECTED_COLOR)
                && s.state.layer_row_color(right).is_none(),
            "Vertex mode suppresses object outlines and hover feedback while retaining the edited object and its selected Layers row",
        )?;
    }
    for redo in [false, true] {
        if redo {
            s.redo()?;
        } else {
            s.undo()?;
        }
        s.require(
            s.state.editor.document == document
                && s.state.is_dirty() == dirty
                && s.state.editor.edit_mode
                && s.state.editor.selected_object == Some(left),
            "Hover, selection, display toggles and mode changes do not alter geometry, saved state or document history",
        )?;
    }
    Ok(())
}
