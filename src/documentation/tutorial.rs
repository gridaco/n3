//! Explicit scenario time drives both the production UI and tutorial frames.
//! No sleeping, event synthesis from pictures, or separate animation renderer.
use super::{Result, Session, annotations, insert, validate_path};
use crate::{
    controls::Control,
    doc_animation::{AnimationSettings, WebpAnimation},
};
use std::time::Duration;

#[derive(Clone, Copy)]
pub struct ClipSpec {
    /// Maximum interval between samples, in integer milliseconds.
    pub sample_ms: u32,
    /// Zero loops forever (the WebP convention).
    pub loop_count: u16,
}

impl Default for ClipSpec {
    fn default() -> Self {
        Self {
            sample_ms: 50,
            loop_count: 0,
        }
    }
}

pub(super) struct Recorder {
    encoder: WebpAnimation,
    sample_ms: u32,
    evidence: Vec<String>,
    loop_count: u16,
}

impl Session<'_> {
    /// All input helpers keep using frame(). That function records the prior
    /// completed render for `elapsed`, then delivers the next input sample.
    /// Zero-time layout passes and event edges never add duplicate clip frames.
    pub(super) fn record_elapsed(&mut self, elapsed: Duration) -> Result<()> {
        if elapsed.is_zero() || self.recorder.is_none() {
            return Ok(());
        }
        let frame = self.capture.framed_frame()?;
        let recorder = self.recorder.as_mut().unwrap();
        if elapsed > Duration::from_millis(recorder.sample_ms.into()) {
            return Err("Clip time must be sampled; use wait() or move_pointer() instead of jumping the clock".into());
        }
        recorder.encoder.push(&frame, elapsed)?;
        recorder.evidence.push(format!(
            "frame @ {:.3}s + {}ms: {}",
            self.time,
            elapsed.as_millis(),
            self.input.summary(self.cursor)
        ));
        Ok(())
    }

    /// Capture a bounded, explicitly timed sequence as lossless animated WebP.
    /// Failed sequences publish nothing and never leak recording/presentation state.
    pub fn capture_clip(
        &mut self,
        name: &str,
        spec: ClipSpec,
        run: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        if self.recorder.is_some() {
            return Err("Tutorial clips cannot be nested".into());
        }
        if !(11..=1000).contains(&spec.sample_ms) {
            return Err("Clip sample interval must be 11..=1000 ms".into());
        }
        let path = format!("assets/{name}.webp");
        validate_path(&path)?;
        if self.images.contains_key(&path) {
            return Err(format!("Duplicate artifact owner: {path}"));
        }
        let (width, height) = self.capture.framed_dimensions();
        let encoder = WebpAnimation::new(
            width,
            height,
            AnimationSettings {
                loop_count: spec.loop_count,
                ..Default::default()
            },
        )?;
        let previous = self.show_inputs;
        let previous_annotations = self.annotations.clone();
        self.show_inputs = true;
        let result = (|| {
            self.settle()?;
            self.recorder = Some(Recorder {
                encoder,
                sample_ms: spec.sample_ms,
                evidence: Vec::new(),
                loop_count: spec.loop_count,
            });
            run(self)?;
            let recorder = self.recorder.take().unwrap();
            if recorder.encoder.sample_count() < 2 {
                return Err("An animation needs at least two sampled frames".into());
            }
            let summary = format!(
                "tutorial clip: {} samples, {} encoded frames, {}ms, sample <= {}ms, loops={}\n{}",
                recorder.encoder.sample_count(),
                recorder.encoder.frame_count(),
                recorder.encoder.duration().as_millis(),
                recorder.sample_ms,
                recorder.loop_count,
                recorder.evidence.join("\n")
            );
            let bytes = recorder.encoder.finish()?;
            insert(&mut self.images, path.clone(), bytes)?;
            self.image_inputs.insert(path.clone(), summary);
            self.animations.insert(path);
            Ok(())
        })();
        self.recorder = None;
        self.show_inputs = previous;
        self.annotations = previous_annotations;
        result
    }

    /// Wait in scenario time, sampling camera transitions and fading input cues.
    pub fn wait(&mut self, duration: Duration) -> Result<()> {
        for elapsed in samples(duration, self.sample_ms())? {
            self.frame(Vec::new(), elapsed)?;
        }
        Ok(())
    }

    pub fn move_pointer(&mut self, destination: egui::Pos2, duration: Duration) -> Result<()> {
        if !destination.is_finite() {
            return Err("Tutorial pointer coordinates must be finite".into());
        }
        let start = self
            .input
            .position()
            .ok_or("Position the pointer before animating its motion")?;
        let intervals = samples(duration, self.sample_ms())?;
        let mut elapsed_total = Duration::ZERO;
        for elapsed in intervals {
            elapsed_total += elapsed;
            let amount = (elapsed_total.as_secs_f64() / duration.as_secs_f64()) as f32;
            let point = start.lerp(destination, amount);
            self.frame(vec![egui::Event::PointerMoved(point)], elapsed)?;
        }
        Ok(())
    }

    /// Button edges stay separate from motion and time; a held drag is real input.
    pub fn pointer_button(&mut self, button: egui::PointerButton, pressed: bool) -> Result<()> {
        let pos = self
            .input
            .position()
            .ok_or("Position the pointer before pressing a button")?;
        self.frame(
            vec![egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers: self.input.modifiers(),
            }],
            Duration::ZERO,
        )?;
        self.settle()
    }

    pub fn callout(&mut self, target: Control, caption: &str) -> Result<()> {
        if caption.is_empty() || caption.chars().count() > 180 {
            return Err(
                "Tutorial callouts need a short nonempty caption (at most 180 characters)".into(),
            );
        }
        self.witness(target)?;
        self.annotations = vec![annotations::Annotation {
            target,
            caption: caption.to_owned(),
        }];
        self.settle()
    }

    pub fn clear_callout(&mut self) -> Result<()> {
        self.annotations.clear();
        self.settle()
    }

    pub(super) fn paint_annotations(&self, ctx: &egui::Context) -> Result<()> {
        if self.show_inputs {
            annotations::paint(
                ctx,
                self.state.viewport,
                &crate::controls::snapshot(ctx),
                &self.annotations,
            )?;
        }
        Ok(())
    }

    fn sample_ms(&self) -> u32 {
        self.recorder
            .as_ref()
            .map_or(50, |recorder| recorder.sample_ms)
    }
}

fn samples(duration: Duration, max_ms: u32) -> Result<Vec<Duration>> {
    let millis =
        u64::try_from(duration.as_millis()).map_err(|_| "Tutorial duration is too long")?;
    if duration != Duration::from_millis(millis) || !(11..=60_000).contains(&millis) {
        return Err("Timed tutorial steps require 11..=60000 integer milliseconds".into());
    }
    let count = millis.div_ceil(u64::from(max_ms));
    let step = millis / count;
    if step < 11 {
        return Err("Tutorial frame holds must be at least 11 ms".into());
    }
    Ok((0..count)
        .map(|index| Duration::from_millis(step + u64::from(index < millis % count)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc_capture::Capture;
    use image::AnimationDecoder;
    #[test]
    fn sample_clock_preserves_exact_duration_without_short_tail_frames() {
        let steps = samples(Duration::from_millis(333), 50).unwrap();
        assert_eq!(steps.iter().sum::<Duration>(), Duration::from_millis(333));
        assert!(
            steps
                .iter()
                .all(|step| (11..=50).contains(&step.as_millis()))
        );
        assert!(samples(Duration::from_nanos(1), 50).is_err());
        assert!(samples(Duration::from_millis(60_001), 50).is_err());
    }

    #[test]
    fn clips_replay_real_held_input_and_errors_do_not_publish_or_leak_recorders() {
        let mut capture =
            pollster::block_on(Capture::new(super::super::WIDTH, super::super::HEIGHT)).unwrap();
        let mut s = Session::new(&mut capture).unwrap();
        s.load_fixture("cube-quads.obj").unwrap();
        let start = s.state.viewport.center();
        s.click_at(start).unwrap();
        let original = s.state.editor.document.clone();
        s.capture_clip("held-pan", ClipSpec::default(), |s| {
            s.key(egui::Key::Space, true, egui::Modifiers::NONE)?;
            s.pointer_button(egui::PointerButton::Primary, true)?;
            s.move_pointer(start + egui::vec2(45.0, 15.0), Duration::from_millis(200))?;
            assert!(s.state.mouse_navigation_active());
            s.pointer_button(egui::PointerButton::Primary, false)?;
            s.key(egui::Key::Space, false, egui::Modifiers::NONE)?;
            s.wait(Duration::from_millis(100))
        })
        .unwrap();
        assert_eq!(s.state.editor.document, original);
        assert!(!s.state.hand_tool_active());
        let decoder = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(
            &s.images["assets/held-pan.webp"],
        ))
        .unwrap();
        let frames = decoder.into_frames().collect_frames().unwrap();
        assert!((2..=6).contains(&frames.len()));
        assert_eq!(
            frames[0].buffer().dimensions(),
            s.capture.framed_dimensions()
        );
        let duration: u32 = frames
            .iter()
            .map(|frame| {
                let (numerator, denominator) = frame.delay().numer_denom_ms();
                numerator / denominator
            })
            .sum();
        assert_eq!(duration, 300);
        assert_ne!(
            frames.first().unwrap().buffer(),
            frames.last().unwrap().buffer()
        );
        assert!(
            s.capture_clip("failed", ClipSpec::default(), |s| {
                s.wait(Duration::from_millis(100))?;
                Err("expected failure".into())
            })
            .is_err()
        );
        assert!(s.recorder.is_none() && !s.images.contains_key("assets/failed.webp"));
        assert!(
            s.capture_clip("nested", ClipSpec::default(), |s| s.capture_clip(
                "inner",
                ClipSpec::default(),
                |_| Ok(())
            ))
            .is_err()
        );
        assert!(s.recorder.is_none());
    }

    #[test]
    fn invalid_callout_discards_pending_texture_updates_without_masking_the_error() {
        let mut capture =
            pollster::block_on(Capture::new(super::super::WIDTH, super::super::HEIGHT)).unwrap();
        let mut s = Session::new(&mut capture).unwrap();
        s.callout(Control::Gizmo, "This target will disappear")
            .unwrap();
        let _pending_texture = s.ctx.load_texture(
            "invalid-frame-regression",
            egui::ColorImage::filled([1, 1], egui::Color32::WHITE),
            egui::TextureOptions::default(),
        );
        s.state.show_ui = false;
        let error = s.frame(Vec::new(), Duration::ZERO).unwrap_err();
        assert!(error.contains("gizmo"), "{error}");
    }

    #[test]
    fn clean_capture_and_callouts_do_not_change_hover_focus_or_geometry() {
        let mut capture =
            pollster::block_on(Capture::new(super::super::WIDTH, super::super::HEIGHT)).unwrap();
        let mut s = Session::new(&mut capture).unwrap();
        s.load_fixture("cube-quads.obj").unwrap();
        s.hover(Control::Gizmo).unwrap();
        let pointer = s.input.position();
        let focus = s.ctx.memory(|memory| memory.focused());
        let camera = s.state.camera.view_projection(1.0);
        let document = s.state.editor.document.clone();
        s.callout(Control::Gizmo, "Drag to orbit").unwrap();
        s.capture_image("clean-preserves-hover").unwrap();
        assert_eq!(s.input.position(), pointer);
        assert_eq!(s.ctx.pointer_hover_pos(), pointer);
        assert_eq!(s.ctx.memory(|memory| memory.focused()), focus);
        assert_eq!(s.state.camera.view_projection(1.0), camera);
        assert_eq!(s.state.editor.document, document);
        assert!(
            s.callout(Control::PreferencesClose, "Missing target")
                .is_err()
        );
        s.clear_callout().unwrap();
    }
}
