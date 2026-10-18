//! Requirement-conflict ledger (CIE-US US-107 engine close).
//!
//! Drift detects contradictions between statements; what was missing is
//! the *management* structure: a conflict record with its source
//! statements, a concrete conflict scenario, resolution options, the
//! owning decision and its history — and resolving a conflict updates
//! the affected acceptance expectations (the ledger returns them; it
//! never silently rewrites them).
//!
//! Append-only discipline: raising, optioning, and resolving are events
//! with timestamps; history replays; a resolution has an owner.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One source statement of the conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    pub doc: String,
    pub text: String,
    /// Applicability scope (e.g. "checkout only").
    pub applicability: String,
}

/// A resolution option (recorded before the decision).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolutionOption {
    pub id: String,
    pub description: String,
}

/// The conflict record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub id: String,
    pub statements: Vec<Statement>,
    /// The concrete scenario where the statements conflict (US-107
    /// obligation: a scenario, not a vibe).
    pub scenario: String,
    pub options: Vec<ResolutionOption>,
    /// open | resolved
    pub status: ConflictStatus,
    /// Owner of the resolution (required to resolve).
    pub owner: Option<String>,
    pub resolution: Option<String>,
    /// History: every event with its timestamp (replayable).
    pub history: Vec<ConflictEvent>,
    /// Acceptance expectations this conflict blocks; returned on
    /// resolution so the caller can update them visibly.
    pub affected_expectations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictEvent {
    pub kind: EventKind,
    pub actor: String,
    pub at_ns: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Raised,
    OptionAdded,
    Resolved,
}

/// The append-only ledger.
#[derive(Debug, Clone, Default)]
pub struct ConflictLedger {
    records: BTreeMap<String, ConflictRecord>,
    seq: u64,
}

impl ConflictLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Raise a conflict (append). Returns the record id.
    pub fn raise(
        &mut self,
        statements: Vec<Statement>,
        scenario: &str,
        affected_expectations: Vec<String>,
        at_ns: u64,
    ) -> String {
        self.seq += 1;
        let id = format!("CF-{:04}", self.seq);
        let rec = ConflictRecord {
            id: id.clone(),
            statements,
            scenario: scenario.to_string(),
            options: Vec::new(),
            status: ConflictStatus::Open,
            owner: None,
            resolution: None,
            history: vec![ConflictEvent {
                kind: EventKind::Raised,
                actor: "drift-detector".into(),
                at_ns,
                detail: scenario.to_string(),
            }],
            affected_expectations,
        };
        self.records.insert(id.clone(), rec);
        id
    }

    /// Add a resolution option (append).
    pub fn add_option(&mut self, id: &str, description: &str, actor: &str, at_ns: u64) -> bool {
        match self.records.get_mut(id) {
            Some(r) if r.status == ConflictStatus::Open => {
                let oid = format!("{}-O{}", id, r.options.len() + 1);
                r.options.push(ResolutionOption {
                    id: oid,
                    description: description.into(),
                });
                r.history.push(ConflictEvent {
                    kind: EventKind::OptionAdded,
                    actor: actor.into(),
                    at_ns,
                    detail: description.into(),
                });
                true
            }
            _ => false,
        }
    }

    /// Resolve with an owner (US-107: decisions have an owner and a
    /// history). Returns the affected expectation ids for the caller to
    /// update visibly; resolving twice is refused (history is honest).
    pub fn resolve(
        &mut self,
        id: &str,
        option_id: &str,
        owner: &str,
        at_ns: u64,
    ) -> Result<Vec<String>, String> {
        let r = self.records.get_mut(id).ok_or_else(|| "no such conflict".to_string())?;
        if r.status == ConflictStatus::Resolved {
            return Err("already resolved — history is append-only".into());
        }
        if owner.is_empty() {
            return Err("a resolution requires an owner".into());
        }
        let opt = r
            .options
            .iter()
            .find(|o| o.id == option_id)
            .ok_or_else(|| "no such option".to_string())?;
        r.status = ConflictStatus::Resolved;
        r.owner = Some(owner.to_string());
        r.resolution = Some(format!("{}: {}", opt.id, opt.description));
        r.history.push(ConflictEvent {
            kind: EventKind::Resolved,
            actor: owner.into(),
            at_ns,
            detail: r.resolution.clone().unwrap_or_default(),
        });
        Ok(r.affected_expectations.clone())
    }

    /// Open conflicts, oldest id first (deterministic).
    pub fn open(&self) -> Vec<&ConflictRecord> {
        self.records
            .values()
            .filter(|r| r.status == ConflictStatus::Open)
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&ConflictRecord> {
        self.records.get(id)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger() -> ConflictLedger {
        let mut l = ConflictLedger::new();
        let id = l.raise(
            vec![
                Statement {
                    doc: "docs/pay.md".into(),
                    text: "charges retry automatically".into(),
                    applicability: "checkout".into(),
                },
                Statement {
                    doc: "docs/ops.md".into(),
                    text: "no automatic retries without backoff sign-off".into(),
                    applicability: "all flows".into(),
                },
            ],
            "a checkout charge fails transiently: auto-retry policy contradicts the backoff sign-off rule",
            vec!["AE-checkout-retry".into()],
            100,
        );
        assert!(l.add_option(&id, "retry with exponential backoff after sign-off", "pm", 110));
        assert!(l.add_option(&id, "surface a manual retry button", "pm", 120));
        l
    }

    #[test]
    fn raise_option_resolve_with_owner_and_history() {
        let mut l = ledger();
        let id = l.open()[0].id.clone();
        let affected = l.resolve(&id, "CF-0001-O1", "eng-lead", 200).unwrap();
        assert_eq!(affected, vec!["AE-checkout-retry".to_string()]);
        let r = l.get(&id).unwrap();
        assert_eq!(r.status, ConflictStatus::Resolved);
        assert_eq!(r.owner.as_deref(), Some("eng-lead"));
        assert_eq!(r.history.len(), 4); // raised + 2 options + resolved
        assert_eq!(r.history[3].kind, EventKind::Resolved);
        assert!(l.open().is_empty());
    }

    #[test]
    fn resolution_requires_owner_and_refuses_double_resolve() {
        let mut l = ledger();
        let id = l.open()[0].id.clone();
        assert!(l.resolve(&id, "CF-0001-O1", "", 200).is_err());
        l.resolve(&id, "CF-0001-O1", "eng-lead", 200).unwrap();
        let err = l.resolve(&id, "CF-0001-O2", "someone", 300).unwrap_err();
        assert!(err.contains("append-only"));
        // record history unchanged by the refused attempt
        assert_eq!(l.get(&id).unwrap().history.len(), 4);
    }

    #[test]
    fn statements_and_scenario_are_first_class() {
        let l = ledger();
        let r = l.get("CF-0001").unwrap();
        assert_eq!(r.statements.len(), 2);
        assert!(r.scenario.contains("contradicts"));
        assert_eq!(r.affected_expectations, vec!["AE-checkout-retry".to_string()]);
    }
}
