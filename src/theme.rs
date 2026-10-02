//! Semantic interface colors, mapped onto egui's built-in light and dark styles.
//! Geometry, selection semantics, and 3D material lighting live elsewhere.
use crate::settings::{AccentColor, ResolvedTheme};
use crate::ui::typography;

/// Emphasis uses Inter's weight axis as well as egui's stronger text color.
pub(crate) fn strong(text: impl Into<String>) -> egui::RichText {
    typography::strong(text)
}

// Keep the complete interface vocabulary even when a component currently
// shares its value with another surface. Tokens without a distinct consumer
// are intentional, not speculative widget styles.
#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: egui::Color32,
    /// Window chrome is a distinct role, initially matching the main surface.
    pub titlebar: egui::Color32,
    pub foreground: egui::Color32,
    pub card: egui::Color32,
    pub card_foreground: egui::Color32,
    pub popover: egui::Color32,
    pub popover_foreground: egui::Color32,
    pub primary: egui::Color32,
    pub primary_foreground: egui::Color32,
    pub secondary: egui::Color32,
    pub secondary_foreground: egui::Color32,
    pub muted: egui::Color32,
    pub muted_foreground: egui::Color32,
    pub accent: egui::Color32,
    pub accent_foreground: egui::Color32,
    pub destructive: egui::Color32,
    pub border: egui::Color32,
    pub input: egui::Color32,
    pub ring: egui::Color32,
    pub sidebar: egui::Color32,
    pub sidebar_foreground: egui::Color32,
    pub sidebar_primary: egui::Color32,
    pub sidebar_primary_foreground: egui::Color32,
    pub sidebar_accent: egui::Color32,
    pub sidebar_accent_foreground: egui::Color32,
    pub sidebar_border: egui::Color32,
    pub sidebar_ring: egui::Color32,
    // N3-specific workbench colors are separate from shadcn interface tokens.
    pub workbench_viewport: egui::Color32,
    pub workbench_hud: egui::Color32,
    pub workbench_hud_border: egui::Color32,
}

impl Palette {
    pub fn new(theme: ResolvedTheme, chosen_accent: AccentColor) -> Self {
        let [r, g, b] = chosen_accent.rgb();
        let chosen_accent = egui::Color32::from_rgb(r, g, b);
        let mut palette = match theme {
            ResolvedTheme::Light => Self {
                background: rgb(255, 255, 255),
                titlebar: rgb(255, 255, 255),
                foreground: rgb(24, 24, 27),
                card: rgb(255, 255, 255),
                card_foreground: rgb(24, 24, 27),
                popover: rgb(255, 255, 255),
                popover_foreground: rgb(24, 24, 27),
                primary: chosen_accent,
                primary_foreground: egui::Color32::WHITE,
                secondary: rgb(245, 245, 246),
                secondary_foreground: rgb(24, 24, 27),
                muted: rgb(249, 249, 250),
                muted_foreground: rgb(105, 105, 114),
                accent: rgb(237, 237, 239),
                accent_foreground: rgb(24, 24, 27),
                destructive: egui::Visuals::light().error_fg_color,
                border: rgb(226, 226, 230),
                input: rgb(226, 226, 230),
                ring: chosen_accent,
                sidebar: rgb(255, 255, 255),
                sidebar_foreground: rgb(24, 24, 27),
                sidebar_primary: chosen_accent,
                sidebar_primary_foreground: egui::Color32::WHITE,
                sidebar_accent: rgb(237, 237, 239),
                sidebar_accent_foreground: rgb(24, 24, 27),
                sidebar_border: rgb(226, 226, 230),
                sidebar_ring: chosen_accent,
                workbench_viewport: rgb(242, 244, 247),
                workbench_hud: egui::Color32::from_rgba_unmultiplied(255, 255, 255, 242),
                workbench_hud_border: rgb(216, 216, 220),
            },
            // Achromatic surfaces and text; emphasis and semantic colors remain colored.
            ResolvedTheme::Dark => Self {
                background: egui::Color32::from_gray(23),
                titlebar: egui::Color32::from_gray(23),
                foreground: egui::Color32::from_gray(250),
                card: egui::Color32::from_gray(23),
                card_foreground: egui::Color32::from_gray(250),
                popover: egui::Color32::from_gray(23),
                popover_foreground: egui::Color32::from_gray(250),
                primary: chosen_accent,
                primary_foreground: egui::Color32::WHITE,
                secondary: egui::Color32::from_gray(38),
                secondary_foreground: egui::Color32::from_gray(250),
                muted: egui::Color32::from_gray(38),
                muted_foreground: egui::Color32::from_gray(163),
                accent: egui::Color32::from_gray(48),
                accent_foreground: egui::Color32::from_gray(250),
                destructive: egui::Visuals::dark().error_fg_color,
                border: egui::Color32::from_gray(64),
                input: egui::Color32::from_gray(64),
                ring: chosen_accent,
                sidebar: egui::Color32::from_gray(23),
                sidebar_foreground: egui::Color32::from_gray(250),
                sidebar_primary: chosen_accent,
                sidebar_primary_foreground: egui::Color32::WHITE,
                sidebar_accent: egui::Color32::from_gray(48),
                sidebar_accent_foreground: egui::Color32::from_gray(250),
                sidebar_border: egui::Color32::from_gray(64),
                sidebar_ring: chosen_accent,
                workbench_viewport: egui::Color32::from_gray(23),
                workbench_hud: egui::Color32::from_rgba_unmultiplied(23, 23, 23, 238),
                workbench_hud_border: egui::Color32::from_gray(64),
            },
        };
        // The saved accent is N3's primary action color. shadcn's accent is
        // the neutral hover surface, so the two must remain separate.
        palette.primary = legible_accent(
            chosen_accent,
            palette.background,
            palette.pressed_fill(),
            theme,
        );
        palette.primary_foreground = if contrast_ratio(egui::Color32::WHITE, palette.primary)
            >= contrast_ratio(egui::Color32::BLACK, palette.primary)
        {
            egui::Color32::WHITE
        } else {
            egui::Color32::BLACK
        };
        palette.ring = palette.primary;
        palette.sidebar_primary = palette.primary;
        palette.sidebar_primary_foreground = palette.primary_foreground;
        palette.sidebar_ring = palette.ring;
        palette
    }

    fn pressed_fill(self) -> egui::Color32 {
        mix(self.accent, self.border, 98)
    }

    pub fn from_context(ctx: &egui::Context, accent: AccentColor) -> Self {
        let theme = match ctx.theme() {
            egui::Theme::Light => ResolvedTheme::Light,
            egui::Theme::Dark => ResolvedTheme::Dark,
        };
        Self::new(theme, accent)
    }
}

fn rgb(r: u8, g: u8, b: u8) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}

fn mix(a: egui::Color32, b: egui::Color32, b_weight: u16) -> egui::Color32 {
    let channel =
        |a: u8, b: u8| ((u16::from(a) * (100 - b_weight) + u16::from(b) * b_weight) / 100) as u8;
    rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

fn contrast_ratio(a: egui::Color32, b: egui::Color32) -> f32 {
    let luminance = |color: egui::Color32| {
        let channel = |value: u8| {
            let s = f32::from(value) / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn legible_accent(
    chosen: egui::Color32,
    panel: egui::Color32,
    active: egui::Color32,
    theme: ResolvedTheme,
) -> egui::Color32 {
    let endpoint = match theme {
        ResolvedTheme::Light => egui::Color32::BLACK,
        ResolvedTheme::Dark => egui::Color32::WHITE,
    };
    for amount in 0..=100 {
        let candidate = mix(chosen, endpoint, amount);
        let selected = mix(panel, candidate, 24);
        if contrast_ratio(candidate, selected) >= 3.0 && contrast_ratio(candidate, active) >= 3.0 {
            return candidate;
        }
    }
    endpoint
}

pub const ROW_HEIGHT: f32 = size::XL_3;
#[allow(dead_code)] // Shared layout expectation witnessed by the executable guide.
pub const TOOL_DOCK_TAB_BAR_HEIGHT: f32 = size::STEP_8;
pub const TOOL_DOCK_TAB_BAR_INSET: f32 = space::SM;
/// Tailwind's default font-size scale in egui points, plus N3's compact and
/// spatial-label sizes. These are font sizes only; egui owns line height.
#[allow(dead_code)] // Keep the complete type vocabulary for future controls.
pub mod text {
    pub const XS: f32 = 12.0;
    pub const SM: f32 = 14.0;
    pub const BASE: f32 = 16.0;
    pub const LG: f32 = 18.0;
    pub const XL: f32 = 20.0;
    pub const XL_2: f32 = 24.0;
    pub const XL_3: f32 = 30.0;
    pub const XL_4: f32 = 36.0;
    pub const XL_5: f32 = 48.0;
    pub const XL_6: f32 = 60.0;
    pub const XL_7: f32 = 72.0;
    pub const XL_8: f32 = 96.0;
    pub const XL_9: f32 = 128.0;

    pub const UI_BODY_13: f32 = 13.0;
    pub const SMALL_UI_11: f32 = 11.0;
    pub const SECTION_TITLE_13: f32 = 13.0;
    pub const AXIS_LABEL_11: f32 = 11.0;
    pub const INSPECTOR_BODY_12_5: f32 = 12.5;
    pub const RULER_10: f32 = 10.0;
    pub const GIZMO_BACK_AXIS_10: f32 = 10.0;
    pub const TOOL_ICON_17: f32 = 17.0;
    pub const GUIDE_CUE_13: f32 = 13.0;
}
/// Named spacing steps in egui points. These align with Tailwind's 4-point
/// spacing unit (xs = 0.5 units, sm = 1, md = 1.5, and so on), while remaining
/// independent from corner radii.
#[allow(dead_code)] // Keep the complete scale available for future layouts.
pub mod space {
    pub const NONE: f32 = 0.0;
    pub const PX: f32 = 1.0;
    pub const XS: f32 = 2.0;
    pub const SM: f32 = 4.0;
    pub const MD: f32 = 6.0;
    pub const LG: f32 = 8.0;
    pub const XL: f32 = 12.0;
    pub const XL_2: f32 = 16.0;
    pub const XL_3: f32 = 24.0;
    pub const XL_4: f32 = 32.0;
    pub const XL_5: f32 = 40.0;

    // egui stores frame margins as i8 even though layout spacing is f32.
    pub const fn margin(value: f32) -> egui::Margin {
        egui::Margin::same(value as i8)
    }

    pub const fn symmetric_margin(x: f32, y: f32) -> egui::Margin {
        egui::Margin::symmetric(x as i8, y as i8)
    }
}

/// Fixed control widths and heights. Values mirror `space`, but a separate
/// namespace makes component dimensions distinct from padding and gaps.
#[allow(dead_code)] // Keep the complete size vocabulary for future controls.
pub mod size {
    pub const NONE: f32 = super::space::NONE;
    pub const PX: f32 = super::space::PX;
    pub const XS: f32 = super::space::XS;
    pub const SM: f32 = super::space::SM;
    pub const MD: f32 = super::space::MD;
    pub const LG: f32 = super::space::LG;
    pub const XL: f32 = super::space::XL;
    pub const XL_2: f32 = super::space::XL_2;
    pub const XL_3: f32 = super::space::XL_3;
    pub const XL_4: f32 = super::space::XL_4;
    pub const XL_5: f32 = super::space::XL_5;

    // Tailwind's numeric size scale: each step is four egui points.
    // Keep the standard steps through 60 available for widths and heights.
    pub const STEP_0: f32 = 0.0;
    pub const STEP_0_5: f32 = 2.0;
    pub const STEP_1: f32 = 4.0;
    pub const STEP_1_5: f32 = 6.0;
    pub const STEP_2: f32 = 8.0;
    pub const STEP_2_5: f32 = 10.0;
    pub const STEP_3: f32 = 12.0;
    pub const STEP_3_5: f32 = 14.0;
    pub const STEP_4: f32 = 16.0;
    pub const STEP_5: f32 = 20.0;
    pub const STEP_6: f32 = 24.0;
    pub const STEP_7: f32 = 28.0;
    pub const STEP_8: f32 = 32.0;
    pub const STEP_9: f32 = 36.0;
    pub const STEP_10: f32 = 40.0;
    pub const STEP_11: f32 = 44.0;
    pub const STEP_12: f32 = 48.0;
    pub const STEP_14: f32 = 56.0;
    pub const STEP_16: f32 = 64.0;
    pub const STEP_20: f32 = 80.0;
    pub const STEP_24: f32 = 96.0;
    pub const STEP_28: f32 = 112.0;
    pub const STEP_32: f32 = 128.0;
    pub const STEP_36: f32 = 144.0;
    pub const STEP_40: f32 = 160.0;
    pub const STEP_44: f32 = 176.0;
    pub const STEP_48: f32 = 192.0;
    pub const STEP_52: f32 = 208.0;
    pub const STEP_56: f32 = 224.0;
    pub const STEP_60: f32 = 240.0;
}

pub const SECTION_TOP: f32 = space::MD + space::PX;
pub const SECTION_BOTTOM: f32 = space::LG + space::PX;
pub const SECTION_GAP: f32 = space::MD;
pub const ROW_GAP: f32 = space::MD;
pub const INSPECTOR_ITEM_GAP: f32 = space::SM;
pub const FIELD_PADDING_X: f32 = space::SM;
pub const FIELD_PADDING_Y: f32 = space::XS + space::PX;
pub const LABEL_INSET: f32 = space::XS;
/// Tailwind's default rounded scale, expressed in egui points. Keep widget
/// corners on this scale; derived outer corners can add their inset and stroke.
#[allow(dead_code)] // Keep the full design vocabulary for future controls.
pub mod radius {
    pub const NONE: u8 = 0;
    pub const XS: u8 = 2;
    pub const SM: u8 = 4;
    pub const MD: u8 = 6;
    pub const LG: u8 = 8;
    pub const XL: u8 = 12;
    pub const XL_2: u8 = 16;
    pub const XL_3: u8 = 24;
    pub const XL_4: u8 = 32;
    pub const FULL: u8 = u8::MAX;
}

/// Tailwind's outer box-shadow vocabulary adapted to egui's single shadow
/// layer. egui cannot represent Tailwind's paired CSS shadows or negative
/// spread exactly, so each token keeps the same visual size progression.
#[allow(dead_code)] // Keep the complete scale available for future surfaces.
pub mod shadow {
    #[derive(Clone, Copy)]
    pub struct Token {
        offset: [i8; 2],
        blur: u8,
        spread: u8,
        light_alpha: u8,
        dark_alpha: u8,
    }

    impl Token {
        pub const fn resolve(self, dark_mode: bool) -> egui::Shadow {
            egui::Shadow {
                offset: self.offset,
                blur: self.blur,
                spread: self.spread,
                color: egui::Color32::from_black_alpha(if dark_mode {
                    self.dark_alpha
                } else {
                    self.light_alpha
                }),
            }
        }
    }

    pub const NONE: Token = Token {
        offset: [0, 0],
        blur: 0,
        spread: 0,
        light_alpha: 0,
        dark_alpha: 0,
    };
    pub const XS_2: Token = Token {
        offset: [0, 1],
        blur: 0,
        spread: 0,
        light_alpha: 13,
        dark_alpha: 32,
    };
    pub const XS: Token = Token {
        offset: [0, 1],
        blur: 2,
        spread: 0,
        light_alpha: 13,
        dark_alpha: 36,
    };
    pub const SM: Token = Token {
        offset: [0, 1],
        blur: 4,
        spread: 0,
        light_alpha: 20,
        dark_alpha: 48,
    };
    pub const MD: Token = Token {
        offset: [0, 2],
        blur: 10,
        spread: 0,
        light_alpha: 22,
        dark_alpha: 64,
    };
    pub const LG: Token = Token {
        offset: [0, 6],
        blur: 16,
        spread: 0,
        light_alpha: 25,
        dark_alpha: 80,
    };
    pub const XL: Token = Token {
        offset: [0, 12],
        blur: 24,
        spread: 0,
        light_alpha: 30,
        dark_alpha: 96,
    };
    pub const XL_2: Token = Token {
        offset: [0, 24],
        blur: 48,
        spread: 0,
        light_alpha: 64,
        dark_alpha: 128,
    };
}

/// Semantic axis identity stays stable across themes and accent choices.
pub const AXIS_COLORS: [egui::Color32; 3] = [
    egui::Color32::from_rgb(235, 111, 123),
    egui::Color32::from_rgb(141, 207, 116),
    egui::Color32::from_rgb(112, 166, 242),
];
/// Object feedback stays distinct from the configurable interface accent and
/// contrasts with both viewport backgrounds.
pub const OBJECT_SELECTED: egui::Color32 = egui::Color32::from_rgb(61, 132, 219);
pub const OBJECT_HOVERED: egui::Color32 = egui::Color32::from_rgb(104, 139, 172);

/// Menu rows share the native flat treatment with a readable text inset.
/// egui's default menu style resets horizontal button padding to two points.
pub fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = egui::vec2(space::LG, space::NONE);
    // Adjacent menu rows share an edge, so pointer hover has no dead gaps.
    style.spacing.item_spacing.y = space::NONE;
}

fn style(theme: ResolvedTheme, accent: AccentColor) -> egui::Style {
    let palette = Palette::new(theme, accent);
    let mut style = egui::Style {
        visuals: match theme {
            ResolvedTheme::Light => egui::Visuals::light(),
            ResolvedTheme::Dark => egui::Visuals::dark(),
        },
        ..Default::default()
    };
    style.spacing.item_spacing = egui::vec2(space::LG, space::MD);
    style.spacing.button_padding = egui::vec2(space::MD + space::PX, space::SM);
    style.spacing.window_margin = space::symmetric_margin(space::XL, space::LG + space::XS);
    style.spacing.menu_margin = space::margin(space::MD);
    style.spacing.icon_width = text::SM;
    style.spacing.interact_size.y = ROW_HEIGHT;
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::proportional(text::UI_BODY_13),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::proportional(text::UI_BODY_13),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::proportional(text::SMALL_UI_11),
    );
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(text::LG, typography::semibold_family()),
    );

    let visuals = &mut style.visuals;
    visuals.panel_fill = palette.background;
    // egui uses window_fill for menus as well; Preferences opts into card.
    visuals.window_fill = palette.popover;
    visuals.window_stroke = egui::Stroke::new(1.0, palette.border);
    visuals.window_corner_radius = egui::CornerRadius::same(radius::LG);
    visuals.menu_corner_radius = egui::CornerRadius::same(radius::MD);
    let surface_shadow = shadow::MD.resolve(visuals.dark_mode);
    visuals.popup_shadow = surface_shadow;
    visuals.window_shadow = surface_shadow;
    visuals.faint_bg_color = palette.muted;
    // shadcn's input is a border token; egui also needs a field fill.
    visuals.extreme_bg_color = palette.secondary;
    visuals.text_edit_bg_color = Some(palette.secondary);
    visuals.code_bg_color = palette.secondary;
    visuals.weak_text_color = Some(palette.muted_foreground);
    visuals.error_fg_color = palette.destructive;
    visuals.selection.bg_fill = mix(palette.background, palette.primary, 24);
    visuals.selection.stroke = egui::Stroke::new(1.0, palette.primary);
    visuals.text_cursor.stroke = egui::Stroke::new(2.0, palette.ring);
    visuals.widgets.noninteractive.bg_fill = palette.background;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, palette.border);
    visuals.widgets.noninteractive.fg_stroke.color = palette.foreground;
    for widget in [&mut visuals.widgets.inactive, &mut visuals.widgets.open] {
        widget.bg_fill = palette.secondary;
        widget.weak_bg_fill = palette.muted;
        widget.bg_stroke = egui::Stroke::new(1.0, palette.input);
        widget.fg_stroke.color = palette.secondary_foreground;
        widget.corner_radius = egui::CornerRadius::same(radius::MD);
    }
    visuals.widgets.hovered.bg_fill = palette.accent;
    visuals.widgets.hovered.weak_bg_fill = palette.accent;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, palette.border);
    visuals.widgets.hovered.fg_stroke.color = palette.accent_foreground;
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(radius::MD);
    visuals.widgets.active.bg_fill = palette.pressed_fill();
    visuals.widgets.active.weak_bg_fill = palette.pressed_fill();
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, palette.ring);
    visuals.widgets.active.fg_stroke.color = palette.accent_foreground;
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(radius::MD);
    style
}

/// Install both built-in style variants, then select the resolved preference.
/// Rebuild only when the resolved theme or accent actually changes.
pub fn apply_context(ctx: &egui::Context, resolved: ResolvedTheme, accent: AccentColor) {
    ctx.set_style_of(egui::Theme::Light, style(ResolvedTheme::Light, accent));
    ctx.set_style_of(egui::Theme::Dark, style(ResolvedTheme::Dark, accent));
    ctx.set_theme(match resolved {
        ResolvedTheme::Light => egui::Theme::Light,
        ResolvedTheme::Dark => egui::Theme::Dark,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_egui_styles_share_metrics_but_keep_distinct_neutral_surfaces() {
        let ctx = egui::Context::default();
        apply_context(&ctx, ResolvedTheme::Light, AccentColor::DEFAULT);
        let light = ctx.style_of(egui::Theme::Light);
        let dark = ctx.style_of(egui::Theme::Dark);
        assert_eq!(ctx.theme(), egui::Theme::Light);
        assert_eq!(light.spacing.item_spacing, dark.spacing.item_spacing);
        assert_ne!(light.visuals.panel_fill, dark.visuals.panel_fill);
        assert_ne!(
            light.visuals.widgets.hovered.bg_fill,
            dark.visuals.widgets.hovered.bg_fill
        );
        assert_ne!(
            light.visuals.selection.bg_fill,
            dark.visuals.selection.bg_fill
        );
    }

    #[test]
    fn accent_changes_selection_and_focus_without_recoloring_errors() {
        let ctx = egui::Context::default();
        apply_context(&ctx, ResolvedTheme::Dark, AccentColor::DEFAULT);
        let original_error = ctx.style_of(ctx.theme()).visuals.error_fg_color;
        let original_selection = ctx.style_of(ctx.theme()).visuals.selection;
        apply_context(&ctx, ResolvedTheme::Dark, AccentColor::new(230, 120, 30));
        assert_ne!(
            ctx.style_of(ctx.theme()).visuals.selection,
            original_selection
        );
        assert_eq!(
            ctx.style_of(ctx.theme()).visuals.error_fg_color,
            original_error
        );
    }

    #[test]
    fn dark_surfaces_and_text_are_neutral_with_legible_contrast() {
        let p = Palette::new(ResolvedTheme::Dark, AccentColor::DEFAULT);
        for color in [
            p.background,
            p.titlebar,
            p.foreground,
            p.card,
            p.card_foreground,
            p.popover,
            p.popover_foreground,
            p.secondary,
            p.secondary_foreground,
            p.muted,
            p.muted_foreground,
            p.accent,
            p.accent_foreground,
            p.border,
            p.input,
            p.sidebar,
            p.sidebar_foreground,
            p.sidebar_accent,
            p.sidebar_accent_foreground,
            p.sidebar_border,
            p.workbench_viewport,
            p.workbench_hud,
            p.workbench_hud_border,
        ] {
            assert_eq!(color.r(), color.g());
            assert_eq!(color.g(), color.b());
        }
        for background in [p.background, p.secondary, p.accent] {
            assert!(contrast_ratio(p.foreground, background) >= 4.5);
            assert!(contrast_ratio(p.muted_foreground, background) >= 4.5);
        }
        assert_ne!(p.primary.r(), p.primary.b());
        assert_eq!(p.destructive, egui::Visuals::dark().error_fg_color);
        for appearance in [ResolvedTheme::Light, ResolvedTheme::Dark] {
            let p = Palette::new(appearance, AccentColor::DEFAULT);
            assert_eq!(p.titlebar, p.background);
            assert_eq!(p.titlebar, p.sidebar);
        }
    }

    #[test]
    fn accents_remain_visible_on_both_schemes_even_at_extreme_values() {
        for (theme, chosen) in [
            (ResolvedTheme::Light, AccentColor::new(255, 255, 255)),
            (ResolvedTheme::Dark, AccentColor::new(0, 0, 0)),
        ] {
            let palette = Palette::new(theme, chosen);
            let selected = mix(palette.background, palette.primary, 24);
            assert!(contrast_ratio(palette.primary, selected) >= 3.0);
            assert!(contrast_ratio(palette.primary, palette.pressed_fill()) >= 3.0);
            assert!(contrast_ratio(palette.primary, palette.primary_foreground) >= 4.5);
        }
    }

    #[test]
    fn token_roles_are_distinct_from_egui_widget_fills() {
        for theme in [ResolvedTheme::Light, ResolvedTheme::Dark] {
            let palette = Palette::new(theme, AccentColor::DEFAULT);
            let style = style(theme, AccentColor::DEFAULT);
            assert_eq!(style.visuals.panel_fill, palette.background);
            assert_eq!(style.visuals.window_fill, palette.popover);
            assert_eq!(
                style.visuals.popup_shadow,
                shadow::MD.resolve(theme == ResolvedTheme::Dark)
            );
            assert_eq!(style.visuals.window_shadow, style.visuals.popup_shadow);
            assert_eq!(style.visuals.text_edit_bg_color, Some(palette.secondary));
            assert_eq!(
                style.visuals.widgets.inactive.bg_stroke.color,
                palette.input
            );
            assert_eq!(style.visuals.widgets.hovered.bg_fill, palette.accent);
            assert_eq!(style.visuals.text_cursor.stroke.color, palette.ring);
            assert_ne!(palette.primary, palette.accent);
        }
    }

    #[test]
    fn shadow_tokens_progress_in_softness_and_adapt_to_dark_surfaces() {
        let scale = [
            shadow::XS_2,
            shadow::XS,
            shadow::SM,
            shadow::MD,
            shadow::LG,
            shadow::XL,
            shadow::XL_2,
        ];
        assert_eq!(shadow::NONE.resolve(false), egui::Shadow::NONE);
        assert!(scale.windows(2).all(|pair| {
            let smaller = pair[0].resolve(false);
            let larger = pair[1].resolve(false);
            smaller.blur <= larger.blur && smaller.offset[1] <= larger.offset[1]
        }));
        assert!(shadow::LG.resolve(true).color.a() > shadow::LG.resolve(false).color.a());
    }
}
