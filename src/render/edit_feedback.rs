//! Visual edit-selection state. Selection remains a set of source vertex IDs;
//! edge and face colors are derived feedback, not additional selection modes.
//! Occlusion belongs to the renderer's depth test, never to these colors.

use std::collections::BTreeSet;

pub const SELECTED: egui::Color32 = egui::Color32::from_rgb(255, 169, 64);
pub const UNSELECTED: egui::Color32 = egui::Color32::from_rgb(25, 28, 34);
pub const VERTEX_BORDER: egui::Color32 = egui::Color32::from_rgb(12, 14, 18);
pub const FACE_ALPHA: f32 = 0.22;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditSelection {
    pub object: Option<u64>,
    pub vertices: BTreeSet<u64>,
}

impl EditSelection {
    pub fn vertex_selected(&self, object: u64, vertex: u64) -> bool {
        self.object == Some(object) && self.vertices.contains(&vertex)
    }

    /// An original polygon receives a face tint only when every one of its
    /// source vertices is selected. A fully selected derived triangle inside
    /// a partially selected n-gon must not light up part of the polygon.
    pub fn face_selected(&self, object: u64, vertices: &[u64]) -> bool {
        self.object == Some(object)
            && !vertices.is_empty()
            && vertices.iter().all(|vertex| self.vertices.contains(vertex))
    }

    /// One color per endpoint allows the GPU to interpolate the orange-to-dark
    /// transition. An edge becomes uniformly orange only when both ends are
    /// selected; it is not implicitly promoted to an edge-selection mode.
    pub fn edge_colors(&self, object: u64, vertices: [u64; 2]) -> [egui::Color32; 2] {
        vertices.map(|vertex| {
            if self.vertex_selected(object, vertex) {
                SELECTED
            } else {
                UNSELECTED
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_endpoints_encode_none_partial_and_full_selection_without_guessing() {
        let mut selection = EditSelection {
            object: Some(20),
            ..Default::default()
        };
        assert_eq!(selection.edge_colors(20, [1, 2]), [UNSELECTED; 2]);
        selection.vertices.insert(1);
        assert_eq!(selection.edge_colors(20, [1, 2]), [SELECTED, UNSELECTED]);
        assert_eq!(selection.edge_colors(20, [2, 1]), [UNSELECTED, SELECTED]);
        selection.vertices.insert(2);
        assert_eq!(selection.edge_colors(20, [1, 2]), [SELECTED; 2]);
    }

    #[test]
    fn original_polygon_requires_every_vertex_even_when_one_triangle_is_selected() {
        let mut selection = EditSelection {
            object: Some(20),
            vertices: BTreeSet::from([1, 2, 3]),
        };
        assert!(selection.face_selected(20, &[1, 2, 3]));
        assert!(!selection.face_selected(20, &[1, 2, 3, 4]));
        assert!(!selection.face_selected(20, &[1, 2, 3, 4, 5]));
        selection.vertices.extend([4, 5]);
        assert!(selection.face_selected(20, &[1, 2, 3, 4, 5]));
        assert!(!selection.face_selected(20, &[]));
    }

    #[test]
    fn local_vertex_ids_never_leak_selection_between_objects() {
        let selection = EditSelection {
            object: Some(20),
            vertices: BTreeSet::from([1, 2, 3]),
        };
        assert!(!selection.vertex_selected(21, 1));
        assert_eq!(selection.edge_colors(21, [1, 2]), [UNSELECTED; 2]);
        assert!(!selection.face_selected(21, &[1, 2, 3]));
        let inactive = EditSelection {
            object: None,
            ..selection
        };
        assert!(!inactive.vertex_selected(20, 1));
        assert!(!inactive.face_selected(20, &[1, 2, 3]));
    }
}
