use super::{Result, Session};
use crate::{
    controls::Control,
    doc_harness::ClipSpec,
    document::{Document, PrimitiveKind},
    editor::Editor,
    orientation::display_rotation,
    ruler_2d::Ruler2DModel,
};
use glam::{DQuat, DVec3};
use std::{collections::BTreeSet, time::Duration};

fn model<'s>(s: &'s Session<'_>) -> Result<&'s Ruler2DModel> {
    s.state
        .ruler_2d_model
        .as_ref()
        .ok_or_else(|| "The aligned view has no ruler model".into())
}

fn screen(s: &Session<'_>, world: DVec3) -> egui::Pos2 {
    let display = s.state.editor.frame.world_to_display(world).as_vec3();
    let matrix = s.state.camera.view_projection(s.state.aspect()) * display_rotation(s.state.z_up);
    let point = matrix.project_point3(display);
    egui::pos2(
        s.state.viewport.left() + (point.x + 1.0) * s.state.viewport.width() * 0.5,
        s.state.viewport.top() + (1.0 - point.y) * s.state.viewport.height() * 0.5,
    )
}

fn verify_scale(s: &mut Session<'_>) -> Result<()> {
    let ruler = model(s)?;
    let origin = screen(s, DVec3::ZERO);
    let units = [DVec3::X, DVec3::Y, DVec3::Z].map(|axis| screen(s, axis) - origin);
    let centimeters_per_unit = s.state.display_unit.to_centimeters(1.0);
    let horizontal =
        f64::from(units.iter().map(|unit| unit.x.abs()).fold(0.0, f32::max)) * centimeters_per_unit;
    let vertical =
        f64::from(units.iter().map(|unit| unit.y.abs()).fold(0.0, f32::max)) * centimeters_per_unit;
    let correct = ruler.unit == s.state.display_unit
        && (ruler.horizontal.origin - f64::from(origin.x)).abs() < 0.05
        && (ruler.vertical.origin - f64::from(origin.y)).abs() < 0.05
        && (ruler.horizontal.pixels_per_unit - horizontal).abs() < 0.05
        && (ruler.vertical.pixels_per_unit - vertical).abs() < 0.05
        && [&ruler.horizontal, &ruler.vertical]
            .into_iter()
            .all(|axis| {
                axis.pixels_per_unit > 0.0
                    && axis
                        .ticks
                        .iter()
                        .filter(|tick| tick.label.is_some())
                        .count()
                        >= 2
                    && axis.ticks.windows(2).all(|pair| {
                        pair[0].position < pair[1].position && pair[0].value < pair[1].value
                    })
                    && axis.ticks.iter().all(|tick| {
                        (f64::from(tick.position) - axis.origin - tick.value * axis.pixels_per_unit)
                            .abs()
                            < 0.05
                    })
            });
    s.require(correct, "Ruler zero matches the projected centimeter origin, ticks use the chosen display unit, and numbers increase rightward and downward")
}

fn bounds(s: &Session<'_>, points: impl IntoIterator<Item = DVec3>) -> egui::Rect {
    points
        .into_iter()
        .fold(egui::Rect::NOTHING, |mut rect, point| {
            rect.extend_with(screen(s, point));
            rect
        })
}

fn object_bounds(s: &Session<'_>, id: u64) -> Result<egui::Rect> {
    let object = s
        .state
        .editor
        .document
        .objects
        .iter()
        .find(|object| object.id == id)
        .ok_or("The ruler example object is missing")?;
    let mesh = s.state.editor.document.eval_object(id)?;
    let transform = object.transform.matrix();
    Ok(bounds(
        s,
        mesh.vertices
            .iter()
            .map(|vertex| transform.transform_point3(DVec3::from_array(vertex.position))),
    ))
}

fn verify_ranges(s: &mut Session<'_>, boxes: &[egui::Rect]) -> Result<()> {
    let ruler = model(s)?;
    let mut horizontal: Vec<_> = boxes
        .iter()
        .map(|rect| (rect.left(), rect.right()))
        .collect();
    let mut vertical: Vec<_> = boxes
        .iter()
        .map(|rect| (rect.top(), rect.bottom()))
        .collect();
    horizontal.sort_by(|a, b| a.0.total_cmp(&b.0));
    vertical.sort_by(|a, b| a.0.total_cmp(&b.0));
    let correct = [&ruler.horizontal, &ruler.vertical]
        .into_iter()
        .zip([horizontal, vertical])
        .all(|(axis, expected)| {
            axis.ranges.len() == expected.len()
                && axis
                    .ranges
                    .iter()
                    .zip(expected)
                    .all(|(span, (start, end))| {
                        (span.start - start).abs() < 0.1 && (span.end - end).abs() < 0.1
                    })
        });
    s.require(correct, "Ruler highlights match the selected geometry's projected bounds and preserve the gaps between separate objects")
}

fn finish_view(s: &mut Session<'_>) -> Result<()> {
    s.wait(Duration::from_millis(s.state.view_duration_ms.into()))?;
    s.settle()
}

fn view(s: &mut Session<'_>, binding: &str) -> Result<()> {
    s.shortcut(binding)?;
    finish_view(s)
}

fn layout(s: &Session<'_>) -> Result<[egui::Rect; 4]> {
    Ok([
        s.state.viewport,
        s.state.viewport_ui_rect,
        s.trace.get(Control::ViewportToolbar)?.rect,
        s.trace.get(Control::Gizmo)?.rect,
    ])
}

fn verify_axis_transition(s: &mut Session<'_>, before: [egui::Rect; 4]) -> Result<()> {
    s.require(
        layout(s)? == before,
        "An axis request preserves the 2D layout immediately",
    )?;
    for _ in 0..4 {
        s.wait(Duration::from_millis(
            u64::from(s.state.view_duration_ms) / 4,
        ))?;
        s.require(
            layout(s)? == before && s.state.ruler_2d_model.is_some(),
            "The ruler, viewport, toolbar and gizmo stay in place throughout a 2D axis transition",
        )?;
        let origin = screen(s, DVec3::ZERO);
        s.require(
            (model(s)?.horizontal.origin - f64::from(origin.x)).abs() < 0.05
                && (model(s)?.vertical.origin - f64::from(origin.y)).abs() < 0.05,
            "The animated ruler zero follows the live scene projection",
        )?;
    }
    finish_view(s)
}

fn drag(s: &mut Session<'_>, button: egui::PointerButton, delta: egui::Vec2) -> Result<()> {
    let start = s.state.viewport_ui_rect.left_bottom() + egui::vec2(40.0, -65.0);
    let event = |pos, pressed| egui::Event::PointerButton {
        pos,
        button,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.frame(vec![event(start, true)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerMoved(start + delta)],
        Duration::ZERO,
    )?;
    s.frame(vec![event(start + delta, false)], Duration::ZERO)?;
    s.settle()
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    let mut document = Document::default();
    let first = document.insert_primitive(PrimitiveKind::Cube)?;
    let second = document.insert_primitive(PrimitiveKind::Cube)?;
    for (id, name, x, y) in [
        (first, "Lower cube", -3.0, -1.8),
        (second, "Upper cube", 3.0, 1.8),
    ] {
        let object = document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap();
        object.name = name.into();
        object.transform.translation = [x, y, 0.0];
        if id == first {
            object.transform.rotation = DQuat::from_rotation_z(0.25).to_array();
        }
        document.convert_object(id)?;
    }
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.witness(Control::Viewport)?;
    let dirty = s.state.is_dirty();
    s.require(
        s.state.ruler_2d_model.is_none(),
        "Perspective starts without a 2D ruler",
    )?;
    s.shortcut("view.2d-ruler")?;
    s.require(
        s.state.show_2d_ruler && s.state.ruler_2d_model.is_none(),
        "The 2D ruler shortcut has no effect in 3D mode",
    )?;
    let empty = s.state.viewport_ui_rect.left_bottom() + egui::vec2(18.0, -18.0);
    s.click_at(empty)?;
    s.shortcut("view.front")?;
    s.require(
        s.state.camera.is_transitioning()
            && s.state.ruler_2d_model.is_none()
            && s.trace.get(Control::Ruler2DHorizontal).is_err()
            && s.trace.get(Control::Ruler2DVertical).is_err(),
        "Rulers stay hidden while the view animates toward Front",
    )?;
    finish_view(s)?;
    for binding in [
        "view.front",
        "view.back",
        "view.right",
        "view.left",
        "view.top",
        "view.bottom",
    ] {
        let before = layout(s)?;
        s.shortcut(binding)?;
        verify_axis_transition(s, before)?;
        let matrix = s.state.camera.view_projection(s.state.aspect());
        s.shortcut(binding)?;
        s.require(
            !s.state.camera.is_transitioning()
                && s.state.camera.view_projection(s.state.aspect()) == matrix
                && layout(s)? == before,
            "Requesting the current axis again does not start an animation or change layout",
        )?;
        s.require(
            model(s)?.horizontal.ranges.is_empty() && model(s)?.vertical.ranges.is_empty(),
            "Every settled cardinal orthographic view has ticks but no highlighted ranges without selection",
        )?;
        verify_scale(s)?;
    }
    view(s, "view.front")?;
    let before = layout(s)?;
    s.capture_clip("2d-ruler-axis-transition", ClipSpec::default(), |s| {
        s.wait(Duration::from_millis(300))?;
        s.shortcut("numpad.top")?;
        s.wait(Duration::from_millis(
            u64::from(s.state.view_duration_ms) / 2,
        ))?;
        let token = s.state.camera.transition_token();
        s.shortcut("numpad.top")?;
        s.require(
            token.is_some() && s.state.camera.transition_token() == token,
            "Repeating a pending numpad view keeps the original animation",
        )?;
        s.shortcut("numpad.right")?;
        s.wait(Duration::from_millis(
            u64::from(s.state.view_duration_ms) / 2,
        ))?;
        s.require(
            layout(s)? == before && s.state.ruler_2d_model.is_some(),
            "Retargeting an animated 2D view keeps the ruler and inset",
        )?;
        s.hover(Control::AxisY)?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.wait(Duration::from_millis(60))?;
        s.require(
            layout(s)? == before && s.state.ruler_2d_model.is_some(),
            "Holding an animated gizmo handle preserves the 2D ruler and inset",
        )?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        verify_axis_transition(s, before)?;
        s.click(Control::AxisY)?;
        verify_axis_transition(s, before)?;
        s.require(
            s.state.camera.direction_in_view(glam::Vec3::NEG_Y).z > 0.9999,
            "The facing gizmo axis retains its intentional opposite-side action",
        )?;
        s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::ViewFront])?;
        verify_axis_transition(s, before)?;
        s.wait(Duration::from_millis(400))?;
        Ok(())
    })?;
    s.shortcut("view.2d-ruler")?;
    let hidden = layout(s)?;
    s.shortcut("view.right")?;
    s.wait(Duration::from_millis(
        u64::from(s.state.view_duration_ms) / 2,
    ))?;
    s.require(
        layout(s)? == hidden && s.state.ruler_2d_model.is_none(),
        "A hidden ruler stays hidden during 2D axis changes",
    )?;
    finish_view(s)?;
    view(s, "view.front")?;
    s.shortcut("view.2d-ruler")?;
    s.witness(Control::Ruler2DHorizontal)?;
    s.witness(Control::Ruler2DVertical)?;
    s.require(
        model(s)?.horizontal.rect.bottom() <= s.state.viewport_ui_rect.top() + 0.1
            && model(s)?.vertical.rect.right() <= s.state.viewport_ui_rect.left() + 0.1
            && s.state.viewport.contains_rect(model(s)?.horizontal.rect)
            && s.state.viewport.contains_rect(model(s)?.vertical.rect),
        "The 2D ruler overlays the full scene while staying outside the viewport's interactive area",
    )?;

    let viewport = s.state.viewport;
    let camera = s.state.camera.view_projection(s.state.aspect());
    let projected_bounds = [object_bounds(s, first)?, object_bounds(s, second)?];
    let toolbar = s.trace.get(Control::ViewportToolbar)?.rect;
    let gizmo = s.trace.get(Control::Gizmo)?.rect;
    s.right_click(Control::Ruler2DHorizontal)?;
    s.require(
        s.trace.get(Control::Ruler2DHide)?.parents == [Control::Ruler2DHorizontal],
        "Right-clicking the horizontal 2D ruler opens its Hide command",
    )?;
    s.wait(Duration::from_millis(250))?;
    s.capture_image("2d-ruler-menu")?;
    s.click(Control::Ruler2DHide)?;
    s.require(
        !s.state.show_2d_ruler && s.state.ruler_2d_model.is_none(),
        "Hiding the 2D ruler clears its visible state and model",
    )?;
    s.require(
        s.trace.get(Control::Ruler2DHorizontal).is_err()
            && s.trace.get(Control::Ruler2DVertical).is_err(),
        "Hiding the 2D ruler removes both strips",
    )?;
    s.require(
        s.state.viewport == viewport
            && s.state.camera.view_projection(s.state.aspect()) == camera
            && [object_bounds(s, first)?, object_bounds(s, second)?] == projected_bounds
            && s.state.editor.document == document,
        "Hiding the 2D ruler preserves render bounds, projection, and the exact screen positions and sizes of objects",
    )?;
    s.require(
        s.state.viewport_ui_rect == viewport
            && s.trace.get(Control::ViewportToolbar)?.rect.min
                == toolbar.min - egui::Vec2::splat(crate::ruler_2d::THICKNESS)
            && s.trace.get(Control::Gizmo)?.rect.top() == gizmo.top() - crate::ruler_2d::THICKNESS,
        "Floating tools and gizmo move into the space released by hiding the 2D ruler",
    )?;
    s.capture_image("2d-ruler-hidden")?;
    s.shortcut("view.2d-ruler")?;
    s.require(
        s.state.show_2d_ruler
            && s.state.ruler_2d_model.is_some()
            && s.state.viewport == viewport
            && [object_bounds(s, first)?, object_bounds(s, second)?] == projected_bounds
            && s.trace.get(Control::ViewportToolbar)?.rect == toolbar
            && s.trace.get(Control::Gizmo)?.rect == gizmo,
        "Shift+R restores the 2D ruler and UI inset without moving or resizing geometry",
    )?;
    s.right_click(Control::Ruler2DVertical)?;
    s.require(
        s.trace.get(Control::Ruler2DHide)?.parents == [Control::Ruler2DVertical],
        "The vertical 2D ruler offers the same Hide command",
    )?;
    s.shortcut("cancel")?;
    s.click_path(&[
        Control::N3Menu,
        Control::ViewMenu,
        Control::Ruler2DViewToggle,
    ])?;
    s.require(
        !s.state.show_2d_ruler && s.state.ruler_2d_model.is_none(),
        "N3 View hides the 2D ruler through the same visibility action",
    )?;
    s.click_path(&[
        Control::N3Menu,
        Control::ViewMenu,
        Control::Ruler2DViewToggle,
    ])?;
    s.require(
        s.state.show_2d_ruler && s.state.ruler_2d_model.is_some(),
        "N3 View restores the 2D ruler",
    )?;
    s.shortcut("view.2d-ruler")?;
    s.click(Control::NavigationFree)?;
    s.require(
        !s.state.is_planar_navigation() && s.state.ruler_2d_model.is_none(),
        "Leaving 2D hides the 2D ruler",
    )?;
    s.click(Control::NavigationPlanar)?;
    finish_view(s)?;
    s.require(
        s.state.is_planar_navigation() && s.state.show_2d_ruler && s.state.ruler_2d_model.is_some(),
        "Entering 2D again restores the default visible 2D ruler",
    )?;

    let first_box = object_bounds(s, first)?;
    let second_box = object_bounds(s, second)?;
    let selection = first_box.union(second_box).expand(5.0);
    s.require(
        s.state.viewport.contains_rect(selection),
        "The ruler example's object-selection box fits within the viewport",
    )?;
    s.drag_at(selection.min, selection.size())?;
    let selected = BTreeSet::from([first, second]);
    s.require(
        s.state.editor.selected_objects == selected,
        "A real selection box selects both separated cubes",
    )?;
    verify_ranges(s, &[first_box, second_box])?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("2d-ruler-objects")?;

    let camera = s.state.camera.view_projection(s.state.aspect());
    for control in [Control::Ruler2DHorizontal, Control::Ruler2DVertical] {
        let point = s.trace.get(control)?.rect.center();
        s.click_at(point)?;
        s.require(
            s.state.camera.view_projection(s.state.aspect()) == camera
                && s.state.editor.selected_objects == selected
                && s.state.editor.document == document,
            "Clicking a read-only ruler does not navigate, select or edit the model",
        )?;
    }
    let origin = (model(s)?.horizontal.origin, model(s)?.vertical.origin);
    let scale = model(s)?.horizontal.pixels_per_unit;
    drag(s, egui::PointerButton::Middle, egui::vec2(20.0, -15.0))?;
    s.require(
        (model(s)?.horizontal.origin - origin.0 - 20.0).abs() < 0.1
            && (model(s)?.vertical.origin - origin.1 + 15.0).abs() < 0.1
            && (model(s)?.horizontal.pixels_per_unit - scale).abs() < 0.001,
        "A real pan moves ruler zero with the model while preserving document-unit scale",
    )?;
    verify_scale(s)?;
    let boxes = [object_bounds(s, first)?, object_bounds(s, second)?];
    verify_ranges(s, &boxes)?;
    // The existing shared wheel adapter verifies zoom behavior, not OS delivery.
    s.hover(Control::Viewport)?;
    s.scroll(0.0, 1.0, false, egui::Modifiers::NONE)?;
    s.settle()?;
    s.require(
        model(s)?.horizontal.pixels_per_unit > scale * 1.05,
        "Zooming increases screen distance per document unit and recomputes the rulers",
    )?;
    verify_scale(s)?;
    let boxes = [object_bounds(s, first)?, object_bounds(s, second)?];
    verify_ranges(s, &boxes)?;

    drag(s, egui::PointerButton::Secondary, egui::vec2(18.0, 10.0))?;
    s.require(
        s.state.camera.is_orthographic() && s.state.ruler_2d_model.is_none(),
        "Orbiting to an oblique orthographic view hides both rulers",
    )?;
    view(s, "view.perspective")?;
    s.require(
        s.state.ruler_2d_model.is_none(),
        "Perspective has no ruler strips",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    view(s, "view.front")?;

    let row = s
        .state
        .layer_row_rect(first)
        .ok_or("The first cube has no Layers row")?;
    s.click_at(row.center())?;
    s.shortcut("edit.confirm")?;
    s.require(
        s.state.editor.edit_mode
            && model(s)?.horizontal.ranges.is_empty()
            && model(s)?.vertical.ranges.is_empty(),
        "Entering vertex editing shows no ranges until vertices are selected",
    )?;
    let mut visible =
        s.state
            .editor
            .selectable_vertices(s.state.viewport, &s.state.camera, s.state.z_up)?;
    visible.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    s.require(
        visible.len() >= 4,
        "The rotated cube exposes four vertices for the ruler example",
    )?;
    let chosen: BTreeSet<_> = visible.iter().take(2).map(|(id, _)| *id).collect();
    let box_vertices = egui::Rect::from_two_pos(visible[0].1, visible[1].1).expand(4.0);
    s.drag_at(box_vertices.min, box_vertices.size())?;
    s.require(
        s.state.editor.selected_vertices == chosen,
        "A real box selects only two vertices on one side of the rotated cube",
    )?;
    let object = s
        .state
        .editor
        .document
        .objects
        .iter()
        .find(|object| object.id == first)
        .unwrap();
    let transform = object.transform.matrix();
    let mesh = s.state.editor.document.eval_object(first)?;
    let selected_bounds = bounds(
        s,
        mesh.vertices
            .iter()
            .filter(|vertex| chosen.contains(&vertex.id))
            .map(|vertex| transform.transform_point3(DVec3::from_array(vertex.position))),
    );
    verify_ranges(s, &[selected_bounds])?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.capture_tutorial("2d-ruler-vertices")?;
    s.shortcut("cancel")?;
    s.require(
        s.state.editor.edit_mode
            && model(s)?.horizontal.ranges.is_empty()
            && model(s)?.vertical.ranges.is_empty()
            && !model(s)?.horizontal.ticks.is_empty(),
        "Clearing vertex selection removes its highlights and keeps the ruler ticks",
    )?;
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document
            && s.state.is_dirty() == dirty
            && s.state.editor.edit_mode
            && s.state.editor.selected_vertices.is_empty(),
        "Rulers, view changes and selections do not edit geometry, saved state or document history",
    )?;
    Ok(())
}
