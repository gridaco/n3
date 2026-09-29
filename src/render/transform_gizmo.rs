//! Small, derived 3D solids shared by transform rendering and pointer picking.
//! Coordinates use the scene's display frame; these meshes never enter documents.

use bytemuck::{Pod, Zeroable};
use glam::{DQuat, DVec3};

#[derive(Clone, Debug)]
pub(crate) struct Endpoint {
    pub triangles: Vec<[DVec3; 3]>,
    pub stem_end: DVec3,
}

impl Endpoint {
    pub fn cone(center: DVec3, axis: DVec3, unit: f64) -> Self {
        let radius = unit * 6.0;
        let half_length = unit * 9.0;
        let base = center - axis * half_length;
        let tip = center + axis * half_length;
        let u = axis.any_orthonormal_vector();
        let v = axis.cross(u);
        let ring: Vec<_> = (0..16)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 16.0;
                base + (u * angle.cos() + v * angle.sin()) * radius
            })
            .collect();
        let mut triangles = Vec::with_capacity(32);
        for i in 0..16 {
            let next = (i + 1) % 16;
            triangles.push([ring[i], ring[next], tip]);
            triangles.push([base, ring[next], ring[i]]);
        }
        Self {
            triangles,
            stem_end: base,
        }
    }

    pub fn cube(center: DVec3, rotation: DQuat, stem_axis: DVec3, unit: f64) -> Self {
        let half_size = unit * 5.5;
        let corners: [DVec3; 8] = std::array::from_fn(|i| {
            let sign = |bit| if i & bit == 0 { -1.0 } else { 1.0 };
            center + rotation * DVec3::new(sign(1), sign(2), sign(4)) * half_size
        });
        let mut triangles = Vec::with_capacity(12);
        for [a, b, c, d] in [
            [0, 4, 6, 2],
            [1, 3, 7, 5],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 2, 3, 1],
            [4, 5, 7, 6],
        ] {
            triangles.push([corners[a], corners[b], corners[c]]);
            triangles.push([corners[a], corners[c], corners[d]]);
        }
        // A diagonal uniform-scale stem meets the first cube face, too.
        let local_axis = rotation.inverse() * stem_axis;
        let inset = half_size / local_axis.abs().max_element();
        Self {
            triangles,
            stem_end: center - stem_axis * inset,
        }
    }
}

/// Homogeneous clip positions preserve perspective interpolation and hardware
/// near-plane clipping. Color is already in the viewport's gamma space.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct GizmoVertex {
    pub position: [f32; 4],
    pub color: [f32; 4],
}
