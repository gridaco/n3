//! Lucide's font lives in one named egui family. Keep codepoints here so a
//! replacement font only needs this table updated from its matching CSS.
use egui::{
    FontFamily, RichText,
    epaint::text::{FontInsert, FontPriority, InsertFontFamily},
};

const FONT_NAME: &str = "n3-lucide";

/// Register the bundled font without changing the app's normal text fallback.
pub(crate) fn install(context: &egui::Context) {
    context.add_font(FontInsert::new(
        FONT_NAME,
        egui::FontData::from_static(include_bytes!("../../assets/fonts/lucide/lucide.ttf")),
        vec![InsertFontFamily {
            family: FontFamily::Name(FONT_NAME.into()),
            priority: FontPriority::Highest,
        }],
    ));
}

/// Icons used by the application, named after Lucide's canonical icon names.
/// Codepoints come from lucide-static 1.48.0's `font/lucide.css`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Icon {
    MousePointer2,
    Move3d,
    Rotate3d,
    Scale3d,
    X,
    Box,
    Cylinder,
    Cone,
    Torus,
    RectangleHorizontal,
    Circle,
    ScanBox,
    ChevronRight,
}

impl Icon {
    fn codepoint(self) -> char {
        match self {
            Self::MousePointer2 => '\u{e1c3}',       // mouse-pointer-2
            Self::Move3d => '\u{e2e5}',              // move-3d
            Self::Rotate3d => '\u{e2ea}',            // rotate-3d
            Self::Scale3d => '\u{e2eb}',             // scale-3d
            Self::X => '\u{e1b2}',                   // x
            Self::Box => '\u{e061}',                 // box
            Self::Cylinder => '\u{e525}',            // cylinder
            Self::Cone => '\u{e523}',                // cone
            Self::Torus => '\u{e52f}',               // torus
            Self::RectangleHorizontal => '\u{e376}', // rectangle-horizontal
            Self::Circle => '\u{e076}',              // circle
            Self::ScanBox => '\u{e720}',             // scan-box
            Self::ChevronRight => '\u{e06f}',        // chevron-right
        }
    }

    pub(crate) fn paint(self, painter: &egui::Painter, position: egui::Pos2, color: egui::Color32) {
        painter.text(
            position,
            egui::Align2::LEFT_CENTER,
            self.codepoint(),
            egui::FontId::new(crate::theme::text::BASE, FontFamily::Name(FONT_NAME.into())),
            color,
        );
    }

    pub(crate) fn paint_centered(
        self,
        painter: &egui::Painter,
        center: egui::Pos2,
        size: f32,
        color: egui::Color32,
    ) {
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            self.codepoint(),
            egui::FontId::new(size, FontFamily::Name(FONT_NAME.into())),
            color,
        );
    }

    pub(crate) fn text(self, size: f32) -> RichText {
        RichText::new(self.codepoint().to_string())
            .family(FontFamily::Name(FONT_NAME.into()))
            .size(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_contains_every_ui_icon() {
        let context = egui::Context::default();
        // Exercise the full registration order, including the default UI face,
        // so replacing interface fonts cannot silently remove the icon family.
        crate::ui::workspace_ui::configure_context(&context);
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().fonts_mut(|fonts| {
                let font_id =
                    egui::FontId::new(crate::theme::text::BASE, FontFamily::Name(FONT_NAME.into()));
                for icon in [
                    Icon::MousePointer2,
                    Icon::Move3d,
                    Icon::Rotate3d,
                    Icon::Scale3d,
                    Icon::X,
                    Icon::Box,
                    Icon::Cylinder,
                    Icon::Cone,
                    Icon::Torus,
                    Icon::RectangleHorizontal,
                    Icon::Circle,
                    Icon::ScanBox,
                    Icon::ChevronRight,
                ] {
                    assert!(fonts.has_glyph(&font_id, icon.codepoint()), "{icon:?}");
                }
            });
        });
        output.textures_delta.clear(); // Font-only test; there is no renderer.
    }
}
