//! Ranked next-step assembly (CIE-US US-069 engine close).
//!
//! The LDFI planner names discriminating sites; this module assembles the
//! ranked *next-step list* the story promises: each recommended check
//! explains which outcome would support or challenge which hypothesis,
//! unavailable tools become concrete blockers (never fabricated results),
//! and ranks are computed from discriminating power, not vibes.
//!
//! Model: hypotheses = fault candidates over lineage edges; evidence =
//! sites already confirmed/refuted by earlier rounds. A check at a site
//! discriminates when the site lies on a path whose confirmed/refuted
//! pattern differs between hypotheses. Unavailable tools for a check make
//! the check a concrete blocker entry, still listed (never silently
//! dropped — and never given a fabricated outcome).

use std::collections::{BTreeMap, BTreeSet};

use crate::ldfi::{Fault, LineageGraph};
use serde::{Deserialize, Serialize};

/// Evidence gathered so far (from prior experiments or runtime feeds).
#[derive(Debug, Clone, Default)]
pub struct EvidenceState {
    /// Sites (edge ids) confirmed healthy.
    pub confirmed: BTreeSet<String>,
    /// Sites (edge ids) refuted (fault demonstrated).
    pub refuted: BTreeSet<String>,
    /// Hypothesis ids already excluded (their provenance strings).
    pub excluded_hypotheses: BTreeSet<String>,
}

/// One ranked next step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NextStep {
    /// What to observe (the check).
    pub check: String,
    /// The lineage site the check observes.
    pub site: String,
    /// Hypotheses (fault provenance strings) whose confirmation this
    /// outcome would support.
    pub supports: Vec<String>,
    /// Hypotheses this outcome would challenge.
    pub challenges: Vec<String>,
    /// Discriminating rank (1 = most discriminating).
    pub rank: u32,
    /// When the tool to run this check is unavailable: the concrete
    /// blocker (named tool), so the step is actionable, not fake.
    pub blocker: Option<String>,
}

/// Build the ranked next-step list (US-069).
///
/// * `lineage` — the successful-run lineage graph;
/// * `faults` — the fault hypotheses under consideration (typically the
///   planner's candidate set);
/// * `evidence` — confirmed/refuted sites so far;
/// * `available_tools` — tool names the harness can actually run; a check
///   whose tool is absent gets a blocker instead of a fabricated result.
pub fn next_steps(
    lineage: &LineageGraph,
    faults: &[Fault],
    evidence: &EvidenceState,
    available_tools: &[&str],
) -> Vec<NextStep> {
    // live hypotheses only (excluded ones never re-enter the ranking)
    let live: Vec<&Fault> = faults
        .iter()
        .filter(|f| !evidence.excluded_hypotheses.contains(&f.provenance()))
        .collect();

    // each site's discriminating power: how differently the live
    // hypotheses treat it (touches vs not), minus already-decided sites
    let mut site_support: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut site_challenge: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for f in &live {
        match f {
            Fault::CrashScheduler => continue, // scheduler pseudo-edge: no single site
            _ => {
                let edge = fault_edge(f);
                let provenance = f.provenance();
                site_support.entry(edge).or_default().push(provenance.clone());
                // every OTHER live hypothesis that does NOT touch this edge
                // is challenged by an anomaly observed here
                for g in &live {
                    if g.provenance() != provenance {
                        match g {
                            Fault::CrashScheduler => {}
                            _ => {
                                if fault_edge(g) != edge {
                                    site_challenge
                                        .entry(edge)
                                        .or_default()
                                        .push(g.provenance());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut steps: Vec<NextStep> = Vec::new();
    for (site, supports) in &site_support {
        // decided sites are not next steps — they are evidence
        if evidence.confirmed.contains(*site) || evidence.refuted.contains(*site) {
            continue;
        }
        let challenges = site_challenge.get(*site).cloned().unwrap_or_default();
        let score = supports.len() as u64 + challenges.len() as u64;
        // tool model: observing a lineage edge needs the `observe-<edge>`
        // tool; available when listed. Deterministic naming keeps the
        // blocker concrete.
        let tool = format!("observe-{site}");
        let blocker = if available_tools.contains(&tool.as_str())
            || available_tools.contains(&"*")
        {
            None
        } else {
            Some(format!("tool unavailable: {tool}"))
        };
        steps.push(NextStep {
            check: format!(
                "observe {site} during a run; an anomaly supports [{}] and challenges [{}]",
                supports.join(", "),
                challenges.join(", ")
            ),
            site: site.to_string(),
            supports: supports.clone(),
            challenges,
            rank: 0,
            blocker,
        });
        let _ = score;
    }
    // rank: more supports+challenges first, then site asc (deterministic)
    steps.sort_by(|a, b| {
        let sa = a.supports.len() + a.challenges.len();
        let sb = b.supports.len() + b.challenges.len();
        sb.cmp(&sa).then(a.site.cmp(&b.site))
    });
    for (i, s) in steps.iter_mut().enumerate() {
        s.rank = (i + 1) as u32;
    }
    steps
}

fn fault_edge(f: &Fault) -> &str {
    match f {
        Fault::Drop(e) | Fault::Duplicate(e) | Fault::Reorder(e) | Fault::Delay(e) => &e.id,
        Fault::CrashScheduler => "scheduler_tick",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ldfi::{LineageEdge, LineageKind};

    fn graph() -> LineageGraph {
        let mut g = LineageGraph::empty();
        g.edges = vec![
            LineageEdge { id: "e1".into(), kind: LineageKind::Message, from: "api".into(), to: "charge".into() },
            LineageEdge { id: "e2".into(), kind: LineageKind::Computation, from: "charge".into(), to: "ledger".into() },
            LineageEdge { id: "e3".into(), kind: LineageKind::StateUpdate, from: "ledger".into(), to: "done".into() },
        ];
        g
    }

    fn faults() -> Vec<Fault> {
        let g = graph();
        vec![
            Fault::Drop(g.edges[0].clone()),
            Fault::Delay(g.edges[1].clone()),
            Fault::Duplicate(g.edges[2].clone()),
        ]
    }

    #[test]
    fn steps_rank_by_discriminating_power_and_cite_hypotheses() {
        let g = graph();
        let fs = faults();
        let ev = EvidenceState::default();
        let steps = next_steps(&g, &fs, &ev, &["*"]);
        assert_eq!(steps.len(), 3);
        // all sites touch exactly one hypothesis each and challenge the
        // other two → equal power → deterministic tiebreak by site name
        for (i, s) in steps.iter().enumerate() {
            assert_eq!(s.rank, (i + 1) as u32);
        }
        let e1 = steps.iter().find(|s| s.site == "e1").unwrap();
        assert!(e1.supports.contains(&"drop(e1:message)".to_string()));
        assert_eq!(e1.challenges.len(), 2);
        assert!(e1.check.contains("supports"));
        assert!(e1.check.contains("challenges"));
    }

    #[test]
    fn decided_sites_are_not_next_steps_and_excluded_hypotheses_vanish() {
        let g = graph();
        let fs = faults();
        let mut ev = EvidenceState::default();
        ev.refuted.insert("e1".into());
        ev.excluded_hypotheses.insert("drop(e1:message)".into());
        let steps = next_steps(&g, &fs, &ev, &["*"]);
        assert!(!steps.iter().any(|s| s.site == "e1"));
        // e2's challenges now exclude the dropped hypothesis
        let e2 = steps.iter().find(|s| s.site == "e2").unwrap();
        assert!(!e2.challenges.contains(&"drop(e1:message)".to_string()));
    }

    #[test]
    fn unavailable_tool_becomes_a_concrete_blocker() {
        let g = graph();
        let fs = faults();
        let ev = EvidenceState::default();
        let steps = next_steps(&g, &fs, &ev, &["observe-e2"]);
        let e1 = steps.iter().find(|s| s.site == "e1").unwrap();
        assert_eq!(e1.blocker.as_deref(), Some("tool unavailable: observe-e1"));
        let e2 = steps.iter().find(|s| s.site == "e2").unwrap();
        assert!(e2.blocker.is_none());
    }
}
