//! Decimal input grammar for a transient transform, independent of hotkeys,
//! topology, document units, and history. Prefixes remain editable but cannot
//! be accepted as completed values.

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParsedNumber {
    Empty,
    Prefix,
    Value(f64),
    Invalid,
}

pub fn accepts(character: char) -> bool {
    character.is_ascii_digit() || matches!(character, '+' | '-' | '.')
}

pub fn parse(text: &str) -> ParsedNumber {
    if text.is_empty() {
        return ParsedNumber::Empty;
    }
    if matches!(text, "+" | "-" | "." | "+." | "-.") {
        return ParsedNumber::Prefix;
    }
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    let mut decimal = false;
    for character in digits.chars() {
        if character == '.' && !decimal {
            decimal = true;
        } else if !character.is_ascii_digit() {
            return ParsedNumber::Invalid;
        }
    }
    match text.parse::<f64>() {
        Ok(value) if value.is_finite() => ParsedNumber::Value(value),
        _ => ParsedNumber::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_values_and_incomplete_prefixes_are_distinct() {
        assert_eq!(parse(""), ParsedNumber::Empty);
        for text in ["-", "+", ".", "-.", "+."] {
            assert_eq!(parse(text), ParsedNumber::Prefix);
        }
        for (text, value) in [("12", 12.0), ("-0.25", -0.25), ("+.5", 0.5), ("1.", 1.0)] {
            assert_eq!(parse(text), ParsedNumber::Value(value));
        }
    }

    #[test]
    fn malformed_and_nonfinite_values_cannot_be_committed() {
        for text in ["1..2", "--1", "1-2", "NaN", "inf", "1e3", " 1", "1 cm"] {
            assert_eq!(parse(text), ParsedNumber::Invalid);
        }
        assert_eq!(parse(&"9".repeat(400)), ParsedNumber::Invalid);
        assert!(!accepts('e'));
        assert!(!accepts('１'));
        assert!(accepts('.'));
    }
}
