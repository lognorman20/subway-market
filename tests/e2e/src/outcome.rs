//! Market resolution rules mirrored from domain (exact-on-time → early).

/// Half WAD — fresh LMSR prices both sides at 50%.
pub const HALF_WAD: &str = "500000000000000000";

/// Arrival ≤ frozen_eta → early; strictly after → late. Never void / on-time.
pub fn outcome_from_arrival(actual_arrival_unix: i64, frozen_eta_unix: i64) -> &'static str {
    if actual_arrival_unix <= frozen_eta_unix {
        "early"
    } else {
        "late"
    }
}

/// Resolve outcomes allowed by product rules.
pub fn is_valid_resolve_outcome(outcome: &str) -> bool {
    matches!(outcome, "early" | "late")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_schedule_is_early() {
        assert_eq!(outcome_from_arrival(1_700_000_100, 1_700_000_100), "early");
    }

    #[test]
    fn before_schedule_is_early() {
        assert_eq!(outcome_from_arrival(1_700_000_050, 1_700_000_100), "early");
    }

    #[test]
    fn after_schedule_is_late() {
        assert_eq!(outcome_from_arrival(1_700_000_101, 1_700_000_100), "late");
    }

    #[test]
    fn never_void_or_on_time() {
        assert!(!is_valid_resolve_outcome("on_time"));
        assert!(!is_valid_resolve_outcome("void"));
        assert!(!is_valid_resolve_outcome("unresolved"));
        assert!(is_valid_resolve_outcome("early"));
        assert!(is_valid_resolve_outcome("late"));
    }
}
