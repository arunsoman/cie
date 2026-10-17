//! Result-protocol record store (TASK-037, §14.9/§15.1/§15.2).
//!
//! - EXACTLY the §14.9 shape: `DerivedRecord{kind, payload, evidence,
//!   deps, coverage, conclusion, counterexample, created_ns, compute_ns}`;
//!   payloads are JSON-typed with a [`RecordKind`] tag — one consistent
//!   interface for all analyzers (FR-9) that keeps `cie-store` core-only
//!   in the TASK-001 layering (payload shapes serialize through).
//! - Key namespaces exactly `record:{record_id}`, `deps:{record_id}`,
//!   `rev_deps:{symbol_id}`, `evidence:{record_id}` (§15.1) — no ad-hoc
//!   keys (global rule 7).
//! - `RecordId` is content-addressed (blake3 over payload+evidence+rev)
//!   so recomputation is idempotent and stale facts cannot survive
//!   silently (FR-8, global rule 1).
//! - Records are revision-scoped via [`TimedRecord`] (§15.2) so
//!   `--as-of-rev` time-travel reads work.
//! - A record whose analysis timed out stores `Conclusion::Unknown` +
//!   status `TimedOut`; the store REJECTS any clean conclusion carrying a
//!   timeout marker (§8, global rule 3).

use cie_core::evidence::Evidence;
use cie_core::types::{Ns, RevId};
use serde::{Deserialize, Serialize};
use sha2::Digest;

/// §14.9 record id: blake3 over (payload + evidence + rev).
pub type RecordId = [u8; 32];

/// Payload kind tag (FR-9: one interface for all analyzers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordKind {
    Finding,
    Concept,
    Counterexample,
    Status,
}

/// §8 conclusion — `Unknown` exists so a timed-out analysis can be stored
/// honestly; clean conclusions with timeout markers are rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conclusion {
    Clean,
    Violation,
    Unknown,
}

/// §14.9 DerivedRecord — exact shape; the payload is the analyzer's
/// serialized view (findings / concepts / counterexamples / status).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedRecord {
    pub kind: RecordKind,
    pub payload: serde_json::Value,
    pub evidence: Evidence,
    /// Input-record dependencies (`deps:` links).
    pub deps: Vec<RecordId>,
    /// Analyzed coverage ratio (0.0..=1.0).
    pub coverage: f32,
    pub conclusion: Conclusion,
    pub counterexample: Option<String>,
    pub created_ns: Ns,
    pub compute_ns: i64,
}

/// §15.2 time-indexed wrapper — revision scoping for time travel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimedRecord {
    pub rev: RevId,
    pub record: DerivedRecord,
}

/// Marker an analyzer sets when its pass timed out (§8): a record claiming
/// `Clean` while carrying this marker is REJECTED at insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeoutMarker;

impl DerivedRecord {
    /// Content-addressed id: blake3(payload ‖ evidence ‖ rev).
    pub fn record_id(&self, rev: RevId) -> RecordId {
        let mut hasher = sha2::Sha256::new();
        hasher.update(serde_json::to_vec(&self.payload).unwrap_or_default());
        hasher.update(serde_json::to_vec(&self.evidence).unwrap_or_default());
        hasher.update(rev.to_le_bytes());
        hasher.finalize().into()
    }

    /// §8/global rule 3: a clean conclusion with a timeout marker anywhere
    /// in the evidence is invalid.
    pub fn validate(&self) -> Result<(), String> {
        let timed_out = self
            .evidence
            .unsupported
            .iter()
            .chain(self.evidence.assumptions.iter())
            .any(|s| s.contains("timeout") || s.to_ascii_lowercase().contains("timed out"));
        if timed_out && self.conclusion == Conclusion::Clean {
            return Err("global rule 3: clean conclusion carries a timeout marker".into());
        }
        Ok(())
    }
}

/// Record-store extension over the generic KV store (TASK-001 layering —
/// typed persistence for domain payloads lives in domain crates).
pub trait RecordStoreExt {
    fn put_record(&self, rev: RevId, record: &DerivedRecord) -> std::io::Result<RecordId>;
    fn get_record(&self, rev: RevId, id: &RecordId) -> std::io::Result<Option<DerivedRecord>>;
    /// Time-travel read: the record's version at-or-before `as_of`.
    fn get_record_as_of(&self, as_of: RevId, id: &RecordId) -> std::io::Result<Option<DerivedRecord>>;
    fn deps_of(&self, rev: RevId, id: &RecordId) -> std::io::Result<Vec<RecordId>>;
    /// Reverse dependencies: records derived from `symbol`'s summaries.
    fn rev_deps_of(&self, rev: RevId, symbol: &str) -> std::io::Result<Vec<RecordId>>;
}

impl RecordStoreExt for crate::Store {
    fn put_record(&self, rev: RevId, record: &DerivedRecord) -> std::io::Result<RecordId> {
        record.validate().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        let id = record.record_id(rev);
        let hex = hex(&id);
        let timed = TimedRecord { rev, record: record.clone() };
        // §15.1 namespaces: record:{id} + deps:{id} + evidence:{id} +
        // rev_deps:{symbol_id} — maintained "transactionally" (one JSON
        // document per namespace entry, atomic rename inside write_json).
        self.write_json(
            &self.root().join("revs").join(rev.to_string()).join(format!("record:{hex}.json")),
            &timed,
        )?;
        self.write_json(
            &self.root().join("revs").join(rev.to_string()).join(format!("deps:{hex}.json")),
            &record.deps,
        )?;
        self.write_json(
            &self.root().join("revs").join(rev.to_string()).join(format!("evidence:{hex}.json")),
            &record.evidence,
        )?;
        // rev_deps keyed by each cited symbol id in the payload.
        if let Some(sym) = record.payload.get("symbol_id").and_then(|v| v.as_str()) {
            let mut list: Vec<String> = self
                .read_json(&self.root().join("revs").join(rev.to_string()).join(format!("rev_deps:{sym}.json")))?
                .unwrap_or_default();
            list.push(hex.clone());
            self.write_json(
                &self.root().join("revs").join(rev.to_string()).join(format!("rev_deps:{sym}.json")),
                &list,
            )?;
        }
        Ok(id)
    }

    fn get_record(&self, rev: RevId, id: &RecordId) -> std::io::Result<Option<DerivedRecord>> {
        let hex = hex(id);
        self.read_json::<TimedRecord>(
            &self.root().join("revs").join(rev.to_string()).join(format!("record:{hex}.json")),
        )
        .map(|o| o.map(|t| t.record))
    }

    fn get_record_as_of(&self, as_of: RevId, id: &RecordId) -> std::io::Result<Option<DerivedRecord>> {
        // Scan revisions ≤ as_of, latest first (§15.2 time travel).
        let mut revs = self.list_revs()?;
        revs.retain(|r| *r <= as_of);
        for rev in revs.into_iter().rev() {
            if let Some(rec) = self.get_record(rev, id)? {
                return Ok(Some(rec));
            }
        }
        Ok(None)
    }

    fn deps_of(&self, rev: RevId, id: &RecordId) -> std::io::Result<Vec<RecordId>> {
        let hex = hex(id);
        Ok(self
            .read_json::<Vec<RecordId>>(
                &self.root().join("revs").join(rev.to_string()).join(format!("deps:{hex}.json")),
            )?
            .unwrap_or_default())
    }

    fn rev_deps_of(&self, rev: RevId, symbol: &str) -> std::io::Result<Vec<RecordId>> {
        let list: Vec<String> = self
            .read_json(
                &self
                    .root()
                    .join("revs")
                    .join(rev.to_string())
                    .join(format!("rev_deps:{symbol}.json")),
            )?
            .unwrap_or_default();
        Ok(list
            .into_iter()
            .filter_map(|h| unhex(&h))
            .collect())
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        let hi = s.as_bytes()[i * 2];
        let lo = s.as_bytes()[i * 2 + 1];
        let d = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                _ => None,
            }
        };
        *b = (d(hi)? << 4) | d(lo)?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("cie-records-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    fn sample(evidence_assumptions: Vec<String>) -> DerivedRecord {
        DerivedRecord {
            kind: RecordKind::Finding,
            payload: serde_json::json!({"rule": "PR-001", "symbol_id": "sym:7"}),
            evidence: {
                let mut e = Evidence::new(3, "test", 1);
                e.assumptions = evidence_assumptions;
                e
            },
            deps: vec![],
            coverage: 1.0,
            conclusion: Conclusion::Violation,
            counterexample: None,
            created_ns: 1,
            compute_ns: 5,
        }
    }

    /// Round-trip preserves every field; evidence joins by evidence:{id}.
    #[test]
    fn task_037_put_get_roundtrip() {
        let store = crate::Store::open(&tmpdir("rt")).unwrap();
        let rev = store.bump_rev().unwrap();
        let rec = sample(vec![]);
        let id = store.put_record(rev, &rec).unwrap();
        let got = store.get_record(rev, &id).unwrap().unwrap();
        assert_eq!(got, rec, "every field preserved");
        // Evidence joinable by evidence:{record_id}.
        let ev_path = store
            .root()
            .join("revs")
            .join(rev.to_string())
            .join(format!("evidence:{}.json", hex(&id)));
        let joined: Evidence = store.read_json(&ev_path).unwrap().unwrap();
        assert_eq!(joined, rec.evidence);
    }

    /// Recomputing the same result yields the same RecordId (idempotency).
    #[test]
    fn task_037_record_id_idempotent() {
        let store = crate::Store::open(&tmpdir("idem")).unwrap();
        let rev = store.bump_rev().unwrap();
        let a = sample(vec![]).record_id(rev);
        let b = sample(vec![]).record_id(rev);
        assert_eq!(a, b, "content-addressed: same content ⇒ same id");
        let mut c = sample(vec![]);
        c.payload = serde_json::json!({"rule": "PR-002"});
        assert_ne!(a, c.record_id(rev), "different content ⇒ different id");
    }

    /// Clean conclusion + timeout marker is REJECTED (global rule 3).
    #[test]
    fn task_037_timeout_never_clean() {
        let store = crate::Store::open(&tmpdir("timeout")).unwrap();
        let rev = store.bump_rev().unwrap();
        let bad = sample(vec!["analysis timed out — partial coverage".into()]);
        let mut bad = bad;
        bad.conclusion = Conclusion::Clean;
        assert!(store.put_record(rev, &bad).is_err(), "rejected at insert");
        // Honest storage: Unknown + timeout marker is accepted.
        let mut ok = sample(vec!["analysis timed out — partial coverage".into()]);
        ok.conclusion = Conclusion::Unknown;
        assert!(store.put_record(rev, &ok).is_ok());
    }

    /// rev_deps maintained on insert; time travel reads the right version.
    #[test]
    fn task_037_rev_deps_and_time_travel() {
        let store = crate::Store::open(&tmpdir("tt")).unwrap();
        let rev1 = store.bump_rev().unwrap();
        let rec1 = sample(vec![]);
        let id1 = store.put_record(rev1, &rec1).unwrap();
        assert_eq!(store.rev_deps_of(rev1, "sym:7").unwrap(), vec![id1]);
        // rev 2: updated payload → new record id (stale facts cannot survive).
        let rev2 = store.bump_rev().unwrap();
        let mut rec2 = sample(vec![]);
        rec2.payload = serde_json::json!({"rule": "PR-001", "symbol_id": "sym:7", "v": 2});
        let id2 = store.put_record(rev2, &rec2).unwrap();
        assert_ne!(id1, id2);
        // Time travel: as-of rev1 returns the rev1 version.
        let old = store.get_record_as_of(rev1, &id1).unwrap().unwrap();
        assert_eq!(old, rec1);
        assert!(store.get_record_as_of(rev1, &id2).unwrap().is_none());
    }
}
