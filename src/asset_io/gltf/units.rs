//! glTF meters become canonical centimeters exactly once at import.
use super::*;
pub(super) fn position_cm(value: [f32; 3]) -> [f64; 3] {
    value.map(|v| f64::from(v) * 100.)
}
pub(super) fn affine_cm(mut matrix: DMat4) -> DMat4 {
    // C * M * C^-1 for a uniform unit conversion changes translation only.
    // This applies equally to node and inverse-bind matrices; never scale bases.
    matrix.w_axis.x *= 100.;
    matrix.w_axis.y *= 100.;
    matrix.w_axis.z *= 100.;
    matrix
}
pub(super) fn length_cm(meters: f32) -> Result<f32> {
    let centimeters = meters * 100.;
    if !centimeters.is_finite() {
        return Err("Camera or light length exceeds the finite centimeter range.".into());
    }
    Ok(centimeters)
}
