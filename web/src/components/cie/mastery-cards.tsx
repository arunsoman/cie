"use client";

// The mastery journey's interactive cards, rendered inside "Ask CIE"
// bubbles — and the blast-radius popover for the source panel.
//
// Honesty rules that carry over unchanged everywhere:
//   CONFIRMED (emerald) = the engine fully recorded that symbol
//   INFERRED  (amber)   = concluded from the call graph; the reason says why
//   YOU CONFIRMED (blue)= you checked it yourself — the strongest state and
//                         the only one that arrives through the card below
// Corrections are append-only: the machine's answer is never silently
// rewritten; the correction rides alongside it, in the same card and in
// the notes doc.

import { useEffect, useMemo, useState } from "react";
import { Check, Database, FileCode2, Flag, ListChecks, PartyPopper, Zap } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { Formatted } from "@/components/cie/chat-panel";
import {
  buildSequenceSpec,
  resolveSymbolAt,
  traceFlow,
  type MasteryCardData,
  type MasteryRow,
  type TraceStep,
} from "@/lib/mastery";
import type { ChatChartSpec } from "@/lib/chat-chart";
import type { CieImpact, CieReport } from "@/lib/cie-types";

const cut = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

function rowLoc(r: MasteryRow) {
  return r.file ? { file: r.file, byteStart: r.byteStart, byteEnd: r.byteEnd } : undefined;
}

function StatusTag({ status, why }: { status: "confirmed" | "inferred"; why?: string }) {
  return (
    <span
      className={`shrink-0 rounded-full px-2 py-0.5 text-[9.5px] font-bold tracking-wide ${
        status === "confirmed"
          ? "bg-emerald-100 text-emerald-800 border border-emerald-200"
          : "bg-amber-100 text-amber-800 border border-amber-200"
      }`}
      title={status === "inferred" ? why ?? "concluded from the recorded graph, not individually read" : "the engine fully recorded this"}
    >
      {status === "confirmed" ? "✓ CONFIRMED" : "INFERRED"}
    </span>
  );
}

function CardShell({ title, children, note, icon }: { title: string; children: React.ReactNode; note?: string; icon: React.ReactNode }) {
  return (
    <div className="rounded-xl border border-stone-200 bg-white overflow-hidden">
      <div className="px-3 pt-2.5 pb-2 border-b border-stone-100">
        <p className="text-[13px] font-bold text-stone-900 flex items-center gap-1.5">{icon} {title}</p>
      </div>
      {children}
      {note && (
        <p className="px-3 pb-2 pt-1.5 text-[10.5px] text-stone-400 border-t border-stone-100 flex items-start gap-1.5">
          <Database className="h-3 w-3 mt-px shrink-0" /> {note}
        </p>
      )}
    </div>
  );
}

// ------------------------------------------------------------------ card root

export function MasteryCard({ card, chart }: { card: MasteryCardData; chart?: ChatChartSpec }) {
  if (card.kind === "entrypoints") return <EntrypointsCard card={card} />;
  if (card.kind === "flows") return <FlowsCard card={card} />;
  if (card.kind === "trace") return <TraceCard card={card} chart={chart} />;
  if (card.kind === "fivethings") return <FiveThingsCard card={card} />;
  return null;
}

// ------------------------------------------------------------- entry points

function EntrypointsCard({ card }: { card: MasteryCardData }) {
  const openSourceViewer = useCie((s) => s.openSourceViewer);
  const rows = card.entryRows ?? [];
  return (
    <CardShell title={card.title} note={card.note} icon={<ListChecks className="h-3.5 w-3.5 text-emerald-700" />}>
      {rows.length ? (
        <ol className="divide-y divide-stone-100">
          {rows.map((r) => (
            <li key={r.idx}>
              <button
                type="button"
                onClick={() => rowLoc(r) && openSourceViewer({ file: r.file!, byteStart: r.byteStart, byteEnd: r.byteEnd, elName: r.title, chartTitle: card.title })}
                className="w-full text-left px-3 py-2 hover:bg-emerald-50/60 transition-colors flex items-start gap-2.5"
              >
                <span className="text-[11px] text-stone-300 font-bold mt-0.5 w-4 shrink-0">{r.idx}</span>
                <span className="min-w-0 flex-1">
                  <span className="block text-[12.5px] font-semibold text-stone-900 truncate font-mono">{cut(r.title, 46)}</span>
                  <span className="block text-[11px] text-stone-500">{cut(r.detail, 90)}</span>
                  {r.file && (
                    <span className="mt-0.5 inline-flex items-center gap-1 rounded-md bg-emerald-50 border border-emerald-200 px-1.5 py-px text-[10px] text-emerald-800 font-mono truncate max-w-full">
                      <FileCode2 className="h-2.5 w-2.5 shrink-0" />{cut(r.file, 44)}
                    </span>
                  )}
                </span>
                <StatusTag status={r.status} why={r.why} />
              </button>
            </li>
          ))}
        </ol>
      ) : (
        <p className="px-3 py-3 text-[12px] text-stone-500">Nothing to list — the honest answer sits above the card.</p>
      )}
    </CardShell>
  );
}

// -------------------------------------------------------------------- flows

function FlowsCard({ card }: { card: MasteryCardData }) {
  const setPendingTrace = useCie((s) => s.setPendingTrace);
  return (
    <CardShell title={card.title} note={card.note} icon={<Zap className="h-3.5 w-3.5 text-emerald-700" />}>
      <ol className="divide-y divide-stone-100">
        {(card.entryRows ?? []).map((r) => (
          <li key={r.idx} className="px-3 py-2 flex items-start gap-2.5">
            <span className="text-[11px] text-stone-300 font-bold mt-0.5 w-4 shrink-0">{r.idx}</span>
            <span className="min-w-0 flex-1">
              <span className="block text-[12.5px] font-semibold text-stone-900 truncate font-mono">{cut(r.title, 42)}</span>
              <span className="block text-[11px] text-stone-500">{cut(r.detail, 90)}</span>
            </span>
            <button
              type="button"
              onClick={() => r.entryId != null && setPendingTrace({ entryId: r.entryId, title: r.title })}
              className="shrink-0 h-7 rounded-lg border border-amber-200 bg-amber-50 px-2.5 text-[11px] font-bold text-amber-800 hover:bg-amber-100 transition-colors"
            >
              Trace this one →
            </button>
          </li>
        ))}
      </ol>
    </CardShell>
  );
}

// ------------------------------------------------------------ 5 things card

function FiveThingsCard({ card }: { card: MasteryCardData }) {
  return (
    <CardShell title={card.title} note={card.note} icon={<PartyPopper className="h-3.5 w-3.5 text-emerald-700" />}>
      <ol className="px-3 py-2 space-y-1.5 list-decimal pl-7 marker:text-stone-400 marker:font-bold">
        {(card.items ?? []).map((it, i) => (
          <li key={i} className="text-[12px] text-stone-700"><Formatted text={it} /></li>
        ))}
      </ol>
    </CardShell>
  );
}

// ------------------------------------------------------------- the trace card

export function TraceCard({ card, chart }: { card: MasteryCardData; chart?: ChatChartSpec }) {
  const mastery = useCie((s) => s.mastery);
  const updateMastery = useCie((s) => s.updateMastery);
  const [corrOpen, setCorrOpen] = useState(false);
  const [corrText, setCorrText] = useState("");
  const steps = card.steps ?? [];
  const traceKey = card.traceKey ?? "";
  const record = mastery?.traces[traceKey];
  const youSet = useMemo(() => new Set(Object.keys(record?.steps ?? {}).map(Number)), [record]);
  const allConfirmed = !!record?.confirmedAt;

  // a trace that was RUN is a real journey event even before anyone verified
  // it — the record exists the moment the card exists ("0 traced" would lie)
  useEffect(() => {
    if (!mastery || !mastery.traces) return; // loaded after mount
    if (mastery.traces[traceKey]) return;
    updateMastery((m) => ({
      ...m,
      traces: {
        ...m.traces,
        [traceKey]: { title: card.title, entryId: card.entryId ?? 0, entryQual: card.entryQual ?? "", at: Date.now(), steps: {} },
      },
    }));
  }, [mastery, traceKey]);

  // the sequence diagram always rebuilds with the CURRENT verification state —
  // the canvas copy is a snapshot; this button opens a fresh one
  const openSequence = () => {
    const spec = buildSequenceSpec(`Sequence — ${card.entryQual ?? card.title}`, steps, youSet) ?? chart;
    if (spec) useCie.getState().openChartOnCanvas(spec);
  };

  const confirmAll = () => {
    const all: Record<number, 1> = {};
    for (const st of steps) all[st.idx] = 1;
    updateMastery((m) => ({
      ...m,
      traces: {
        ...m.traces,
        [traceKey]: {
          title: card.title,
          entryId: card.entryId!,
          entryQual: card.entryQual!,
          at: m.traces[traceKey]?.at ?? Date.now(),
          confirmedAt: Date.now(),
          steps: all,
        },
      },
      prompts: { ...m.prompts, trace: Date.now() },
    }));
  };

  const saveCorrection = () => {
    const text = corrText.trim();
    if (!text) return;
    updateMastery((m) => ({
      ...m,
      corrections: [...m.corrections, { at: Date.now(), text, trace: card.entryQual }],
    }));
    setCorrText("");
    setCorrOpen(false);
  };

  const correctionsHere = (mastery?.corrections ?? []).filter((c) => c.trace === card.entryQual);

  return (
    <CardShell title={card.title} note={card.note} icon={<Zap className="h-3.5 w-3.5 text-emerald-700" />}>
      <ol className="divide-y divide-stone-100">
        {steps.map((st) => (
          <li key={st.idx} className="px-3 py-2 flex items-start gap-2.5">
            <span className="text-[11px] text-stone-300 font-bold mt-0.5 w-4 shrink-0">{st.idx}</span>
            <span className="min-w-0 flex-1">
              <span className="block text-[12.5px] font-semibold text-stone-900 truncate font-mono">{cut(st.qualName, 44)}</span>
              <span className="block text-[11px] text-stone-600">
                {readsWritesLine(st)}
                {st.exceptions.length ? <span className="text-rose-700 font-semibold"> · ⚠ {cut(st.exceptions[0], 40)}</span> : ""}
              </span>
              <button
                type="button"
                onClick={() => st.file && useCie.getState().openSourceViewer({ file: st.file, byteStart: st.byteStart, byteEnd: st.byteEnd, elName: st.name, chartTitle: card.title })}
                className="mt-0.5 inline-flex items-center gap-1 rounded-md bg-emerald-50 border border-emerald-200 px-1.5 py-px text-[10px] text-emerald-800 font-mono truncate max-w-full hover:bg-emerald-100"
              >
                <FileCode2 className="h-2.5 w-2.5 shrink-0" />{cut(st.file, 42)}
              </button>
            </span>
            {youSet.has(st.idx) || allConfirmed ? (
              <span className="shrink-0 rounded-full px-2 py-0.5 text-[9.5px] font-bold tracking-wide bg-sky-100 text-sky-800 border border-sky-200">YOU CONFIRMED</span>
            ) : (
              <StatusTag status={traceStatusEngine(st)} why={traceStatusWhy(st)} />
            )}
          </li>
        ))}
      </ol>

      {/* the verification loop — the one thing a plain LLM chat doesn't have */}
      <div className={`m-3 rounded-lg border p-2.5 ${allConfirmed ? "border-emerald-200 bg-emerald-50" : "border-amber-200 bg-amber-50"}`}>
        <p className={`text-[11.5px] font-bold flex items-center gap-1.5 ${allConfirmed ? "text-emerald-900" : "text-amber-800"}`}>
          {allConfirmed ? "✅ Verified — you ran this flow yourself" : "🖐️ Your turn — verify it with your own debugger"}
        </p>
        <p className={`text-[11px] mt-0.5 ${allConfirmed ? "text-emerald-800" : "text-amber-800/90"}`}>
          {allConfirmed
            ? "Every drawn step carries your own check now. Mastery notes and the sequence diagram show the dashes; your corrections below stay logged."
            : `Set a breakpoint in ${card.entryQual ?? "the first step"}, run this flow once for real, then confirm — inferred steps are CIE's conclusions until you check them.`}
        </p>
        {!allConfirmed && (
          <div className="mt-2 flex items-center gap-2 flex-wrap">
            <button
              type="button"
              onClick={confirmAll}
              className="h-8 px-2.5 rounded-lg bg-emerald-700 text-white text-[11.5px] font-bold hover:bg-emerald-800 inline-flex items-center gap-1"
            >
              <Check className="h-3 w-3" /> I confirmed all of it
            </button>
            <button
              type="button"
              onClick={() => setCorrOpen((v) => !v)}
              className="h-8 px-2.5 rounded-lg border border-amber-300 bg-white text-[11.5px] font-bold text-amber-800 hover:bg-amber-50 inline-flex items-center gap-1"
            >
              <Flag className="h-3 w-3" /> Something&apos;s wrong…
            </button>
          </div>
        )}
        {allConfirmed && (
          <button
            type="button"
            onClick={() => setCorrOpen((v) => !v)}
            className="mt-1 text-[10.5px] text-amber-700 hover:text-amber-900 font-medium"
          >
            + report a discrepancy (stays logged even after verifying)
          </button>
        )}
        {corrOpen && (
          <div className="mt-2 flex items-center gap-1.5">
            <input
              value={corrText}
              onChange={(e) => setCorrText(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && saveCorrection()}
              placeholder="e.g. step 5 — the write happens before the check, not after"
              className="flex-1 h-8 rounded-lg border border-amber-200 bg-white px-2.5 text-[12px] placeholder:text-stone-400 focus:outline-none focus:ring-2 focus:ring-amber-600/20"
            />
            <button type="button" onClick={saveCorrection} className="h-8 px-2.5 rounded-lg bg-amber-600 text-white text-[11.5px] font-bold hover:bg-amber-700 shrink-0">
              Log it
            </button>
          </div>
        )}
        {correctionsHere.length > 0 && (
          <div className={`mt-2 rounded-lg bg-white border border-dashed px-2.5 py-2 space-y-1 ${allConfirmed ? "border-emerald-200" : "border-amber-200"}`}>
            {correctionsHere.map((c, i) => (
              <p key={i} className={`text-[11px] flex gap-1.5 ${allConfirmed ? "text-emerald-900" : "text-amber-900"}`}>
                <span className="shrink-0 font-bold">✎</span>
                <span>{cut(c.text, 160)}</span>
              </p>
            ))}
            <p className="text-[9.5px] text-stone-400">CIE never silently rewrites its answer — the correction rides alongside it.</p>
          </div>
        )}
      </div>

      <div className="px-3 pb-2.5 flex items-center gap-3 flex-wrap">
        {chart && (
          <button type="button" onClick={openSequence} className="text-[11px] font-semibold text-emerald-700 hover:text-emerald-900 underline">
            ↗ Sequence diagram → canvas
          </button>
        )}
        <button type="button" onClick={() => useCie.getState().setView("mastery")} className="text-[11px] font-semibold text-emerald-700 hover:text-emerald-900 underline">
          → Master view
        </button>
      </div>
    </CardShell>
  );
}

// status helpers local to the card
function traceStatusEngine(st: TraceStep): "confirmed" | "inferred" {
  return st.complete && !st.noSummary ? "confirmed" : "inferred";
}
function traceStatusWhy(st: TraceStep): string {
  return st.noSummary ? "no behaviour summary was recorded for this symbol" : "the summary for this symbol is incomplete";
}
function readsWritesLine(st: TraceStep): string {
  const bits: string[] = [];
  if (st.reads.length) bits.push(`reads ${st.reads[0]}`);
  if (st.writes.length) bits.push(`writes ${st.writes[0]}`);
  if (!bits.length && st.effects.length) bits.push(`touches: ${st.effects.join(", ")}`);
  if (!bits.length && !st.noSummary) bits.push("local computation only");
  if (st.noSummary) bits.push("no records kept");
  return bits.join(" · ");
}

// =====================================================================
// Blast radius popover — lives in the source panel's footer (and any
// other symbol header). Impact facts:
//   • recorded callers + downstream reach — from the loaded report (every
//     project, instant)
//   • full engine impact (cie impact: min test set, affected concepts) —
//     live call, only when the project's files are still on disk; its
//     state is named, not silently left blank
//   • history coupling — real cochange partners of the file
// =====================================================================

export function BlastRadiusPopover({
  onClose,
  symbolName,
  file,
  byteStart,
  style,
}: {
  onClose: () => void;
  symbolName?: string;
  file?: string;
  byteStart?: number;
  style?: React.CSSProperties;
}) {
  const report = useCie((s) => s.report);
  const cochange = useCie((s) => s.cochange);
  const sourceRoot = useCie((s) => s.sourceRoot);
  const updateMastery = useCie((s) => s.updateMastery);

  // memoised: resolveSymbolAt builds a fresh wrapper object each call, and
  // sym rides in the fetch effect's deps — recreation here caused the
  // "too many re-renders" loop the error boundary once caught
  const sym = useMemo(
    () => resolveSymbolAt(report, symbolName, file, byteStart),
    [report, symbolName, file, byteStart]
  );

  const [engine, setEngine] = useState<CieImpact | null>(null);
  const [engineState, setEngineState] = useState<{ key: string; status: "loading" | "ok" | "fail" }>({ key: "", status: "loading" });
  const engineKey = sourceRoot && sym ? `${sourceRoot}|${sym.qual_name}` : null;

  // the officially blessed render-phase adjustment: a NEW symbol resets its
  // own fetch state (never the previous symbol's answershown for this one).
  // NB: engineKey is null when there's no symbol — compare empty-string forms
  // or a null symbol loops forever here.
  const nextKey = engineKey ?? "";
  if (nextKey !== engineState.key) {
    setEngine(null);
    setEngineState({ key: nextKey, status: "loading" });
  }
  const busyKey = engineState.key;

  useEffect(() => {
    if (!engineKey || !sourceRoot || !sym || engineKey !== busyKey) return;
    let dead = false;
    const run = async () => {
      try {
        const res = await fetch("/api/impact", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ sourceRoot, symbol: sym!.qual_name || sym!.name }),
        });
        const d = (await res.json().catch(() => ({}))) as Partial<CieImpact>;
        if (!dead) {
          if (res.ok && d.symbol) {
            setEngine(d as CieImpact);
            setEngineState({ key: busyKey, status: "ok" });
          } else setEngineState({ key: busyKey, status: "fail" });
        }
      } catch {
        if (!dead) setEngineState({ key: busyKey, status: "fail" });
      }
    };
    void run();
    return () => {
      dead = true;
    };
  }, [engineKey, busyKey, sourceRoot, sym]);

  const callers = useMemo(() => {
    if (!report || !sym) return 0;
    const id = Number(sym.id);
    return report.summaries.filter((s) => (s.callees ?? []).some((c) => Number(c) === id)).length;
  }, [report, sym]);

  const downstream = useMemo(() => {
    if (!report || !sym) return null;
    const r = traceFlow(report, Number(sym.id));
    return r ? { count: r.steps.length + r.truncated, effects: r.effectsSeen } : null;
  }, [report, sym]);

  const fileHere = sym && report ? report.files[sym.loc.file] : undefined;
  const partners = useMemo(() => {
    if (!fileHere || !cochange?.edges?.length) return [];
    return cochange.edges.filter((e) => e.a === fileHere || e.b === fileHere).slice(0, 3);
  }, [fileHere, cochange]);

  if (!sym) {
    return (
      <div style={style} className="w-[min(430px,86vw)] max-h-[80vh] overflow-y-auto rounded-xl border border-stone-200 bg-white shadow-2xl p-3.5">
        <p className="text-[12px] text-stone-600">That element doesn&apos;t map to a named thing in the loaded report — impact needs a real symbol, and CIE won&apos;t fake one.</p>
        <button type="button" onClick={onClose} className="mt-2 text-[11px] text-emerald-700 underline">close</button>
      </div>
    );
  }

  const entry = sym.qual_name || sym.name;
  return (
    <div style={style} className="w-[min(460px,88vw)] max-h-[80vh] overflow-y-auto rounded-xl border border-stone-200 bg-white shadow-2xl p-3.5 space-y-2.5" role="dialog" aria-label={`Blast radius of ${entry}`}>
      <div className="flex items-start justify-between gap-2">
        <p className="text-[12.5px] font-bold text-stone-900 flex items-center gap-1.5">
          <Zap className="h-3.5 w-3.5 text-emerald-700 shrink-0" /> Blast radius — <span className="font-mono text-[11px] font-semibold">{cut(entry, 32)}</span>
        </p>
        <button type="button" onClick={onClose} aria-label="Close" className="h-6 w-6 grid place-items-center rounded-lg text-stone-400 hover:bg-stone-100 text-sm shrink-0">✕</button>
      </div>

      <p className="text-[12px] text-stone-700">
        <b className="text-stone-900">Recorded callers:</b>{" "}
        {callers > 0 ? (
          <>
            <span className="font-semibold text-emerald-800">{callers}</span> recorded call site(s) in this report
          </>
        ) : (
          <span>none recorded — this IS a start point (the journey&apos;s entry-points card says the same)</span>
        )}
      </p>

      {downstream && (
        <p className="text-[12px] text-stone-700">
          <b className="text-stone-900">What changing it would touch:</b>{" "}
          the walk from here reaches <span className="font-semibold text-emerald-800">{downstream.count}</span> named thing(s)
          {downstream.effects.length ? <> · outside touches: <span className="font-mono">{cut(downstream.effects.join(", "), 60)}</span></> : ""}
        </p>
      )}

      {sourceRoot ? (
        engine && engineState.status === "ok" ? (
          <p className="text-[12px] text-stone-700">
            <b className="text-stone-900">Full engine impact</b> (<span className="font-mono">cie impact</span>, run just now):{" "}
            {engine.affected_symbols.length} symbol(s) downstream.
            {engine.minimum_test_set.length ? (
              <> Minimum tests to run: <span className="font-mono">{cut(engine.minimum_test_set.join(", "), 70)}</span>.</>
            ) : (
              " CIE found no tests covering this path — a change here has no safety net."
            )}
            {engine.affected_concepts.length ? <> Reaches {engine.affected_concepts.length} named part(s) of the system.</> : null}
          </p>
        ) : (
          <p className="text-[12px] text-stone-500">
            {engineState.status === "loading" ? "Running the real engine impact walk right now…" : engineState.status === "fail" ? "The engine call failed just now — the recorded-graph facts above still stand." : ""}
          </p>
        )
      ) : (
        <p className="text-[12px] text-stone-500">
          The full <span className="font-mono">cie impact</span> walk needs this project&apos;s files still on disk — for uploads and cloned links only the loaded facts above apply. Connect the folder itself and this goes deeper.
        </p>
      )}

      {partners.length > 0 && (
        <p className="text-[12px] text-stone-700">
          <b className="text-stone-900">Changes together with:</b>{" "}
          {partners.map((p) => `${p.a === fileHere ? p.b : p.a} (${p.support}×)`).join(", ")} {" "}
          <span className="text-stone-400">— real git history, not a guess</span>
        </p>
      )}

      <div className="flex items-center gap-2 flex-wrap pt-1.5 border-t border-stone-100">
        <button
          type="button"
          onClick={() => useCie.getState().setPendingTrace({ entryId: Number(sym.id), title: entry })}
          className="h-7 rounded-lg border border-amber-200 bg-amber-50 text-[11px] font-bold text-amber-800 px-2.5 hover:bg-amber-100"
        >
          🛤 Trace from here
        </button>
        <button
          type="button"
          onClick={() => { updateMastery((m) => ({ ...m, blastChecks: m.blastChecks + 1, prompts: { ...m.prompts, blast: Date.now() } })); onClose(); }}
          className="h-7 rounded-lg bg-emerald-700 text-white text-[11px] font-bold px-2.5 hover:bg-emerald-800 inline-flex items-center gap-1"
        >
          <Check className="h-3 w-3" /> Log this check
        </button>
      </div>
      <p className="text-[10px] text-stone-400">Logged checks show in the journey&apos;s Ongoing station (station 5). Nothing goes anywhere — this panel never writes.</p>
    </div>
  );
}

/** Blast-radius button + its popover, ready to drop into a symbol header —
 *  the source panel's footer carries it (the Ongoing station's habit). */
export function BlastRadiusButton(props: { symbolName?: string; file?: string; byteStart?: number; className?: string }) {
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);
  // fixed-position anchor: the button often lives inside panels whose overflow
  // clips absolute-positioned popovers — measure and lift it out (clamped)
  const toggle = () => {
    setOpen((v) => {
      if (!v) {
        const r = document.activeElement instanceof HTMLElement ? document.activeElement.getBoundingClientRect() : null;
        setAnchor(r ? { x: Math.min(r.left, window.innerWidth - 470), y: r.bottom + 6 } : { x: window.innerWidth / 2 - 220, y: 120 });
      }
      return !v;
    });
  };
  return (
    <span className={`relative inline-flex ${props.className ?? ""}`}>
      <button
        type="button"
        onClick={toggle}
        aria-expanded={open}
        title="Before you change this: recorded callers, what it reaches, history coupling, and — when your files are on disk — the live engine walk with the minimum tests"
        className="h-8 rounded-xl border border-amber-200 bg-amber-50 px-2.5 text-xs font-bold text-amber-800 hover:bg-amber-100 inline-flex items-center gap-1.5"
      >
        <Zap className="h-3.5 w-3.5" /> Blast radius
      </button>
      {open && anchor && (
        <>
          <span className="fixed inset-0 z-[60] cursor-default" aria-hidden onClick={() => setOpen(false)} />
          <BlastRadiusPopover
            onClose={() => setOpen(false)}
            symbolName={props.symbolName}
            file={props.file}
            byteStart={props.byteStart}
            style={{ position: "fixed" as const, left: anchor.x, top: Math.min(anchor.y, window.innerHeight - 340), zIndex: 70 }}
          />
        </>
      )}
    </span>
  );
}