// Quality / health charts + local answers — built from the ETL export
// (runtime-data/quality.json, produced by
// examples/bankapp/scripts/collect_quality.py).
//
// The pipeline: EXTRACT real sources (the sample's own files, the hosting
// repository's git history, the GitHub issues API recorded as found, the
// measured coverage export) → TRANSFORM with the classic formulas (stated on
// every chart) → LOAD quality.json. This module turns that export into the
// same ChatChartSpec every other view renders — so the fisheye lens, the
// engine+layout disclosure and click-to-chat all carry over unchanged.
//
// Nothing here invents numbers: an empty tracker is recorded as empty, a
// metric that cannot be computed renders an honest empty state.

import type { CannedChart } from "./canned-charts";
import type { CieQuality, CieReport, ConceptNode, ConceptTree, VizElement, CieCochange, CieOverlap, CieDrift } from "./cie-types";
import { engineFor, layoutFor, type ChatChartSpec, type ElementLoc, type TreeNode, type HeatTile } from "./chat-chart";
import { containerOfPath } from "./atlas";

function cut(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

function spec(kind: ChatChartSpec["kind"], title: string, tell: string, source?: string): ChatChartSpec {
  return { kind, engine: engineFor(kind), layout: layoutFor(kind), title, tell, source };
}

// ---------------------------------------------------------------------------
// Concept hierarchy (from concepts.json — the engine's own concept tree)
// ---------------------------------------------------------------------------

/**
 * Which concept node (if any) this question is actually about — so the
 * concept tree stops being a fixed, unscoped "universal default" and
 * becomes an explanatory chart for THIS question, same as fileScopedChart
 * does for quality answers. Two signals, in priority order:
 *   1. the map bubble the user tapped (mapSelected) — matched by the file
 *      it points to, since both viewspec elements and concept Module nodes
 *      ultimately name a real file from report.json.
 *   2. a symbol named in the question itself — longest name wins, so
 *      "LeafAccountController" beats a generic word like "account".
 * Returns null when neither signal resolves — the honest, genuinely
 * generic case still gets the full project tree, unscoped.
 */
/**
 * The symbol most likely named in free-form question text — longest name
 * wins, so "LeafAccountController" beats a generic word like "account".
 * Shared by resolveConceptFocus and the impact on-demand chat path, so
 * "which function is this question about" is answered the same way
 * everywhere instead of two slightly different guesses.
 */
export function findSymbolInQuestion(report: CieReport | null, question: string): CieReport["symbols"][number] | null {
  if (!report?.symbols?.length) return null;
  const q = question.toLowerCase();
  let best: { s: CieReport["symbols"][number]; len: number } | null = null;
  for (const s of report.symbols) {
    const name = s.name?.toLowerCase();
    if (name && name.length > 2 && q.includes(name) && (!best || name.length > best.len)) {
      best = { s, len: name.length };
    }
  }
  return best?.s ?? null;
}

export function resolveConceptFocus(
  concepts: ConceptTree | null,
  report: CieReport | null,
  question: string,
  mapSelected?: VizElement | null
): number | null {
  if (!concepts?.nodes?.length) return null;

  if (mapSelected?.evidence?.file) {
    const file = mapSelected.evidence.file;
    const hit = concepts.nodes.find(
      (n) => n.kind === "Module" && (file.endsWith(n.label) || n.label.endsWith(file))
    );
    if (hit) return hit.id;
  }

  const best = findSymbolInQuestion(report, question);
  if (best) {
    // The project's own Domain root lists EVERY symbol, so the first match
    // is never the right answer — pick the node with the SMALLEST symbol
    // set that still contains it (the most specific one, e.g. the Module
    // or UseCase the symbol actually lives in, not the whole project).
    let node: ConceptTree["nodes"][number] | null = null;
    for (const n of concepts.nodes) {
      if (!n.symbols?.includes(best.id)) continue;
      if (!node || n.symbols.length < node.symbols.length) node = n;
    }
    if (node) return node.id;
  }

  return null;
}

export function buildConceptTreeSpec(concepts: ConceptTree | null, focusId?: number | null): ChatChartSpec | null {
  if (!concepts?.nodes?.length) return null;
  const kinds = new Set(concepts.nodes.map((n) => n.kind));
  if (!kinds.has("Domain")) return null; // no hierarchy to draw — honest empty

  const byId = new Map(concepts.nodes.map((n) => [n.id, n]));
  let slice = concepts.nodes;
  let focusLabel: string | null = null;

  if (focusId != null && byId.has(focusId)) {
    // Keep only the focus node's ancestor chain (its context, up to the
    // project root) plus its own immediate children — never the whole tree.
    const keep = new Set<number>();
    let cur = byId.get(focusId);
    while (cur) {
      keep.add(cur.id);
      cur = cur.parent != null ? byId.get(cur.parent) : undefined;
    }
    for (const childId of byId.get(focusId)!.children ?? []) keep.add(childId);
    slice = concepts.nodes.filter((n) => keep.has(n.id));
    focusLabel = cut(byId.get(focusId)!.label, 30);
  } else {
    slice = concepts.nodes.slice(0, 40);
  }

  const tree: TreeNode[] = slice.map((n) => ({
    id: `c${n.id}`,
    label: cut(n.label, 30),
    kind: n.kind,
    parent: n.parent === null || n.parent === undefined ? null : `c${n.parent}`,
    note: n.behaviors?.length
      ? `${n.behaviors.length} ${n.behaviors.length === 1 ? "behaviour" : "behaviours"}`
      : undefined,
    certainty: undefined,
    highlight: n.id === focusId,
  }));

  if (focusLabel) {
    return {
      ...spec(
        "tree",
        `Where ${focusLabel} sits`,
        `Centred on ${focusLabel} — its place in the project, from the big idea down. Tap any node to ask about it.`
      ),
      tree,
      source: "concepts.json (the engine's concept tree)",
    };
  }

  const roots = tree.filter((t) => t.parent === null).length;
  return {
    ...spec("tree", "What the project is made of", `The idea hierarchy: ${roots} big-picture ${roots === 1 ? "idea" : "ideas"}, the files under them, and what each does. Tap any node to ask about it.`),
    tree,
    source: "concepts.json (the engine's concept tree)",
  };
}

// ---------------------------------------------------------------------------
// Repeated behavior (concepts.json's FCA pass — cie-analysis/src/fca.rs,
// wired in cie-analysis/src/hierarchy.rs). Formal Concept Analysis groups
// symbols that share the exact same behavior/effect tags into one formal
// concept (primary_edge "FcaSubsumption"); a concept with 2+ symbols in its
// extent is two or more DIFFERENT functions the engine independently found
// doing the same thing. This is behavior-based, not text matching — it
// survives renaming and crosses files/classes — and unlike quality.json's
// duplication table (DUP-1, a demo-only Python script), it runs as part of
// every `cie analyze`/`cie concepts` call, on any project.
//
// Specificity matters more than extent size: a concept whose intent is one
// generic tag ("print") is weak evidence — plenty of unrelated functions
// print something. Several shared tags together is the real signal, so
// clusters rank by intent size first, and anything with only one shared
// tag is left out of the chart entirely (still real data, just too noisy
// to call "repeated behavior" on its own).
// ---------------------------------------------------------------------------

export interface FcaCluster {
  node: ConceptNode;
  symbolNames: string[];
  loc?: ElementLoc;
}

/** FCA-derived concept nodes, ranked by how specific (not just how big) the shared behavior is. */
export function selectFcaClusters(concepts: ConceptTree | null, report: CieReport | null, minIntent = 1): FcaCluster[] {
  if (!concepts?.nodes?.length) return [];
  const symName = (id: number) => report?.symbols.find((s) => s.id === id)?.qual_name;
  const nodes = concepts.nodes
    .filter((n) => n.primary_edge === "FcaSubsumption" && n.symbols.length >= 2 && (n.intent?.length ?? 0) >= minIntent)
    .sort((a, b) => (b.intent?.length ?? 0) - (a.intent?.length ?? 0) || b.symbols.length - a.symbols.length);
  return nodes.map((node) => {
    const symbolNames = node.symbols.map(symName).filter((x): x is string => !!x);
    const firstSym = report?.symbols.find((s) => s.id === node.symbols[0]);
    const loc = firstSym ? ({ file: report!.files[firstSym.loc.file] } as ElementLoc) : undefined;
    return { node, symbolNames, loc };
  });
}

function conceptPlainLabel(label: string): string {
  // "Concept:<attr-keys>:<pretty words>" (cie-analysis/src/fca.rs's
  // concept_label) — the pretty, human part is everything after the 2nd colon.
  const parts = label.split(":");
  return parts.length > 2 ? parts.slice(2).join(":") : label;
}

export function buildRepeatedBehaviorSpec(concepts: ConceptTree | null, report: CieReport | null): ChatChartSpec | null {
  const clusters = selectFcaClusters(concepts, report, 2).slice(0, 8);
  if (!clusters.length) return null;
  const data = clusters.map((c) => ({
    name: cut(conceptPlainLabel(c.node.label), 40),
    value: c.node.symbols.length,
    color: (c.node.intent?.length ?? 0) >= 3 ? ("rose" as const) : ("amber" as const),
    loc: c.loc,
  }));
  return {
    ...spec(
      "hbar",
      "Repeated behavior CIE found",
      "Different functions sharing the exact same behavior signature (what they read, write, call, and touch) — found by the engine's own concept analysis, not text matching, so renamed copies still show up. More shared traits is a stronger signal than a bigger count."
    ),
    data,
    source: "concepts.json — formal concept analysis (fca:godin-v1)",
  };
}

// ---------------------------------------------------------------------------
// Co-change coupling (cochange.json — `cie cochange`, real git history)
// ---------------------------------------------------------------------------

export function buildCochangeSpec(cochange: CieCochange | null): ChatChartSpec | null {
  if (!cochange?.edges?.length) return null;
  const edges = cochange.edges.slice(0, 8);
  const data = edges.map((e) => ({
    name: cut(`${e.a.split("/").pop()} ↔ ${e.b.split("/").pop()}`, 40),
    value: e.support,
    color: e.cross_directory ? ("rose" as const) : ("amber" as const),
    loc: { file: e.a } as ElementLoc,
  }));
  const shallow = cochange.sample_commits <= 1;
  return {
    ...spec(
      "hbar",
      "Files that change together",
      shallow
        ? "Only 1 commit was available to mine (a shallow clone) — not enough history for this to mean anything yet."
        : `Real git history, ${cochange.counted_commits} commits counted (${cochange.excluded_bulk} bulk edits excluded). Rose = different top-level directories — the more surprising, more worth checking kind.`
    ),
    data,
    source: "cochange.json — git log --name-status, real commit history",
  };
}

// ---------------------------------------------------------------------------
// Capability overlap (overlap.json — `cie overlap`, concept-tree symbol sets)
// ---------------------------------------------------------------------------

export function buildOverlapSpec(overlap: CieOverlap | null): ChatChartSpec | null {
  if (!overlap?.pairs?.length) return null;
  // Two-way distinctness (both sides have something the other doesn't) is
  // the meaningful "two different-looking things might be the same
  // capability" signal — a pure subset (only_a or only_b empty) is just one
  // concept nested inside another by construction, not a finding.
  const candidates = overlap.pairs.filter((p) => p.only_a.length > 0 && p.only_b.length > 0);
  const pool = (candidates.length ? candidates : overlap.pairs).slice().sort((a, b) => b.similarity - a.similarity).slice(0, 8);
  if (!pool.length) return null;
  const rows = pool.map((p) => [cut(p.a, 28), cut(p.b, 28), String(p.shared.length), `${Math.round(p.similarity * 100)}%`]);
  return {
    ...spec(
      "table",
      "Where capabilities overlap",
      "Parts of the project that share real symbols — not similar names, actual shared functions. High similarity with both sides still having their own code is the 'does this already exist' signal."
    ),
    columns: ["Part A", "Part B", "Shared", "Similarity"],
    rows,
    source: "overlap.json — capability overlap over the concept tree (cie-concepts, US-106)",
  };
}

// ---------------------------------------------------------------------------
// Doc-vs-code drift (drift.json — `cie drift`, real claim verification)
// ---------------------------------------------------------------------------

const DRIFT_STATUS_COLOR: Record<string, "rose" | "amber" | "emerald" | "stone"> = {
  Implemented: "emerald",
  Partial: "amber",
  Drifted: "amber",
  Missing: "rose",
  Contradicting: "rose",
  Undocumented: "stone",
  Unverifiable: "stone",
  Unknown: "stone",
  Superseded: "stone",
};

export function buildDriftSpec(drift: CieDrift | null): ChatChartSpec | null {
  if (!drift?.claims?.length) return null;
  // aggregate's keys and verifications[].status use different casing in the
  // engine's own output (e.g. "missing" vs "Missing") — normalize so the
  // colour lookup and the label are never at the mercy of that.
  const titleCase = (s: string) => (s.length ? s[0].toUpperCase() + s.slice(1).toLowerCase() : s);
  const data = Object.entries(drift.aggregate)
    .map(([status, count]) => {
      const name = titleCase(status);
      return { name, value: count, color: DRIFT_STATUS_COLOR[name] ?? ("stone" as const) };
    })
    .sort((a, b) => b.value - a.value);
  if (!data.length) return null;
  return {
    ...spec(
      "hbar",
      "Does the code still match the docs?",
      `${drift.claims.length} claim(s) read from your own docs, each checked against what the code actually does. Green = implemented, amber = partial or drifted, rose = missing or contradicted.`
    ),
    data,
    source: `drift.json — claim extraction + verification over ${drift.docs.length} doc(s) (cie drift)`,
  };
}

// ---------------------------------------------------------------------------
// Coverage heatmap (from the measured coverage export)
// ---------------------------------------------------------------------------

/** Optional `container` scopes the tiles to one top-level folder — the same
 *  file list, filtered before capping, with the headline percent recomputed
 *  from exactly the rows shown (never the repo total dressed up as a scope). */
export function buildCoverageHeatmapSpec(quality: CieQuality | null, container: string | null = null): ChatChartSpec | null {
  const cov = quality?.coverage_heatmap;
  if (!cov?.files?.length) return null;
  const rows = container ? cov.files.filter((f) => containerOfPath(f.file) === container) : cov.files;
  if (!rows.length) return null;
  const tiles: HeatTile[] = rows.map((f) => ({
    name: cut(f.file, 22),
    value: Math.round(f.percent),
    label: f.covered != null && f.statements != null ? `${f.covered} of ${f.statements} lines` : "never even imported",
    loc: { file: f.file },
  }));
  let pct: number | undefined = cov.totals?.percent;
  if (container) {
    const withNums = rows.filter((f) => f.statements != null && f.statements > 0);
    const stmts = withNums.reduce((s, f) => s + (f.statements ?? 0), 0);
    const covd = withNums.reduce((s, f) => s + (f.covered ?? 0), 0);
    pct = stmts > 0 ? Math.round((covd / stmts) * 100) : undefined;
  }
  const scopeNote = container ? `Scoped to ${container}: ${rows.length} of the ${cov.files.length} measured file(s) shown; the percent is recomputed from just those rows.` : `Whole project: measured across ${cov.files.length} file(s).`;
  return {
    ...spec(
      "heatmap",
      container ? `Test coverage, file by file — ${container} (measured)` : "Test coverage, file by file (measured)",
      `Measured by ${cov.tool ?? "coverage.py"} on the sample's real test run. ${scopeNote} Green is well covered; rose was never even imported — the label says so.`,
    ),
    tiles,
    source: "quality.json — coverage_heatmap (coverage.py)",
  };
}

// ---------------------------------------------------------------------------
// Shared spec builders — ONE builder per collected dataset, used by both the
// one-tap menu and the chat answers, so a metric question and its menu card
// always draw the exact same picture from the exact same export.
// ---------------------------------------------------------------------------

/** Most complex functions, worst first (real AST walk). Optional `container`
 *  filters the ranking to one top-level folder before the cap applies. */
export function buildComplexitySpec(q: CieQuality, container: string | null = null): ChatChartSpec | null {
  const all = q.complexity.functions ?? [];
  const fns = (container ? all.filter((f) => containerOfPath(f.file) === container) : all).slice(0, 8);
  if (!fns.length) return null;
  const data = fns.map((f) => ({
    name: cut(f.function, 20),
    value: f.cc,
    color: f.cc >= q.complexity.totals.max_cc ? "rose" : f.cc >= 4 ? "amber" : "stone",
    loc: { file: f.file, line: f.line } as ElementLoc,
  }));
  return {
    ...spec("hbar", container ? `Most complex functions — ${container} (measured)` : "Most complex functions (measured)", q.complexity.formula),
    data,
    source: `quality.json — ${q.complexity.tool}`,
  };
}

/** Churn per day, hosting repository (real git numstat, as written by git). */
export function buildDailyChurnSpec(q: CieQuality): ChatChartSpec | null {
  const daily = q.churn.hosting_repo.daily ?? [];
  if (!daily.length) return null;
  return {
    ...spec("hbar", "Churn per day (hosting repository)", q.churn.tool + " — " + q.churn.formula),
    data: daily.slice(-8).map((dd) => ({ name: dd.day.slice(5), value: dd.churn, color: "amber" as const })),
    source: "quality.json — churn (hosting repo, labelled)",
  };
}

/** Churn per file in the sample project — which files moved the most.
 *  Optional `container` scopes the ranking to one top-level folder. */
export function buildPerFileChurnSpec(q: CieQuality, container: string | null = null): ChatChartSpec | null {
  const all = q.churn.sample.per_file ?? [];
  const rows = (container ? all.filter((r) => containerOfPath(r.file) === container) : all).slice(0, 8);
  if (!rows.length) return null;
  return {
    ...spec("hbar", container ? `Churn per file — ${container} (sample project)` : "Churn per file (sample project)", `${q.churn.tool} over the sample's own files — added + deleted lines per file.`),
    data: rows.map((r) => ({ name: cut(r.file, 22), value: r.churn, color: "amber" as const, loc: { file: r.file } as ElementLoc })),
    source: "quality.json — churn (sample, per file)",
  };
}

/**
 * The fix-commit events behind the defect metrics (density, MTBF, arrival
 * rate), grouped by the day each commit landed — exactly as recorded by
 * `git log -i --grep`. Discloses how many of the total are drawn.
 */
export function buildFixCommitsSpec(q: CieQuality, title: string, tell: string): ChatChartSpec | null {
  const fc = q.defects.fix_commits;
  const recent = fc.recent ?? [];
  if (!recent.length) return null;
  const byDay = new Map<string, number>();
  for (const c of recent) byDay.set(c.date, (byDay.get(c.date) ?? 0) + 1);
  const data = [...byDay.entries()]
    .sort((a, b) => (a[0] < b[0] ? -1 : 1))
    .map(([day, n]) => ({ name: day.slice(5), value: n, color: "rose" as const }));
  const scope = recent.length < fc.count ? ` Showing the ${recent.length} most recent of ${fc.count}.` : "";
  return {
    ...spec("hbar", title, tell + scope),
    data,
    source: "quality.json — fix commits (git log, real history)",
  };
}

/** The measured crashes behind the crash rate. */
export function buildCrashTableSpec(q: CieQuality): ChatChartSpec | null {
  const rows = (q.crash_probe.crashes_detail ?? [])
    .filter((c) => c.crashed)
    .slice(0, 8)
    .map((c) => [`${c.module}.${c.function}`, cut(c.session, 24), cut(c.reason || "timeout", 40)]);
  if (!rows.length) return null;
  return {
    ...spec("table", "The crashes behind the rate", q.crash_probe.definition),
    columns: ["Entry point", "Input", "What happened"],
    rows,
    source: "quality.json — crash probe",
  };
}

/** The timed fail→fix→recheck loops behind MTTR. */
export function buildRepairTableSpec(q: CieQuality): ChatChartSpec | null {
  const iters = q.repair_probe.iterations ?? [];
  if (!iters.length) return null;
  return {
    ...spec("table", "The repair loops behind MTTR", q.repair_probe.tool),
    columns: ["Bug", "Detect (s)", "Repair (s)", "Recheck (s)", "Total (s)"],
    rows: iters.map((i) => [cut(i.bug, 30), String(i.detect_s), String(i.repair_s), String(i.recheck_s), String(i.total_s)]),
    source: "quality.json — repair probe",
  };
}

/** Hot spots as bars (risk 0–100 per file) — the ranked list, drawn.
 *  Optional `container` scopes the ranking to one top-level folder. */
export function buildHotspotSpec(q: CieQuality, container: string | null = null): ChatChartSpec | null {
  const all = q.hotspots.rows ?? [];
  const rows = (container ? all.filter((r) => containerOfPath(r.file) === container) : all).slice(0, 8);
  if (!rows.length) return null;
  return {
    ...spec("hbar", container ? `Hot spots — ${container}, ranked` : "Hot spots, ranked", q.hotspots.formula),
    data: rows.map((r) => ({
      name: cut(r.file, 22),
      value: Math.round(r.risk),
      color: r.risk >= 50 ? "rose" : r.risk >= 25 ? "amber" : "stone",
      loc: { file: r.file } as ElementLoc,
    })),
    source: "quality.json — hotspots",
  };
}

/** Duplicated blocks as a table. */
export function buildDuplicationSpec(q: CieQuality): ChatChartSpec | null {
  const pairs = (q.duplication.pairs ?? []).slice(0, 8);
  const rows = pairs.map((p) => [
    `${p.a_file}:${p.a_line}`,
    `${p.b_file}:${p.b_line}`,
    String(p.lines),
    cut(p.preview || "(same block)", 34),
  ]);
  if (!rows.length) return null;
  return {
    ...spec("table", "Duplicated blocks (measured)", "Blocks of the same normalized lines appearing in two different files."),
    columns: ["Here", "And again", "Lines", "Starts with"],
    rows,
    rowLoc: pairs.map((p) => ({ file: p.a_file, line: p.a_line } as ElementLoc)),
    source: `quality.json — ${q.duplication.tool}`,
  };
}

/** Code drift vs tests, as a table. */
export function buildCodeDriftSpec(q: CieQuality): ChatChartSpec | null {
  const entries = (q.drift.code ?? []).slice(0, 8);
  const rows = entries.map((c) => [
    cut(c.file, 20),
    c.last_change ?? "—",
    c.tests_last_change ?? "no test file",
    c.test_lag_days != null ? String(c.test_lag_days) : "—",
    cut(c.note, 30),
  ]);
  if (!rows.length) return null;
  return {
    ...spec("table", "Code drift vs tests", "Lag = days since the file changed minus days since its tests did. “No test file” is the loudest entry."),
    columns: ["File", "Code last changed", "Tests last changed", "Lag (days)", "Note"],
    rows,
    rowLoc: entries.map((c) => ({ file: c.file } as ElementLoc)),
    source: "quality.json — drift (git dates)",
  };
}

/** Document drift, as a table. */
export function buildDocumentDriftSpec(q: CieQuality): ChatChartSpec | null {
  const entries = (q.drift.document ?? []).slice(0, 8);
  const rows = entries.map((d) => [
    cut(d.doc, 20),
    cut(d.describes, 34),
    `${d.doc_age_days}d ago`,
    d.drift_days > 0 ? `${d.drift_days}d behind` : "fresh",
    cut(d.note, 30),
  ]);
  if (!rows.length) return null;
  return {
    ...spec("table", "Document drift", "A doc can be old and still correct — this says which ones moved apart and by how many days."),
    columns: ["Doc", "Describes", "Doc age", "Drift", "Note"],
    rows,
    rowLoc: entries.map((d) => ({ file: d.doc } as ElementLoc)),
    source: "quality.json — drift (git dates)",
  };
}

// ---------------------------------------------------------------------------
// All the health + metric charts, for the one-tap menu
// ---------------------------------------------------------------------------

const SEV_COLOR: Record<string, string> = { Critical: "rose", High: "amber", Medium: "stone", Low: "emerald" };

export function buildQualityCharts(quality: CieQuality | null, concepts: ConceptTree | null): CannedChart[] {
  const charts: CannedChart[] = [];
  if (!quality) return charts;
  const q = quality;

  // ---------- Health ----------

  // 1. concept hierarchy
  {
    const s = buildConceptTreeSpec(concepts);
    charts.push({
      id: "concept-hierarchy",
      group: "Health",
      question: "What is this project made of?",
      hint: s ? "The idea hierarchy, drawn from the concept tree." : "No concept tree in this project — honest empty.",
      icon: "shapes",
      spec: s,
    });
  }

  // 2. security issues by kind
  {
    const byCat = Object.entries(q.security.by_category ?? {}).sort((a, b) => b[1] - a[1]);
    const data = byCat.map(([name, v]) => ({
      name: cut(name, 24),
      value: v,
      color: name.includes("Hardcoded") || name.includes("SQL") || name.includes("Shell") ? "rose" : "amber",
    }));
    charts.push({
      id: "security-issues",
      group: "Health",
      question: "Where is the code insecure?",
      hint: data.length ? `${q.security.issues.length} findings from ${q.security.tool.split("—")[0].trim()}.` : "The scan found nothing to flag.",
      icon: "shield",
      spec: data.length
        ? { ...spec("hbar", "Security issues by kind", "Each bar counts findings of one kind. Tap a bar to ask about it."), data, source: "quality.json — security scan (rules disclosed)" }
        : null,
    });
  }

  // 3. vulnerabilities — the CWE-mapped table
  {
    const issues = q.security.issues.slice(0, 8);
    const rows = issues.map((it) => [
      `${it.file}:${it.line}`,
      cut(it.name, 30),
      it.cwe,
      it.severity,
    ]);
    charts.push({
      id: "vulnerabilities",
      group: "Health",
      question: "Which weaknesses have a CWE number?",
      hint: rows.length ? "CWE = the common catalogue of software weaknesses." : "No CWE-mapped findings.",
      icon: "shield",
      spec: rows.length
        ? { ...spec("table", "Weaknesses with a CWE number", "Each row is a real finding: where it lives, what kind, its catalogue id."), columns: ["Where", "Weakness", "CWE", "Urgency"], rows, rowLoc: issues.map((it) => ({ file: it.file, line: it.line } as ElementLoc)), source: "quality.json — security scan" }
        : null,
    });
  }

  // 4. code duplications
  {
    const dup = q.duplication;
    charts.push({
      id: "duplications",
      group: "Health",
      question: "Where is the same code written twice?",
      hint: (dup.blocks_found ?? 0) > 0 ? `${dup.blocks_found} repeated block(s), ${dup.duplicated_lines} lines — real matches.` : "No repeated blocks found.",
      icon: "list",
      spec: buildDuplicationSpec(q),
    });
  }

  // 5. hot spots
  {
    charts.push({
      id: "hotspots",
      group: "Health",
      question: "Which parts are the hot spots?",
      hint: "Churn × complexity × urgent findings — the formula is disclosed.",
      icon: "activity",
      spec: buildHotspotSpec(q),
    });
  }

  // 6. coverage heatmap
  {
    const s = buildCoverageHeatmapSpec(quality);
    charts.push({
      id: "coverage-heatmap",
      group: "Health",
      question: "A heat map of test coverage?",
      hint: s ? "Every file, measured — rose tiles were never even imported." : "No coverage export — honest empty.",
      icon: "shield",
      spec: s,
    });
  }

  // 7. code drift (code vs its tests)
  {
    charts.push({
      id: "code-drift",
      group: "Health",
      question: "Which code moved ahead of its tests?",
      hint: "Last change of each file vs its tests' last change (real git dates).",
      icon: "timer",
      spec: buildCodeDriftSpec(q),
    });
  }

  // 8. document drift
  {
    charts.push({
      id: "document-drift",
      group: "Health",
      question: "Which words fell behind the code?",
      hint: "Docs' last change vs the code they describe.",
      icon: "timer",
      spec: buildDocumentDriftSpec(q),
    });
  }

  // 9. most complex parts
  {
    const s = buildComplexitySpec(q);
    charts.push({
      id: "most-complex",
      group: "Health",
      question: "Where is the most twisted logic?",
      hint: s ? `Cyclomatic complexity, worst first — avg ${q.complexity.totals.avg_cc}, max ${q.complexity.totals.max_cc}.` : "No functions parsed — honest empty.",
      icon: "shapes",
      spec: s,
    });
  }

  // ---------- Quality metrics (the eight classic measures) ----------

  const metricById = new Map(q.metrics.map((m) => [m.id, m]));

  const metricChart = (
    id: string,
    chartId: string,
    question: string,
    hint: string,
    icon: CannedChart["icon"],
    extra?: { spec: ChatChartSpec | null }
  ): CannedChart => {
    const m = metricById.get(id);
    const s: ChatChartSpec | null = m
      ? {
          ...spec("stat", m.name, `${m.what_it_measures}. Typical calculation: ${m.typical_calculation}.`),
          data: [{ name: m.value_text, value: typeof m.value === "number" ? m.value : 0 }],
          unit: m.unit,
          source: `${m.source} · scope: ${m.scope}`,
        }
      : null;
    // prefer the real data chart; the stat (hero number) is the fallback
    return { id: chartId, group: "Quality metrics", question, hint, icon, spec: extra?.spec ?? s };
  };

  const d = q.defects;
  charts.push(
    metricChart("defect-density", "metric-defect-density",
      "What is the defect density?",
      (d.derived.defect_density_per_kloc ?? 0) + " fix-commits per KLOC — the tracker is empty, commit mining is the disclosed proxy.",
      "gauge",
      { spec: buildFixCommitsSpec(q, "The fix commits behind the density", "Each bar counts real fix commits landing that day — the events the density is computed from.") }),
    metricChart("mttf-mtbf", "metric-mtbf",
      "What is the mean time between failures?",
      d.derived.mtbf_hours + " h between real fix-commit timestamps.",
      "timer",
      { spec: buildFixCommitsSpec(q, "When the fix commits landed (behind MTBF)", "MTBF is the mean gap between real fix-commit timestamps; this day-level view shows when those commits actually landed.") }),
    metricChart("test-coverage", "metric-coverage",
      "How much do the tests cover?",
      "Measured by coverage.py on the sample's own test run.",
      "shield",
      { spec: buildCoverageHeatmapSpec(quality) }),
    metricChart("cyclomatic", "metric-cyclomatic",
      "How complex is the control flow?",
      `Avg ${q.complexity.totals.avg_cc}, max ${q.complexity.totals.max_cc} — real AST walk.`,
      "shapes",
      { spec: buildComplexitySpec(q) }),
    metricChart("arrival-rate", "metric-arrival",
      "How fast are defects arriving?",
      d.derived.arrival_rate_per_day + " fix-commits per day over the real history.",
      "gauge",
      { spec: buildFixCommitsSpec(q, "Fix commits per day — the arrivals behind the rate", "The arrival rate counts real fix commits per day of history; these are those commits, grouped by day.") }),
    metricChart("code-churn", "metric-churn",
      "How much is the code churning?",
      "Lines added+deleted, real git numstat.",
      "activity",
      { spec: buildDailyChurnSpec(q) ?? buildPerFileChurnSpec(q) }),
    metricChart("crash-rate", "metric-crash",
      "What share of sessions crashes?",
      `${q.crash_probe.crashes} of ${q.crash_probe.sessions} seeded sessions — run live during collection.`,
      "activity",
      { spec: buildCrashTableSpec(q) }),
    metricChart("mttr", "metric-mttr",
      "How fast does a repair land?",
      "Mean of really executed fail→fix→recheck loops, timed.",
      "timer",
      { spec: buildRepairTableSpec(q) }),
  );

  return charts;
}

// ---------------------------------------------------------------------------
// Click-to-chat: answer questions about the health charts + their elements
// straight from the collected data — no AI round-trip, nothing invented.
// ---------------------------------------------------------------------------

export interface QualityAnswer {
  answer: string;
  chart: ChatChartSpec | null;
}

/**
 * The chart for a per-file answer (issue #3): scoped to that file wherever
 * the collected data allows, otherwise the matching project-wide chart with
 * the asked file outlined. Never again the same undifferentiated
 * whole-project heatmap for every single-file question.
 *
 * Priority = how concrete the per-file evidence is:
 *   1. security findings on the file → this file's own rows, with lines
 *   2. the file's own functions → complexity bars scoped to the file
 *   3. coverage → the measured heatmap with this file's tile outlined
 *   4. hot-spot risk → the ranked bars with this file outlined (context)
 *   5. churn → per-file churn with this file outlined (context)
 *   6. drift-only → the file's own drift row as a one-row table
 *   7. nothing matched well enough → null (the honest empty state)
 */
function fileScopedChart(
  q: CieQuality,
  f: {
    file: string;
    cc?: (typeof q.complexity.files)[number];
    churn?: (typeof q.churn.sample.per_file)[number];
    sec: (typeof q.security.issues)[number][];
    cov?: (typeof q.coverage_heatmap.files)[number];
    spot?: (typeof q.hotspots.rows)[number];
    drift?: (typeof q.drift.code)[number];
  }
): ChatChartSpec | null {
  // 1. Concrete findings on this file — the most actionable evidence.
  if (f.sec.length) {
    return {
      ...spec(
        "table",
        `Security findings in ${f.file}`,
        "Only this file's rows from the disclosed rule-set scan — line numbers included. Project-wide counts stay one tap away."
      ),
      columns: ["Line", "Finding", "CWE", "Urgency"],
      rows: f.sec.map((it) => [String(it.line), cut(it.name, 34), it.cwe, it.severity]),
      rowLoc: f.sec.map((it) => ({ file: f.file, line: it.line } as ElementLoc)),
      source: "quality.json — security scan, scoped to the file you asked about",
    };
  }

  // 2. This file's own functions, complexity worst-first — a genuinely scoped chart.
  const fns = (q.complexity.functions ?? [])
    .filter((fn) => fn.file === f.file)
    .sort((a, b) => b.cc - a.cc)
    .slice(0, 8);
  if (fns.length) {
    return {
      ...spec("hbar", `Complexity inside ${cut(f.file, 26)}`, `${q.complexity.formula} — this file's own functions only, worst first.`),
      data: fns.map((fn) => ({
        name: cut(fn.function, 20),
        value: fn.cc,
        color: fn.cc >= q.complexity.totals.max_cc ? "rose" : fn.cc >= 4 ? "amber" : "stone",
        loc: { file: fn.file, line: fn.line } as ElementLoc,
      })),
      source: `quality.json — ${q.complexity.tool}, scoped to the file you asked about`,
    };
  }

  // 3. Coverage — the whole-project picture is the honest one here, but the
  //    asked file's tile is outlined so the chart names what was asked.
  if (f.cov) {
    const heat = buildCoverageHeatmapSpec(q);
    if (heat) {
      return {
        ...heat,
        tiles: (heat.tiles ?? []).map((t) => (t.loc?.file === f.file ? { ...t, highlight: true } : t)),
        tell: `${heat.tell ?? ""} The dark-outlined tile is ${f.file} — the file you asked about.`.trim(),
      };
    }
  }

  // 4. Hot-spot risk — ranked project context, asked file outlined.
  if (f.spot) {
    const hs = buildHotspotSpec(q);
    if (hs) {
      return {
        ...hs,
        data: (hs.data ?? []).map((d) => (d.loc?.file === f.file ? { ...d, highlight: true } : d)),
        tell: `${hs.tell ?? ""} The dark-outlined bar is ${f.file}, in project context.`.trim(),
      };
    }
  }

  // 5. Churn — per-file ranking, asked file outlined.
  if (f.churn) {
    const ch = buildPerFileChurnSpec(q);
    if (ch) {
      return {
        ...ch,
        data: (ch.data ?? []).map((d) => (d.loc?.file === f.file ? { ...d, highlight: true } : d)),
        tell: `${ch.tell ?? ""} The dark-outlined bar is ${f.file}.`.trim(),
      };
    }
  }

  // 6. Drift-only match — the file's own row, as a small honest table.
  if (f.drift) {
    return {
      ...spec("table", `Drift for ${cut(f.file, 26)}`, "This file's own row from the drift table (git dates, as recorded)."),
      columns: ["File", "Code last changed", "Tests last changed", "Lag (days)"],
      rows: [[cut(f.file, 24), f.drift.last_change ?? "—", f.drift.tests_last_change ?? "no test file", f.drift.test_lag_days != null ? String(f.drift.test_lag_days) : "—"]],
      rowLoc: [{ file: f.file } as ElementLoc],
      source: "quality.json — drift (git dates), scoped to the file you asked about",
    };
  }

  return null;
}

function fileFacts(q: CieQuality, name: string): QualityAnswer | null {
  // exact filename tokens first ("auth.py", "001_init.sql"), then stem match
  const tokens: string[] = name.toLowerCase().match(/[a-z0-9_\-]+\.(py|sql|md|txt|sh)/g) ?? [];
  const exactFile = tokens.length
    ? q.complexity.files.find((f) => tokens.includes(f.file.toLowerCase()))?.file ??
      q.coverage_heatmap.files.find((f) => tokens.includes(f.file.toLowerCase()))?.file
    : undefined;

  const base = (exactFile ?? name).toLowerCase().replace(/\.(py|sql|md|txt|sh)$/, "");
  const matchFile = (f: string) => {
    const lf = f.toLowerCase();
    if (exactFile) return lf === exactFile.toLowerCase();
    return lf.includes(base) && base.length > 2;
  };
  if (base.length <= 2 && !exactFile) return null;

  const cc = q.complexity.files.find((f) => matchFile(f.file));
  const churn = q.churn.sample.per_file.find((r) => matchFile(r.file));
  const sec = q.security.issues.filter((it) => matchFile(it.file));
  const cov = q.coverage_heatmap.files.find((f) => matchFile(f.file));
  const spot = q.hotspots.rows.find((r) => matchFile(r.file));
  const drift = q.drift.code.find((c) => matchFile(c.file));
  if (!cc && !churn && !sec.length && !cov && !spot) return null;

  // the heading names the file the data actually matched — never the raw
  // question (which can be a whole sentence when the panel grounds it)
  const matchedFile = cc?.file ?? churn?.file ?? cov?.file ?? spot?.file ?? drift?.file ?? exactFile ?? name;
  const lines: string[] = [];
  lines.push(`**Everything collected about \`${matchedFile}\`**`);
  if (cc) lines.push(`**Complexity:** ${cc.functions} functions, avg CC ${cc.avg_cc}, max ${cc.max_cc} (formula: ${q.complexity.formula}).`);
  if (churn) lines.push(`**Churn:** ${churn.added} lines added + ${churn.deleted} deleted over ${churn.commits} commit(s); last changed ${churn.last_change ?? "unknown"}.`);
  if (cov) lines.push(`**Test coverage:** ${cov.percent}% (${cov.note ?? (cov.covered != null ? `${cov.covered} of ${cov.statements} lines` : "never even imported")}) — measured by ${q.coverage_heatmap.tool}.`);
  if (spot) lines.push(`**Hot-spot risk:** ${spot.risk}/100 (${q.hotspots.formula}).`);
  if (sec.length) {
    lines.push(`**Security findings:** ${sec.map((it) => `${it.name} (${it.cwe}) at line ${it.line}`).join("; ")}.`);
  } else if (cc || cov) {
    lines.push(`**Security findings:** none on this file from the disclosed rule set.`);
  }
  if (drift) lines.push(`**Drift:** ${drift.note}; code last changed ${drift.last_change}, tests ${drift.tests_last_change ?? "none"}.`);
  lines.push(`_From quality.json — collected by the ETL collector on ${q.collected_at.slice(0, 10)}._`);
  return {
    answer: lines.join("\n\n"),
    // Chart scoped/highlighted to THIS file, matched to the evidence the
    // answer actually cites — not a fixed project-wide heatmap (issue #3).
    chart: fileScopedChart(q, { file: matchedFile, cc, churn, sec, cov, spot, drift }),
  };
}

function securityFacts(q: CieQuality, needle: string): QualityAnswer | null {
  const n = needle.toLowerCase();
  // a generic "are there security problems" question should NOT pin one
  // category — only match when a concrete kind / CWE / rule is named
  const hitCategory = Object.keys(q.security.by_category ?? {}).find((c) => n.includes(c.toLowerCase().slice(0, 14)));
  const hitIssue = q.security.issues.find((it) => n.includes(it.cwe.toLowerCase()) || n.includes(it.rule.toLowerCase()) || n.includes(it.name.toLowerCase()));
  if (!hitCategory && !hitIssue) return null;
  const name = hitIssue?.name ?? hitCategory!;
  const rows = q.security.issues.filter((it) => it.name === name || (hitIssue && it.id === hitIssue.id));
  const list = rows.map((it) => `- ${it.file}:${it.line} — ${it.cwe}, ${it.severity.toLowerCase()} urgency. Why it matters: ${it.why}`);
  const answer = [
    `**${name} — ${rows.length} finding(s) in the sample**`,
    list.join("\n"),
    `The scan is ${q.security.tool} — heuristics with file:line evidence, not a full SAST scanner. On production projects the same slots come from ${q.security.real_world_equivalents?.join(", ") ?? "bandit, semgrep, CodeQL"}.`,
    `_Ask about a specific file name to see its full collected facts._`,
  ].join("\n\n");
  const data = Object.entries(q.security.by_category ?? {}).map(([k, v]) => ({
    name: cut(k, 24),
    value: v,
    color: k.includes("Hardcoded") || k.includes("SQL") ? "rose" : "amber",
  }));
  return {
    answer,
    chart: data.length
      ? { ...spec("hbar", "Security issues by kind", "Counts per finding kind, from the disclosed rule set."), data, source: "quality.json — security scan" }
      : null,
  };
}

function functionFacts(q: CieQuality, needle: string): QualityAnswer | null {
  const n = needle.toLowerCase();
  const fn = q.complexity.functions.find((f) => f.function.toLowerCase() === n.replace(/[^a-z0-9_]/g, "") || n.includes(` ${f.function.toLowerCase()} `) || n.includes(`"${f.function.toLowerCase()}"`));
  if (!fn) return null;
  const churnRow = q.churn.sample.per_file.find((r) => r.file === fn.file);
  const cov = q.coverage_heatmap.files.find((f) => f.file === fn.file);
  const answer = [
    `**\`${fn.function}\` (in ${fn.file}, line ${fn.line})**`,
    `Cyclomatic complexity **${fn.cc}** — ${q.complexity.formula}. Its file's max CC is ${q.complexity.files.find((f) => f.file === fn.file)?.max_cc}.`,
    churnRow ? `The file churned ${churnRow.added + churnRow.deleted} lines over ${churnRow.commits} commit(s).` : "",
    cov ? `Test coverage of ${fn.file}: ${cov.percent}%.` : "",
    `Measured by ${q.complexity.tool}. On production projects: ${q.complexity.real_world_equivalents?.join(", ") ?? "radon, lizard, SonarQube"}.`,
  ].filter(Boolean).join("\n\n");
  // companion: the tapped function shown among its file-mates, worst first
  const mates = q.complexity.functions.filter((f) => f.file === fn.file).sort((a, b) => b.cc - a.cc).slice(0, 8);
  const chart: ChatChartSpec | null = mates.length
    ? {
        ...spec("hbar", `Complexity in ${fn.file}`, `${q.complexity.formula} — the function you tapped is marked.`),
        data: mates.map((f) => ({
          name: cut(f.function, 20),
          value: f.cc,
          color: f.function === fn.function ? "rose" : f.cc >= 4 ? "amber" : "stone",
          loc: { file: f.file, line: f.line } as ElementLoc,
        })),
        source: `quality.json — ${q.complexity.tool}`,
      }
    : null;
  return { answer, chart };
}

function metricFacts(q: CieQuality, needle: string): QualityAnswer | null {
  const n = needle.toLowerCase();
  const m = q.metrics.find((mm) =>
    (mm.id === "defect-density" && (n.includes("defect density") || n.includes("density"))) ||
    (mm.id === "mttf-mtbf" && (n.includes("mtbf") || n.includes("mttf") || n.includes("between failures"))) ||
    (mm.id === "test-coverage" && n.includes("coverage") && !n.includes("heatmap") && !n.includes("heat map")) ||
    (mm.id === "cyclomatic" && (n.includes("cyclomatic") || (n.includes("complex") && n.includes("control")))) ||
    (mm.id === "arrival-rate" && (n.includes("arrival") || n.includes("how fast are defects"))) ||
    (mm.id === "code-churn" && n.includes("churn")) ||
    (mm.id === "crash-rate" && (n.includes("crash") || /\bhangs?\b/.test(n))) ||
    (mm.id === "mttr" && (n.includes("mttr") || n.includes("repair") || n.includes("time to resolve")))
  );
  if (!m) return null;
  const answer = [
    `**${m.name}: ${m.value_text}**`,
    `**What it measures:** ${m.what_it_measures}. **Typical calculation:** ${m.typical_calculation}.`,
    `**How it was computed here:** ${m.how_computed_here}`,
    `**Source:** ${m.source} · **Scope:** ${m.scope}.`,
    q.defects.derived.proxy_note && (m.id === "defect-density" || m.id === "mttf-mtbf" || m.id === "arrival-rate") ? `**Proxy disclosure:** ${q.defects.derived.proxy_note}` : "",
  ].filter(Boolean).join("\n\n");
  // Every metric answer carries the chart of the REAL data behind the
  // number — same builder the one-tap menu uses, so answer and card agree.
  let chart: ChatChartSpec | null = null;
  switch (m.id) {
    case "test-coverage":
      chart = buildCoverageHeatmapSpec(q);
      break;
    case "code-churn":
      chart = buildDailyChurnSpec(q) ?? buildPerFileChurnSpec(q);
      break;
    case "cyclomatic":
      chart = buildComplexitySpec(q);
      break;
    case "crash-rate":
      chart = buildCrashTableSpec(q);
      break;
    case "mttr":
      chart = buildRepairTableSpec(q);
      break;
    case "defect-density":
      chart = buildFixCommitsSpec(q, "The fix commits behind the density", `Each bar counts real fix commits landing that day. Density = ${q.defects.fix_commits.count} fix commits ÷ ${q.defects.derived.kloc_measured ?? "?"} KLOC = ${q.defects.derived.defect_density_per_kloc ?? "?"}/KLOC.`);
      break;
    case "mttf-mtbf":
      chart = buildFixCommitsSpec(q, "When the fix commits landed (behind MTBF)", "MTBF is the mean gap between real fix-commit timestamps; this day-level view shows when those commits actually landed.");
      break;
    case "arrival-rate":
      chart = buildFixCommitsSpec(q, "Fix commits per day — the arrivals behind the rate", "The arrival rate counts real fix commits per day of history; these are those commits, grouped by day.");
      break;
  }
  return { answer, chart };
}

export function tryAnswerQuality(question: string, quality: CieQuality | null, concepts: ConceptTree | null): QualityAnswer | null {
  if (!quality) return null;
  const q = quality;
  const n = question.toLowerCase();

  // ---------- element-level answers FIRST (a tapped bar/tile/row must win
  // over the whole-chart answer, whose keywords often appear in the same
  // question) — UNLESS the question is asked in the metric's own vocabulary
  // ("quality metric", "what it measures", "how was it computed"). Those
  // want THE metric: without the guard, "code churn" collides with the
  // sample's function actually named `churn` and mis-answers. ----------
  if (
    n.includes("quality metric") || n.includes("what it measures") ||
    n.includes("how was it computed") || n.includes("typical calculation") ||
    n.includes("collected value")
  ) {
    const mFirst = metricFacts(q, question);
    if (mFirst) return mFirst;
  }
  const fileAns = fileFacts(q, question);
  if (fileAns) return fileAns;
  const fnAns = functionFacts(q, question);
  if (fnAns) return fnAns;
  const secEl = securityFacts(q, question);
  if (secEl) return secEl;
  const dayMatch = question.match(/\d{4}-\d{2}-\d{2}/);
  if (dayMatch) {
    const row = q.churn.hosting_repo.daily.find((d) => d.day === dayMatch[0]);
    if (row) {
      return {
        answer: `**Churn on ${row.day}** — ${row.added} lines added, ${row.deleted} deleted (${row.churn} total), from ${q.churn.tool} over the hosting repository's real history. The biggest day of this sprint is the day the sample playground landed.`,
        chart: buildDailyChurnSpec(q),
      };
    }
  }

  // ---------- whole-chart keyword answers ----------
  const wants = (w: string[]) => w.some((k) => n.includes(k));

  if (wants(["hierarchy", "made of"]) && !wants(["file "])) {
    const s = buildConceptTreeSpec(concepts);
    if (s) return { answer: `**The concept hierarchy** — the big picture first, then the files under it, then what each does. Every node comes from the engine's concept tree; tap any node to ask about it.\n\n_Source: concepts.json._`, chart: s };
  }
  if (wants(["security", "insecure", "secret", "credential", "vulnerab", "weakness"])) {
    const sec = securityFacts(q, question) ?? fileFacts(q, question);
    if (sec) return sec;
    const data = Object.entries(q.security.by_category ?? {}).map(([k, v]) => ({ name: cut(k, 24), value: v, color: "amber" as const }));
    if (data.length) {
      return {
        answer: `**The security scan found ${q.security.issues.length} findings** (${Object.entries(q.security.by_severity ?? {}).map(([s, c]) => `${c} ${s.toLowerCase()}`).join(", ")}). Kinds: ${Object.entries(q.security.by_category).map(([k, v]) => `${v}× ${k}`).join(", ")}.\n\nThe scan is ${q.security.tool} — ${q.security.honesty} On production projects: ${q.security.real_world_equivalents?.join(", ")}.`,
        chart: { ...spec("hbar", "Security issues by kind", "Counts per finding kind."), data, source: "quality.json — security scan" },
      };
    }
  }
  if (wants(["duplicat", "twice", "copy-paste", "copy paste", "clone", "repeat"])) {
    const dup = q.duplication;
    const rows = (dup.pairs ?? []).slice(0, 6).map((p) => `- ${p.a_file}:${p.a_line} ↔ ${p.b_file}:${p.b_line} — ${p.lines} lines (${p.preview || "same block"})`);
    return {
      answer: rows.length
        ? `**${dup.blocks_found} duplicated block(s), ${dup.duplicated_lines} lines** — matched by ${dup.tool}:\n${rows.join("\n")}\n\nHere the duplicated blocks are the CREATE TABLE definitions shared between the migrations and the schema dump — a classic sample of schema defined twice.`
        : `No repeated blocks found by ${dup.tool} — recorded as found.`,
      chart: buildDuplicationSpec(q),
    };
  }
  if (wants(["hot spot", "hotspot", "hot spot"])) {
    const rows = (q.hotspots.rows ?? []).slice(0, 5).map((r) => `- ${r.file} — risk ${r.risk}/100 (churn ${r.churn}, max CC ${r.max_cc}, urgent findings ${r.urgent_findings})`);
    return { answer: `**Hot spots, ranked** by the disclosed formula: ${q.hotspots.formula}.\n${rows.join("\n")}\n\nOn production projects: ${q.hotspots.real_world_equivalents?.join(", ") ?? "CodeScene, SonarQube"}.`, chart: buildHotspotSpec(q) };
  }
  if (wants(["heatmap", "heat map", "coverage map"]) || (wants(["coverage"]) && wants(["every", "file by file", "grid"]))) {
    const s = buildCoverageHeatmapSpec(q);
    if (s) return { answer: `**The coverage heat map** — every file, measured by ${q.coverage_heatmap.tool}. ${q.coverage_heatmap.honesty}\n\n_Tap any tile to ask about that file._`, chart: s };
  }
  if (wants(["code drift", "ahead of its tests", "test lag"])) {
    const rows = (q.drift.code ?? []).slice(0, 6).map((c) => `- ${c.file} — code ${c.last_change}, tests ${c.tests_last_change ?? "none"}${c.test_lag_days != null ? ` (lag ${c.test_lag_days}d)` : ""}`);
    return { answer: `**Code drift vs tests** (${q.drift.tool}):\n${rows.join("\n")}\n\n${q.drift.honesty}`, chart: buildCodeDriftSpec(q) };
  }
  if (wants(["document drift", "docs behind", "words fell"])) {
    const rows = (q.drift.document ?? []).slice(0, 6).map((d) => `- ${d.doc} — ${d.note}${d.drift_days > 0 ? ` (${d.drift_days}d behind)` : ""}`);
    return { answer: `**Document drift** (${q.drift.tool}):\n${rows.join("\n")}\n\n${q.drift.honesty}`, chart: buildDocumentDriftSpec(q) };
  }

  // metric answers
  const mAns = metricFacts(q, question);
  if (mAns) return mAns;

  return null;
}

/**
 * The de-facto companion chart: when ANY answer comes back without a chart,
 * this picks the most relevant real chart from the loaded data — so every
 * reply explains itself with a picture drawn from the collected exports.
 * Ranked keyword match, then an honest fallback order. Never invents: every
 * spec comes from the same builders the menu uses, or renders an honest empty.
 */
export function buildCompanionChart(
  question: string,
  quality: CieQuality | null,
  concepts: ConceptTree | null,
  report?: CieReport | null,
  mapSelected?: VizElement | null,
  cochange?: CieCochange | null,
  overlap?: CieOverlap | null,
  drift?: CieDrift | null
): ChatChartSpec | null {
  const n = question.toLowerCase();
  const wants = (w: string[]) => w.some((k) => n.includes(k));

  // Repeated behavior (concepts.json's FCA pass) works on ANY project, not
  // just the bundled demo — try it first so a real project's duplication
  // question gets a real chart instead of falling through to the universal
  // concept-tree default. quality.json's own duplication table (demo-only)
  // stays the fallback below, for the cases the FCA pass can't see (e.g.
  // non-function symbols like SQL table definitions).
  if (wants(["duplicat", "copy-paste", "copy paste", "twice", "clone", "repeat"])) {
    const s = buildRepeatedBehaviorSpec(concepts, report ?? null);
    if (s) return s;
  }

  // Co-change, overlap and drift are all real `cie` engine data (cochange/
  // overlap/drift subcommands) — work the same way on any project, so they
  // get checked before the quality.json-only (demo-only) block below.
  if (wants(["co-change", "cochange", "change together", "changed together", "committed together", "edited together", "coupling", "coupled"])) {
    const s = buildCochangeSpec(cochange ?? null);
    if (s) return s;
  }
  if (wants(["overlap", "already exist", "already have", "reuse", "redundant", "similar capability"])) {
    const s = buildOverlapSpec(overlap ?? null);
    if (s) return s;
  }
  if (wants(["drift", "stale doc", "out of date", "outdated", "docs match", "documentation accurate", "docs accurate", "docs still"])) {
    const s = buildDriftSpec(drift ?? null);
    if (s) return s;
  }

  if (quality) {
    const q = quality;
    if (wants(["coverage", "test", "tests"])) {
      const s = buildCoverageHeatmapSpec(q);
      if (s) return s;
    }
    if (wants(["churn", "change", "unstable"])) {
      const s = buildDailyChurnSpec(q) ?? buildPerFileChurnSpec(q);
      if (s) return s;
    }
    if (wants(["defect", "bug", "density", "mtbf", "mttf", "arrival", "failure"])) {
      const s = buildFixCommitsSpec(q, "The fix commits behind the answer", "Each bar counts real fix commits landing that day — the events the defect metrics are computed from.");
      if (s) return s;
    }
    if (wants(["complex", "cyclomatic", "twisted", "control flow"])) {
      const s = buildComplexitySpec(q);
      if (s) return s;
    }
    if (wants(["security", "secret", "credential", "vulnerab", "weakness", "insecure"])) {
      const data = Object.entries(q.security.by_category ?? {}).map(([k, v]) => ({ name: cut(k, 24), value: v, color: "amber" as const }));
      if (data.length) return { ...spec("hbar", "Security issues by kind", "Counts per finding kind, from the disclosed rule set."), data, source: "quality.json — security scan" };
    }
    if (wants(["duplicat", "copy-paste", "copy paste", "twice", "clone", "repeat"])) {
      const s = buildDuplicationSpec(q);
      if (s) return s;
    }
    if (wants(["hot spot", "hotspot", "fix first", "priority", "risk", "worst"])) {
      const s = buildHotspotSpec(q);
      if (s) return s;
    }
    if (wants(["drift", "stale", "behind"])) {
      const s = buildCodeDriftSpec(q) ?? buildDocumentDriftSpec(q);
      if (s) return s;
    }
    if (wants(["crash", "hang"])) {
      const s = buildCrashTableSpec(q);
      if (s) return s;
    }
    if (wants(["repair", "mttr", "resolve"])) {
      const s = buildRepairTableSpec(q);
      if (s) return s;
    }
  }
  // universal default: what the project is made of — the picture that gives
  // any answer its context. Centred on whatever the question is actually
  // about (resolveConceptFocus), never the blind whole-project tree, when
  // that can be resolved. Falls through the real exports in order.
  const focusId = resolveConceptFocus(concepts, report ?? null, question, mapSelected ?? null);
  const tree = buildConceptTreeSpec(concepts, focusId);
  if (tree) return tree;
  if (quality) {
    return (
      buildHotspotSpec(quality) ??
      buildComplexitySpec(quality) ??
      buildDailyChurnSpec(quality) ??
      buildCoverageHeatmapSpec(quality)
    );
  }
  return null;
}

/**
 * The question a chart element hands to the chat when tapped. When the
 * element carries a source location, the question names the exact file and
 * line; when the caller also holds the real code (sourceCache), a small
 * verbatim snippet rides along — so the answerer (and the AI fallback)
 * ground the reply in the very code the user is looking at. The snippet is
 * optional and always verbatim: no snippet is sent rather than an invented one.
 */
export function askAboutChartElement(
  chartTitle: string | undefined,
  el: { name: string; value?: number | string; loc?: ElementLoc; snippet?: string }
): string {
  const v = el.value !== undefined ? ` (value ${el.value})` : "";
  const where = el.loc?.file
    ? el.loc.line != null
      ? ` in ${el.loc.file} at line ${el.loc.line}`
      : ` in ${el.loc.file}`
    : "";
  const base = `About the chart "${chartTitle ?? "untitled"}" — tell me about "${el.name}"${v}${where}. What does the collected data say about it, and how was it collected?`;
  return el.snippet && el.loc?.file ? `${base}\n\nThe code at that spot (${el.loc.file}${el.loc.line != null ? `, line ${el.loc.line}` : ""}):\n${el.snippet}` : base;
}

/** A small verbatim, line-numbered window around one line — built from real
 * content the caller already has (store sourceCache). Never guessed. */
export function snippetAround(content: string, line: number | null | undefined, before = 3, after = 4): string | undefined {
  if (line == null || line < 1) return undefined;
  const lines = content.split("\n");
  if (line > lines.length) return undefined;
  const start = Math.max(1, line - before);
  const end = Math.min(lines.length, line + after);
  const width = String(end).length;
  const out: string[] = [];
  for (let i = start; i <= end; i++) {
    const mark = i === line ? ">" : " ";
    out.push(`${mark} ${String(i).padStart(width)} | ${lines[i - 1].slice(0, 120)}`);
  }
  return out.join("\n");
}
