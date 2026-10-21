import { recomputeSemanticHash, recomputeRenderHash } from "viz-adapter";
import type { ViewSpec, ViewElement } from "viz-adapter";
import type { ChatChartSpec, ElementLoc } from "../chat-chart";
import type { CieReport, CieViewSpec, ConceptTree } from "../cie-types";
export interface ChartElement {
  id: string;
  label: string;
  kind: string;
  loc?: ElementLoc;
  files: string[];
  parentId?: string;
  mode?: ViewElement["displayMode"];
  lines?: string[];
  selected?: boolean;
}
/** The catalog bridge is total over the chat vocabulary. No drawing path
 * can infer evidence or upgrade a certainty category from a missing field. */
export const CHAT_CATALOG = {
  hbar: "H3",
  donut: "H3",
  stat: "H3",
  table: "H1",
  uml: "A1",
  flow: "E1",
  tree: "E10",
  heatmap: "G5",
  sequence: "B3",
  c4: "A10",
  mindmap: "E10",
} as const;
export function chartElements(
  spec: ChatChartSpec,
  report: CieReport | null,
  concepts: ConceptTree | null,
): ChartElement[] {
  if (spec.nodes)
    return spec.nodes.map((n) => {
      const files = n.loc?.file
        ? [n.loc.file]
        : (report?.files ?? []).filter(
            (f) =>
              f === n.id ||
              f.startsWith(n.id + "/") ||
              (n.id === "(project root)" && !f.includes("/")),
          );
      const sym = report?.symbols.find((s) =>
        files.includes(report.files[s.loc.file]),
      );
      // A file-table anchor is a recorded file, never a fabricated line number.
      const loc =
        n.loc ??
        (files[0]
          ? {
              file: files[0],
              ...(sym
                ? { byteStart: sym.loc.start, byteEnd: sym.loc.end }
                : {}),
            }
          : undefined);
      return {
        id: n.id,
        label: n.title,
        kind: n.kind,
        loc,
        files,
        mode: n.certainty,
        lines: n.lines,
        selected: n.selected,
      };
    });
  if (spec.tree)
    return spec.tree.map((n) => {
      const concept = concepts?.nodes.find(
        (c) => String(c.id) === n.id || `c${c.id}` === n.id,
      );
      const syms =
        report?.symbols.filter((s) => concept?.symbols.includes(s.id)) ?? [];
      const files = [
        ...new Set(syms.map((s) => report!.files[s.loc.file]).filter(Boolean)),
      ];
      const first = syms[0];
      return {
        id: n.id,
        label: n.label,
        kind: n.kind,
        parentId: n.parent ?? undefined,
        mode: n.certainty,
        files,
        loc: first
          ? {
              file: report!.files[first.loc.file],
              byteStart: first.loc.start,
              byteEnd: first.loc.end,
            }
          : undefined,
        lines: n.note ? [n.note] : [],
      };
    });
  if (spec.msgs)
    return spec.msgs.map((m, i) => ({
      id: `message:${i}:${m.label}`,
      label: `${m.from} → ${m.to}: ${m.label}`,
      kind: "step",
      loc: m.loc,
      files: m.loc?.file ? [m.loc.file] : [],
      mode: m.status === "inferred" ? "Inference" : "Fact",
    }));
  if (spec.tiles)
    return spec.tiles.map((t, i) => ({
      id: `tile:${i}:${t.name}`,
      label: `${t.name}: ${t.value}${t.label ? ` · ${t.label}` : ""}`,
      kind: "metric",
      loc: t.loc,
      files: t.loc?.file ? [t.loc.file] : [],
    }));
  if (spec.rows)
    return spec.rows.map((row, i) => ({
      id: `row:${i}`,
      label: row.join(" · "),
      kind: "row",
      loc: spec.rowLoc?.[i] ?? undefined,
      files: spec.rowLoc?.[i]?.file ? [spec.rowLoc[i]!.file] : [],
    }));
  return (spec.data ?? []).map((d, i) => ({
    id: `point:${i}:${d.name}`,
    label: `${d.name}: ${d.value}${spec.unit ?? ""}`,
    kind: "metric",
    loc: d.loc,
    files: d.loc?.file ? [d.loc.file] : [],
  }));
}
export function chartView(
  spec: ChatChartSpec,
  report: CieReport | null,
  concepts: ConceptTree | null,
  sealed: CieViewSpec | null = null,
) {
  const entries = chartElements(spec, report, concepts);
  const originals = new Map(sealed?.elements.map((e) => [e.id, e]));
  const elements: ViewSpec["elements"] = [];
  const hidden: ViewSpec["hidden"] = [];
  for (const n of entries) {
    const original = originals.get(n.id);
    const evidence =
      original?.evidence ??
      (n.loc?.file
        ? {
            file: n.loc.file,
            start: n.loc.byteStart ?? 0,
            end: n.loc.byteEnd ?? n.loc.byteStart ?? 0,
          }
        : null);
    if (!evidence) {
      hidden.push({
        id: n.id,
        label: n.label,
        kind: n.kind,
        reason: "No recorded source span; available in text outline only",
        salienceWeight: 0,
      });
      continue;
    }
    const mode = (original?.displayMode ??
      n.mode ??
      "Fact") as ViewElement["displayMode"];
    elements.push({
      id: n.id,
      elementKind: n.kind,
      label: n.label,
      evidence,
      displayMode: mode,
      claimId:
        original?.claimId ?? (mode !== "Fact" ? `derived:${n.id}` : undefined),
      counterArgument:
        original?.counterArgument ??
        (mode === "Hypothesis"
          ? "This hypothesis is not statically proven."
          : undefined),
      fogReason: original?.fogReason,
      salience: original?.salience ?? {
        factors: ["STORE_RECORD"],
        weight: 0.6,
        whyShown: "Recorded export with cited source span",
      },
      fields: {
        ...original?.fields,
        bodyLines: n.lines?.map((text) => ({
          text,
          line: n.loc?.line ?? 0,
          evidence,
        })),
        selected: n.selected || undefined,
      },
      parentId: n.parentId,
    });
  }
  const ids = new Set(elements.map((e) => e.id));
  const byId = new Map(elements.map((e) => [e.id, e]));
  const edges: ViewSpec["edges"] = (spec.links ?? [])
    .filter((l) => ids.has(l.from) && ids.has(l.to))
    .map((l, i) => ({
      id: `edge:${i}`,
      source: l.from,
      target: l.to,
      edgeKind: l.label ?? "relates",
      displayMode: l.certainty ?? (l.dashed ? "Inference" : "Fact"),
      evidence: byId.get(l.from)!.evidence,
    }));
  for (const e of elements)
    if (e.parentId && ids.has(e.parentId))
      edges.push({
        id: `parent:${e.id}`,
        source: e.parentId,
        target: e.id,
        edgeKind: "contains",
        displayMode: e.displayMode,
        evidence: e.evidence,
      });
  if (spec.kind === "sequence") {
    for (let i = 1; i < elements.length; i++) {
      const from = elements[i - 1],
        to = elements[i];
      edges.push({
        id: `order:${i}`,
        source: from.id,
        target: to.id,
        edgeKind: "recorded order (not runtime)",
        displayMode:
          from.displayMode === "Fact" && to.displayMode === "Fact"
            ? "Fact"
            : "Inference",
        evidence: from.evidence,
      });
    }
  }
  edges.forEach((e) => {
    const source = entries.find((n) => n.id === e.source),
      target = entries.find((n) => n.id === e.target);
    entries.push({
      id: e.id,
      label: `${source?.label ?? e.source} → ${target?.label ?? e.target}: ${e.edgeKind}`,
      kind: "edge",
      loc: source?.loc,
      files: [...new Set([...(source?.files ?? []), ...(target?.files ?? [])])],
    });
  });
  const view: ViewSpec = {
    elements,
    edges,
    hidden,
    gaps: hidden.length
      ? [
          {
            id: "missing-spans",
            area: "evidence",
            why: `${hidden.length} rows have no source span; text outline only`,
          },
        ]
      : [],
    footer: {
      facts: elements.filter((e) => e.displayMode === "Fact").length,
      inferences: elements.filter((e) => e.displayMode === "Inference").length,
      hypotheses: elements.filter((e) => e.displayMode === "Hypothesis").length,
      fog: elements.filter((e) => e.displayMode === "Fog").length,
      stale: 0,
      caption: spec.source ?? "Recorded project exports",
    },
    scope: {
      revision: report?.rev ?? 0,
      environments: [],
      coverage: { analyzed: report?.coverage.complete ?? false },
    },
    layoutHint: {
      strategy:
        spec.kind === "tree" || spec.kind === "mindmap"
          ? "compact-box"
          : spec.kind === "sequence"
            ? "sequence-lanes"
            : edges.length
              ? "dagre"
              : "grid",
    },
    semanticHash: "",
    renderHash: "",
  };
  view.semanticHash = recomputeSemanticHash(view);
  view.renderHash = recomputeRenderHash(view, "");
  return { view, entries };
}
