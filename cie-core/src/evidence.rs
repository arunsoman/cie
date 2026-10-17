//! Evidence and coverage (PRD §20 / FR-4).
//!
//! Every analysis result carries: revision, provenance, assumptions,
//! unsupported constructs, unresolved calls, and generation time. Analysis
//! completion and conclusion are separate dimensions — a budget cut yields
//! `complete == false`, never a silent "no defect".

use crate::types::{Ns, NsDelta, RevId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Evidence {
    pub rev: RevId,
    pub provenance: String,
    pub assumptions: Vec<String>,
    pub unsupported: Vec<String>,
    pub unresolved: Vec<String>,
    pub generated_at_ns: Ns,
}

impl Evidence {
    pub fn new(rev: RevId, provenance: impl Into<String>, now: Ns) -> Self {
        Evidence {
            rev,
            provenance: provenance.into(),
            assumptions: vec![],
            unsupported: vec![],
            unresolved: vec![],
            generated_at_ns: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub files_total: u32,
    pub files_analyzed: u32,
    pub symbols_total: u32,
    pub symbols_analyzed: u32,
    /// False when a budget/timeout cut the analysis short.
    pub complete: bool,
    pub notes: Vec<String>,
    pub wall_ns: NsDelta,
}

impl Default for Coverage {
    fn default() -> Self {
        Coverage {
            files_total: 0,
            files_analyzed: 0,
            symbols_total: 0,
            symbols_analyzed: 0,
            complete: true,
            notes: vec![],
            wall_ns: 0,
        }
    }
}

impl Coverage {
    pub fn symbol_ratio(&self) -> f32 {
        if self.symbols_total == 0 {
            return 1.0;
        }
        self.symbols_analyzed as f32 / self.symbols_total as f32
    }

    pub fn merge_notes(&mut self, note: impl Into<String>) {
        self.notes.push(note.into());
    }
}
