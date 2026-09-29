//! Action-independent snapshot history with one optional transaction.
//!
//! The caller owns the working value and validates each preview before applying
//! it. Starting a transaction retains only its baseline; previews never enter
//! undo history. Accepting records one change, while cancelling returns the
//! baseline for the caller to restore. The same lifecycle fits a transform, a
//! color picker, or another editor without knowledge of its widgets or actions:
//!
//! ```text
//! let mut color = (255_u8, 0_u8, 0_u8);
//! let mut history = EditHistory::new(64);
//! history.begin_transaction(color);
//! color = (200, 30, 40); // validated live preview from the picker
//! color = (180, 50, 60); // another preview, still no undo entry
//! history.commit_transaction(&color); // Accept: one undo entry
//! color = history.undo(color).unwrap(); // original red
//! ```
//!
//! Equivalent UI callbacks call `begin_transaction` on opening, update their
//! working value during preview, and call `commit_transaction` or restore the
//! value returned by `cancel_transaction` on acceptance or cancellation.

#[derive(Debug)]
struct EditTransaction<T> {
    before: T,
}

#[derive(Debug)]
pub struct EditHistory<T: Clone + PartialEq> {
    limit: usize,
    undo: Vec<T>,
    redo: Vec<T>,
    transaction: Option<EditTransaction<T>>,
}

impl<T: Clone + PartialEq> EditHistory<T> {
    /// Retain at most `limit` undo snapshots. Zero disables undo retention but
    /// preserves transaction cancellation and reports accepted changes normally.
    pub fn new(limit: usize) -> Self {
        Self {
            limit,
            undo: Vec::new(),
            redo: Vec::new(),
            transaction: None,
        }
    }

    pub fn has_transaction(&self) -> bool {
        self.transaction.is_some()
    }

    /// Inspect the immutable baseline when the caller needs to compare a subset
    /// of its snapshot, such as document content without transient selection.
    pub fn transaction_baseline(&self) -> Option<&T> {
        self.transaction
            .as_ref()
            .map(|transaction| &transaction.before)
    }

    /// Start a transaction unless one is already open. A nested attempt returns
    /// false and cannot replace the original cancellation baseline.
    pub fn begin_transaction(&mut self, before: T) -> bool {
        if self.has_transaction() {
            return false;
        }
        self.transaction = Some(EditTransaction { before });
        true
    }

    /// Finish the transaction, returning whether its value changed. An unchanged
    /// acceptance creates no undo entry and preserves an existing redo branch.
    pub fn commit_transaction(&mut self, current: &T) -> bool {
        let Some(transaction) = self.transaction.take() else {
            return false;
        };
        self.record(transaction.before, current)
    }

    /// Return the baseline without changing either history stack. The caller
    /// restores this value into its working state and refreshes derived views.
    pub fn cancel_transaction(&mut self) -> Option<T> {
        self.transaction
            .take()
            .map(|transaction| transaction.before)
    }

    /// Record a validated, immediate change. An open transaction owns history,
    /// so recording during one returns false without changing either stack.
    pub fn record(&mut self, before: T, current: &T) -> bool {
        if self.has_transaction() || before == *current {
            return false;
        }
        self.push_undo(before);
        self.clear_redo();
        true
    }

    /// Invalidate redo when a caller coalesces a repeated immediate edit into
    /// its preceding undo entry. An open transaction keeps its redo branch;
    /// calls during previews are ignored until a changed acceptance occurs.
    pub fn clear_redo(&mut self) {
        if !self.has_transaction() {
            self.redo.clear();
        }
    }

    /// Cancel an open transaction first. Otherwise, return the previous value
    /// and retain the caller's current value for redo.
    pub fn undo(&mut self, current: T) -> Option<T> {
        if let Some(before) = self.cancel_transaction() {
            return Some(before);
        }
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// Cancel an open transaction first. Otherwise, return the next value and
    /// retain the caller's current value for undo.
    pub fn redo(&mut self, current: T) -> Option<T> {
        if let Some(before) = self.cancel_transaction() {
            return Some(before);
        }
        let next = self.redo.pop()?;
        self.push_undo(current);
        Some(next)
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[cfg(test)]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    fn push_undo(&mut self, before: T) {
        if self.limit == 0 {
            return;
        }
        if self.undo.len() == self.limit {
            self.undo.remove(0);
        }
        self.undo.push(before);
    }
}

#[cfg(test)]
mod tests {
    use super::EditHistory;

    #[test]
    fn color_picker_previews_accept_as_one_undo_entry() {
        let original = (255_u8, 0_u8, 0_u8);
        let mut color = original;
        let mut history = EditHistory::new(64);
        assert!(history.begin_transaction(color));
        for preview in [(230, 20, 30), (200, 30, 40), (180, 50, 60)] {
            color = preview;
            assert!(history.has_transaction());
            assert_eq!((history.undo_len(), history.redo_len()), (0, 0));
        }
        assert!(history.commit_transaction(&color));
        assert!(!history.has_transaction());
        assert_eq!(history.undo_len(), 1);
        assert!(!history.commit_transaction(&color));
        let accepted = color;
        color = history.undo(color).unwrap();
        assert_eq!(color, original);
        assert_eq!(history.redo(color), Some(accepted));
    }

    #[test]
    fn cancellation_and_unchanged_acceptance_preserve_redo() {
        let mut history = EditHistory::new(4);
        assert!(history.record(10, &20));
        assert_eq!(history.undo(20), Some(10));
        assert!(history.begin_transaction(10));
        history.clear_redo();
        assert_eq!(history.redo_len(), 1);
        assert_eq!(history.cancel_transaction(), Some(10));
        assert_eq!((history.undo_len(), history.redo_len()), (0, 1));
        assert!(history.begin_transaction(10));
        history.clear_redo();
        assert!(!history.commit_transaction(&10));
        assert!(!history.record(10, &10));
        assert_eq!((history.undo_len(), history.redo_len()), (0, 1));
        assert_eq!(history.redo(10), Some(20));
        assert_eq!(history.cancel_transaction(), None);
    }

    #[test]
    fn nested_begin_and_immediate_record_cannot_replace_an_active_baseline() {
        let mut history = EditHistory::new(4);
        assert!(history.record(1, &2));
        assert!(history.record(2, &3));
        assert_eq!(history.undo(3), Some(2));
        assert!(history.begin_transaction(2));
        assert!(!history.begin_transaction(40));
        assert_eq!(history.transaction_baseline(), Some(&2));
        assert!(!history.record(40, &80));
        assert_eq!((history.undo_len(), history.redo_len()), (1, 1));
        assert!(history.commit_transaction(&50));
        assert_eq!((history.undo_len(), history.redo_len()), (2, 0));
        assert_eq!(history.undo(50), Some(2));
        assert_eq!(history.undo(2), Some(1));
    }

    #[test]
    fn undo_and_redo_cancel_an_active_transaction_before_traversing_history() {
        for use_redo in [false, true] {
            let mut history = EditHistory::new(4);
            history.record(1, &2);
            history.record(2, &3);
            assert_eq!(history.undo(3), Some(2));
            history.begin_transaction(2);
            let restored = if use_redo {
                history.redo(99)
            } else {
                history.undo(99)
            };
            assert_eq!(restored, Some(2));
            assert!(!history.has_transaction());
            assert_eq!((history.undo_len(), history.redo_len()), (1, 1));
            assert_eq!(history.redo(2), Some(3));
            assert_eq!(history.undo(3), Some(2));
            assert_eq!(history.undo(2), Some(1));
        }
    }

    #[test]
    fn history_is_bounded_and_new_edits_clear_only_the_redo_branch() {
        let mut history = EditHistory::new(2);
        for before in 0..5 {
            assert!(history.record(before, &(before + 1)));
            assert!(history.undo_len() <= 2);
        }
        assert_eq!(history.undo(5), Some(4));
        assert_eq!(history.undo(4), Some(3));
        assert_eq!(history.undo(3), None);
        assert_eq!(history.redo(3), Some(4));
        history.clear_redo();
        assert_eq!(history.redo(4), None);
        assert_eq!(history.undo(4), Some(3));
        assert!(history.record(3, &10));
        assert_eq!(history.redo_len(), 0);
        assert_eq!(history.undo(10), Some(3));
    }

    #[test]
    fn zero_capacity_retains_transaction_cancellation_without_undo_snapshots() {
        let mut history = EditHistory::new(0);
        assert!(history.record(1, &2));
        assert_eq!(history.undo(2), None);
        assert!(history.begin_transaction(2));
        assert_eq!(history.cancel_transaction(), Some(2));
        assert!(history.begin_transaction(2));
        assert!(history.commit_transaction(&3));
        assert_eq!((history.undo_len(), history.redo_len()), (0, 0));
        assert_eq!(history.undo(3), None);
        assert_eq!(history.redo(3), None);
    }
}
