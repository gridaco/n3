//! Preserve an authored source while editing its evaluated value.
//!
//! Evaluation alone does not replace a recipe, generator, or other source with
//! its output. The caller captures both at entry, then resolves each valid edit:
//! output equal to the captured baseline retains the original source; changed
//! output becomes an explicit edited value. Returning to the baseline restores
//! the original source configuration without trying to infer it from the value.
//!
//! This provenance may span several short history transactions. It owns neither
//! working state nor history, and does not validate, mutate, or publish values.
//! Those responsibilities remain with the caller's existing edit lifecycle.

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedEdit<Source, Value> {
    source: Source,
    baseline: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DerivedValue<Source, Value> {
    Source(Source),
    Edited(Value),
}

impl<Source, Value> DerivedEdit<Source, Value> {
    pub fn new(source: Source, baseline: Value) -> Self {
        Self { source, baseline }
    }

    pub fn source(&self) -> &Source {
        &self.source
    }

    pub fn baseline(&self) -> &Value {
        &self.baseline
    }
}

impl<Source: Clone, Value: PartialEq> DerivedEdit<Source, Value> {
    /// Select a representation using the value's exact `PartialEq` contract.
    /// There is no tolerance or recognition of an unrelated equivalent source.
    /// Resolution does not consume or advance the captured baseline, allowing
    /// the caller to resolve previews, reversals, and restored history alike.
    pub fn resolve(&self, value: Value) -> DerivedValue<Source, Value> {
        if value == self.baseline {
            DerivedValue::Source(self.source.clone())
        } else {
            DerivedValue::Edited(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DerivedEdit, DerivedValue};

    #[derive(Clone, Debug, PartialEq)]
    struct TextRecipe {
        pattern: String,
        repetitions: usize,
    }

    fn repeated_text() -> DerivedEdit<TextRecipe, String> {
        let source = TextRecipe {
            pattern: "ab".into(),
            repetitions: 2,
        };
        let baseline = source.pattern.repeat(source.repetitions);
        DerivedEdit::new(source, baseline)
    }

    #[test]
    fn evaluating_without_edits_retains_the_original_source() {
        let edit = repeated_text();
        assert_eq!(edit.baseline(), "abab");
        assert_eq!(
            edit.resolve(edit.baseline().clone()),
            DerivedValue::Source(edit.source().clone())
        );
    }

    #[test]
    fn changed_output_is_an_explicit_value_without_altering_the_baseline() {
        let edit = repeated_text();
        let baseline = edit.clone();
        for value in ["abax", "abab!", ""] {
            assert_eq!(
                edit.resolve(value.into()),
                DerivedValue::Edited(value.to_owned())
            );
            assert_eq!(edit, baseline);
        }
    }

    #[test]
    fn returning_to_baseline_recovers_the_exact_authored_configuration() {
        let edit = repeated_text();
        assert!(matches!(
            edit.resolve("changed".into()),
            DerivedValue::Edited(_)
        ));
        assert_eq!(
            edit.resolve("abab".into()),
            DerivedValue::Source(TextRecipe {
                pattern: "ab".into(),
                repetitions: 2,
            })
        );
        // Another recipe could generate the same value, but the captured source
        // survives instead of being reconstructed as "abab" repeated once.
        assert_eq!(edit.source().repetitions, 2);
    }

    #[test]
    fn small_numeric_changes_are_not_treated_as_unchanged() {
        let edit = DerivedEdit::new("authored number", 1.0_f64);
        let adjacent = f64::from_bits(1.0_f64.to_bits() + 1);
        assert_eq!(edit.resolve(adjacent), DerivedValue::Edited(adjacent));
        assert_eq!(edit.resolve(1.0), DerivedValue::Source("authored number"));
    }
}
