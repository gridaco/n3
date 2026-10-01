//! Static presentation around generated guide media. This changes only the
//! saved pixels; egui layout, pointer coordinates, and the application render
//! keep using the inner capture size.

use super::CapturedFrame;
use crate::doc_animation::AnimationSettings;

pub(crate) const TEMPLATE_NAME: &str = "macos27-themed-v5";

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

// Chrome follows the resolved capture appearance, never the host OS theme.
// Keep a restrained one-pixel outline, including around the content corners.
const BORDER_WIDTH: f32 = 1.0;
#[derive(Clone, Copy, PartialEq, Eq)]
struct FrameColors {
    body: [u8; 4],
    titlebar: [u8; 4],
    border: [u8; 4],
    divider: [u8; 4],
}
impl FrameColors {
    fn from_palette(palette: crate::theme::Palette) -> Self {
        // This is a role mapping, not another set of theme color definitions.
        Self {
            body: palette.background.to_srgba_unmultiplied(),
            titlebar: palette.titlebar.to_srgba_unmultiplied(),
            border: palette.border.to_srgba_unmultiplied(),
            divider: palette.sidebar_border.to_srgba_unmultiplied(),
        }
    }
}

// Only the clipped boundary needs per-pixel blending on each captured frame.
struct ContentEdge {
    source: usize,
    target: usize,
    coverage: f32,
}

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
    colors: FrameColors,
    content_edges: Vec<ContentEdge>,
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
        Self::with_colors(
            inner_width,
            inner_height,
            shadow,
            FrameColors::from_palette(crate::theme::Palette::new(
                crate::settings::ResolvedTheme::Light,
                crate::settings::AccentColor::DEFAULT,
            )),
        )
    }

    fn with_colors(
        inner_width: u32,
        inner_height: u32,
        shadow: bool,
        colors: FrameColors,
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
        let mut content_edges = Vec::new();
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
                    pixel.copy_from_slice(&[0, 0, 0, ((ambient + contact) * 255.0) as u8]);
                }

                let distance = rounded_rect_distance(px, py, window, CORNER_RADIUS);
                let coverage = (0.5 - distance).clamp(0.0, 1.0);
                let is_content = x >= side
                    && x < side + inner_width
                    && y >= top + TITLE
                    && y < top + TITLE + inner_height;
                let content_coverage = (0.5 - distance - BORDER_WIDTH).clamp(0.0, 1.0);
                if is_content && content_coverage < 1.0 {
                    content_edges.push(ContentEdge {
                        source: (((y - top - TITLE) * inner_width + x - side) * 4) as usize,
                        target: index,
                        coverage: content_coverage,
                    });
                }
                if coverage > 0.0 {
                    let color = if (is_content && content_coverage < 1.0) || distance > -1.0 {
                        colors.border
                    } else if py < window.1 + TITLE as f32 - 1.0 {
                        colors.titlebar
                    } else if py < window.1 + TITLE as f32 {
                        colors.divider
                    } else {
                        colors.body
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
            colors,
            content_edges,
        })
    }

    pub(crate) fn set_palette(&mut self, palette: crate::theme::Palette) -> Result<(), String> {
        let colors = FrameColors::from_palette(palette);
        if self.colors != colors {
            *self = Self::with_colors(
                self.inner_width,
                self.inner_height,
                self.side != SIDE,
                colors,
            )?;
        }
        Ok(())
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
        for (y, source_row) in frame.rgba.chunks_exact(row_bytes).enumerate() {
            let target = (y_offset + y) * stride + x_offset;
            rgba[target..target + row_bytes].copy_from_slice(source_row);
        }
        // Copy the interior unchanged, then restore only the cached outline and
        // corner mask. The capture must never overwrite the window's border.
        for edge in &self.content_edges {
            let pixel = &mut rgba[edge.target..edge.target + 4];
            pixel.copy_from_slice(&self.rgba[edge.target..edge.target + 4]);
            blend_over(
                pixel,
                frame.rgba[edge.source..edge.source + 4].try_into().unwrap(),
                edge.coverage,
            );
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

    fn palette(theme: egui::Theme) -> crate::theme::Palette {
        let theme = match theme {
            egui::Theme::Light => crate::settings::ResolvedTheme::Light,
            egui::Theme::Dark => crate::settings::ResolvedTheme::Dark,
        };
        crate::theme::Palette::new(theme, crate::settings::AccentColor::DEFAULT)
    }

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
            pixel(&first, SIDE + 48, TOP + TITLE + 38),
            pixel(&second, SIDE + 48, TOP + TITLE + 38)
        );
        assert_eq!(pixel(&first, 0, 0)[3], 0);
        assert_eq!(pixel(&first, SIDE, TOP + TITLE + 20)[3], 255);
        assert!(pixel(&first, SIDE + BUTTON_CENTERS[0], TOP + TITLE / 2)[0] > 180);
    }

    #[test]
    fn outline_survives_content_and_all_corners_share_the_same_silhouette() {
        let mut template = WindowFrameTemplate::new(96, 40).unwrap();
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            template.set_palette(palette(theme)).unwrap();
            let frame = template
                .compose(CapturedFrame {
                    width: 96,
                    height: 40,
                    rgba: [255, 0, 255, 255].repeat(96 * 40),
                })
                .unwrap();
            let palette = FrameColors::from_palette(palette(theme));
            for (x, y) in [(48, 0), (0, TITLE + 20), (95, TITLE + 20), (48, 81)] {
                assert_eq!(pixel(&frame, x, y), palette.border);
            }
            assert_eq!(pixel(&frame, 80, TITLE / 2), palette.titlebar);
            assert_eq!(pixel(&frame, 48, TITLE - 1), palette.divider);
            for y in 0..CORNER_RADIUS as u32 {
                for x in 0..CORNER_RADIUS as u32 {
                    let alpha = pixel(&frame, x, y)[3];
                    assert_eq!(alpha, pixel(&frame, 95 - x, y)[3]);
                    assert_eq!(alpha, pixel(&frame, x, 81 - y)[3]);
                    assert_eq!(alpha, pixel(&frame, 95 - x, 81 - y)[3]);
                }
            }
            assert_eq!(pixel(&frame, 1, TITLE + 20), [255, 0, 255, 255]);
            assert_eq!(pixel(&frame, 48, 80), [255, 0, 255, 255]);
        }
    }

    #[test]
    fn theme_switching_is_deterministic_and_keeps_the_cached_mask_between_samples() {
        let mut template = WindowFrameTemplate::new(96, 40).unwrap();
        let light = template.rgba.clone();
        let pointer = template.rgba.as_ptr();
        let mask_pointer = template.content_edges.as_ptr();
        template.set_palette(palette(egui::Theme::Light)).unwrap();
        assert_eq!(pointer, template.rgba.as_ptr());
        assert_eq!(mask_pointer, template.content_edges.as_ptr());
        template.set_palette(palette(egui::Theme::Dark)).unwrap();
        assert_ne!(light, template.rgba);
        template.set_palette(palette(egui::Theme::Light)).unwrap();
        assert_eq!(light, template.rgba);
        // Very narrow and short canvases must not duplicate or overrun mask pixels.
        let tiny = WindowFrameTemplate::new(1, 1).unwrap();
        assert_eq!(tiny.content_edges.len(), 1);
        assert!(
            tiny.compose(CapturedFrame {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            })
            .is_ok()
        );
    }

    #[test]
    fn shared_palette_updates_chrome_without_a_theme_switch() {
        let mut template = WindowFrameTemplate::new(96, 40).unwrap();
        let mut p = palette(egui::Theme::Light);
        // A primary accent change must not invalidate neutral frame chrome.
        let pointer = template.rgba.as_ptr();
        p.primary = egui::Color32::RED;
        template.set_palette(p).unwrap();
        assert_eq!(pointer, template.rgba.as_ptr());
        // Titlebar has its own role; future changes propagate without editing
        // this compositor or aliasing titlebar back to background.
        p.titlebar = egui::Color32::from_gray(120);
        p.border = egui::Color32::from_gray(90);
        p.sidebar_border = egui::Color32::from_gray(100);
        template.set_palette(p).unwrap();
        let frame = template
            .compose(CapturedFrame {
                width: 96,
                height: 40,
                rgba: [17, 42, 91, 255].repeat(96 * 40),
            })
            .unwrap();
        assert_eq!(
            pixel(&frame, 80, TITLE / 2),
            p.titlebar.to_srgba_unmultiplied()
        );
        assert_eq!(
            pixel(&frame, 0, TITLE + 20),
            p.border.to_srgba_unmultiplied()
        );
        assert_eq!(
            pixel(&frame, 48, TITLE - 1),
            p.sidebar_border.to_srgba_unmultiplied()
        );
        assert_eq!(pixel(&frame, 48, TITLE + 20), [17, 42, 91, 255]);
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
