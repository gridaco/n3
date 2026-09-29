use super::*;

fn test_context() -> egui::Context {
    let context = egui::Context::default();
    configure_context(&context);
    context
}

fn layout_test_frame(state: &mut WorkspaceUi, ctx: &egui::Context, events: Vec<egui::Event>) {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            state.ui(ui);
        },
    )
    .textures_delta
    .clear();
}

#[test]
fn inserting_planar_shapes_in_2d_faces_the_view_without_leaving_2d() {
    for kind in [PrimitiveKind::Plane, PrimitiveKind::Circle] {
        for z_up in [false, true] {
            let mut state = WorkspaceUi::new(egui::TextureId::User(0));
            state.z_up = z_up;
            state.set_view(View::Right);
            state.set_planar_navigation(true);
            state.insert_shape(kind);
            assert!(state.is_planar_navigation());
            assert!(state.error.is_none(), "{:?}", state.error);
            let id = state.editor.selected_object.unwrap();
            let object = state
                .editor
                .document
                .objects
                .iter()
                .find(|object| object.id == id)
                .unwrap();
            assert!(
                matches!(object.geometry, Geometry::Primitive(ref primitive) if primitive.kind == kind)
            );
            let normal = glam::DQuat::from_array(object.transform.rotation) * glam::DVec3::Z;
            let source_view = crate::orientation::display_rotation(z_up)
                .inverse()
                .transform_vector3(state.camera.nearest_axis_direction())
                .as_dvec3();
            assert!(normal.distance(source_view) < 1e-6);
        }
    }
}

#[test]
fn inspector_exposes_only_controls_owned_by_the_inserted_shape() {
    let ctx = test_context();
    controls::enable(&ctx);
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));

    state.insert_shape(PrimitiveKind::Plane);
    layout_test_frame(&mut state, &ctx, vec![]);
    let plane = controls::snapshot(&ctx);
    assert!(plane.get(Control::PrimitiveX).is_ok());
    assert!(plane.get(Control::PrimitiveY).is_ok());
    assert!(plane.get(Control::PrimitiveZ).is_err());
    assert!(plane.get(Control::Segments).is_err());

    state.insert_shape(PrimitiveKind::Circle);
    layout_test_frame(&mut state, &ctx, vec![]);
    let circle = controls::snapshot(&ctx);
    for control in [
        Control::CircleRadius,
        Control::CircleVertices,
        Control::CircleFill,
    ] {
        assert!(
            circle.get(control).is_ok(),
            "Circle must expose {control:?}"
        );
    }
    for control in [
        Control::PrimitiveX,
        Control::PrimitiveY,
        Control::PrimitiveZ,
        Control::Segments,
        Control::MinorSegments,
        Control::SphereRings,
        Control::PolyhedronSize,
    ] {
        assert!(
            circle.get(control).is_err(),
            "Circle must not expose {control:?}"
        );
    }
    assert!(
        matches!(&state.editor.document.objects.last().unwrap().geometry,
        Geometry::Primitive(primitive) if !primitive.fill && primitive.segments == 32)
    );

    state.insert_shape(PrimitiveKind::Sphere);
    layout_test_frame(&mut state, &ctx, vec![]);
    let sphere = controls::snapshot(&ctx);
    assert!(sphere.get(Control::PrimitiveZ).is_ok());
    assert!(sphere.get(Control::Segments).is_ok());
    assert!(sphere.get(Control::SphereRings).is_ok());
    assert!(sphere.get(Control::MinorSegments).is_err());

    state.insert_shape(PrimitiveKind::Polyhedron);
    layout_test_frame(&mut state, &ctx, vec![]);
    let polyhedron = controls::snapshot(&ctx);
    assert!(polyhedron.get(Control::PrimitiveX).is_err());
    assert!(polyhedron.get(Control::PolyhedronTypeMenu).is_ok());
    assert!(polyhedron.get(Control::PolyhedronSize).is_ok());
    assert!(polyhedron.get(Control::Segments).is_err());
    assert!(polyhedron.get(Control::SphereRings).is_err());
}

#[test]
fn main_menu_logo_is_centered_inside_the_existing_button() {
    let ctx = test_context();
    controls::enable(&ctx);
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1600.0, 900.0),
            )),
            ..Default::default()
        },
        |ui| {
            state.ui(ui);
        },
    );
    let button = controls::snapshot(&ctx).get(Control::N3Menu).unwrap().rect;
    assert!((button.width() - theme::size::STEP_8).abs() < 0.1);
    let logo = state.n3_logo_texture.as_ref().unwrap();
    assert_eq!(logo.size(), [48, 48]);
    let painted = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == logo.id() => Some(mesh.calc_bounds()),
            _ => None,
        })
        .expect("the menu paints the bundled logo");
    assert!(button.contains_rect(painted));
    assert!((painted.width() - theme::size::STEP_6).abs() < 0.1);
    assert!((painted.height() - theme::size::STEP_6).abs() < 0.1);
    assert!(painted.center().distance(button.center()) < 0.1);
    output.textures_delta.clear();
}

#[test]
fn main_menu_and_insert_stack_use_their_new_locations_and_real_menu_paths() {
    let ctx = test_context();
    controls::enable(&ctx);
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let click = |state: &mut WorkspaceUi, point| {
        layout_test_frame(
            state,
            &ctx,
            vec![
                egui::Event::PointerMoved(point),
                egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        layout_test_frame(
            state,
            &ctx,
            vec![egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    };
    layout_test_frame(&mut state, &ctx, vec![]);
    let trace = controls::snapshot(&ctx);
    let status = trace.get(Control::StatusBar).unwrap().rect;
    assert!((status.height() - theme::size::XL_4).abs() < 0.1);
    assert!((status.top() - state.viewport.bottom()).abs() < 0.1);
    let n3 = trace.get(Control::N3Menu).unwrap().rect;
    let insert = trace.get(Control::InsertMenu).unwrap().rect;
    let tool = trace.get(Control::ToolView).unwrap().rect;
    let move_tool = trace.get(Control::ToolMove).unwrap().rect;
    let toolbar = trace.get(Control::ViewportToolbar).unwrap().rect;
    assert!(
        trace
            .get(Control::ObjectList)
            .unwrap()
            .rect
            .contains_rect(n3)
    );
    assert!(state.viewport.contains_rect(insert));
    assert!((insert.width() - insert.height()).abs() < 1.0);
    assert!((insert.left() - toolbar.left()).abs() < 1.0);
    assert!((insert.right() - toolbar.right()).abs() < 1.0);
    assert!((toolbar.top() - insert.bottom() - theme::space::LG).abs() < 1.0);
    assert!((n3.width() - n3.height()).abs() < 1.0);
    assert!((n3.width() - N3_MENU_SIZE).abs() < 1.0);
    assert!((insert.center().x - tool.center().x).abs() < 1.0);
    assert!(tool.top() > insert.bottom());
    assert!(move_tool.top() > tool.bottom());
    assert!(
        (tool.center().x - move_tool.center().x).abs() < 1.0,
        "tool={tool:?} move={move_tool:?}"
    );
    let base_tools = [
        Control::ToolView,
        Control::ToolMove,
        Control::ToolRotate,
        Control::ToolScale,
    ]
    .map(|control| trace.get(control).unwrap().rect);
    let base_toolbar = trace.get(Control::ViewportToolbar).unwrap().rect;
    for (control, rect) in [
        Control::ToolView,
        Control::ToolMove,
        Control::ToolRotate,
        Control::ToolScale,
    ]
    .into_iter()
    .zip(base_tools)
    {
        layout_test_frame(
            &mut state,
            &ctx,
            vec![egui::Event::PointerMoved(rect.center())],
        );
        layout_test_frame(&mut state, &ctx, vec![]);
        let hovered = controls::snapshot(&ctx);
        assert_eq!(hovered.get(control).unwrap().rect, rect, "{control:?}");
        assert_eq!(
            hovered.get(Control::ViewportToolbar).unwrap().rect,
            base_toolbar
        );
    }
    for rect in [insert, n3] {
        layout_test_frame(
            &mut state,
            &ctx,
            vec![egui::Event::PointerMoved(rect.center())],
        );
        layout_test_frame(&mut state, &ctx, vec![]);
        let hovered = controls::snapshot(&ctx);
        assert_eq!(hovered.get(Control::InsertMenu).unwrap().rect, insert);
        assert_eq!(hovered.get(Control::N3Menu).unwrap().rect, n3);
        assert_eq!(
            hovered.get(Control::ViewportToolbar).unwrap().rect,
            base_toolbar
        );
    }
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.editor.enter_edit().unwrap();
    layout_test_frame(&mut state, &ctx, vec![]);
    let trace = controls::snapshot(&ctx);
    let edit = trace.get(Control::EditToolbar).unwrap().rect;
    assert!((edit.center().x - state.viewport.center().x).abs() < 1.0);
    assert!((state.viewport.bottom() - edit.bottom() - theme::space::XL).abs() < 1.0);
    assert!(edit.left() > trace.get(Control::ViewportToolbar).unwrap().rect.right());
    state.editor.leave_edit();
    layout_test_frame(&mut state, &ctx, vec![]);

    click(&mut state, n3.center());
    let trace = controls::snapshot(&ctx);
    assert_eq!(
        trace.get(Control::FileMenu).unwrap().parents,
        [Control::N3Menu]
    );
    assert_eq!(
        trace.get(Control::ViewMenu).unwrap().parents,
        [Control::N3Menu]
    );
    assert!(trace.get(Control::FileMenu).unwrap().rect.width() >= theme::size::STEP_52);
    assert!(trace.get(Control::ViewMenu).unwrap().rect.width() >= theme::size::STEP_52);
    click(
        &mut state,
        trace.get(Control::FileMenu).unwrap().rect.center(),
    );
    let trace = controls::snapshot(&ctx);
    assert_eq!(
        trace.get(Control::New).unwrap().parents,
        [Control::N3Menu, Control::FileMenu]
    );
    assert!(trace.get(Control::New).unwrap().rect.width() >= theme::size::STEP_52);

    let view_ctx = test_context();
    controls::enable(&view_ctx);
    let mut view_state = WorkspaceUi::new(egui::TextureId::User(0));
    layout_test_frame(&mut view_state, &view_ctx, vec![]);
    let n3 = controls::snapshot(&view_ctx)
        .get(Control::N3Menu)
        .unwrap()
        .rect;
    let click_view = |state: &mut WorkspaceUi, point| {
        for pressed in [true, false] {
            layout_test_frame(
                state,
                &view_ctx,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    };
    click_view(&mut view_state, n3.center());
    let view = controls::snapshot(&view_ctx)
        .get(Control::ViewMenu)
        .unwrap()
        .rect;
    click_view(&mut view_state, view.center());
    assert_eq!(
        controls::snapshot(&view_ctx)
            .get(Control::ViewPerspective)
            .unwrap()
            .parents,
        [Control::N3Menu, Control::ViewMenu]
    );
    assert!(controls::snapshot(&view_ctx).get(Control::ZUp).is_err());
}

#[test]
fn resolved_appearance_reaches_panels_in_the_same_pass() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        state.theme_mode = mode;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600.0, 900.0),
                )),
                focused: true,
                ..Default::default()
            },
            |ui| {
                state.ui(ui);
            },
        );
        output.textures_delta.clear();
        let panel_fill = ctx.global_style().visuals.panel_fill;
        // This point lies in the unused body of the hierarchy, clear of text
        // and controls. A global style change alone does not update the root
        // Ui that run_ui already created before WorkspaceUi resolves theme.
        let point = egui::pos2(100.0, 500.0);
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.rect.contains(point) && rect.fill == panel_fill)
            }),
            "{mode:?} must paint the hierarchy with its resolved surface immediately"
        );
    }
}

#[test]
fn saved_layout_restores_resized_panels_without_touching_editor_state() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    let document = state.editor.document.clone();
    let revision = state.editor.revision;
    layout_test_frame(&mut state, &ctx, Vec::new());
    layout_test_frame(&mut state, &ctx, Vec::new());
    let initial = state.save_layout(&ctx);
    let camera = state.camera.view_projection(state.aspect());
    assert_eq!(initial, SavedLayout::DEFAULT);

    let divider = egui::pos2(initial.hierarchy_width - 4.0, 500.0);
    let moved = egui::pos2(320.0, 500.0);
    layout_test_frame(&mut state, &ctx, vec![egui::Event::PointerMoved(divider)]);
    layout_test_frame(
        &mut state,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: divider,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    layout_test_frame(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(280.0, 500.0))],
    );
    layout_test_frame(&mut state, &ctx, vec![egui::Event::PointerMoved(moved)]);
    layout_test_frame(
        &mut state,
        &ctx,
        vec![egui::Event::PointerButton {
            pos: moved,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    let resized = state.save_layout(&ctx);
    assert!(
        resized.hierarchy_width > initial.hierarchy_width + 50.0,
        "initial={initial:?}, resized={resized:?}"
    );
    assert_eq!(resized.inspector_width, initial.inspector_width);

    state.apply_saved_layout(&ctx, initial);
    layout_test_frame(&mut state, &ctx, Vec::new());
    assert_eq!(state.save_layout(&ctx), initial);
    assert_eq!(state.editor.document, document);
    assert_eq!(state.editor.revision, revision);
    assert_eq!(state.camera.view_projection(state.aspect()), camera);
}

#[test]
fn saved_layout_normalizes_invalid_widths() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.apply_saved_layout(
        &ctx,
        SavedLayout {
            hierarchy_width: f32::NAN,
            inspector_width: f32::INFINITY,
        },
    );
    assert_eq!(state.save_layout(&ctx).hierarchy_width, 240.0);
    assert_eq!(state.save_layout(&ctx).inspector_width, 240.0);
    state.apply_saved_layout(
        &ctx,
        SavedLayout {
            hierarchy_width: -20.0,
            inspector_width: 9_000.0,
        },
    );
    layout_test_frame(&mut state, &ctx, Vec::new());
    let actual = state.save_layout(&ctx);
    assert_eq!(actual.hierarchy_width, HIERARCHY_MIN_WIDTH);
    assert_eq!(actual.inspector_width, 480.0);
    assert!(state.show_ui);
}

#[test]
fn local_view_is_reversible_view_state_and_frames_whole_objects() {
    let ctx = test_context();
    let mut document = Document::default();
    let left = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let right = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.objects[0].transform.translation[0] = -20.0;
    document.objects[1].transform.translation[0] = 20.0;
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    state.editor = Editor::new(document.clone()).unwrap();
    state.editor.select_object(left).unwrap();
    state.refresh_mesh().unwrap();
    state.set_view(View::Front);
    state.camera.finish_transition();
    state.camera.pan(37.0, -15.0, 600.0);
    let before_camera = state.camera.view_projection(state.aspect());
    let before_revision = state.editor.revision;
    let before_mesh_revision = state.mesh_revision;
    let before_navigation = state.is_planar_navigation();

    state.dispatch(Command::ToggleLocalView, &ctx, false);
    assert!(state.is_local_view());
    assert_eq!(state.visible_objects(), Some(&BTreeSet::from([left])));
    assert!(!state.editor.is_object_visible(right));
    assert_eq!(state.editor.selected_objects, BTreeSet::from([left]));
    assert!(state.camera.is_transitioning());
    assert_eq!(state.camera.view_projection(state.aspect()), before_camera);
    state.camera.advance_transition(Duration::from_millis(60));
    assert!(state.camera.is_transitioning());
    assert_ne!(state.camera.view_projection(state.aspect()), before_camera);
    state.camera.finish_transition();
    assert_eq!(state.editor.document, document);
    assert_eq!(state.editor.revision, before_revision);
    state.refresh_mesh().unwrap();
    assert_eq!(state.mesh_revision, before_mesh_revision);

    state.frame_all();
    assert!(state.is_local_view());
    state.camera.orbit(10.0, 5.0);
    let isolated_camera = state.camera.view_projection(state.aspect());
    state.dispatch(Command::ToggleLocalView, &ctx, false);
    assert!(!state.is_local_view());
    assert!(state.visible_objects().is_none());
    assert!(state.editor.is_object_visible(right));
    assert!(state.camera.is_transitioning());
    assert_eq!(
        state.camera.view_projection(state.aspect()),
        isolated_camera
    );
    state.camera.advance_transition(Duration::from_millis(60));
    assert!(state.camera.is_transitioning());
    assert_ne!(
        state.camera.view_projection(state.aspect()),
        isolated_camera
    );
    assert_ne!(state.camera.view_projection(state.aspect()), before_camera);
    state.camera.finish_transition();
    assert_eq!(state.camera.view_projection(state.aspect()), before_camera);
    assert_eq!(state.is_planar_navigation(), before_navigation);
    assert_eq!(state.editor.document, document);
    assert_eq!(state.editor.revision, before_revision);
    state.refresh_mesh().unwrap();
    assert_eq!(state.mesh_revision, before_mesh_revision);
}

#[test]
fn local_view_respects_animation_preferences_in_both_directions() {
    for (animate, milliseconds) in [(false, 300), (true, 0), (true, 300)] {
        let mut state = WorkspaceUi::new(egui::TextureId::User(0));
        state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let selected = state.editor.insert(PrimitiveKind::Cube).unwrap();
        state.refresh_mesh().unwrap();
        state.camera.pan(170.0, -60.0, 600.0);
        state.animate_views = animate;
        state.view_duration_ms = milliseconds;
        let full_camera = state.camera.view_projection(state.aspect());
        let mut fit = state.camera.clone();
        let points =
            local_view::object_points(&state.editor, &BTreeSet::from([selected]), false).unwrap();
        assert!(fit.frame_points(&points, state.aspect()));
        let fit_camera = fit.view_projection(state.aspect());
        assert_ne!(fit_camera, full_camera);
        for expected in [fit_camera, full_camera] {
            let previous = state.camera.view_projection(state.aspect());
            state.toggle_local_view();
            if animate && milliseconds > 0 {
                assert!(state.camera.is_transitioning());
                assert_eq!(state.camera.view_projection(state.aspect()), previous);
                state.camera.advance_transition(Duration::from_millis(299));
                assert!(state.camera.is_transitioning());
                state.camera.advance_transition(Duration::from_millis(1));
            }
            assert!(!state.camera.is_transitioning());
            assert_eq!(state.camera.view_projection(state.aspect()), expected);
        }
    }
}

#[test]
fn rapid_local_view_toggles_keep_the_original_camera_destination() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.refresh_mesh().unwrap();
    state.camera.pan(170.0, -60.0, 600.0);
    // Entry during an axis-view transition must still reach that axis, so the
    // restored Planar mode never ends up stranded at an intermediate angle.
    state.set_view(View::Right);
    state.camera.advance_transition(Duration::from_millis(30));
    let mut destination = state.camera.clone();
    destination.finish_transition();
    let full_camera = destination.view_projection(state.aspect());
    for _ in 0..3 {
        let before_entry = state.camera.view_projection(state.aspect());
        state.toggle_local_view();
        assert!(state.is_local_view());
        assert_eq!(state.camera.view_projection(state.aspect()), before_entry);
        state.camera.advance_transition(Duration::from_millis(30));
        let before_exit = state.camera.view_projection(state.aspect());
        state.toggle_local_view();
        assert!(!state.is_local_view());
        assert_eq!(state.camera.view_projection(state.aspect()), before_exit);
        state.camera.advance_transition(Duration::from_millis(30));
    }
    state.camera.finish_transition();
    assert_eq!(state.camera.view_projection(state.aspect()), full_camera);
    assert!(state.is_planar_navigation());
    assert!(state.camera.is_orthographic());
}

#[test]
fn local_view_handles_empty_selection_edit_mode_and_new_objects() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let first = state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.editor.deselect();
    let camera = state.camera.view_projection(state.aspect());
    state.toggle_local_view();
    assert!(!state.is_local_view());
    assert_eq!(state.camera.view_projection(state.aspect()), camera);

    state.editor.select_object(first).unwrap();
    state.editor.convert_selected().unwrap();
    state.editor.enter_edit().unwrap();
    state.editor.selected_vertices.clear();
    assert!(state.editor.selection_points(false).unwrap().is_empty());
    assert!(
        !local_view::object_points(&state.editor, &BTreeSet::from([first]), false)
            .unwrap()
            .is_empty()
    );
    state.toggle_local_view();
    assert!(state.is_local_view());
    assert_eq!(state.visible_objects(), Some(&BTreeSet::from([first])));
    state.toggle_local_view();
    state.editor.leave_edit();

    state.toggle_local_view();
    let created = state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.refresh_mesh().unwrap();
    assert_eq!(
        state.visible_objects(),
        Some(&BTreeSet::from([first, created]))
    );
    state.editor.undo();
    state.refresh_mesh().unwrap();
    assert_eq!(state.visible_objects(), Some(&BTreeSet::from([first])));
    state.editor.redo();
    state.refresh_mesh().unwrap();
    assert_eq!(
        state.visible_objects(),
        Some(&BTreeSet::from([first, created]))
    );
    state.editor.select_object(first).unwrap();
    state.editor.delete_selection().unwrap();
    state.refresh_mesh().unwrap();
    assert!(state.is_local_view());
    state.editor.select_object(created).unwrap();
    state.editor.delete_selection().unwrap();
    state.refresh_mesh().unwrap();
    assert!(!state.is_local_view());
}

#[test]
fn local_view_exits_without_selection_and_clears_on_document_replacement() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let id = state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.toggle_local_view();
    state.editor.deselect();
    assert!(state.is_local_view());
    state.toggle_local_view();
    assert!(!state.is_local_view());
    assert!(state.visible_objects().is_none());

    state.editor.select_object(id).unwrap();
    state.toggle_local_view();
    state.new_document();
    assert!(!state.is_local_view());
    assert!(state.visible_objects().is_none());

    state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.toggle_local_view();
    assert!(state.is_local_view());
    let mut imported = Document::default();
    imported.insert_primitive(PrimitiveKind::Cube).unwrap();
    state
        .install_document(PathBuf::from("local-view-reset.obj"), imported)
        .unwrap();
    assert!(!state.is_local_view());
    assert!(state.visible_objects().is_none());
}

#[test]
fn local_view_does_not_reveal_objects_restored_from_prior_history() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let first = state.editor.insert(PrimitiveKind::Cube).unwrap();
    let hidden = state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.editor.delete_selection().unwrap();
    state.editor.select_object(first).unwrap();
    state.toggle_local_view();
    state.editor.undo();
    state.refresh_mesh().unwrap();
    assert!(
        state
            .editor
            .document
            .objects
            .iter()
            .any(|object| object.id == hidden)
    );
    assert!(state.is_local_view());
    assert_eq!(state.visible_objects(), Some(&BTreeSet::from([first])));
    assert!(!state.editor.is_object_visible(hidden));
}

#[test]
fn snapping_preferences_survive_document_creation_and_import() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let settings = crate::snapping::SnapSettings {
        enabled: false,
        step_cm: 0.25,
        reference: crate::snapping::GridReference::Relative,
    };
    state.editor.snapping = settings;
    let policy = StepPolicy::Adaptive {
        max_step_points: 8.0,
    };
    state.editor.snap_policy = policy;
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.new_document();
    assert_eq!(state.editor.snapping, settings);
    assert_eq!(state.editor.snap_policy, policy);
    assert!(state.editor.document.objects.is_empty());

    let mut imported = Document::default();
    imported.insert_primitive(PrimitiveKind::Cube).unwrap();
    state
        .install_document(
            PathBuf::from("snapping-preference-test.obj"),
            imported.clone(),
        )
        .unwrap();
    assert_eq!(state.editor.snapping, settings);
    assert_eq!(state.editor.snap_policy, policy);
    assert_eq!(state.editor.document, imported);
}

fn snapping_ui_frame(
    state: &mut WorkspaceUi,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> controls::Trace {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1160.0, 900.0),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            state.ui(ui);
        },
    )
    .textures_delta
    .clear();
    controls::snapshot(ctx)
}

fn snapping_ui_click(
    state: &mut WorkspaceUi,
    ctx: &egui::Context,
    control: Control,
) -> controls::Trace {
    let position = controls::snapshot(ctx).get(control).unwrap().rect.center();
    for pressed in [true, false] {
        snapping_ui_frame(
            state,
            ctx,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    snapping_ui_frame(state, ctx, Vec::new())
}

#[test]
fn preferences_header_aligns_title_and_close_with_the_content_inset() {
    let ctx = test_context();
    controls::enable(&ctx);
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.show_preferences = true;
    let trace = snapping_ui_frame(&mut state, &ctx, Vec::new());
    let window = trace.get(Control::PreferencesWindow).unwrap().rect;
    let title = trace.get(Control::PreferencesTitle).unwrap().rect;
    let close = trace.get(Control::PreferencesClose).unwrap().rect;
    assert!(window.contains_rect(title));
    assert!(window.contains_rect(close));
    assert!((title.left() - window.left() - (window.right() - close.right())).abs() < 2.0);
    assert!(
        (title.center().y - close.center().y).abs() < 2.0,
        "title={title:?}, close={close:?}, window={window:?}"
    );
    assert!(close.left() > title.right());
    snapping_ui_frame(
        &mut state,
        &ctx,
        vec![egui::Event::PointerMoved(close.center())],
    );
    snapping_ui_click(&mut state, &ctx, Control::PreferencesClose);
    assert!(!state.show_preferences);
}

#[test]
fn preferences_select_auto_or_fixed_without_changing_geometry() {
    let ctx = test_context();
    controls::enable(&ctx);
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    let before = state.editor.document.clone();
    state.show_preferences = true;
    snapping_ui_frame(&mut state, &ctx, Vec::new());
    let trace = snapping_ui_frame(&mut state, &ctx, Vec::new());
    assert_eq!(
        trace.get(Control::ZUp).unwrap().parents,
        [Control::PreferencesWindow]
    );
    assert!(!trace.get(Control::SnapStep).unwrap().enabled);
    snapping_ui_click(&mut state, &ctx, Control::SnapSpacingMenu);
    let trace = snapping_ui_click(&mut state, &ctx, Control::SnapFixed);
    assert_eq!(state.editor.snap_policy, StepPolicy::Fixed);
    assert!(trace.get(Control::SnapStep).unwrap().enabled);
    state.editor.snapping.step_cm = 0.25;
    snapping_ui_click(&mut state, &ctx, Control::SnapSpacingMenu);
    let trace = snapping_ui_click(&mut state, &ctx, Control::SnapAdaptive);
    assert_eq!(state.editor.snap_policy, StepPolicy::default());
    assert!(!trace.get(Control::SnapStep).unwrap().enabled);
    assert_eq!(state.editor.snapping.step_cm, 0.25);
    assert_eq!(state.editor.document, before);
    let trace = snapping_ui_click(&mut state, &ctx, Control::SnapGrid);
    assert!(!state.editor.snapping.enabled);
    assert!(!trace.get(Control::SnapSpacingMenu).unwrap().enabled);
}

#[test]
fn snap_feedback_reformats_units_and_never_captures_viewport_pointer() {
    let ctx = test_context();
    controls::enable(&ctx);
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.editor.set_tool(Tool::Move);
    state.editor.snap_policy = StepPolicy::Fixed;
    state.editor.snapping.step_cm = 0.02;
    snapping_ui_frame(&mut state, &ctx, Vec::new());
    let trace = snapping_ui_frame(&mut state, &ctx, Vec::new());
    let feedback = trace.get(Control::SnapFeedback).unwrap();
    let stats = trace.get(Control::SceneInfo).unwrap().rect;
    assert_eq!(feedback.label, "Snap: 0.02 cm");
    assert!(state.viewport.contains_rect(feedback.rect));
    assert!((stats.left() - state.viewport.left() - theme::space::XL).abs() < 1.0);
    assert!((state.viewport.bottom() - stats.bottom() - theme::space::XL).abs() < 1.0);
    assert!((feedback.rect.left() - stats.left()).abs() < 1.0);
    assert!((stats.top() - feedback.rect.bottom() - theme::space::LG).abs() < 1.0);
    assert!(crate::navigation_events::viewport_accepts_pointer(
        &ctx,
        state.viewport,
        feedback.rect.center()
    ));
    let before = state.editor.document.clone();
    state.display_unit = LengthUnit::Millimeters;
    let trace = snapping_ui_frame(&mut state, &ctx, Vec::new());
    assert_eq!(
        trace.get(Control::SnapFeedback).unwrap().label,
        "Snap: 0.2 mm"
    );
    assert_eq!(state.editor.document, before);
    state.editor.snapping.enabled = false;
    let trace = snapping_ui_frame(&mut state, &ctx, Vec::new());
    assert!(trace.get(Control::SnapFeedback).is_err());
    state.editor.snapping.enabled = true;
    state.editor.set_tool(Tool::View);
    let trace = snapping_ui_frame(&mut state, &ctx, Vec::new());
    assert!(trace.get(Control::SnapFeedback).is_err());
}

#[test]
fn snap_feedback_keeps_tiny_lengths_visible_and_falls_back_from_unit_overflow() {
    assert_eq!(
        movement_snap_label(1.0e-200, LengthUnit::Centimeters),
        "Snap: 1e-200 cm"
    );
    assert_eq!(
        movement_snap_label(f64::MAX, LengthUnit::Millimeters),
        "Snap: 1.79769e308 cm"
    );
    assert_eq!(
        movement_snap_label(f64::from_bits(1), LengthUnit::Meters),
        "Snap: 5e-324 cm"
    );
}

#[test]
fn explicit_projection_and_orbit_commands_keep_their_own_navigation_semantics() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.animate_views = false;
    state.set_view(View::Left);
    assert!(state.has_planar_view());
    state.orbit(0.0, 0.0);
    state.orbit(f32::NAN, 2.0);
    assert!(state.has_planar_view());
    state.orbit(20.0, -10.0);
    assert!(!state.is_planar_navigation());
    assert!(state.camera.is_orthographic());
    state.set_planar_navigation(true);
    assert!(state.has_planar_view());
    let aligned = state.camera.orientation();
    state.toggle_projection();
    assert!(!state.is_planar_navigation() && !state.camera.is_orthographic());
    assert_eq!(state.camera.orientation(), aligned);
    state.toggle_projection();
    assert!(!state.is_planar_navigation() && state.camera.is_orthographic());
    state.set_view(View::Bottom);
    state.frame_all();
    assert!(!state.is_planar_navigation());
    assert_eq!(state.camera.orientation(), Camera::default().orientation());
}

#[test]
fn entering_planar_uses_current_orientation_for_all_six_axes_and_preserves_framing() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    let document = state.editor.document.clone();
    let selection = state.editor.selected_objects.clone();
    for z_up in [false, true] {
        state.z_up = z_up;
        for animate in [false, true] {
            state.animate_views = animate;
            for axis in [
                glam::Vec3::X,
                -glam::Vec3::X,
                glam::Vec3::Y,
                -glam::Vec3::Y,
                glam::Vec3::Z,
                -glam::Vec3::Z,
            ] {
                state.set_view(View::Front);
                state.camera.finish_transition();
                state.set_planar_navigation(false);
                state
                    .camera
                    .look_from(axis + glam::Vec3::new(0.15, -0.12, 0.2));
                state.camera.toggle_projection();
                state.camera.pan(37.0, -25.0, 600.0);
                state.camera.zoom(0.1);
                let visible = state.camera.view_projection(1.0);
                let mut expected = state.camera.clone();
                expected.look_from(axis);
                state.set_planar_navigation(true);
                if animate {
                    assert_eq!(state.camera.view_projection(1.0), visible);
                    assert!(state.camera.is_transitioning());
                    state.camera.finish_transition();
                }
                assert!(state.has_planar_view());
                assert!(
                    state
                        .camera
                        .view_projection(1.0)
                        .abs_diff_eq(expected.view_projection(1.0), 1e-5)
                );
                assert!(
                    state
                        .camera
                        .direction_in_view(axis)
                        .abs_diff_eq(glam::Vec3::Z, 1e-5)
                );
                assert_eq!(state.editor.document, document);
                assert_eq!(state.editor.selected_objects, selection);
            }
        }
    }
}

#[test]
fn pan_and_zoom_during_axis_animation_finish_alignment_before_applying_motion() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    for zoom in [false, true] {
        state.set_view(View::Perspective);
        state.camera.finish_transition();
        state.set_view(View::Top);
        state.camera.advance_transition(Duration::from_millis(30));
        assert!(state.is_planar_navigation() && state.camera.is_transitioning());
        state.trackpad_scroll(0.0, 0.0, egui::Modifiers::NONE, ScrollPhase::Started, None);
        assert!(state.camera.is_transitioning());
        if zoom {
            state.pinch(0.2, None);
        } else {
            state.trackpad_scroll(10.0, 15.0, egui::Modifiers::NONE, ScrollPhase::Moved, None);
        }
        assert!(state.has_planar_view());
        assert!(
            state
                .camera
                .direction_in_view(glam::Vec3::Y)
                .abs_diff_eq(glam::Vec3::Z, 1e-5)
        );
    }
}

#[test]
fn native_twist_preserves_pending_planar_snap_and_still_orbits_in_free_navigation() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.set_view(View::Top);
    state.camera.advance_transition(Duration::from_millis(30));
    assert!(state.is_planar_navigation() && state.camera.is_transitioning());
    let mut expected = state.camera.clone();
    let visible = expected.view_projection(1.0);
    for degrees in [0.0, 0.01, -0.01, 15.0, -180.0, f32::NAN, f32::INFINITY] {
        state.trackpad_rotate(degrees);
        assert_eq!(state.camera.view_projection(1.0), visible);
        assert!(state.is_planar_navigation() && state.camera.is_transitioning());
    }
    state.camera.advance_transition(Duration::from_millis(90));
    expected.advance_transition(Duration::from_millis(90));
    assert_eq!(
        state.camera.view_projection(1.0),
        expected.view_projection(1.0)
    );
    assert!(state.has_planar_view());

    // Free navigation can be orthographic too; mode owns twist policy.
    state.return_3d = PlanarExit::OrientationOnly;
    state.set_planar_navigation(false);
    state.camera.finish_transition();
    let facing = state.camera.direction_in_view(glam::Vec3::Z);
    state.trackpad_rotate(15.0);
    assert!(!state.is_planar_navigation());
    assert!(state.camera.is_orthographic());
    assert_ne!(state.camera.direction_in_view(glam::Vec3::Z), facing);
}

#[test]
fn native_shift_scroll_pans_without_exiting_planar_through_momentum() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    state.animate_views = false;
    for planar in [true, false] {
        state.set_view(View::Front);
        if !planar {
            state.orbit(20.0, -15.0);
        }
        for phase in [
            ScrollPhase::Started,
            ScrollPhase::Moved,
            ScrollPhase::Ended,
            ScrollPhase::Started,
            ScrollPhase::Ended,
        ] {
            let before = state.camera.orientation();
            let mut expected = state.camera.clone();
            expected.pan(10.0, 15.0, state.viewport.height());
            state.trackpad_scroll(10.0, 15.0, egui::Modifiers::SHIFT, phase, None);
            assert_eq!(state.is_planar_navigation(), planar);
            assert_eq!(state.camera.orientation(), before);
            assert!(
                state
                    .camera
                    .view_projection(1.0)
                    .abs_diff_eq(expected.view_projection(1.0), 1e-5)
            );
        }
        state.navigation_modifiers_changed(egui::Modifiers::NONE);
        state.navigation_modifiers_changed(egui::Modifiers::SHIFT);
        let before = state.camera.view_projection(1.0);
        state.trackpad_scroll(0.0, 0.0, egui::Modifiers::SHIFT, ScrollPhase::Started, None);
        assert_eq!(state.camera.view_projection(1.0), before);
        assert_eq!(state.is_planar_navigation(), planar);
        state.reset_navigation_input();
    }
}

#[test]
fn ruler_2d_selection_cache_tracks_geometry_selection_frame_and_up_convention() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.editor.set_tool(Tool::Move);
    let id = state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.refresh_ruler_2d_selection().unwrap();
    let initial = state.ruler_2d_selection.as_ref().unwrap().corners.clone();
    let stored = state.ruler_2d_selection.as_ref().unwrap().corners.as_ptr();
    state.camera.pan(30.0, 15.0, 600.0);
    state.camera.zoom(0.25);
    state.refresh_ruler_2d_selection().unwrap();
    assert_eq!(
        state.ruler_2d_selection.as_ref().unwrap().corners.as_ptr(),
        stored,
        "Camera navigation must reuse geometry extents"
    );
    state.editor.nudge(glam::DVec3::Y, false).unwrap();
    state.refresh_ruler_2d_selection().unwrap();
    let moved = state.ruler_2d_selection.as_ref().unwrap().corners.clone();
    for (before, after) in initial[0].iter().zip(&moved[0]) {
        assert!(after.abs_diff_eq(
            *before + glam::Vec3::Y * state.editor.frame.scale as f32,
            1e-6
        ));
    }
    state.z_up = true;
    state.refresh_ruler_2d_selection().unwrap();
    assert_ne!(state.ruler_2d_selection.as_ref().unwrap().corners, moved);
    state.editor.deselect();
    state.refresh_ruler_2d_selection().unwrap();
    assert!(
        state
            .ruler_2d_selection
            .as_ref()
            .unwrap()
            .corners
            .is_empty()
    );
    state.editor.select_object(id).unwrap();
    state.editor.convert_selected().unwrap();
    state.editor.enter_edit().unwrap();
    state.editor.selected_vertices.insert(1);
    state.refresh_ruler_2d_selection().unwrap();
    let selected = state.editor.selection_points(true).unwrap();
    assert_eq!(selected.len(), 1);
    assert!(
        state.ruler_2d_selection.as_ref().unwrap().corners[0]
            .iter()
            .all(|point| *point == selected[0])
    );
    state.editor.reframe().unwrap();
    state.refresh_ruler_2d_selection().unwrap();
    assert_eq!(
        state.ruler_2d_selection.as_ref().unwrap().frame,
        state.editor.frame
    );
    state.new_document();
    assert!(state.ruler_2d_selection.is_none() && state.ruler_2d_model.is_none());
}

#[test]
fn ruler_gutters_are_outside_viewport_interaction_and_do_not_edit_or_navigate() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.animate_views = false;
    state.set_view(View::Front);
    let run = |state: &mut WorkspaceUi, events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 700.0),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                state.ui(ui);
            },
        )
        .textures_delta
        .clear();
    };
    run(&mut state, vec![]);
    run(&mut state, vec![]);
    let model = state.ruler_2d_model.as_ref().unwrap();
    let points = [model.horizontal.rect.center(), model.vertical.rect.center()];
    let document = state.editor.document.clone();
    let selected = state.editor.selected_objects.clone();
    let camera = state.camera.view_projection(state.aspect());
    for point in points {
        assert!(!crate::navigation_events::viewport_accepts_pointer(
            &ctx,
            state.viewport_ui_rect,
            point
        ));
        for button in [
            egui::PointerButton::Primary,
            egui::PointerButton::Middle,
            egui::PointerButton::Secondary,
        ] {
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            run(
                &mut state,
                vec![egui::Event::PointerMoved(point), pointer(point, true)],
            );
            let end = state.viewport.center();
            run(&mut state, vec![egui::Event::PointerMoved(end)]);
            run(&mut state, vec![pointer(end, false)]);
            assert_eq!(state.editor.document, document);
            assert_eq!(state.editor.selected_objects, selected);
            assert_eq!(state.camera.view_projection(state.aspect()), camera);
            assert!(!state.editor.is_interacting() && !state.mouse_navigation_active());
            assert!(!egui::Popup::is_any_open(&ctx));
        }
    }
}

#[test]
fn view_pie_resize_and_document_replacement_cancel_pending_choice() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    state.animate_views = false;
    let run = |state: &mut WorkspaceUi, size, events| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                state.ui(ui);
            },
        )
        .textures_delta
        .clear();
    };
    let key = |pressed| egui::Event::Key {
        key: egui::Key::Backtick,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    let size = egui::vec2(1000., 700.);
    run(&mut state, size, Vec::new());
    run(&mut state, size, Vec::new());
    // The empty 3D viewport now has an Insert button at its center. Start the
    // pie gesture on free canvas so this test exercises viewport ownership.
    let anchor = state.viewport.center() + egui::vec2(160., 80.);
    run(
        &mut state,
        size,
        vec![egui::Event::PointerMoved(anchor), key(true)],
    );
    assert!(state.view_pie_active());
    let aspect = state.aspect();
    let pose = state.camera.view_projection(aspect);
    let smaller = egui::vec2(800., 600.);
    run(
        &mut state,
        smaller,
        vec![
            egui::Event::PointerMoved(anchor + egui::vec2(105., 0.)),
            key(false),
        ],
    );
    assert!(!state.view_pie_active());
    // Compare at the old aspect: a resize changes projection width only.
    assert_eq!(state.camera.view_projection(aspect), pose);
    run(&mut state, size, Vec::new());
    run(&mut state, size, Vec::new());
    run(
        &mut state,
        size,
        vec![egui::Event::PointerMoved(anchor), key(true)],
    );
    assert!(state.view_pie_active());
    state.new_document();
    let pose = state.camera.view_projection(state.aspect());
    run(
        &mut state,
        size,
        vec![
            egui::Event::PointerMoved(anchor + egui::vec2(105., 0.)),
            key(false),
        ],
    );
    assert!(!state.view_pie_active());
    assert_eq!(state.camera.view_projection(state.aspect()), pose);
}

#[test]
fn layers_and_viewport_share_feedback_with_selected_precedence() {
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let selected = state.editor.insert(PrimitiveKind::Cube).unwrap();
    let other = state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.editor.select_object(selected).unwrap();
    state.editor.hover_object(Some(other));
    assert_eq!(
        state.object_highlights(),
        ObjectHighlights {
            selected: [selected].into(),
            hovered: Some(other)
        }
    );
    assert_eq!(
        state.layer_row_color(selected),
        Some(object_feedback::SELECTED_COLOR)
    );
    assert_eq!(
        state.layer_row_color(other),
        Some(object_feedback::HOVERED_COLOR)
    );
    state.editor.hover_object(Some(selected));
    assert_eq!(
        state.layer_row_color(selected),
        Some(object_feedback::SELECTED_COLOR)
    );
    state.editor.convert_selected().unwrap();
    state.editor.enter_edit().unwrap();
    assert_eq!(state.object_highlights(), ObjectHighlights::default());
    assert_eq!(
        state.layer_row_color(selected),
        Some(object_feedback::SELECTED_COLOR),
        "The active Layers row remains selected during vertex editing"
    );
    assert_eq!(state.layer_row_color(other), None);
}

#[test]
fn layers_show_primitive_icons_and_a_generic_icon_after_mesh_conversion() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let shapes = [
        (PrimitiveKind::Cube, lucide::Icon::Box),
        (PrimitiveKind::Cylinder, lucide::Icon::Cylinder),
        (PrimitiveKind::Cone, lucide::Icon::Cone),
        (PrimitiveKind::Torus, lucide::Icon::Torus),
        (PrimitiveKind::Circle, lucide::Icon::ScanBox),
    ];
    let ids: Vec<_> = shapes
        .iter()
        .map(|(kind, _)| state.editor.insert(*kind).unwrap())
        .collect();
    layout_test_frame(&mut state, &ctx, Vec::new());
    for ((_, icon), id) in shapes.iter().zip(&ids) {
        assert_eq!(state.layer_row_icon(*id), Some(*icon));
    }

    state.editor.select_object(ids[0]).unwrap();
    state.editor.convert_selected().unwrap();
    layout_test_frame(&mut state, &ctx, Vec::new());
    assert_eq!(state.layer_row_icon(ids[0]), Some(lucide::Icon::ScanBox));
}

#[test]
fn double_clicking_layer_selects_its_name_for_immediate_replacement() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let id = state.editor.insert(PrimitiveKind::Cube).unwrap();
    layout_test_frame(&mut state, &ctx, Vec::new());
    let row = state.layer_row_rect(id).unwrap().center();
    let click = |pressed| egui::Event::PointerButton {
        pos: row,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    layout_test_frame(&mut state, &ctx, vec![egui::Event::PointerMoved(row)]);
    for _ in 0..2 {
        layout_test_frame(&mut state, &ctx, vec![click(true)]);
        layout_test_frame(&mut state, &ctx, vec![click(false)]);
    }
    layout_test_frame(&mut state, &ctx, Vec::new());

    let edit_id = egui::Id::new(("n3.layer.rename", id));
    assert!(ctx.memory(|memory| memory.has_focus(edit_id)));
    let selection = egui::TextEdit::load_state(&ctx, edit_id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_eq!(
        selection.as_sorted_char_range(),
        egui::text::CharIndex(0)..egui::text::CharIndex("Cube".chars().count())
    );

    layout_test_frame(&mut state, &ctx, vec![egui::Event::Text("Bracket".into())]);
    assert_eq!(state.layer_rename.as_ref().unwrap().value, "Bracket");
    assert_eq!(state.editor.document.objects[0].name, "Cube");
    layout_test_frame(
        &mut state,
        &ctx,
        vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert_eq!(state.editor.document.objects[0].name, "Bracket");
}

#[test]
fn viewport_selection_paints_the_layers_row_in_the_same_ui_pass() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let id = state.editor.insert(PrimitiveKind::Cube).unwrap();
    state.editor.deselect();
    let run = |state: &mut WorkspaceUi, events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1160., 800.),
                )),
                events,
                focused: true,
                ..Default::default()
            },
            |ui| {
                state.ui(ui);
            },
        );
        output.textures_delta.clear();
        output
    };
    let _ = run(&mut state, Vec::new());
    let _ = run(&mut state, Vec::new());
    let center = state.viewport.center();
    let button = |pressed| egui::Event::PointerButton {
        pos: center,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let _ = run(
        &mut state,
        vec![egui::Event::PointerMoved(center), button(true)],
    );
    let output = run(&mut state, vec![button(false)]);
    assert_eq!(state.editor.selected_object, Some(id));
    assert_eq!(state.editor.hovered_object, Some(id));
    let row = state.layer_row_rect(id).unwrap();
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.rect == row && rect.stroke.color == object_feedback::SELECTED_COLOR)), "The already laid-out row must paint the newly selected viewport object in this same frame");
}

#[test]
fn navigation_cancel_and_empty_selection_are_valid_command_contexts() {
    let ctx = test_context();
    let mut state = WorkspaceUi::new(egui::TextureId::User(0));
    let id = state.editor.insert(PrimitiveKind::Cube).unwrap();
    let original = state.editor.document.clone();
    assert_eq!(
        state.dispatch(Command::Escape, &ctx, true),
        HostEffect::CancelNavigation
    );
    assert_eq!(state.editor.selected_object, Some(id));
    state.dispatch(Command::Escape, &ctx, false);
    assert!(state.editor.selected_object.is_none());
    for command in [
        Command::Escape,
        Command::Confirm,
        Command::Tool(Tool::Rotate),
    ] {
        state.dispatch(command, &ctx, false);
        assert!(state.editor.selected_object.is_none());
        assert!(!state.editor.edit_mode);
        assert!(state.error.is_none());
        assert_eq!(state.editor.document, original);
    }
}
