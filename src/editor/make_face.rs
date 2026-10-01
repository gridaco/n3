//! Conservative single-face creation. Selection supplies membership, never an
//! invented polygon order: larger faces require one authored boundary cycle.
use std::collections::{BTreeMap, BTreeSet};

use glam::DVec3;

use crate::document::{EditableMesh, Face, Geometry, MAX_FACE_CORNERS, planar_polygon_normal};

use super::{Editor, geometry_edit};

type Edge = (u64, u64);

impl Editor {
    /// Cheap interaction eligibility, not a promise that the selected topology
    /// forms a valid face. The command reports geometric rejection explicitly.
    pub fn can_make_face(&self) -> bool {
        self.can_edit()
            && self.edit_mode
            && self.selected_objects.len() == 1
            && self.selected_object.is_some()
            && self.selected_vertices.len() >= 3
            && !self.is_interacting()
            && self.numeric.is_none()
            && self.transform_axis.is_none()
    }

    /// Create one planar polygon through the ordinary validated edit lifecycle.
    /// Pending edits must be acknowledged first: `commit` cancels previews, so
    /// this action must never enter it while another interaction owns history.
    /// No-op/rejected attempts leave source recipes, redo, and selection intact.
    pub fn make_face(&mut self) -> Result<bool, String> {
        self.require_write()?;
        if self.is_interacting() || self.numeric.is_some() || self.transform_axis.is_some() {
            return Err("Apply or cancel the current interaction before making a face.".into());
        }
        if !self.edit_mode || self.selected_objects.len() != 1 {
            return Err("Enter vertex edit mode on one object before making a face.".into());
        }
        if self.selected_vertices.len() < 3 {
            return Err("Select at least three vertices to make a face.".into());
        }
        if self.selected_vertices.len() > MAX_FACE_CORNERS {
            return Err(format!(
                "A face supports at most {MAX_FACE_CORNERS} vertices."
            ));
        }
        let id = self.selected_object.ok_or("Select an object first.")?;
        let object = self
            .document
            .objects
            .iter()
            .find(|object| object.id == id)
            .ok_or("The edited object no longer exists.")?;
        let mesh = geometry_edit::evaluated(&object.geometry)?;
        let Some(face) = new_face(&mesh, &self.selected_vertices)? else {
            return Ok(false);
        };
        let consumed: BTreeSet<_> = face_edges(&face.vertices)
            .map(|[a, b]| edge(a, b))
            .collect();
        let mut mesh = mesh.into_owned();
        mesh.edges
            .retain(|[a, b]| !consumed.contains(&edge(*a, *b)));
        mesh.faces.push(face);

        let mut candidate = self.document.clone();
        candidate
            .objects
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap()
            .geometry = Geometry::Mesh(mesh);
        // Preflight before `commit` can clear an idle axis/nudge state. Rejected
        // candidates must not have any interaction or document side effects.
        self.validate_candidate(&mut candidate)?;
        self.commit("Make face", move |document| {
            *document = candidate;
            Ok(())
        })
    }
}

fn edge(a: u64, b: u64) -> Edge {
    (a.min(b), a.max(b))
}

fn face_edges(vertices: &[u64]) -> impl Iterator<Item = [u64; 2]> + '_ {
    vertices
        .iter()
        .copied()
        .zip(vertices.iter().copied().cycle().skip(1))
        .map(|(a, b)| [a, b])
}

fn new_face(mesh: &EditableMesh, selected: &BTreeSet<u64>) -> Result<Option<Face>, String> {
    let positions: BTreeMap<_, _> = mesh
        .vertices
        .iter()
        .map(|vertex| (vertex.id, DVec3::from_array(vertex.position)))
        .collect();
    if selected.iter().any(|id| !positions.contains_key(id)) {
        return Err("The selection contains a vertex that no longer exists.".into());
    }
    // Identical membership is already a face, regardless of its start corner
    // or winding. Do not convert primitives merely to discover this no-op.
    if mesh.faces.iter().any(|face| {
        face.vertices.len() == selected.len()
            && face.vertices.iter().all(|id| selected.contains(id))
    }) {
        return Ok(None);
    }
    for face in &mesh.faces {
        let corners: BTreeSet<_> = face.vertices.iter().copied().collect();
        if selected.is_subset(&corners) || corners.is_subset(selected) {
            return Err(
                "The selection overlaps an existing face; select an unfilled boundary.".into(),
            );
        }
    }

    // Retain direction per incident face. This is local manifold/winding
    // validation, not a global solidness or surface-intersection guarantee.
    let mut incident: BTreeMap<Edge, Vec<[u64; 2]>> = BTreeMap::new();
    for face in &mesh.faces {
        for [a, b] in face_edges(&face.vertices) {
            if selected.contains(&a) && selected.contains(&b) {
                incident.entry(edge(a, b)).or_default().push([a, b]);
            }
        }
    }
    let mut vertices = if selected.len() == 3 {
        selected.iter().copied().collect()
    } else {
        boundary_cycle(mesh, selected, &incident)?
    };
    let points: Vec<_> = vertices.iter().map(|id| positions[id]).collect();
    let normal = planar_polygon_normal(&points)?;
    let mut reverse = None;
    for [a, b] in face_edges(&vertices) {
        if let Some(neighbors) = incident.get(&edge(a, b)) {
            if neighbors.len() >= 2 {
                return Err("Making this face would add a third face to an occupied edge.".into());
            }
            let required = neighbors[0] == [a, b];
            if reverse.is_some_and(|value| value != required) {
                return Err(
                    "Neighboring faces require conflicting winding around this boundary.".into(),
                );
            }
            reverse = Some(required);
        }
    }
    // An isolated face has no neighbor to orient it. Choose the positive
    // dominant object-local normal, with X/Y/Z tie priority; never use camera
    // direction or selection order. Keep the smallest ID as the start corner.
    let dominant = (1..3).fold(0, |axis, next| {
        if normal[next].abs() > normal[axis].abs() {
            next
        } else {
            axis
        }
    });
    if reverse.unwrap_or(normal[dominant] < 0.0) {
        vertices[1..].reverse();
    }
    let id = mesh
        .faces
        .iter()
        .map(|face| face.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or("Face ID space exhausted.")?;
    Ok(Some(Face { id, vertices }))
}

fn boundary_cycle(
    mesh: &EditableMesh,
    selected: &BTreeSet<u64>,
    incident: &BTreeMap<Edge, Vec<[u64; 2]>>,
) -> Result<Vec<u64>, String> {
    let mut adjacency: BTreeMap<u64, BTreeSet<u64>> =
        selected.iter().map(|id| (*id, BTreeSet::new())).collect();
    for (a, b) in mesh.edges.iter().map(|[a, b]| (*a, *b)).chain(
        incident
            .iter()
            .filter(|(_, faces)| faces.len() == 1)
            .map(|(edge, _)| *edge),
    ) {
        if selected.contains(&a) && selected.contains(&b) {
            adjacency.get_mut(&a).unwrap().insert(b);
            adjacency.get_mut(&b).unwrap().insert(a);
        }
    }
    if adjacency.values().any(|neighbors| neighbors.len() != 2) {
        return Err(
            "Select one closed boundary made from loose edges or open face boundaries.".into(),
        );
    }
    let start = *selected.first().unwrap();
    let mut vertices = vec![start];
    let mut previous = start;
    let mut current = *adjacency[&start].first().unwrap();
    while current != start {
        vertices.push(current);
        let next = *adjacency[&current]
            .iter()
            .find(|id| **id != previous)
            .unwrap();
        previous = current;
        current = next;
    }
    if vertices.len() != selected.len() {
        return Err("Select a single boundary; separate loops and holes are not supported.".into());
    }
    Ok(vertices)
}

#[cfg(test)]
#[path = "make_face_tests.rs"]
mod tests;
