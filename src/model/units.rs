//! Document lengths are centimeters. Display and input units only convert at
//! the UI boundary; changing a display unit never rescales authored geometry.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanonicalLengthUnit {
    #[default]
    #[serde(rename = "cm")]
    Centimeters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LengthUnit {
    Millimeters,
    #[default]
    Centimeters,
    Meters,
    Inches,
    Feet,
}

impl LengthUnit {
    pub const ALL: [Self; 5] = [
        Self::Millimeters,
        Self::Centimeters,
        Self::Meters,
        Self::Inches,
        Self::Feet,
    ];

    pub fn symbol(self) -> &'static str {
        match self {
            Self::Millimeters => "mm",
            Self::Centimeters => "cm",
            Self::Meters => "m",
            Self::Inches => "in",
            Self::Feet => "ft",
        }
    }

    pub fn to_centimeters(self, value: f64) -> f64 {
        value * self.centimeters_per_unit()
    }

    // The receiver selects the display unit, rather than constructing a LengthUnit.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_centimeters(self, value: f64) -> f64 {
        value / self.centimeters_per_unit()
    }

    fn centimeters_per_unit(self) -> f64 {
        match self {
            Self::Millimeters => 0.1,
            Self::Centimeters => 1.0,
            Self::Meters => 100.0,
            Self::Inches => 2.54,
            Self::Feet => 30.48,
        }
    }
}

/// Shortest round-trippable field text, using scientific notation for very small
/// or large values. Fixed decimal limits can turn a positive length into zero
/// when an otherwise untouched numeric field is parsed again on focus loss.
pub fn format_length_number(value: f64) -> String {
    if value != 0.0 && (value.abs() < 1.0e-4 || value.abs() >= 1.0e6) {
        format!("{value:e}")
    } else {
        value.to_string()
    }
}

/// Parse one number or a numerator/denominator fraction, optionally followed by
/// mm, cm, m, in, or ft. Numbers may use decimal or scientific notation. An
/// explicit suffix overrides `default_unit`; signs and fractional centimeters
/// are preserved. Expressions, mixed fractions, and nonfinite values are rejected.
pub fn parse_length(text: &str, default_unit: LengthUnit) -> Option<f64> {
    let text = text.trim();
    let number = text.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let suffix = &text[number.len()..];
    let unit = if suffix.is_empty() {
        default_unit
    } else {
        LengthUnit::ALL
            .into_iter()
            .find(|unit| unit.symbol().eq_ignore_ascii_case(suffix))?
    };
    let number = number.trim();
    let value = if let Some((numerator, denominator)) = number.split_once('/') {
        let numerator = finite_number(numerator)?;
        let denominator = finite_number(denominator)?;
        if denominator == 0.0 {
            return None;
        }
        numerator / denominator
    } else {
        finite_number(number)?
    };
    let centimeters = unit.to_centimeters(value);
    centimeters.is_finite().then_some(centimeters)
}

fn finite_number(text: &str) -> Option<f64> {
    let value = text.trim().parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_number_format_preserves_every_bit_without_hiding_small_lengths() {
        for value in [
            0.0,
            -0.0,
            f64::from_bits(1),
            1.0e-300,
            -1.0e-12,
            1.0e-8,
            0.0001,
            0.123_456_789_012_345_66,
            -12.345_678_901_234_567,
            999_999.999_999_999_9,
            1.0e6,
            f64::MAX,
        ] {
            let text = format_length_number(value);
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), value.to_bits());
            if value != 0.0 {
                assert_ne!(text.parse::<f64>().unwrap(), 0.0);
            }
            assert_eq!(
                text.contains('e'),
                value != 0.0 && (value.abs() < 1.0e-4 || value.abs() >= 1.0e6)
            );
        }
    }

    #[test]
    fn formatted_lengths_roundtrip_through_each_display_unit_parser() {
        for unit in LengthUnit::ALL {
            for centimeters in [
                1.0e-200,
                1.0e-10,
                -0.123_456_789_012_345_66,
                12.345_678_901_234_567,
                1.0e200,
            ] {
                let displayed = unit.from_centimeters(centimeters);
                let text = format_length_number(displayed);
                assert_eq!(text.parse::<f64>().unwrap().to_bits(), displayed.to_bits());
                let parsed = parse_length(&text, unit).unwrap();
                assert!((parsed - centimeters).abs() <= centimeters.abs() * f64::EPSILON * 2.0);
                assert_ne!(parsed, 0.0);
                assert_eq!(
                    unit.from_centimeters(parsed).to_bits(),
                    displayed.to_bits(),
                    "Field parser must retain {text} {} on focus loss",
                    unit.symbol()
                );
            }
        }
    }

    #[test]
    fn display_conversions_preserve_fractional_canonical_lengths() {
        assert_eq!(LengthUnit::default(), LengthUnit::Centimeters);
        for (unit, symbol, one_in_cm) in [
            (LengthUnit::Millimeters, "mm", 0.1),
            (LengthUnit::Centimeters, "cm", 1.0),
            (LengthUnit::Meters, "m", 100.0),
            (LengthUnit::Inches, "in", 2.54),
            (LengthUnit::Feet, "ft", 30.48),
        ] {
            assert_eq!(unit.symbol(), symbol);
            assert_eq!(unit.to_centimeters(1.0), one_in_cm);
            for centimeters in [-25.4, -0.1, 0.0, 0.1, 1.0, 12.375, 1000.0] {
                let restored = unit.to_centimeters(unit.from_centimeters(centimeters));
                assert!((restored - centimeters).abs() <= centimeters.abs().max(1.0) * 1e-14);
            }
        }
    }

    #[test]
    fn explicit_suffixes_override_the_display_unit_without_rounding_to_whole_centimeters() {
        for default in LengthUnit::ALL {
            for (text, expected) in [
                (".1cm", 0.1),
                ("1 mm", 0.1),
                (".001m", 0.1),
                ("1/10 cm", 0.1),
                ("1/2 in", 1.27),
                ("-1 / 4 FT", -7.62),
                ("  +2.5e-1 M  ", 25.0),
                ("1e2/2e1 mm", 0.5),
                ("2 CM", 2.0),
                ("0 in", 0.0),
            ] {
                let actual = parse_length(text, default).unwrap();
                assert!((actual - expected).abs() < 1e-12, "{text} with {default:?}");
            }
        }
    }

    #[test]
    fn unsuffixed_numbers_and_fractions_use_the_selected_display_unit() {
        for unit in LengthUnit::ALL {
            assert_eq!(parse_length(".5", unit), Some(unit.to_centimeters(0.5)));
            assert_eq!(parse_length("1 / 2", unit), Some(unit.to_centimeters(0.5)));
            assert_eq!(parse_length("1e-3", unit), Some(unit.to_centimeters(0.001)));
            assert_eq!(parse_length("-3/2", unit), Some(unit.to_centimeters(-1.5)));
        }
    }

    #[test]
    fn malformed_nonfinite_and_overflowing_lengths_are_rejected() {
        for text in [
            "",
            " ",
            "cm",
            "m",
            "1e",
            "1e+",
            ".",
            "1/0",
            "1/-0",
            "/2",
            "1/",
            "1/2/3",
            "1 1/2",
            "1 + 2",
            "1cm/2",
            "1 cm cm",
            "1e309",
            "1e308 m",
            "1e308/1e-308",
            "NaN",
            "NaN cm",
            "inf",
            "-inf ft",
            "1/inf",
            "1yd",
            "1inch",
            "1µm",
            "½ cm",
        ] {
            assert_eq!(parse_length(text, LengthUnit::Centimeters), None, "{text}");
        }
    }

    #[test]
    fn canonical_serialization_accepts_only_centimeters() {
        assert_eq!(
            serde_json::to_string(&CanonicalLengthUnit::default()).unwrap(),
            "\"cm\""
        );
        assert_eq!(
            serde_json::from_str::<CanonicalLengthUnit>("\"cm\"").unwrap(),
            CanonicalLengthUnit::Centimeters
        );
        for unsupported in ["mm", "m", "in", "ft", "CM"] {
            assert!(serde_json::from_value::<CanonicalLengthUnit>(unsupported.into()).is_err());
        }
    }
}
