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

/// Bottom-up interprocedural summary (PRD §18 / FR-5). Versioned by `rev`,
/// invalidatable, and tagged with the analysis conditions under which it
/// was computed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Distinct constant return values, if any return sites are constant.
    pub returns_constants: BTreeSet<String>,
    /// True when every return site is a constant and there is no raise.
    pub all_returns_constant: bool,
    pub has_failure_path: bool,
    pub has_loops: bool,
    /// Behavior tags used downstream: auth, sensitive, money, test,
    /// entrypoint, query, mutation, delegation, print_only ...
    pub tags: BTreeSet<String>,
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
            returns_constants: BTreeSet::new(),
            all_returns_constant: false,
            has_failure_path: false,
            has_loops: false,
            tags: BTreeSet::new(),
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
