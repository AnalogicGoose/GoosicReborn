//! Queue edits preserve the playing entry; an undo snapshot belongs to one revision.
//! Kept to the same cases as the Swift shell's `QueueEditing.swift`.

pub const UNDO_WINDOW_SECONDS: f64 = 10.0;

pub fn retained_count(count: usize, current_index: usize) -> Option<usize> {
    (current_index < count).then(|| current_index + 1)
}

pub fn can_undo(expected_revision: u64, revision: u64, deadline: f64, now: f64) -> bool {
    expected_revision == revision && now.is_finite() && deadline.is_finite() && now < deadline
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_keeps_history_and_the_current_entry() {
        assert_eq!(retained_count(4, 1), Some(2));
        assert_eq!(retained_count(4, 3), Some(4));
        assert_eq!(retained_count(0, 0), None);
        assert_eq!(retained_count(4, 4), None);
    }

    #[test]
    fn undo_expires_or_is_invalidated_by_another_revision() {
        assert!(can_undo(7, 7, 20.0, 19.0));
        assert!(!can_undo(7, 8, 20.0, 19.0));
        assert!(!can_undo(7, 7, 20.0, 20.0));
        assert!(!can_undo(7, 7, 20.0, f64::NAN));
        assert!(!can_undo(7, 7, f64::INFINITY, 19.0));
    }
}
