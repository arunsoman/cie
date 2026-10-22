// PRD v2.0 §25 — the polyglot bundle → viz-adapter ViewSpec mapper.
//
// The engine emits one bundle (`cie polyglot <dir> --viewspec all`, kind
// "cie-polyglot-viewspec-bundle") whose specs are ALREADY in the §25
// ViewSpec honesty contract — byte-span evidence on every element AND
// edge, Fact/Inference/Hypothesis/Fog display modes, salience bags, counted
// gaps — and the Rust producer validated them against the same clause set
// the TS gate runs (viewspec::validate mirrors gate.ts clause-for-clause).
//
// What this module adds is only the last mile the renderer needs and the
// open-bag producer does not carry:
//   - `salience` / `fields` defaults (the gate reads salience.weight on
//     hidden rows; slice.ts reads it on every element);
//   - edge `id`s (the producer keys edges positionally; the adapter needs
//     stable ids for selection);
//   - the EpistemicFooter with counted display modes + a caption;
//   - `{reason}` gaps → `{id, area, why}` Gap rows;
//   - a ScopeAttestation (the bundle's per-artifact scope object keeps its
//     own keys — artifact/tier/center — which the component reads straight
//     from the raw spec);
//   - semanticHash / renderHash, recomputed with the adapter's own
//     functions (never hand-rolled);
//   - layoutHint mapped onto the strategies the g6 render path supports
//     today ("cie-state-machine" / "cie-impact-duplicate" are named for
//     future dedicated layouts; dagre is the honest baseline until then).
//
// Then the REAL gate runs on the mapped spec — a mapped spec that fails is
// a NAMED result (the stable codes print), never a silently drawn lie.

import { gate, recomputeSemanticHash, recomputeRenderHash } from "viz-adapter";
import type { ViewSpec, ViewElement, ViewEdge } from "viz-adapter";
import type { PolyglotBundle, PolyglotViewSpec } from "@/lib/polyglot-types";

export type PolyglotArtifactKind = "c4" | "workflows" | "state-machine" | "impact";

/** Which artifact a bundle key carries. Unknown `impact:<id>`-shaped keys
 *  are impact views; anything else stays an honest unknown → impact is the
 *  catch-all only because the bundle vocabulary is closed (c4, workflows,
 *  state-machine:*, impact:*). */
export function artifactKind(name: string): PolyglotArtifactKind {
  if (name === "c4") return "c4";
  if (name === "workflows") return "workflows";
  if (name.startsWith("state-machine:")) return "state-machine";
  return "impact";
}

/** Stable presentation order: architecture → flows → machines → impact. */
export function artifactOrder(name: string): number {
  const k = artifactKind(name);
  return k === "c4" ? 0 : k === "workflows" ? 1 : k === "state-machine" ? 2 : 3;
}

/** Human title + one-line plain-words hint for the picker. */
export function artifactMeta(
  name: string,
  spec: PolyglotViewSpec,
): { label: string; hint: string } {
  const k = artifactKind(name);
  if (k === "c4")
    return {
      label: "C4 — containers and components",
      hint: "Manifest-rooted containers, framework- and naming-evidenced components, §22.3-closed relationship reasons.",
    };
  if (k === "workflows")
    return {
      label: "Workflows — how calls flow",
      hint: "Entry points and resolved call edges forward; async boundaries are Fog on purpose.",
    };
  if (k === "state-machine") {
    const machine = name.slice("state-machine:".length);
    const tier = spec.footer && typeof spec.footer.tier === "string" ? spec.footer.tier : "";
    return {
      label: `State machine — ${machine}${tier ? ` (${tier})` : ""}`,
      hint: "States and transitions the engine detected, tier-labelled by how it knows.",
    };
  }
  const center = spec.elements.find((e) => e.fields && e.fields.role === "center");
  return {
    label: `Impact — ${center ? center.label : name.slice("impact:".length)}`,
    hint: "Caller rings from resolved call edges, plus the duplicate hemisphere (§23.2).",
  };
}

const LAYOUT_FALLBACK = "dagre";

/** The g6 render path supports dagre / elk-layered / tree strategies and
 *  sequence lanes today; producer-named strategies that have no dedicated
 *  renderer yet render on the dagre baseline (the bundle keeps the
 *  producer's name — this mapping is presentation-only).
 *  Edgeless graphs never touch dagre: a layering layout with zero edges
 *  never settles (an honest grid is the same rule chartView() applies). */
function renderLayout(spec: PolyglotViewSpec): string {
  if (spec.edges.length === 0) return "grid";
  const raw = spec.layoutHint?.strategy ?? "";
  return raw === "dagre" || raw === "elk-layered" ? raw : LAYOUT_FALLBACK;
}

/** Map one bundle spec to a gated, renderable ViewSpec. A refusal is a
 *  named result: `view: null` + the gate's stable codes, verbatim. */
export function mapPolyglotSpec(
  spec: PolyglotViewSpec,
  ctx: { revision?: number; artifact?: string } = {},
): { view: ViewSpec | null; violations: string[] } {
  const elements: ViewElement[] = spec.elements.map((e) => ({
    ...e,
    salience:
      e.salience ??
      {
        factors: ["producer-default"],
        weight: e.displayMode === "Fact" ? 1 : 0.6,
        whyShown: "shown by the producer with cited source bytes",
      },
    fields: e.fields ?? {},
  }));

  const edges: ViewEdge[] = spec.edges.map((e, i) => ({
    ...e,
    id: `${e.source}->${e.target}:${e.edgeKind}#${i}`,
  }));

  const count = (mode: string) =>
    elements.filter((e) => e.displayMode === mode).length +
    edges.filter((e) => e.displayMode === mode).length;

  const footer = spec.footer ?? {};
  const caption =
    typeof footer.note === "string"
      ? footer.note
      : typeof footer.counterArgument === "string"
        ? footer.counterArgument
        : "Named by the polyglot layer — every box cites the bytes that earned it.";

  const view: ViewSpec = {
    elements,
    edges,
    footer: {
      facts: count("Fact"),
      inferences: count("Inference"),
      hypotheses: count("Hypothesis"),
      fog: count("Fog"),
      stale: 0,
      caption,
    },
    gaps: (spec.gaps ?? []).map((g, i) => ({
      id: `gap:${ctx.artifact ?? "spec"}:${i}`,
      area: ctx.artifact ?? "polyglot",
      why: g.reason,
    })),
    hidden: [],
    scope: {
      revision: ctx.revision ?? 0,
      environments: [],
      coverage: { analyzed: true },
    },
    layoutHint: { strategy: renderLayout(spec) },
    semanticHash: "",
    renderHash: "",
  };
  view.semanticHash = recomputeSemanticHash(view);
  view.renderHash = recomputeRenderHash(view, "");

  try {
    gate(view);
    return { view, violations: [] };
  } catch (err) {
    return { view: null, violations: [err instanceof Error ? err.message : String(err)] };
  }
}

export interface PolyglotArtifact {
  name: string; // the bundle key, verbatim
  kind: PolyglotArtifactKind;
  label: string;
  hint: string;
  spec: PolyglotViewSpec;
  view: ViewSpec | null; // null = the gate refused; violations say exactly why
  violations: string[];
}

/** Every artifact in the bundle, in stable order (c4 → workflows →
 *  machines → impact), each independently gated — one refused spec never
 *  takes the others down. */
export function polyglotArtifacts(
  bundle: PolyglotBundle,
  ctx: { revision?: number } = {},
): PolyglotArtifact[] {
  return Object.entries(bundle.specs)
    .sort(([a], [b]) => artifactOrder(a) - artifactOrder(b) || a.localeCompare(b))
    .map(([name, spec]) => {
      const kind = artifactKind(name);
      const { label, hint } = artifactMeta(name, spec);
      const { view, violations } = mapPolyglotSpec(spec, { ...ctx, artifact: name });
      return { name, kind, label, hint, spec, view, violations };
    });
}
