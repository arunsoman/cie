//! Deep checks — Repository access and orientation (US-001..US-007).

use super::fixture;
use std::fs;

/// US-001: connecting identifies the studied revision; a summary never
/// implies complete analysis (coverage metadata travels with every snapshot).
pub fn us_001() -> Result<(), String> {
    let outcome = fixture::ingest("us001");
    let s = &outcome.snapshot;
    if s.files.len() != 3 {
        return Err(format!("expected 3 files, got {}", s.files.len()));
    }
    if s.rev == 0 {
        return Err("revision must be identified (rev != 0)".into());
    }
    let cov = &s.coverage;
    if cov.files_analyzed == 0 || cov.files_total == 0 {
        return Err("coverage must report analyzed/total file counts".into());
    }
    // The honesty flag exists and reflects this run (complete here).
    if !cov.complete {
        return Err("unbudgeted fixture run should report complete coverage".into());
    }
    Ok(())
}

/// US-002: the product states what it understands — supported languages are
/// analysed, unsupported ones are skipped openly (never guessed).
pub fn us_002() -> Result<(), String> {
    let dir = fixture::python_repo("us002");
    fs::write(dir.join("helper.js"), "function ping(x) { return x; }\n").unwrap();
    fs::write(dir.join("blob.xyz"), "not source").unwrap();
    let outcome = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    let s = &outcome.snapshot;
    let langs: std::collections::BTreeSet<String> =
        s.symbols.iter().map(|f| format!("{:?}", f.lang)).collect();
    let joined = langs.iter().cloned().collect::<Vec<_>>().join(",");
    if !joined.contains("Python") || !joined.contains("JavaScript") {
        return Err(format!("expected Python + JavaScript symbols, got [{joined}]"));
    }
    if s.files.iter().any(|f| f.contains("blob.xyz")) {
        return Err("unsupported extension must not pretend to be understood".into());
    }
    if s.coverage.symbol_ratio() <= 0.0 {
        return Err("coverage ratio must be reported for analysed files".into());
    }
    Ok(())
}

/// US-003: understanding stays current — incremental re-ingest identifies
/// the changed file, revisions advance, and the old answer remains
/// retrievable under its own revision (never relabelled as current).
pub fn us_003() -> Result<(), String> {
    let dir = fixture::python_repo("us003");
    let first = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    let rev1 = first.rev;
    fs::write(
        dir.join("app.py"),
        "from svc import charge\n\ndef checkout(user, amount):\n    return charge(user, amount * 2)\n",
    )
    .unwrap();
    let second = cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .map_err(|e| e.to_string())?;
    if second.rev <= rev1 {
        return Err("revision must advance after a change".into());
    }
    if second.plan.changed != vec!["app.py".to_string()] {
        return Err(format!("plan must name app.py changed, got {:?}", second.plan.changed));
    }
    // Old answer still retrievable as historical evidence.
    let old = cie_ingest::load_outcome(&dir, rev1).map_err(|e| e.to_string())?;
    if old.snapshot.rev != rev1 {
        return Err("loaded snapshot must keep its own revision identity".into());
    }
    Ok(())
}

/// US-004: several repositories from one place — merged concept view keeps
/// per-repo sources (identical names stay separate, conflicts named).
pub fn us_004() -> Result<(), String> {
    let a = fixture::python_repo("us004a");
    let b = fixture::python_repo("us004b");
    let opts = cie_ingest::IngestOptions::default();
    let org = cie_ingest::run_org_ingest(&[a.clone(), b.clone()], &opts).map_err(|e| e.to_string())?;
    if org.repos.len() != 2 {
        return Err(format!("expected 2 repos, got {}", org.repos.len()));
    }
    let merged = org
        .merged_concepts
        .iter()
        .find(|m| m.sources.len() == 2)
        .ok_or_else(|| {
            use cie_incremental::SnapshotStoreExt;
            let dbg = |d: &std::path::Path| {
                cie_store::Store::open(d)
                    .ok()
                    .and_then(|st| {
                        let rev = st.last_rev();
                        st.load_snapshot(rev).ok().flatten().map(|s| {
                            format!(
                                "rev={} files={} sums={} nodes={}",
                                s.rev,
                                s.files.len(),
                                s.summaries.len(),
                                s.concepts.nodes.len()
                            )
                        })
                    })
                    .unwrap_or_else(|| "no-snapshot".into())
            };
            let merged_dbg: Vec<(String, usize)> = org
                .merged_concepts
                .iter()
                .map(|m| (m.label.clone(), m.sources.len()))
                .collect();
            let disk_a: Vec<String> = std::fs::read_dir(&a)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            format!(
                "expected at least one concept merged across both repos; merged={merged_dbg:?}; repo-a[{}]; repo-b[{}]; disk-a={disk_a:?}",
                dbg(&a),
                dbg(&b),
            )
        })?;
    if merged.label.is_empty() {
        return Err("merged concept must carry its label".into());
    }
    Ok(())
}

/// US-006: background work control — budgets + cancellation checkpoints.
/// An exhausted LLM budget refuses cleanly (no silent overrun), and a
/// zero wall-clock budget ingest completes without panic and reports its
/// coverage state honestly (complete flag + notes, never silent truncation).
pub fn us_006() -> Result<(), String> {
    let b = cie_llm::Budget::new(0.0);
    if !b.exceeded() {
        return Err("a zero budget must report exceeded immediately".into());
    }
    let dir = fixture::python_repo("us006");
    let opts = cie_ingest::IngestOptions {
        budget_ms: Some(0),
        ..cie_ingest::IngestOptions::default()
    };
    let outcome = cie_ingest::run_ingest(&dir, &opts).map_err(|e| e.to_string())?;
    // Either state is honest; what must never happen is a missing report.
    let cov = &outcome.snapshot.coverage;
    if cov.files_total == 0 {
        return Err("budgeted run must still report totals".into());
    }
    if cov.complete && cov.files_analyzed < cov.files_total {
        return Err("complete=true with unanalyzed files is a lie".into());
    }
    Ok(())
}
