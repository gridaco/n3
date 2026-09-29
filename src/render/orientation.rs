use glam::{Mat4, Vec4};

/// The same source-to-display axes are used by the mesh and navigation gizmo.
pub fn display_rotation(z_up: bool) -> Mat4 {
    if z_up {
        // Source +Z becomes display +Y; source +Y becomes display -Z.
        Mat4::from_cols(Vec4::X, -Vec4::Z, Vec4::Y, Vec4::W)
    } else {
        Mat4::IDENTITY
    }
}
