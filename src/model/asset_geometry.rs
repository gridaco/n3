//! Derived placement geometry for bounds, picking and selection feedback.
//! Render vertices have no authoring identity: never expose these as editable
//! mesh topology or infer polygons by welding material/UV seams.
use glam::DVec3;

use super::document::{AssetFrames, DisplayFrame, Document, Geometry, Object};
use crate::{
    mesh::{MeshData, ObjectRange, Vertex},
    scene::{EvaluatedScene, Topology},
};

pub(crate) struct AssetGeometry {
    pub points: Vec<DVec3>,
    pub triangles: Vec<[usize; 3]>,
    pub lines: Vec<[usize; 2]>,
    pub standalone_points: Vec<usize>,
}

/// Bound aggregate instance amplification before allocating selection proxies.
/// A shared source can be placed many times, but every placement costs geometry.
pub(crate) fn validate_budget(document: &Document, assets: &AssetFrames) -> Result<(), String> {
    let mut vertices = 0usize;
    let mut indices = 0usize;
    for object in &document.objects {
        let Geometry::Asset(reference) = &object.geometry else {
            continue;
        };
        let Some(frame) = assets.get(reference) else {
            continue;
        };
        for draw in &frame.draws {
            vertices = vertices
                .checked_add(draw.vertices.len())
                .filter(|count| *count <= crate::scene::budgets::MAX_FRAME_VERTICES)
                .ok_or("Placed assets exceed the four million vertex selection budget.")?;
            indices = indices
                .checked_add(draw.indices.len())
                .filter(|count| *count <= crate::scene::budgets::MAX_FRAME_INDICES)
                .ok_or("Placed assets exceed the twelve million index selection budget.")?;
        }
    }
    Ok(())
}

impl AssetGeometry {
    pub(crate) fn new(object: &Object, frame: &EvaluatedScene) -> Result<Self, String> {
        let mut output = Self {
            points: Vec::new(),
            triangles: Vec::new(),
            lines: Vec::new(),
            standalone_points: Vec::new(),
        };
        let matrix = object.transform.matrix();
        for draw in &frame.draws {
            let offset = output.points.len();
            for vertex in &draw.vertices {
                let point = matrix.transform_point3(DVec3::from_array(vertex.position));
                if !point.is_finite() {
                    return Err(format!(
                        "Object {}: linked asset placement exceeds the finite coordinate range.",
                        object.id
                    ));
                }
                output.points.push(point);
            }
            let index = |index: u32| -> Result<usize, String> {
                let index = index as usize;
                if index >= draw.vertices.len() {
                    return Err("Linked asset contains an invalid evaluated index.".into());
                }
                Ok(offset + index)
            };
            match draw.topology {
                Topology::Triangles => {
                    if !draw.indices.len().is_multiple_of(3) {
                        return Err("Linked asset contains an incomplete triangle.".into());
                    }
                    for triangle in draw.indices.as_chunks::<3>().0 {
                        let mut triangle = [
                            index(triangle[0])?,
                            index(triangle[1])?,
                            index(triangle[2])?,
                        ];
                        if draw.mirrored ^ (matrix.determinant() < 0.0) {
                            triangle.swap(1, 2);
                        }
                        output.triangles.push(triangle);
                    }
                }
                Topology::Lines => {
                    if !draw.indices.len().is_multiple_of(2) {
                        return Err("Linked asset contains an incomplete line.".into());
                    }
                    for line in draw.indices.as_chunks::<2>().0 {
                        output.lines.push([index(line[0])?, index(line[1])?]);
                    }
                }
                Topology::Points => {
                    for &point in draw.indices.iter() {
                        output.standalone_points.push(index(point)?);
                    }
                }
            }
        }
        Ok(output)
    }

    /// Empty scenes (for example lights/cameras only) retain a placement anchor
    /// for framing and transform controls without adding visible proxy geometry.
    pub(crate) fn into_framing_points(mut self, object: &Object) -> Vec<DVec3> {
        if self.points.is_empty() {
            self.points
                .push(DVec3::from_array(object.transform.translation));
        }
        self.points
    }

    pub(crate) fn append_render(
        &self,
        frame: &DisplayFrame,
        object: u64,
        output: &mut MeshData,
    ) -> Result<(), String> {
        let first_edge =
            u32::try_from(output.edges.len()).map_err(|_| "Combined render index overflow.")?;
        let first =
            u32::try_from(output.vertices.len()).map_err(|_| "Combined render index overflow.")?;
        let positions: Vec<_> = self.points.iter().map(|&point| {
            let position = frame.world_to_display(point).as_vec3();
            if !position.is_finite() {
                return Err("Linked asset exceeds the display coordinate range; frame the document again.".to_string());
            }
            Ok(position)
        }).collect::<Result<_,_>>()?;
        for &[a, b, c] in &self.triangles {
            // Degenerate imported triangles are legal render data. They neither
            // acquire an authored face nor invalidate otherwise useful assets.
            let normal = (positions[b] - positions[a])
                .cross(positions[c] - positions[a])
                .normalize_or_zero()
                .to_array();
            for index in [a, b, c] {
                output.vertices.push(Vertex {
                    position: positions[index].to_array(),
                    normal,
                });
            }
            for [a, b] in [[a, b], [b, c], [c, a]] {
                for index in [a, b] {
                    output.edges.push(Vertex {
                        position: positions[index].to_array(),
                        normal: [0.0; 3],
                    });
                }
            }
        }
        let first_loose_edge =
            u32::try_from(output.edges.len()).map_err(|_| "Combined render index overflow.")?;
        for &[a, b] in &self.lines {
            for index in [a, b] {
                output.edges.push(Vertex {
                    position: positions[index].to_array(),
                    normal: [0.0; 3],
                });
            }
        }
        let last =
            u32::try_from(output.vertices.len()).map_err(|_| "Combined render index overflow.")?;
        let last_edge =
            u32::try_from(output.edges.len()).map_err(|_| "Combined render index overflow.")?;
        output.object_ranges.push(ObjectRange {
            object,
            triangles: first..last,
            edges: first_edge..last_edge,
            loose_edges: first_loose_edge..last_edge,
        });
        output.vertex_count += self.points.len();
        // Imported triangles are rendering primitives, not authored polygons.
        // They contribute triangle/vertex statistics, never editable face counts.
        Ok(())
    }
}
