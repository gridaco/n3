use super::*;

fn source_mesh(document: &Document) -> &EditableMesh {
    match &document.objects[0].geometry {
        Geometry::Mesh(mesh) => mesh,
        _ => panic!("Expected mesh"),
    }
}
fn cube() -> Document {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    document.convert_object(id).unwrap();
    document
}

#[test]
fn object_draw_ranges_cover_derived_vertices_and_keep_source_ids() {
    let mut document = Document::default();
    let first = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let second = document.insert_primitive(PrimitiveKind::Cylinder).unwrap();
    // IDs are identities, not offsets into the object list or vertex buffer.
    document.objects[0].id = first + 100;
    document.objects[1].id = second + 200;
    document.objects.push(Object {
        id: 900,
        name: "Empty mesh".into(),
        transform: Transform::default(),
        geometry: Geometry::Mesh(EditableMesh {
            vertices: Vec::new(),
            faces: Vec::new(),
            edges: Vec::new(),
        }),
    });
    let original = document.clone();
    let frame = DisplayFrame::from_document(&document).unwrap();
    let mesh = document.render_mesh(&frame).unwrap();
    let mut next = 0;
    for (object, range) in document.objects.iter().zip(&mesh.object_ranges) {
        assert_eq!(range.object, object.id);
        assert_eq!(range.triangles.start, next);
        let vertices = document
            .eval_object(object.id)
            .unwrap()
            .triangles()
            .unwrap()
            .len()
            * 3;
        assert_eq!(range.triangles.len(), vertices);
        assert_eq!(range.triangles.start % 3, 0);
        assert_eq!(range.triangles.end % 3, 0);
        next = range.triangles.end;
    }
    assert_eq!(mesh.object_ranges.len(), 3);
    assert!(mesh.object_ranges[2].triangles.is_empty());
    assert_eq!(next as usize, mesh.vertices.len());
    assert_eq!(document, original);
    document.convert_object(first + 100).unwrap();
    assert_eq!(
        document.render_mesh(&frame).unwrap().object_ranges,
        mesh.object_ranges
    );
    document.objects.remove(0);
    let remaining = document.render_mesh(&frame).unwrap();
    assert_eq!(remaining.object_ranges[0].object, second + 200);
    assert_eq!(remaining.object_ranges[0].triangles.start, 0);
}

#[test]
fn edit_topology_preserves_standalone_vertices_and_empty_objects() {
    let document = Document {
        objects: vec![
            Object {
                id: 10,
                name: "Loose points".into(),
                transform: Transform::default(),
                geometry: Geometry::Mesh(EditableMesh {
                    vertices: vec![
                        MeshVertex {
                            id: 200,
                            position: [1.0, 2.0, 3.0],
                        },
                        MeshVertex {
                            id: 900,
                            position: [4.0, 6.0, 8.0],
                        },
                    ],
                    faces: vec![],
                    edges: vec![],
                }),
            },
            Object {
                id: 30,
                name: "Empty".into(),
                transform: Transform::default(),
                geometry: Geometry::Mesh(EditableMesh {
                    vertices: vec![],
                    faces: vec![],
                    edges: vec![],
                }),
            },
        ],
        ..Default::default()
    };
    let frame = DisplayFrame::from_document(&document).unwrap();
    let rendered = document.render_mesh(&frame).unwrap();
    assert!(rendered.vertices.is_empty());
    assert!(rendered.edges.is_empty());
    assert_eq!(rendered.edit_topology.len(), 2);
    assert_eq!(
        rendered.edit_topology[0]
            .vertices
            .iter()
            .map(|vertex| vertex.id)
            .collect::<Vec<_>>(),
        vec![200, 900]
    );
    for topology in &rendered.edit_topology {
        assert_eq!(topology.edges, 0..0);
        assert!(topology.edge_vertices.is_empty());
        assert!(topology.faces.is_empty());
    }
    assert!(rendered.edit_topology[1].vertices.is_empty());
    for vertex in &rendered.edit_topology[0].vertices {
        assert!(
            vertex
                .position
                .iter()
                .all(|coordinate| coordinate.is_finite())
        );
    }
}

#[test]
fn all_primitives_are_closed_outward_surfaces_and_convert_without_moving() {
    for kind in [
        PrimitiveKind::Cube,
        PrimitiveKind::Cylinder,
        PrimitiveKind::Cone,
        PrimitiveKind::Torus,
        PrimitiveKind::Sphere,
        PrimitiveKind::Polyhedron,
    ] {
        let mut document = Document::default();
        let id = document.insert_primitive(kind).unwrap();
        document.objects[0].transform.translation = [2., 3., 4.];
        document.objects[0].transform.scale = [2., 1., 0.5];
        let transform = document.objects[0].transform.clone();
        let generated = document.eval_object(id).unwrap();
        let positions = generated.vertex_map().unwrap();
        let mut edge_uses = BTreeMap::<(u64, u64), (usize, isize)>::new();
        for f in &generated.faces {
            for i in 0..f.vertices.len() {
                let a = f.vertices[i];
                let b = f.vertices[(i + 1) % f.vertices.len()];
                let entry = edge_uses.entry((a.min(b), a.max(b))).or_default();
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(
            edge_uses.values().all(|v| *v == (2, 0)),
            "{kind:?} must be closed with consistent winding"
        );
        let volume: f64 = generated
            .triangles()
            .unwrap()
            .iter()
            .map(|t| positions[&t[0]].dot(positions[&t[1]].cross(positions[&t[2]])) / 6.0)
            .sum();
        assert!(volume > 0., "{kind:?} must face outward: {volume}");
        document.convert_object(id).unwrap();
        assert_eq!(document.objects[0].id, id);
        assert_eq!(document.objects[0].transform, transform);
        assert_eq!(source_mesh(&document), &generated);
    }
}

#[test]
fn plane_is_one_open_quad_with_live_dimensions() {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Plane).unwrap();
    let mesh = document.eval_object(id).unwrap();
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.faces.len(), 1);
    assert_eq!(mesh.faces[0].vertices.len(), 4);
    assert!(mesh.vertices.iter().all(|vertex| vertex.position[2] == 0.0));
    if let Geometry::Primitive(plane) = &mut document.objects[0].geometry {
        plane.size[0] = 5.0;
        plane.size[1] = 3.0;
    }
    let resized = document.eval_object(id).unwrap();
    assert_eq!(resized.vertices[0].position, [-2.5, -1.5, 0.0]);
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[test]
fn circle_defaults_to_a_closed_unfilled_loop_with_live_radius_and_vertex_count() {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Circle).unwrap();
    let Geometry::Primitive(primitive) = &document.objects[0].geometry else {
        panic!("Expected a parametric circle");
    };
    assert!(!primitive.fill);
    assert_eq!(primitive.segments, 32);
    for segments in [3, 5, 32, 256] {
        let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
            unreachable!();
        };
        primitive.segments = segments;
        primitive.size = [6.0, 6.0, 1.0];
        let mesh = document.eval_object(id).unwrap();
        mesh.validate().unwrap();
        assert_eq!(mesh.vertices.len(), segments as usize);
        assert_eq!(mesh.edges.len(), segments as usize);
        assert!(mesh.faces.is_empty());
        assert!(mesh.triangles().unwrap().is_empty());
        let mut degrees = BTreeMap::<u64, usize>::new();
        for [a, b] in &mesh.edges {
            *degrees.entry(*a).or_default() += 1;
            *degrees.entry(*b).or_default() += 1;
        }
        assert_eq!(degrees.len(), segments as usize);
        assert!(degrees.values().all(|degree| *degree == 2));
        for vertex in &mesh.vertices {
            let position = DVec3::from_array(vertex.position);
            assert_eq!(position.z, 0.0);
            assert!((position.length() - 3.0).abs() < 1e-12);
        }
    }
    let before = document.clone();
    let mesh = document.eval_object(id).unwrap();
    document.convert_object(id).unwrap();
    assert_eq!(source_mesh(&document), &mesh);
    assert_eq!(
        Document::from_json(&before.to_json().unwrap()).unwrap(),
        before
    );
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
}

#[test]
fn filling_circle_keeps_vertex_ids_and_creates_one_outward_polygon() {
    let mut circle = Primitive::new(PrimitiveKind::Circle);
    circle.segments = 7;
    let unfilled = circle.evaluate().unwrap();
    circle.fill = true;
    let filled = circle.evaluate().unwrap();
    assert_eq!(filled.vertices, unfilled.vertices);
    assert!(filled.edges.is_empty());
    assert_eq!(filled.faces.len(), 1);
    assert_eq!(filled.faces[0].vertices, (1..=7).collect::<Vec<_>>());
    let positions = filled.vertex_map().unwrap();
    let triangles = filled.triangles().unwrap();
    assert_eq!(triangles.len(), 5);
    for [a, b, c] in triangles {
        assert!(
            (positions[&b] - positions[&a])
                .cross(positions[&c] - positions[&a])
                .z
                > 0.0
        );
    }
    let text = serde_json::to_string(&circle).unwrap();
    assert!(text.contains("\"fill\":true"));
    assert_eq!(serde_json::from_str::<Primitive>(&text).unwrap(), circle);
    circle.size[1] *= 2.0;
    assert!(
        circle.evaluate().is_err(),
        "Circle radius must remain uniform"
    );
}

#[test]
fn legacy_meshes_and_primitives_omit_new_optional_edge_and_fill_fields() {
    let document = cube();
    let text = document.to_json().unwrap();
    assert!(!text.contains("\"edges\""));
    let restored = Document::from_json(&text).unwrap();
    assert!(source_mesh(&restored).edges.is_empty());
    assert_eq!(restored, document);
    let mut primitive_document = Document::default();
    primitive_document
        .insert_primitive(PrimitiveKind::Cube)
        .unwrap();
    let text = primitive_document.to_json().unwrap();
    assert!(!text.contains("\"fill\""));
    assert_eq!(Document::from_json(&text).unwrap(), primitive_document);
}

#[test]
fn loose_edges_keep_transformed_endpoints_and_separate_draw_ranges() {
    let mut document = cube();
    let id = document.insert_primitive(PrimitiveKind::Circle).unwrap();
    document.objects[1].transform = Transform {
        translation: [4.0, -2.0, 3.0],
        rotation: DQuat::from_rotation_x(0.7).to_array(),
        scale: [2.0, 0.5, 3.0],
    };
    let source = document.eval_object(id).unwrap();
    let frame = DisplayFrame::from_document(&document).unwrap();
    let rendered = document.render_mesh(&frame).unwrap();
    let topology = &rendered.edit_topology[1];
    assert!(rendered.edit_topology[0].loose_edges.is_empty());
    assert_eq!(topology.edges, topology.loose_edges);
    assert_eq!(
        topology.loose_edges.start,
        rendered.edit_topology[0].edges.end
    );
    assert_eq!(topology.loose_edges.len(), source.edges.len() * 2);
    assert_eq!(topology.edge_vertices, source.edges);
    assert!(rendered.object_ranges[1].triangles.is_empty());
    let matrix = document.objects[1].transform.matrix();
    let positions = source.vertex_map().unwrap();
    for (endpoints, vertices) in source.edges.iter().zip(
        rendered.edges[topology.loose_edges.start as usize..topology.loose_edges.end as usize]
            .as_chunks::<2>()
            .0
            .iter(),
    ) {
        for (id, vertex) in endpoints.iter().zip(vertices) {
            let expected = frame
                .world_to_display(matrix.transform_point3(positions[id]))
                .as_vec3();
            assert_eq!(vertex.position, expected.to_array());
        }
    }
}

#[test]
fn invalid_explicit_edges_are_rejected_before_commit() {
    let mut circle = Primitive::new(PrimitiveKind::Circle);
    circle.segments = 3;
    let baseline = circle.evaluate().unwrap();
    for bad_edges in [vec![[1, 100]], vec![[1, 1]], vec![[1, 2], [2, 1]]] {
        let mut mesh = baseline.clone();
        mesh.edges = bad_edges;
        assert!(mesh.validate().is_err());
    }
    let mut coincident = baseline.clone();
    coincident.vertices[1].position = coincident.vertices[0].position;
    assert!(
        coincident
            .validate()
            .unwrap_err()
            .contains("nonzero length")
    );
    circle.fill = true;
    let mut duplicate_boundary = circle.evaluate().unwrap();
    duplicate_boundary.edges.push([1, 2]);
    assert!(duplicate_boundary.validate().is_err());
    assert!(check_edge_budget(MAX_CORNERS / 2 + 1, &mut 0).is_err());
    assert!(check_edge_budget(usize::MAX, &mut 0).is_err());

    let mut document = Document::default();
    document.insert_primitive(PrimitiveKind::Circle).unwrap();
    let before = document.clone();
    assert!(
        document
            .transact(|candidate| {
                candidate.objects[0].transform.translation = [1e100; 3];
                Ok(())
            })
            .unwrap_err()
            .contains("transformed edge")
    );
    assert_eq!(document, before);
}

#[test]
fn one_polyhedron_recipe_switches_between_all_five_regular_solids() {
    let mut document = Document::default();
    let id = document
        .insert_primitive(PrimitiveKind::Polyhedron)
        .unwrap();
    for (kind, vertices, faces, corners) in [
        (PolyhedronType::Tetrahedron, 4, 4, 3),
        (PolyhedronType::Cube, 8, 6, 4),
        (PolyhedronType::Octahedron, 6, 8, 3),
        (PolyhedronType::Icosahedron, 12, 20, 3),
        (PolyhedronType::Dodecahedron, 20, 12, 5),
    ] {
        let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
            unreachable!();
        };
        primitive.polyhedron_type = Some(kind);
        primitive.size = [4.0; 3];
        document.validate().unwrap();
        assert_eq!(document.objects[0].name, "Polyhedron");
        assert!(matches!(
            document.objects[0].geometry,
            Geometry::Primitive(_)
        ));
        let mesh = document.eval_object(id).unwrap();
        assert_eq!(mesh.vertices.len(), vertices);
        assert_eq!(mesh.faces.len(), faces);
        assert!(mesh.faces.iter().all(|face| face.vertices.len() == corners));
        let positions = mesh.vertex_map().unwrap();
        let (minimum, maximum) = bounds(positions.values().copied()).unwrap().unwrap();
        assert!((minimum + DVec3::splat(2.0)).length() < 1e-10);
        assert!((maximum - DVec3::splat(2.0)).length() < 1e-10);
        let radius = positions.values().next().unwrap().length();
        assert!(
            positions
                .values()
                .all(|point| (point.length() - radius).abs() < 1e-10)
        );
        let mut edges = BTreeMap::<_, (usize, i32)>::new();
        for face in &mesh.faces {
            let origin = positions[&face.vertices[0]];
            let normal = (positions[&face.vertices[1]] - origin)
                .cross(positions[&face.vertices[2]] - origin);
            assert!(normal.dot(origin) > 0.0, "{kind:?} must face outward");
            assert!(
                positions
                    .values()
                    .all(|point| normal.dot(*point - origin) < 1e-10),
                "{kind:?} must be convex"
            );
            for index in 0..face.vertices.len() {
                let a = face.vertices[index];
                let b = face.vertices[(index + 1) % face.vertices.len()];
                let edge = edges.entry((a.min(b), a.max(b))).or_default();
                edge.0 += 1;
                edge.1 += if a < b { 1 } else { -1 };
            }
        }
        assert_eq!(edges.len(), vertices + faces - 2);
        assert!(
            edges.values().all(|incidence| *incidence == (2, 0)),
            "{kind:?} must be closed with consistently wound adjacent faces"
        );
        let edge_lengths: Vec<_> = edges
            .keys()
            .map(|(a, b)| positions[a].distance(positions[b]))
            .collect();
        assert!(
            edge_lengths
                .iter()
                .all(|length| (length - edge_lengths[0]).abs() < 1e-10),
            "{kind:?} must have equal edges at uniform size"
        );
        let volume: f64 = mesh
            .triangles()
            .unwrap()
            .iter()
            .map(|triangle| {
                positions[&triangle[0]].dot(positions[&triangle[1]].cross(positions[&triangle[2]]))
                    / 6.0
            })
            .sum();
        assert!(volume > 0.0, "{kind:?} must face outward");
        assert_eq!(
            Document::from_json(&document.to_json().unwrap()).unwrap(),
            document
        );
        if kind == PolyhedronType::Cube {
            let mut cube = Primitive::new(PrimitiveKind::Cube);
            cube.size = [4.0; 3];
            assert_eq!(mesh, cube.evaluate().unwrap());
        }
        let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
            unreachable!();
        };
        primitive.size = [1.0; 3];
        let smaller = document.eval_object(id).unwrap();
        assert_eq!(smaller.faces, mesh.faces);
        for (small, original) in smaller.vertices.iter().zip(&mesh.vertices) {
            assert_eq!(small.id, original.id);
            assert_eq!(
                DVec3::from_array(small.position) * 4.0,
                DVec3::from_array(original.position)
            );
        }
    }
    let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
        unreachable!();
    };
    primitive.polyhedron_type = None;
    assert!(document.validate().is_err());
    let Geometry::Primitive(primitive) = &mut document.objects[0].geometry else {
        unreachable!();
    };
    primitive.polyhedron_type = Some(PolyhedronType::Icosahedron);
    primitive.size = [4.0, 5.0, 4.0];
    assert!(document.validate().is_err());
}

#[test]
fn sphere_tessellation_is_one_recipe_not_a_second_sphere_kind() {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Sphere).unwrap();
    let Geometry::Primitive(sphere) = &mut document.objects[0].geometry else {
        unreachable!();
    };
    sphere.segments = 8;
    sphere.minor_segments = 4;
    let mesh = document.eval_object(id).unwrap();
    assert_eq!(mesh.vertices.len(), 26);
    assert_eq!(mesh.faces.len(), 32);
    assert_eq!(
        mesh.faces
            .iter()
            .filter(|face| face.vertices.len() == 3)
            .count(),
        16
    );
}

#[test]
fn primitive_parameters_remain_live_until_conversion_and_roundtrip() {
    let mut document = Document::default();
    let id = document.insert_primitive(PrimitiveKind::Cylinder).unwrap();
    if let Geometry::Primitive(p) = &mut document.objects[0].geometry {
        p.segments = 12;
        p.size = [4., 6., 8.];
    }
    assert_eq!(document.eval_object(id).unwrap().vertices.len(), 24);
    let saved = document.to_json().unwrap();
    let restored = Document::from_json(&saved).unwrap();
    assert_eq!(document, restored);
    assert_eq!(saved, restored.to_json().unwrap());
    assert!(matches!(
        restored.objects[0].geometry,
        Geometry::Primitive(_)
    ));
    document.convert_object(id).unwrap();
    assert!(matches!(document.objects[0].geometry, Geometry::Mesh(_)));
    assert_eq!(document.eval_object(id).unwrap().vertices.len(), 24);
}

#[test]
fn rejected_geometry_change_is_atomic() {
    let mut document = cube();
    let before = document.clone();
    let error = document
        .transact(|candidate| {
            let Geometry::Mesh(mesh) = &mut candidate.objects[0].geometry else {
                unreachable!()
            };
            mesh.vertices[0].position = mesh.vertices[1].position;
            Ok(())
        })
        .unwrap_err();
    assert!(!error.is_empty());
    assert_eq!(document, before);
    assert!(
        document
            .transact(|candidate| {
                candidate.objects[0].transform.scale[0] = 0.;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(document, before);
}

#[test]
fn fixed_display_frame_does_not_recentere_after_transform_or_vertex_edit() {
    let mut document = cube();
    let frame = DisplayFrame::from_document(&document).unwrap();
    let before = document.render_mesh(&frame).unwrap();
    document.objects[0].transform.translation[0] = 3.;
    let after = document.render_mesh(&frame).unwrap();
    assert!((after.vertices[0].position[0] - before.vertices[0].position[0] - 3.).abs() < 1e-6);
    assert_eq!(source_mesh(&document).vertices[0].position, [-1., -1., -1.]);
    assert_eq!(frame.center, [0.; 3]);
}

#[test]
fn empty_documents_are_valid_roundtrip_and_render_empty() {
    let document = Document::default();
    let restored = Document::from_json(&document.to_json().unwrap()).unwrap();
    assert_eq!(document, restored);
    let frame = DisplayFrame::from_document(&document).unwrap();
    assert_eq!(frame, DisplayFrame::default());
    let mesh = document.render_mesh(&frame).unwrap();
    assert!(mesh.vertices.is_empty());
    assert!(mesh.edges.is_empty());
}

#[test]
fn new_documents_save_explicit_centimeters_without_changing_default_sizes() {
    let mut document = Document::default();
    assert_eq!(document.version, 1);
    assert_eq!(document.length_unit, CanonicalLengthUnit::Centimeters);
    let id = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let Geometry::Primitive(primitive) = &document.objects[0].geometry else {
        unreachable!()
    };
    assert_eq!(
        primitive.size, [2.0; 3],
        "The default cube is two centimeters across"
    );
    let evaluated = document.eval_object(id).unwrap();
    assert!(evaluated.vertices.iter().all(|vertex| {
        vertex
            .position
            .iter()
            .all(|coordinate| coordinate.abs() == 1.0)
    }));
    let saved: serde_json::Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["length_unit"], "cm");
}

#[test]
fn unsupported_canonical_units_are_rejected_instead_of_reinterpreted() {
    for invalid in [
        serde_json::json!("m"),
        serde_json::json!("mm"),
        serde_json::json!("in"),
        serde_json::json!("ft"),
        serde_json::json!("CM"),
        serde_json::Value::Null,
        serde_json::json!(1),
    ] {
        let text = serde_json::json!({
            "version": 1,
            "length_unit": invalid,
            "objects": [],
        })
        .to_string();
        assert!(Document::from_json(&text).is_err(), "{text}");
    }
}

#[test]
fn fractional_centimeter_geometry_and_transforms_roundtrip_exactly() {
    let mut document = Document::default();
    let live = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    let editable = document.insert_primitive(PrimitiveKind::Cube).unwrap();
    for object in &mut document.objects {
        let Geometry::Primitive(primitive) = &mut object.geometry else {
            unreachable!()
        };
        primitive.size = [0.1, 1.0 / 3.0, 2.54];
        object.transform.translation = [0.125, -0.1, 30.48];
    }
    document.convert_object(editable).unwrap();
    let before_live = document.eval_object(live).unwrap();
    let before_editable = document.eval_object(editable).unwrap();
    let saved = document.to_json().unwrap();
    let restored = Document::from_json(&saved).unwrap();
    assert_eq!(restored, document);
    assert_eq!(restored.eval_object(live).unwrap(), before_live);
    assert_eq!(restored.eval_object(editable).unwrap(), before_editable);
    assert_eq!(restored.to_json().unwrap(), saved);
    assert!(
        before_editable
            .vertices
            .iter()
            .any(|vertex| vertex.position[0] == 0.05)
    );
}

#[test]
fn invalid_versions_ids_indices_and_nonfinite_inputs_are_rejected() {
    assert!(
        Document::from_json("{\"version\":2,\"objects\":[]}")
            .unwrap_err()
            .contains("version")
    );
    assert!(Document::from_json("{\"version\":1,\"objects\":[],\"typo\":1}").is_err());
    let mut document = cube();
    let Geometry::Mesh(mesh) = &mut document.objects[0].geometry else {
        unreachable!()
    };
    mesh.vertices[1].id = mesh.vertices[0].id;
    assert!(document.validate().is_err());
}
