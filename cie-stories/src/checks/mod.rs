//! Deep per-story checks (batch C: US-001..US-017).
//!
//! Each check is a plain `fn() -> Result<(), String>` that runs REAL engine
//! code against a generated fixture and asserts the story's engine-level
//! obligation. `deep_check(id)` dispatches; stories without a deep check
//! (Planned dispositions, model/product-layer parts) return `None` and stay
//! on registry-audit verdicts.

pub mod fixture;
pub mod s1_repo;
pub mod s2_answers;

/// Deep check registry — story id → check.
pub fn deep_check(id: &str) -> Option<Result<(), String>> {
    let f: fn() -> Result<(), String> = match id {
        // ---- Repository access and orientation ----
        "US-001" => s1_repo::us_001,
        "US-002" => s1_repo::us_002,
        "US-003" => s1_repo::us_003,
        "US-004" => s1_repo::us_004,
        "US-006" => s1_repo::us_006,
        // US-005, US-007: Planned — no engine obligation to check yet.
        // ---- Asking questions and working with answers ----
        "US-008" => s2_answers::us_008,
        "US-011" => s2_answers::us_011,
        "US-013" => s2_answers::us_013,
        "US-014" => s2_answers::us_014,
        "US-015" => s2_answers::us_015,
        "US-016" => s2_answers::us_016,
        "US-017" => s2_answers::us_017,
        // US-009, US-010, US-012: model/product-layer parts — audit only.
        _ => return None,
    };
    Some(std::panic::catch_unwind(f).unwrap_or_else(|p| {
        Err(format!("check panicked: {p:?}"))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_c_dispatch_table_complete() {
        for id in [
            "US-001", "US-002", "US-003", "US-004", "US-006", "US-008", "US-011", "US-013",
            "US-014", "US-015", "US-016", "US-017",
        ] {
            assert!(deep_check(id).is_some(), "{id} should have a deep check");
        }
        for id in ["US-005", "US-007", "US-009", "US-010", "US-012", "US-018"] {
            assert!(deep_check(id).is_none(), "{id} has no deep check by design");
        }
    }
}
