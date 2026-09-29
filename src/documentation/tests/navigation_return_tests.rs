//! Integration contracts for planar navigation and its remembered 3D orientation.

use super::{Capture, Control, HEIGHT, Session, WIDTH, pointer};
use crate::{
    camera::{Camera, Transition, View},
    document::{Document, PrimitiveKind},
    editor::Editor,
    navigation_state::PlanarExit,
    workspace_ui::WorkspaceUi,
};
use egui::{Modifiers, Pos2, Rect, TextureId};
use glam::{DVec3, Vec3};
use std::{path::PathBuf, time::Duration};

fn viewer() -> WorkspaceUi {
    let mut state = WorkspaceUi::new(TextureId::User(0));
    state.viewport = Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0));
    state.animate_views = false;
    state.orbit(73.0, -31.0);
    state
}

fn basis(camera: &Camera) -> [Vec3; 3] {
    [Vec3::X, Vec3::Y, Vec3::Z].map(|axis| camera.direction_in_view(axis))
}

fn assert_basis(actual: &Camera, expected: &Camera) {
    for (a, b) in basis(actual).into_iter().zip(basis(expected)) {
        assert!(
            a.abs_diff_eq(b, 2e-5),
            "Orientation differs: {a:?} vs {b:?}"
        );
    }
}

#[test]
fn all_return_policies_preserve_current_pan_zoom_and_original_orientation_memory() {
    for policy in [
        PlanarExit::PerspectiveOnly,
        PlanarExit::OrientationOnly,
        PlanarExit::OrientationAndPerspective,
    ] {
        for (animate, duration_ms) in [(false, 120), (true, 0), (true, 120)] {
            for initial_orthographic in [false, true] {
                for z_up in [false, true] {
                    let mut state = viewer();
                    state.return_3d = policy;
                    state.animate_views = animate;
                    state.view_duration_ms = duration_ms;
                    state.z_up = z_up;
                    if initial_orthographic {
                        state.camera.toggle_projection();
                    }
                    let remembered = state.camera.orientation();
                    state.set_view(View::Front);
                    state.camera.finish_transition();
                    state.set_view(View::Top);
                    state.camera.finish_transition();
                    state.set_planar_navigation(true);
                    state.set_view(View::Left);
                    state.camera.finish_transition();
                    assert!(state.is_planar_navigation());
                    state.pan(48.0, -26.0);
                    state.scroll(0.0, 1.0, false, Modifiers::NONE, None);
                    let planar = state.camera.clone();
                    let expected_orientation = match policy {
                        PlanarExit::PerspectiveOnly => planar.orientation(),
                        _ => remembered,
                    };
                    let expected_orthographic = matches!(policy, PlanarExit::OrientationOnly);
                    let mut expected = planar.clone();
                    expected.set_orientation_with_transition(
                        expected_orientation,
                        expected_orthographic,
                        Transition::Instant,
                    );

                    state.set_planar_navigation(false);
                    assert!(!state.is_planar_navigation());
                    if animate && duration_ms != 0 {
                        assert_eq!(
                            state.camera.view_projection(state.aspect()),
                            planar.view_projection(state.aspect()),
                            "An animated return starts from the current visible pose"
                        );
                        assert!(state.camera.is_transitioning());
                        state.camera.advance_transition(Duration::from_millis(40));
                        assert!(state.camera.is_transitioning());
                        state.camera.advance_transition(Duration::from_millis(80));
                    }
                    assert!(!state.camera.is_transitioning());
                    assert_eq!(state.camera.is_orthographic(), expected_orthographic);
                    assert_basis(&state.camera, &expected);
                    assert!(
                        state
                            .camera
                            .view_projection(state.aspect())
                            .abs_diff_eq(expected.view_projection(state.aspect()), 3e-5),
                        "Returning must retain planar pan and zoom for {policy:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn planar_toggle_recaptures_each_new_free_orientation_but_not_intra_planar_changes() {
    let mut state = viewer();
    state.return_3d = PlanarExit::OrientationAndPerspective;
    for delta in [(0.0, 0.0), (24.0, 17.0), (-13.0, 21.0)] {
        state.orbit(delta.0, delta.1);
        let free = state.camera.clone();
        state.set_planar_navigation(true);
        assert!(state.is_planar_navigation());
        assert!(state.camera.is_orthographic());
        state.set_planar_navigation(true);
        state.set_view(View::Bottom);
        state.set_view(View::Back);
        state.set_planar_navigation(false);
        assert_basis(&state.camera, &free);
        assert!(!state.camera.is_orthographic());
    }
}

#[test]
fn entering_planar_captures_the_visible_orientation_of_an_interrupted_free_animation() {
    let mut state = viewer();
    state.return_3d = PlanarExit::OrientationAndPerspective;
    state.animate_views = true;
    state.view_duration_ms = 120;
    state.set_view(View::Perspective);
    state.camera.advance_transition(Duration::from_millis(35));
    assert!(state.camera.is_transitioning());
    let visible = state.camera.clone();
    let mut destination = state.camera.clone();
    destination.finish_transition();
    assert_ne!(basis(&visible), basis(&destination));
    state.set_view(View::Right);
    state.camera.finish_transition();
    state.set_view(View::Top);
    state.camera.finish_transition();
    state.set_planar_navigation(false);
    state.camera.finish_transition();
    assert_basis(&state.camera, &visible);
    assert!(!state.camera.is_orthographic());
}

#[test]
fn manual_orbit_leaves_planar_from_the_visible_pose_without_recalling_memory() {
    for policy in [
        PlanarExit::PerspectiveOnly,
        PlanarExit::OrientationOnly,
        PlanarExit::OrientationAndPerspective,
    ] {
        for animate in [false, true] {
            let mut state = viewer();
            state.return_3d = policy;
            state.animate_views = animate;
            state.set_view(View::Top);
            if animate {
                state.camera.advance_transition(Duration::from_millis(40));
            }
            let mut expected = state.camera.clone();
            expected.orbit(17.0, -11.0);
            state.orbit(17.0, -11.0);
            assert!(!state.is_planar_navigation());
            assert_eq!(
                state.camera.view_projection(state.aspect()),
                expected.view_projection(state.aspect())
            );
            assert!(!state.camera.is_transitioning());
            let orbited = state.camera.clone();
            state.set_planar_navigation(true);
            state.camera.finish_transition();
            state.return_3d = PlanarExit::OrientationOnly;
            state.set_planar_navigation(false);
            state.camera.finish_transition();
            assert_basis(&state.camera, &orbited);
        }
    }
}

#[test]
fn explicit_frame_new_and_open_do_not_reuse_an_old_planar_return_orientation() {
    for reset in 0..3 {
        let mut state = viewer();
        state.return_3d = PlanarExit::OrientationAndPerspective;
        state.set_view(View::Left);
        match reset {
            0 => state.frame_all(),
            1 => state.new_document(),
            _ => {
                let mut document = Document::default();
                document.insert_primitive(PrimitiveKind::Cube).unwrap();
                state
                    .install_document(PathBuf::from("navigation-return.obj"), document)
                    .unwrap();
            }
        }
        assert!(!state.is_planar_navigation());
        let reset_camera = state.camera.clone();
        assert_basis(&reset_camera, &Camera::default());
        state.set_view(View::Bottom);
        state.set_planar_navigation(false);
        assert_basis(&state.camera, &reset_camera);
    }
}

#[test]
fn planar_navigation_preserves_edit_selection_document_and_history() {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.convert_object(id).unwrap();
    let mut state = viewer();
    state.editor = Editor::new(document.clone()).unwrap();
    state.editor.select_object(id).unwrap();
    state.editor.enter_edit().unwrap();
    state.editor.selected_vertices.insert(1);
    state
        .editor
        .commit("Rename", |document| {
            document.objects[0].name = "Navigation preserves this edit".into();
            Ok(())
        })
        .unwrap();
    let edited = state.editor.document.clone();
    let revision = state.editor.revision;
    let vertices = state.editor.selected_vertices.clone();
    let objects = state.editor.selected_objects.clone();
    let frame = state.editor.frame;
    let dirty = state.is_dirty();
    for z_up in [false, true] {
        state.z_up = z_up;
        state.set_view(View::Front);
        state.pan(13.0, 25.0);
        state.scroll(0.0, -1.0, false, Modifiers::NONE, None);
        state.set_view(View::Top);
        state.set_planar_navigation(false);
    }
    assert_eq!(state.editor.document, edited);
    assert_eq!(state.editor.revision, revision);
    assert_eq!(state.editor.frame, frame);
    assert_eq!(state.editor.selected_objects, objects);
    assert_eq!(state.editor.selected_vertices, vertices);
    assert!(state.editor.edit_mode);
    assert_eq!(state.is_dirty(), dirty);
    assert!(state.editor.undo());
    assert_eq!(state.editor.document, document);
    assert!(!state.editor.undo(), "Navigation cannot add an undo entry");
    assert!(state.editor.redo());
    assert_eq!(state.editor.document, edited);
}

#[test]
fn paused_move_session_preserves_its_preview_across_navigation_and_return_orientation_changes() {
    let mut state = viewer();
    state.editor.set_tool(crate::editor::Tool::Move);
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    let before = state.editor.document.clone();
    let camera = state.camera.clone();
    state.editor.toggle_transform_axis(0).unwrap();
    state.editor.nudge(DVec3::X, false).unwrap();
    let preview = state.editor.document.clone();
    assert!(state.editor.has_transform_session());
    assert!(!state.editor.is_pointer_interacting());
    state.set_view(View::Top);
    assert!(state.is_planar_navigation());
    state.pan(24.0, -18.0);
    state.scroll(0.0, 1.0, false, Modifiers::NONE, None);
    state.set_planar_navigation(false);
    assert_basis(&state.camera, &camera);
    state.orbit(30.0, 20.0);
    let navigated = state.camera.clone();
    state.set_view(View::Right);
    state.set_planar_navigation(false);
    assert!(!state.is_planar_navigation());
    assert_basis(&state.camera, &navigated);
    assert_ne!(basis(&state.camera), basis(&camera));
    assert_eq!(state.editor.document, preview);
    assert!(state.editor.has_transform_session());
    assert_eq!(state.editor.transform_axis, Some(0));
    let pose = state.camera.view_projection(state.aspect());
    state.editor.escape();
    assert_eq!(state.editor.document, before);
    assert_eq!(state.camera.view_projection(state.aspect()), pose);
}

#[test]
fn real_gizmo_axis_then_3d_button_restore_the_pre_click_orientation() {
    let mut capture = pollster::block_on(Capture::new(WIDTH, HEIGHT)).unwrap();
    let mut session = Session::new(&mut capture).unwrap();
    session.load_fixture("bracket.obj").unwrap();
    session.state.animate_views = false;
    session.state.return_3d = PlanarExit::OrientationAndPerspective;
    session.state.orbit(71.0, -23.0);
    session.settle().unwrap();
    let remembered = session.state.camera.clone();
    let document = session.state.editor.document.clone();
    let revision = session.state.editor.revision;
    session.extra_layout_pass = true;
    session.click(Control::AxisX).unwrap();
    assert!(session.state.is_planar_navigation());
    session.click(Control::AxisZ).unwrap();
    assert!(session.state.is_planar_navigation());
    session.state.pan(21.0, -15.0);
    session.state.scroll(0.0, 1.0, false, Modifiers::NONE, None);
    let mut expected = session.state.camera.clone();
    expected.set_orientation_with_transition(remembered.orientation(), false, Transition::Instant);
    session.click(Control::NavigationFree).unwrap();
    assert!(!session.state.is_planar_navigation());
    assert_basis(&session.state.camera, &remembered);
    assert!(
        session
            .state
            .camera
            .view_projection(session.state.aspect())
            .abs_diff_eq(expected.view_projection(session.state.aspect()), 3e-5)
    );
    assert_eq!(session.state.editor.document, document);
    assert_eq!(session.state.editor.revision, revision);

    // A second axis press can freeze a snap between cardinal views. Its held
    // pose is still Planar, so the next snap cannot overwrite the Free memory.
    session.state.animate_views = true;
    session.state.view_duration_ms = 120;
    session.click(Control::AxisX).unwrap();
    session.frame(vec![], Duration::from_millis(40)).unwrap();
    assert!(session.state.is_planar_navigation());
    assert!(session.state.camera.is_transitioning());
    let next_axis = session.trace.get(Control::AxisZ).unwrap().rect.center();
    session
        .frame(
            vec![
                egui::Event::PointerMoved(next_axis),
                pointer(next_axis, true),
            ],
            Duration::ZERO,
        )
        .unwrap();
    assert!(session.state.is_planar_navigation());
    assert!(!session.state.camera.is_transitioning());
    let held = session.state.camera.view_projection(session.state.aspect());
    session.frame(vec![], Duration::from_millis(30)).unwrap();
    assert!(session.state.is_planar_navigation());
    assert_eq!(
        session.state.camera.view_projection(session.state.aspect()),
        held
    );
    session
        .frame(vec![pointer(next_axis, false)], Duration::ZERO)
        .unwrap();
    assert!(session.state.is_planar_navigation());
    session.frame(vec![], Duration::from_millis(120)).unwrap();
    session.click(Control::NavigationFree).unwrap();
    session.frame(vec![], Duration::from_millis(120)).unwrap();
    assert!(!session.state.is_planar_navigation());
    assert_basis(&session.state.camera, &remembered);
    assert_eq!(session.state.editor.document, document);
    assert_eq!(session.state.editor.revision, revision);

    // Interrupting a return to Free on pointer-down must not make its temporary
    // visible angle the remembered orientation for the next Planar snap.
    session.state.set_view(View::Front);
    session.frame(vec![], Duration::from_millis(120)).unwrap();
    assert!(session.state.is_planar_navigation());
    session.click(Control::NavigationFree).unwrap();
    session.frame(vec![], Duration::from_millis(30)).unwrap();
    assert!(!session.state.is_planar_navigation());
    assert!(session.state.camera.is_transitioning());
    assert_ne!(basis(&session.state.camera), basis(&remembered));
    let return_pose = session.state.camera.view_projection(session.state.aspect());
    let next_axis = session.trace.get(Control::AxisX).unwrap().rect.center();
    session
        .frame(
            vec![
                egui::Event::PointerMoved(next_axis),
                pointer(next_axis, true),
            ],
            Duration::ZERO,
        )
        .unwrap();
    assert!(
        !session.state.is_planar_navigation(),
        "An axis press alone has not snapped yet"
    );
    assert!(!session.state.camera.is_transitioning());
    assert_eq!(
        session.state.camera.view_projection(session.state.aspect()),
        return_pose
    );
    session
        .frame(vec![pointer(next_axis, false)], Duration::ZERO)
        .unwrap();
    assert!(session.state.is_planar_navigation());
    session.frame(vec![], Duration::from_millis(120)).unwrap();
    session.click(Control::NavigationFree).unwrap();
    session.frame(vec![], Duration::from_millis(120)).unwrap();
    assert!(!session.state.is_planar_navigation());
    assert_basis(&session.state.camera, &remembered);
    assert_eq!(session.state.editor.document, document);
    assert_eq!(session.state.editor.revision, revision);
}
