//! OBJ import into N3 authored topology; format conventions stop here.
use crate::document::{
    Document, EditableMesh, Face, Geometry, MAX_FACE_CORNERS, MAX_OBJECTS, MAX_VERTICES,
    MeshVertex, Object, Transform, check_budget,
};
use std::collections::BTreeSet;

type Result<T> = std::result::Result<T, String>;

/// Preserve OBJ position indices and source polygons. Each object owns its
/// referenced vertices; equal coordinates are never welded. Unreferenced source
/// vertices belong to the first nonempty object, so no position is discarded.
/// A source position shared across OBJ groups becomes one owned vertex in each.
/// OBJ provides no canonical length metadata: raw coordinates are kept exactly
/// and interpreted as centimeters, without guessing or applying a scale factor.
/// Non-surface records
/// (materials, UVs, normals, lines and points) are outside this importer.
pub(crate) fn parse_obj(text: &str, default_name: &str) -> Result<Document> {
    let mut positions = Vec::<[f64; 3]>::new();
    let mut groups: Vec<(String, Vec<Face>)> = vec![(default_name.into(), Vec::new())];
    let mut normal_count = 0usize;
    let mut uv_count = 0usize;
    let mut face_id = 0u64;
    let mut corners = 0usize;
    let mut work = 0usize;
    for (line_index, line) in text.lines().enumerate() {
        let body = line.split('#').next().unwrap_or("").trim();
        let mut fields = body.split_whitespace();
        let Some(record) = fields.next() else {
            continue;
        };
        let fail = |message: String| format!("OBJ line {}: {message}", line_index + 1);
        match record {
            "v" | "vn" | "vt" => {
                let values: Vec<f64> = fields
                    .map(|s| {
                        s.parse::<f64>()
                            .map_err(|_| fail("Invalid coordinate".into()))
                    })
                    .collect::<Result<_>>()?;
                if values.iter().any(|v| !v.is_finite()) {
                    return Err(fail("Coordinates must be finite".into()));
                }
                match record {
                    "v" => {
                        if !(3..=7).contains(&values.len()) {
                            return Err(fail("A vertex requires XYZ coordinates".into()));
                        }
                        if positions.len() >= MAX_VERTICES {
                            return Err(fail("OBJ exceeds the vertex limit".into()));
                        }
                        positions.push([values[0], values[1], values[2]]);
                    }
                    "vn" => {
                        if values.len() != 3 {
                            return Err(fail("A normal requires three coordinates".into()));
                        }
                        normal_count += 1;
                    }
                    _ => {
                        if !(1..=3).contains(&values.len()) {
                            return Err(fail(
                                "Texture coordinates require one to three values".into(),
                            ));
                        }
                        uv_count += 1;
                    }
                }
                if normal_count > MAX_VERTICES || uv_count > MAX_VERTICES {
                    return Err(fail("OBJ exceeds the attribute limit".into()));
                }
            }
            "o" | "g" => {
                let name = fields.collect::<Vec<_>>().join(" ");
                let name = if name.is_empty() {
                    default_name.to_owned()
                } else {
                    name
                };
                if groups.last().is_some_and(|(_, faces)| faces.is_empty()) {
                    groups.last_mut().unwrap().0 = name;
                } else {
                    if groups.len() >= MAX_OBJECTS {
                        return Err(fail("OBJ exceeds the object limit".into()));
                    }
                    groups.push((name, Vec::new()));
                }
            }
            "f" => {
                let mut vertices = Vec::new();
                for corner in fields {
                    let indices: Vec<_> = corner.split('/').collect();
                    if indices.len() > 3 || indices[0].is_empty() {
                        return Err(fail("Malformed face corner".into()));
                    }
                    vertices
                        .push(obj_index(indices[0], positions.len()).map_err(&fail)? as u64 + 1);
                    if indices.len() > 1 && !indices[1].is_empty() {
                        obj_index(indices[1], uv_count).map_err(&fail)?;
                    }
                    if indices.len() > 2 && !indices[2].is_empty() {
                        obj_index(indices[2], normal_count).map_err(&fail)?;
                    }
                    if vertices.len() > MAX_FACE_CORNERS {
                        return Err(fail("Polygon exceeds the corner limit".into()));
                    }
                }
                check_budget(vertices.len(), &mut corners, &mut work).map_err(&fail)?;
                face_id += 1;
                groups.last_mut().unwrap().1.push(Face {
                    id: face_id,
                    vertices,
                });
            }
            _ => {}
        }
    }
    if face_id == 0 {
        return Err("OBJ contains no polygon faces".into());
    }
    let mut document = Document::default();
    let all_referenced: BTreeSet<_> = groups
        .iter()
        .flat_map(|(_, faces)| faces.iter().flat_map(|f| f.vertices.iter().copied()))
        .collect();
    let mut owned_vertex_count = 0usize;
    for (name, faces) in groups {
        if faces.is_empty() {
            continue;
        }
        let mut referenced: BTreeSet<_> = faces
            .iter()
            .flat_map(|f| f.vertices.iter().copied())
            .collect();
        if document.objects.is_empty() {
            referenced
                .extend((1..=positions.len() as u64).filter(|id| !all_referenced.contains(id)));
        }
        owned_vertex_count += referenced.len();
        if owned_vertex_count > MAX_VERTICES {
            return Err("OBJ exceeds the owned vertex limit after splitting objects".into());
        }
        let vertices = referenced
            .into_iter()
            .map(|id| MeshVertex {
                id,
                position: positions[id as usize - 1],
            })
            .collect();
        document.objects.push(Object {
            id: document.objects.len() as u64 + 1,
            name,
            transform: Transform::default(),
            geometry: Geometry::Mesh(EditableMesh {
                vertices,
                faces,
                edges: Vec::new(),
            }),
        });
    }
    document.validate()?;
    Ok(document)
}

fn obj_index(text: &str, count: usize) -> Result<usize> {
    let index = text
        .parse::<i64>()
        .map_err(|_| "Invalid OBJ index".to_owned())?;
    let resolved = if index > 0 {
        index - 1
    } else if index < 0 {
        count as i64 + index
    } else {
        -1
    };
    if resolved < 0 || resolved >= count as i64 {
        return Err(format!("OBJ index {index} is out of range"));
    }
    Ok(resolved as usize)
}

#[cfg(test)]
#[path = "obj_tests.rs"]
mod tests;
