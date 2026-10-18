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
pub mod s3_viz;
pub mod s4_catalog;
pub mod s5_concepts;
pub mod s6_arch;
pub mod s7_investigations;
pub mod s8_runtime;
pub mod s9_delivery;
pub mod s10_honesty;
pub mod s11_closes;

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
        "US-009" => s11_closes::us_009,
        "US-010" => s11_closes::us_010,
        "US-024" => s11_closes::us_024,
        // US-012: model/product-layer parts — audit only.
        // ---- Interactive visual exploration ----
        "US-018" => s3_viz::us_018,
        "US-019" => s3_viz::us_019,
        "US-021" => s3_viz::us_021,
        "US-025" => s3_viz::us_025,
        "US-026" => s3_viz::us_026,
        "US-027" => s3_viz::us_027,
        "US-029" => s3_viz::us_029,
        "US-030" => s3_viz::us_030,
        "US-022" => s11_closes::us_022,
        "US-023" => s11_closes::us_023,
        "US-028" => s11_closes::us_028,
        // US-020: audit-only.
        // ---- The visual catalogue ----
        "US-031" => s4_catalog::us_031,
        "US-032" => s4_catalog::us_032,
        "US-033" => s4_catalog::us_033,
        "US-034" => s4_catalog::us_034,
        "US-035" => s4_catalog::us_035,
        "US-036" => s4_catalog::us_036,
        "US-038" => s4_catalog::us_038,
        "US-040" => s4_catalog::us_040,
        "US-042" => s4_catalog::us_042,
        "US-044" => s4_catalog::us_044,
        "US-045" => s4_catalog::us_045,
        "US-049" => s4_catalog::us_049,
        "US-037" => s11_closes::us_037,
        "US-039" => s11_closes::us_039,
        "US-046" => s11_closes::us_046,
        // US-032/034/040 keep their s4_catalog checks.
        // US-041/043/047/048: evidence is the named suite — audit-only.
        // ---- Behaviour, concepts and business rules ----
        "US-050" => s5_concepts::us_050,
        "US-051" => s5_concepts::us_051,
        "US-053" => s5_concepts::us_053,
        "US-054" => s5_concepts::us_054,
        "US-055" => s5_concepts::us_055,
        "US-056" => s5_concepts::us_056,
        "US-052" => s11_closes::us_052,
        // (US-052 closed: endpoint extraction ships)
        // ---- Architecture, dependencies and impact ----
        "US-057" => s6_arch::us_057,
        "US-058" => s6_arch::us_058,
        "US-060" => s6_arch::us_060,
        "US-059" => s11_closes::us_059,
        // US-060: s6_arch::us_060 (FCA separations); co-change covered by
        // the cie-ingest::churn suite — audit-only here.
        // US-061: audit; US-062/063: planned.
        // ---- Debugging and investigations ----
        "US-065" => s7_investigations::us_065,
        "US-067" => s7_investigations::us_067,
        "US-068" => s7_investigations::us_068,
        "US-072" => s7_investigations::us_072,
        "US-074" => s7_investigations::us_074,
        "US-064" => s11_closes::us_064,
        "US-066" => s11_closes::us_066,
        "US-069" => s11_closes::us_069,
        "US-071" => s11_closes::us_071,
        // US-070/073: audit-only (covered patterns). US-074:
        // s7_investigations::us_074 remains the check.
        // ---- Runtime, history, security, twin (engine subset) ----
        "US-089" => s8_runtime::us_089,
        "US-090" => s8_runtime::us_090,
        "US-091" => s8_runtime::us_091,
        "US-093" => s8_runtime::us_093,
        "US-094" => s8_runtime::us_094,
        "US-095" => s8_runtime::us_095,
        "US-097" => s8_runtime::us_097,
        "US-099" => s8_runtime::us_099,
        "US-103" => s8_runtime::us_103,
        "US-080" => s11_closes::us_080,
        "US-085" => s11_closes::us_085,
        "US-086" => s11_closes::us_086,
        "US-100" => s11_closes::us_100,
        "US-102" => s11_closes::us_102,
        "US-106" => s11_closes::us_106,
        // US-075..078, 081..084, 087/088, 092, 096, 098: planned or
        // audit-only (patterns covered by earlier checks). US-089/091/099
        // keep their s8_runtime checks.
        // ---- P2F / validation / delivery / editor-agent ----
        "US-107" => s9_delivery::us_107,
        "US-133" => s9_delivery::us_133,
        "US-134" => s9_delivery::us_134,
        "US-113" => s11_closes::us_113,
        "US-114" => s11_closes::us_114,
        "US-119" => s11_closes::us_119,
        "US-120" => s11_closes::us_120,
        "US-127" => s11_closes::us_127,
        "US-128" => s11_closes::us_128,
        "US-079" => s11_closes::us_079,
        // ---- Privacy, models, honest operation ----
        "US-135" => s10_honesty::us_135,
        "US-136" => s10_honesty::us_136,
        "US-137" => s10_honesty::us_137,
        "US-138" => s10_honesty::us_138,
        "US-139" => s10_honesty::us_139,
        "US-144" => s10_honesty::us_144,
        "US-145" => s10_honesty::us_145,
        "US-147" => s10_honesty::us_147,
        "US-140" => s11_closes::us_140,
        "US-141" => s11_closes::us_141,
        "US-142" => s11_closes::us_142,
        "US-143" => s11_closes::us_143,
        // US-104..105, 108..112, 115..118, 121..126, 129..132, 146:
        // planned/product/host-layer or audit-only (patterns covered by
        // earlier checks). US-106/134 keep their checks.
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
            // CIE-US closes batch: newly engine-checked stories
            "US-009", "US-010", "US-022", "US-023", "US-024", "US-028", "US-030",
            "US-037", "US-039", "US-046", "US-052", "US-059", "US-064", "US-066",
            "US-069", "US-071", "US-079", "US-080", "US-085", "US-086", "US-100",
            "US-102", "US-106", "US-113", "US-114", "US-119", "US-120", "US-127",
            "US-128", "US-140", "US-141", "US-142", "US-143",
        ] {
            assert!(deep_check(id).is_some(), "{id} should have a deep check");
        }
        for id in [
            "US-005", "US-007", "US-012", "US-020", "US-041", "US-043", "US-047",
            "US-048", "US-061", "US-062", "US-063", "US-070", "US-073", "US-075", "US-076",
            "US-077", "US-078", "US-081", "US-082", "US-083", "US-084", "US-087",
            "US-088", "US-092", "US-096", "US-098", "US-101",
            "US-104", "US-105", "US-108", "US-109", "US-110", "US-111", "US-112",
            "US-115", "US-116", "US-117", "US-118",
            "US-121", "US-122", "US-123", "US-124", "US-125", "US-126",
            "US-129", "US-130", "US-131", "US-132",
            "US-146",
        ] {
            assert!(deep_check(id).is_none(), "{id} has no deep check by design");
        }
    }
}
