// Merge note (RFC-0023 stages 1–5 came in through origin/main): this module
// now carries the two complementary halves of scoped answering —
//
//  1. `scopeDigest(report, viewspec, concepts, selection)` (RFC-0023 stage 5):
//     restricts the REPORT/VIEWSPEC/CONCEPTS triple itself before ANY answerer
//     (local fast-paths included) sees them. File indexes stay stable; symbols,
//     findings, contracts, calls and ancestor rollups outside the selection are
//     removed, and the coverage note says the slice is not project-wide.
//  2. `scopeChatDigest(digest, chips)` (RFC-0024 stage 5): slices the built
//     chat digest to the chips' code space (files ∪ named things ∪ recorded
//     edges) and attaches the binding `selection_scope` manifest + the honest
//     scope line, so the route's system prompt can hold the model to it.
//
// They compose in chat-panel: the inputs are scoped first, then the digest
// is sliced, then the payload carries both. Nothing here invents: a section
// with nothing in scope stays empty and SAYS so.

import type { CieReport, CieViewSpec, ConceptTree } from './cie-types';
import type { ChatContextElement } from './chat-context';

export interface AnswerScope {
  revision: number;
  files: string[];
  symbols: number;
  selections: string[];
  header: string;
}
/** Restrict facts before ANY answerer sees them. File indexes remain stable. */
export function scopeDigest(report: CieReport | null, viewspec: CieViewSpec | null, concepts: ConceptTree | null, selection: ChatContextElement[]) {
  if (!report || !selection.length) return {report, viewspec, concepts, scope:null as AnswerScope | null};
  const files = new Set(selection.filter(c => c.revision === report.rev).flatMap(c => c.files).filter(f => report.files.includes(f)));
  const symbols = report.symbols.filter(s => files.has(report.files[s.loc.file]));
  const ids = new Set(symbols.map(s => s.id));
  const names = new Set(symbols.flatMap(s => [s.name,s.qual_name]));
  const scope: AnswerScope = {
    revision:report.rev, files:[...files], symbols:symbols.length, selections:selection.map(c=>c.label),
    header:`Drawn from ${files.size} files over ${symbols.length} named things, covered by your selections. Other project facts are outside this answer's scope.`,
  };
  const scopedReport: CieReport = {
    ...report,
    files:report.files.map(f => files.has(f) ? f : ''), symbols,
    summaries:report.summaries.filter(s=>ids.has(s.sym)).map(s=>({...s,callees:s.callees.filter(n=>names.has(n)),unresolved_callees:[]})),
    findings:report.findings.filter(f=>ids.has(f.symbol)),
    contracts:report.contracts.filter(c=>c.symbols.length && c.symbols.every(id=>ids.has(id))).map(c=>({...c,verified_by:c.verified_by?.filter(id=>ids.has(id))})),
    coverage:{...report.coverage,files_total:files.size,files_analyzed:files.size,symbols_total:symbols.length,symbols_analyzed:symbols.length,complete:false,notes:['Selection-filtered facts; project-wide coverage does not establish completeness of this slice.']},
    evidence:{...report.evidence,assumptions:[],unresolved:[],unsupported:[]}, warnings:[],
    fault_analysis:null, state_model:null,
  };
  const elements=viewspec?.elements.filter(e=>files.has(e.evidence?.file));
  const elementIds=new Set(elements?.map(e=>e.id));
  const scopedView=viewspec && elements ? {...viewspec,elements,edges:viewspec.edges.filter(e=>elementIds.has(e.source)&&elementIds.has(e.target)),hidden:[],gaps:[],footer:undefined,semanticHash:undefined,renderHash:undefined} : null;
  // Whole-domain behavior/rules can include other files: retain only nodes
  // entirely supported by the selected symbols, not broad ancestor rollups.
  const nodes=concepts?.nodes.filter(n=>n.symbols.length>0 && n.symbols.every(id=>ids.has(id)));
  const nodeIds=new Set(nodes?.map(n=>n.id));
  const scopedConcepts=concepts && nodes ? {...concepts,nodes:nodes.map(n=>({...n,parent:n.parent !== null && nodeIds.has(n.parent) ? n.parent : null,children:n.children.filter(id=>nodeIds.has(id))})),unattached_behaviors:[],unattached_rules:[]} : null;
  return {report:scopedReport,viewspec:scopedView,concepts:scopedConcepts,scope};
}

export interface ScopedDigest {
  scoped: Record<string, unknown>;
  files: string[];
  namedThings: string[];
  scopeLine: string;
}

/** Slice a built digest to the chips' code space (RFC-0024 stage 5). The
 *  digest's sections are plain arrays of objects; the known carriers of
 *  code-space identity are sliced per-field, everything else passes through
 *  with the scope banner attached (the model is told to treat the banner as
 *  binding). Empty chips = today's behavior: the full digest, no banner. */
export function scopeChatDigest(
  digest: Record<string, unknown> | null,
  chips: ChatContextElement[]
): ScopedDigest | null {
  if (!digest) return null;
  if (!chips.length) return { scoped: digest, files: [], namedThings: [], scopeLine: "" };

  const files = new Set<string>();
  const things = new Set<string>();
  for (const c of chips) {
    for (const f of c.files) files.add(f);
    if (c.loc?.file) files.add(c.loc.file);
    things.add(c.label);
    // a module chip names its file directly; a symbol chip may carry it
    const f = c.label.split("/").length > 1 ? c.label : null;
    if (f) files.add(f);
  }

  const inSpace = (text: unknown): boolean => {
    if (typeof text !== "string") return false;
    if (things.has(text)) return true;
    for (const f of files) if (text === f || text.includes(f)) return true;
    // "file.java line 12" / "file.java:12" carry the file name
    for (const t of things) if (text.includes(t)) return true;
    return false;
  };

  const sliceArray = (arr: unknown[]): unknown[] =>
    arr.filter((item) => {
      if (typeof item === "string") return inSpace(item);
      if (item && typeof item === "object") {
        const o = item as Record<string, unknown>;
        return (
          inSpace(o.where) ||
          inSpace(o.in_code_name) ||
          inSpace(o.file) ||
          inSpace(o.label) ||
          inSpace(o.name) ||
          (Array.isArray(o.must_be_kept_by) && o.must_be_kept_by.some(inSpace)) ||
          (Array.isArray(o.found_in) && o.found_in.some(inSpace))
        );
      }
      return false;
    });

  const out: Record<string, unknown> = { ...digest };
  for (const key of ["findings", "promises", "repeated_behavior", "cochange_facts", "overlap_facts", "drift_facts"]) {
    if (Array.isArray(digest[key])) out[key] = sliceArray(digest[key] as unknown[]);
  }

  const scope = {
    rule: "ONLY the selection's code space — the chips below are the entire world of this question",
    chips: chips.map((c) => ({ label: c.label, kind: c.kind, file: c.loc?.file ?? null })),
    files: [...files],
    named_things: [...things],
  };
  out.selection_scope = scope;

  const scopeLine = `drawn from ${files.size} file(s) over ${things.size} named thing(s) — the selection only`;
  return { scoped: out, files: [...files], namedThings: [...things], scopeLine };
}
