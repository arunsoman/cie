// Builds the compact, plain-words digest of the loaded CIE data that the
// chat backend uses as its single source of truth. Everything is truncated
// and capped so the payload stays small, and every technical term is
// translated with the same plain-words policy as the rest of the UI.

import type { CieReport, CieViewSpec, ConceptTree, CieDeps, CieRuntime, CieQuality, CieCochange, CieOverlap, CieDrift } from "./cie-types";
import { findingTitle, obligationTitle, effectPlain, certaintyPlain } from "./plain-words";
import { selectFcaClusters, type FcaCluster } from "./quality";

function cut(s: string, n: number): string {
  const t = String(s ?? "").replace(/\s+/g, " ").trim();
  return t.length > n ? t.slice(0, n - 1) + "…" : t;
}

export function buildDigest(
  report: CieReport | null,
  viewspec: CieViewSpec | null,
  concepts: ConceptTree | null,
  label: string,
  deps?: CieDeps | null,
  runtime?: CieRuntime | null,
  quality?: CieQuality | null,
  cochange?: CieCochange | null,
  overlap?: CieOverlap | null,
  drift?: CieDrift | null
): Record<string, unknown> | null {
  if (!report) return null;

  const fileName = (idx: number) => report.files?.[idx] ?? `file #${idx}`;
  const sym = (id: number) => report.symbols.find((s) => s.id === id);

  // Findings — plain titles, capped at 40.
  const findings = report.findings.slice(0, 40).map((f) => {
    const s = sym(f.symbol);
    return {
      urgency: f.severity,
      problem: findingTitle(f),
      technical_kind: f.kind,
      where: s ? `${fileName(s.loc.file)} line ${f.evidence_lines?.[0] ?? s.loc.start}` : "unknown location",
      in_code_name: s?.qual_name ?? f.symbol_name,
      says: cut(f.message, 260),
      how_to_fix: cut(f.remediation, 200),
    };
  });

  // Promises (contracts) — all of them, usually a handful.
  const promises = report.contracts.map((c) => ({
    promise: obligationTitle(c.obligation),
    exact_words: cut(c.expr, 140),
    who_wrote_it: c.authority === "Explicit" ? "written by the developers" : "CIE's guess (worth checking)",
    must_be_kept_by: c.symbols
      .map((id) => sym(id)?.qual_name)
      .filter(Boolean)
      .slice(0, 8),
  }));

  // Coverage + trust, verbatim from the report (already human-readable).
  const trust = {
    checked_files: `${report.coverage.files_analyzed} of ${report.coverage.files_total}`,
    checked_named_things: `${report.coverage.symbols_analyzed} of ${report.coverage.symbols_total}`,
    fully_checked: report.coverage.complete,
    notes: report.coverage.notes?.map((n) => cut(n, 200)).slice(0, 10),
    assumptions: report.evidence.assumptions?.map((a) => cut(a, 200)).slice(0, 10) ?? [],
    names_cie_could_not_match: (report.evidence.unresolved ?? []).map((u) => cut(u, 80)).slice(0, 25),
    warnings: (report.warnings ?? []).map((w) => cut(w, 200)).slice(0, 10),
    engine_provenance: cut(report.evidence.provenance, 120),
  };

  // Map summary — counts only; the digest stays small.
  let map: Record<string, unknown> | null = null;
  if (viewspec) {
    const tally = (items: { displayMode: string }[]) => {
      const t = { proven: 0, good_guess: 0, not_sure: 0 };
      for (const it of items) {
        if (it.displayMode === "Fact") t.proven++;
        else if (it.displayMode === "Inference") t.good_guess++;
        else t.not_sure++;
      }
      return t;
    };
    map = {
      what_the_map_shows: "bubbles = parts of your code; connections = how they relate",
      bubbles_by_kind: viewspec.elements.reduce<Record<string, number>>((acc, e) => {
        acc[e.elementKind] = (acc[e.elementKind] ?? 0) + 1;
        return acc;
      }, {}),
      bubbles_by_certainty: tally(viewspec.elements),
      connections_by_certainty: tally(viewspec.edges),
      things_cie_hid: Array.isArray(viewspec.hidden) ? viewspec.hidden.length : 0,
      gaps_cie_admits: Array.isArray(viewspec.gaps) ? viewspec.gaps.length : 0,
      caption: viewspec.footer?.caption ? cut(viewspec.footer.caption, 160) : undefined,
    };
  }

  // Concepts — what each code file does, in plain words.
  let whatItDoes: Record<string, unknown> | null = null;
  if (concepts) {
    const byKind = (kind: string) => concepts.nodes.filter((n) => n.kind === kind);
    whatItDoes = {
      whole_project: byKind("Domain").map((n) => ({
        does: n.behaviors.map((b) => cut(b.kind, 60)).slice(0, 8),
        covers_named_things: n.symbols.length,
      })),
      code_files: byKind("Module").map((n) => ({
        file: n.label,
        does: n.behaviors.map((b) => cut(b.kind, 60)).slice(0, 6),
        rules: (n.rules ?? []).map((r) => cut(String(r.kind ?? "").replace(/_/g, " "), 60)).slice(0, 6),
      })),
    };
  }

  // Repeated behavior — concepts.json's FCA pass (cie-analysis/src/fca.rs):
  // different symbols sharing the exact same behavior/effect signature,
  // found by the engine on THIS project (not a text-matching tool, not
  // demo-only). This is CIE's real answer to "is there duplication?" — use
  // it instead of guessing from symbol names. Clusters with only one shared
  // trait are left out as too generic to call "repeated behavior."
  const repeatedBehavior: Record<string, unknown>[] = concepts
    ? selectFcaClusters(concepts, report, 2)
        .slice(0, 10)
        .map((c: FcaCluster) => ({
          shared_traits: c.node.intent ?? [],
          found_in: c.symbolNames.slice(0, 6),
          times: c.node.symbols.length,
        }))
    : [];

  // Effects — what the code touches in the outside world.
  const effectTally: Record<string, number> = {};
  for (const s of report.summaries) {
    for (const e of s.effects ?? []) {
      const k = effectPlain(e);
      effectTally[k] = (effectTally[k] ?? 0) + 1;
    }
  }

  // Dependencies — what the manifests declare, as written (US-062).
  let dependencies: Record<string, unknown> | null = null;
  if (deps?.table && deps.table.files_scanned > 0) {
    const byEco: Record<string, number> = {};
    for (const d of deps.table.deps) byEco[d.ecosystem] = (byEco[d.ecosystem] ?? 0) + 1;
    dependencies = {
      manifests_read: deps.table.files_scanned,
      declared_total: deps.table.deps.length,
      per_ecosystem: byEco,
      packages: deps.table.deps.slice(0, 40).map((d) => ({
        name: d.name,
        ecosystem: d.ecosystem,
        version_as_written: cut(d.version_req || "*", 24),
        scope: d.scope,
        declared_in: cut(d.source, 60),
      })),
      unparsed_manifests: deps.table.unparsed?.map((u) => cut(u, 80)).slice(0, 4) ?? [],
      honesty: "versions as written; CIE does not resolve transitive trees or judge compatibility",
    };
  }

  // Runtime evidence — measured numbers only; absence is stated, never filled.
  let runtime_evidence: Record<string, unknown> | null = null;
  const ds = runtime?.datasets;
  if (runtime && ds) {
    runtime_evidence = {
      project: cut(runtime.project, 80),
      collected_at: runtime.collected_at,
      honesty: cut(runtime.honesty, 220),
      datasets_present: Object.keys(ds),
      cpu_hotspots: ds.cpu?.top_functions.slice(0, 6).map((f) => ({
        function: cut(f.function, 40),
        cumulative_ms: Math.round(f.cum_ms),
        measured_by: cut(ds.cpu!.tool, 40),
      })),
      test_coverage: ds.coverage
        ? {
            measured_total_percent: ds.coverage.totals.percent,
            files: ds.coverage.files.slice(0, 10).map((f) => ({ file: f.file, percent: f.percent })),
            measured_by: cut(ds.coverage.tool, 40),
          }
        : undefined,
      deadlock: ds.deadlock
        ? {
            detected: ds.deadlock.detected,
            cycle: ds.deadlock.cycle,
            threads: ds.deadlock.threads.map((t) => ({
              thread: t.thread,
              holds: t.holds,
              waits_for: t.wants,
              blocked_at: t.blocked_at ? `${t.blocked_at.file}:${t.blocked_at.line}` : undefined,
            })),
            measured_by: cut(ds.deadlock.tool, 60),
          }
        : undefined,
      race: ds.race
        ? {
            lost_updates_natural: ds.race.lost_natural,
            lost_updates_widened: ds.race.lost_widened,
            note: "widened window is disclosed (GIL yielded between read and write); a clean run never proves the race is gone",
          }
        : undefined,
      concurrency: ds.concurrency
        ? {
            tasks: ds.concurrency.tasks,
            workers: ds.concurrency.workers,
            makespan_ms: ds.concurrency.makespan_ms,
            observed_overlap: ds.concurrency.observed_overlap,
          }
        : undefined,
      memory: ds.memory
        ? {
            live_bytes: ds.memory.allocated_bytes,
            peak_bytes: ds.memory.peak_bytes,
            top_sites: ds.memory.top_sites.slice(0, 6).map((s) => ({ where: s.where, bytes: s.bytes })),
          }
        : undefined,
    };
  }

  // Quality / health — the ETL export (quality.json): the eight classic
  // measures plus the security/duplication/drift findings, all collected.
  let quality_facts: Record<string, unknown> | null = null;
  if (quality) {
    quality_facts = {
      collected_at: quality.collected_at,
      honesty: cut(quality.honesty, 240),
      metrics: quality.metrics.map((m) => ({
        name: m.name,
        value: m.value_text,
        what_it_measures: cut(m.what_it_measures, 90),
        typical_calculation: cut(m.typical_calculation, 90),
        scope: cut(m.scope, 80),
      })),
      security: {
        scanner: cut(quality.security.tool, 90),
        total: quality.security.issues.length,
        findings: quality.security.issues.slice(0, 12).map((it) => ({
          where: `${it.file}:${it.line}`,
          kind: it.name,
          cwe: it.cwe,
          urgency: it.severity,
        })),
      },
      duplication: {
        blocks: quality.duplication.blocks_found,
        lines: quality.duplication.duplicated_lines,
        pairs: quality.duplication.pairs.slice(0, 6).map((p) => `${p.a_file}:${p.a_line} <-> ${p.b_file}:${p.b_line} (${p.lines} lines)`),
      },
      most_complex: quality.complexity.functions.slice(0, 6).map((f) => ({
        where: `${f.file}:${f.line}`,
        function: f.function,
        cyclomatic: f.cc,
      })),
      hotspots: quality.hotspots.rows.slice(0, 5).map((r) => ({ file: r.file, risk: r.risk })),
      code_drift: quality.drift.code.slice(0, 6).map((c) => ({ file: c.file, note: cut(c.note, 60) })),
      document_drift: quality.drift.document.slice(0, 4).map((d) => ({ doc: d.doc, note: cut(d.note, 60) })),
      defect_tracker: {
        service: quality.defects.tracker.service,
        repo: quality.defects.tracker.repo,
        issues_total: quality.defects.tracker.issues_total,
        recorded_as_found: true,
        proxy: cut(quality.defects.derived.proxy_note ?? "", 220),
      },
      crash_probe: { sessions: quality.crash_probe.sessions, crashes: quality.crash_probe.crashes },
      repair_probe: { mttr: quality.repair_probe.mttr_text },
    };
  }

  // Co-change — real git history (cie cochange), works on any project with
  // enough commits on disk. A shallow URL clone has almost none; said so
  // honestly via sample_commits rather than hidden.
  let cochange_facts: Record<string, unknown> | null = null;
  if (cochange) {
    cochange_facts = {
      sample_commits: cochange.sample_commits,
      counted_commits: cochange.counted_commits,
      shallow_history: cochange.sample_commits <= 1,
      pairs: cochange.edges.slice(0, 12).map((e) => ({
        a: e.a,
        b: e.b,
        times_together: e.support,
        cross_directory: e.cross_directory,
      })),
      honesty: cochange.note,
    };
  }

  // Capability overlap — real concept-tree comparison (cie overlap). Only
  // the genuinely two-way pairs (both sides have their own code too) are
  // worth citing; a pure subset is just one concept nested in another.
  let overlap_facts: Record<string, unknown> | null = null;
  if (overlap?.pairs?.length) {
    const two_way = overlap.pairs.filter((p) => p.only_a.length > 0 && p.only_b.length > 0);
    overlap_facts = {
      compared: overlap.pairs.length + overlap.unmapped.length,
      pairs: (two_way.length ? two_way : overlap.pairs)
        .slice()
        .sort((a, b) => b.similarity - a.similarity)
        .slice(0, 10)
        .map((p) => ({
          a: p.a,
          b: p.b,
          shared_symbols: p.shared.slice(0, 6),
          similarity_percent: Math.round(p.similarity * 100),
        })),
      honesty: "based on real shared symbols, not names — a high percentage with both sides still having their own code is the 'does this already exist' signal",
    };
  }

  // Doc drift — real claim extraction + verification (cie drift), not the
  // git-date proxy quality_facts.document_drift uses for the demo.
  let drift_facts: Record<string, unknown> | null = null;
  if (drift?.claims?.length) {
    const byClaim = new Map(drift.claims.map((c) => [c.id, c]));
    drift_facts = {
      docs_checked: drift.docs,
      claims_total: drift.claims.length,
      by_status: drift.aggregate,
      claims: drift.verifications.slice(0, 15).map((v) => ({
        claim: cut(byClaim.get(v.claim)?.text ?? "", 160),
        doc: byClaim.get(v.claim)?.doc,
        status: v.status,
        evidence: v.evidence.slice(0, 2).map((e) => cut(e, 160)),
      })),
      honesty: "every status is a real verification against the code's own summaries, never guessed from the doc text alone",
    };
  }

  // Certainty vocabulary, so the model uses the same words as the UI.
  const words = {
    Fact: certaintyPlain("Fact").label,
    Inference: certaintyPlain("Inference").label,
    Hypothesis: certaintyPlain("Hypothesis").label,
    Critical: "fix soon",
    High: "please look",
    Medium: "worth checking",
    Low: "good to know",
  };

  return {
    about: `Plain-words health report for: ${label}. Produced by the CIE engine from a direct read of the project (revision ${report.rev}).`,
    certainty_words: words,
    // The app can draw these locally from the loaded store, instantly and
    // without the model — so "I can't draw that" is never the right answer
    // for a picture. For uml/flow the model only picks the kind; the app
    // fills the boxes and arrows from the map itself.
    drawings_cie_can_make: [
      "uml — UML-style structure diagram of the code (boxes = files and parts, arrows = connections)",
      "flow — who-calls-whom flow from the call records",
      "hbar — bars comparing counts",
      "donut — one whole split into parts",
      "stat — one big number",
      "table — a few rows x columns",
      ...(dependencies ? ["donut/table of the declared dependencies (from the dependencies facts below)"] : []),
      ...(runtime_evidence
        ? ["bars/stat/table of the measured runtime evidence below — always name the tool that measured it"]
        : []),
      ...(quality_facts
        ? ["bars/stat/table/tree/heatmap of the collected quality data below — metrics, security findings, duplication, hot spots, drift, coverage heatmap, concept hierarchy — always state the source"]
        : []),
      ...(repeatedBehavior.length
        ? ["hbar of repeated_behavior below for 'is there duplication / repeated code' questions — this is a real finding from the engine's own analysis, on THIS project, not a demo stand-in"]
        : []),
      ...(cochange_facts ? ["hbar of cochange_facts below for 'what changes together / coupling' questions — real git history on THIS project"] : []),
      ...(overlap_facts ? ["table of overlap_facts below for 'does this already exist / overlap / redundant' questions — real shared-symbol comparison on THIS project"] : []),
      ...(drift_facts ? ["hbar of drift_facts below for 'do the docs match the code / drift' questions — real claim verification on THIS project, not the git-date proxy"] : []),
    ],
    project: {
      files: report.files?.filter(Boolean).map((f) => f.split("/").pop() ?? f) ?? [],
      named_things_total: report.symbols.length,
      languages: [...new Set(report.symbols.map((s) => s.lang))],
      touches_outside_world: effectTally,
    },
    things_to_look_at: {
      total: report.findings.length,
      fix_soon_first: true,
      list: findings,
    },
    promises,
    coverage_and_trust: trust,
    map,
    what_it_does: whatItDoes,
    repeated_behavior: repeatedBehavior.length ? repeatedBehavior : null,
    ...(dependencies ? { dependencies } : {}),
    ...(runtime_evidence ? { runtime_evidence } : { runtime_evidence: null }),
    ...(quality_facts ? { quality_facts } : { quality_facts: null }),
    cochange_facts,
    overlap_facts,
    drift_facts,
  };
}
