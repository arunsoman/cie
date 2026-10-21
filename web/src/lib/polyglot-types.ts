/**
 * PRD v2.0 polyglot layer (`cie polyglot --viewspec all`) — the bundle the
 * engine emits in one document so the pipeline reads it with a single
 * parse. Every spec is a §25 ViewSpec under the same honesty law as the
 * v1 map (the `CieViewSpec` in ./cie-types): Fact solid, Inference dashed,
 * Hypothesis dotted, Fog hatched — and evidence is a byte span
 * (`{file, start, end}`) on every element AND edge, enforced twice (Rust
 * `viewspec::validate` mirrors the TS `gate()` clause-for-clause, so a
 * spec that passes there passes here).
 *
 * Artifacts in the bundle (keyed by name):
 * - `c4` — §20 containers/components from the §11.7 concept DAG;
 * - `state-machine:<name>` — §19 tiers T1–T3;
 * - `workflows` — §19.2 forward DFS over resolved call edges;
 * - `impact:<symbolId>` — §23.1 center/caller-rings/duplicate-hemisphere,
 *   one per duplicate-family center (capped at 50, cap counted).
 *
 * These are instances of chart kinds the catalog already enumerates
 * (state diagram, flowchart, C4 architecture, impact graph) — the
 * negotiator + engines render them without new catalog rows.
 */

export interface PolyglotEvidence {
  file: string;
  start: number;
  end: number;
}

export type PolyglotDisplayMode = "Fact" | "Inference" | "Hypothesis" | "Fog";

export interface PolyglotElement {
  id: string;
  elementKind: string;
  displayMode: PolyglotDisplayMode;
  label: string;
  evidence: PolyglotEvidence;
  /** required for every non-Fact element (the gate refuses otherwise) */
  claimId?: string;
  counterArgument?: string;
  fogReason?: string;
  salience?: { factors: string[]; weight: number; whyShown: string };
  /** open bag — tier, machine, ring, similarity, provenance … */
  fields?: Record<string, unknown>;
}

export interface PolyglotEdge {
  source: string;
  target: string;
  edgeKind: string;
  displayMode: PolyglotDisplayMode;
  evidence: PolyglotEvidence;
  fields?: Record<string, unknown>;
}

export interface PolyglotViewSpec {
  elements: PolyglotElement[];
  edges: PolyglotEdge[];
  footer?: Record<string, unknown>;
  /** counted-not-shown honesty: what the producer left out and why */
  gaps?: { reason: string }[];
  scope?: Record<string, unknown>;
  layoutHint?: { strategy: string };
}

export interface PolyglotBundle {
  kind: "cie-polyglot-viewspec-bundle";
  "spec-count": number;
  "impact-centers-truncated": boolean;
  specs: Record<string, PolyglotViewSpec>;
}

/** client-safe, no imports needed elsewhere */
export function isPolyglotBundle(o: unknown): o is PolyglotBundle {
  const b = o as PolyglotBundle | null;
  if (!b || typeof b !== "object" || b.kind !== "cie-polyglot-viewspec-bundle") return false;
  if (!b.specs || typeof b.specs !== "object") return false;
  // spot-check the first spec's shape: elements with span evidence
  const first = Object.values(b.specs)[0];
  if (!first) return typeof b["spec-count"] === "number" && b["spec-count"] === 0;
  const spec = first as PolyglotViewSpec;
  return (
    Array.isArray(spec.elements) &&
    Array.isArray(spec.edges) &&
    spec.elements.every(
      (e) => e && typeof e.id === "string" && !!e.evidence && typeof e.evidence.file === "string" && e.evidence.file.length > 0
    )
  );
}

/** one line for the step list, honest about what was and wasn't produced */
export function polyglotDetail(b: PolyglotBundle): string {
  const specs = Object.keys(b.specs);
  const c4 = specs.filter((k) => k === "c4").length;
  const machines = specs.filter((k) => k.startsWith("state-machine:")).length;
  const flows = specs.filter((k) => k === "workflows").length;
  const impacts = specs.filter((k) => k.startsWith("impact:")).length;
  const parts: string[] = [];
  if (c4) parts.push("C4 components");
  if (machines) parts.push(`${machines} state machine(s)`);
  if (flows) parts.push("workflow(s)");
  if (impacts) parts.push(`${impacts} impact view(s)`);
  if (b["impact-centers-truncated"]) parts.push("impact views capped at 50 — the rest stay counted, not shown");
  return parts.length ? parts.join(", ") + " ready." : "Nothing nameable found — the honest answer for this project.";
}
