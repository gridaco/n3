use super::*;
use crate::{document::DisplayFrame, mesh::EditFace, units::CanonicalLengthUnit};
use glam::{DQuat, DVec3};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures/obj")
        .join(name)
}
fn source_mesh(document: &Document) -> &EditableMesh {
    match &document.objects[0].geometry {
        Geometry::Mesh(mesh) => mesh,
        _ => panic!("Expected mesh"),
    }
}
fn cube() -> Document {
    load_path(&fixture("cube-quads.obj")).unwrap()
}

#[test]
fn source_vertices_polygons_and_units_survive_rendering() {
    let document = cube();
    let mesh = source_mesh(&document);
    assert_eq!((mesh.vertices.len(), mesh.faces.len()), (8, 6));
    assert!(mesh.faces.iter().all(|f| f.vertices.len() == 4));
    let before = document.clone();
    let rendered = document
        .render_mesh(&DisplayFrame::from_document(&document).unwrap())
        .unwrap();
    assert_eq!(
        (
            rendered.vertex_count,
            rendered.vertices.len(),
            rendered.triangle_count
        ),
        (8, 36, 12)
    );
    assert_eq!(rendered.edges.len(), 24);
    assert_eq!(document, before);
    let offset = parse_obj(
        "v 100 200 300\nv 104 200 300\nv 100 204 300\nf 1 2 3",
        "Offset",
    )
    .unwrap();
    let frame = DisplayFrame::from_document(&offset).unwrap();
    assert_eq!(
        source_mesh(&offset).vertices[0].position,
        [100., 200., 300.]
    );
    assert_eq!(frame.center, [102., 202., 300.]);
    assert_eq!(frame.scale, 0.5);
}

#[test]
fn edit_topology_retains_original_concave_polygon_without_triangle_diagonals() {
    let document = parse_obj(
        "v 0 0 0\nv 3 0 0\nv 3 3 0\nv 1.5 1 0\nv 0 3 0\nf 1 2 3 4 5",
        "Concave polygon",
    )
    .unwrap();
    let before = document.to_json().unwrap();
    let rendered = document
        .render_mesh(&DisplayFrame::from_document(&document).unwrap())
        .unwrap();
    let topology = &rendered.edit_topology[0];
    assert_eq!(topology.object, document.objects[0].id);
    assert_eq!(topology.vertices.len(), 5);
    assert_eq!(topology.edges, 0..10);
    assert_eq!(
        topology.edge_vertices,
        vec![[1, 2], [2, 3], [3, 4], [4, 5], [5, 1]]
    );
    assert_eq!(
        topology.faces,
        vec![EditFace {
            vertices: vec![1, 2, 3, 4, 5],
            triangles: 0..9
        }]
    );
    assert_eq!(rendered.triangle_count, 3);
    let selection = crate::edit_feedback::EditSelection {
        object: Some(topology.object),
        vertices: BTreeSet::from([1, 2, 3]),
    };
    assert!(!selection.face_selected(topology.object, &topology.faces[0].vertices));
    assert_eq!(
        document.to_json().unwrap(),
        before,
        "Rendered edit metadata must never enter canonical serialization"
    );
}

#[test]
fn edit_topology_ranges_and_endpoints_match_transformed_gpu_buffers_across_objects() {
    let mut document = parse_obj(
        "v 0 0 0\nv 2 0 0\nv 2 2 0\nv 0 2 0\nf 1 2 3\nf 1 3 4",
        "Panel",
    )
    .unwrap();
    document.objects[0].id = 40;
    document.objects[0].transform = Transform {
        translation: [1.0, 3.0, -2.0],
        rotation: DQuat::from_euler(glam::EulerRot::XYZ, 0.2, 0.5, -0.3).to_array(),
        scale: [2.0, 0.5, 3.0],
    };
    let mut second = document.objects[0].clone();
    second.id = 70;
    second.transform.translation = [-4.0, 2.0, 8.0];
    document.objects.push(second);
    let frame = DisplayFrame::from_document(&document).unwrap();
    let rendered = document.render_mesh(&frame).unwrap();
    let mut edge_end = 0;
    let mut triangle_end = 0;
    for (object, topology) in document.objects.iter().zip(&rendered.edit_topology) {
        assert_eq!(topology.object, object.id);
        assert_eq!(topology.edges.start, edge_end);
        assert_eq!(topology.edges.len(), topology.edge_vertices.len() * 2);
        assert_eq!(
            topology.edge_vertices.len(),
            5,
            "The shared source edge is emitted only once"
        );
        edge_end = topology.edges.end;
        let source = document.eval_object(object.id).unwrap();
        for vertex in &topology.vertices {
            let point = source
                .vertices
                .iter()
                .find(|source| source.id == vertex.id)
                .unwrap();
            let expected = frame
                .world_to_display(
                    object
                        .transform
                        .matrix()
                        .transform_point3(DVec3::from_array(point.position)),
                )
                .as_vec3()
                .to_array();
            assert_eq!(vertex.position, expected);
        }
        for (index, pair) in topology.edge_vertices.iter().enumerate() {
            for (endpoint, id) in pair.iter().enumerate() {
                let vertex = topology
                    .vertices
                    .iter()
                    .find(|vertex| vertex.id == *id)
                    .unwrap();
                assert_eq!(
                    rendered.edges[topology.edges.start as usize + index * 2 + endpoint].position,
                    vertex.position
                );
            }
        }
        for face in &topology.faces {
            assert_eq!(face.triangles.start, triangle_end);
            assert_eq!(face.triangles.len(), 3);
            for vertex in
                &rendered.vertices[face.triangles.start as usize..face.triangles.end as usize]
            {
                assert!(
                    topology
                        .vertices
                        .iter()
                        .any(|source| face.vertices.contains(&source.id)
                            && source.position == vertex.position)
                );
            }
            triangle_end = face.triangles.end;
        }
    }
    assert_eq!(edge_end as usize, rendered.edges.len());
    assert_eq!(triangle_end as usize, rendered.vertices.len());
    assert_eq!(
        rendered.edit_topology[0].vertices[0].id,
        rendered.edit_topology[1].vertices[0].id
    );
    assert_ne!(
        rendered.edit_topology[0].vertices[0].position,
        rendered.edit_topology[1].vertices[0].position
    );
}

#[test]
fn obj_corner_attributes_do_not_split_position_identity_or_weld_coincidences() {
    let document = parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 0\nvn 0 0 1\nvn 0 0 -1\nf 1//1 2//1 3//1\nf 4//2 3//2 2//2", "Shared").unwrap();
    let mesh = source_mesh(&document);
    assert_eq!(mesh.vertices.len(), 4);
    assert_eq!(mesh.vertices[0].position, mesh.vertices[3].position);
    assert_ne!(mesh.vertices[0].id, mesh.vertices[3].id);
    assert_eq!(mesh.faces[0].vertices[1], mesh.faces[1].vertices[2]);
    assert_eq!(mesh.triangles().unwrap().len(), 2);
}

#[test]
fn negative_obj_indices_and_concave_polygons_preserve_source_topology() {
    let negative = load_path(&fixture("negative-indices.obj")).unwrap();
    assert_eq!(source_mesh(&negative).faces[0].vertices, vec![1, 2, 3, 4]);
    let concave = load_path(&fixture("concave-ngon.obj")).unwrap();
    assert_eq!(source_mesh(&concave).faces[0].vertices.len(), 6);
    assert_eq!(source_mesh(&concave).triangles().unwrap().len(), 4);
}

#[test]
fn suzanne_imports_without_missing_material_failure() {
    let document = super::super::document::load_path(&fixture("suzanne.obj")).unwrap();
    let vertices: usize = document
        .objects
        .iter()
        .map(|o| document.eval_object(o.id).unwrap().vertices.len())
        .sum();
    let faces: usize = document
        .objects
        .iter()
        .map(|o| document.eval_object(o.id).unwrap().faces.len())
        .sum();
    assert_eq!((vertices, faces), (2012, 3936));
    let mesh = document
        .render_mesh(&DisplayFrame::from_document(&document).unwrap())
        .unwrap();
    assert_eq!(mesh.triangle_count, 3936);
    assert!(mesh.warnings.iter().any(|w| w.contains("authored normals")));
}

#[test]
fn invalid_obj_indices_and_nonfinite_inputs_are_rejected() {
    for text in [
        "v NaN 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3",
        "v 0 0 0\nf 0 1 1",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 1",
    ] {
        assert!(parse_obj(text, "Bad").is_err());
    }
}

#[test]
fn tiny_and_huge_units_validate_without_cross_product_overflow() {
    for scale in [1e-150, 1e150] {
        let mut document = cube();
        let Geometry::Mesh(mesh) = &mut document.objects[0].geometry else {
            unreachable!()
        };
        for vertex in &mut mesh.vertices {
            vertex.position = vertex.position.map(|v| v * scale);
        }
        let frame = DisplayFrame::from_document(&document).unwrap();
        let rendered = document.render_mesh(&frame).unwrap();
        assert_eq!(rendered.triangle_count, 12);
        assert!(rendered.vertices.iter().all(|v| {
            v.position
                .iter()
                .all(|p| p.is_finite() && p.abs() <= 1.00001)
        }));
    }
    let offset = parse_obj(
        "v 10000000000000000 0 0\nv 10000000000000004 0 0\nv 10000000000000000 4 0\nf 1 2 3",
        "Far",
    )
    .unwrap();
    let frame = DisplayFrame::from_document(&offset).unwrap();
    assert_eq!(
        frame.world_to_display(DVec3::from_array(source_mesh(&offset).vertices[0].position)),
        DVec3::new(-1., -1., 0.)
    );
}

#[test]
fn open_and_nonmanifold_meshes_are_valid_without_watertightness_assumptions() {
    let document = parse_obj(
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 -1 0\nv 0 0 1\nf 1 2 3\nf 2 1 4\nf 1 2 5",
        "Nonmanifold",
    )
    .unwrap();
    assert_eq!(source_mesh(&document).faces.len(), 3);
    document.validate().unwrap();
}

#[test]
fn legacy_v1_without_length_metadata_retains_coordinates_and_gains_cm_on_save() {
    let mut document = parse_obj(
        "v .1 .25 -1.5\nv 2.54 .25 -1.5\nv .1 30.48 -1.5\nf 1 2 3",
        "Legacy lengths",
    )
    .unwrap();
    document.objects[0].transform.translation = [0.125, -2.54, 30.48];
    let mut legacy: serde_json::Value = serde_json::from_str(&document.to_json().unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("length_unit");
    let reopened = Document::from_json(&legacy.to_string()).unwrap();
    assert_eq!(
        reopened, document,
        "Legacy coordinates must not be rescaled"
    );
    assert_eq!(reopened.length_unit, CanonicalLengthUnit::Centimeters);
    let saved: serde_json::Value = serde_json::from_str(&reopened.to_json().unwrap()).unwrap();
    assert_eq!(saved["version"], 1);
    assert_eq!(saved["length_unit"], "cm");
    assert_eq!(saved["objects"], legacy["objects"]);
    assert_eq!(
        Document::from_json(r#"{"version":1,"objects":[]}"#).unwrap(),
        Document::default()
    );
}

#[test]
fn obj_coordinates_are_imported_unchanged_as_centimeters() {
    let document = parse_obj(
        "v .1 2.54 30.48\nv 1.1 2.54 30.48\nv .1 3.54 30.48\nf 1 2 3",
        "Unscaled OBJ",
    )
    .unwrap();
    assert_eq!(document.length_unit, CanonicalLengthUnit::Centimeters);
    assert_eq!(
        source_mesh(&document)
            .vertices
            .iter()
            .map(|vertex| vertex.position)
            .collect::<Vec<_>>(),
        vec![[0.1, 2.54, 30.48], [1.1, 2.54, 30.48], [0.1, 3.54, 30.48]]
    );
    assert_eq!(document.objects[0].transform, Transform::default());
    assert_eq!(cube().length_unit, CanonicalLengthUnit::Centimeters);
}
