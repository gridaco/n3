//! Derived rendering buffers and polygon triangulation. Authored data lives in
//! document.rs; external format parsing belongs to asset_io.

use glam::{DVec2, DVec3};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectRange {
    pub object: u64,
    /// Vertex indices into `MeshData::vertices`, ready for a triangle-list draw.
    pub triangles: std::ops::Range<u32>,
    /// Derived edge-buffer ranges for any object, including immutable assets.
    /// Unlike EditObjectTopology, these do not imply authoring vertex IDs.
    pub edges: std::ops::Range<u32>,
    pub loose_edges: std::ops::Range<u32>,
}

/// Authored topology identities alongside derived GPU buffers. These are
/// render-cache metadata, never canonical document fields or triangulation
/// replacements. Vertex IDs are scoped to their containing object.
#[derive(Clone, Debug, PartialEq)]
pub struct EditVertex {
    pub id: u64,
    /// Normalized display coordinates, matching the mesh vertex buffer.
    pub position: [f32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditFace {
    /// Complete original polygon, so selecting one derived triangle does not
    /// imply that the authored face is selected.
    pub vertices: Vec<u64>,
    /// Vertex indices into `MeshData::vertices`, ready for triangle-list draw.
    pub triangles: std::ops::Range<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditObjectTopology {
    pub object: u64,
    /// Vertex indices into `MeshData::edges` (two entries per authored edge).
    pub edges: std::ops::Range<u32>,
    /// Standalone edges within `edges`, following the polygon boundaries.
    /// These remain visible when the optional Solid face-edge overlay is hidden.
    pub loose_edges: std::ops::Range<u32>,
    /// Includes standalone vertices, even when no face references them.
    pub vertices: Vec<EditVertex>,
    /// Original endpoint IDs in the same pair order as the edge buffer range.
    /// No triangulation diagonals are introduced.
    pub edge_vertices: Vec<[u64; 2]>,
    pub faces: Vec<EditFace>,
}

pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub edges: Vec<Vertex>,
    pub object_ranges: Vec<ObjectRange>,
    pub edit_topology: Vec<EditObjectTopology>,
    pub vertex_count: usize,
    pub face_count: usize,
    pub triangle_count: usize,
    pub object_count: usize,
    pub source_extent: [f64; 3],
    pub warnings: Vec<String>,
}

/// Triangulate a simple polygon's projection, preserving winding. Warped input
/// is displayable, but the chosen diagonals are not an authoring guarantee.
pub(crate) fn triangulate(points: &[DVec3]) -> Result<(Vec<usize>, DVec3, bool), String> {
    if points.len() < 3 {
        return Err("A polygon needs at least three corners".into());
    }
    let origin = points[0];
    let mut area_vector = DVec3::ZERO;
    let mut scale: f64 = 0.0;
    for i in 0..points.len() {
        let a = points[i] - origin;
        let b = points[(i + 1) % points.len()] - origin;
        area_vector += a.cross(b);
        scale = scale.max(a.length());
    }
    let tolerance = scale * scale * 1e-12;
    let area = area_vector.length();
    if scale == 0.0 || !area.is_finite() || area <= tolerance {
        return Err("Degenerate polygon has no stable surface normal".into());
    }
    let normal = area_vector / area;
    if points.len() == 3 {
        return Ok((vec![0, 1, 2], normal, false));
    }
    let nonplanar = points
        .iter()
        .any(|point| (*point - origin).dot(normal).abs() > scale * 1e-6);
    let dominant = normal.abs().max_position();
    let project = |p: DVec3| match dominant {
        0 => DVec2::new(p.y, p.z),
        1 => DVec2::new(p.x, p.z),
        _ => DVec2::new(p.x, p.y),
    };
    let projected: Vec<_> = points
        .iter()
        .map(|&point| project(point - origin))
        .collect();
    for i in 0..points.len() {
        let next_i = (i + 1) % points.len();
        if (projected[i] - projected[next_i]).length_squared() <= tolerance * 1e-12 {
            return Err("Polygon has a zero-length boundary edge".into());
        }
        for j in i + 1..points.len() {
            let next_j = (j + 1) % points.len();
            if next_i == j || next_j == i {
                continue;
            }
            if segments_intersect(
                projected[i],
                projected[next_i],
                projected[j],
                projected[next_j],
                tolerance,
            ) {
                return Err("Self-intersecting polygons are unsupported".into());
            }
        }
    }
    let coordinates: Vec<_> = projected.iter().flat_map(|p| [p.x, p.y]).collect();
    let mut indices = earcutr::earcut(&coordinates, &[], 2)
        .map_err(|error| format!("Polygon triangulation failed: {error:?}"))?;
    if indices.is_empty() || indices.len() % 3 != 0 {
        return Err("Polygon triangulation produced no complete triangles".into());
    }
    let mut triangle_area = 0.0;
    for triangle in indices.as_chunks_mut::<3>().0 {
        if triangle.iter().any(|&index| index >= points.len()) {
            return Err("Triangulation produced an invalid index".into());
        }
        let winding = (points[triangle[1]] - points[triangle[0]])
            .cross(points[triangle[2]] - points[triangle[0]])
            .dot(normal);
        if winding.abs() <= tolerance {
            return Err("Triangulation produced a degenerate triangle".into());
        }
        if winding < 0.0 {
            triangle.swap(1, 2);
        }
        triangle_area += winding.abs();
    }
    if (triangle_area - area).abs() > area * 1e-8 + tolerance {
        return Err("Triangulation does not cover the polygon area".into());
    }
    Ok((indices, normal, nonplanar))
}

fn segments_intersect(a: DVec2, b: DVec2, c: DVec2, d: DVec2, epsilon: f64) -> bool {
    let orient = |p: DVec2, q: DVec2, r: DVec2| (q - p).perp_dot(r - p);
    let (ab_c, ab_d, cd_a, cd_b) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    if ((ab_c > epsilon && ab_d < -epsilon) || (ab_c < -epsilon && ab_d > epsilon))
        && ((cd_a > epsilon && cd_b < -epsilon) || (cd_a < -epsilon && cd_b > epsilon))
    {
        return true;
    }
    let on_segment = |p: DVec2, q: DVec2, r: DVec2, cross: f64| {
        cross.abs() <= epsilon && (r - p).dot(r - q) <= epsilon
    };
    on_segment(a, b, c, ab_c)
        || on_segment(a, b, d, ab_d)
        || on_segment(c, d, a, cd_a)
        || on_segment(c, d, b, cd_b)
}

#[cfg(test)]
#[path = "mesh_tests.rs"]
mod tests;
