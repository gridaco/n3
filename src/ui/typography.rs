//! Interface fonts only: a single upright Inter variable face, with weights
//! instantiated by egui. Monospace and glyph fallbacks remain egui's defaults.
//!
//! TODO: Document fonts are separate design materials. Eventually support
//! OS-installed fonts there; defer their discovery, storage, and editing model.
use egui::{FontData, FontDefinitions, FontFamily, RichText};

const INTER: &[u8] = include_bytes!("../../assets/fonts/inter/InterVariable.ttf");
const REGULAR_NAME: &str = "n3-inter";
const SEMIBOLD_NAME: &str = "n3-inter-semibold";
pub(crate) const REGULAR_WEIGHT: f32 = 400.0;
pub(crate) const SEMIBOLD_WEIGHT: f32 = 600.0;

pub(crate) fn semibold_family() -> FontFamily {
    FontFamily::Name(SEMIBOLD_NAME.into())
}

fn inter(weight: f32) -> FontData {
    // Both registrations borrow the same embedded bytes. These are variable
    // coordinates evaluated by egui, not additional static font files.
    let mut data = FontData::from_static(INTER);
    data.tweak
        .coords
        .push(egui::epaint::text::Tag::new(b"wght"), weight);
    data
}

fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    let fallbacks = fonts.families[&FontFamily::Proportional].clone();
    fonts
        .font_data
        .insert(REGULAR_NAME.into(), inter(REGULAR_WEIGHT).into());
    fonts
        .font_data
        .insert(SEMIBOLD_NAME.into(), inter(SEMIBOLD_WEIGHT).into());
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .expect("egui defines its proportional family")
        .insert(0, REGULAR_NAME.into());
    let mut semibold = vec![SEMIBOLD_NAME.into()];
    semibold.extend(fallbacks);
    fonts.families.insert(semibold_family(), semibold);
    fonts
}

/// Configure once at context creation, before installing the dedicated Lucide
/// family. Preserve egui's complete default glyph and monospace fallback chains.
pub(crate) fn install(context: &egui::Context) {
    context.set_fonts(definitions());
}

/// `RichText::strong` only changes color in egui; select real variable weight too.
pub(crate) fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text).family(semibold_family()).strong()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upright_inter_has_a_weight_axis_and_instances_share_its_bytes() {
        let fonts = definitions();
        let regular = &fonts.font_data[REGULAR_NAME];
        let semibold = &fonts.font_data[SEMIBOLD_NAME];
        assert!(std::ptr::eq(regular.font.as_ref(), semibold.font.as_ref()));
        let axes = regular.variation_axes();
        let weight = axes
            .iter()
            .find(|axis| axis.tag == egui::epaint::text::Tag::new(b"wght"))
            .expect("the bundled face must remain variable");
        assert!(weight.range.contains(REGULAR_WEIGHT));
        assert!(weight.range.contains(SEMIBOLD_WEIGHT));
        assert!(
            axes.iter()
                .all(|axis| axis.tag != egui::epaint::text::Tag::new(b"ital"))
        );
    }

    #[test]
    fn default_monospace_and_proportional_fallbacks_survive() {
        let original = FontDefinitions::default();
        let fonts = definitions();
        assert_eq!(
            fonts.families[&FontFamily::Monospace],
            original.families[&FontFamily::Monospace]
        );
        assert_eq!(
            &fonts.families[&FontFamily::Proportional][1..],
            original.families[&FontFamily::Proportional]
        );
        assert_eq!(
            &fonts.families[&semibold_family()][1..],
            original.families[&FontFamily::Proportional]
        );
        for (name, data) in original.font_data {
            assert_eq!(fonts.font_data[&name], data);
        }
    }

    #[test]
    fn egui_rasterizes_distinct_variable_weights() {
        let render = |family: FontFamily| {
            let context = egui::Context::default();
            install(&context);
            let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                ui.label(
                    RichText::new("Hamburgefonts 123")
                        .family(family.clone())
                        .size(crate::theme::text::XL_4),
                );
            });
            output.textures_delta.clear(); // This unit test inspects the atlas without a renderer.
            context.fonts_mut(|fonts| fonts.image())
        };
        let regular = render(FontFamily::Proportional);
        let semibold = render(semibold_family());
        assert!(regular != semibold, "weight must affect rasterized glyphs");
        assert!(regular == render(FontFamily::Proportional));
    }

    #[test]
    fn glyph_fallback_and_monospace_work_with_the_full_application_font_setup() {
        let context = egui::Context::default();
        crate::ui::workspace_ui::configure_context(&context);
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            ui.ctx().fonts_mut(|fonts| {
                for family in [FontFamily::Proportional, semibold_family()] {
                    // egui 0.36's has_glyph compares face IDs and reports false
                    // for valid glyphs from the face containing the replacement
                    // square. Check actual layout instead: Inter lacks this
                    // emoji, which must render differently from a missing glyph.
                    let galley = fonts.layout_no_wrap(
                        "😀\u{10ffff}".into(),
                        egui::FontId::new(crate::theme::text::SM, family),
                        egui::Color32::WHITE,
                    );
                    let glyphs = &galley.rows[0].glyphs;
                    assert_eq!(glyphs.len(), 2);
                    assert_ne!(glyphs[0].uv_rect, glyphs[1].uv_rect);
                }
                let mono = egui::FontId::monospace(crate::theme::text::SM);
                assert_eq!(fonts.glyph_width(&mono, 'i'), fonts.glyph_width(&mono, 'W'));
            });
        });
        output.textures_delta.clear(); // No renderer in this font-only regression.
    }
}
