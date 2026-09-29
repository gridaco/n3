use super::{Result, Session, pointer};
use crate::{
    camera::{Camera, Transition},
    controls::Control,
    doc_harness::ClipSpec,
    navigation_state::PlanarExit,
    orientation::display_rotation,
    workspace_ui::{DEFAULT_VIEW_DURATION_MS, MAX_VIEW_DURATION_MS},
};
use glam::Vec3;
use std::time::Duration;

fn finish_view(s: &mut Session<'_>) -> Result<()> {
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.require(
        !s.state.camera.is_transitioning(),
        "The view transition reaches its configured endpoint",
    )
}

fn projection_control(s: &mut Session<'_>) -> Result<()> {
    let camera = s.state.camera.clone();
    let document = s.state.editor.document.clone();
    let objects = s.state.editor.selected_objects.clone();
    let vertices = s.state.editor.selected_vertices.clone();
    let revision = s.state.editor.revision;
    let dirty = s.state.is_dirty();
    s.witness(Control::Projection)?;
    // Trace validation rejects duplicate identities, including a stale top-bar
    // copy. The one live button must belong to the gizmo's control group.
    s.trace.validate()?;
    s.require(
        s.trace.get(Control::Projection)?.parents == [Control::Gizmo]
            && s.trace.get(Control::Projection)?.label == Control::Projection.label()
            && !camera.is_orthographic()
            && !s.state.is_planar_navigation(),
        "The single Projection button belongs to the gizmo, retains its semantic label and initially shows perspective",
    )?;
    s.hover(Control::Viewport)?;
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.hover(Control::Projection)?;
    s.callout(
        Control::Projection,
        "Perspective now. Click for orthographic.",
    )?;
    s.capture_tutorial("gizmo-projection-perspective")?;
    s.clear_callout()?;

    s.click(Control::Projection)?;
    let mut expected = camera.clone();
    expected.toggle_projection();
    s.require(
        s.state.camera.is_orthographic()
            && !s.state.is_planar_navigation()
            && s.state.camera.orientation() == camera.orientation()
            && s.state.camera.view_projection(1.0) == expected.view_projection(1.0)
            && s.state.editor.document == document
            && s.state.editor.selected_objects == objects
            && s.state.editor.selected_vertices == vertices
            && s.state.editor.revision == revision
            && s.state.is_dirty() == dirty,
        "Clicking the perspective cube switches only projection, retaining orientation, pan, zoom, 3D navigation, geometry and selection",
    )?;
    s.callout(
        Control::Projection,
        "Orthographic now. Click for perspective.",
    )?;
    s.capture_tutorial("gizmo-projection-orthographic")?;
    s.clear_callout()?;

    s.click(Control::Projection)?;
    s.require(
        !s.state.camera.is_orthographic()
            && !s.state.is_planar_navigation()
            && s.state.camera.view_projection(1.0) == camera.view_projection(1.0)
            && s.state.editor.document == document
            && s.state.editor.selected_objects == objects
            && s.state.editor.selected_vertices == vertices
            && s.state.editor.revision == revision
            && s.state.is_dirty() == dirty,
        "Clicking the orthographic cube restores the original perspective camera without editing geometry or selection",
    )
}

fn dot_navigation(s: &mut Session<'_>) -> Result<()> {
    s.drag(Control::Gizmo, egui::vec2(18.0, -12.0))?;
    let free = s.state.camera.clone();
    let document = s.state.editor.document.clone();
    let selection = s.state.editor.selected_objects.clone();
    let dirty = s.state.is_dirty();
    let nearest = free.nearest_axis_direction();
    let mut planar = free.clone();
    planar.look_from(nearest);

    s.shortcut_down("view.planar")?;
    s.require(
        !s.state.is_planar_navigation()
            && s.state.camera.view_projection(1.0) == free.view_projection(1.0),
        "A navigation-toggle key press waits for release before toggling navigation",
    )?;
    s.shortcut_up("view.planar")?;
    s.require(
        s.state.is_planar_navigation() && s.state.camera.is_transitioning(),
        "Releasing the navigation-toggle key enters Planar navigation through the usual animated transition",
    )?;
    finish_view(s)?;
    s.require(
        s.state
            .camera
            .view_projection(1.0)
            .abs_diff_eq(planar.view_projection(1.0), 1e-5)
            && s.state.ruler_2d_model.is_some(),
        "Dot snaps to the nearest cardinal direction with orthographic projection and rulers",
    )?;
    s.capture_tutorial("gizmo-dot-navigation")?;
    s.shortcut("view.planar")?;
    finish_view(s)?;
    s.require(
        !s.state.is_planar_navigation()
            && s.state
                .camera
                .view_projection(1.0)
                .abs_diff_eq(free.view_projection(1.0), 1e-5),
        "A second navigation-toggle key tap restores the remembered 3D orientation and perspective",
    )?;

    s.click(Control::NavigationPlanar)?;
    finish_view(s)?;
    s.require(
        s.state
            .camera
            .view_projection(1.0)
            .abs_diff_eq(planar.view_projection(1.0), 1e-5),
        "The 2D button reaches the same view as the navigation-toggle key action",
    )?;
    s.click(Control::NavigationFree)?;
    finish_view(s)?;
    s.require(
        s.state
            .camera
            .view_projection(1.0)
            .abs_diff_eq(free.view_projection(1.0), 1e-5)
            && s.state.editor.document == document
            && s.state.editor.selected_objects == selection
            && s.state.is_dirty() == dirty,
        "The 3D button and navigation-toggle key share the return policy without editing geometry or selection",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    Ok(())
}

fn return_to_3d(s: &mut Session<'_>, expected: &Camera) -> Result<()> {
    let visible = s.state.camera.view_projection(1.0);
    s.click(Control::NavigationFree)?;
    s.require(
        !s.state.is_planar_navigation()
            && s.state.ruler_2d_model.is_none()
            && s.state.camera.is_transitioning()
            && s.state.camera.view_projection(1.0) == visible,
        "Choosing 3D starts the configured return animation at the visible pose and hides rulers",
    )?;
    finish_view(s)?;
    s.require(
        s.state.camera.view_projection(1.0).abs_diff_eq(expected.view_projection(1.0), 1e-5)
            && s.state.camera.is_orthographic() == expected.is_orthographic(),
        "Returning to 3D uses the chosen orientation/projection policy while preserving the current pan and zoom",
    )
}

fn return_preferences(s: &mut Session<'_>) -> Result<()> {
    for (control, policy) in [
        (Control::Return3DPerspective, PlanarExit::PerspectiveOnly),
        (Control::Return3DOrientation, PlanarExit::OrientationOnly),
        (Control::Return3DBoth, PlanarExit::OrientationAndPerspective),
    ] {
        s.click_path(&[Control::N3Menu, Control::Preferences])?;
        s.reveal_preferences_control(Control::Return3DMenu)?;
        s.click(Control::Return3DMenu)?;
        s.require(
            s.trace.get(control)?.parents == [Control::PreferencesWindow, Control::Return3DMenu],
            "Each return policy is a real option inside the Gizmo preferences menu",
        )?;
        s.click(control)?;
        s.require(
            s.state.return_3d == policy,
            "The live preference stores the chosen 3D return policy",
        )?;
        s.close_preferences()?;

        s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
        s.click(Control::ViewPerspective)?;
        finish_view(s)?;
        s.drag(Control::Gizmo, egui::vec2(24.0, -16.0))?;
        let previous = s.state.camera.orientation();
        s.click(Control::AxisX)?;
        finish_view(s)?;
        s.require(
            s.state.is_planar_navigation() && s.state.camera.direction_in_view(Vec3::X).z > 0.9999,
            "Entering 2D through a gizmo axis captures the immediately preceding 3D orientation",
        )?;
        s.state.pan(20.0, -12.0);
        s.hover(Control::Viewport)?;
        s.pinch(0.1)?;
        s.settle()?;
        s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
        s.click(Control::ViewTop)?;
        finish_view(s)?;
        let planar = s.state.camera.orientation();
        let mut expected = s.state.camera.clone();
        let (orientation, orthographic) = match policy {
            PlanarExit::PerspectiveOnly => (planar, false),
            PlanarExit::OrientationOnly => (previous, true),
            PlanarExit::OrientationAndPerspective => (previous, false),
        };
        expected.set_orientation_with_transition(orientation, orthographic, Transition::Instant);
        return_to_3d(s, &expected)?;
        s.require(
            s.state.camera.orientation() == orientation,
            "A later 2D axis change does not overwrite the saved 3D orientation; each policy restores exactly its selected orientation",
        )?;
    }
    // An explicit Perspective preset remains a preset, not an orientation recall.
    s.click(Control::AxisX)?;
    finish_view(s)?;
    let mut expected = s.state.camera.clone();
    expected.set_view(crate::camera::View::Perspective);
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(Control::ViewPerspective)?;
    finish_view(s)?;
    s.require(
        !s.state.is_planar_navigation()
            && s.state.camera.view_projection(1.0).abs_diff_eq(expected.view_projection(1.0), 1e-5),
        "The explicit Perspective preset uses its default viewing angle rather than recalling the previous 3D orientation",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    Ok(())
}

fn animated_orbit_and_align(s: &mut Session<'_>) -> Result<()> {
    let document = s.state.editor.document.clone();
    let selected = s.state.editor.selected_objects.clone();
    let dirty = s.state.is_dirty();
    s.hover(Control::Gizmo)?;
    let start = s.trace.get(Control::Gizmo)?.rect.center();
    let before = s.state.camera.view_projection(s.state.aspect());
    s.capture_clip("gizmo-orbit-and-align", ClipSpec::default(), |s| {
        s.callout(Control::Gizmo, "Drag to orbit freely.")?;
        s.wait(Duration::from_millis(400))?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.wait(Duration::from_millis(200))?;
        s.move_pointer(start + egui::vec2(-110.0, 45.0), Duration::from_millis(900))?;
        s.require(
            s.input.is_pressed(egui::PointerButton::Primary)
                && s.cursor == egui::CursorIcon::Grabbing
                && s.state.camera.view_projection(s.state.aspect()) != before
                && !s.state.is_planar_navigation(),
            "The animated gizmo tutorial orbits through real held pointer samples",
        )?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.wait(Duration::from_millis(300))?;
        s.callout(Control::AxisX, "Click an axis to align the view.")?;
        let axis = s.trace.get(Control::AxisX)?.rect.center();
        s.move_pointer(axis, Duration::from_millis(400))?;
        s.wait(Duration::from_millis(200))?;
        s.pointer_button(egui::PointerButton::Primary, true)?;
        s.wait(Duration::from_millis(100))?;
        s.pointer_button(egui::PointerButton::Primary, false)?;
        s.require(
            s.state.camera.is_transitioning(),
            "The animated axis click begins the real camera transition",
        )?;
        let aligned_start = s.state.camera.view_projection(s.state.aspect());
        s.wait(Duration::from_millis(50))?;
        s.require(
            s.state.camera.is_transitioning()
                && s.state.camera.view_projection(s.state.aspect()) != aligned_start,
            "The clip samples the camera between the starting and aligned poses",
        )?;
        s.wait(Duration::from_millis(200))?;
        s.require(
            !s.state.camera.is_transitioning()
                && s.state.is_planar_navigation()
                && s.state.camera.direction_in_view(Vec3::X).z > 0.9999,
            "The animated axis click ends in the requested planar view",
        )?;
        s.wait(Duration::from_millis(450))?;
        s.clear_callout()
    })?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.selected_objects == selected
            && s.state.is_dirty() == dirty,
        "Animated gizmo interaction preserves geometry, selection and saved state",
    )?;
    // Frame resets the angle but intentionally preserves projection. Restore
    // Perspective explicitly so later navigation examples keep their baseline.
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::ViewPerspective])?;
    finish_view(s)?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    Ok(())
}

pub fn run(s: &mut Session<'_>) -> Result<()> {
    s.load_fixture("bracket.obj")?;
    s.require(
        s.state.animate_views && s.state.view_duration_ms == DEFAULT_VIEW_DURATION_MS,
        "Axis views animate by default using the viewer's configured duration",
    )?;
    s.value("default-duration", s.state.view_duration_ms);
    s.witness(Control::Gizmo)?;
    s.capture_image("gizmo-orbit")?;
    projection_control(s)?;
    animated_orbit_and_align(s)?;
    dot_navigation(s)?;

    s.witness(Control::NavigationPlanar)?;
    s.witness(Control::NavigationFree)?;
    s.require(
        s.trace.get(Control::NavigationPlanar)?.parents == [Control::Gizmo]
            && s.trace.get(Control::NavigationFree)?.parents == [Control::Gizmo],
        "The live 2D and 3D navigation tabs belong to the gizmo",
    )?;
    let document = s.state.editor.document.clone();
    let dirty = s.state.is_dirty();
    let selection = s.state.editor.selected_objects.clone();
    s.require(
        !s.state.is_planar_navigation()
            && s.state.return_3d == PlanarExit::OrientationAndPerspective,
        "The initial camera uses 3D navigation and defaults to restoring the previous orientation plus perspective",
    )?;
    s.drag(Control::Gizmo, egui::vec2(16.0, 0.0))?;
    let front = s.state.camera.direction_in_view(Vec3::Z).z;
    s.require(
        front > s.state.camera.direction_in_view(Vec3::X).z.abs()
            && front > s.state.camera.direction_in_view(Vec3::Y).z.abs(),
        "A small visible gizmo orbit makes Front the closest cardinal viewing direction",
    )?;
    let previous_3d = s.state.camera.orientation();
    s.click(Control::NavigationPlanar)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.require(
        s.state.is_planar_navigation()
            && s.state.camera.is_orthographic()
            && s.state
                .camera
                .direction_in_view(Vec3::Z)
                .abs_diff_eq(Vec3::Z, 1e-5)
            && s.state.ruler_2d_model.is_some(),
        "Choosing 2D snaps to the nearest visible axis, Front, in orthographic projection with rulers",
    )?;
    s.state.pan(20.0, -12.0);
    s.hover(Control::Viewport)?;
    s.pinch(0.1)?;
    s.settle()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu])?;
    s.click(Control::ViewTop)?;
    finish_view(s)?;
    let mut expected = s.state.camera.clone();
    expected.set_orientation_with_transition(previous_3d, false, Transition::Instant);
    return_to_3d(s, &expected)?;
    // Ruler removal changes the viewport layout. Point at the settled control
    // after the old click cue expires, so the tutorial identifies its new place.
    s.frame(Vec::new(), crate::doc_input::CUE_LIFETIME)?;
    s.hover(Control::NavigationFree)?;
    s.capture_tutorial("gizmo-navigation-mode")?;
    let direction = s.state.camera.direction_in_view(Vec3::Z);
    s.hover(Control::Viewport)?;
    s.scroll(18.0, -12.0, true, egui::Modifiers::NONE)?;
    s.settle()?;
    s.require(
        !s.state.is_planar_navigation()
            && !s
                .state
                .camera
                .direction_in_view(Vec3::Z)
                .abs_diff_eq(direction, 1e-5),
        "Unmodified precise scrolling orbits from the returned 3D viewing direction",
    )?;
    s.click(Control::AxisX)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.require(
        s.state.is_planar_navigation()
            && s.state
                .camera
                .direction_in_view(Vec3::X)
                .abs_diff_eq(Vec3::Z, 1e-5)
            && s.state.ruler_2d_model.is_some(),
        "Clicking the X gizmo axis enters 2D in the Right view",
    )?;
    let mut expected_orbit = s.state.camera.clone();
    expected_orbit.orbit(15.0, 160.0);
    let start = s.state.viewport.center() + egui::vec2(-35.0, -80.0);
    s.frame(vec![egui::Event::PointerMoved(start)], Duration::ZERO)?;
    s.shortcut_down("navigation.orbit")?;
    s.pointer_button(egui::PointerButton::Primary, true)?;
    s.frame(
        vec![egui::Event::PointerMoved(start + egui::vec2(15.0, 160.0))],
        Duration::ZERO,
    )?;
    s.pointer_button(egui::PointerButton::Primary, false)?;
    s.shortcut_up("navigation.orbit")?;
    s.require(
        !s.state.is_planar_navigation()
            && !s.state.camera.is_transitioning()
            && s.state.camera.view_projection(1.0).abs_diff_eq(expected_orbit.view_projection(1.0), 1e-5),
        "Manual orbit dragging starts directly from the visible Right view without recalling the saved 3D orientation",
    )?;
    s.state.pan(24.0, -16.0);
    s.hover(Control::Viewport)?;
    s.pinch(0.15)?;
    s.settle()?;
    let top = s.state.camera.direction_in_view(Vec3::Y).z;
    s.require(
        top > 0.8
            && top > s.state.camera.direction_in_view(Vec3::X).z.abs()
            && top > s.state.camera.direction_in_view(Vec3::Z).z.abs(),
        "After leaving Right, the current visible camera is nearer Top than any other cardinal axis",
    )?;
    let mut expected = s.state.camera.clone();
    expected.look_from(Vec3::Y);
    s.click(Control::NavigationPlanar)?;
    s.frame(
        Vec::new(),
        Duration::from_millis(s.state.view_duration_ms.into()),
    )?;
    s.settle()?;
    s.require(
        s.state.is_planar_navigation()
            && s.state
                .camera
                .direction_in_view(Vec3::Y)
                .abs_diff_eq(Vec3::Z, 1e-5)
            && s.state.camera.view_projection(1.0)
                .abs_diff_eq(expected.view_projection(1.0), 1e-5)
            && s.state.ruler_2d_model.is_some()
            && s.state.editor.document == document
            && s.state.is_dirty() == dirty
            && s.state.editor.selected_objects == selection,
        "Choosing 2D selects the current nearest axis, Top rather than previous Right view, preserving pan, zoom, geometry and selection",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    return_preferences(s)?;
    s.require(
        s.state.editor.document == document
            && s.state.editor.selected_objects == selection
            && s.state.is_dirty() == dirty,
        "Changing and exercising every 3D return policy leaves geometry, selection and document dirty state unchanged",
    )?;

    let before_hover = s.state.camera.view_projection(s.state.aspect());
    s.hover(Control::AxisX)?;
    let hover_center = s.trace.get(Control::AxisX)?.rect.center();
    s.require(
        s.input
            .position()
            .is_some_and(|position| position.distance(hover_center) < 0.001)
            && s.cursor == egui::CursorIcon::PointingHand
            && s.state.camera.view_projection(s.state.aspect()) == before_hover,
        "Hovering the live X handle shows a pointing cursor without moving the camera",
    )?;
    s.capture_tutorial("gizmo-hover")?;

    // Every handle is selected by its live hit area, never by dispatching a camera command.
    for (control, direction) in [
        (Control::AxisX, Vec3::X),
        (Control::AxisNegX, -Vec3::X),
        (Control::AxisY, Vec3::Y),
        (Control::AxisNegY, -Vec3::Y),
        (Control::AxisZ, Vec3::Z),
        (Control::AxisNegZ, -Vec3::Z),
    ] {
        s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
        let before = s.state.camera.view_projection(s.state.aspect());
        s.click(control)?;
        s.require(
            s.state.camera.is_transitioning()
                && s.state.camera.view_projection(s.state.aspect()) == before,
            &format!(
                "{} starts a transition without jumping the camera (transitioning={}, preferences={}, duration={}, rect={:?})",
                control.id(),
                s.state.camera.is_transitioning(),
                s.state.show_preferences,
                s.state.view_duration_ms,
                s.trace.get(control)?.rect
            ),
        )?;
        let rejected = s.capture_image("premature").is_err();
        s.require(rejected, "Unsettled camera captures are rejected")?;
        s.frame(
            vec![],
            Duration::from_millis((s.state.view_duration_ms / 2).into()),
        )?;
        s.require(
            s.state.camera.is_transitioning()
                && s.state.camera.view_projection(s.state.aspect()) != before,
            "At half duration the camera has moved and has not snapped to the endpoint",
        )?;
        s.frame(
            vec![],
            Duration::from_millis((s.state.view_duration_ms - s.state.view_duration_ms / 2).into()),
        )?;
        s.require(
            !s.state.camera.is_transitioning()
                && s.state.is_planar_navigation()
                && s.state.camera.is_orthographic()
                && s.state.camera.direction_in_view(direction).z > 0.9999,
            &format!(
                "{} finishes looking from the requested axis in orthographic projection",
                control.id()
            ),
        )?;
        if control == Control::AxisX {
            s.capture_image("gizmo-aligned")?;
            s.click(control)?;
            s.frame(
                vec![],
                Duration::from_millis(s.state.view_duration_ms.into()),
            )?;
            s.require(
                s.state.camera.direction_in_view(-direction).z > 0.9999,
                "Clicking the facing axis again reaches its opposite side",
            )?;
        }
    }

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = s.state.camera.view_projection(s.state.aspect());
    s.drag(Control::Gizmo, egui::vec2(-55., 24.))?;
    s.require(
        s.state.camera.view_projection(s.state.aspect()) != before
            && !s.state.camera.is_transitioning()
            && !s.state.is_planar_navigation(),
        "Dragging the gizmo body orbits in 3D without triggering an axis snap",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    let before = s.state.camera.view_projection(s.state.aspect());
    s.hover(Control::AxisX)?;
    let start = s.trace.get(Control::AxisX)?.rect.center();
    let end = start + egui::vec2(-170., 45.);
    s.frame(vec![pointer(start, true)], Duration::ZERO)?;
    s.frame(
        vec![egui::Event::PointerMoved(start + egui::vec2(-85., 22.5))],
        Duration::ZERO,
    )?;
    s.frame(vec![egui::Event::PointerMoved(end)], Duration::ZERO)?;
    s.require(
        s.input.is_pressed(egui::PointerButton::Primary)
            && s.cursor == egui::CursorIcon::Grabbing
            && s.state.camera.view_projection(s.state.aspect()) != before
            && !s.state.camera.is_transitioning(),
        "A held primary drag orbits from the live handle and shows a grabbing cursor",
    )?;
    let held_pose = s.state.camera.view_projection(s.state.aspect());
    s.capture_tutorial("gizmo-drag")?;
    s.require(
        s.input.is_pressed(egui::PointerButton::Primary)
            && s.state.camera.view_projection(s.state.aspect()) == held_pose,
        "The drag tutorial capture preserves the held pointer and visible pose",
    )?;
    s.frame(vec![pointer(end, false)], Duration::ZERO)?;
    s.settle()?;
    s.require(
        s.state.camera.view_projection(s.state.aspect()) != before
            && s.state.camera.view_projection(s.state.aspect()) == held_pose
            && !s.input.is_pressed(egui::PointerButton::Primary)
            && !s.state.camera.is_transitioning(),
        "Dragging an axis handle continues outside the gizmo and does not click-snap on release",
    )?;

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click(Control::AxisX)?;
    s.frame(vec![], Duration::from_millis(30))?;
    let pose = s.state.camera.view_projection(s.state.aspect());
    let center = s.trace.get(Control::Gizmo)?.rect.center();
    s.frame(
        vec![egui::Event::PointerMoved(center), pointer(center, true)],
        Duration::ZERO,
    )?;
    s.frame(vec![], Duration::from_secs(1))?;
    s.require(
        !s.state.camera.is_transitioning()
            && s.state.camera.view_projection(s.state.aspect()) == pose,
        "Taking hold of the gizmo interrupts animation at the visible pose",
    )?;
    s.frame(
        vec![egui::Event::PointerMoved(center + egui::vec2(-35., 10.))],
        Duration::ZERO,
    )?;
    s.frame(
        vec![pointer(center + egui::vec2(-35., 10.), false)],
        Duration::ZERO,
    )?;
    s.require(
        !s.state.is_planar_navigation(),
        "Orbiting from an interrupted axis animation switches to 3D navigation",
    )?;

    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click(Control::AxisX)?;
    s.frame(vec![], Duration::from_millis(30))?;
    let context_pose = s.state.camera.view_projection(s.state.aspect());
    s.right_click(Control::Gizmo)?;
    s.require(
        s.state.camera.is_transitioning()
            && s.state.camera.view_projection(s.state.aspect()) == context_pose
            && s.trace.get(Control::GizmoPreferences)?.parents == [Control::Gizmo],
        "Right-clicking the gizmo opens its preferences menu without moving or cancelling the active camera transition",
    )?;
    // Advance actual popup fading and the camera clock explicitly before the
    // tutorial image, then hover the live menu item at its settled hit area.
    s.frame(
        vec![],
        crate::doc_input::CUE_LIFETIME.max(Duration::from_millis(s.state.view_duration_ms.into())),
    )?;
    s.hover(Control::GizmoPreferences)?;
    let context_item = s.trace.get(Control::GizmoPreferences)?.clone();
    s.require(
        !s.state.camera.is_transitioning()
            && s.state.camera.direction_in_view(Vec3::X).z > 0.9999
            && context_item.parents == [Control::Gizmo]
            && s.input.position().is_some_and(|position| context_item.rect.contains(position)),
        "The camera transition finishes normally while the gizmo preferences menu remains open and hovered",
    )?;
    s.capture_tutorial("gizmo-context-menu")?;
    s.click(Control::GizmoPreferences)?;
    s.require(
        s.state.show_preferences
            && s.trace.get(Control::GizmoPreferences).is_err()
            && s.trace.get(Control::PreferencesWindow).is_ok(),
        "Gizmo preferences opens the shared Preferences window and closes the context menu",
    )?;
    s.require(
        s.trace.get(Control::AnimateViews)?.parents == [Control::PreferencesWindow]
            && s.trace.get(Control::Duration)?.parents == [Control::PreferencesWindow],
        "Animation settings are reachable inside the Preferences window",
    )?;
    s.witness(Control::AnimateViews)?;
    s.witness(Control::Duration)?;
    s.frame(vec![], crate::doc_input::CUE_LIFETIME)?;
    s.settle()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.capture_image("gizmo-settings")?;

    s.click(Control::AxisX)?;
    s.frame(vec![], Duration::from_millis(30))?;
    s.click(Control::AnimateViews)?;
    s.require(
        !s.state.animate_views
            && !s.state.camera.is_transitioning()
            && s.state.camera.direction_in_view(Vec3::X).z > 0.9999,
        "Turning animation off completes the current transition",
    )?;
    s.require(
        !s.trace.get(Control::Duration)?.enabled,
        "Duration is disabled when animation is off",
    )?;
    let rejected = s.click(Control::Duration).is_err();
    s.require(
        rejected,
        "The harness refuses to interact with disabled controls",
    )?;
    // Preferences can overlap the gizmo at compact inspector widths. Close
    // the window before testing a viewport click, then reopen the same setting.
    s.close_preferences()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click(Control::AxisZ)?;
    s.require(
        !s.state.camera.is_transitioning() && s.state.camera.direction_in_view(Vec3::Z).z > 0.9999,
        "Axis clicks are immediate when animation is off",
    )?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.click(Control::AnimateViews)?;

    // Drag the real slider from inside its left edge beyond each endpoint.
    // Coordinates are relative to the current widget, not a fixed screen layout.
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click(Control::AxisX)?;
    let current_duration = s.state.view_duration_ms;
    let rect = s.trace.get(Control::Duration)?.rect;
    s.drag_at(
        egui::pos2(rect.left() + 2., rect.center().y),
        egui::vec2(rect.width() + 20., 0.),
    )?;
    s.require(
        s.state.view_duration_ms == MAX_VIEW_DURATION_MS,
        "Duration slider reaches its declared maximum",
    )?;
    s.value("max-duration", s.state.view_duration_ms);
    s.frame(vec![], Duration::from_millis((current_duration - 1).into()))?;
    s.require(
        s.state.camera.is_transitioning(),
        "Editing duration does not complete the active transition early",
    )?;
    s.frame(vec![], Duration::from_millis(1))?;
    s.require(
        !s.state.camera.is_transitioning() && s.state.camera.direction_in_view(Vec3::X).z > 0.9999,
        "An active transition keeps its original duration after the slider changes",
    )?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click(Control::AxisX)?;
    s.frame(
        vec![],
        Duration::from_millis((s.state.view_duration_ms - 1).into()),
    )?;
    s.require(
        s.state.camera.is_transitioning(),
        "The next transition uses the newly configured duration",
    )?;
    s.frame(vec![], Duration::from_millis(1))?;
    s.require(
        !s.state.camera.is_transitioning(),
        "The next transition completes at the new duration",
    )?;
    let rect = s.trace.get(Control::Duration)?.rect;
    s.drag_at(
        egui::pos2(rect.left() + 2., rect.center().y),
        egui::vec2(-20., 0.),
    )?;
    s.require(
        s.state.view_duration_ms == 0,
        "Duration slider reaches zero",
    )?;
    s.close_preferences()?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click(Control::AxisY)?;
    s.require(
        !s.state.camera.is_transitioning() && s.state.camera.direction_in_view(Vec3::Y).z > 0.9999,
        "Zero duration makes the next axis view immediate",
    )?;

    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.reveal_preferences_control(Control::ZUp)?;
    s.click(Control::ZUp)?;
    s.click(Control::PreferencesClose)?;
    s.click_path(&[Control::N3Menu, Control::ViewMenu, Control::Frame])?;
    s.click(Control::AxisZ)?;
    s.require(
        s.state
            .camera
            .direction_in_view(display_rotation(true).transform_vector3(Vec3::Z))
            .z
            > 0.9999,
        "Gizmo axes follow the same Z-up display orientation as the mesh",
    )?;

    let settings = (
        s.state.animate_views,
        s.state.view_duration_ms,
        s.state.precise_scroll_zoom,
        s.state.return_3d,
    );
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.close_preferences()?;
    s.require(
        !s.state.show_preferences && s.trace.get(Control::Duration).is_err(),
        "Closing Preferences removes the window and its animation controls",
    )?;
    s.right_click(Control::Gizmo)?;
    s.click(Control::GizmoPreferences)?;
    s.require(
        s.state.show_preferences
            && (
                s.state.animate_views,
                s.state.view_duration_ms,
                s.state.precise_scroll_zoom,
                s.state.return_3d,
            ) == settings,
        "Reopening Preferences through the gizmo preserves the changed settings",
    )?;
    s.close_preferences()?;
    s.click_path(&[Control::N3Menu, Control::Preferences])?;
    s.require(
        s.state.show_preferences
            && s.trace.get(Control::PreferencesWindow).is_ok()
            && (
                s.state.animate_views,
                s.state.view_duration_ms,
                s.state.precise_scroll_zoom,
                s.state.return_3d,
            ) == settings,
        "The toolbar Preferences control opens the same window with the same settings",
    )?;
    s.close_preferences()?;
    s.require(
        !s.state.show_preferences
            && s.trace.get(Control::AnimateViews).is_err()
            && s.trace.get(Control::Duration).is_err()
            && s.trace.get(Control::PreciseScroll).is_err(),
        "Closing Preferences hides all settings controls",
    )?;
    Ok(())
}
