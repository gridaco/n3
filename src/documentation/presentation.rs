//! Static presentation around generated guide media. This changes only the
//! saved pixels; egui layout, pointer coordinates, and the application render
//! keep using the inner capture size.

use super::CapturedFrame;
use crate::doc_animation::AnimationSettings;

pub(crate) const TEMPLATE_NAME: &str = "macos27-light-v3";

// Apple confirms a tighter, consistent macOS 27 window radius but does not
// publish numeric dimensions. These guide-pixel values are inferred from the
// user's 2x macOS 27 reference: ~30 px corner diameter and 28 px controls,
// centered ~47 px apart. Title height is an approximation because AppKit's
// actual title/toolbar height varies by window configuration.
pub(super) const SIDE: u32 = 0;
pub(super) const TOP: u32 = 0;
pub(super) const TITLE: u32 = 42;
const SHADOW_SIDE: u32 = 48;
const SHADOW_TOP: u32 = 36;
const SHADOW_BOTTOM: u32 = 60;
pub(super) const CORNER_RADIUS: f32 = 15.0;
const BUTTON_CENTERS: [u32; 3] = [21, 45, 68];
const BUTTON_RADIUS: f32 = 7.0;

const BODY: [u8; 4] = [255, 255, 255, 255];
const TITLEBAR: [u8; 4] = [250, 250, 251, 255];
const BORDER: [u8; 4] = [204, 206, 211, 255];
const DIVIDER: [u8; 4] = [228, 229, 232, 255];
const BUTTONS: [[u8; 4]; 3] = [[241, 99, 97, 255], [246, 193, 61, 255], [54, 201, 88, 255]];

/// Reused for every still and clip sample in one documentation capture. The
/// default static chrome has no shadow or outside margin. Optional shadowed
/// output is retained for future presentation needs, not the canonical guide.
pub(crate) struct WindowFrameTemplate {
    inner_width: u32,
    inner_height: u32,
    width: u32,
    height: u32,
    side: u32,
    top: u32,
    rgba: Vec<u8>,
}

impl WindowFrameTemplate {
    pub(crate) fn new(inner_width: u32, inner_height: u32) -> Result<Self, String> {
        Self::new_with_shadow(inner_width, inner_height, false)
    }

    pub(crate) fn new_with_shadow(
        inner_width: u32,
        inner_height: u32,
        shadow: bool,
    ) -> Result<Self, String> {
        if inner_width == 0 || inner_height == 0 {
            return Err("Window-frame content dimensions must be positive".into());
        }
        let (side, top, bottom) = if shadow {
            (SHADOW_SIDE, SHADOW_TOP, SHADOW_BOTTOM)
        } else {
            (SIDE, TOP, 0)
        };
        let width = inner_width
            .checked_add(side * 2)
            .ok_or("Window-frame width overflows")?;
        let height = inner_height
            .checked_add(top + TITLE + bottom)
            .ok_or("Window-frame height overflows")?;
        let byte_count = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or("Window-frame byte count overflows")?;
        if byte_count > AnimationSettings::default().max_frame_bytes {
            return Err("Window-frame canvas exceeds the animation RGBA frame-byte limit".into());
        }

        let mut rgba = vec![0; byte_count];
        let window = (
            side as f32,
            top as f32,
            (side + inner_width) as f32,
            (top + TITLE + inner_height) as f32,
        );
        for y in 0..height {
            for x in 0..width {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let index = ((y * width + x) * 4) as usize;
                let pixel = &mut rgba[index..index + 4];

                if shadow {
                    // Broad cast shadow plus a restrained contact shadow.
                    let shadow_distance = rounded_rect_distance(
                        px,
                        py,
                        (window.0, window.1 + 9.0, window.2, window.3 + 9.0),
                        CORNER_RADIUS,
                    )
                    .max(0.0);
                    let ambient = 0.18 * (-shadow_distance.powi(2) / (2.0 * 17.0f32.powi(2))).exp();
                    let contact = 0.11 * (-shadow_distance.powi(2) / (2.0 * 5.0f32.powi(2))).exp();
                    pixel.copy_from_slice(&[25, 29, 38, ((ambient + contact) * 255.0) as u8]);
                }

                let distance = rounded_rect_distance(px, py, window, CORNER_RADIUS);
                let coverage = (0.5 - distance).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let color = if distance > -1.0 {
                        BORDER
                    } else if py < window.1 + TITLE as f32 - 1.0 {
                        TITLEBAR
                    } else if py < window.1 + TITLE as f32 {
                        DIVIDER
                    } else {
                        BODY
                    };
                    blend_over(pixel, color, coverage);
                }
            }
        }
        if inner_width >= 96 {
            let cy = top as f32 + TITLE as f32 / 2.0;
            for (cx, color) in BUTTON_CENTERS.into_iter().zip(BUTTONS) {
                paint_traffic_light(&mut rgba, width, side + cx, cy, color);
            }
        }
        Ok(Self {
            inner_width,
            inner_height,
            width,
            height,
            side,
            top,
            rgba,
        })
    }

    pub(crate) fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn compose(&self, frame: CapturedFrame) -> Result<CapturedFrame, String> {
        let row_bytes = usize::try_from(self.inner_width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or("Window-frame content row overflows")?;
        let content_bytes = row_bytes
            .checked_mul(self.inner_height as usize)
            .ok_or("Window-frame content size overflows")?;
        if frame.width != self.inner_width
            || frame.height != self.inner_height
            || frame.rgba.len() != content_bytes
        {
            return Err("Window-frame content dimensions or RGBA length do not match".into());
        }

        let mut rgba = self.rgba.clone();
        let stride = self.width as usize * 4;
        let x_offset = self.side as usize * 4;
        let y_offset = (self.top + TITLE) as usize;
        let corner_rows = CORNER_RADIUS.ceil() as u32;
        for (y, source_row) in frame.rgba.chunks_exact(row_bytes).enumerate() {
            let target = (y_offset + y) * stride + x_offset;
            rgba[target..target + row_bytes].copy_from_slice(source_row);
            if (y as u32) < self.inner_height.saturating_sub(corner_rows) {
                continue;
            }
            for x in 0..self.inner_width.min(corner_rows) {
                for corner_x in [x, self.inner_width - 1 - x] {
                    let target_x = self.side + corner_x;
                    let target_y = self.top + TITLE + y as u32;
                    let coverage = (0.5
                        - rounded_rect_distance(
                            target_x as f32 + 0.5,
                            target_y as f32 + 0.5,
                            (
                                self.side as f32,
                                self.top as f32,
                                (self.side + self.inner_width) as f32,
                                (self.top + TITLE + self.inner_height) as f32,
                            ),
                            CORNER_RADIUS,
                        ))
                    .clamp(0.0, 1.0);
                    let source = corner_x as usize * 4;
                    let target_index = target + source;
                    rgba[target_index..target_index + 4]
                        .copy_from_slice(&self.rgba[target_index..target_index + 4]);
                    blend_over(
                        &mut rgba[target_index..target_index + 4],
                        source_row[source..source + 4].try_into().unwrap(),
                        coverage,
                    );
                }
            }
        }
        Ok(CapturedFrame {
            width: self.width,
            height: self.height,
            rgba,
        })
    }
}

fn rounded_rect_distance(x: f32, y: f32, rect: (f32, f32, f32, f32), radius: f32) -> f32 {
    let (left, top, right, bottom) = rect;
    let radius = radius.min((right - left) / 2.0).min((bottom - top) / 2.0);
    let center_x = (left + right) / 2.0;
    let center_y = (top + bottom) / 2.0;
    let dx = (x - center_x).abs() - (right - left) / 2.0 + radius;
    let dy = (y - center_y).abs() - (bottom - top) / 2.0 + radius;
    dx.max(0.0).hypot(dy.max(0.0)) + dx.max(dy).min(0.0) - radius
}

fn blend_over(dst: &mut [u8], src: [u8; 4], coverage: f32) {
    let source_alpha = (src[3] as f32 / 255.0) * coverage;
    let dest_alpha = dst[3] as f32 / 255.0;
    let alpha = source_alpha + dest_alpha * (1.0 - source_alpha);
    if alpha <= 0.0 {
        return;
    }
    for channel in 0..3 {
        dst[channel] = ((src[channel] as f32 * source_alpha
            + dst[channel] as f32 * dest_alpha * (1.0 - source_alpha))
            / alpha)
            .round() as u8;
    }
    dst[3] = (alpha * 255.0).round() as u8;
}

fn paint_traffic_light(rgba: &mut [u8], width: u32, cx: u32, cy: f32, color: [u8; 4]) {
    // The macOS 27 glass material and its changing highlights are deliberately
    // deferred. The static circle only conveys the reference's size and place.
    for y in (cy as u32 - 8)..=(cy as u32 + 8) {
        for x in cx - 8..=cx + 8 {
            let dx = x as f32 + 0.5 - cx as f32;
            let dy = y as f32 + 0.5 - cy;
            let distance = dx.hypot(dy);
            let coverage = (BUTTON_RADIUS + 0.5 - distance).clamp(0.0, 1.0);
            if coverage > 0.0 {
                let index = ((y * width + x) * 4) as usize;
                blend_over(&mut rgba[index..index + 4], color, coverage);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(frame: &CapturedFrame, x: u32, y: u32) -> &[u8] {
        let start = ((y * frame.width + x) * 4) as usize;
        &frame.rgba[start..start + 4]
    }

    #[test]
    fn frame_preserves_content_and_static_chrome_except_rounded_corners() {
        let template = WindowFrameTemplate::new(96, 40).unwrap();
        assert_eq!(template.dimensions(), (96, 82));
        let content = [17, 42, 91, 255].repeat(96 * 40);
        let first = template
            .compose(CapturedFrame {
                width: 96,
                height: 40,
                rgba: content.clone(),
            })
            .unwrap();
        let second = template
            .compose(CapturedFrame {
                width: 96,
                height: 40,
                rgba: vec![119; content.len()],
            })
            .unwrap();
        assert_eq!(
            pixel(&first, SIDE + 48, TOP + TITLE + 20),
            &[17, 42, 91, 255]
        );
        assert_eq!(
            pixel(&first, SIDE, TOP + TITLE + 39),
            pixel(&second, SIDE, TOP + TITLE + 39)
        );
        assert_ne!(
            pixel(&first, SIDE + 14, TOP + TITLE + 39),
            pixel(&second, SIDE + 14, TOP + TITLE + 39)
        );
        assert_eq!(pixel(&first, 0, 0)[3], 0);
        assert_eq!(pixel(&first, SIDE, TOP + TITLE + 20)[3], 255);
        assert!(pixel(&first, SIDE + BUTTON_CENTERS[0], TOP + TITLE / 2)[0] > 180);
    }

    #[test]
    fn shadow_is_optional_and_only_shadowed_output_has_margins() {
        let plain = WindowFrameTemplate::new(96, 40).unwrap();
        let shadowed = WindowFrameTemplate::new_with_shadow(96, 40, true).unwrap();
        assert_eq!(plain.dimensions(), (96, 82));
        assert_eq!(shadowed.dimensions(), (192, 178));
        let content = CapturedFrame {
            width: 96,
            height: 40,
            rgba: [17, 42, 91, 255].repeat(96 * 40),
        };
        let plain = plain
            .compose(CapturedFrame {
                width: content.width,
                height: content.height,
                rgba: content.rgba.clone(),
            })
            .unwrap();
        let shadowed = shadowed.compose(content).unwrap();
        assert_eq!(pixel(&plain, 0, 0)[3], 0);
        assert!(pixel(&shadowed, SHADOW_SIDE - 6, SHADOW_TOP + TITLE + 20)[3] > 0);
        assert_eq!(
            pixel(&plain, 48, TITLE + 20),
            pixel(&shadowed, SHADOW_SIDE + 48, SHADOW_TOP + TITLE + 20)
        );
    }

    #[test]
    fn frame_rejects_mismatched_and_overflowing_content() {
        let template = WindowFrameTemplate::new(96, 4).unwrap();
        assert!(
            template
                .compose(CapturedFrame {
                    width: 95,
                    height: 4,
                    rgba: vec![0; 96 * 4 * 4]
                })
                .is_err()
        );
        assert!(
            template
                .compose(CapturedFrame {
                    width: 96,
                    height: 4,
                    rgba: vec![0; 96 * 4 * 4 - 1]
                })
                .is_err()
        );
        assert!(WindowFrameTemplate::new(0, 4).is_err());
        assert!(WindowFrameTemplate::new(u32::MAX, 4).is_err());
    }
}
