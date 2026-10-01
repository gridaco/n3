use super::{Result, Session};
use crate::{
    camera::Camera,
    controls::Control,
    document::{Document, PrimitiveKind},
    editor::Editor,
};
use glam::{Mat4, Vec3};
use std::{collections::BTreeSet, time::Duration};

fn matrix(s: &Session<'_>) -> Mat4 {
    s.state.camera.view_projection(s.state.aspect())
}

fn outward(s: &Session<'_>) -> Vec3 {
    Vec3::new(
        s.state.camera.direction_in_view(Vec3::X).z,
        s.state.camera.direction_in_view(Vec3::Y).z,
        s.state.camera.direction_in_view(Vec3::Z).z,
    )
}

fn finish_view(s: &mut Session<'_>) -> Result<()> {
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.require(
        !s.state.camera.is_transitioning(),
        "The requested view finishes on the explicit scenario clock",
    )
}

fn view(s: &mut Session<'_>, binding: &str) -> Result<()> {
    s.shortcut(binding)?;
    finish_view(s)
}

fn center(points: &[Vec3]) -> Vec3 {
    let (min, max) = points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    );
    (min + max) * 0.5
}

fn extent(s: &Session<'_>, points: &[Vec3]) -> f32 {
    let projection = matrix(s);
    let bounds = points.iter().fold(egui::Rect::NOTHING, |bounds, point| {
        let projected = projection.project_point3(*point);
        bounds.union(egui::Rect::from_min_max(
            egui::pos2(projected.x, projected.y),
            egui::pos2(projected.x, projected.y),
        ))
    });
    bounds.width().max(bounds.height())
}

fn fitted(s: &Session<'_>, points: &[Vec3]) -> bool {
    let projection = matrix(s);
    projection
        .project_point3(center(points))
        .truncate()
        .length()
        < 1e-4
        && points.iter().all(|point| {
            let clip = projection * point.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            clip.w > 0.0 && ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0 && (0.0..=1.0).contains(&ndc.z)
        })
}

fn ortho_scale(s: &Session<'_>) -> f32 {
    let right = Vec3::new(
        s.state.camera.direction_in_view(Vec3::X).x,
        s.state.camera.direction_in_view(Vec3::Y).x,
        s.state.camera.direction_in_view(Vec3::Z).x,
    );
    let projection = matrix(s);
    projection
        .project_point3(right)
        .truncate()
        .distance(projection.project_point3(Vec3::ZERO).truncate())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    let mut document = Document::default();
    let mut ids = Vec::new();
    for (name, x) in [
        ("Left cube", -4.0),
        ("Middle cube", 1.0),
        ("Distant cube", 12.0),
    ] {
        let id = document.insert_primitive(PrimitiveKind::Cube)?;
        let object = document
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap();
        object.name = name.into();
        object.transform.translation[0] = x;
        document.convert_object(id)?;
        ids.push(id);
    }
    s.state.editor = Editor::new(document.clone())?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.witness(Control::Viewport)?;
    let empty = s.empty_viewport_point()?;
    s.click_at(empty)?;
    let dirty = s.state.is_dirty();
    let before = matrix(s);
    view(s, "view.frame-selection")?;
    s.require(
        matrix(s) == before && s.state.editor.selected_objects.is_empty(),
        "Frame selection with no selected objects leaves the view unchanged",
    )?;

    for (binding, direction) in [
        ("view.perspective", None),
        ("view.front", Some(Vec3::Z)),
        ("view.right", Some(Vec3::X)),
        ("view.back", Some(Vec3::NEG_Z)),
        ("view.left", Some(Vec3::NEG_X)),
        ("view.top", Some(Vec3::Y)),
        ("view.bottom", Some(Vec3::NEG_Y)),
        ("numpad.perspective", None),
        ("numpad.front", Some(Vec3::Z)),
        ("numpad.right", Some(Vec3::X)),
        ("numpad.top", Some(Vec3::Y)),
    ] {
        view(s, binding)?;
        let correct = direction.map_or_else(
            || {
                !s.state.camera.is_orthographic()
                    && s.state
                        .camera
                        .direction_in_view(Vec3::Z)
                        .abs_diff_eq(Camera::default().direction_in_view(Vec3::Z), 1e-5)
            },
            |axis| {
                s.state.camera.is_orthographic()
                    && s.state
                        .camera
                        .direction_in_view(axis)
                        .abs_diff_eq(Vec3::Z, 1e-5)
            },
        );
        s.require(correct, "The source-specific number key reaches its documented perspective or orthographic preset")?;
    }

    for (control, direction) in [
        (Control::ViewBack, Vec3::NEG_Z),
        (Control::ViewLeft, Vec3::NEG_X),
        (Control::ViewBottom, Vec3::NEG_Y),
    ] {
        s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
        s.require(
            s.trace.get(control)?.parents == [Control::N3Menu, Control::ViewMenu],
            "The opposite-direction preset belongs to the actual View menu",
        )?;
        s.click(control)?;
        finish_view(s)?;
        s.require(
            s.state.camera.is_orthographic()
                && s.state
                    .camera
                    .direction_in_view(direction)
                    .abs_diff_eq(Vec3::Z, 1e-5),
            "The View menu reaches the same Back, Left and Bottom axes as the top-row keys",
        )?;
    }

    for (binding, axis, sign) in [
        ("numpad.orbit-left", Vec3::X, -1.0),
        ("numpad.orbit-right", Vec3::X, 1.0),
        ("numpad.orbit-down", Vec3::Y, -1.0),
        ("numpad.orbit-up", Vec3::Y, 1.0),
    ] {
        view(s, "numpad.front")?;
        view(s, binding)?;
        let direction = outward(s);
        let angle = Vec3::Z.dot(direction).clamp(-1.0, 1.0).acos().to_degrees();
        s.require(
            s.state.camera.is_orthographic() && direction.dot(axis) * sign > 0.0 && (angle - 15.0).abs() < 0.001,
            "The numpad orbit shortcuts move left/right and down/up by fifteen degrees while preserving projection",
        )?;
    }
    s.value("orbit-degrees", 15);
    let direction = outward(s);
    for orthographic in [false, true] {
        view(s, "numpad.projection")?;
        s.require(
            s.state.camera.is_orthographic() == orthographic
                && outward(s).abs_diff_eq(direction, 1e-5),
            "The numpad projection shortcut toggles projection without changing the viewing direction",
        )?;
    }
    view(s, "view.perspective")?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let frame_all = matrix(s);
    view(s, "numpad.orbit-right")?;
    view(s, "view.frame")?;
    s.require(
        matrix(s).abs_diff_eq(frame_all, 1e-5),
        "The Frame all shortcut performs the same Frame all reset and fit as the View menu command",
    )?;

    // Fit a proper subset: a third distant object remains unselected.
    view(s, "numpad.orbit-right")?;
    let selected = BTreeSet::from([ids[0], ids[1]]);
    let bounds =
        s.state
            .editor
            .object_selection_bounds(s.state.viewport, &s.state.camera, s.state.z_up)?;
    let selection_box = bounds
        .iter()
        .filter(|(id, _)| selected.contains(id))
        .fold(egui::Rect::NOTHING, |rect, (_, next)| rect.union(*next))
        .expand(6.0);
    s.require(
        s.state.viewport.contains(selection_box.min)
            && s.state.viewport.contains(selection_box.max),
        "The selection-fit example starts with a box inside the viewport",
    )?;
    s.drag_at(selection_box.min, selection_box.size())?;
    s.require(
        s.state.editor.selected_objects == selected,
        "A real object box selects the two nearby cubes and excludes the distant one",
    )?;
    let points = s.state.editor.selection_points(s.state.z_up)?;
    let before_extent = extent(s, &points);
    let direction = outward(s);
    let projection = s.state.camera.is_orthographic();
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.shortcut_down("view.frame-selection")?;
    finish_view(s)?;
    s.require(
        fitted(s, &points) && extent(s, &points) > before_extent * 1.1
            && outward(s).abs_diff_eq(direction, 1e-5)
            && s.state.camera.is_orthographic() == projection
            && s.state.editor.selected_objects == selected,
        "Frame selection centers and fits the selected object group more closely, preserving its viewing direction, projection and selection",
    )?;
    s.capture_tutorial("view-keys-object-fit")?;
    s.shortcut_up("view.frame-selection")?;
    let fitted_group = matrix(s);
    s.right_click(Control::Viewport)?;
    s.require(
        s.trace.get(Control::FrameSelection)?.parents == [Control::ViewportMenu],
        "Frame selection is available in the viewport context menu",
    )?;
    s.click(Control::FrameSelection)?;
    s.require(
        matrix(s).abs_diff_eq(fitted_group, 1e-5),
        "The context-menu Frame selection command produces the same selected-group fit",
    )?;

    let empty = s.empty_viewport_point()?;
    s.click_at(empty)?;
    view(s, "numpad.projection")?;
    s.require(
        s.state.camera.is_orthographic(),
        "The vertex fit example uses orthographic projection",
    )?;
    s.shortcut("selection.next")?;
    s.shortcut("edit.confirm")?;
    s.shortcut("selection.all")?;
    s.require(
        s.state.editor.edit_mode && !s.state.editor.selected_vertices.is_empty(),
        "The vertex fit example selects actual visible components of the first mesh",
    )?;
    let vertices = s.state.editor.selected_vertices.clone();
    let points = s.state.editor.selection_points(s.state.z_up)?;
    let direction = outward(s);
    let before_extent = extent(s, &points);
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.shortcut_down("view.frame-selection")?;
    finish_view(s)?;
    s.require(
        fitted(s, &points) && extent(s, &points) > before_extent * 1.1
            && outward(s).abs_diff_eq(direction, 1e-5)
            && s.state.camera.is_orthographic()
            && s.state.editor.selected_vertices == vertices,
        "Frame selection fits the selected vertices while preserving orthographic projection, direction and component selection",
    )?;
    s.capture_tutorial("view-keys-vertex-fit")?;
    s.shortcut_up("view.frame-selection")?;
    s.shortcut("cancel")?;
    let empty_vertex_view = matrix(s);
    view(s, "view.frame-selection")?;
    s.require(
        matrix(s) == empty_vertex_view,
        "Frame selection with no selected vertices is a no-op even though a mesh is being edited",
    )?;
    s.shortcut("selection.next")?;
    let points = s.state.editor.selection_points(s.state.z_up)?;
    s.require(
        points.len() == 1,
        "The final fit target is one selected vertex",
    )?;
    let scale = ortho_scale(s);
    view(s, "view.frame-selection")?;
    s.require(
        fitted(s, &points) && (ortho_scale(s) - scale).abs() < 1e-5,
        "Fitting one vertex centers it while retaining the existing zoom",
    )?;
    let selection = s.state.editor.selected_vertices.clone();
    s.shortcut("history.undo")?;
    s.require(
        s.state.editor.document == document && s.state.is_dirty() == dirty
            && s.state.editor.selected_vertices == selection,
        "Preset keys, orbit steps, projection changes and fitting never edit geometry, saved state, selection, or document history",
    )?;
    Ok(())
}
