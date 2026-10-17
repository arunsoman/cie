//! Engler-style belief-signal collection (TASK-040, `cie-analysis::beliefs`).
//!
//! Scans summaries + the relation graph for the three signal families:
//! inconsistent checks, cleanup pairing, and API-usage co-occurrence.
//! Output factors feed the Kremenek factor graph
//! (`cie-contract::beliefs::FactorGraph`).
//!
//! Checking proxy (documented, deterministic): a caller *honors* callee
//! `F`'s result iff the caller's summary contains a path condition that
//! mentions `F`'s name — i.e. some branch is decided on F's result. Sites
//! that call F without such a condition are the deviants (Engler: the
//! minority pattern is the bug candidate).

use cie_contract::{BeliefFactor, Family, Predicate, SiteRef};
use cie_core::types::{RevId, SymbolId};
use cie_relations::{RelKind, RelationGraph};
use cie_semantic::facts::SymbolStore;
use cie_summary::FunctionSummary;
use std::collections::BTreeMap;

/// Collected signals, indexed by their query keys.
#[derive(Debug, Clone, Default)]
pub struct BeliefSignals {
    /// callee name → (support sites, deviant sites), inconsistent checks.
    check: BTreeMap<String, (Vec<SiteRef>, Vec<SiteRef>)>,
    /// acquire name → (paired release, support sites, deviant sites).
    cleanup: BTreeMap<String, (String, Vec<SiteRef>, Vec<SiteRef>)>,
    /// (a, b) co-occurrence → (support sites, deviant sites).
    co_occurs: BTreeMap<(String, String), (Vec<SiteRef>, Vec<SiteRef>)>,
    pub rev: RevId,
}

impl BeliefSignals {
    /// Collect all three families in one deterministic pass. Callers and
    /// callees are resolved through the relation graph's `Calls` edges
    /// where present, falling back to summary callee ids (the graph is the
    /// authority; summaries fill the names).
    pub fn collect(
        store: &SymbolStore,
        summaries: &BTreeMap<SymbolId, FunctionSummary>,
        relations: &RelationGraph,
        rev: RevId,
    ) -> Self {
        let mut sig = BeliefSignals { rev, ..Default::default() };

        // caller-id → callee-ids from the relation graph (deterministic
        // BTreeMap keyed by caller id).
        let mut call_edges: BTreeMap<SymbolId, Vec<SymbolId>> = BTreeMap::new();
        for (from, to, kind) in relations.edge_triples() {
            if kind != RelKind::Calls {
                continue;
            }
            let (Some(a), Some(b)) = (relations.node_weight(from), relations.node_weight(to))
            else {
                continue;
            };
            call_edges.entry(a).or_default().push(b);
        }
        // summary-only edges for callers missing from the graph
        for (id, s) in summaries {
            let e = call_edges.entry(*id).or_default();
            for c in &s.callees {
                if !e.contains(c) {
                    e.push(*c);
                }
            }
            e.sort();
            e.dedup();
        }

        // name lookup
        let name_of = |id: SymbolId| -> Option<String> {
            store.get(id).map(|f| f.name.to_ascii_lowercase())
        };

        for (caller, callees) in &call_edges {
            let Some(s) = summaries.get(caller) else { continue };
            for callee in callees {
                let Some(cname) = name_of(*callee) else { continue };
                // ---- family 1: inconsistent checks (Engler) ----
                let honored = s
                    .path_conditions
                    .iter()
                    .any(|p| p.to_ascii_lowercase().contains(&cname));
                let entry = sig
                    .check
                    .entry(cname.clone())
                    .or_insert_with(|| (vec![], vec![]));
                if honored {
                    entry.0.push((*caller, *callee));
                } else {
                    entry.1.push((*caller, *callee));
                }
                // ---- family 3: co-occurrence regularities ----
                for other in callees {
                    if other == callee {
                        continue;
                    }
                    let Some(oname) = name_of(*other) else { continue };
                    let key = if cname < oname {
                        (cname.clone(), oname)
                    } else {
                        (oname, cname.clone())
                    };
                    let entry = sig.co_occurs.entry(key).or_insert_with(|| (vec![], vec![]));
                    entry.0.push((*caller, *callee));
                }
            }
            // ---- family 2: cleanup pairing (acquire/release) ----
            let callee_names: Vec<String> =
                callees.iter().filter_map(|c| name_of(*c)).collect();
            for (idx, a) in callee_names.iter().enumerate() {
                let Some(a_stem) = acquire_stem(a) else { continue };
                let paired = callee_names
                    .iter()
                    .find(|n| release_stem(n, a_stem).is_some());
                let entry = sig
                    .cleanup
                    .entry(a.clone())
                    .or_insert_with(|| (String::new(), vec![], vec![]));
                if let Some(r) = paired {
                    if entry.0.is_empty() {
                        entry.0 = r.clone();
                    }
                    entry.1.push((*caller, callees[idx]));
                } else {
                    entry.2.push((*caller, callees[idx]));
                }
            }
        }
        // sort all site lists for byte-identical determinism
        for (_, (sup, dev)) in sig.check.iter_mut() {
            sup.sort();
            sup.dedup();
            dev.sort();
            dev.dedup();
        }
        for (_, (_r, sup, dev)) in sig.cleanup.iter_mut() {
            sup.sort();
            sup.dedup();
            dev.sort();
            dev.dedup();
        }
        for (_, (sup, dev)) in sig.co_occurs.iter_mut() {
            sup.sort();
            sup.dedup();
            dev.sort();
            dev.dedup();
        }
        sig
    }

    /// Inconsistent-check signal for a callee name (sketch:
    /// `signals.check_consistency("frob")`).
    pub fn check_consistency(&self, callee: &str) -> Option<CheckSignal> {
        let (sup, dev) = self.check.get(&callee.to_ascii_lowercase())?;
        Some(CheckSignal {
            callee: callee.to_string(),
            support: sup.clone(),
            deviants: dev.clone(),
            rev: self.rev,
        })
    }

    /// Cleanup-pairing signal (sketch: `signals.cleanup_pairing("lock",
    /// "unlock")`).
    pub fn cleanup_pairing(&self, acquire: &str, release: &str) -> Option<CleanupSignal> {
        let (r, sup, dev) = self.cleanup.get(&acquire.to_ascii_lowercase())?;
        Some(CleanupSignal {
            acquire: acquire.to_string(),
            release: release.to_string(),
            observed_release: r.clone(),
            support: sup.clone(),
            deviants: dev.clone(),
            rev: self.rev,
        })
    }

    /// Co-occurrence regularity signal.
    pub fn co_occurrence(&self, a: &str, b: &str) -> Option<CoOccurSignal> {
        let key = if a < b { (a.into(), b.into()) } else { (b.into(), a.into()) };
        let (sup, dev) = self.co_occurs.get(&key)?;
        Some(CoOccurSignal {
            a: a.to_string(),
            b: b.to_string(),
            support: sup.clone(),
            deviants: dev.clone(),
            rev: self.rev,
        })
    }

    /// All signals as normalized factors (ready for the factor graph).
    pub fn factors(&self) -> Vec<BeliefFactor> {
        let mut out = vec![];
        for (name, (sup, dev)) in &self.check {
            if sup.is_empty() || dev.is_empty() {
                continue; // no majority pattern without both sides
            }
            out.push(BeliefFactor {
                family: Family::InconsistentCheck,
                predicate: Predicate::retval_checked(name.clone()),
                support_sites: sup.clone(),
                deviant_sites: dev.clone(),
                rev: self.rev,
                weight: 2.0,
            });
        }
        for (a, (r, sup, dev)) in &self.cleanup {
            if r.is_empty() || sup.is_empty() || dev.is_empty() {
                continue;
            }
            out.push(BeliefFactor {
                family: Family::CleanupPairing,
                predicate: Predicate::CleanupPaired {
                    acquire: a.clone(),
                    release: r.clone(),
                },
                support_sites: sup.clone(),
                deviant_sites: dev.clone(),
                rev: self.rev,
                weight: 1.5,
            });
        }
        for ((a, b), (sup, dev)) in &self.co_occurs {
            if sup.is_empty() || dev.is_empty() {
                continue;
            }
            out.push(BeliefFactor {
                family: Family::UsageRegulariry,
                predicate: Predicate::CoOccurs {
                    a: a.clone(),
                    b: b.clone(),
                },
                support_sites: sup.clone(),
                deviant_sites: dev.clone(),
                rev: self.rev,
                weight: 1.0,
            });
        }
        out.sort_by(|x, y| x.predicate.as_str().cmp(&y.predicate.as_str()));
        out
    }
}

/// Inconsistent-check signal (sketch: `.support()` = #honoring sites).
#[derive(Debug, Clone)]
pub struct CheckSignal {
    pub callee: String,
    pub support: Vec<SiteRef>,
    pub deviants: Vec<SiteRef>,
    pub rev: RevId,
}

impl CheckSignal {
    pub fn support(&self) -> usize {
        self.support.len()
    }
    pub fn deviant_sites(&self) -> &[SiteRef] {
        &self.deviants
    }
    pub fn factor(&self) -> BeliefFactor {
        BeliefFactor {
            family: Family::InconsistentCheck,
            predicate: Predicate::retval_checked(self.callee.clone()),
            support_sites: self.support.clone(),
            deviant_sites: self.deviants.clone(),
            rev: self.rev,
            weight: 2.0,
        }
    }
}

/// Cleanup-pairing signal.
#[derive(Debug, Clone)]
pub struct CleanupSignal {
    pub acquire: String,
    pub release: String,
    pub observed_release: String,
    pub support: Vec<SiteRef>,
    pub deviants: Vec<SiteRef>,
    pub rev: RevId,
}

impl CleanupSignal {
    pub fn support(&self) -> usize {
        self.support.len()
    }
    pub fn factor(&self) -> BeliefFactor {
        BeliefFactor {
            family: Family::CleanupPairing,
            predicate: Predicate::CleanupPaired {
                acquire: self.acquire.clone(),
                release: if self.observed_release.is_empty() {
                    self.release.clone()
                } else {
                    self.observed_release.clone()
                },
            },
            support_sites: self.support.clone(),
            deviant_sites: self.deviants.clone(),
            rev: self.rev,
            weight: 1.5,
        }
    }
}

/// Co-occurrence regularity signal.
#[derive(Debug, Clone)]
pub struct CoOccurSignal {
    pub a: String,
    pub b: String,
    pub support: Vec<SiteRef>,
    pub deviants: Vec<SiteRef>,
    pub rev: RevId,
}

impl CoOccurSignal {
    pub fn support(&self) -> usize {
        self.support.len()
    }
    pub fn factor(&self) -> BeliefFactor {
        BeliefFactor {
            family: Family::UsageRegulariry,
            predicate: Predicate::CoOccurs {
                a: self.a.clone(),
                b: self.b.clone(),
            },
            support_sites: self.support.clone(),
            deviant_sites: self.deviants.clone(),
            rev: self.rev,
            weight: 1.0,
        }
    }
}

/// Acquire-flavored stems: (name) → stem family.
fn acquire_stem(name: &str) -> Option<&'static str> {
    for s in ["acquire", "lock", "connect", "open"] {
        if name.contains(s) {
            return Some(s);
        }
    }
    None
}

/// Release-flavored counterpart of an acquire stem.
fn release_stem(name: &str, acquire_stem: &str) -> Option<&'static str> {
    let pair: &[(&str, &str)] = &[
        ("acquire", "release"),
        ("lock", "unlock"),
        ("connect", "disconnect"),
        ("open", "close"),
    ];
    pair.iter()
        .find(|(a, _)| *a == acquire_stem)
        .and_then(|(_, r)| if name.contains(r) { Some(*r) } else { None })
}
