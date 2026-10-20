// RFC-0024 stage 2 — client-side per-scope ViewSpec slicing + re-gate
// (web/src/lib/viz/slice.ts; RFC-0023 §4.3, RFC-0024 §3.3).
//
// R3 panels re-slice to the selected scope without a server round-trip.
// The slice is NOT a hand-filter: it re-runs the adapter gate on the
// sliced spec and recomputes the footer from the FINAL element set
// (VIZ-A-07) — a slice that fails the gate is a NAMED result (the gate
// codes print), never a blank panel.

import { gate, ELEMENT_CAP, recomputeSemanticHash, recomputeRenderHash } from "viz-adapter";
import type { ViewSpec } from "viz-adapter";

export interface GateOutcome {
  spec: ViewSpec | null;
  violations: string[]; // stable gate codes + ids, printed verbatim
  dropped: number; // elements removed by the slice (disclosed in the tell)
}

/** Slice a ViewSpec to a code space: keep the element ids named, their
 *  parentId closure (drill chains), and every edge between kept ids.
 *  `factsOnly` applies the REGATE verb: non-Fact elements fade (dropped
 *  here, with the count disclosed). */
export function sliceViewSpec(
  view: ViewSpec,
  opts: { keepIds?: Set<string>; factsOnly?: boolean } = {},
): GateOutcome {
  const keep = opts.keepIds;
  const byId = new Map(view.elements.map((e) => [e.id, e]));

  // parentId closure: a kept child keeps its parent chain.
  const kept = new Set<string>();
  const addWithParents = (id: string) => {
    if (kept.has(id)) return;
    const el = byId.get(id);
    if (!el) return;
    kept.add(id);
    if (el.parentId) addWithParents(el.parentId);
  };
  for (const el of view.elements) {
    if (!keep || keep.has(el.id)) addWithParents(el.id);
  }

  let elements = view.elements.filter((e) => kept.has(e.id));
  const factsDropped = opts.factsOnly
    ? elements.filter((e) => e.displayMode !== "Fact").length
    : 0;
  if (opts.factsOnly) elements = elements.filter((e) => e.displayMode === "Fact");
  const keptIds = new Set(elements.map((e) => e.id));
  const edges = view.edges.filter((e) => keptIds.has(e.source) && keptIds.has(e.target));

  const sliced: ViewSpec = {
    ...view,
    elements,
    edges,
    footer: {
      facts: elements.filter((e) => e.displayMode === "Fact").length,
      inferences: elements.filter((e) => e.displayMode === "Inference").length,
      hypotheses: elements.filter((e) => e.displayMode === "Hypothesis").length,
      fog: elements.filter((e) => e.displayMode === "Fog").length,
      stale: 0,
      caption: view.footer.caption,
    },
    gaps: view.gaps,
    hidden: view.hidden,
    semanticHash: "",
    renderHash: "",
  };
  sliced.semanticHash = recomputeSemanticHash(sliced);
  sliced.renderHash = recomputeRenderHash(sliced, "");

  // Re-gate ONCE, client-side, with the shared stable codes.
  try {
    gate(sliced);
    return { spec: sliced, violations: [], dropped: factsDropped };
  } catch (err) {
    return {
      spec: null,
      violations: [err instanceof Error ? err.message : String(err)],
      dropped: factsDropped,
    };
  }
}

export { ELEMENT_CAP };
