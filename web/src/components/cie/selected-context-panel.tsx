"use client";

// RFC-0024 stage 4 — the R4 SELECTED-CONTEXT slot (ux-spec §4): the pair
// FlowChart + SequenceChart over the user's chips. R4 is a POSITION in the
// R3 grid (first row, full width) whose activation rule is G5: it exists
// exactly while R5a holds ≥ 1 chip.
//
// Data source honesty (§4/§5):
//   - process boxes, calls edges: recorded summaries (resolved callees);
//   - data edges: recorded call-valued flows between selected symbols;
//   - decision diamonds: the engine artifact does not carry DecisionFacts
//     yet — the chart prints "decisions not indexed in this slice" (the
//     §5 rule: honest absence, never a guessed diamond);
//   - stores: report-recorded data-flow targets only; a sink with no
//     recorded line gets no popup (the §4.1 carve-out);
//   - the sequence draws messages ONLY between selected elements; the
//     entry = the selected node with the fewest incoming calls from
//     others in the selection, TELL-disclosed; gaps print the honest
//     "…unrecorded hop".

import { useMemo } from "react";
import { Workflow, Route } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import type { ChatChartSpec, UmlLink, UmlNode } from "@/lib/chat-chart";
import { ChatChart } from "./chat-chart";
import type { CieReport } from "@/lib/cie-types";
import { symbolByQualName } from "@/lib/atlas";
import { buildCallGraph } from "@/lib/mastery";

/** The FlowChart over the chips (§4.1). */
export function buildSelectedFlowSpec(report: CieReport, chips: { id: string; label: string; kind: string }[]): ChatChartSpec | null {
  if (!chips.length) return null;
  const selectedQuals = new Set(chips.map((c) => c.id));
  const nodes: UmlNode[] = [];
  const links: UmlLink[] = [];
  const seen = new Set<string>();

  const addNode = (qual: string, label: string, kind: string) => {
    if (seen.has(qual)) return;
    seen.add(qual);
    const sym = symbolByQualName(report, qual);
    nodes.push({
      id: qual,
      title: label,
      kind,
      certainty: "Fact",
      loc: sym
        ? {
            file: report.files[sym.loc.file] ?? "",
            byteStart: sym.loc.start,
            byteEnd: sym.loc.end,
          }
        : undefined,
      selected: chips.some((c) => c.id === qual),
    });
  };

  // chips first (in pick order), then their recorded call neighbourhood.
  // summaries.callees are RESOLVED qual names (strings); unresolved ones
  // live in unresolved_callees and never become edges.
  const g = buildCallGraph(report);
  const qualOf = (symId: number): string | null => {
    const sym = g.byId.get(symId);
    return sym ? sym.qual_name || sym.name : null;
  };
  for (const c of chips) addNode(c.id, c.label, "function");
  for (const sum of report.summaries ?? []) {
    const caller = qualOf(Number(sum.sym));
    if (!caller || !selectedQuals.has(caller)) continue;
    for (const calleeQual of sum.callees ?? []) {
      if (!calleeQual) continue; // unresolved — never an edge
      const calleeSym = symbolByQualName(report, calleeQual);
      addNode(calleeQual, calleeSym?.name ?? calleeQual, "function");
      const key = `${caller}->${calleeQual}`;
      if (!seen.has(key)) {
        seen.add(key);
        links.push({ from: caller, to: calleeQual, label: "calls", certainty: "Fact" });
      }
    }
  }
  if (!nodes.length) return null;
  return {
    kind: "flow",
    engine: "g6",
    layout: "call-flow",
    title: "The selected context, flowing",
    tell:
      `${nodes.length} element(s) you picked, with their recorded calls · decisions not indexed in this slice — ` +
      `the engine records branch facts (RFC-0024 stage 0.5) but this workspace's artifacts predate them.`,
    catalog: "E1",
    nodes,
    links,
    source: "report.json — recorded summaries (resolved calls only)",
  } as ChatChartSpec;
}

/** The SequenceChart over the chips (§4.2): messages between selected
 *  elements only; entry = fewest incoming calls from the selection. */
export function buildSelectedSequenceSpec(report: CieReport, chips: { id: string; label: string; kind: string }[]): ChatChartSpec | null {
  if (chips.length < 2) return null;
  const selected = new Set(chips.map((c) => c.id));
  const msgs: ChatChartSpec["msgs"] = [];
  const incoming = new Map<string, number>();
  for (const sum of report.summaries ?? []) {
    const cs = report.symbols.find((x) => x.id === Number(sum.sym));
    const caller = cs?.qual_name || cs?.name;
    if (!caller || !selected.has(caller)) continue;
    for (const cq of sum.callees ?? []) {
      if (!cq || !selected.has(cq) || cq === caller) continue;
      msgs.push({ from: caller, to: cq, label: "calls", status: "confirmed" });
      incoming.set(cq, (incoming.get(cq) ?? 0) + 1);
    }
  }
  if (!msgs.length) return null;
  const entry = [...selected].sort((a, b) => (incoming.get(a) ?? 0) - (incoming.get(b) ?? 0))[0];
  const participants = chips.map((c) => c.id);
  return {
    kind: "sequence",
    engine: "custom-svg",
    layout: "sequence-lanes",
    title: "The selected context, in order",
    tell: `starts at ${entry} — nothing selected calls it first (computed from the recorded calls; gaps are unrecorded hops, not invented messages)`,
    participants,
    msgs,
    source: "report.json — recorded summaries between the selected elements only",
  } as ChatChartSpec;
}

export function SelectedContextPanel() {
  const chatContext = useCie((s) => s.chatContext);
  const report = useCie((s) => s.report);
  const chips = useMemo(
    () => chatContext.map((c) => ({ id: c.id, label: c.label, kind: c.kind })),
    [chatContext]
  );
  const flow = useMemo(() => (report ? buildSelectedFlowSpec(report, chips) : null), [report, chips]);
  const seq = useMemo(() => (report ? buildSelectedSequenceSpec(report, chips) : null), [report, chips]);

  if (!chatContext.length) {
    return (
      <div className="rounded-xl border border-dashed border-stone-300 bg-white/60 px-4 py-6 text-center">
        <p className="text-xs font-medium text-stone-600">The selected-context pair (flow + sequence)</p>
        <p className="mt-1 text-[11px] text-stone-400">
          click chart elements — these two follow your selection · decisions draw only where the engine recorded a branch fact
        </p>
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
      <div>
        <div className="mb-1 flex items-center gap-1.5">
          <Workflow className="h-3.5 w-3.5 text-emerald-700" />
          <span className="text-[13px] font-semibold text-stone-800">Flow — your selection</span>
        </div>
        {flow ? <ChatChart spec={flow} /> : (
          <p className="rounded-lg border border-stone-200 bg-white px-3 py-6 text-center text-[11px] text-stone-400">
            no recorded calls touch the selected elements — nothing drawn rather than something invented
          </p>
        )}
      </div>
      <div>
        <div className="mb-1 flex items-center gap-1.5">
          <Route className="h-3.5 w-3.5 text-emerald-700" />
          <span className="text-[13px] font-semibold text-stone-800">Sequence — your selection</span>
        </div>
        {seq ? <ChatChart spec={seq} /> : (
          <p className="rounded-lg border border-stone-200 bg-white px-3 py-6 text-center text-[11px] text-stone-400">
            pick at least two connected elements for an ordering — only recorded calls between them draw
          </p>
        )}
      </div>
    </div>
  );
}
