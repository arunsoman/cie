// Drilldowns for the one-tap chart menu: pick a question → instead of one
// static picture, you open a small dashboard of LINKED charts plus a viewer.
//
// One question = one plan. A plan carries:
//   rows — the real units behind the question (findings, weaknesses,
//          packages, functions, contracts, map bubbles…), each with the two
//          aggregation axes it belongs to and, wherever the export recorded
//          one, its source location.
//   meta — what the two linked charts aggregate (kind/axis/labels), the row
//          list's title, and the viewer's per-question content.
//
// The component (components/cie/menu-drilldown.tsx) renders a plan as:
//   chart 1 (axis A) + chart 2 (axis B) + matching rows + a viewer that
//   stays empty until a selection fills it. Picking a bucket on one axis
//   re-aggregates the other, filters the rows, and — whenever the surviving
//   rows share one file — opens that file in the viewer. Nothing is invented.
//
// Reused infra (deliberately not rewritten): the file loading + "Ask CIE"
// plumbing of the source panel (loadDemoSources, sourceCache, the captured-source storage API),
// askAboutChartElement/snippetAround (quality.ts), the plain-words
// translators, and ChatChart's interactive canvas for diagram-backed
// questions (UML, call flow, concept tree, heat maps).

import type {
  CieReport,
  CieViewSpec,
  ConceptTree,
  CieDeps,
  CieRuntime,
  CieQuality,
  Finding,
  DeclaredDep,
} from "./cie-types";
import {
  findingTitle,
  severityPlain,
  authorityPlain,
  certaintyPlain,
  elementKindPlain,
  effectPlain,
  obligationTitle,
} from "./plain-words";

// ---------------------------------------------------------------- types ---

/** Where a row lives in the source — the viewer's anchor. A flat line when
 *  the export carried one, byte offsets when it carried a symbol span, and
 *  nothing at all when neither exists (the viewer then shows its honest
 *  per-question explanation instead of guessing). */
export interface DrillLoc {
  file: string;
  line?: number;
  endLine?: number;
  byteStart?: number;
  byteEnd?: number;
}

export interface DrillRow {
  id: string;
  /** Axis A: a stable selection key (full path, raw kind…) + display label. */
  keyA: string;
  a: string;
  /** Axis B: same pair, for the second chart. */
  keyB: string;
  b: string;
  title: string;
  detail: string;
  severity?: string; // raw severity, when the row carries one (chip + sort)
  extra?: string; // CWE, sha, licence… a short mono chip on the row
  /** Sum-aggregated on the axes (ms, %, lines…); absent → each row counts 1. */
  value?: number;
  marks?: number[]; // every evidence line the row recorded (viewer highlight)
  loc?: DrillLoc; // primary anchor
  loc2?: DrillLoc; // the paired twin (the duplicated other side, doc→code…)
  /** What the row's "Ask CIE" button sends: the canned three-part question
   *  (what is it / impact / how to fix) plus every fact that grounds it. */
  askCtx?: { headline?: string; message?: string; remediation?: string };
}

export type DrillAxis = "bar" | "pie" | "none";

export interface DrillMeta {
  axisA: { title: string; tell: string };
  axisB: { title: string; tell: string };
  kindA: DrillAxis;
  kindB: DrillAxis;
  aName: string; // "File", "Urgency"…
  bName: string; // "Kind of error", "Manifest file"…
  rowName: string; // what one row is — "problem", "package", "bubble"…
  listTitle: string;
  blurb: string; // the question-specific one-liner under the header
  /** Empty-state + caption: each question explains ITS OWN viewer. */
  viewer: { kind: "code" | "commit" | "none"; empty: string; caption: string };
  valueUnit?: string; // for rows with a value (ms, %, lines…)
  source?: string; // where the rows come from (honesty footer)
}

export interface RowsDrill {
  kind: "rows";
  rows: DrillRow[];
  meta: DrillMeta;
}

/** Questions whose picture is ALREADY an interactive canvas (UML boxes, call
 *  flow, concept tree, heat maps): the drill keeps the ChatChart canvas and
 *  its element-click → source panel behaviour, and adds the back affordance. */
export interface ChartDrill {
  kind: "chart";
  note: string;
}

export type Drill = RowsDrill | ChartDrill;

// ------------------------------------------------------------- helpers ---

function cut(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

const baseOf = (p: string) => p.split("/").pop() ?? p;
const containerOf = (p: string) => (p.includes("/") ? p.slice(0, p.indexOf("/")) : "(project root)");

function bucketFor(v: number, edges: number[], labels: string[]): string {
  for (let i = 0; i < edges.length; i++) if (v < edges[i]) return labels[i];
  return labels[labels.length - 1];
}

const SEV_ORDER = ["Critical", "High", "Medium", "Low"];

const ECO_NAMES: Record<string, string> = {
  pypi: "Python (pip)", npm: "npm", cargo: "Rust", go: "Go",
  maven: "Maven", gradle: "Gradle", composer: "PHP", nuget: ".NET",
};
const ecoName = (e: string) => ECO_NAMES[e] ?? e;

/** Move severity to axis A, file to axis B (the urgency questions).
 *  Axis keys stay stable: severity by its raw string, files by full path. */
function bySeverityAxis(rows: DrillRow[]): DrillRow[] {
  return rows.map((r) => ({
    ...r,
    keyA: r.severity ?? "Low",
    a: severityPlain(r.severity ?? "Low").label,
    keyB: r.keyA, // the file (full path)
    b: r.a, // the file (short label)
  }));
}

/** Move kind to axis A, file to axis B (the problem-kinds question). */
function byKindAxis(rows: DrillRow[]): DrillRow[] {
  return rows.map((r) => ({
    ...r,
    keyA: r.keyB, // the kind
    a: r.b, // the kind label
    keyB: r.keyA, // the file (full path)
    b: r.a, // the file (short label)
  }));
}

// ------------------------------------------------------------ row makers ---

/** The engine's findings — file (×) kind, urgency chip, evidence lines. */
export function findingRows(report: CieReport): DrillRow[] {
  const symById = new Map(report.symbols.map((s) => [s.id, s]));
  return report.findings.map((f, i) => {
    const sym = symById.get(f.symbol);
    const file = sym ? report.files[sym.loc.file] ?? "" : "";
    const kind = findingTitle(f);
    const nums = (f.evidence_lines ?? []).filter((n) => Number.isFinite(n) && n >= 1);
    const line = nums.length ? Math.min(...nums) : undefined;
    const endLine = nums.length > 1 ? Math.max(...nums) : undefined;
    return {
      id: f.id || `finding-${i}`,
      keyA: file || "(no file recorded)",
      a: file ? baseOf(file) : "(no file recorded)",
      keyB: kind,
      b: kind,
      title: f.symbol_name || kind,
      detail: f.message,
      severity: f.severity,
      marks: nums,
      loc: file
        ? { file, ...(line != null ? { line } : {}), ...(endLine != null ? { endLine } : {}) }
        : undefined,
      askCtx: { headline: kind, message: f.message, remediation: f.remediation || undefined },
    };
  });
}

/** Every file as a unit — the clean-files question. */
function fileRows(report: CieReport): DrillRow[] {
  const symById = new Map(report.symbols.map((s) => [s.id, s]));
  const byFile = new Map<number, Finding[]>();
  for (const f of report.findings) {
    const sym = symById.get(f.symbol);
    if (!sym) continue;
    const arr = byFile.get(sym.loc.file) ?? [];
    arr.push(f);
    byFile.set(sym.loc.file, arr);
  }
  const max = Math.max(1, ...[...byFile.values()].map((v) => v.length));
  return report.files.map((file, i) => {
    const fs = byFile.get(i) ?? [];
    const count = fs.length;
    const worst = count ? findingTitle(fs[0]) : "";
    return {
      id: `file-${i}`,
      keyA: count ? "Has problems" : "Completely clean",
      a: count ? "Has problems" : "Completely clean",
      keyB: file,
      b: baseOf(file),
      title: baseOf(file),
      detail: count
        ? `${count} ${count === 1 ? "problem" : "problems"} flagged${count === max ? " — the most-troubled file in the project" : ""}, first is “${worst}”.`
        : "CIE flagged nothing here at all.",
      loc: { file },
    };
  });
}

/** Named things (symbols) — the things-per-file question. */
function symbolRows(report: CieReport): DrillRow[] {
  const byFile = new Map<number, number>();
  for (const s of report.symbols) byFile.set(s.loc.file, (byFile.get(s.loc.file) ?? 0) + 1);
  const max = Math.max(1, ...byFile.values());
  return report.symbols.map((s) => {
    const file = report.files[s.loc.file] ?? "";
    const kind = (s.kind || "named thing").toLowerCase();
    const fileCount = byFile.get(s.loc.file) ?? 1;
    return {
      id: `sym-${s.id}`,
      keyA: file || "(no file recorded)",
      a: file ? baseOf(file) : "(no file recorded)",
      keyB: kind,
      b: kind,
      title: s.name,
      detail:
        fileCount === max
          ? `One of the named things in the busiest file in the project (${fileCount} here).`
          : `A ${kind} in this file (${fileCount} named things here).`,
      loc: { file, byteStart: s.loc.start, byteEnd: s.loc.end },
    };
  });
}

/** One row per recorded outside-world touch (summary × effect). */
function effectRows(report: CieReport): DrillRow[] {
  const symById = new Map(report.symbols.map((s) => [s.id, s]));
  const rows: DrillRow[] = [];
  for (const s of report.summaries) {
    for (const e of s.effects ?? []) {
      const sym = symById.get(s.sym);
      const file = sym ? report.files[sym.loc.file] ?? "" : "";
      const plain = effectPlain(e);
      rows.push({
        id: `fx-${s.sym}-${e}`,
        keyA: file || "(no file recorded)",
        a: file ? baseOf(file) : "(no file recorded)",
        keyB: e,
        b: plain,
        title: sym?.name ?? `symbol ${s.sym}`,
        detail: `${plain} — recorded by the engine's summary for this symbol.`,
        loc: sym ? { file, byteStart: sym.loc.start, byteEnd: sym.loc.end } : undefined,
      });
    }
  }
  return rows;
}

/** Concept modules × their recognised behaviours — the file-does-what drill. */
function moduleRows(concepts: ConceptTree, report: CieReport): DrillRow[] {
  const symById = new Map(report.symbols.map((s) => [s.id, s]));
  const rows: DrillRow[] = [];
  for (const n of concepts.nodes.filter((n) => n.kind === "Module")) {
    const sym = (n.symbols ?? []).map((id) => symById.get(id)).find(Boolean);
    const file = sym ? report.files[sym.loc.file] ?? "" : "";
    const kinds = [...new Set((n.behaviors ?? []).map((b) => b.kind))];
    for (const k of kinds) {
      const count = (n.behaviors ?? []).filter((b) => b.kind === k).length;
      const plain = k.replace(/_/g, " ").toLowerCase();
      rows.push({
        id: `mod-${n.id}-${k}`,
        keyA: file || n.label,
        a: file ? baseOf(file) : n.label,
        keyB: k,
        b: plain,
        title: n.label,
        detail: `${count} ${count === 1 ? "behaviour" : "behaviours"} of this kind recognised in this file.`,
        loc: sym ? { file, byteStart: sym.loc.start, byteEnd: sym.loc.end } : undefined,
      });
    }
  }
  return rows;
}

/** Contracts (promises) — both promise questions. */
function contractRows(report: CieReport): DrillRow[] {
  const symById = new Map(report.symbols.map((s) => [s.id, s]));
  return report.contracts.map((c) => {
    const sym = (c.symbols ?? []).map((s) => symById.get(s)).find(Boolean);
    const file = sym ? report.files[sym.loc.file] ?? "" : "";
    const obl = obligationTitle(c.obligation);
    return {
      id: `contract-${c.id}`,
      keyA: authorityPlain(c.authority).label,
      a: authorityPlain(c.authority).label,
      keyB: obl,
      b: obl,
      title: obl,
      detail: cut(c.expr || "— the rule, exactly as recorded —", 120),
      extra: c.source,
      loc: sym ? { file, byteStart: sym.loc.start, byteEnd: sym.loc.end } : undefined,
    };
  });
}

/** Map bubbles — the certainty and kinds questions. */
function elementRows(viewspec: CieViewSpec): DrillRow[] {
  return viewspec.elements.map((e, i) => {
    const cert = certaintyPlain(e.displayMode).label;
    const kind = elementKindPlain(e.elementKind);
    return {
      id: `el-${i}-${e.id}`.slice(0, 90),
      keyA: cert,
      a: cert,
      keyB: kind,
      b: kind,
      title: e.label,
      detail:
        e.displayMode === "Fact"
          ? "Checked directly against the code — read, not guessed."
          : e.displayMode === "Inference"
            ? "CIE worked this out from patterns; usually right, worth a glance."
            : "CIE spotted something but can't confirm it yet.",
      extra: e.elementKind,
      loc: { file: e.evidence.file, byteStart: e.evidence.start, byteEnd: e.evidence.end },
    };
  });
}

/** Declared packages — the dependencies questions. The manifest is the
 *  "code" of this question: the viewer opens the manifest file itself. */
function depRows(deps: CieDeps): DrillRow[] {
  return deps.table.deps.map((d: DeclaredDep, i) => ({
    id: `dep-${i}-${d.name}`,
    keyA: ecoName(d.ecosystem),
    a: ecoName(d.ecosystem),
    keyB: d.source,
    b: baseOf(d.source),
    title: d.name,
    detail: `declared ${d.scope} — version “${d.version_req}” exactly as written`,
    extra: d.licence ?? undefined,
    loc: { file: d.source },
  }));
}

/** Security findings — the security and CWE questions. */
function securityRows(q: CieQuality): DrillRow[] {
  return q.security.issues.map((it, i) => ({
    id: it.id || `sec-${i}`,
    keyA: it.file,
    a: cut(baseOf(it.file), 24),
    keyB: it.name,
    b: cut(it.name, 30),
    title: it.name,
    detail: it.why || it.snippet || "",
    severity: it.severity,
    extra: it.cwe || it.rule,
    marks: [it.line],
    loc: { file: it.file, line: it.line },
    askCtx: { headline: it.name, message: it.why || `${it.cwe} — ${it.rule}`, remediation: it.snippet ? undefined : undefined },
  }));
}

/** Complexity functions — most-twisted and the cyclomatic metric. */
function complexityRows(q: CieQuality): DrillRow[] {
  const bucket = (cc: number) => bucketFor(cc, [2, 4], ["straight-line (1–2)", "branchy (3–4)", "tangled (5+)"]);
  return (q.complexity.functions ?? []).map((f, i) => ({
    id: `cc-${i}-${f.function}`,
    keyA: f.file,
    a: cut(baseOf(f.file), 24),
    keyB: bucket(f.cc),
    b: bucket(f.cc),
    title: `${f.function} — cc ${f.cc}`,
    detail: `Complexity ${f.cc} (project max ${Math.max(1, q.complexity.totals.max_cc)}) — decision points over ~${f.loc} lines, by ${q.complexity.tool}.`,
    value: f.cc,
    marks: [f.line],
    loc: { file: f.file, line: f.line },
  }));
}

/** Duplicated block pairs — both twins carried (loc2). */
function duplicationRows(q: CieQuality): DrillRow[] {
  const length = (n: number) => bucketFor(n, [4, 8], ["short blocks (1–3 lines)", "mid blocks (4–7 lines)", "long blocks (8+ lines)"]);
  return (q.duplication.pairs ?? []).map((p, i) => ({
    id: `dup-${i}`,
    keyA: p.a_file,
    a: cut(baseOf(p.a_file), 24),
    keyB: length(p.lines),
    b: length(p.lines),
    title: `${baseOf(p.a_file)}:${p.a_line} ↔ ${baseOf(p.b_file)}:${p.b_line}`,
    detail: `${p.lines} identical lines — starts with “${cut(p.preview || "…", 60)}”`,
    marks: [p.a_line],
    loc: { file: p.a_file, line: p.a_line },
    loc2: { file: p.b_file, line: p.b_line },
  }));
}

/** Hot-spot ranked rows ("Which parts are the hot spots?"). */
function hotspotRows(q: CieQuality): DrillRow[] {
  const bucket = (risk: number) => bucketFor(risk, [25, 50], ["calm (< 25)", "warm (25–49)", "hot (50+)"]);
  return (q.hotspots.rows ?? []).map((r, i) => ({
    id: `hot-${i}-${r.file}`,
    keyA: containerOf(r.file),
    a: containerOf(r.file),
    keyB: r.file,
    b: cut(baseOf(r.file), 24),
    title: baseOf(r.file),
    detail: `risk ${Math.round(r.risk)} of 100 — churn ${r.churn} lines, max complexity ${r.max_cc}, ${r.urgent_findings} urgent finding(s), coverage ${r.coverage_percent != null ? r.coverage_percent + "%" : "unmeasured"}.`,
    value: r.risk,
    loc: { file: r.file },
  }));
}

/** Churn per file (sample project, real git numstat). */
function churnRows(q: CieQuality): DrillRow[] {
  const size = (n: number) => bucketFor(n, [20, 100], ["small (< 20 lines)", "busy (20–99)", "very busy (100+)"]);
  return (q.churn.sample.per_file ?? []).map((r, i) => ({
    id: `churn-${i}-${r.file}`,
    keyA: containerOf(r.file),
    a: containerOf(r.file),
    keyB: r.file,
    b: cut(baseOf(r.file), 24),
    title: baseOf(r.file),
    detail: `${r.added} added + ${r.deleted} deleted across ${r.commits} commit(s); last touched ${r.last_change ?? "—"}.`,
    value: r.churn,
    loc: { file: r.file },
  }));
}

/** Fix commits — the real events behind the defect metrics. */
function commitRows(q: CieQuality): DrillRow[] {
  return (q.defects.fix_commits.recent ?? []).map((c, i) => ({
    id: `fix-${i}-${c.sha}`,
    keyA: c.date.slice(0, 7),
    a: c.date.slice(0, 7),
    keyB: c.title.includes(":") ? cut(c.title.split(":")[0], 26) : "(root)",
    b: c.title.includes(":") ? cut(c.title.split(":")[0], 26) : "(root)",
    title: cut(c.title, 90),
    detail: `landed ${c.date} — mined with git log over the hosting repository's real history`,
    extra: c.sha,
    loc: undefined,
  }));
}

/** Crash-probe sessions (run live during collection). */
function crashRows(q: CieQuality): DrillRow[] {
  const family = (reason: string) => cut((reason || "timeout").split(/[:(]/)[0], 30);
  return (q.crash_probe.crashes_detail ?? []).map((c, i) => ({
    id: `crash-${i}`,
    keyA: c.module,
    a: c.module,
    keyB: family(c.reason || "timeout"),
    b: family(c.reason || "timeout"),
    title: `${c.module}.${c.function}`,
    detail: `input ${c.session} — ${c.reason || "timeout"}`,
    severity: c.crashed ? "High" : "Low",
    loc: undefined,
  }));
}

/** Code drift vs its tests (real git dates). */
function codeDriftRows(q: CieQuality): DrillRow[] {
  const lag = (d: number | null) =>
    d == null ? "no test file" : d > 7 ? "far behind (7d+)" : d > 0 ? "a little behind" : "in step";
  return (q.drift.code ?? []).map((c, i) => ({
    id: `cdrift-${i}-${c.file}`,
    keyA: lag(c.test_lag_days),
    a: lag(c.test_lag_days),
    keyB: c.file,
    b: cut(baseOf(c.file), 24),
    title: baseOf(c.file),
    detail: `code ${c.last_change ?? "—"} · tests ${c.tests_last_change ?? "none"} · lag ${c.test_lag_days ?? "—"} day(s) — ${c.note}`,
    loc: { file: c.file },
  }));
}

/** Document drift — a doc can be old and still correct. */
function docDriftRows(q: CieQuality): DrillRow[] {
  const lag = (d: number) => (d > 30 ? "way behind (30d+)" : d > 0 ? "drifting" : "fresh");
  return (q.drift.document ?? []).map((d, i) => ({
    id: `ddrift-${i}-${d.doc}`,
    keyA: lag(d.drift_days),
    a: lag(d.drift_days),
    keyB: d.describes || "(nothing recorded)",
    b: d.describes ? cut(baseOf(d.describes), 26) : "(nothing recorded)",
    title: d.doc,
    detail: `describes ${d.describes} — the doc is ${d.doc_age_days}d old, ${d.drift_days > 0 ? `${d.drift_days}d behind the code` : "not behind the code"} — ${d.note}`,
    loc: { file: d.doc },
    loc2: d.describes ? { file: d.describes } : undefined,
  }));
}

/** Measured coverage files (runtime.json, the project's own test run). */
function runtimeCoverageRows(
  files: NonNullable<NonNullable<CieRuntime["datasets"]["coverage"]>>["files"]
): DrillRow[] {
  const bucket = (pct: number) =>
    pct >= 70 ? "well covered (70%+)" : pct > 0 ? "partly covered" : "never even imported";
  return files.map((f, i) => ({
    id: `rtcov-${i}-${f.file}`,
    keyA: bucket(f.percent),
    a: bucket(f.percent),
    keyB: f.file,
    b: cut(baseOf(f.file), 24),
    title: baseOf(f.file),
    detail:
      f.covered != null && f.statements != null
        ? `${f.covered} of ${f.statements} lines covered (${Math.round(f.percent)}%) — measured on a real test run.`
        : `the project's own test run never imported this file${f.note ? ` (note: ${f.note})` : ""}.`,
    value: f.percent,
    loc: { file: f.file },
  }));
}

/** CPU profile rows (measured, file + line recorded by the profiler). */
function cpuRows(top: NonNullable<NonNullable<CieRuntime["datasets"]["cpu"]>>["top_functions"]): DrillRow[] {
  const heat = (ms: number) => bucketFor(ms, [1, 10], ["light (< 1 ms)", "warm (1–10 ms)", "hot (10+ ms)"]);
  return top.map((f, i) => ({
    id: `cpu-${i}-${f.function}`,
    keyA: f.file || "(no file recorded)",
    a: baseOf(f.file || "—"),
    keyB: heat(f.cum_ms),
    b: heat(f.cum_ms),
    title: `${f.function} — ${Math.round(f.cum_ms)} ms cumulative`,
    detail: `self ${f.self_ms} ms across ${f.calls} call(s), cumulative ${f.cum_ms} ms — line ${f.line}, from a real profile run.`,
    value: f.cum_ms,
    marks: [f.line],
    loc: { file: f.file, line: f.line },
  }));
}

/**
 * The canned "Ask CIE" prompt a drilldown row sends: one structured
 * question (problem → impact → fix) plus EVERY recorded fact that grounds
 * the answer — the chart it came from, the engine's own message and
 * remediation, the severity, the exact file:line, how the data was
 * collected, and the verbatim code window when the file is in hand.
 * The AI answers the three parts; nothing in the prompt is invented.
 */
export function buildCannedAsk(
  chartQuestion: string,
  row: {
    title: string;
    detail?: string;
    severity?: string;
    extra?: string;
    askCtx?: { headline?: string; message?: string; remediation?: string };
  } | null,
  loc: DrillLoc | null,
  snippet?: string,
  source?: string
): string {
  const head =
    "Answer in three clearly-signed parts:\n" +
    "1. WHAT is the problem — in plain words, exactly as CIE recorded it;\n" +
    "2. IMPACT — what could realistically go wrong if it stays as it is;\n" +
    "3. FIX — the concrete changes to make, in order, smallest safe step first.\n" +
    "Ground every claim in the facts below (and the code, when present); say plainly when something is your reasoning rather than a recorded fact.";

  const facts: string[] = [];
  facts.push(`- The question this came from: “${chartQuestion}”`);
  if (row) {
    facts.push(`- Subject: ${row.title}`);
    if (row.askCtx?.headline && row.askCtx.headline !== row.title)
      facts.push(`- What CIE calls it: ${row.askCtx.headline}`);
    const msg = row.askCtx?.message ?? row.detail;
    if (msg) facts.push(`- CIE's recorded finding: ${msg}`);
    const sev = row.severity ? severityPlain(row.severity) : null;
    if (sev) facts.push(`- Urgency: ${row.severity} (${sev.label} — ${sev.hint})`);
    if (row.extra) facts.push(`- ${/^[A-Z]{3}\b|CWE|sha|[0-9a-f]{7,}/.test(row.extra) ? `Reference: ${row.extra}` : `Recorded note: ${row.extra}`}`);
    if (row.askCtx?.remediation) facts.push(`- The engine's recorded remediation note: ${row.askCtx.remediation}`);
  }
  if (loc) {
    facts.push(`- Where: ${loc.file}${loc.line != null ? `, around line ${loc.line}` : ""}`);
  }
  if (source) facts.push(`- Data provenance: ${source}`);
  if (snippet) facts.push(`- The code at that spot, verbatim, line-numbered:\n${snippet}`);
  if (loc?.file == null && row) facts.push("- This one is not anchored to a single source line (it is a probe/measured record) — base the three parts on the recorded facts.");

  return `${head}\n\nThe recorded facts:\n${facts.join("\n")}`;
}

// --------------------------------------------------------- the planner ---

export interface DrillStore {
  report: CieReport | null;
  viewspec: CieViewSpec | null;
  concepts: ConceptTree | null;
  deps: CieDeps | null;
  runtime: CieRuntime | null;
  quality: CieQuality | null;
}

/** tiny builder keeps the per-question blocks below readable */
function rows(p: {
  rows: DrillRow[];
  axisA: { title: string; tell: string };
  axisB: { title: string; tell: string };
  kindA: DrillAxis;
  kindB: DrillAxis;
  aName: string;
  bName: string;
  rowName: string;
  listTitle: string;
  blurb: string;
  viewer: DrillMeta["viewer"];
  valueUnit?: string;
  source?: string;
}): Drill {
  return {
    kind: "rows",
    rows: p.rows,
    meta: {
      axisA: p.axisA, axisB: p.axisB, kindA: p.kindA, kindB: p.kindB,
      aName: p.aName, bName: p.bName, rowName: p.rowName, listTitle: p.listTitle,
      blurb: p.blurb, viewer: p.viewer, valueUnit: p.valueUnit, source: p.source,
    },
  };
}

const PROBLEM_VIEWER: DrillMeta["viewer"] = {
  kind: "code",
  empty: "Nothing selected yet. Click a bar above — this viewer shows that file's code straight away.",
  caption: "Recorded evidence lines are marked; the focused problem's line is ringed.",
};

/** The per-question plan. Two questions never share content: axis labels,
 *  chart kinds, row lists and the viewer's own text all follow the question. */
export function drillFor(id: string, s: DrillStore): Drill {
  const { report, viewspec, concepts, deps, runtime, quality } = s;
  if (!report) return { kind: "chart", note: "Load a project first." };

  // ---------- Problems ----------
  if (id === "files-most-problems")
    return rows({
      rows: findingRows(report), // axis A = file, axis B = kind (defaults)
      axisA: {
        title: "Problems per file",
        tell: "Each bar counts what CIE flagged in that file. Click a bar to select it — click it again to release.",
      },
      axisB: {
        title: "Errors inside the selection",
        tell: "The count, grouped by kind of error. Click a slice to add it to the filter.",
      },
      kindA: "bar",
      kindB: "pie",
      aName: "File",
      bName: "Kind of error",
      rowName: "problem",
      listTitle: "Matching problems",
      blurb: "Charts, list and code, all linked: pick a file — or a file + an error kind — and everything follows.",
      viewer: PROBLEM_VIEWER,
      source: "report.json findings × symbols[].loc",
    });

  if (id === "urgency-donut")
    return rows({
      rows: bySeverityAxis(findingRows(report)), // axis A = urgency, B = file
      axisA: {
        title: "Urgency, counted",
        tell: "Rose means soon; green can wait. Click a slice to keep only that urgency.",
      },
      axisB: { title: "Files inside the selection", tell: "Where that urgency lives — click a bar to focus a file and open it." },
      kindA: "pie",
      kindB: "bar",
      aName: "Urgency",
      bName: "File",
      rowName: "problem",
      listTitle: "Matching problems",
      blurb: "The same findings, two angles: pick an urgency, then a file — the list and the code follow both.",
      viewer: {
        ...PROBLEM_VIEWER,
        empty: "Nothing selected yet. Pick an urgency above, then a file — the code appears here.",
      },
      source: "report.json findings",
    });

  if (id === "problem-kinds")
    return rows({
      rows: byKindAxis(findingRows(report)),
      axisA: { title: "Kinds of trouble", tell: "The same kind of thing, counted. Click a kind to keep only those." },
      axisB: { title: "Files holding the selection", tell: "Which files carry this kind of trouble — click a bar to focus one." },
      kindA: "bar",
      kindB: "bar",
      aName: "Kind of problem",
      bName: "File",
      rowName: "problem",
      listTitle: "Matching problems",
      blurb: "Counts, then places: choose a kind of trouble, see which files hold it, then read the exact lines.",
      viewer: {
        ...PROBLEM_VIEWER,
        empty: "Nothing selected yet. Click a kind, then a file — the code appears here.",
      },
      source: "report.json findings",
    });

  if (id === "fix-right-away")
    return rows({
      rows: bySeverityAxis(
        findingRows(report).filter((r) => r.severity === "Critical" || r.severity === "High")
      ),
      axisA: { title: "How urgent, among the urgent", tell: "Only Critical and High rows exist here — click a slice to keep one." },
      axisB: { title: "Where the urgent work lives", tell: "Files holding it — click a bar to open that file." },
      kindA: "pie",
      kindB: "bar",
      aName: "Urgency",
      bName: "File",
      rowName: "urgent problem",
      listTitle: "The urgent list",
      blurb: "The honest headline, made navigable: urgency, then file, then the exact line.",
      viewer: {
        ...PROBLEM_VIEWER,
        empty: "Nothing selected yet. Pick a file above (or a problem in the list) — the code appears here.",
      },
      source: "report.json findings — only Critical + High",
    });

  if (id === "clean-files")
    return rows({
      rows: fileRows(report), // axis A = clean/troubled; axis B unused
      axisA: {
        title: "Clean vs troubled files",
        tell: "Files CIE flagged, against files it found nothing in. Click a slice to keep one side.",
      },
      axisB: { title: "none", tell: "" },
      kindA: "pie",
      kindB: "none",
      aName: "Status",
      bName: "",
      rowName: "file",
      listTitle: "Matching files",
      blurb: "Every file has its moment here — even the spotless ones. Pick from the pie or the list; the code opens below.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Pick a file in the list — its code appears here, with flagged lines marked when there are any.",
        caption: "The file as recorded — flagged lines marked.",
      },
      source: "report.json files[] × findings",
    });

  // ---------- Promises ----------
  if (id === "who-wrote-promises")
    return rows({
      rows: contractRows(report), // axis A = authority, axis B = obligation
      axisA: {
        title: "Promises by author",
        tell: "Green ones are written down by the developers; amber ones are CIE's guesses. Click a slice to keep one.",
      },
      axisB: { title: "What those promises say", tell: "The obligations inside the selection — click a bar to focus one." },
      kindA: "pie",
      kindB: "bar",
      aName: "Who wrote it",
      bName: "The obligation",
      rowName: "promise",
      listTitle: "Matching promises",
      blurb: "A promise is something the code must always do — pick one and see the code bound to it.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a promise (or a bar) — the code that must keep it appears here.",
        caption: "The first symbol bound to this promise — the promise must hold here.",
      },
      source: "report.json contracts × symbols[].loc",
    });

  if (id === "promise-list")
    return rows({
      rows: contractRows(report).map((r) => ({
        ...r, // axis A = obligation type, axis B = authorship (the other order)
        keyA: r.keyB,
        a: r.b,
        keyB: r.keyA,
        b: r.a,
      })),
      axisA: { title: "Promise types, counted", tell: "What the promises say, grouped by obligation. Click a bar to keep one type." },
      axisB: { title: "Who wrote them", tell: "Inside the selection — the provenance of each promise. Click a slice to keep one." },
      kindA: "bar",
      kindB: "pie",
      aName: "The obligation",
      bName: "Who wrote it",
      rowName: "promise",
      listTitle: "Matching promises",
      blurb: "The full promise list, reshapable: type, then author, then the bound code below.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a promise (or a bar) — the code bound to it appears here.",
        caption: "The first symbol bound to this promise — the promise must hold here.",
      },
      source: "report.json contracts",
    });

  // ---------- The map ----------
  if (id === "map-certainty" && viewspec)
    return rows({
      rows: elementRows(viewspec), // axis A = certainty, axis B = kind
      axisA: { title: "Certainty on the map", tell: "How each bubble got onto the map. Click a slice to keep one certainty." },
      axisB: { title: "What those bubbles are", tell: "Kinds of things carrying the selected certainty — click a bar to focus one." },
      kindA: "pie",
      kindB: "bar",
      aName: "How sure",
      bName: "Kind of bubble",
      rowName: "bubble",
      listTitle: "Matching bubbles",
      blurb: "The map's honesty, one click deep: certainty, then a bubble — the exact code it stands for appears below.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a certainty, then a bubble — the exact code appears here.",
        caption: "The code this bubble stands for (the engine's recorded evidence span).",
      },
      source: "viewspec.json elements[].evidence",
    });

  if (id === "map-kinds" && viewspec)
    return rows({
      rows: elementRows(viewspec).map((r) => ({
        ...r, // axis A = kind, axis B = certainty (the other order)
        keyA: r.keyB,
        a: r.b,
        keyB: r.keyA,
        b: r.a,
      })),
      axisA: { title: "What the map is made of", tell: "Each bar is a kind of bubble on the Map page. Click a bar to keep one kind." },
      axisB: { title: "How sure CIE is about those", tell: "The certainty mix inside the selection — click a slice to keep one." },
      kindA: "bar",
      kindB: "pie",
      aName: "Kind of bubble",
      bName: "How sure",
      rowName: "bubble",
      listTitle: "Matching bubbles",
      blurb: "Files, behaviours, rules — pick a kind, then a certainty, and the exact code appears below.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a kind, then a bubble — the exact code appears here.",
        caption: "The code this bubble stands for (the engine's recorded evidence span).",
      },
      source: "viewspec.json elements[].evidence",
    });

  // ---------- What the code does ----------
  if (id === "things-per-file")
    return rows({
      rows: symbolRows(report), // axis A = file, axis B = symbol kind
      axisA: {
        title: "Named things per file",
        tell: "Functions, classes and modules, counted per file. Click a bar to focus that file.",
      },
      axisB: { title: "What those named things are", tell: "Inside the selection, by kind — click a bar to focus one." },
      kindA: "bar",
      kindB: "bar",
      aName: "File",
      bName: "Kind of named thing",
      rowName: "named thing",
      listTitle: "Matching named things",
      blurb: "Bigger files do more — then each named thing, with the exact span of code it is.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a file, then a named thing — its code appears here.",
        caption: "The exact span this named thing covers in the file.",
      },
      source: "report.json symbols[].loc",
    });

  if (id === "outside-world")
    return rows({
      rows: effectRows(report), // axis A = file, axis B = effect
      axisA: {
        title: "Touches to the outside world, per file",
        tell: "How many named things do each of these, counted per file. Click a bar to focus.",
      },
      axisB: { title: "What those touches are", tell: "Inside the selection, by effect — click a slice to keep one." },
      kindA: "bar",
      kindB: "pie",
      aName: "File",
      bName: "Effect on the world",
      rowName: "outside touch",
      listTitle: "Matching effects",
      blurb: "Databases, printers, secrets — pick a file, then an effect, then see the symbol that does it.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a file, then an effect — the symbol's code appears here.",
        caption: "The symbol whose summary records this outside-world touch.",
      },
      source: "report.json summaries[].effects",
    });

  if (id === "file-does-what" && concepts)
    return rows({
      rows: moduleRows(concepts, report), // axis A = file, axis B = behaviour
      axisA: { title: "Behaviours per file", tell: "What CIE recognised in each file, counted. Click a bar to focus that file." },
      axisB: { title: "Those behaviours by kind", tell: "Inside the selection — click a slice to keep one kind." },
      kindA: "bar",
      kindB: "pie",
      aName: "File",
      bName: "Behaviour",
      rowName: "behaviour",
      listTitle: "Matching behaviours",
      blurb: "Plain answers per file — pick a file and a behaviour kind, then read the code.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a file above — its code appears here.",
        caption: "The file whose behaviours CIE recognised.",
      },
      source: "concepts.json Module nodes × behaviours",
    });

  // ---------- Dependencies ----------
  if (id === "deps-ecosystems" && deps)
    return rows({
      rows: depRows(deps), // axis A = ecosystem, axis B = manifest
      axisA: { title: "Declared packages per ecosystem", tell: "Counted straight from the manifests, as written. Click a slice to keep one." },
      axisB: { title: "The manifests behind the selection", tell: "Which files declare these — click one to open it in the viewer." },
      kindA: "pie",
      kindB: "bar",
      aName: "Ecosystem",
      bName: "Manifest file",
      rowName: "package",
      listTitle: "Matching packages",
      blurb: "Pick an ecosystem, then a manifest — the manifest itself opens in the viewer (it is code too).",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a manifest bar (or a package row) — the manifest opens here.",
        caption: "The manifest file, exactly as the project wrote it.",
      },
      source: "deps.json table.deps — version_req never interpreted",
    });

  if (id === "deps-manifests" && deps)
    return rows({
      rows: depRows(deps).map((r) => ({
        ...r, // axis A = manifest, axis B = ecosystem (the other order)
        keyA: r.keyB,
        a: r.b,
        keyB: r.keyA,
        b: r.a,
      })),
      axisA: { title: "Packages per manifest", tell: "Each manifest and how many packages it declares. Click a bar to open it." },
      axisB: { title: "Ecosystems inside the selection", tell: "The mix of package kinds in that manifest — click a slice to keep one." },
      kindA: "bar",
      kindB: "pie",
      aName: "Manifest file",
      bName: "Ecosystem",
      rowName: "package",
      listTitle: "Matching packages",
      blurb: "One manifest at a time: click it, read it, and see its packages sorted by ecosystem.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a manifest above — it opens here in the viewer.",
        caption: "The manifest file, exactly as the project wrote it.",
      },
      source: "deps.json table.deps",
    });

  // ---------- Watching it run ----------
  if (id === "runtime-coverage" && runtime?.datasets.coverage)
    return rows({
      rows: runtimeCoverageRows(runtime.datasets.coverage.files), // A = bucket, B = file
      axisA: { title: "How well the tests cover", tell: "Bucketed measured coverage. Click a slice to keep one bucket." },
      axisB: { title: "Files inside the selection", tell: "Bars are each file's measured percent — click one to open it." },
      kindA: "pie",
      kindB: "bar",
      aName: "Coverage bucket",
      bName: "File",
      rowName: "measured file",
      listTitle: "Matching files",
      blurb: `Measured by ${runtime.datasets.coverage.tool} on a real test run — pick a bucket, then a file.`,
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Pick a bucket, then a file — its code appears here.",
        caption: "The file the measurement named (the measurement is per file, so no line is marked).",
      },
      valueUnit: "%",
      source: `runtime.json coverage — ${runtime.datasets.coverage.tool}`,
    });

  if (id === "runtime-cpu" && runtime?.datasets.cpu)
    return rows({
      rows: cpuRows(runtime.datasets.cpu.top_functions), // A = file, B = heat bucket
      axisA: {
        title: "CPU hotspots, by file",
        tell: "Cumulative milliseconds per file, from a real profile run. Click a bar to focus a file.",
      },
      axisB: { title: "How hot those functions are", tell: "Inside the selection, by time bucket — click to keep one." },
      kindA: "bar",
      kindB: "pie",
      aName: "File",
      bName: "Time bucket",
      rowName: "hot function",
      listTitle: "Matching hot functions",
      blurb: `Measured by ${runtime.datasets.cpu.tool} — pick a file, then a hot function, then the exact line.`,
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a file above, then a function — the exact line appears here.",
        caption: "The line the profiler recorded.",
      },
      valueUnit: "ms",
      source: `runtime.json cpu — ${runtime.datasets.cpu.tool}`,
    });

  // ---------- Health (the ETL export: quality.json) ----------
  if ((id === "security-issues" || id === "vulnerabilities") && quality)
    return rows({
      rows: securityRows(quality), // axis A = file, axis B = weakness
      axisA: {
        title: id === "security-issues" ? "Security issues by file" : "Weaknesses, by file",
        tell: "Each bar counts the real findings on that file. Click a bar to focus it.",
      },
      axisB: { title: "What the weaknesses are", tell: "The count, grouped by weakness. Click a slice to add it to the filter." },
      kindA: "bar",
      kindB: "pie",
      aName: "File",
      bName: "Weakness",
      rowName: "security finding",
      listTitle: "Matching findings",
      blurb: "Real scan findings: a file, then a weakness, then the exact line — snippet and CWE on the row.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a bar above — the file's code marks every flagged line.",
        caption: "The flagged line(s), exactly as the scan recorded them.",
      },
      source: `quality.json security — ${quality.security.tool}`,
    });

  if (id === "duplications" && quality)
    return rows({
      rows: duplicationRows(quality), // axis A = file, axis B = block length
      axisA: { title: "Duplicated blocks per file", tell: "Where the repeated blocks start. Click a bar to focus that file." },
      axisB: { title: "How long the repeats are", tell: "Inside the selection, by block length — click to keep one." },
      kindA: "bar",
      kindB: "pie",
      aName: "File",
      bName: "Block length",
      rowName: "repeated pair",
      listTitle: "Matching pairs",
      blurb: "Two places, one block: pick a pair — both twins open in the viewer, side by side.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a pair above — both sides of the repeat appear here.",
        caption: "Twin A first; “show the twin” flips to where the block repeats.",
      },
      source: `quality.json duplication — ${quality.duplication.tool}`,
    });

  if ((id === "hotspots" || id === "metric-cyclomatic" || id === "most-complex") && quality)
    return rows({
      rows: id === "hotspots" ? hotspotRows(quality) : complexityRows(quality),
      axisA: {
        title: id === "hotspots" ? "Hot spots, by top-level folder" : "Most complex functions, by file",
        tell: id === "hotspots"
          ? "Risk summed from the ranked files. Click a bar to keep one folder."
          : "Decision points summed from the AST walk. Click a bar to keep one file.",
      },
      axisB: {
        title: id === "hotspots" ? "The files inside the selection" : "How tangled those functions are",
        tell: id === "hotspots"
          ? "Bars are each file's risk — click one to open it."
          : "Inside the selection, by bucket — click to keep one.",
      },
      kindA: "bar",
      kindB: id === "hotspots" ? "bar" : "pie",
      aName: id === "hotspots" ? "Folder" : "File",
      bName: id === "hotspots" ? "File" : "Complexity bucket",
      rowName: id === "hotspots" ? "hot file" : "function",
      listTitle: id === "hotspots" ? "Matching hot files" : "Matching functions",
      blurb: id === "hotspots" ? `Formula: ${quality.hotspots.formula}` : `Formula: ${quality.complexity.formula}`,
      viewer: {
        kind: "code",
        empty: id === "hotspots"
          ? "Nothing selected yet. Click a folder, then a file — its code appears here."
          : "Nothing selected yet. Click a file above, then a function — the exact line appears here.",
        caption: id === "hotspots" ? "The file the risk was computed from." : "The function line the walk recorded.",
      },
      valueUnit: id === "hotspots" ? "risk" : "cc",
      source: id === "hotspots" ? "quality.json hotspots" : `quality.json complexity — ${quality.complexity.tool}`,
    });

  if (id === "code-drift" && quality)
    return rows({
      rows: codeDriftRows(quality), // axis A = lag bucket, axis B = file
      axisA: { title: "How far code is from its tests", tell: "Real git dates, bucketed. Click a slice to keep one lag bucket." },
      axisB: { title: "The files inside the selection", tell: "Click a bar to open the file." },
      kindA: "pie",
      kindB: "bar",
      aName: "Drift bucket",
      bName: "File",
      rowName: "drifting file",
      listTitle: "Matching files",
      blurb: "Code vs its tests, by real git dates — pick a bucket, then a file, then read it.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a slice, then a file — its code appears here.",
        caption: "The file whose history was measured.",
      },
      source: "quality.json drift.code — real git dates",
    });

  if (id === "document-drift" && quality)
    return rows({
      rows: docDriftRows(quality), // axis A = lag bucket, axis B = describes
      axisA: { title: "Docs vs the code", tell: "How far behind the code each doc is. Click a slice to keep one." },
      axisB: { title: "The code those docs describe", tell: "Inside the selection — click a bar to focus one doc/code pair." },
      kindA: "pie",
      kindB: "bar",
      aName: "Drift bucket",
      bName: "Describes",
      rowName: "drifting doc",
      listTitle: "Matching docs",
      blurb: "A doc can be old and still correct — pick one and see it, plus the code it describes.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a doc above — the doc appears here; “show the twin” flips to the code it describes.",
        caption: "The doc itself — the paired side is the code it describes, when known.",
      },
      source: "quality.json drift.document — real git dates",
    });

  if (id === "metric-churn" && quality)
    return rows({
      rows: churnRows(quality), // axis A = folder, axis B = file
      axisA: {
        title: "Churn, by top-level folder",
        tell: "Lines added+deleted (real git numstat), summed per folder. Click a bar to keep one folder.",
      },
      axisB: { title: "The files inside the selection", tell: "Bars are each file's churn — click one to open it." },
      kindA: "bar",
      kindB: "bar",
      aName: "Folder",
      bName: "File",
      rowName: "churning file",
      listTitle: "Matching files",
      blurb: "Real git numstat — pick a folder, then a file, then read it in the viewer.",
      viewer: {
        kind: "code",
        empty: "Nothing selected yet. Click a bar (or a slice), then a file — its code appears here.",
        caption: "The file whose history was measured.",
      },
      valueUnit: "lines",
      source: `quality.json churn.sample — ${quality.churn.tool}`,
    });

  // ---------- Metrics behind fix commits ----------
  if ((id === "metric-defect-density" || id === "metric-arrival" || id === "metric-mtbf") && quality)
    return rows({
      rows: commitRows(quality), // axis A = month, axis B = area prefix
      axisA: { title: "Fix commits, by month", tell: "Real commits mined with git log. Click a slice to keep one month." },
      axisB: { title: "Areas inside the selection", tell: "The commit-message area prefix (derived) — click a bar to keep one." },
      kindA: "pie",
      kindB: "bar",
      aName: "Month",
      bName: "Area (message prefix)",
      rowName: "fix commit",
      listTitle: "Matching fix commits",
      blurb: "The events behind the metric: pick a month, then an area — the commit card shows each fix.",
      viewer: {
        kind: "commit",
        empty: "Nothing selected yet. Click a commit (or a bar) — its card appears here: sha, title, date.",
        caption: "The real commit, as mined from the hosting repository's history.",
      },
      source: "quality.json defects.fix_commits — git log over the hosting repo",
    });

  if (id === "metric-crash" && quality)
    return rows({
      rows: crashRows(quality), // axis A = module, axis B = failure family
      axisA: { title: "Crashes per entry module", tell: "Seeded sessions that crashed, by entry module. Click a bar to keep one." },
      axisB: { title: "How those sessions failed", tell: "The failure family — the exception, as thrown. Click a slice to keep one." },
      kindA: "bar",
      kindB: "pie",
      aName: "Entry module",
      bName: "Failure family",
      rowName: "crashed session",
      listTitle: "Matching crashed sessions",
      blurb: "Run for real during collection — a probe result, not a source line, so the viewer shows the probe record.",
      viewer: {
        kind: "none",
        empty: "These numbers came from a live probe run, not from one line of code — the matching sessions are listed here in full.",
        caption: "Probe record — module.function, the input, and what it threw.",
      },
      source: `quality.json crash_probe — ${quality.crash_probe.tool ?? "seeded fuzz run"}`,
    });

  // ---------- Everything else: the picture is already interactive ----------
  return {
    kind: "chart",
    note: "This question draws on an interactive canvas — click any element in it to see the code it stands for, or export the picture.",
  };
}