//! Naming adapter — method-name / implementation mismatch (TASK-039,
//! §7 Høst & Østvold "Debugging Method Names" row, FR-10 / FR-11).
//!
//! Compares what a method *name promises* (lexicon verbs) with what its
//! *summary shows it does* (effects + resolved callees). An `authorize()`
//! that only prints becomes (a) a `NameMismatch`/`SuspectedStub` record with
//! complete evidence and (b) an `ObligationKind::Naming` contract entering
//! the registry as `Authority::InferredCandidate` — never `Explicit`
//! (FR-11 conformance-finding example, §8 global rule 4).
//!
//! Rules honored:
//! - **FR-10/global rule 4**: labels are immutable
//!   [`Label{source, model_id, prompt_hash}`] values and NEVER influence
//!   detection, membership, or verification — they are annotations.
//! - **FR-28**: with the LLM disabled (`label(_, None)`), the deterministic
//!   lexicon fallback still produces labels and findings end-to-end.
//! - **FR-3/global rule 6**: per-language application lives behind the
//!   language-adapter vouch table — a language that does not vouch the
//!   naming rule is skipped, so rules do not leak across languages.
//! - **global rule 2**: evidence cites the name token, the observed action
//!   set, and the missing promised action.

use crate::{Authority, Contract, ContractSource, ObligationKind};
use cie_core::evidence::Evidence;
use cie_core::types::{hash_hex, RevId, SymbolId};
use cie_semantic::facts::SymbolStore;
use cie_summary::{EffectKind, FunctionSummary};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The action space a name can promise (sketch: `Action::CheckPermission`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Action {
    CheckPermission,
    Authenticate,
    Validate,
    Verify,
    Delete,
    Read,
    Write,
    Open,
    Close,
    Acquire,
    Release,
    Begin,
    Commit,
    Rollback,
    Log,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::CheckPermission => "check_permission",
            Action::Authenticate => "authenticate",
            Action::Validate => "validate",
            Action::Verify => "verify",
            Action::Delete => "delete",
            Action::Read => "read",
            Action::Write => "write",
            Action::Open => "open",
            Action::Close => "close",
            Action::Acquire => "acquire",
            Action::Release => "release",
            Action::Begin => "begin",
            Action::Commit => "commit",
            Action::Rollback => "rollback",
            Action::Log => "log",
        }
    }

    /// The deterministic lexicon: verbs that promise this action
    /// (Høst & Østvold verb analysis). Name tokens are matched
    /// case-insensitively against these stems.
    pub fn verbs(&self) -> &'static [&'static str] {
        match self {
            Action::CheckPermission => &["authorize", "allowed", "can", "may", "grant", "check"],
            Action::Authenticate => &["authenticate", "login", "signin"],
            Action::Validate => &["validate"],
            Action::Verify => &["verify", "assert"],
            Action::Delete => &["delete", "remove", "destroy", "drop"],
            Action::Read => &["read", "get", "fetch", "load", "query"],
            Action::Write => &["write", "set", "save", "store", "update", "put"],
            Action::Open => &["open"],
            Action::Close => &["close", "shut"],
            Action::Acquire => &["acquire", "lock", "connect"],
            Action::Release => &["release", "unlock", "disconnect"],
            Action::Begin => &["begin", "start"],
            Action::Commit => &["commit"],
            Action::Rollback => &["rollback", "abort"],
            Action::Log => &["log"],
        }
    }

    /// All actions whose verb lexicon matches `token` (deterministic order).
    pub fn from_token(token: &str) -> Vec<Action> {
        let t = token.to_ascii_lowercase();
        let mut out: Vec<Action> = Action::all()
            .iter()
            .copied()
            .filter(|a| a.verbs().iter().any(|v| t == *v || t.starts_with(v)))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Table order for deterministic iteration.
    pub fn all() -> &'static [Action] {
        &[
            Action::CheckPermission,
            Action::Authenticate,
            Action::Validate,
            Action::Verify,
            Action::Delete,
            Action::Read,
            Action::Write,
            Action::Open,
            Action::Close,
            Action::Acquire,
            Action::Release,
            Action::Begin,
            Action::Commit,
            Action::Rollback,
            Action::Log,
        ]
    }
}

/// Immutable label source (FR-10). `Deterministic` = the LLM-off lexicon
/// fallback; `Llm` = an LLM-proposed annotation (inert either way).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelSource {
    Deterministic,
    Llm,
}

/// Immutable `Label{source, model_id, prompt_hash}` (FR-10). Stored at
/// `label:{record_id}`; annotates records, never influences detection,
/// membership, or verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub source: LabelSource,
    pub model_id: Option<String>,
    pub prompt_hash: Option<String>,
}

impl Label {
    /// §15.1 key for the persisted label of a record id.
    pub fn key(record_id: &str) -> String {
        format!("label:{record_id}")
    }
}

/// Naming-record kind (sketch: `RecordKind::NameMismatch | SuspectedStub`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NamingKind {
    /// Name promises an action the summary never performs.
    NameMismatch,
    /// Constant-returning no-effect body behind an action name — the
    /// classic stub shape (stronger claim than a bare mismatch).
    SuspectedStub,
}

impl NamingKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NamingKind::NameMismatch => "name_mismatch",
            NamingKind::SuspectedStub => "suspected_stub",
        }
    }
}

/// One naming-mismatch record (FR-9 payload shape before DerivedRecord
/// wrapping — `payload_json()` is the store's payload view).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamingRecord {
    /// Subject symbol (sketch: `r.subject`).
    pub subject: SymbolId,
    pub name: String,
    pub kind: NamingKind,
    /// Actions promised by the name (lexicon hit set).
    pub promised: Vec<Action>,
    /// Observed effect set rendered as strings (evidence, rule 2).
    pub observed: Vec<String>,
    /// The promised action the summary never satisfies (sketch:
    /// `p.missing_action == Some(Action::CheckPermission)`).
    pub missing: Option<Action>,
    pub evidence: Evidence,
    pub label: Option<Label>,
}

impl NamingRecord {
    /// FR-9 payload view for DerivedRecord wrapping (one interface for all
    /// analyzers; the store stays core-only).
    pub fn payload_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    /// The FR-11 contract this mismatch implies: the name's promise as an
    /// `ObligationKind::Naming` candidate. Authority is ALWAYS
    /// `InferredCandidate` — the lexicon/LLM can never establish (§8).
    pub fn to_contract(&self, symbols: Vec<SymbolId>) -> Contract {
        let missing = self
            .missing
            .map(|a| a.as_str().to_string())
            .unwrap_or_else(|| "promised".into());
        Contract {
                introduced: None,
            id: 0, // assigned by the registry
            obligation: ObligationKind::Naming,
            expr: format!(
                "`{}` must {} (naming lexicon)",
                self.name,
                missing.replace('_', " ")
            ),
            authority: Authority::InferredCandidate,
            symbols,
            source: ContractSource::NamingConvention,
        }
    }
}

/// Per-language application seam (FR-3/global rule 6): the language adapter
/// vouches that the naming rule applies to its language. The default table
/// vouches every shipped language; adapters override via `set_vouch`.
#[derive(Debug, Clone)]
pub struct NamingAdapter {
    vouch: BTreeMap<cie_semantic::lang::LangId, bool>,
}

impl Default for NamingAdapter {
    fn default() -> Self {
        Self::lexicon_default()
    }
}

impl NamingAdapter {
    /// Deterministic lexicon fallback (FR-28): all shipped languages
    /// vouched, standard verb lexicon, no LLM anywhere.
    pub fn lexicon_default() -> Self {
        use cie_semantic::lang::LangId;
        let vouch = [
            (LangId::Unknown, false),
            (LangId::Python, true),
            (LangId::JavaScript, true),
            (LangId::TypeScript, true),
            (LangId::Java, true),
            (LangId::C, true),
            (LangId::Cpp, true),
            (LangId::CSharp, true),
            (LangId::Go, true),
            (LangId::Rust, true),
            (LangId::Php, true),
        ]
        .into_iter()
        .collect();
        Self { vouch }
    }

    /// FR-3 seam: an adapter re-vouches (or withdraws) its language.
    pub fn set_vouch(&mut self, lang: cie_semantic::lang::LangId, applies: bool) {
        self.vouch.insert(lang, applies);
    }

    pub fn vouches(&self, lang: cie_semantic::lang::LangId) -> bool {
        self.vouch.get(&lang).copied().unwrap_or(false)
    }

    /// Split an identifier into name tokens: snake_case, camelCase,
    /// PascalCase and kebab-case all normalize to lowercase words
    /// (per-language verb normalization is exactly this + the vouch gate).
    pub fn name_tokens(name: &str) -> Vec<String> {
        let mut out = vec![];
        let mut cur = String::new();
        for ch in name.chars() {
            if ch == '_' || ch == '-' || ch == '.' {
                if !cur.is_empty() {
                    out.push(cur.clone());
                    cur.clear();
                }
            } else if ch.is_uppercase() {
                if !cur.is_empty() {
                    out.push(cur.clone());
                }
                cur = ch.to_ascii_lowercase().to_string();
            } else {
                cur.push(ch);
            }
        }
        if !cur.is_empty() {
            out.push(cur);
        }
        out
    }

    /// Promised actions for a method name (deterministic, deduped).
    pub fn promised_actions(name: &str) -> Vec<Action> {
        let mut out: Vec<Action> = Self::name_tokens(name)
            .iter()
            .flat_map(|t| Action::from_token(t))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Run the adapter over the symbol store + summaries at `rev`.
    ///
    /// Deterministic: iterates `store.symbols` in vec order; all set math
    /// goes through sorted vecs. Skips non-function-like and test symbols,
    /// and every language that did not vouch the rule (FR-3).
    pub fn run(
        &self,
        store: &SymbolStore,
        summaries: &BTreeMap<SymbolId, FunctionSummary>,
        rev: RevId,
    ) -> Vec<NamingRecord> {
        let mut out = vec![];
        for sym in &store.symbols {
            if !sym.is_function_like() || sym.is_test() {
                continue;
            }
            if !self.vouches(sym.lang) {
                continue;
            }
            let Some(s) = summaries.get(&sym.id) else {
                continue;
            };
            let promised = Self::promised_actions(&sym.name);
            if promised.is_empty() {
                continue;
            }
            let observed_effects: Vec<String> = s
                .effects
                .iter()
                .map(|e| e.as_str().to_string())
                .collect();
            let callee_names: Vec<String> = s
                .callees
                .iter()
                .filter_map(|c| store.get(*c).map(|cs| cs.name.to_ascii_lowercase()))
                .collect();
            let mut missing: Option<Action> = None;
            for action in &promised {
                if !self.satisfies(*action, s, &observed_effects, &callee_names) {
                    missing = Some(*action);
                    break;
                }
            }
            let Some(missing) = missing else { continue };
            let kind = if s.all_returns_constant
                && observed_effects.is_empty()
                && callee_names.is_empty()
            {
                NamingKind::SuspectedStub
            } else {
                NamingKind::NameMismatch
            };
            // global rule 2: cite name token, observed action set, missing action.
            let evidence = Evidence {
                rev,
                provenance: format!(
                    "naming adapter: name `{}` promises [{}]; summary observes [{}]; \
                     missing action `{}`",
                    sym.name,
                    promised
                        .iter()
                        .map(|a| a.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    if observed_effects.is_empty() {
                        "-".to_string()
                    } else {
                        observed_effects.join(", ")
                    },
                    missing.as_str(),
                ),
                assumptions: vec![format!(
                    "lexicon: {}",
                    missing.verbs().join("|")
                )],
                unsupported: vec![],
                unresolved: s.unresolved_callees.clone(),
                generated_at_ns: 0,
            };
            out.push(NamingRecord {
                subject: sym.id,
                name: sym.name.clone(),
                kind,
                promised,
                observed: observed_effects,
                missing: Some(missing),
                evidence,
                label: None,
            });
        }
        out
    }

    /// Does the summary give observable evidence that the promised action
    /// is actually performed? Conservative: absence of matching evidence is
    /// a mismatch candidate, not proof of a bug — the finding is a
    /// *suspicion* by construction (it enters the registry as
    /// InferredCandidate only).
    fn satisfies(
        &self,
        action: Action,
        s: &FunctionSummary,
        effects: &[String],
        callee_names: &[String],
    ) -> bool {
        let has_effect = |want: &[EffectKind]| want.iter().any(|w| s.effects.contains(w));
        let callee_matches = |stems: &[&str]| {
            callee_names
                .iter()
                .any(|n| stems.iter().any(|st| n.contains(st)))
        };
        match action {
            // A permission check is consulted when the body calls a
            // check-flavored callee or touches a state/network source.
            Action::CheckPermission | Action::Authenticate | Action::Validate | Action::Verify => {
                callee_matches(&[
                    "check", "verify", "valid", "allowed", "permission", "auth",
                    "token", "lookup", "policy", "grant", "acl", "role", "hash",
                    "secret", "db", "query", "store", "repo", "session",
                ]) || has_effect(&[EffectKind::DbRead, EffectKind::Network, EffectKind::EnvRead, EffectKind::Crypto])
            }
            Action::Delete => {
                callee_matches(&["delete", "remove", "destroy", "drop", "unlink"])
                    || has_effect(&[EffectKind::DbWrite, EffectKind::WriteFile])
            }
            Action::Read => {
                callee_matches(&["read", "get", "fetch", "load", "query", "select"])
                    || has_effect(&[EffectKind::DbRead, EffectKind::ReadFile, EffectKind::EnvRead, EffectKind::Network])
            }
            Action::Write => {
                callee_matches(&["write", "set", "save", "store", "insert", "update", "put"])
                    || has_effect(&[EffectKind::DbWrite, EffectKind::WriteFile])
            }
            Action::Open => {
                callee_matches(&["open", "connect", "new"])
                    || has_effect(&[EffectKind::Network])
            }
            Action::Close => {
                callee_matches(&["close", "disconnect", "shutdown"])
                    || has_effect(&[EffectKind::Network])
            }
            Action::Acquire => {
                callee_matches(&["acquire", "lock", "connect", "obtain"])
                    || has_effect(&[EffectKind::Lock, EffectKind::Network])
            }
            Action::Release => {
                callee_matches(&["release", "unlock", "disconnect", "free"])
            }
            Action::Begin => callee_matches(&["begin", "start", "init", "open"]),
            Action::Commit => callee_matches(&["commit", "flush", "apply", "send"])
                || has_effect(&[EffectKind::DbWrite, EffectKind::Network]),
            Action::Rollback => callee_matches(&["rollback", "abort", "revert", "cancel"]),
            // Logging promised by the name and a Print/Log effect satisfies it.
            Action::Log => has_effect(&[EffectKind::Log, EffectKind::Print]),
        }
    }

    /// Immutable label for a record (FR-10). `llm: None` = LLM disabled →
    /// the deterministic fallback (FR-28); `Some(model_id)` = LLM
    /// annotation with a content-derived prompt hash. Labels annotate; they
    /// never influence detection, membership, or verification.
    pub fn label(&self, rec: &NamingRecord, llm: Option<&str>) -> Label {
        match llm {
            None => Label { source: LabelSource::Deterministic, model_id: None, prompt_hash: None },
            Some(model_id) => {
                let prompt = format!(
                    "name={}|promised={:?}|observed={:?}|missing={:?}",
                    rec.name, rec.promised, rec.observed, rec.missing
                );
                Label {
                    source: LabelSource::Llm,
                    model_id: Some(model_id.to_string()),
                    prompt_hash: Some(hash_hex(&cie_core::types::hash(prompt.as_bytes()))),
                }
            }
        }
    }
}
