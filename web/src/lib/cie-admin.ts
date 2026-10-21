// CIE Admin — every data structure CIE holds for the loaded project,
// flattened into one generic shape (a title, a category, and rows) so one
// table component and one G6 overview can cover all of them without a
// bespoke view per artifact. Pure and headless: no React, no engine import —
// easy to keep in sync as new exports are added upstream.

import type {
  CieReport,
  ConceptTree,
  CieQuality,
  CieDeps,
  CieRuntime,
  CieStories,
  CieCochange,
  CieOverlap,
  CieDrift,
} from "./cie-types";
import type { TechStack } from "./stack-types";

export type AdminCategory =
  | "code" // report.json — symbols, summaries, findings, contracts
  | "stack" // stack.json — apps, routes, mains, docker, jvm facts
  | "concepts" // concepts.json — the concept DAG
  | "quality" // quality.json — the eight classic measures
  | "deps" // deps.json
  | "runtime" // runtime.json — measured evidence
  | "stories" // stories.json — the 33 registry demos
  | "trust"; // cochange / overlap / drift

export interface AdminSection {
  id: string;
  title: string;
  category: AdminCategory;
  source: string; // which export this came from, shown as a footer note
  rows: Record<string, unknown>[];
}

export interface AdminStore {
  report: CieReport | null;
  stack: TechStack | null;
  concepts: ConceptTree | null;
  quality: CieQuality | null;
  deps: CieDeps | null;
  runtime: CieRuntime | null;
  stories: CieStories | null;
  cochange: CieCochange | null;
  overlap: CieOverlap | null;
  drift: CieDrift | null;
}

const wrap = (
  arr: unknown[] | undefined,
  key: string,
): Record<string, unknown>[] =>
  (arr ?? []).map((v, i) => ({ "#": i, [key]: v }));

const asRows = <T,>(arr: T[] | undefined): Record<string, unknown>[] =>
  ((arr ?? []) as unknown as Record<string, unknown>[]);

const single = (obj: unknown): Record<string, unknown>[] =>
  obj ? [obj as Record<string, unknown>] : [];

/** The total list — one entry per array-shaped (or single-record) field CIE
 *  exports, across every artifact the store can hold. A section is omitted
 *  only when its PARENT artifact wasn't loaded at all (null); an artifact
 *  that loaded with an empty array still gets its row ("0 rows"), honestly —
 *  that's still a real answer about what the engine recorded. */
export function buildAdminSections(s: AdminStore): AdminSection[] {
  const out: AdminSection[] = [];
  const add = (
    id: string,
    title: string,
    category: AdminCategory,
    source: string,
    rows: Record<string, unknown>[],
  ) => out.push({ id, title, category, source, rows });

  // ---- report.json ----
  const r = s.report;
  if (r) {
    add("report-files", "Files", "code", "report.json", wrap(r.files, "file"));
    add("report-symbols", "Symbols", "code", "report.json", asRows(r.symbols));
    add("report-summaries", "Per-symbol summaries", "code", "report.json", asRows(r.summaries));
    add("report-findings", "Findings", "code", "report.json", asRows(r.findings));
    add("report-contracts", "Contracts", "code", "report.json", asRows(r.contracts));
    add("report-warnings", "Warnings", "code", "report.json", wrap(r.warnings, "warning"));
    add("report-coverage", "Analysis coverage", "code", "report.json", single(r.coverage));
    add("report-evidence", "Evidence envelope", "code", "report.json", single(r.evidence));
    if (r.state_model) {
      add("report-transitions", "State-model transitions", "code", "report.json", asRows(r.state_model.transitions));
    }
    if (r.fault_analysis) {
      add("report-fault-edges", "Fault-analysis lineage edges", "code", "report.json", asRows(r.fault_analysis.lineage?.edges));
      add("report-fault-experiments", "Fault-analysis experiments", "code", "report.json", asRows(r.fault_analysis.experiments));
    }
  }

  // ---- stack.json ----
  const st = s.stack;
  if (st) {
    add("stack-build", "Build info", "stack", "stack.json", single(st.build));
    add("stack-apps", "Applications", "stack", "stack.json", asRows(st.apps));
    add("stack-mains", "Entry points (mains)", "stack", "stack.json", asRows(st.mains));
    add("stack-routes", "Routes", "stack", "stack.json", asRows(st.routes));
    add("stack-cross-service", "Cross-service calls", "stack", "stack.json", asRows(st.crossService));
    add("stack-languages", "Languages", "stack", "stack.json", asRows(st.languages));
    add("stack-frameworks", "Frameworks", "stack", "stack.json", asRows(st.frameworks));
    add("stack-config", "App config entries", "stack", "stack.json", asRows(st.config));
    add("stack-external-docker", "External systems (compose, image-only)", "stack", "stack.json", asRows(st.externalDocker));
    add("stack-dockerfiles", "Dockerfiles", "stack", "stack.json", asRows(st.docker?.dockerfiles));
    add("stack-compose-services", "Compose services", "stack", "stack.json", asRows(st.docker?.compose?.services));
    add("stack-notes", "Notes", "stack", "stack.json", wrap(st.notes, "note"));
    add(
      "stack-flows",
      "Controller flows (per handler file)",
      "stack",
      "stack.json",
      (st.flows ?? []).map((f) => ({ name: f.name, rel: f.rel, basePath: f.basePath, handlers: f.flows.length })),
    );
    if (st.jvm) {
      add("stack-jvm-config", "JVM config classes", "stack", "stack.json", asRows(st.jvm.configClasses));
      add("stack-jvm-services", "JVM services", "stack", "stack.json", asRows(st.jvm.services));
      add("stack-jvm-repositories", "JVM repositories", "stack", "stack.json", asRows(st.jvm.repositories));
      add("stack-jvm-entities", "JVM entities", "stack", "stack.json", asRows(st.jvm.entities));
      add("stack-jvm-scheduled", "JVM scheduled jobs", "stack", "stack.json", asRows(st.jvm.scheduledIn));
      add("stack-jvm-listeners", "JVM listeners", "stack", "stack.json", asRows(st.jvm.listeners));
      add("stack-jvm-outbound", "JVM outbound calls", "stack", "stack.json", asRows(st.jvm.outbound));
    }
  }

  // ---- concepts.json ----
  const c = s.concepts;
  if (c) {
    add("concepts-nodes", "Concept nodes", "concepts", "concepts.json", asRows(c.nodes));
    add("concepts-unattached-behaviors", "Unattached behaviors", "concepts", "concepts.json", asRows(c.unattached_behaviors));
    add("concepts-unattached-rules", "Unattached rules", "concepts", "concepts.json", asRows(c.unattached_rules));
  }

  // ---- quality.json ----
  const q = s.quality;
  if (q) {
    add("quality-complexity-fns", "Complexity — functions", "quality", "quality.json", asRows(q.complexity?.functions));
    add("quality-complexity-files", "Complexity — files", "quality", "quality.json", asRows(q.complexity?.files));
    add("quality-dup-pairs", "Duplication pairs", "quality", "quality.json", asRows(q.duplication?.pairs));
    add("quality-security-rules", "Security — rules", "quality", "quality.json", asRows(q.security?.rules));
    add("quality-security-issues", "Security — issues", "quality", "quality.json", asRows(q.security?.issues));
    add("quality-coverage-files", "Coverage heatmap — files", "quality", "quality.json", asRows(q.coverage_heatmap?.files));
    add("quality-churn-files", "Churn — per file", "quality", "quality.json", asRows(q.churn?.sample?.per_file));
    add("quality-churn-daily", "Churn — hosting repo, daily", "quality", "quality.json", asRows(q.churn?.hosting_repo?.daily));
    add("quality-hotspots", "Hotspots", "quality", "quality.json", asRows(q.hotspots?.rows));
    add("quality-drift-docs", "Drift — docs", "quality", "quality.json", asRows(q.drift?.document));
    add("quality-drift-code", "Drift — code", "quality", "quality.json", asRows(q.drift?.code));
    add("quality-defect-bugs", "Defects — tracked bugs", "quality", "quality.json", asRows(q.defects?.tracker?.bugs));
    add("quality-fix-commits", "Defects — recent fix commits", "quality", "quality.json", asRows(q.defects?.fix_commits?.recent));
    add("quality-crash-probe", "Crash probe — sessions", "quality", "quality.json", asRows(q.crash_probe?.crashes_detail));
    add("quality-repair-probe", "Repair probe — iterations", "quality", "quality.json", asRows(q.repair_probe?.iterations));
    add("quality-metrics", "The eight classic metrics", "quality", "quality.json", asRows(q.metrics));
  }

  // ---- deps.json ----
  const d = s.deps;
  if (d) {
    add("deps-declared", "Declared dependencies", "deps", "deps.json", asRows(d.table?.deps));
    add("deps-unparsed", "Manifests not parsed", "deps", "deps.json", wrap(d.table?.unparsed, "manifest"));
    add("deps-usage", "Dependency usage", "deps", "deps.json", asRows(d.usage));
  }

  // ---- runtime.json ----
  const rt = s.runtime;
  if (rt) {
    add("runtime-cpu", "CPU — top functions", "runtime", "runtime.json", asRows(rt.datasets.cpu?.top_functions));
    add("runtime-spans", "Concurrency spans", "runtime", "runtime.json", asRows(rt.datasets.concurrency?.spans));
    add("runtime-memory-sites", "Memory — top allocation sites", "runtime", "runtime.json", asRows(rt.datasets.memory?.top_sites));
    add("runtime-deadlock-threads", "Deadlock — threads", "runtime", "runtime.json", asRows(rt.datasets.deadlock?.threads));
    add("runtime-race-natural", "Race — natural trials", "runtime", "runtime.json", asRows(rt.datasets.race?.natural_trials));
    add("runtime-race-widened", "Race — widened trials", "runtime", "runtime.json", asRows(rt.datasets.race?.widened_trials));
    add("runtime-coverage-files", "Runtime coverage — files", "runtime", "runtime.json", asRows(rt.datasets.coverage?.files));
  }

  // ---- stories.json ----
  const sto = s.stories;
  if (sto) {
    add(
      "stories",
      "Story demos (registry)",
      "stories",
      "stories.json",
      Object.values(sto.stories ?? {}).map((st2) => ({
        id: st2.id,
        title: st2.title,
        kind: st2.kind,
        shape: st2.demo?.shape,
        source: st2.source?.label,
        gap: st2.gap,
      })),
    );
  }

  // ---- cochange.json / overlap.json / drift.json ----
  if (s.cochange) add("cochange-edges", "Co-change edges", "trust", "cochange.json", asRows(s.cochange.edges));
  if (s.overlap) {
    add("overlap-pairs", "Capability overlap pairs", "trust", "overlap.json", asRows(s.overlap.pairs));
    add("overlap-unmapped", "Concepts with no symbols to compare", "trust", "overlap.json", wrap(s.overlap.unmapped, "concept"));
  }
  if (s.drift) {
    add("drift-claims", "Drift — extracted claims", "trust", "drift.json", asRows(s.drift.claims));
    add("drift-mappings", "Drift — claim→symbol mappings", "trust", "drift.json", asRows(s.drift.mappings));
    add("drift-verifications", "Drift — verifications", "trust", "drift.json", asRows(s.drift.verifications));
    add("drift-reverse", "Drift — reverse (symbol→doc gaps)", "trust", "drift.json", asRows(s.drift.reverse_drift));
  }

  return out;
}

/** Substring match over a row's own stringified form — the same "search
 *  everything, no schema needed" contract every section's columns vary by. */
export function rowMatches(row: Record<string, unknown>, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  try {
    return JSON.stringify(row).toLowerCase().includes(q);
  } catch {
    return false;
  }
}
