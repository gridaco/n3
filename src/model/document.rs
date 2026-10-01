//! Editable source data. Triangulation, display normalization and GPU vertices
//! are derived, and never replace authored coordinates or polygon identities.
//! One canonical length unit is one centimeter, including fractional values.
//! Input/display conversions do not alter document units or authored numbers.
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use glam::{DMat4, DQuat, DVec3};
use serde::{Deserialize, Serialize};

use crate::{
    mesh::{EditFace, EditObjectTopology, EditVertex, MeshData, ObjectRange, Vertex, triangulate},
    units::CanonicalLengthUnit,
};

type Result<T> = std::result::Result<T, String>;
// This version also fixes primitive evaluation recipes. A recipe change that
// changes evaluated geometry requires a document migration or a version bump.
pub const VERSION: u32 = 1;
pub(crate) const MAX_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) const MAX_OBJECTS: usize = 10_000;
pub(crate) const MAX_VERTICES: usize = 1_000_000;
const MAX_CORNERS: usize = 2_000_000;
pub(crate) const MAX_FACE_CORNERS: usize = 4096;
const MAX_POLYGON_WORK: usize = 50_000_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub version: u32,
    /// Additive v1 metadata: legacy files without this field retain their exact
    /// coordinates, now explicitly interpreted as centimeters. Never rescale.
    #[serde(default)]
    pub length_unit: CanonicalLengthUnit,
    pub objects: Vec<Object>,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            version: VERSION,
            length_unit: CanonicalLengthUnit::Centimeters,
            objects: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub id: u64,
    pub name: String,
    pub transform: Transform,
    pub geometry: Geometry,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    /// Translation in canonical centimeters; rotation and scale are dimensionless.
    pub translation: [f64; 3],
    /// Quaternion in xyzw order. Stored rotations must be unit length.
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
        }
    }
}

impl Transform {
    pub fn matrix(&self) -> DMat4 {
        DMat4::from_scale_rotation_translation(
            DVec3::from_array(self.scale),
            DQuat::from_array(self.rotation),
            DVec3::from_array(self.translation),
        )
    }

    fn validate(&self) -> Result<()> {
        if !self
            .translation
            .iter()
            .chain(self.scale.iter())
            .chain(self.rotation.iter())
            .all(|v| v.is_finite())
        {
            return Err("Transform components must be finite".into());
        }
        if self.scale.contains(&0.0) {
            return Err("Object scale must be nonzero on every axis".into());
        }
        let norm = self.rotation.iter().map(|v| v * v).sum::<f64>();
        if !norm.is_finite() || (norm - 1.0).abs() > 1e-8 {
            return Err("Object rotation must be a unit quaternion".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "data",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Geometry {
    Primitive(Primitive),
    Mesh(EditableMesh),
    /// Linked immutable content. Placement belongs to the object; imported
    /// vertex attributes, materials and animation never enter edit history.
    Asset(AssetInstance),
}

/// Additive v1 geometry variant. Existing documents retain their exact schema;
/// older binaries reject this unknown variant rather than losing linked data.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetInstance {
    pub source: String,
    pub scene: usize,
}

/// Runtime-only evaluated resources, shared by duplicate placements and excluded
/// from serialization, authored topology, snapshots and undo history.
pub(crate) type AssetFrames = BTreeMap<AssetInstance, Arc<crate::scene::EvaluatedScene>>;

impl AssetInstance {
    fn validate(&self) -> Result<()> {
        if self.source.trim().is_empty() || self.source.contains('\0') || self.source.len() > 16_384
        {
            return Err("Asset source must be a nonempty path of at most 16384 bytes without NUL characters.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveKind {
    Cube,
    Cylinder,
    Cone,
    Torus,
    Plane,
    Circle,
    Sphere,
    Polyhedron,
}

impl PrimitiveKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cube => "Cube",
            Self::Cylinder => "Cylinder",
            Self::Cone => "Cone",
            Self::Torus => "Torus",
            Self::Plane => "Plane",
            Self::Circle => "Circle",
            Self::Sphere => "Sphere",
            Self::Polyhedron => "Polyhedron",
        }
    }
}

/// The five convex regular solids share one parametric recipe. Switching type
/// changes topology while preserving the primitive and its object transform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolyhedronType {
    Tetrahedron,
    Cube,
    Octahedron,
    Icosahedron,
    Dodecahedron,
}

impl PolyhedronType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tetrahedron => "Tetrahedron (4 faces)",
            Self::Cube => "Cube (6 faces)",
            Self::Octahedron => "Octahedron (8 faces)",
            Self::Icosahedron => "Icosahedron (20 faces)",
            Self::Dodecahedron => "Dodecahedron (12 faces)",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Primitive {
    pub kind: PrimitiveKind,
    /// Nominal full XYZ dimensions in object-local centimeters; Y is up.
    /// Plane uses X/Y only; Circle uses equal X/Y diameters. Both retain Z as
    /// a positive schema placeholder.
    pub size: [f64; 3],
    pub segments: u32,
    pub minor_segments: u32,
    /// Torus tube radius divided by outer radius, strictly between zero and 0.5.
    pub minor_radius: f64,
    /// Present only for the Polyhedron recipe; older v1 primitives omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub polyhedron_type: Option<PolyhedronType>,
    /// Circle alone can fill its boundary with one polygon. Omission preserves
    /// legacy recipes and gives new circles an unfilled boundary by default.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fill: bool,
}

impl Primitive {
    pub fn new(kind: PrimitiveKind) -> Self {
        Self {
            kind,
            size: match kind {
                PrimitiveKind::Torus => [2.0, 0.5, 2.0],
                PrimitiveKind::Plane | PrimitiveKind::Circle => [2.0, 2.0, 1.0],
                _ => [2.0; 3],
            },
            segments: 32,
            minor_segments: 16,
            minor_radius: 0.25,
            polyhedron_type: (kind == PrimitiveKind::Polyhedron)
                .then_some(PolyhedronType::Icosahedron),
            fill: false,
        }
    }

    fn validate(&self) -> Result<()> {
        if (self.kind == PrimitiveKind::Polyhedron) != self.polyhedron_type.is_some() {
            return Err("Only Polyhedron primitives require a polyhedron type".into());
        }
        if self.kind == PrimitiveKind::Polyhedron
            && (self.size[0] != self.size[1] || self.size[0] != self.size[2])
        {
            return Err("Polyhedron size must be uniform; use object Scale to stretch it".into());
        }
        if self.fill && self.kind != PrimitiveKind::Circle {
            return Err("Only Circle primitives have an optional fill".into());
        }
        if self.kind == PrimitiveKind::Circle && self.size[0] != self.size[1] {
            return Err("Circle diameters must match; use object Scale to stretch it".into());
        }
        if self.size.iter().any(|v| !v.is_finite() || *v <= 0.0) {
            return Err("Primitive dimensions must be positive finite numbers".into());
        }
        if !(3..=256).contains(&self.segments) || !(3..=128).contains(&self.minor_segments) {
            return Err("Primitive segments must be 3–256; tube segments must be 3–128".into());
        }
        if !self.minor_radius.is_finite() || self.minor_radius <= 0.0 || self.minor_radius >= 0.5 {
            return Err("Torus tube ratio must be strictly between zero and 0.5".into());
        }
        Ok(())
    }

    pub fn evaluate(&self) -> Result<EditableMesh> {
        self.validate()?;
        let mut positions = Vec::new();
        let mut polygons: Vec<Vec<usize>> = Vec::new();
        let mut edges = Vec::new();
        let half = DVec3::from_array(self.size) * 0.5;
        let n = self.segments as usize;
        let angle = |i: usize, count: usize| std::f64::consts::TAU * i as f64 / count as f64;
        match self.kind {
            PrimitiveKind::Circle => {
                for i in 0..n {
                    let a = angle(i, n);
                    positions.push(DVec3::new(a.cos() * half.x, a.sin() * half.x, 0.0));
                }
                if self.fill {
                    polygons.push((0..n).collect());
                } else {
                    edges.extend((0..n).map(|i| [i as u64 + 1, ((i + 1) % n) as u64 + 1]));
                }
            }
            PrimitiveKind::Plane => {
                // Local XY faces +Z. In 2D the insertion path rotates this
                // plane into the active view without changing its recipe.
                positions = vec![
                    DVec3::new(-half.x, -half.y, 0.0),
                    DVec3::new(half.x, -half.y, 0.0),
                    DVec3::new(half.x, half.y, 0.0),
                    DVec3::new(-half.x, half.y, 0.0),
                ];
                polygons.push(vec![0, 1, 2, 3]);
            }
            PrimitiveKind::Cube => {
                (positions, polygons) = cuboid(half);
            }
            PrimitiveKind::Cylinder | PrimitiveKind::Cone => {
                for i in 0..n {
                    let a = angle(i, n);
                    positions.push(DVec3::new(a.cos() * half.x, -half.y, a.sin() * half.z));
                }
                // Increasing azimuth viewed from below winds outward.
                polygons.push((0..n).collect());
                if self.kind == PrimitiveKind::Cylinder {
                    for i in 0..n {
                        let mut p = positions[i];
                        p.y = half.y;
                        positions.push(p);
                    }
                    polygons.push((n..2 * n).rev().collect());
                    for i in 0..n {
                        let j = (i + 1) % n;
                        polygons.push(vec![i, n + i, n + j, j]);
                    }
                } else {
                    positions.push(DVec3::new(0.0, half.y, 0.0));
                    for i in 0..n {
                        polygons.push(vec![i, n, (i + 1) % n]);
                    }
                }
            }
            PrimitiveKind::Torus => {
                let m = self.minor_segments as usize;
                for i in 0..n {
                    let u = angle(i, n);
                    for j in 0..m {
                        let v = angle(j, m);
                        let radius = 1.0 - self.minor_radius + self.minor_radius * v.cos();
                        positions.push(DVec3::new(
                            radius * u.cos() * half.x,
                            v.sin() * half.y,
                            radius * u.sin() * half.z,
                        ));
                    }
                }
                for i in 0..n {
                    for j in 0..m {
                        polygons.push(vec![
                            i * m + j,
                            i * m + (j + 1) % m,
                            ((i + 1) % n) * m + (j + 1) % m,
                            ((i + 1) % n) * m + j,
                        ]);
                    }
                }
            }
            PrimitiveKind::Sphere => {
                let rings = self.minor_segments as usize;
                positions.push(DVec3::new(0.0, half.y, 0.0));
                for latitude in 1..rings {
                    let theta = std::f64::consts::PI * latitude as f64 / rings as f64;
                    for longitude in 0..n {
                        let phi = angle(longitude, n);
                        positions.push(DVec3::new(
                            half.x * theta.sin() * phi.cos(),
                            half.y * theta.cos(),
                            half.z * theta.sin() * phi.sin(),
                        ));
                    }
                }
                let bottom = positions.len();
                positions.push(DVec3::new(0.0, -half.y, 0.0));
                for longitude in 0..n {
                    let next = (longitude + 1) % n;
                    polygons.push(vec![0, 1 + next, 1 + longitude]);
                    for latitude in 0..rings - 2 {
                        let top = 1 + latitude * n;
                        let below = top + n;
                        polygons.push(vec![
                            top + longitude,
                            top + next,
                            below + next,
                            below + longitude,
                        ]);
                    }
                    let last = 1 + (rings - 2) * n;
                    polygons.push(vec![bottom, last + longitude, last + next]);
                }
            }
            PrimitiveKind::Polyhedron => {
                (positions, polygons) = match self.polyhedron_type.expect("validated above") {
                    PolyhedronType::Tetrahedron => regular_tetrahedron(),
                    PolyhedronType::Cube => cuboid(DVec3::ONE),
                    PolyhedronType::Octahedron => regular_octahedron(),
                    PolyhedronType::Icosahedron => regular_icosahedron(),
                    PolyhedronType::Dodecahedron => regular_dodecahedron(),
                };
                let extent = positions
                    .iter()
                    .fold(DVec3::ZERO, |max, point| max.max(point.abs()));
                for point in &mut positions {
                    *point = *point / extent * half;
                }
            }
        }
        let mut mesh = editable_mesh(positions, polygons);
        mesh.edges = edges;
        Ok(mesh)
    }
}

fn editable_mesh(positions: Vec<DVec3>, polygons: Vec<Vec<usize>>) -> EditableMesh {
    EditableMesh {
        edges: Vec::new(),
        vertices: positions
            .into_iter()
            .enumerate()
            .map(|(i, p)| MeshVertex {
                id: i as u64 + 1,
                position: p.to_array(),
            })
            .collect(),
        faces: polygons
            .into_iter()
            .enumerate()
            .map(|(i, p)| Face {
                id: i as u64 + 1,
                vertices: p.into_iter().map(|v| v as u64 + 1).collect(),
            })
            .collect(),
    }
}

fn cuboid(half: DVec3) -> (Vec<DVec3>, Vec<Vec<usize>>) {
    let positions = [
        [-1., -1., -1.],
        [1., -1., -1.],
        [1., 1., -1.],
        [-1., 1., -1.],
        [-1., -1., 1.],
        [1., -1., 1.],
        [1., 1., 1.],
        [-1., 1., 1.],
    ]
    .into_iter()
    .map(|point| DVec3::from_array(point) * half)
    .collect();
    let faces = vec![
        vec![3, 2, 1, 0],
        vec![4, 5, 6, 7],
        vec![0, 1, 5, 4],
        vec![1, 2, 6, 5],
        vec![2, 3, 7, 6],
        vec![3, 0, 4, 7],
    ];
    (positions, faces)
}

fn regular_tetrahedron() -> (Vec<DVec3>, Vec<Vec<usize>>) {
    // Alternating corners of a cube give four equilateral triangular faces.
    let positions = [[1., 1., 1.], [1., -1., -1.], [-1., 1., -1.], [-1., -1., 1.]]
        .into_iter()
        .map(DVec3::from_array)
        .collect();
    let faces = [[0, 1, 2], [0, 3, 1], [0, 2, 3], [1, 3, 2]]
        .into_iter()
        .map(Vec::from)
        .collect();
    (positions, faces)
}

fn regular_octahedron() -> (Vec<DVec3>, Vec<Vec<usize>>) {
    let positions = [
        DVec3::X,
        DVec3::NEG_X,
        DVec3::Y,
        DVec3::NEG_Y,
        DVec3::Z,
        DVec3::NEG_Z,
    ]
    .into_iter()
    .collect();
    let faces = [
        [2, 4, 0],
        [2, 0, 5],
        [2, 5, 1],
        [2, 1, 4],
        [3, 0, 4],
        [3, 5, 0],
        [3, 1, 5],
        [3, 4, 1],
    ]
    .into_iter()
    .map(Vec::from)
    .collect();
    (positions, faces)
}

fn regular_icosahedron() -> (Vec<DVec3>, Vec<Vec<usize>>) {
    let golden = (1.0 + 5.0_f64.sqrt()) * 0.5;
    let positions = vec![
        [-1.0, golden, 0.0],
        [1.0, golden, 0.0],
        [-1.0, -golden, 0.0],
        [1.0, -golden, 0.0],
        [0.0, -1.0, golden],
        [0.0, 1.0, golden],
        [0.0, -1.0, -golden],
        [0.0, 1.0, -golden],
        [golden, 0.0, -1.0],
        [golden, 0.0, 1.0],
        [-golden, 0.0, -1.0],
        [-golden, 0.0, 1.0],
    ]
    .into_iter()
    .map(DVec3::from_array)
    .collect();
    let faces = [
        [0, 11, 5],
        [0, 5, 1],
        [0, 1, 7],
        [0, 7, 10],
        [0, 10, 11],
        [1, 5, 9],
        [5, 11, 4],
        [11, 10, 2],
        [10, 7, 6],
        [7, 1, 8],
        [3, 9, 4],
        [3, 4, 2],
        [3, 2, 6],
        [3, 6, 8],
        [3, 8, 9],
        [4, 9, 5],
        [2, 4, 11],
        [6, 2, 10],
        [8, 6, 7],
        [9, 8, 1],
    ]
    .into_iter()
    .map(Vec::from)
    .collect();
    (positions, faces)
}

fn regular_dodecahedron() -> (Vec<DVec3>, Vec<Vec<usize>>) {
    // The dual of the icosahedron has one vertex per triangular face and one
    // pentagon per original vertex. Sort each ring in its outward tangent plane.
    let (icosahedron, triangles) = regular_icosahedron();
    let positions: Vec<DVec3> = triangles
        .iter()
        .map(|face| (icosahedron[face[0]] + icosahedron[face[1]] + icosahedron[face[2]]) / 3.0)
        .collect();
    let polygons = icosahedron
        .iter()
        .enumerate()
        .map(|(vertex, outward)| {
            let normal = outward.normalize();
            let tangent = normal.any_orthonormal_vector();
            let bitangent = normal.cross(tangent);
            let mut ring: Vec<usize> = triangles
                .iter()
                .enumerate()
                .filter_map(|(face, triangle)| triangle.contains(&vertex).then_some(face))
                .collect();
            ring.sort_by(|&a, &b| {
                let angle = |index: usize| {
                    let point = positions[index];
                    point.dot(bitangent).atan2(point.dot(tangent))
                };
                angle(a).total_cmp(&angle(b))
            });
            let a = positions[ring[1]] - positions[ring[0]];
            let b = positions[ring[2]] - positions[ring[0]];
            if a.cross(b).dot(normal) < 0.0 {
                ring.reverse();
            }
            ring
        })
        .collect();
    (positions, polygons)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EditableMesh {
    pub vertices: Vec<MeshVertex>,
    pub faces: Vec<Face>,
    /// Explicit edges that do not belong to a face. Endpoints use stable vertex
    /// IDs; face boundaries remain derived from their authored polygons.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<[u64; 2]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshVertex {
    pub id: u64,
    /// Object-local position in canonical centimeters, without display normalization.
    pub position: [f64; 3],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Face {
    pub id: u64,
    pub vertices: Vec<u64>,
}

impl EditableMesh {
    pub fn validate(&self) -> Result<()> {
        let map = self.vertex_map()?;
        let mut face_ids = BTreeSet::new();
        let mut corners = 0usize;
        let mut work = 0usize;
        let mut edges = BTreeSet::new();
        for face in &self.faces {
            if face.id == 0 || !face_ids.insert(face.id) {
                return Err("Face IDs must be nonzero and unique within their mesh".into());
            }
            check_budget(face.vertices.len(), &mut corners, &mut work)?;
            let mut seen = BTreeSet::new();
            let points: Vec<_> = face
                .vertices
                .iter()
                .map(|id| {
                    if !seen.insert(*id) {
                        return Err(format!("Face {} repeats vertex {id}", face.id));
                    }
                    map.get(id)
                        .copied()
                        .ok_or_else(|| format!("Face {} references missing vertex {id}", face.id))
                })
                .collect::<Result<_>>()?;
            checked_triangulate(&points).map_err(|e| format!("Face {}: {e}", face.id))?;
            for i in 0..face.vertices.len() {
                let a = face.vertices[i];
                let b = face.vertices[(i + 1) % face.vertices.len()];
                edges.insert((a.min(b), a.max(b)));
            }
        }
        check_edge_budget(self.edges.len(), &mut corners)?;
        for [a, b] in &self.edges {
            let first = map
                .get(a)
                .ok_or_else(|| format!("Edge references missing vertex {a}"))?;
            let second = map
                .get(b)
                .ok_or_else(|| format!("Edge references missing vertex {b}"))?;
            if a == b {
                return Err("Edge endpoints must be distinct vertex IDs".into());
            }
            if first == second {
                return Err("Edges must have nonzero length".into());
            }
            if !edges.insert(((*a).min(*b), (*a).max(*b))) {
                return Err("Explicit edges must not repeat an edge or a face boundary".into());
            }
        }
        Ok(())
    }

    fn vertex_map(&self) -> Result<BTreeMap<u64, DVec3>> {
        if self.vertices.len() > MAX_VERTICES {
            return Err("Mesh exceeds the vertex limit".into());
        }
        let mut map = BTreeMap::new();
        for vertex in &self.vertices {
            let p = DVec3::from_array(vertex.position);
            if !p.is_finite() {
                return Err(format!("Vertex {} has a nonfinite position", vertex.id));
            }
            if vertex.id == 0 || map.insert(vertex.id, p).is_some() {
                return Err("Vertex IDs must be nonzero and unique within their mesh".into());
            }
        }
        Ok(map)
    }

    pub fn triangles(&self) -> Result<Vec<[u64; 3]>> {
        self.validate()?;
        let map = self.vertex_map()?;
        let mut triangles = Vec::new();
        for face in &self.faces {
            let points: Vec<_> = face.vertices.iter().map(|id| map[id]).collect();
            let (indices, _, _) = checked_triangulate(&points)?;
            triangles.extend(indices.as_chunks::<3>().0.iter().map(|t| {
                [
                    face.vertices[t[0]],
                    face.vertices[t[1]],
                    face.vertices[t[2]],
                ]
            }));
        }
        Ok(triangles)
    }
}

fn check_edge_budget(count: usize, corners: &mut usize) -> Result<()> {
    *corners = count
        .checked_mul(2)
        .and_then(|endpoints| corners.checked_add(endpoints))
        .ok_or("Edge endpoint count overflow")?;
    if *corners > MAX_CORNERS {
        return Err("Geometry exceeds the edge and polygon corner limit".into());
    }
    Ok(())
}

pub(crate) fn check_budget(count: usize, corners: &mut usize, work: &mut usize) -> Result<()> {
    if !(3..=MAX_FACE_CORNERS).contains(&count) {
        return Err(format!(
            "Polygons must contain 3–{MAX_FACE_CORNERS} distinct vertices"
        ));
    }
    *corners = corners
        .checked_add(count)
        .ok_or("Polygon corner count overflow")?;
    *work = work
        .checked_add(if count > 3 { count * count } else { count })
        .ok_or("Polygon work overflow")?;
    if *corners > MAX_CORNERS || *work > MAX_POLYGON_WORK {
        return Err("Geometry exceeds the polygon processing limit".into());
    }
    Ok(())
}

/// A per-face normalization protects validation and normals from absolute world
/// offsets, overflow of cross products, and very small authored coordinate units.
fn checked_triangulate(points: &[DVec3]) -> Result<(Vec<usize>, DVec3, bool)> {
    let (minimum, maximum) = bounds(points.iter().copied())?.ok_or("Polygon is empty")?;
    let extent = (maximum - minimum).max_element();
    if extent <= 0.0 {
        return Err("Polygon has no extent".into());
    }
    let normalized: Vec<_> = points.iter().map(|p| (*p - minimum) / extent).collect();
    triangulate(&normalized)
}

/// Face creation uses the same scale-aware polygon classification as document
/// validation, but requires a planar boundary. Import/evaluation still allow
/// nonplanar polygons and retain their existing projected-triangulation policy.
pub(crate) fn planar_polygon_normal(points: &[DVec3]) -> Result<DVec3> {
    check_budget(points.len(), &mut 0, &mut 0)?;
    let (_, normal, nonplanar) = checked_triangulate(points)?;
    if nonplanar {
        return Err(
            "Make Face needs a planar boundary; selected vertices will not be flattened.".into(),
        );
    }
    Ok(normal)
}

fn bounds(points: impl IntoIterator<Item = DVec3>) -> Result<Option<(DVec3, DVec3)>> {
    let mut minimum = DVec3::splat(f64::INFINITY);
    let mut maximum = DVec3::splat(f64::NEG_INFINITY);
    let mut any = false;
    for point in points {
        if !point.is_finite() {
            return Err("Geometry contains a nonfinite world coordinate".into());
        }
        minimum = minimum.min(point);
        maximum = maximum.max(point);
        any = true;
    }
    if !any {
        return Ok(None);
    }
    if !(maximum - minimum).is_finite() {
        return Err("Geometry extent exceeds supported f64 range".into());
    }
    Ok(Some((minimum, maximum)))
}

impl Document {
    pub fn eval_object(&self, id: u64) -> Result<EditableMesh> {
        let object = self
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| format!("Object {id} does not exist"))?;
        match &object.geometry {
            Geometry::Primitive(p) => p.evaluate(),
            Geometry::Mesh(m) => Ok(m.clone()),
            Geometry::Asset(_) => Err(
                "Linked asset contents are read-only; only their object placement can be edited."
                    .into(),
            ),
        }
    }

    /// The closure edits a candidate; failure leaves the original document intact.
    pub fn transact(&mut self, edit: impl FnOnce(&mut Self) -> Result<()>) -> Result<()> {
        let mut candidate = self.clone();
        edit(&mut candidate)?;
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn convert_object(&mut self, id: u64) -> Result<()> {
        let mesh = self.eval_object(id)?;
        self.transact(move |document| {
            document
                .objects
                .iter_mut()
                .find(|o| o.id == id)
                .ok_or("Object no longer exists")?
                .geometry = Geometry::Mesh(mesh);
            Ok(())
        })
    }

    pub fn insert_primitive(&mut self, kind: PrimitiveKind) -> Result<u64> {
        let id = self
            .objects
            .iter()
            .map(|o| o.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Object ID space exhausted")?;
        self.transact(|document| {
            document.objects.push(Object {
                id,
                name: kind.label().into(),
                transform: Transform::default(),
                geometry: Geometry::Primitive(Primitive::new(kind)),
            });
            Ok(())
        })?;
        Ok(id)
    }

    #[cfg(test)]
    pub fn insert_asset(&mut self, source: AssetInstance, name: String) -> Result<u64> {
        let id = self
            .objects
            .iter()
            .map(|object| object.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Object ID space exhausted")?;
        self.transact(|document| {
            document.objects.push(Object {
                id,
                name,
                transform: Transform::default(),
                geometry: Geometry::Asset(source),
            });
            Ok(())
        })?;
        Ok(id)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version != VERSION {
            return Err(format!(
                "Unsupported document version {}; expected {VERSION}",
                self.version
            ));
        }
        if self.objects.len() > MAX_OBJECTS {
            return Err("Document exceeds the object limit".into());
        }
        let mut ids = BTreeSet::new();
        let mut vertices = 0usize;
        let mut corners = 0usize;
        let mut work = 0usize;
        let mut world_points = Vec::new();
        for object in &self.objects {
            if object.id == 0 || !ids.insert(object.id) {
                return Err("Object IDs must be nonzero and unique".into());
            }
            object.transform.validate()?;
            if let Geometry::Asset(asset) = &object.geometry {
                asset.validate()?;
                continue;
            }
            let mesh = self.eval_object(object.id)?;
            vertices = vertices
                .checked_add(mesh.vertices.len())
                .ok_or("Vertex count overflow")?;
            if vertices > MAX_VERTICES {
                return Err("Document exceeds the vertex limit".into());
            }
            for face in &mesh.faces {
                check_budget(face.vertices.len(), &mut corners, &mut work)?;
            }
            check_edge_budget(mesh.edges.len(), &mut corners)?;
            mesh.validate()
                .map_err(|e| format!("Object {}: {e}", object.id))?;
            let matrix = object.transform.matrix();
            let transformed: Vec<_> = mesh
                .vertices
                .iter()
                .map(|v| (v.id, matrix.transform_point3(DVec3::from_array(v.position))))
                .collect();
            let map: BTreeMap<_, _> = transformed.iter().copied().collect();
            // Invertible transforms in exact arithmetic can still collapse a
            // face after finite-precision arithmetic at a very large offset.
            for face in &mesh.faces {
                let points: Vec<_> = face.vertices.iter().map(|id| map[id]).collect();
                checked_triangulate(&points).map_err(|e| {
                    format!("Object {}, transformed face {}: {e}", object.id, face.id)
                })?;
            }
            for [a, b] in &mesh.edges {
                if map[a] == map[b] {
                    return Err(format!(
                        "Object {}, transformed edge has zero length",
                        object.id
                    ));
                }
            }
            world_points.extend(transformed.into_iter().map(|(_, p)| p));
        }
        bounds(world_points)?;
        Ok(())
    }

    pub fn from_json(text: &str) -> Result<Self> {
        if text.len() as u64 > MAX_BYTES {
            return Err("Document exceeds the 64 MiB input limit".into());
        }
        let document: Self =
            serde_json::from_str(text).map_err(|e| format!("Invalid document JSON: {e}"))?;
        document.validate()?;
        Ok(document)
    }

    pub fn to_json(&self) -> Result<String> {
        self.validate()?;
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Cannot serialize document: {e}"))?;
        text.push('\n');
        if text.len() as u64 > MAX_BYTES {
            return Err("Serialized document exceeds the 64 MiB input limit".into());
        }
        Ok(text)
    }

    #[cfg(test)]
    pub fn render_mesh(&self, frame: &DisplayFrame) -> Result<MeshData> {
        self.render_mesh_with_assets(frame, &AssetFrames::new())
    }

    pub(crate) fn render_mesh_with_assets(
        &self,
        frame: &DisplayFrame,
        assets: &AssetFrames,
    ) -> Result<MeshData> {
        super::asset_geometry::validate_budget(self, assets)?;
        self.validate()?;
        frame.validate()?;
        let mut output = MeshData {
            vertices: Vec::new(),
            edges: Vec::new(),
            object_ranges: Vec::new(),
            edit_topology: Vec::new(),
            vertex_count: 0,
            face_count: 0,
            triangle_count: 0,
            object_count: self.objects.len(),
            source_extent: [0.; 3],
            warnings: Vec::new(),
        };
        let mut all_points = Vec::new();
        let mut warped = 0;
        for object in &self.objects {
            if let Geometry::Asset(asset) = &object.geometry {
                if let Some(evaluated) = assets.get(asset) {
                    crate::scene::display_limits::validate_display(
                        DVec3::from_array(frame.center),
                        frame.scale,
                        object.transform.matrix(),
                        evaluated,
                    )?;
                    let geometry = super::asset_geometry::AssetGeometry::new(object, evaluated)?;
                    geometry.append_render(frame, object.id, &mut output)?;
                    all_points.extend(geometry.into_framing_points(object));
                } else {
                    output.warnings.push(format!(
                        "Linked asset '{}' is unavailable; its placement remains in the document.",
                        object.name
                    ));
                    all_points.push(DVec3::from_array(object.transform.translation));
                }
                continue;
            }
            let first_vertex = output.vertices.len() as u32;
            let first_edge = output.edges.len() as u32;
            let mesh = self.eval_object(object.id)?;
            output.vertex_count += mesh.vertices.len();
            output.face_count += mesh.faces.len();
            let matrix = object.transform.matrix();
            let positions: BTreeMap<_, _> = mesh
                .vertices
                .iter()
                .map(|v| (v.id, matrix.transform_point3(DVec3::from_array(v.position))))
                .collect();
            all_points.extend(positions.values().copied());
            let mut topology = EditObjectTopology {
                object: object.id,
                edges: first_edge..first_edge,
                loose_edges: first_edge..first_edge,
                vertices: mesh.vertices.iter().map(|vertex| {
                    let position = frame.world_to_display(positions[&vertex.id]).as_vec3();
                    if !position.is_finite() {
                        return Err("Geometry exceeds the fixed display frame's f32 range; frame the document again".into());
                    }
                    Ok(EditVertex { id: vertex.id, position: position.to_array() })
                }).collect::<Result<Vec<_>>>()?,
                edge_vertices: Vec::new(),
                faces: Vec::with_capacity(mesh.faces.len()),
            };
            let mut edges = BTreeSet::new();
            let local_map = mesh.vertex_map()?;
            for face in &mesh.faces {
                let first_face_vertex = output.vertices.len() as u32;
                let points: Vec<_> = face.vertices.iter().map(|id| positions[id]).collect();
                let local_points: Vec<_> = face.vertices.iter().map(|id| local_map[id]).collect();
                // Picking and rendering share triangulation in object-local space.
                let (triangles, _, nonplanar) = checked_triangulate(&local_points)?;
                let (_, normal, _) = checked_triangulate(&points)?;
                warped += usize::from(nonplanar);
                let vertex = |point: DVec3| -> Result<Vertex> {
                    let p = frame.world_to_display(point).as_vec3();
                    if !p.is_finite() {
                        return Err("Geometry exceeds the fixed display frame's f32 range; frame the document again".into());
                    }
                    Ok(Vertex {
                        position: p.to_array(),
                        normal: normal.as_vec3().to_array(),
                    })
                };
                for index in triangles {
                    output.vertices.push(vertex(points[index])?);
                }
                topology.faces.push(EditFace {
                    vertices: face.vertices.clone(),
                    triangles: first_face_vertex..output.vertices.len() as u32,
                });
                for i in 0..face.vertices.len() {
                    let a = face.vertices[i];
                    let b = face.vertices[(i + 1) % face.vertices.len()];
                    if edges.insert((a.min(b), a.max(b))) {
                        output.edges.push(vertex(positions[&a])?);
                        output.edges.push(vertex(positions[&b])?);
                        topology.edge_vertices.push([a, b]);
                    }
                }
            }
            topology.loose_edges.start = output.edges.len() as u32;
            for [a, b] in &mesh.edges {
                for id in [a, b] {
                    let position = frame.world_to_display(positions[id]).as_vec3();
                    if !position.is_finite() {
                        return Err("Geometry exceeds the fixed display frame's f32 range; frame the document again".into());
                    }
                    output.edges.push(Vertex {
                        position: position.to_array(),
                        // Line passes use positions and color, without lighting.
                        normal: [0.0; 3],
                    });
                }
                topology.edge_vertices.push([*a, *b]);
            }
            topology.loose_edges.end = output.edges.len() as u32;
            output.object_ranges.push(ObjectRange {
                object: object.id,
                triangles: first_vertex..output.vertices.len() as u32,
                edges: first_edge..output.edges.len() as u32,
                loose_edges: topology.loose_edges.clone(),
            });
            topology.edges.end = output.edges.len() as u32;
            output.edit_topology.push(topology);
        }
        output.triangle_count = output.vertices.len() / 3;
        if let Some((minimum, maximum)) = bounds(all_points)? {
            output.source_extent = (maximum - minimum).to_array();
        }
        if warped > 0 {
            output.warnings.push(format!(
                "{warped} non-planar polygon(s) use projected triangulation."
            ));
        }
        if self
            .objects
            .iter()
            .any(|object| !matches!(object.geometry, Geometry::Asset(_)))
        {
            output.warnings.push("Geometry documents use generated flat normals; OBJ materials, UVs and authored normals are not retained.".into());
        }
        Ok(output)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayFrame {
    pub center: [f64; 3],
    pub scale: f64,
}

impl Default for DisplayFrame {
    fn default() -> Self {
        Self {
            center: [0.; 3],
            scale: 1.0,
        }
    }
}

impl DisplayFrame {
    pub fn from_document(document: &Document) -> Result<Self> {
        Self::from_document_with_assets(document, &AssetFrames::new())
    }

    pub(crate) fn from_document_with_assets(
        document: &Document,
        assets: &AssetFrames,
    ) -> Result<Self> {
        super::asset_geometry::validate_budget(document, assets)?;
        document.validate()?;
        let mut points = Vec::new();
        for object in &document.objects {
            let matrix = object.transform.matrix();
            if let Geometry::Asset(asset) = &object.geometry {
                if let Some(evaluated) = assets.get(asset) {
                    points.extend(
                        super::asset_geometry::AssetGeometry::new(object, evaluated)?
                            .into_framing_points(object),
                    );
                } else {
                    points.push(DVec3::from_array(object.transform.translation));
                }
                continue;
            }
            points.extend(
                document
                    .eval_object(object.id)?
                    .vertices
                    .into_iter()
                    .map(|v| matrix.transform_point3(DVec3::from_array(v.position))),
            );
        }
        let Some((minimum, maximum)) = bounds(points)? else {
            return Ok(Self::default());
        };
        let extent = maximum - minimum;
        let largest = extent.max_element();
        let frame = Self {
            center: (minimum + extent * 0.5).to_array(),
            scale: if largest > 0.0 { 2.0 / largest } else { 1.0 },
        };
        frame.validate()?;
        Ok(frame)
    }

    fn validate(&self) -> Result<()> {
        if self.center.iter().any(|v| !v.is_finite())
            || !self.scale.is_finite()
            || self.scale <= 0.0
        {
            return Err("Display frame requires a finite center and positive finite scale".into());
        }
        Ok(())
    }
    pub fn world_to_display(&self, point: DVec3) -> DVec3 {
        (point - DVec3::from_array(self.center)) * self.scale
    }
    pub fn display_to_world(&self, point: DVec3) -> DVec3 {
        point / self.scale + DVec3::from_array(self.center)
    }
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
