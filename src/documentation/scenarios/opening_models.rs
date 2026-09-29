use super::{Result, Session};
use crate::{controls::Control, orientation::display_rotation};
use glam::Vec3;

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("bracket.obj")?;
    s.settle()?;
    let imported_document = s.state.editor.document.clone();
    s.require(
        s.state.save_path.is_none() && s.state.is_dirty() && !imported_document.objects.is_empty(),
        "Importing OBJ creates an unsaved native document with mesh objects",
    )?;
    let mesh = s
        .state
        .mesh
        .as_ref()
        .ok_or("The bracket fixture did not load")?;
    let counts = (mesh.vertex_count, mesh.face_count, mesh.triangle_count);
    let source_positions: Vec<_> = mesh.vertices.iter().map(|vertex| vertex.position).collect();
    let minimum = source_positions
        .iter()
        .fold(Vec3::splat(f32::INFINITY), |bounds, position| {
            bounds.min(Vec3::from_array(*position))
        });
    let maximum = source_positions
        .iter()
        .fold(Vec3::splat(f32::NEG_INFINITY), |bounds, position| {
            bounds.max(Vec3::from_array(*position))
        });
    let projection = s.state.camera.view_projection(s.state.aspect());
    let fits = mesh.vertices.iter().all(|vertex| {
        let point = projection * Vec3::from_array(vertex.position).extend(1.0);
        let ndc = point.truncate() / point.w;
        point.w > 0.0 && ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0 && (0.0..=1.0).contains(&ndc.z)
    });
    s.require(
        counts == (12, 8, 20),
        "The loaded bracket reports its authored geometry counts",
    )?;
    s.require(
        fits,
        "Loading frames every bracket vertex inside the viewport",
    )?;
    s.require(
        (maximum + minimum).abs_diff_eq(Vec3::ZERO, 1e-6)
            && ((maximum - minimum).max_element() - 2.0).abs() < 1e-6,
        "Loading centers display geometry and uniformly normalizes its largest extent",
    )?;
    s.require(
        s.state
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .is_some_and(|name| name == "bracket.obj"),
        "The imported source path identifies the loaded OBJ file",
    )?;
    s.value("sample-name", "bracket.obj");
    s.value("sample-vertices", counts.0);
    s.value("sample-faces", counts.1);
    s.value("sample-triangles", counts.2);
    let original_path = s.state.path.clone();
    let requested = s.click_path(&[Control::N3Menu, Control::FileMenu, Control::Open])?;
    s.require(
        requested,
        "The live Open control emits a file-picker request",
    )?;
    s.require(
        s.state.path == original_path
            && s.state.mesh.as_ref().is_some_and(|mesh| {
                (mesh.vertex_count, mesh.face_count, mesh.triangle_count) == counts
            }),
        "Requesting a file does not remove the current model",
    )?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click(Control::ZUp)?;
    s.require(s.state.z_up, "Z-up is selectable in Preferences")?;
    s.click(Control::PreferencesClose)?;
    s.require(
        display_rotation(s.state.z_up)
            .transform_vector3(Vec3::Z)
            .abs_diff_eq(Vec3::Y, 1e-6),
        "Z-up displays the source Z direction as vertical",
    )?;
    let positions_unchanged = s.state.mesh.as_ref().is_some_and(|mesh| {
        mesh.vertices
            .iter()
            .map(|vertex| vertex.position)
            .eq(source_positions.iter().copied())
    });
    s.require(
        positions_unchanged && s.state.editor.document == imported_document,
        "Z-up changes display orientation without altering display vertices or canonical document geometry",
    )?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click(Control::ZUp)?;
    s.click(Control::PreferencesClose)?;
    s.require(
        !s.state.z_up,
        "Z-up can be returned to the initial display orientation",
    )?;
    s.settle()?;
    s.capture_image("opening-models")?;

    s.load_fixture("cube-quads.obj")?;
    s.settle()?;
    let replaced = s.state.mesh.as_ref().is_some_and(|mesh| {
        (mesh.vertex_count, mesh.face_count, mesh.triangle_count) == (8, 6, 12)
    }) && s
        .state
        .path
        .as_ref()
        .and_then(|path| path.file_name())
        .is_some_and(|name| name == "cube-quads.obj");
    s.require(
        replaced,
        "A successful second load replaces the displayed model and statistics",
    )
}
