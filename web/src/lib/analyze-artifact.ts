// Shared by both result routes (core + extra): fetches one storage
// artifact field as its own small call instead of the whole `ui_bootstrap_v1`
// bundle, so a large repo's combined bundle size never has to fit in one
// response (cie-storage/src/retrieval/mod.rs's 16MB response-byte ceiling,
// the policy max — not something either route can raise further).
//
// Big repositories can exceed that ceiling with a SINGLE kind (`report` is
// ~48MB on a 3.4k-file repo). Such kinds are fetched as report *sections*
// (cie-storage's `Output::Artifact { section, page }`): each stored field
// served whole, arrays served as bounded windows with a plain-offset
// continuation. The whole-kind path stays first so repositories whose
// artifact already fits keep the one-call shape.
import { storageCall, StorageFailure } from "./storage-native";

// The storage layer treats ANY field whose stored value is null/missing —
// not just overlap/drift — as a candidate for an on-demand rebuild
// (cie-storage/src/retrieval/outputs.rs's `missing` check). Since this web
// transport never injects a DeferredBuilder (only the MCP transport does),
// every such field errors with ArtifactUnavailable instead of coming back
// null — e.g. `deps` for a repo with no manifest CIE recognizes. Fetching
// the whole `ui_bootstrap_v1` bundle never hit this (it never subjects the
// bundle's own top-level fields to that check). ArtifactUnavailable is the
// expected "nothing was captured for this kind" signal, not a real
// failure, so `fetchArtifactField` always resolves to a value — overlap/
// drift get the same placeholder shape ingestion publishes at ingest time
// (cie-cli/src/ingestion_job.rs's literals), so the dashboard sees the
// identical "not checked yet" state either way; every other field just
// resolves to null.
const DEFERRED_PLACEHOLDER: Record<string, unknown> = {
  overlap: { pairs: [], unmapped: [], min_shared: 1, incomplete_reason: "deferred_on_demand" },
  drift: {
    rev: null, docs: [], claims: [], mappings: [], verifications: [], reverse_drift: [],
    aggregate: {}, complete: false, generated_ns: 0, incomplete_reason: "deferred_on_demand",
  },
};

const BUDGET = { max_response_bytes: 16 * 1024 * 1024, timeout_ms: 30000, max_visited: 1000000 };

// Artifact outputs ignore the query ("filter_application":"inapplicable"),
// but a `select {kind:"file"}` still materializes every captured file
// entity before rendering anything (~3–11s per call on large repos). A
// never-matching predicate keeps the match phase out of artifact calls.
const ARTIFACT_QUERY = {
  select: { kind: "file", filter: { field: { field: "path", op: "eq", value: "__cie_none__" } } },
};

interface WholeArtifactOutput {
  schema_version: number;
  status: string;
  error?: { code: string; message: string };
  data: { type: string; document: unknown };
}
interface SectionArtifactOutput {
  schema_version: number;
  status: string;
  error?: { code: string; message: string };
  data: { type: string; document: {
    section: string;
    total: number;
    offset: number;
    complete: boolean;
    document: unknown;
  } };
}

// report sections the workspace consumes. Everything the UI reads from the
// report is stored as one of these fields (the engine's own report writer);
// a field the revision didn't capture degrades to absent — the same honest
// empty state a bundle without it produces. The whole-report path (small
// repos) never sees the section list; a revision that captures MORE fields
// keeps serving them whole today and via describe's catalog tomorrow.

const unwrap = (output: WholeArtifactOutput | SectionArtifactOutput): { value: unknown; error?: { code: string; message: string } } => {
  if (output.error || (output.data as { type?: string } | undefined | null)?.type !== "artifact") {
    return { value: null, error: output.error ?? { code: "ArtifactUnavailable", message: "This artifact is unavailable in the selected revision" } };
  }
  return { value: output.data.document ?? null };
};

/** A whole stored artifact kind — the shape every small/medium repo uses. */
async function fetchWholeField(context: string, kind: string): Promise<unknown> {
  let res;
  try {
    res = await storageCall<{ outputs: WholeArtifactOutput[] }>({
      op: "retrieve",
      context,
      request: { query: ARTIFACT_QUERY, outputs: [{ id: kind, output: { artifact: { kind } } }], budget: BUDGET },
    });
  } catch (error) {
    const code = storageCode(error);
    if (code === "ArtifactUnavailable") return DEFERRED_PLACEHOLDER[kind] ?? null;
    throw error;
  }
  const { value, error } = unwrap(res.outputs[0]);
  if (value !== null) return value;
  // Only the "nothing was captured for this kind" signal resolves to a
  // placeholder — a genuinely FAILED on-demand build (ProjectionUnavailable)
  // still throws, so trust-sensitive fields never disguise failure.
  if (error && error.code === "ArtifactUnavailable") return DEFERRED_PLACEHOLDER[kind] ?? null;
  throw new StorageFailure(error?.code ?? "Io", error?.message ?? "Artifact unavailable");
}

/** One stored report section: whole object, or a paged array window. */
async function fetchSection(context: string, section: string, limit: number, offset: string | undefined): Promise<{ document: unknown; complete: boolean; total: number; offset: number } | undefined> {
  const res = await storageCall<{ outputs: SectionArtifactOutput[] }>({
    op: "retrieve",
    context,
    request: { query: ARTIFACT_QUERY, outputs: [{ id: section, output: { artifact: { kind: "report", section, page: { limit, continuation: offset } } } }], budget: BUDGET },
  });
  const out = res.outputs[0];
  if (out.error || out.status === "failed" || out.status === "unavailable") return undefined;
  return out.data.document;
}

/** Sections the pinned revision actually captured (one 1ms describe call).
 *  `fault_analysis` is merged into the report at publish and is not listed
 *  by describe on every revision, so it's attempted opportunistically. */
const SECTION_ORDER = [
  "rev", "files", "symbols", "summaries", "contracts", "findings", "concepts",
  "usecase_view", "summary_cache", "state_model", "interaction_gaps", "evidence",
  "warnings", "coverage", "incremental", "summary_reuse", "ingest_wall_ns",
  "concepts_tier", "fault_analysis",
];

async function capturedReportSections(context: string): Promise<Set<string> | null> {
  try {
    const described = await storageCall<{ result: { available_inputs?: { captured_snapshot_fields?: string[] } } }>({
      op: "describe",
      context,
    });
    const fields = (described as { result?: { available_inputs?: { captured_snapshot_fields?: string[] } } }).result?.available_inputs?.captured_snapshot_fields;
    return Array.isArray(fields) && fields.length ? new Set<string>(fields) : null;
  } catch {
    return null; // describe is a nicety — composition attempts the full list
  }
}

/**
 * The `report` kind, sized for any repository: composed from stored
 * sections — the describe catalog names the captured fields (1ms), array
 * fields page at a probe-proven 2000 records, window retries halve on a
 * byte-budget failure (offset continuations make retries trivially
 * resumable), and fields the revision didn't capture stay absent. The
 * first sectioned call pays the one-time pinned-bundle parse; every
 * further one is a milliseconds-level window. A whole-artifact fetch is
 * kept only as the last-resort fallback for a capture the sectioned
 * engine path can't read (older engines without sections).
 */
async function fetchReportField(context: string): Promise<unknown> {
  const present = await capturedReportSections(context);
  const composed: Record<string, unknown> = {};
  const wanted = present
    ? [...SECTION_ORDER, ...[...present].filter((field) => !SECTION_ORDER.includes(field))]
    : [...SECTION_ORDER];
  for (const section of wanted) {
    if (present && !present.has(section) && section !== "fault_analysis") continue;
    let limit = 2000;
    let cursor: string | undefined;
    let windows: unknown[] | null = null; // array sections accumulate
    let total = 0;
    // object sections resolve in one window; arrays loop until complete.
    for (;;) {
      let chunk: Awaited<ReturnType<typeof fetchSection>> | undefined;
      try {
        chunk = await fetchSection(context, section, limit, cursor);
      } catch (error) {
        if (!(storageCode(error) === "BudgetExceeded" || storageCode(error) === "Io")) throw error;
        // Byte budget again — halve and retry the same cursor; scalars
        // (limit already minimal) degrade to absent once limit bottoms out.
        if (limit > 1) { limit = Math.max(1, Math.floor(limit / 2)); continue; }
        break;
      }
      if (!chunk) {
        // Section not captured in this revision — it stays absent (the
        // same state a bundle without that field produces).
        if (!cursor && !windows) break;
        break;
      }
      total = chunk.total;
      const window = chunk.document;
      if (Array.isArray(window)) {
        windows = [...(windows ?? []), ...window];
        if (chunk.complete) break;
        cursor = String(chunk.offset + window.length);
      } else {
        composed[section] = window;
        break;
      }
    }
    if (windows) {
      // A whole paged section that came back SHORT (retries skipped a
      // window mid-loop) is still bounded-honest: expose what the capture
      // holds per window ordering.
      composed[section] = windows;
      if (total && windows.length !== total) composed[`${section}_truncated`] = true;
    }
  }
  if (!Array.isArray(composed.files) || !Array.isArray(composed.symbols) || !Array.isArray(composed.findings)) {
    // Composed shape unavailable — fall back to the whole artifact for
    // engines that predate sections; absent either way stays null.
    try { return await fetchWholeField(context, "report"); }
    catch (error) { return storageCode(error) === "ArtifactUnavailable" ? null : Promise.reject(error); }
  }
  return composed;
}

// The worker bridge rejects with a StorageFailure constructed from
// whichever module instance spawned the worker — in dev, bundlers may load
// this module twice, so instanceof across copies lies. Duck-typing the
// code is the honest check.
const storageCode = (error: unknown): string | undefined =>
  (error as { code?: string } | undefined)?.code;

export async function fetchArtifactField(context: string, kind: string): Promise<unknown> {
  return kind === "report" ? fetchReportField(context) : fetchWholeField(context, kind);
}

// `report` is the one field every rendered view needs (StartView — the
// page actually shown right after a project loads — reads only `report`;
// see sections.tsx's StartView). Everything else is read by specific
// later tabs (map/mastery/trust/deps/…), not the landing page, so it's
// fetched lazily by the `extra` route instead of blocking first paint.
export const CORE_FIELDS = ["report"] as const;
export const EXTRA_FIELDS = [
  "viewspec", "concepts", "stack", "deps", "cochange", "polyglotSpecs",
  "runtime", "quality", "stories", "overlap", "drift",
] as const;