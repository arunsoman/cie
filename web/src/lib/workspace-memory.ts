// Launch with the LAST OPENED WORKSPACE, not the default sample, every time.
//
// The dashboard's analyze jobs keep their results in server memory only
// (AnalyzeJob.result — the scratch folder is deleted when the job ends).
// Two cooperating halves make "reopen where I was" work:
//
// 1. THE BROWSER remembers which workspace was opened last, in
//    localStorage. A sample workspace records just its kind; a
//    connect-a-repo workspace records the analyze job's id.
// 2. THE SERVER persists the finished result (analyze-jobs.ts writes
//    result.json under its jobs root), so the result route can rehydrate
//    that job by id even after the dev server restarted and its in-memory
//    map is empty.
//
// On launch: restore the remembered workspace if both halves still hold it
// (browser record + fetchable result / sample data); otherwise fall back to
// the sample with an honest note. Uploaded files are never recorded — they
// only exist inside the tab that uploaded them, and can't be re-fetched.

import type { CieReport, CieViewSpec, ConceptTree, CieDeps, CieRuntime, CieCochange, CieOverlap, CieDrift } from "./cie-types";
import { isValidReport, isValidViewspec, isValidConcepts } from "./cie-types";
import type { TechStack } from "./stack-types";
import { isTechStack } from "./stack-types";
import type { PolyglotBundle } from "./polyglot-types";
import { isPolyglotBundle } from "./polyglot-types";
import type { RepoScan } from "./language-profiles";
import { useCie } from "./cie-store";

const KEY = "cie.workspace.last";

export type LastWorkspace =
  | { kind: "sample"; demo: "banking" | "spring" | "polyglot" }
  | { kind: "job"; jobId: string };

export function rememberWorkspace(w: LastWorkspace): void {
  try { window.localStorage.setItem(KEY, JSON.stringify(w)); } catch { /* private-mode quota — restoring is a nicety, not a fact */ }
}

export function forgetWorkspace(): void {
  try { window.localStorage.removeItem(KEY); } catch {}
}

export function readLastWorkspace(): LastWorkspace | null {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return null;
    const v = JSON.parse(raw) as LastWorkspace;
    if (v?.kind === "sample" && (v.demo === "banking" || v.demo === "spring" || v.demo === "polyglot")) return v;
    if (v?.kind === "job" && typeof v.jobId === "string" && v.jobId) return v;
    return null;
  } catch {
    return null;
  }
}

// Fetch a finished analyze job's exports and open them in the store — the
// same mapping the connect-a-repo dialog performs, reused for launch-time
// restore so a reopened workspace IS the workspace, not a lookalike.
export async function restoreJobResult(jobId: string): Promise<boolean> {
  const res = await fetch(`/api/analyze/result?job=${encodeURIComponent(jobId)}`);
  const data = (await res.json().catch(() => ({}))) as {
    report?: CieReport;
    viewspec?: CieViewSpec | null;
    concepts?: ConceptTree | null;
    deps?: CieDeps | null;
    runtime?: CieRuntime | null;
    cochange?: CieCochange | null;
    overlap?: CieOverlap | null;
    drift?: CieDrift | null;
    stack?: TechStack | null;
    polyglotSpecs?: PolyglotBundle | null;
    label?: string;
    scan?: RepoScan | null;
    sourceRoot?: string | null;
    error?: string;
  } | null;
  if (!res.ok || !data || !isValidReport(data.report ?? null)) return false;
  useCie.getState().setCustom(data.report ?? null, isValidViewspec(data.viewspec) ? data.viewspec : null, isValidConcepts(data.concepts) ? data.concepts : null, data.label ?? "Your project", {
    deps: data.deps?.table ? data.deps : null,
    runtime: data.runtime?.datasets ? data.runtime : null,
    cochange: data.cochange ?? null,
    overlap: data.overlap ?? null,
    drift: data.drift ?? null,
    stack: data.stack && isTechStack(data.stack) ? data.stack : null,
    polyglotSpecs: data.polyglotSpecs && isPolyglotBundle(data.polyglotSpecs) ? data.polyglotSpecs : null,
    repoScan: data.scan ?? null,
    sourceRoot: data.sourceRoot ?? null,
  });
  return true;
}