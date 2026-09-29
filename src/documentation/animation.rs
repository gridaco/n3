//! Bounded, deterministic animated WebP for headless documentation captures.
//!
//! The existing image codec compresses each RGBA frame losslessly; this module
//! only multiplexes its VP8L chunks into the standard animation container:
//! <https://developers.google.com/speed/webp/docs/riff_container>.
//! After the first full canvas, only the exact changed pixel rectangle is stored;
//! identical samples extend the prior hold. Rectangles replace pixels without
//! blending or disposal, including transparent pixels and their RGB values.
//! Timing is authored display time, never wall-clock time.

use std::{io::Write, time::Duration};

use crate::doc_capture::CapturedFrame;

const HEADER_BYTES: usize = 44;
const MAX_FRAME_MS: u128 = 0x00ff_ffff;
const MIN_FRAME_MS: u128 = 11;
const MAX_DIMENSION: u32 = 16384;
const MAX_FILE_BYTES: u64 = u32::MAX as u64 - 1;

#[derive(Clone, Copy, Debug)]
pub struct AnimationSettings {
    /// Zero loops forever; a positive value is the total number of plays.
    pub loop_count: u16,
    /// Maximum authored samples, including samples coalesced into one frame.
    pub max_frames: usize,
    pub max_duration: Duration,
    pub max_encoded_bytes: usize,
    pub max_frame_bytes: usize,
}

impl Default for AnimationSettings {
    fn default() -> Self {
        Self {
            loop_count: 0,
            max_frames: 600,
            max_duration: Duration::from_secs(60),
            max_encoded_bytes: 128 * 1024 * 1024,
            max_frame_bytes: 64 * 1024 * 1024,
        }
    }
}

/// A bounded clip retaining exactly one prior RGBA canvas for lossless delta
/// comparison. A rejected push leaves pixels, holds, counters and output bytes
/// unchanged. No historical raw frames are retained.
pub struct WebpAnimation {
    width: u32,
    height: u32,
    frame_bytes: usize,
    settings: AnimationSettings,
    bytes: Vec<u8>,
    previous: Vec<u8>,
    sample_count: usize,
    frame_count: usize,
    last_hold: Option<(usize, u32)>,
    duration: Duration,
}

impl WebpAnimation {
    pub fn new(width: u32, height: u32, settings: AnimationSettings) -> Result<Self, String> {
        if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
            return Err("Animation canvas dimensions must be between 1 and 16384 pixels.".into());
        }
        if settings.max_frames == 0
            || settings.max_duration < Duration::from_millis(MIN_FRAME_MS as u64)
        {
            return Err(
                "Animation limits must allow at least one frame lasting 11 milliseconds.".into(),
            );
        }
        if settings.max_encoded_bytes < HEADER_BYTES
            || settings.max_encoded_bytes as u64 > MAX_FILE_BYTES
        {
            return Err("Animation encoded-byte limit must fit a WebP container.".into());
        }
        let frame_bytes = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .and_then(|bytes| usize::try_from(bytes).ok())
            .ok_or("Animation RGBA dimensions overflow the host address space.")?;
        if frame_bytes > settings.max_frame_bytes {
            return Err("Animation canvas exceeds the configured RGBA frame-byte limit.".into());
        }
        let mut bytes = b"RIFF\0\0\0\0WEBP".to_vec();
        let mut extended = vec![0x12, 0, 0, 0]; // animation + RGBA alpha information
        put_u24(&mut extended, width - 1);
        put_u24(&mut extended, height - 1);
        append_chunk(&mut bytes, b"VP8X", &extended)?;
        let mut animation = vec![0, 0, 0, 0]; // transparent BGRA background
        animation.extend_from_slice(&settings.loop_count.to_le_bytes());
        append_chunk(&mut bytes, b"ANIM", &animation)?;
        debug_assert_eq!(bytes.len(), HEADER_BYTES);
        Ok(Self {
            width,
            height,
            frame_bytes,
            settings,
            bytes,
            previous: Vec::new(),
            sample_count: 0,
            frame_count: 0,
            last_hold: None,
            duration: Duration::ZERO,
        })
    }

    /// `duration` is the time this frame remains displayed before the next one.
    /// Require whole milliseconds and avoid 0–10 ms delays, whose playback rate
    /// is implementation-defined in common WebP viewers. Never round silently.
    /// Identical samples coalesce when their combined hold fits WebP's 24 bits;
    /// otherwise a one-pixel unchanged frame carries the entire next hold.
    pub fn push(&mut self, frame: &CapturedFrame, duration: Duration) -> Result<(), String> {
        if (frame.width, frame.height) != (self.width, self.height)
            || frame.rgba.len() != self.frame_bytes
        {
            return Err("Every animation frame must match its canvas and contain exactly width × height × 4 RGBA bytes.".into());
        }
        let milliseconds = duration.as_millis();
        if !duration.subsec_nanos().is_multiple_of(1_000_000)
            || !(MIN_FRAME_MS..=MAX_FRAME_MS).contains(&milliseconds)
        {
            return Err(
                "Animation frame duration must be an integer from 11 to 16777215 milliseconds."
                    .into(),
            );
        }
        if self.sample_count >= self.settings.max_frames {
            return Err("Animation exceeds its configured sample-count limit.".into());
        }
        let total_duration = self
            .duration
            .checked_add(duration)
            .ok_or("Animation duration overflows.")?;
        if total_duration > self.settings.max_duration {
            return Err("Animation exceeds its configured duration limit.".into());
        }
        let milliseconds = milliseconds as u32;
        let changed = if self.previous.is_empty() {
            Some(PixelRect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            })
        } else {
            changed_rect(&self.previous, &frame.rgba, self.width, self.height)
        };
        if changed.is_none()
            && let Some((offset, hold)) = self.last_hold
            && u128::from(hold) + u128::from(milliseconds) <= MAX_FRAME_MS
        {
            let hold = hold + milliseconds;
            self.bytes[offset..offset + 3].copy_from_slice(&hold.to_le_bytes()[..3]);
            self.last_hold = Some((offset, hold));
            self.sample_count += 1;
            self.duration = total_duration;
            return Ok(());
        }
        // If an identical hold overflowed, replacing the unchanged top-left
        // pixel preserves the canvas without storing another complete image.
        let rect = changed.unwrap_or(PixelRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        });
        let cropped;
        let rgba = if rect.width == self.width && rect.height == self.height {
            frame.rgba.as_slice()
        } else {
            cropped = crop(&frame.rgba, self.width, rect)?;
            cropped.as_slice()
        };
        // The static container's 12-byte RIFF header is replaced by a 24-byte
        // ANMF header. Limit the writer before compression appends output bytes.
        let available = self
            .settings
            .max_encoded_bytes
            .checked_sub(self.bytes.len())
            .and_then(|bytes| bytes.checked_sub(12))
            .ok_or("Animation exceeds its encoded-byte limit.")?;
        let mut encoded = LimitedBuffer {
            bytes: Vec::new(),
            limit: available,
        };
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .encode(
                rgba,
                rect.width,
                rect.height,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(|error| format!("Could not encode animation frame: {error}"))?;
        let lossless = lossless_chunk(&encoded.bytes, rect.width, rect.height)?;
        let mut payload = Vec::with_capacity(16 + lossless.len());
        put_u24(&mut payload, rect.x / 2);
        put_u24(&mut payload, rect.y / 2);
        put_u24(&mut payload, rect.width - 1);
        put_u24(&mut payload, rect.height - 1);
        put_u24(&mut payload, milliseconds);
        payload.push(0x02); // replace this rectangle; no alpha blend or disposal
        payload.extend_from_slice(lossless);
        let needed = 8_usize
            .checked_add(payload.len())
            .and_then(|size| size.checked_add(payload.len() & 1))
            .and_then(|size| self.bytes.len().checked_add(size))
            .ok_or("Animation encoded size overflows.")?;
        if needed > self.settings.max_encoded_bytes {
            return Err("Animation exceeds its encoded-byte limit.".into());
        }
        self.bytes
            .try_reserve(needed - self.bytes.len())
            .map_err(|error| format!("Could not reserve animation output: {error}"))?;
        if self.previous.is_empty() {
            self.previous
                .try_reserve_exact(self.frame_bytes)
                .map_err(|error| {
                    format!("Could not reserve the prior animation canvas: {error}")
                })?;
        }
        let hold_offset = self.bytes.len() + 8 + 12;
        append_chunk(&mut self.bytes, b"ANMF", &payload)?;
        self.previous.resize(self.frame_bytes, 0);
        self.previous.copy_from_slice(&frame.rgba);
        self.sample_count += 1;
        self.frame_count += 1;
        self.last_hold = Some((hold_offset, milliseconds));
        self.duration = total_duration;
        Ok(())
    }

    /// Authored samples accepted, including identical consecutive samples.
    pub fn sample_count(&self) -> usize {
        self.sample_count
    }
    /// Physical frames in the encoded animation after hold coalescing.
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }
    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn finish(mut self) -> Result<Vec<u8>, String> {
        if self.frame_count == 0 {
            return Err("An animated capture needs at least one frame.".into());
        }
        let riff_size = u32::try_from(self.bytes.len() - 8)
            .map_err(|_| "Animation exceeds the RIFF container size.")?;
        self.bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
        Ok(self.bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PixelRect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

fn changed_rect(previous: &[u8], next: &[u8], width: u32, height: u32) -> Option<PixelRect> {
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (index, (a, b)) in previous
        .as_chunks::<4>()
        .0
        .iter()
        .zip(next.as_chunks::<4>().0.iter())
        .enumerate()
    {
        if a != b {
            let x = (index % width as usize) as u32;
            let y = (index / width as usize) as u32;
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if right == 0 {
        return None;
    }
    // ANMF stores x/2 and y/2. Include the preceding unchanged row/column
    // when needed; dimensions themselves need not be even.
    left &= !1;
    top &= !1;
    Some(PixelRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn crop(rgba: &[u8], canvas_width: u32, rect: PixelRect) -> Result<Vec<u8>, String> {
    let row_bytes = rect.width as usize * 4;
    let stride = canvas_width as usize * 4;
    let mut cropped = Vec::new();
    cropped
        .try_reserve_exact(row_bytes * rect.height as usize)
        .map_err(|error| format!("Could not reserve animation delta pixels: {error}"))?;
    for y in rect.y..rect.y + rect.height {
        let start = y as usize * stride + rect.x as usize * 4;
        cropped.extend_from_slice(&rgba[start..start + row_bytes]);
    }
    Ok(cropped)
}

fn put_u24(bytes: &mut Vec<u8>, value: u32) {
    debug_assert!(value <= 0x00ff_ffff);
    bytes.extend_from_slice(&value.to_le_bytes()[..3]);
}

fn append_chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) -> Result<(), String> {
    let size = u32::try_from(payload.len()).map_err(|_| "WebP chunk is too large.")?;
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(payload);
    if size & 1 != 0 {
        bytes.push(0);
    }
    Ok(())
}

/// Accept only the codec's simple lossless output. A future codec change must
/// be handled explicitly rather than silently embedding metadata or lossy data
/// inside an ANMF chunk. The returned chunk retains its RIFF padding byte.
fn lossless_chunk(encoded: &[u8], width: u32, height: u32) -> Result<&[u8], String> {
    let invalid = || "The frame encoder returned an unsupported WebP container.".to_owned();
    if encoded.len() < 25
        || &encoded[..4] != b"RIFF"
        || &encoded[8..12] != b"WEBP"
        || &encoded[12..16] != b"VP8L"
    {
        return Err(invalid());
    }
    let file_size = u32::from_le_bytes(encoded[4..8].try_into().unwrap()) as u64 + 8;
    let chunk_size = u32::from_le_bytes(encoded[16..20].try_into().unwrap()) as u64;
    if file_size != encoded.len() as u64
        || chunk_size < 5
        || 20 + chunk_size + (chunk_size & 1) != file_size
        || encoded[20] != 0x2f
    {
        return Err(invalid());
    }
    if chunk_size & 1 != 0 && encoded.last() != Some(&0) {
        return Err(invalid());
    }
    let header = u32::from_le_bytes(encoded[21..25].try_into().unwrap());
    let actual = ((header & 0x3fff) + 1, ((header >> 14) & 0x3fff) + 1);
    if actual != (width, height) || header >> 29 != 0 {
        return Err(invalid());
    }
    Ok(&encoded[12..])
}

struct LimitedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for LimitedBuffer {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        if data.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "Animation exceeds its encoded-byte limit.",
            ));
        }
        self.bytes
            .try_reserve(data.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{AnimationDecoder, ImageDecoder};
    use std::io::Cursor;

    fn frame(rgba: &[u8]) -> CapturedFrame {
        CapturedFrame {
            width: 2,
            height: 1,
            rgba: rgba.to_vec(),
        }
    }
    fn opaque() -> CapturedFrame {
        frame(&[255, 0, 0, 255, 0, 0, 255, 255])
    }
    fn decode(bytes: &[u8]) -> image::codecs::webp::WebPDecoder<Cursor<&[u8]>> {
        image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).unwrap()
    }

    fn rectangles(bytes: &[u8]) -> Vec<PixelRect> {
        fn u24(bytes: &[u8]) -> u32 {
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], 0])
        }
        let mut cursor = HEADER_BYTES;
        let mut rectangles = Vec::new();
        while cursor < bytes.len() {
            assert_eq!(&bytes[cursor..cursor + 4], b"ANMF");
            let length =
                u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
            let payload = &bytes[cursor + 8..cursor + 8 + length];
            assert_eq!(
                payload[15], 2,
                "Deltas replace pixels without disposal or blending"
            );
            rectangles.push(PixelRect {
                x: u24(&payload[..3]) * 2,
                y: u24(&payload[3..6]) * 2,
                width: u24(&payload[6..9]) + 1,
                height: u24(&payload[9..12]) + 1,
            });
            cursor += 8 + length + (length & 1);
        }
        assert_eq!(cursor, bytes.len());
        rectangles
    }

    fn assert_timeline(bytes: &[u8], authored: &[(CapturedFrame, u32)]) {
        let decoded = decode(bytes).into_frames().collect_frames().unwrap();
        let mut cumulative = 0_u64;
        let actual = decoded
            .iter()
            .map(|frame| {
                let start = cumulative;
                let (duration, divisor) = frame.delay().numer_denom_ms();
                assert_eq!(divisor, 1);
                cumulative += u64::from(duration);
                (start, cumulative, frame.buffer().as_raw())
            })
            .collect::<Vec<_>>();
        let mut start = 0_u64;
        for (frame, duration) in authored {
            let end = start + u64::from(*duration);
            for timestamp in [start, end - 1] {
                let (_, _, pixels) = actual
                    .iter()
                    .find(|(a, b, _)| *a <= timestamp && timestamp < *b)
                    .unwrap();
                assert_eq!(
                    *pixels, &frame.rgba,
                    "Composited pixels differ at {timestamp} ms"
                );
            }
            start = end;
        }
        assert_eq!(
            cumulative, start,
            "Optimization must preserve the complete authored timeline"
        );
    }

    fn copy_frame(frame: &CapturedFrame) -> CapturedFrame {
        CapturedFrame {
            width: frame.width,
            height: frame.height,
            rgba: frame.rgba.clone(),
        }
    }

    fn set_pixel(frame: &mut CapturedFrame, x: u32, y: u32, rgba: [u8; 4]) {
        let start = ((y * frame.width + x) * 4) as usize;
        frame.rgba[start..start + 4].copy_from_slice(&rgba);
    }

    #[test]
    fn cropped_deltas_preserve_shifted_odd_rectangles_alpha_erasure_and_invisible_rgb() {
        let mut frame = CapturedFrame {
            width: 8,
            height: 6,
            rgba: [10, 20, 30, 255].repeat(48),
        };
        set_pixel(&mut frame, 0, 0, [90, 80, 70, 0]);
        let mut authored = vec![(copy_frame(&frame), 50)];
        // A single odd-origin pixel needs the preceding unchanged row/column.
        set_pixel(&mut frame, 3, 3, [200, 100, 50, 127]);
        authored.push((copy_frame(&frame), 33));
        authored.push((copy_frame(&frame), 17));
        // Alpha erases an opaque pixel; no blending may retain its prior color.
        set_pixel(&mut frame, 6, 1, [17, 9, 3, 0]);
        authored.push((copy_frame(&frame), 50));
        set_pixel(&mut frame, 7, 5, [255, 200, 100, 255]);
        authored.push((copy_frame(&frame), 50));
        // RGB changes under alpha=0 are still lossless changes, not discarded.
        set_pixel(&mut frame, 6, 1, [42, 43, 44, 0]);
        authored.push((copy_frame(&frame), 50));
        set_pixel(&mut frame, 3, 3, [10, 20, 30, 255]);
        authored.push((copy_frame(&frame), 50));
        let mut clip = WebpAnimation::new(8, 6, AnimationSettings::default()).unwrap();
        for (frame, ms) in &authored {
            clip.push(frame, Duration::from_millis((*ms).into()))
                .unwrap();
        }
        assert_eq!(clip.sample_count(), 7);
        assert_eq!(clip.frame_count(), 6);
        assert_eq!(clip.previous.len(), 8 * 6 * 4);
        let bytes = clip.finish().unwrap();
        assert_eq!(
            rectangles(&bytes),
            vec![
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 6
                },
                PixelRect {
                    x: 2,
                    y: 2,
                    width: 2,
                    height: 2
                },
                PixelRect {
                    x: 6,
                    y: 0,
                    width: 1,
                    height: 2
                },
                PixelRect {
                    x: 6,
                    y: 4,
                    width: 2,
                    height: 2
                },
                PixelRect {
                    x: 6,
                    y: 0,
                    width: 1,
                    height: 2
                },
                PixelRect {
                    x: 2,
                    y: 2,
                    width: 2,
                    height: 2
                },
            ]
        );
        assert_timeline(&bytes, &authored);
    }

    #[test]
    fn identical_hold_overflow_adds_a_minimal_frame_without_truncation() {
        let maximum = MAX_FRAME_MS as u32;
        let mut clip = WebpAnimation::new(
            2,
            1,
            AnimationSettings {
                max_duration: Duration::from_millis(u64::from(maximum) + 22),
                ..Default::default()
            },
        )
        .unwrap();
        let authored = [maximum - 11, 11, 11, 11]
            .into_iter()
            .map(|ms| (opaque(), ms))
            .collect::<Vec<_>>();
        for (frame, ms) in &authored {
            clip.push(frame, Duration::from_millis((*ms).into()))
                .unwrap();
        }
        assert_eq!(clip.sample_count(), 4);
        assert_eq!(clip.frame_count(), 2);
        let before = clip.bytes.clone();
        assert!(clip.push(&opaque(), Duration::from_millis(11)).is_err());
        assert_eq!(clip.bytes, before);
        let bytes = clip.finish().unwrap();
        assert_eq!(
            rectangles(&bytes),
            vec![
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 2,
                    height: 1
                },
                PixelRect {
                    x: 0,
                    y: 0,
                    width: 1,
                    height: 1
                },
            ]
        );
        let decoded = decode(&bytes).into_frames().collect_frames().unwrap();
        assert_eq!(decoded[0].delay().numer_denom_ms(), (maximum, 1));
        assert_eq!(decoded[1].delay().numer_denom_ms(), (22, 1));
        assert_timeline(&bytes, &authored);
    }

    #[test]
    fn independent_decoder_preserves_frame_order_rgba_alpha_and_authored_delays() {
        let frames = [
            opaque(),
            frame(&[17, 99, 203, 0, 0, 255, 0, 127]),
            frame(&[5, 6, 7, 255, 200, 190, 180, 255]),
        ];
        let mut clip = WebpAnimation::new(
            2,
            1,
            AnimationSettings {
                loop_count: 3,
                ..Default::default()
            },
        )
        .unwrap();
        for (frame, ms) in frames.iter().zip([33, 34, 333]) {
            clip.push(frame, Duration::from_millis(ms)).unwrap();
        }
        assert_eq!(clip.frame_count(), 3);
        assert_eq!(clip.sample_count(), 3);
        assert_eq!(clip.duration(), Duration::from_millis(400));
        let bytes = clip.finish().unwrap();
        let decoder = decode(&bytes);
        assert_eq!(decoder.dimensions(), (2, 1));
        assert!(
            matches!(decoder.loop_count(), image::metadata::LoopCount::Finite(count) if count.get() == 3)
        );
        let decoded = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(decoded.len(), 3);
        for ((decoded, original), ms) in decoded.iter().zip(&frames).zip([33, 34, 333]) {
            assert_eq!(decoded.delay().numer_denom_ms(), (ms, 1));
            assert_eq!(
                decoded.buffer().as_raw(),
                &original.rgba,
                "Full-frame replace must not blend against prior pixels"
            );
        }
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
            bytes.len()
        );
    }

    #[test]
    fn repeated_frames_coalesce_without_changing_authored_timing_or_loop_count() {
        let mut clip = WebpAnimation::new(2, 1, AnimationSettings::default()).unwrap();
        for ms in [50, 50, 500] {
            clip.push(&opaque(), Duration::from_millis(ms)).unwrap();
        }
        assert_eq!(clip.sample_count(), 3);
        assert_eq!(clip.frame_count(), 1);
        assert_eq!(clip.duration(), Duration::from_millis(600));
        let bytes = clip.finish().unwrap();
        let decoder = decode(&bytes);
        assert!(matches!(
            decoder.loop_count(),
            image::metadata::LoopCount::Infinite
        ));
        let decoded = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(
            decoded
                .iter()
                .map(|frame| frame.delay().numer_denom_ms())
                .collect::<Vec<_>>(),
            vec![(600, 1)]
        );
        assert_eq!(decoded[0].buffer().as_raw(), &opaque().rgba);
    }

    #[test]
    fn full_duration_and_loop_fields_roundtrip_without_truncation() {
        let mut clip = WebpAnimation::new(
            2,
            1,
            AnimationSettings {
                loop_count: u16::MAX,
                max_duration: Duration::from_millis(MAX_FRAME_MS as u64),
                ..Default::default()
            },
        )
        .unwrap();
        clip.push(&opaque(), Duration::from_millis(MAX_FRAME_MS as u64))
            .unwrap();
        let bytes = clip.finish().unwrap();
        let decoder = decode(&bytes);
        assert!(
            matches!(decoder.loop_count(), image::metadata::LoopCount::Finite(count) if count.get() == u32::from(u16::MAX))
        );
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert_eq!(frames[0].delay().numer_denom_ms(), (MAX_FRAME_MS as u32, 1));
        for settings in [
            AnimationSettings {
                max_frames: 0,
                ..Default::default()
            },
            AnimationSettings {
                max_duration: Duration::from_millis(10),
                ..Default::default()
            },
            AnimationSettings {
                max_encoded_bytes: HEADER_BYTES - 1,
                ..Default::default()
            },
            AnimationSettings {
                max_frame_bytes: 7,
                ..Default::default()
            },
        ] {
            assert!(WebpAnimation::new(2, 1, settings).is_err());
        }
    }

    #[test]
    fn invalid_dimensions_buffers_and_timing_are_rejected_before_state_changes() {
        for (width, height) in [(0, 1), (1, 0), (16385, 1), (1, 16385), (16384, 16384)] {
            assert!(WebpAnimation::new(width, height, AnimationSettings::default()).is_err());
        }
        let mut clip = WebpAnimation::new(2, 1, AnimationSettings::default()).unwrap();
        assert!(
            clip.push(&frame(&[0; 7]), Duration::from_millis(50))
                .is_err()
        );
        let mut wrong = opaque();
        wrong.width = 1;
        assert!(clip.push(&wrong, Duration::from_millis(50)).is_err());
        for duration in [
            Duration::ZERO,
            Duration::from_millis(10),
            Duration::from_micros(11500),
            Duration::from_millis(MAX_FRAME_MS as u64 + 1),
        ] {
            assert!(clip.push(&opaque(), duration).is_err());
        }
        assert_eq!(clip.frame_count(), 0);
        assert_eq!(clip.duration(), Duration::ZERO);
        clip.push(&opaque(), Duration::from_millis(11)).unwrap();
        assert_eq!(
            decode(&clip.finish().unwrap())
                .into_frames()
                .collect_frames()
                .unwrap()
                .len(),
            1
        );
        assert!(
            WebpAnimation::new(2, 1, AnimationSettings::default())
                .unwrap()
                .finish()
                .is_err()
        );
    }

    #[test]
    fn frame_duration_and_encoded_byte_budgets_are_atomic() {
        for settings in [
            AnimationSettings {
                max_frames: 1,
                ..Default::default()
            },
            AnimationSettings {
                max_duration: Duration::from_millis(75),
                ..Default::default()
            },
        ] {
            let mut clip = WebpAnimation::new(2, 1, settings).unwrap();
            clip.push(&opaque(), Duration::from_millis(50)).unwrap();
            let before = clip.bytes.clone();
            assert!(clip.push(&opaque(), Duration::from_millis(50)).is_err());
            assert_eq!(clip.bytes, before);
            assert_eq!(clip.frame_count(), 1);
            assert_eq!(clip.duration(), Duration::from_millis(50));
        }
        let mut probe = WebpAnimation::new(2, 1, AnimationSettings::default()).unwrap();
        probe.push(&opaque(), Duration::from_millis(50)).unwrap();
        let one_frame_bytes = probe.finish().unwrap().len();
        let mut clip = WebpAnimation::new(
            2,
            1,
            AnimationSettings {
                max_encoded_bytes: one_frame_bytes,
                ..Default::default()
            },
        )
        .unwrap();
        clip.push(&opaque(), Duration::from_millis(50)).unwrap();
        // A repeated hold fits even when there is no room for another chunk.
        clip.push(&opaque(), Duration::from_millis(50)).unwrap();
        let before = clip.bytes.clone();
        let changed = frame(&[0, 255, 0, 0, 0, 0, 255, 255]);
        assert!(clip.push(&changed, Duration::from_millis(50)).is_err());
        assert_eq!(clip.bytes, before);
        assert_eq!(clip.frame_count(), 1);
        assert_eq!(clip.sample_count(), 2);
        assert_eq!(clip.duration(), Duration::from_millis(100));
        assert_eq!(clip.previous, opaque().rgba);
        // Retrying after a rejected delta still compares with the last accepted
        // canvas and preserves the coalesced hold exactly.
        clip.settings.max_encoded_bytes += 1024;
        clip.push(&changed, Duration::from_millis(50)).unwrap();
        let bytes = clip.finish().unwrap();
        let decoded = decode(&bytes).into_frames().collect_frames().unwrap();
        assert_eq!(decoded[0].delay().numer_denom_ms(), (100, 1));
        assert_eq!(decoded[1].buffer().as_raw(), &changed.rgba);
        let mut tiny = WebpAnimation::new(
            2,
            1,
            AnimationSettings {
                max_encoded_bytes: one_frame_bytes - 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(tiny.push(&opaque(), Duration::from_millis(50)).is_err());
        assert_eq!(tiny.frame_count(), 0);
    }

    #[test]
    fn malformed_static_codec_containers_are_rejected_and_chunks_are_padded() {
        let mut encoded = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut encoded)
            .encode(&opaque().rgba, 2, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        assert!(lossless_chunk(&encoded, 2, 1).is_ok());
        for length in 0..encoded.len() {
            assert!(lossless_chunk(&encoded[..length], 2, 1).is_err());
        }
        assert!(lossless_chunk(&encoded, 1, 2).is_err());
        for position in [0, 8, 12, 20] {
            let mut invalid = encoded.clone();
            invalid[position] ^= 0x20;
            assert!(lossless_chunk(&invalid, 2, 1).is_err());
        }
        let mut trailing = encoded.clone();
        trailing.push(0);
        assert!(lossless_chunk(&trailing, 2, 1).is_err());
        let mut padded = Vec::new();
        append_chunk(&mut padded, b"TEST", &[1, 2, 3]).unwrap();
        assert_eq!(&padded, b"TEST\x03\x00\x00\x00\x01\x02\x03\x00");
    }
}
