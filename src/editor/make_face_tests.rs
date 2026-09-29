use super::*;
use crate::document::{Document, MeshVertex, Object, Primitive, PrimitiveKind, Transform};
use crate::editor::{BoxSelectionPolicy, Gesture, Tool};
use egui::Pos2;

fn mesh(points: &[[f64; 3]], polygons: &[&[u64]], edges: &[[u64; 2]]) -> EditableMesh {
    EditableMesh {
        vertices: points
            .iter()
            .enumerate()
            .map(|(index, position)| MeshVertex {
                id: index as u64 + 1,
                position: *position,
            })
            .collect(),
        faces: polygons
            .iter()
            .enumerate()
            .map(|(index, vertices)| Face {
                id: index as u64 + 1,
                vertices: vertices.to_vec(),
            })
            .collect(),
        edges: edges.to_vec(),
    }
}

fn editor_with(geometry: Geometry) -> Editor {
    let mut editor = Editor::new(Document {
        objects: vec![Object {
            id: 1,
            name: "Test".into(),
            transform: Transform::default(),
            geometry,
        }],
        ..Document::default()
    })
    .unwrap();
    editor.select_object(1).unwrap();
    editor.enter_edit().unwrap();
    editor.selected_vertices = editor
        .document
        .eval_object(1)
        .unwrap()
        .vertices
        .iter()
        .map(|vertex| vertex.id)
        .collect();
    editor
}

fn triangle() -> EditableMesh {
    mesh(&[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]], &[], &[])
}

fn square() -> EditableMesh {
    mesh(
        &[[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
        &[],
        &[[1, 2], [2, 3], [3, 4], [4, 1]],
    )
}

fn all(mesh: &EditableMesh) -> BTreeSet<u64> {
    mesh.vertices.iter().map(|vertex| vertex.id).collect()
}

fn assert_unchanged_after_rejection(editor: &mut Editor) -> String {
    let before = editor.snapshot();
    let revision = editor.revision;
    let history = (editor.history.undo_len(), editor.history.redo_len());
    let axis = editor.transform_axis;
    let gesture = editor.is_pointer_interacting();
    let transaction = editor.history.has_transaction();
    let error = editor.make_face().unwrap_err();
    assert_eq!(editor.snapshot(), before);
    assert_eq!(editor.revision, revision);
    assert_eq!(
        (editor.history.undo_len(), editor.history.redo_len()),
        history
    );
    assert_eq!(editor.transform_axis, axis);
    assert_eq!(editor.is_pointer_interacting(), gesture);
    assert_eq!(editor.history.has_transaction(), transaction);
    error
}

#[test]
fn free_triangle_is_one_atomic_edit_with_stable_vertices_and_selection() {
    let mut editor = editor_with(Geometry::Mesh(triangle()));
    let original = editor.document.clone();
    let vertices = editor.selected_vertices.clone();
    assert!(editor.can_make_face());
    assert!(editor.make_face().unwrap());
    let result = editor.document.eval_object(1).unwrap();
    assert_eq!(result.vertices, original.eval_object(1).unwrap().vertices);
    assert_eq!(
        result.faces,
        vec![Face {
            id: 1,
            vertices: vec![1, 2, 3]
        }]
    );
    assert_eq!(editor.selected_vertices, vertices);
    assert!(editor.edit_mode);
    assert_eq!(editor.history.undo_len(), 1);
    let filled = editor.document.clone();
    assert!(editor.undo());
    assert_eq!(editor.document, original);
    assert!(editor.redo());
    assert_eq!(editor.document, filled);
}

#[test]
fn concave_boundary_uses_authored_edges_and_consumes_only_its_loose_edges() {
    let source = mesh(
        &[
            [0., 0., 0.],
            [2., 0., 0.],
            [2., 1., 0.],
            [1., 1., 0.],
            [1., 2., 0.],
            [0., 2., 0.],
            [3., 0., 0.],
            [4., 0., 0.],
        ],
        &[],
        &[[1, 2], [2, 3], [3, 4], [4, 5], [5, 6], [6, 1], [7, 8]],
    );
    let mut editor = editor_with(Geometry::Mesh(source.clone()));
    editor.selected_vertices = (1..=6).collect();
    assert!(editor.make_face().unwrap());
    let result = editor.document.eval_object(1).unwrap();
    assert_eq!(result.faces[0].vertices, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(result.triangles().unwrap().len(), 4);
    assert_eq!(result.edges, vec![[7, 8]]);
    assert_eq!(result.vertices, source.vertices);
}

#[test]
fn circle_materialization_and_fill_undo_restore_the_exact_primitive() {
    let primitive = Primitive::new(PrimitiveKind::Circle);
    let mut editor = editor_with(Geometry::Primitive(primitive));
    let original = editor.document.clone();
    let selection = editor.selected_vertices.clone();
    assert!(editor.make_face().unwrap());
    assert!(matches!(
        editor.document.objects[0].geometry,
        Geometry::Mesh(_)
    ));
    let filled = editor.document.eval_object(1).unwrap();
    assert!(filled.edges.is_empty());
    assert_eq!(filled.faces.len(), 1);
    assert_eq!(filled.faces[0].vertices.len(), selection.len());
    assert_eq!(editor.history.undo_len(), 1);
    assert!(editor.undo());
    assert_eq!(editor.document, original);
    assert!(editor.edit_mode);
    assert_eq!(editor.selected_vertices, selection);
    assert!(!editor.undo());
    assert!(editor.redo());
}

#[test]
fn existing_primitive_face_is_a_noop_without_materialization_or_history() {
    let mut editor = editor_with(Geometry::Primitive(Primitive::new(PrimitiveKind::Cube)));
    editor.selected_vertices = editor.document.eval_object(1).unwrap().faces[0]
        .vertices
        .iter()
        .copied()
        .collect();
    let original = editor.snapshot();
    let revision = editor.revision;
    assert!(!editor.make_face().unwrap());
    assert_eq!(editor.snapshot(), original);
    assert_eq!(editor.revision, revision);
    assert_eq!(editor.history.undo_len(), 0);
}

#[test]
fn duplicate_noop_retains_redo_and_rejecting_invalid_geometry_does_too() {
    let mut editor = editor_with(Geometry::Mesh(triangle()));
    editor.set_tool(Tool::Move);
    assert!(editor.make_face().unwrap());
    assert!(editor.nudge(DVec3::Z, false).unwrap());
    let translated = editor.document.clone();
    assert!(editor.undo());
    let revision = editor.revision;
    assert!(!editor.make_face().unwrap());
    assert_eq!(editor.revision, revision);
    assert!(editor.redo());
    assert_eq!(editor.document, translated);

    let mut editor = editor_with(Geometry::Mesh(mesh(
        &[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 0.]],
        &[],
        &[],
    )));
    editor.selected_vertices = BTreeSet::from([1, 2, 3]);
    assert!(editor.make_face().unwrap());
    assert!(editor.undo());
    editor.selected_vertices.insert(4);
    assert_unchanged_after_rejection(&mut editor);
    assert!(editor.redo());
    assert_eq!(editor.document.eval_object(1).unwrap().faces.len(), 1);
}

#[test]
fn missing_cube_face_follows_neighbor_winding_even_for_negative_normal() {
    let mut source = Primitive::new(PrimitiveKind::Cube).evaluate().unwrap();
    let removed = source.faces.remove(0);
    let expected_normal = planar_polygon_normal(
        &removed
            .vertices
            .iter()
            .map(|id| {
                DVec3::from_array(
                    source
                        .vertices
                        .iter()
                        .find(|vertex| vertex.id == *id)
                        .unwrap()
                        .position,
                )
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut editor = editor_with(Geometry::Mesh(source.clone()));
    editor.selected_vertices = removed.vertices.iter().copied().collect();
    assert!(editor.make_face().unwrap());
    let result = editor.document.eval_object(1).unwrap();
    let new = result.faces.last().unwrap();
    let normal = planar_polygon_normal(
        &new.vertices
            .iter()
            .map(|id| {
                DVec3::from_array(
                    result
                        .vertices
                        .iter()
                        .find(|vertex| vertex.id == *id)
                        .unwrap()
                        .position,
                )
            })
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(normal.dot(expected_normal) > 0.999);
    assert_eq!(&result.faces[..source.faces.len()], source.faces.as_slice());
    assert!(new.id > source.faces.iter().map(|face| face.id).max().unwrap());
}

#[test]
fn rejects_scattered_open_branched_disconnected_and_nested_boundaries() {
    let mut cases = Vec::new();
    let mut scattered = square();
    scattered.edges.clear();
    cases.push(scattered);
    let mut open = square();
    open.edges.pop();
    cases.push(open);
    let mut branched = square();
    branched.edges.push([1, 3]);
    cases.push(branched);
    for offset in [3., 0.25] {
        let mut loops = square();
        for position in [
            [offset, offset, 0.],
            [offset + 0.25, offset, 0.],
            [offset + 0.25, offset + 0.25, 0.],
            [offset, offset + 0.25, 0.],
        ] {
            loops.vertices.push(MeshVertex {
                id: loops.vertices.len() as u64 + 1,
                position,
            });
        }
        loops.edges.extend([[5, 6], [6, 7], [7, 8], [8, 5]]);
        cases.push(loops);
    }
    for source in cases {
        let mut editor = editor_with(Geometry::Mesh(source));
        assert_unchanged_after_rejection(&mut editor);
    }
}

#[test]
fn rejects_degenerate_self_crossing_and_nonplanar_candidates_without_mutation() {
    let collinear = mesh(&[[0., 0., 0.], [1., 0., 0.], [2., 0., 0.]], &[], &[]);
    let coincident = mesh(&[[0., 0., 0.], [1., 0., 0.], [0., 0., 0.]], &[], &[]);
    let mut crossing = square();
    crossing.vertices[2].position = [0., 2., 0.];
    crossing.vertices[3].position = [2., 2., 0.];
    let mut warped = square();
    warped.vertices[3].position[2] = 0.25;
    for source in [collinear, coincident, crossing, warped] {
        let mut editor = editor_with(Geometry::Mesh(source));
        assert_unchanged_after_rejection(&mut editor);
    }
}

#[test]
fn uses_existing_scale_aware_polygon_validation_at_tiny_and_huge_scales() {
    for scale in [1e-200, 1., 1e200] {
        let mut source = square();
        for vertex in &mut source.vertices {
            for coordinate in &mut vertex.position {
                *coordinate *= scale;
            }
        }
        assert!(new_face(&source, &all(&source)).unwrap().is_some());
        source.vertices[3].position[2] = scale * 0.1;
        assert!(
            new_face(&source, &all(&source))
                .unwrap_err()
                .contains("planar")
        );
    }
}

#[test]
fn rejects_subset_and_superset_of_existing_faces_instead_of_overlapping_them() {
    let mut quad = square();
    quad.edges.clear();
    quad.faces.push(Face {
        id: 1,
        vertices: vec![1, 2, 3, 4],
    });
    let mut editor = editor_with(Geometry::Mesh(quad));
    editor.selected_vertices.remove(&4);
    assert!(assert_unchanged_after_rejection(&mut editor).contains("overlap"));

    let source = mesh(
        &[[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
        &[&[1, 2, 3], &[1, 3, 4]],
        &[],
    );
    let mut editor = editor_with(Geometry::Mesh(source));
    assert!(assert_unchanged_after_rejection(&mut editor).contains("overlap"));
}

#[test]
fn rejects_third_edge_incidence_and_conflicting_neighbor_winding() {
    let points = [
        [0., 0., 0.],
        [1., 0., 0.],
        [0., 1., 0.],
        [0., -1., 0.],
        [1., 1., 0.],
    ];
    for (polygons, error) in [
        (vec![vec![1, 2, 4], vec![2, 1, 5]], "third face"),
        (vec![vec![1, 2, 4], vec![3, 2, 5]], "conflicting winding"),
    ] {
        let source = mesh(
            &points,
            &polygons.iter().map(Vec::as_slice).collect::<Vec<_>>(),
            &[],
        );
        let mut editor = editor_with(Geometry::Mesh(source));
        editor.selected_vertices = BTreeSet::from([1, 2, 3]);
        assert!(assert_unchanged_after_rejection(&mut editor).contains(error));
    }
}

#[test]
fn isolated_winding_is_positive_dominant_object_local_and_not_selection_order() {
    for positions in [
        [[0., 0., 0.], [0., 0., 1.], [0., 1., 0.]],
        [[0., 0., 0.], [1., 0., 0.], [0., 0., 1.]],
        [[0., 0., 0.], [0., 1., 0.], [1., 0., 0.]],
    ] {
        let source = mesh(&positions, &[], &[]);
        let face = new_face(&source, &BTreeSet::from([3, 2, 1]))
            .unwrap()
            .unwrap();
        let normal = planar_polygon_normal(
            &face
                .vertices
                .iter()
                .map(|id| DVec3::from_array(positions[*id as usize - 1]))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(normal.max_element(), 1.);
        assert_eq!(face.vertices[0], 1);
    }
}

#[test]
fn face_id_overflow_is_rejected_atomically() {
    let mut source = mesh(
        &[
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [3., 0., 0.],
            [4., 0., 0.],
            [3., 1., 0.],
        ],
        &[&[4, 5, 6]],
        &[],
    );
    source.faces[0].id = u64::MAX;
    let mut editor = editor_with(Geometry::Mesh(source));
    editor.selected_vertices = BTreeSet::from([1, 2, 3]);
    assert!(assert_unchanged_after_rejection(&mut editor).contains("ID space"));
}

#[test]
fn wrong_mode_too_few_stale_and_unselected_object_states_reject() {
    let mut editor = editor_with(Geometry::Mesh(triangle()));
    editor.leave_edit();
    assert!(!editor.can_make_face());
    assert_unchanged_after_rejection(&mut editor);
    editor.enter_edit().unwrap();
    editor.selected_vertices = BTreeSet::from([1, 2]);
    assert!(!editor.can_make_face());
    assert_unchanged_after_rejection(&mut editor);
    editor.selected_vertices.insert(99);
    assert!(assert_unchanged_after_rejection(&mut editor).contains("no longer exists"));
    editor.deselect();
    assert_unchanged_after_rejection(&mut editor);
}

#[test]
fn armed_axis_pending_numeric_property_and_marquee_keep_their_ownership() {
    let mut editor = editor_with(Geometry::Mesh(triangle()));
    editor.set_tool(Tool::Move);
    editor.toggle_transform_axis(0).unwrap();
    assert!(!editor.can_make_face());
    assert_unchanged_after_rejection(&mut editor);
    editor.numeric_input('2').unwrap();
    assert_unchanged_after_rejection(&mut editor);
    assert!(editor.numeric.is_some());
    editor.cancel();
    editor.toggle_transform_axis(0).unwrap();
    assert!(editor.nudge(DVec3::X, false).unwrap());
    editor.toggle_transform_axis(0).unwrap();
    assert!(editor.transform_axis.is_none());
    assert!(editor.has_transform_session());
    assert_unchanged_after_rejection(&mut editor);
    editor.cancel();
    assert!(editor.begin_property_edit());
    assert_unchanged_after_rejection(&mut editor);
    assert!(editor.has_property_edit());
    editor.cancel();
    editor.gesture = Some(Gesture::Marquee {
        start: Pos2::ZERO,
        current: Pos2::new(10., 10.),
        additive: false,
        before: editor.selected_vertices.clone(),
        pending: BTreeSet::new(),
        policy: BoxSelectionPolicy::default(),
        dragged: true,
    });
    assert_unchanged_after_rejection(&mut editor);
}

#[test]
fn triangle_consumes_its_loose_edges_and_leaves_unselected_edge_untouched() {
    let source = mesh(
        &[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 2., 0.]],
        &[],
        &[[1, 2], [2, 3], [3, 4]],
    );
    let mut editor = editor_with(Geometry::Mesh(source));
    editor.selected_vertices = BTreeSet::from([1, 2, 3]);
    assert!(editor.make_face().unwrap());
    assert_eq!(editor.document.eval_object(1).unwrap().edges, vec![[3, 4]]);
}

#[test]
fn local_checks_do_not_reject_unrelated_preexisting_nonmanifold_edges() {
    let source = mesh(
        &[
            [0., 0., 0.],
            [1., 0., 0.],
            [0., 1., 0.],
            [3., 0., 0.],
            [4., 0., 0.],
            [3., 1., 0.],
            [3., -1., 0.],
            [3., 0., 1.],
        ],
        &[&[4, 5, 6], &[5, 4, 7], &[4, 5, 8]],
        &[],
    );
    let mut editor = editor_with(Geometry::Mesh(source.clone()));
    editor.selected_vertices = BTreeSet::from([1, 2, 3]);
    assert!(editor.make_face().unwrap());
    let result = editor.document.eval_object(1).unwrap();
    assert_eq!(&result.faces[..3], source.faces.as_slice());
    assert_eq!(result.faces[3].id, 4);
}

#[test]
fn too_many_selected_vertices_fail_the_polygon_budget_before_boundary_work() {
    let source = EditableMesh {
        vertices: (1..=MAX_FACE_CORNERS as u64 + 1)
            .map(|id| MeshVertex {
                id,
                position: [id as f64, 0., 0.],
            })
            .collect(),
        faces: Vec::new(),
        edges: Vec::new(),
    };
    let mut editor = editor_with(Geometry::Mesh(source));
    assert!(assert_unchanged_after_rejection(&mut editor).contains("at most 4096"));
}
