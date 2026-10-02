//! Passive application-window readout. Collection belongs to the shared frame meter;
//! this painter adds neither input ownership nor an independent repaint loop.
use super::*;
use crate::frame_meter::Sample;

struct Content {
    sample: Option<Sample>,
    labels: [String; 3],
    label: String,
}

#[derive(Default)]
pub(super) struct Display {
    content: Option<Content>,
}

impl WorkspaceUi {
    pub(super) fn paint_fps_meter(&mut self, ctx: &egui::Context) {
        if !self.show_ui || !self.fps_meter.enabled() {
            return;
        }
        let window = ctx.viewport_rect();
        // A painter layer has no Area or response: ordinary controls retain
        // pointer ownership even when this diagnostic paints over them.
        let painter = ctx
            .layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("n3.fps-meter"),
            ))
            .with_clip_rect(window);
        let sample = self.fps_meter.sample();
        if self
            .fps_meter_display
            .content
            .as_ref()
            .map(|content| content.sample)
            != Some(sample)
        {
            let labels = [
                "App FPS · last sample".to_owned(),
                sample.map_or_else(|| "Waiting for frames".to_owned(), sample_label),
                sample.map_or_else(
                    || "Updates with app frames".to_owned(),
                    |sample| format!("Measured over {:.2} s", sample.elapsed().as_secs_f64()),
                ),
            ];
            let label = labels.join("\n");
            self.fps_meter_display.content = Some(Content {
                sample,
                labels,
                label,
            });
        }
        let content = self.fps_meter_display.content.as_ref().unwrap();
        // Reuse egui's font/layout cache rather than retaining atlas-dependent
        // galleys across font resets and scale changes. Number formatting only
        // happens when the completed sample changes.
        let lines: [_; 3] = std::array::from_fn(|index| {
            painter.layout_no_wrap(
                content.labels[index].clone(),
                egui::FontId::monospace(theme::text::XS),
                if index == 1 {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_gray(176)
                },
            )
        });
        let size = egui::vec2(
            lines.iter().map(|line| line.size().x).fold(0.0, f32::max),
            lines.iter().map(|line| line.size().y).sum::<f32>() + theme::space::XS * 2.0,
        ) + egui::Vec2::splat(theme::space::MD * 2.0);
        let rect = egui::Rect::from_min_size(window.right_bottom() - size, size);
        // Developer instrumentation deliberately uses a plain black readout in
        // both themes, separate from the editor's ordinary surface styling.
        painter.rect_filled(rect, egui::CornerRadius::ZERO, egui::Color32::BLACK);
        let mut y = rect.top() + theme::space::MD;
        for (index, line) in lines.iter().enumerate() {
            painter.galley(
                egui::pos2(rect.left() + theme::space::MD, y),
                line.clone(),
                if index == 1 {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_gray(176)
                },
            );
            y += line.size().y + theme::space::XS;
        }
        controls::record(ctx, Control::FpsMeter, &content.label, rect, true);
    }
}

fn sample_label(sample: Sample) -> String {
    let rate = if sample.fps() < 0.1 {
        "<0.1".to_owned()
    } else {
        format!("{:.1}", sample.fps())
    };
    format!("{rate} FPS · {:.2} ms", sample.mean_interval_ms())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_redraw_sample_does_not_display_false_zero_fps() {
        let mut meter = crate::frame_meter::FrameMeter::default();
        meter.toggle();
        meter.record_submission(Duration::ZERO);
        meter.record_submission(Duration::from_secs(20));
        assert_eq!(
            sample_label(meter.sample().unwrap()),
            "<0.1 FPS · 20000.00 ms"
        );
        meter.record_submission(Duration::from_secs(30));
        assert_eq!(
            sample_label(meter.sample().unwrap()),
            "0.1 FPS · 10000.00 ms"
        );
    }
}
