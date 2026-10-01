//! Shared terminal color resolution for painting and application palette queries.
use crate::theme::Palette;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use egui::Color32;

pub(super) fn resolve_color(
    color: Color,
    palette: Palette,
    dark: bool,
    overrides: &alacritty_terminal::term::color::Colors,
) -> Color32 {
    match color {
        Color::Spec(rgb) => Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Named(named) => {
            if let Some(rgb) = overrides[named] {
                return Color32::from_rgb(rgb.r, rgb.g, rgb.b);
            }
            match named {
                NamedColor::Foreground | NamedColor::BrightForeground | NamedColor::Cursor => {
                    palette.foreground
                }
                NamedColor::Background => palette.background,
                NamedColor::DimForeground => palette.muted_foreground,
                NamedColor::DimBlack
                | NamedColor::DimRed
                | NamedColor::DimGreen
                | NamedColor::DimYellow
                | NamedColor::DimBlue
                | NamedColor::DimMagenta
                | NamedColor::DimCyan
                | NamedColor::DimWhite => {
                    ansi_color((named as usize - NamedColor::DimBlack as usize) as u8, dark)
                        .gamma_multiply(0.65)
                }
                _ => ansi_color(named as u8, dark),
            }
        }
        Color::Indexed(index) => {
            if let Some(rgb) = overrides[index as usize] {
                return Color32::from_rgb(rgb.r, rgb.g, rgb.b);
            }
            ansi_color(index, dark)
        }
    }
}

fn ansi_color(index: u8, dark: bool) -> Color32 {
    // ANSI's first 16 colors are semantic; keep them readable in both N3 themes.
    const LIGHT: [[u8; 3]; 16] = [
        [24, 24, 24],
        [180, 35, 35],
        [28, 119, 47],
        [135, 102, 0],
        [35, 77, 179],
        [147, 53, 154],
        [0, 113, 124],
        [118, 118, 118],
        [99, 99, 99],
        [201, 34, 34],
        [23, 135, 40],
        [145, 106, 0],
        [48, 90, 204],
        [171, 54, 181],
        [0, 126, 145],
        [150, 150, 150],
    ];
    const DARK: [[u8; 3]; 16] = [
        [40, 40, 40],
        [235, 99, 99],
        [112, 192, 113],
        [223, 188, 98],
        [112, 159, 234],
        [195, 139, 219],
        [105, 194, 202],
        [216, 216, 216],
        [138, 138, 138],
        [255, 137, 137],
        [153, 219, 142],
        [255, 219, 131],
        [159, 191, 255],
        [228, 174, 243],
        [149, 219, 228],
        [250, 250, 250],
    ];
    let [r, g, b] = match index {
        0..=15 => {
            if dark {
                DARK[index as usize]
            } else {
                LIGHT[index as usize]
            }
        }
        16..=231 => {
            let index = index - 16;
            let channel = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
            [
                channel(index / 36),
                channel((index / 6) % 6),
                channel(index % 6),
            ]
        }
        _ => [8 + (index - 232) * 10; 3],
    };
    Color32::from_rgb(r, g, b)
}
