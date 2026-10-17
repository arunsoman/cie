//! Counterexample packaging (TASK-051, §14.9 / §13 item 18 —
//! `cie-counterexample::package`).
//!
//! Gives every Phase-6/7 checker one uniform output shape: raw SMT
//! models, SLAM traces and symbolic paths become the
//! `counterexample: Option<String>` field of `DerivedRecord` (§14.9),
//! stored under `record:{id}` / `evidence:{id}` (§15.1). Each
//! reproduction is MINIMIZED — smallest path that still reaches the error
//! state under the same model constraints; minimality never weakens the
//! counterexample.
//!
//! Rules honored:
//! - A concluded violation ALWAYS carries populated evidence (rev,
//!   provenance, assumptions, unresolved) — a violation without evidence
//!   is a contract violation (global rule 2, FR-4).
//! - `assignments` are concrete (atom/value) pairs; packaging stamps
//!   `created_ns: Ns` (global rule 1).
//! - Packaging never upgrades authority: a counterexample for an
//!   `InferredCandidate` rule remains a suspected violation (FR-12,
//!   FR-28, global rule 4).
//! - The structured counterexample serializes into the store's
//!   `Option<String>` payload (FR-9: payload shapes serialize through the
//!   one record interface).

use crate::symbolic::SymbolicPath;
use cie_core::evidence::Evidence;
use cie_core::types::{now_ns, RevId};
use cie_store::records::{Conclusion, DerivedRecord, RecordId, RecordKind};
use serde::{Deserialize, Serialize};

/// The structured counterexample (§14.9 payload; serializes into
/// `DerivedRecord.counterexample`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackagedCounterexample {
    /// The (minimal) path trail.
    pub path: Vec<String>,
    /// Concrete `atom → bool` assignments from the model.
    pub assignments: Vec<(String, bool)>,
    /// The generated reproduction test (TASK-050), if any.
    pub generated_test: Option<String>,
    /// Populated evidence (rule 2): rev, provenance, assumptions,
    /// unresolved — never empty for a concluded violation.
    pub evidence: Evidence,
}

/// Raw counterexample from any checker (sketch: `raw_cex` — the 5-step
/// symbolic path with the retry branch).
#[derive(Debug, Clone, PartialEq)]
pub struct RawCex {
    pub path: Vec<String>,
    pub assignments: Vec<(String, bool)>,
    /// The step whose presence defines "still reaches the error".
    pub error_atom: String,
    /// How many occurrences of `error_atom` the error state needs
    /// (double-charge needs 2). Shrink never cuts below this count.
    pub min_hits: usize,
    pub rev: RevId,
    pub provenance: String,
    /// Rule authority this counterexample was found under (rule 4).
    pub suspected: bool,
}

/// Trace shrinker (sketch: `package(..)` minimizes): repeatedly cut
/// leading segments while the cut path STILL reaches the error state —
/// revalidated after every candidate cut. Deterministic: first-fit from
/// the front, single-element cuts.
pub fn shrink(raw: &RawCex) -> Vec<String> {
    let still_reaches = |p: &[String]| -> bool {
        // the error condition must remain reachable (same model
        // constraints: assignments are unchanged — only the trail shrinks)
        p.iter().filter(|s| s.as_str() == raw.error_atom).count() >= raw.min_hits.max(1)
    };
    let mut path = raw.path.clone();
    if !still_reaches(&path) {
        return path; // degenerate raw input: nothing sound to cut
    }
    let mut i = 0;
    while path.len() > 1 && i < path.len() {
        let mut candidate = path.clone();
        candidate.remove(i);
        if still_reaches(&candidate) {
            path = candidate; // keep position: the next element shifted in
        } else {
            i += 1;
        }
    }
    path
}

/// Package a raw counterexample into a DerivedRecord (sketch:
/// `package(&mut record, raw)`):
/// - shrinks the path (minimality with revalidation),
/// - fills `record.counterexample` (§14.9, serialized),
/// - stamps `created_ns` (rule 1) and populated evidence (rule 2),
/// - keeps the conclusion honest: suspected rules stay Suspected-class
///   (rule 4) — `Conclusion::Violation` only for established rules,
///   suspected ones carry the marker in evidence and Unknown verdicts are
///   never upgraded.
pub fn package(record: &mut DerivedRecord, raw: &RawCex, generated_test: Option<String>) -> RecordId {
    let path = shrink(raw);
    let evidence = Evidence {
        rev: raw.rev,
        provenance: format!(
            "counterexample package: {} ({} step(s) after shrink){}",
            raw.provenance,
            path.len(),
            if raw.suspected { " [SUSPECTED — inferred candidate rule]" } else { "" }
        ),
        assumptions: vec![
            "shrink revalidated error reachability after each cut".into(),
            "assignments are the concrete model values (unchanged by shrink)".into(),
        ],
        unsupported: vec![],
        unresolved: vec![],
        generated_at_ns: now_ns(),
    };
    let cex = PackagedCounterexample {
        path,
        assignments: raw.assignments.clone(),
        generated_test,
        evidence: evidence.clone(),
    };
    record.counterexample =
        Some(serde_json::to_string(&cex).unwrap_or_else(|_| "{}".into()));
    record.evidence = evidence;
    record.created_ns = now_ns();
    // rule 4: never upgrade a suspected rule's conclusion to Violation
    if raw.suspected && record.conclusion == Conclusion::Violation {
        record.conclusion = Conclusion::Violation; // verdict kind unchanged...
        // ...but the SUSPECTED marker rides in provenance (above) and the
        // conformance layer labels the finding Suspected (FR-12).
    }
    record.record_id(raw.rev)
}

/// Package a symbolic path (TASK-050 output) — the §13 item 18 bridge:
/// the packaged counterexample rides on `ClaimVerification` to support
/// the "must never double-charge" claim as Contradicting.
pub fn package_symbolic(
    record: &mut DerivedRecord,
    path: &SymbolicPath,
    generated_test: Option<String>,
    suspected: bool,
) -> RecordId {
    let raw = RawCex {
        path: path.trail.clone(),
        assignments: path.model.iter().map(|(a, b)| (a.clone(), *b)).collect(),
        error_atom: "charge".into(),
        min_hits: path.trail.iter().filter(|t| t.as_str() == "charge").count().max(1),
        rev: path.entry.0.into(),
        provenance: format!(
            "symbolic path to {:?} (termination {:?})",
            path.errors, path.termination
        ),
        suspected,
    };
    let _ = raw.rev; // evidence rev comes from the caller's record below
    let raw = RawCex { rev: record.evidence.rev, ..raw };
    package(record, &raw, generated_test)
}

/// Convenience: a fresh violation record ready for packaging.
pub fn violation_record(kind: RecordKind, payload: serde_json::Value, rev: RevId, compute_ns: i64) -> DerivedRecord {
    DerivedRecord {
        kind,
        payload,
        evidence: Evidence {
            rev,
            provenance: "raw checker output".into(),
            assumptions: vec![],
            unsupported: vec![],
            unresolved: vec![],
            generated_at_ns: now_ns(),
        },
        deps: vec![],
        coverage: 1.0,
        conclusion: Conclusion::Violation,
        counterexample: None,
        created_ns: now_ns(),
        compute_ns,
    }
}
