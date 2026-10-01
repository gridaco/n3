//! Legacy parser regressions and derived mesh checks.

use super::*;
use std::{collections::HashSet, io::Cursor};

// Deliberate limits for a synchronous, disposable viewer. Check before parsing
// or triangulating so unexpectedly large input cannot expand without a bound.
const MAX_OBJ_BYTES: usize = 64 * 1024 * 1024;
const MAX_COORDINATE_RECORDS: usize = 1_000_000;
const MAX_TOTAL_CORNERS: usize = 2_000_000;
const MAX_FACE_CORNERS: usize = 4096;
const MAX_POLYGON_WORK: usize = 50_000_000;

struct SourceInfo {
    vertex_count: usize,
    face_normal_presence: Vec<Vec<bool>>,
    warnings: Vec<String>,
}

/// Validate before the parser can discard unsupported or malformed records.
/// Face normal presence is retained because parsers can fill missing indices.
fn inspect_source(text: &str) -> Result<SourceInfo, String> {
    if text.len() > MAX_OBJ_BYTES {
        return Err("OBJ exceeds this viewer's 64 MiB input limit".into());
    }
    let mut info = SourceInfo {
        vertex_count: 0,
        face_normal_presence: Vec::new(),
        warnings: Vec::new(),
    };
    let (mut normal_count, mut uv_count) = (0, 0);
    let (mut materials, mut lines_or_points) = (false, false);
    let (mut total_corners, mut polygon_work) = (0, 0);
    let mut group_count = 0;
    for (line_index, line) in text.lines().enumerate() {
        let line_number = line_index + 1;
        let mut parts = line.split('#').next().unwrap_or("").split_whitespace();
        let Some(kind) = parts.next() else { continue };
        let fields: Vec<_> = parts.take(MAX_FACE_CORNERS + 1).collect();
        let fail = |message: &str| format!("Line {line_number}: {message}");
        match kind {
            "v" | "vn" | "vt" => {
                let minimum = if kind == "vt" { 1 } else { 3 };
                let maximum = if kind == "v" { 7 } else { 3 };
                if fields.len() < minimum || fields.len() > maximum {
                    return Err(fail("unsupported coordinate record length"));
                }
                for field in &fields {
                    let value: f64 = field.parse().map_err(|_| fail("invalid coordinate"))?;
                    if !value.is_finite() {
                        return Err(fail("non-finite coordinates are unsupported"));
                    }
                }
                match kind {
                    "v" => info.vertex_count += 1,
                    "vn" => normal_count += 1,
                    _ => uv_count += 1,
                }
                if info.vertex_count > MAX_COORDINATE_RECORDS
                    || normal_count > MAX_COORDINATE_RECORDS
                    || uv_count > MAX_COORDINATE_RECORDS
                {
                    return Err(fail(
                        "coordinate records exceed this viewer's 1,000,000-per-type limit",
                    ));
                }
            }
            "f" => {
                if fields.len() < 3 {
                    return Err(fail("a face needs at least three vertices"));
                }
                if fields.len() > MAX_FACE_CORNERS {
                    return Err(fail("polygon exceeds this viewer's 4,096-corner limit"));
                }
                total_corners += fields.len();
                if total_corners > MAX_TOTAL_CORNERS {
                    return Err(fail(
                        "mesh exceeds this viewer's 2,000,000 face-corner limit",
                    ));
                }
                if fields.len() > 3 {
                    polygon_work += fields.len() * fields.len();
                }
                if polygon_work > MAX_POLYGON_WORK {
                    return Err(fail(
                        "polygons exceed this viewer's bounded triangulation-work budget; export triangles instead",
                    ));
                }
                let mut indices = HashSet::new();
                let mut normal_presence = Vec::with_capacity(fields.len());
                for corner in fields {
                    let values: Vec<_> = corner.split('/').take(4).collect();
                    if values.len() > 3 || values[0].is_empty() {
                        return Err(fail("invalid face corner"));
                    }
                    let index = resolve_index(values[0], info.vertex_count)
                        .map_err(|error| fail(&error))?;
                    if !indices.insert(index) {
                        return Err(fail("a face repeats a vertex"));
                    }
                    if let Some(uv) = values.get(1).filter(|value| !value.is_empty()) {
                        resolve_index(uv, uv_count).map_err(|error| fail(&error))?;
                    }
                    if let Some(normal) = values.get(2).filter(|value| !value.is_empty()) {
                        resolve_index(normal, normal_count).map_err(|error| fail(&error))?;
                        normal_presence.push(true);
                    } else {
                        normal_presence.push(false);
                    }
                }
                info.face_normal_presence.push(normal_presence);
            }
            "mtllib" | "usemtl" => materials = true,
            "l" | "p" => lines_or_points = true,
            "o" | "g" => {
                group_count += 1;
                if group_count > 10_000 {
                    return Err(fail(
                        "objects/groups exceed this viewer's 10,000-record limit",
                    ));
                }
            }
            _ => {}
        }
    }
    if info.face_normal_presence.is_empty() {
        return Err("No polygon faces found".into());
    }
    if materials {
        info.warnings
            .push("Materials and textures are ignored in this geometry viewer.".into());
    }
    if lines_or_points {
        info.warnings
            .push("Standalone OBJ lines and points are ignored.".into());
    }
    Ok(info)
}

fn resolve_index(text: &str, count: usize) -> Result<usize, String> {
    let value: i64 = text.parse().map_err(|_| "Invalid OBJ index")?;
    let count = i64::try_from(count).map_err(|_| "Too many OBJ records")?;
    let index = if value > 0 { value - 1 } else { count + value };
    if value == 0 || index < 0 || index >= count {
        return Err(format!("OBJ index {value} is out of range"));
    }
    Ok(index as usize)
}

fn load_text(text: &str) -> Result<MeshData, String> {
    let source = inspect_source(text)?;
    let mut geometry_text = String::with_capacity(text.len());
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut words = line.split_whitespace();
        match words.next() {
            // Ignored records must not create a second, unbounded parser path.
            None | Some("mtllib" | "usemtl" | "l" | "p") => continue,
            Some("vt") if words.clone().count() == 1 => {
                // OBJ permits one-component texture coordinates; tobj expects two.
                geometry_text.push_str(line);
                geometry_text.push_str(" 0\n");
            }
            _ => {
                geometry_text.push_str(line);
                geometry_text.push('\n');
            }
        }
    }
    let options = tobj::LoadOptions {
        triangulate: false,
        single_index: false,
        ignore_points: true,
        ignore_lines: true,
    };
    // No MTL reads: missing files must not prevent geometry inspection.
    let (models, _) = tobj::load_obj_buf(&mut Cursor::new(geometry_text), &options, |_| {
        Ok((Vec::new(), Default::default()))
    })
    .map_err(|error| format!("OBJ parse failed: {error}"))?;

    let mut minimum = DVec3::splat(f64::INFINITY);
    let mut maximum = DVec3::splat(f64::NEG_INFINITY);
    for model in &models {
        for position in model.mesh.positions.as_chunks::<3>().0 {
            let position = DVec3::new(position[0], position[1], position[2]);
            if !position.is_finite() {
                return Err("Non-finite mesh position".into());
            }
            minimum = minimum.min(position);
            maximum = maximum.max(position);
        }
    }
    let extent = maximum - minimum;
    let largest = extent.max_element();
    if !extent.is_finite() || !largest.is_finite() || largest <= 0.0 {
        return Err("Empty, zero-size, or unrepresentable mesh bounds".into());
    }
    // Work relative to the minimum first: the absolute midpoint may lie between
    // representable f64 values even when both bounds and their difference fit.
    let relative_extent = extent / largest;
    let mut output = MeshData {
        vertices: Vec::new(),
        edges: Vec::new(),
        object_ranges: Vec::new(),
        edit_topology: Vec::new(),
        vertex_count: source.vertex_count,
        face_count: source.face_normal_presence.len(),
        triangle_count: 0,
        object_count: models
            .iter()
            .filter(|model| !model.mesh.indices.is_empty())
            .count(),
        source_extent: extent.to_array(),
        warnings: source.warnings,
    };
    let mut source_face = 0;
    let mut invalid_normals = false;
    let mut nonplanar_faces = 0;
    for (object, model) in models.into_iter().enumerate() {
        let mesh = model.mesh;
        if mesh.indices.is_empty() {
            continue;
        }
        let first_vertex = output.vertices.len() as u32;
        let first_edge = output.edges.len() as u32;
        let positions: Vec<DVec3> = mesh
            .positions
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| ((DVec3::new(p[0], p[1], p[2]) - minimum) / largest) * 2.0 - relative_extent)
            .collect();
        // tobj omits face_arities when every face is a triangle.
        let arities: Vec<usize> = if mesh.face_arities.is_empty() {
            if mesh.indices.len() % 3 != 0 {
                return Err("Invalid triangle index count".into());
            }
            vec![3; mesh.indices.len() / 3]
        } else {
            mesh.face_arities
                .iter()
                .map(|&arity| arity as usize)
                .collect()
        };
        let mut offset = 0;
        let mut seen_edges = HashSet::new();
        for arity in arities {
            let face_indices = mesh
                .indices
                .get(offset..offset + arity)
                .ok_or("Invalid polygon index range")?;
            let points: Vec<DVec3> = face_indices
                .iter()
                .map(|&index| {
                    positions
                        .get(index as usize)
                        .copied()
                        .ok_or("Invalid vertex index")
                })
                .collect::<Result<_, _>>()?;
            let (triangles, flat_normal, nonplanar) = triangulate(&points)
                .map_err(|error| format!("Face {}: {error}", source_face + 1))?;
            nonplanar_faces += usize::from(nonplanar);
            let normal_presence = source
                .face_normal_presence
                .get(source_face)
                .ok_or("Parser changed the face count")?;
            if normal_presence.len() != arity {
                return Err("Parser changed the polygon corner count".into());
            }
            let mut normals = vec![flat_normal; arity];
            for corner in 0..arity {
                if normal_presence[corner] {
                    let authored = (|| {
                        let index = *mesh.normal_indices.get(offset + corner)? as usize;
                        let xyz = mesh.normals.get(index * 3..index * 3 + 3)?;
                        let normal = DVec3::new(xyz[0], xyz[1], xyz[2]);
                        // Scaling first also supports very large or very small finite normals.
                        let scale = normal.abs().max_element();
                        if scale == 0.0 || !scale.is_finite() {
                            return None;
                        }
                        Some((normal / scale).normalize())
                    })();
                    if let Some(authored) = authored {
                        normals[corner] = authored;
                    } else {
                        invalid_normals = true;
                    }
                }
            }
            let vertex = |corner: usize| Vertex {
                position: points[corner].as_vec3().to_array(),
                normal: normals[corner].as_vec3().to_array(),
            };
            for triangle in triangles.as_chunks::<3>().0 {
                output
                    .vertices
                    .extend(triangle.iter().map(|&corner| vertex(corner)));
            }
            for corner in 0..arity {
                let next = (corner + 1) % arity;
                let (a, b) = (face_indices[corner], face_indices[next]);
                if seen_edges.insert((a.min(b), a.max(b))) {
                    output.edges.extend([vertex(corner), vertex(next)]);
                }
            }
            offset += arity;
            source_face += 1;
        }
        if offset != mesh.indices.len() {
            return Err("Unconsumed polygon indices".into());
        }
        output.object_ranges.push(ObjectRange {
            object: object as u64 + 1,
            triangles: first_vertex..output.vertices.len() as u32,
            edges: first_edge..output.edges.len() as u32,
            loose_edges: output.edges.len() as u32..output.edges.len() as u32,
        });
    }
    if source_face != output.face_count || output.vertices.is_empty() {
        return Err("No complete polygon geometry, or parser changed the face count".into());
    }
    output.triangle_count = output.vertices.len() / 3;
    if invalid_normals {
        output
            .warnings
            .push("Invalid authored normals were replaced with flat face normals.".into());
    }
    if nonplanar_faces > 0 {
        output.warnings.push(format!(
            "{nonplanar_faces} non-planar polygon(s) were triangulated by projection; the surface may differ from the exporting application."
        ));
    }
    Ok(output)
}

const CUBE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/obj/cube-quads.obj"
));
const CONCAVE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/obj/concave-ngon.obj"
));
const NEGATIVE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/obj/negative-indices.obj"
));
const BRACKET: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/obj/bracket.obj"
));

#[test]
fn fixtures_preserve_polygon_statistics_and_edges() {
    for (source, vertices, faces, triangles, edges) in [
        (CUBE, 8, 6, 12, 12),
        (CONCAVE, 6, 1, 4, 6),
        (NEGATIVE, 4, 1, 2, 4),
        (BRACKET, 12, 8, 20, 18),
    ] {
        let mesh = load_text(source).unwrap();
        assert_eq!(
            (mesh.vertex_count, mesh.face_count, mesh.triangle_count),
            (vertices, faces, triangles)
        );
        assert_eq!(mesh.edges.len() / 2, edges);
        assert_eq!(mesh.object_count, 1);
    }
}

#[test]
fn concave_triangulation_covers_only_the_l_shape_and_preserves_winding() {
    let mesh = load_text(CONCAVE).unwrap();
    let mut total_area = 0.0;
    for triangle in mesh.vertices.as_chunks::<3>().0 {
        let p: Vec<_> = triangle
            .iter()
            .map(|v| DVec3::from_array(v.position.map(f64::from)))
            .collect();
        let cross = (p[1] - p[0]).cross(p[2] - p[0]);
        assert!(cross.z > 0.0);
        total_area += cross.length() / 2.0;
        let centroid = (p[0] + p[1] + p[2]) / 3.0;
        assert!(centroid.x <= 0.0 || centroid.y <= 0.0);
    }
    assert!((total_area - 3.0).abs() < 1e-6);
}

#[test]
fn malformed_nonfinite_and_empty_geometry_fail_cleanly() {
    for source in [
        "",
        "v 0 0 0\n",
        "v NaN 0 0\nf 1 1 1",
        "v inf 0 0\nf 1 1 1",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 9",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 0 2 3",
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 2",
        "v 0 0 0\nv 1 0 0\nv 2 0 0\nf 1 2 3",
        "v 0 0 0\nv 1 1 0\nv 0 1 0\nv 1 0 0\nf 1 2 3 4",
    ] {
        assert!(
            load_text(source).is_err(),
            "Unexpected success for {source:?}"
        );
    }
}

#[test]
fn normalizes_in_f64_before_gpu_conversion() {
    let source = "v 1000000000000000 1000000000000000 0\nv 1000000000000002 1000000000000000 0\nv 1000000000000002 1000000000000002 0\nv 1000000000000000 1000000000000002 0\nf 1 2 3 4";
    let mesh = load_text(source).unwrap();
    assert_eq!(mesh.source_extent, [2.0, 2.0, 0.0]);
    assert_eq!(mesh.triangle_count, 2);
    for v in mesh.vertices {
        assert_eq!(v.position[0].abs(), 1.0);
        assert_eq!(v.position[1].abs(), 1.0);
        assert_eq!(v.position[2], 0.0);
    }
}

#[test]
fn missing_material_does_not_block_geometry() {
    let source = format!("mtllib deliberately-missing.mtl\nusemtl absent\n{CUBE}");
    let mesh = load_text(&source).unwrap();
    assert_eq!(mesh.triangle_count, 12);
    assert!(
        mesh.warnings
            .iter()
            .any(|warning| warning.contains("Materials"))
    );
}

#[test]
fn authored_normals_are_normalized_and_missing_normals_are_flat() {
    let mesh = load_text(NEGATIVE).unwrap();
    assert!(mesh.vertices.iter().all(|v| v.normal == [0.0, 0.0, 1.0]));
    let source = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 1 1\nf 1//1 2//1 3//1\nf 1 2 3";
    let mesh = load_text(source).unwrap();
    for v in &mesh.vertices[..3] {
        assert!((v.normal[1] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
    }
    assert!(
        mesh.vertices[3..]
            .iter()
            .all(|v| v.normal == [0.0, 0.0, 1.0])
    );
}

#[test]
fn zero_authored_normals_fall_back_to_the_face_normal() {
    let source = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 0 0\nf 1//1 2//1 3//1";
    let mesh = load_text(source).unwrap();
    assert!(mesh.vertices.iter().all(|v| v.normal == [0.0, 0.0, 1.0]));
    assert!(
        mesh.warnings
            .iter()
            .any(|warning| warning.contains("Invalid authored normals"))
    );
}

#[test]
fn inline_comments_are_not_face_indices() {
    let source = "v 0 0 0 # origin\nv 1 0 0\nv 0 1 0\nf 1 2 3 # triangle";
    assert_eq!(load_text(source).unwrap().triangle_count, 1);
}

#[test]
fn crossing_boundaries_fail_but_warped_quads_are_displayable() {
    let crossed = "v 0 0 0\nv 2 3 0\nv 4 0 0\nv 0 2 0\nv 4 2 0\nf 1 2 3 4 5";
    assert!(
        load_text(crossed)
            .err()
            .unwrap()
            .contains("Self-intersecting")
    );
    let warped = "v 0 0 0\nv 1 0 0\nv 1 1 1\nv 0 1 0\nf 1 2 3 4";
    let mesh = load_text(warped).unwrap();
    assert_eq!(mesh.triangle_count, 2);
    assert!(
        mesh.warnings
            .iter()
            .any(|warning| warning.contains("non-planar"))
    );
}

#[test]
fn unrepresentable_absolute_midpoint_does_not_shift_the_display() {
    let source = "v 10000000000000000 0 0\nv 10000000000000002 0 0\nv 10000000000000002 2 0\nv 10000000000000000 2 0\nf 1 2 3 4";
    let mesh = load_text(source).unwrap();
    assert_eq!(mesh.source_extent, [2.0, 2.0, 0.0]);
    for vertex in mesh.vertices {
        assert_eq!(vertex.position[0].abs(), 1.0);
        assert_eq!(vertex.position[1].abs(), 1.0);
    }
}

#[test]
fn extreme_finite_sizes_normalize_before_geometric_calculations() {
    for size in ["1e-300", "1e300"] {
        let source = format!("v 0 0 0\nv {size} 0 0\nv 0 {size} 0\nf 1 2 3");
        let mesh = load_text(&source).unwrap();
        assert_eq!(mesh.triangle_count, 1);
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex
                .position
                .iter()
                .all(|n| n.is_finite() && n.abs() <= 1.0)
        }));
    }
}

#[test]
fn partial_authored_normals_and_one_component_uvs_remain_usable() {
    let source = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 1 0\nvt 0.5\nf 1/1/1 2/1 3/1";
    let mesh = load_text(source).unwrap();
    assert_eq!(mesh.vertices[0].normal, [0.0, 1.0, 0.0]);
    assert_eq!(mesh.vertices[1].normal, [0.0, 0.0, 1.0]);
    assert_eq!(mesh.vertices[2].normal, [0.0, 0.0, 1.0]);
}

#[test]
fn oversized_polygons_fail_before_index_parsing_or_quadratic_work() {
    let source = format!("f {}", vec!["1"; MAX_FACE_CORNERS + 1].join(" "));
    assert!(
        inspect_source(&source)
            .err()
            .unwrap()
            .contains("4,096-corner limit")
    );
    let vertices = "v 0 0 0\n".repeat(4000);
    let face = format!(
        "f {}\n",
        (1..=4000)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    );
    let source = format!("{vertices}{}", face.repeat(4));
    assert!(
        inspect_source(&source)
            .err()
            .unwrap()
            .contains("triangulation-work budget")
    );
}
