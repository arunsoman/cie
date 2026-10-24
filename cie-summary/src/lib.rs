//! Effect kinds, function summaries (FR-5) and the summary fixpoint helper.

use cie_core::types::{Ns, RevId, SymbolId};
use serde::{Deserialize, Serialize};
pub mod cache;

pub use cache::{ReuseStats, SummaryCacheEntry};

use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EffectKind {
    ReadFile,
    WriteFile,
    DbRead,
    DbWrite,
    Network,
    Print,
    Log,
    Spawn,
    Crypto,
    Random,
    EnvRead,
    Eval,
    Sleep,
    Lock,
    Assert,
    Time,
}

impl EffectKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EffectKind::ReadFile => "read_file",
            EffectKind::WriteFile => "write_file",
            EffectKind::DbRead => "db_read",
            EffectKind::DbWrite => "db_write",
            EffectKind::Network => "network",
            EffectKind::Print => "print",
            EffectKind::Log => "log",
            EffectKind::Spawn => "spawn",
            EffectKind::Crypto => "crypto",
            EffectKind::Random => "random",
            EffectKind::EnvRead => "env_read",
            EffectKind::Eval => "eval",
            EffectKind::Sleep => "sleep",
            EffectKind::Lock => "lock",
            EffectKind::Assert => "assert",
            EffectKind::Time => "time",
        }
    }

    pub fn security_relevant(&self) -> bool {
        matches!(
            self,
            EffectKind::DbRead
                | EffectKind::DbWrite
                | EffectKind::Network
                | EffectKind::Crypto
                | EffectKind::EnvRead
                | EffectKind::Eval
                | EffectKind::Spawn
                | EffectKind::WriteFile
                | EffectKind::ReadFile
        )
    }
}

/// A db effect claim WITH its receiver/library provenance (behavioural-
/// evidence redesign): which receiver/library identity the claim came from
/// and which rung of the resolution ladder produced it (declared binding →
/// imported/type-resolved API identity → factory-derived handle → hint).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EffectClaim {
    pub kind: EffectKind,
    /// The resolved receiver/library identity ("SmsOutboundRepository",
    /// "jaydebeapi connection", "db-cursor (factory: self.conn.cursor())").
    pub receiver: String,
    /// Ladder rung: declared-binding | type-resolved-api-identity |
    /// imported-module-identity | factory-derived-db-handle.
    pub resolution_basis: String,
}

/// A lexical hint the source never validated into a claim: kept in a typed
/// field, deliberately SEPARATE from effects/tags/contracts (the redesign's
/// semantic-claims rule). `note` states the absence honestly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LexicalHint {
    /// Hint vocabulary: auth | db_read | db_write | money | destructive.
    pub hint: String,
    /// How the hint arose: lexical-name-match | lexical-spelling.
    pub basis: String,
    /// The honest gap statement (what evidence was looked for and missing).
    pub note: String,
}

/// Byte span of a call expression within its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CallSpan {
    pub start: u32,
    pub end: u32,
}

/// One call site, per-occurrence: the record that lets repeated invocations
/// stay distinct and lets a sequence provider reconstruct source order
/// without re-parsing (BB2/BB5/BB6 oracle shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSiteRecord {
    /// Callee text as written ("submissionService.submit").
    pub callee: String,
    /// Receiver expression as written, when the call has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver: Option<String>,
    /// Resolved receiver type/library identity, when resolution succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receiver_type: Option<String>,
    /// Declaration dimension: resolved | external | unresolved.
    pub declaration: String,
    /// Dispatch dimension: exact | candidate | unknown | reflective | external.
    pub dispatch: String,
    /// Observation dimension: unobserved (static capture) for now.
    pub observation: String,
    /// Source span of this exact occurrence.
    pub span: CallSpan,
    /// 1-based occurrence index per (caller, callee text) in source order.
    pub occurrence: u32,
    /// Argument count at the site, when counted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<u32>,
    /// Enclosing guard: the condition text, "loop", or "unconditional".
    pub guard: String,
    /// 0-based evaluation order among the caller's call sites.
    pub source_order: u32,
    /// 1-based ordinal of the call's line among the caller's distinct call lines.
    pub statement_index: u32,
    pub line: u32,
    /// A db effect claim carried by THIS site, with provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<EffectClaim>,
    /// A lexical hint this site produced (demoted db spelling), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// Bottom-up interprocedural summary (PRD §18 / FR-5). Versioned by `rev`,
/// invalidatable, and tagged with the analysis conditions under which it
/// was computed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionSummary {
    pub sym: SymbolId,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub reads: BTreeSet<String>,
    pub writes: BTreeSet<String>,
    pub effects: BTreeSet<EffectKind>,
    pub exceptions: Vec<String>,
    pub path_conditions: Vec<String>,
    /// Resolved callee symbol ids (edges of the call graph).
    pub callees: Vec<SymbolId>,
    /// Unresolved callee texts with reasons (evidence).
    pub unresolved_callees: Vec<String>,
    /// Reverse of `callees`: every symbol whose OWN summary names this one
    /// as a callee, across the whole revision. Derived, not extracted — a
    /// single pass over every summary's `callees` after they're all built
    /// (see `derive_callers` below), so it's always exactly consistent with
    /// `callees` by construction and costs no new parsing.
    #[serde(default)]
    pub callers: Vec<SymbolId>,
    /// Distinct constant return values, if any return sites are constant.
    pub returns_constants: BTreeSet<String>,
    /// True when every return site is a constant and there is no raise.
    pub all_returns_constant: bool,
    pub has_failure_path: bool,
    pub has_loops: bool,
    /// Behavior tags used downstream: auth, sensitive, money, test,
    /// entrypoint, query, mutation, delegation, print_only ...
    pub tags: BTreeSet<String>,
    /// Per-occurrence call-site records (BB2/BB5/BB6): declaration/dispatch/
    /// observation dimensions, spans, occurrence identity, guards, source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub call_sites: Vec<CallSiteRecord>,
    /// Lexical hints (auth/db/money/destructive name-or-spelling matches)
    /// that were NOT promoted into tags/effects/contracts for lack of
    /// non-lexical validation evidence. Typed, separable, honest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lexical_hints: Vec<LexicalHint>,
    /// Distinct db receiver identities behind this summary's db claims
    /// (direct + absorbed). Empty when the summary claims no db effects.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub receiver: BTreeSet<String>,
    /// Distinct resolution-ladder rungs behind this summary's db claims.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub resolution_basis: BTreeSet<String>,
    pub rev: RevId,
    pub compute_ns: Ns,
    /// False when a budget cut stopped refinement — never treat as "no defect".
    pub complete: bool,
}

impl FunctionSummary {
    pub fn empty(sym: SymbolId, rev: RevId, now: Ns) -> Self {
        FunctionSummary {
            sym,
            inputs: vec![],
            outputs: vec![],
            reads: BTreeSet::new(),
            writes: BTreeSet::new(),
            effects: BTreeSet::new(),
            exceptions: vec![],
            path_conditions: vec![],
            callees: vec![],
            unresolved_callees: vec![],
            callers: vec![],
            returns_constants: BTreeSet::new(),
            all_returns_constant: false,
            has_failure_path: false,
            has_loops: false,
            tags: BTreeSet::new(),
            call_sites: vec![],
            lexical_hints: vec![],
            receiver: BTreeSet::new(),
            resolution_basis: BTreeSet::new(),
            rev,
            compute_ns: now,
            complete: true,
        }
    }

    /// Transitive closure step: absorb callee effects into this summary.
    pub fn absorb_callee(&mut self, callee: &FunctionSummary) {
        for e in &callee.effects {
            self.effects.insert(*e);
        }
        for r in &callee.reads {
            self.reads.insert(r.clone());
        }
        for w in &callee.writes {
            self.writes.insert(w.clone());
        }
        for x in &callee.exceptions {
            self.exceptions.push(x.clone());
        }
        if callee.has_failure_path {
            self.has_failure_path = true;
        }
        // db provenance travels with the db effects it justifies — an
        // absorber's db claim is exactly as strong as its callee's evidence.
        for r in &callee.receiver {
            self.receiver.insert(r.clone());
        }
        for b in &callee.resolution_basis {
            self.resolution_basis.insert(b.clone());
        }
    }

    pub fn is_print_only(&self) -> bool {
        let interesting: BTreeSet<&EffectKind> = self
            .effects
            .iter()
            .filter(|e| !matches!(e, EffectKind::Print | EffectKind::Log))
            .collect();
        interesting.is_empty() && !self.effects.is_empty()
    }
}

/// Map a builtin call classification to summary effect kinds.
pub fn builtin_effects(kind: cie_semantic::facts::BuiltinKind) -> &'static [EffectKind] {
    use cie_semantic::facts::BuiltinKind as B;
    match kind {
        B::Print => &[EffectKind::Print],
        B::Log => &[EffectKind::Log],
        B::DbExecute => &[EffectKind::DbWrite],
        B::DbQuery => &[EffectKind::DbRead],
        B::Network => &[EffectKind::Network],
        B::FileRead => &[EffectKind::ReadFile],
        B::FileWrite => &[EffectKind::WriteFile],
        B::Crypto => &[EffectKind::Crypto],
        B::Random => &[EffectKind::Random],
        B::Env => &[EffectKind::EnvRead],
        B::ProcessExec => &[EffectKind::Spawn],
        B::Eval => &[EffectKind::Eval],
        B::Assert => &[EffectKind::Assert],
        B::Sleep => &[EffectKind::Sleep],
        B::Lock => &[EffectKind::Lock],
        B::Time => &[EffectKind::Time],
        B::Other => &[],
    }
}
