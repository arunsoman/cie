"use client";

// "Master this codebase" — the 5-station guided journey over the loaded
// project. This view owns nothing:
//   • every Phase-1 chip is a prompt the chat answers INSTANTLY from the
//     loaded engine exports (fast paths in lib/mastery.ts — no AI round-trip)
//   • every Phase-3 button opens the REAL views/charts — the journey forks
//     nothing, it links
//   • progress lives per project in localStorage (lib/mastery.ts) and never
//     leaves the machine; export is an explicit button
// The chat stays the engine of the journey — this page is the map of it.

import { useEffect, useMemo, useState } from "react";
import { CalendarClock, Check, Clipboard, Download, FileText, GitMerge, Layers, Map as MapIcon, PartyPopper, PenLine, RotateCcw, Sparkles, Target, Waves, Zap } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import {
  exportArchitecture,
  exportNotes,
  downloadTextFile,
  summaryAnswer,
  traceFlow,
} from "@/lib/mastery";
import { buildCochangeSpec, buildOverlapSpec } from "@/lib/quality";
import { computeRisk } from "@/lib/mastery";
import { Formatted } from "@/components/cie/chat-panel";

// ---- the four Phase-1 prompts, each answered instantly by the chat ----

const PHASE1_PROMPTS = [
  { id: "summary", icon: "🗺️", title: "What is this system?", question: "What is this system? Summarize it in plain words.", detail: "Instant, from the engine facts" },
  { id: "entry", icon: "🚪", title: "Where does it start from?", question: "Where does it start from?", detail: "Entry points with the real file:line — click a row, see the code" },
  { id: "c4", icon: "📦", title: "Draw the big picture", question: "Draw the big picture — C4 containers of the parts.", detail: "Containers + calls across them → the canvas" },
  { id: "flows", icon: "🛤️", title: "The top flows", question: "Show the top flow candidates.", detail: "Ranked by how much code each start reaches — pick one, trace it" },
] as const;

export function MasteryView() {
  const source = useCie((s) => s.source);
  const label = useCie((s) => s.label);
  const loadMastery = useCie((s) => s.loadMastery);
  const report = useCie((s) => s.report);
  const mastery = useCie((s) => s.mastery);
  const updateMastery = useCie((s) => s.updateMastery);
  const resetMastery = useCie((s) => s.resetMastery);
  const setPendingAsk = useCie((s) => s.setPendingAsk);
  const sourceRoot = useCie((s) => s.sourceRoot);
  const concepts = useCie((s) => s.concepts);
  const deps = useCie((s) => s.deps);
  const cochange = useCie((s) => s.cochange);
  const overlap = useCie((s) => s.overlap);
  const drift = useCie((s) => s.drift);
  const setView = useCie((s) => s.setView);
  const openChartOnCanvas = useCie((s) => s.openChartOnCanvas);
  const [confirmReset, setConfirmReset] = useState(false);
  const [yourDraft, setYourDraft] = useState<string | null>(null);

  // (re)load whenever the loaded project changes — one effect, local, explicit
  useEffect(() => {
    loadMastery();
  }, [source, label, loadMastery]);

  // the CIE side of the checkpoint — derived fresh, never stored as if it were
  const cieWords = useMemo(
    () => (report ? summaryAnswer({ report, concepts, deps, cochange, overlap, drift, label }) : ""),
    [report, concepts, deps, cochange, overlap, drift, label]
  );

  if (!mastery) {
    return <div className="py-16 text-center text-sm text-stone-400" role="status">Opening your journey…</div>;
  }

  const done = (id: string) => mastery.prompts[id] != null;
  const whenDone = (id: string) => (mastery.prompts[id] ? new Date(mastery.prompts[id]).toLocaleDateString() : "");

  // ---- station progress, always honest counts, nothing inflated ----
  const p1 = PHASE1_PROMPTS.filter((p) => done(p.id)).length;
  const traceEntries = Object.entries(mastery.traces);
  let youVerified = 0;
  let totalSteps = 0;
  for (const [, t] of traceEntries) {
    const r = traceFlow(report, t.entryId);
    totalSteps += r?.steps.length ?? 0;
    youVerified += Object.keys(t.steps).length;
  }
  const s2frac = totalSteps > 0 ? Math.min(1, youVerified / totalSteps) : 0;
  const s3 = (["cochange", "overlap", "deps"] as const).filter((k) => done(k)).length;
  const s4 = mastery.exports.length > 0;

  const ask = (question: string, id: string) => {
    setPendingAsk(question);
    if (!done(id)) updateMastery((m) => ({ ...m, prompts: { ...m.prompts, [id]: Date.now() } }));
  };

  const markSeen = (id: string) => {
    if (!done(id)) updateMastery((m) => ({ ...m, prompts: { ...m.prompts, [id]: Date.now() } }));
  };

  const saveCheckpoint = () => {
    updateMastery((m) => ({ ...m, yourWords: yourDraft ?? m.yourWords, prompts: { ...m.prompts, checkpoint: Date.now() } }));
    setYourDraft(null);
  };

  const doExport = (name: "ARCHITECTURE.md" | "NOTES.md") => {
    if (!report) return;
    const ctx = { state: mastery, report, concepts, deps, cochange, overlap, drift, label };
    const text = name === "ARCHITECTURE.md" ? exportArchitecture(ctx) : exportNotes(ctx);
    downloadTextFile(name, text);
    updateMastery((m) => ({ ...m, exports: [...m.exports, { at: Date.now(), name }], prompts: { ...m.prompts, export: Date.now() } }));
  };

  const openCochange = () => {
    if (!cochange?.edges?.length) return;
    const spec = buildCochangeSpec(cochange);
    if (spec) openChartOnCanvas(spec);
    markSeen("cochange");
  };
  const openOverlap = () => {
    if (!overlap?.pairs?.length) return;
    const spec = buildOverlapSpec(overlap);
    if (spec) openChartOnCanvas(spec);
    markSeen("overlap");
  };

  return (
    <div className="max-w-3xl mx-auto space-y-5" aria-label="Master this codebase journey">
      {/* ---- header ---- */}
      <div className="flex items-start gap-3 flex-wrap">
        <div className="flex-1 min-w-0">
          <h2 className="text-xl font-bold text-stone-900 flex items-center gap-2">
            <span className="h-7 w-7 rounded-lg bg-emerald-700 text-white grid place-items-center shrink-0"><MapIcon className="h-4 w-4" /></span>
            Master this codebase
          </h2>
          <p className="text-xs text-stone-500 mt-1 truncate">
            {label}{report ? ` · ${report.files.length} files · ${report.symbols.length} named things` : ""} — CIE guides, you verify, both remember.
          </p>
          {Object.keys(mastery.prompts).length > 0 && (
            <p className="text-[11px] text-stone-400 mt-1 flex items-center gap-1">
              <CalendarClock className="h-3 w-3 shrink-0" />
              Your journey so far: {Object.keys(mastery.prompts).length} step(s) logged, {traceEntries.length} flow(s) traced, {mastery.corrections.length} correction(s) — picked up right where you left them.
            </p>
          )}
        </div>
        {confirmReset ? (
          <span className="flex items-center gap-1.5 flex-wrap">
            <span className="text-[11px] text-stone-500">Erase journey progress for this project?</span>
            <button className="h-8 px-2.5 rounded-lg bg-rose-600 text-white text-xs font-bold hover:bg-rose-700" onClick={resetMastery}>Yes, reset</button>
            <button className="h-8 px-2.5 rounded-lg border border-stone-300 text-xs text-stone-600 hover:bg-stone-100" onClick={() => setConfirmReset(false)}>No</button>
          </span>
        ) : (
          <button className="h-8 px-2.5 rounded-lg border border-stone-300 text-xs text-stone-600 hover:bg-stone-100 inline-flex items-center gap-1.5 shrink-0" onClick={() => setConfirmReset(true)}>
            <RotateCcw className="h-3.5 w-3.5" /> Reset journey
          </button>
        )}
      </div>

      {!report && (
        <div className="rounded-xl border border-amber-200 bg-amber-50 p-4 text-[13px] text-amber-900 leading-relaxed">
          No project is loaded yet. <b>Use your own</b> (a repo link with its branch, a folder on this computer, or the engine&apos;s own JSON files) — the journey turns the engine&apos;s read of your code into the five stations below. The sample project also works, loaded by default.
        </div>
      )}

      {/* ---- the station rail ---- */}
      <ol className="rounded-2xl border border-stone-200 bg-white divide-y divide-stone-100 overflow-hidden">
        <Station n={1} title="Fast orientation" frac={report ? p1 / 4 : 0} label={report ? `${p1} of 4 prompts run` : "—"} />
        <Station n={2} title="Deep dive" frac={s2frac} label={traceEntries.length ? `${traceEntries.length} flow(s) traced · ${youVerified}/${totalSteps} steps you-confirmed` : "no flows traced yet"} />
        <Station n={3} title="Structure & risk" frac={report ? Math.min(1, s3 / 3) : 0} label={report ? `${s3} of 3 views opened` : "—"} />
        <Station n={4} title="Lock it in" frac={s4 ? 1 : 0} label={s4 ? "exported — the doc exists" : "not exported yet"} />
        <Station n={5} title="Ongoing" frac={Math.min(1, mastery.blastChecks / 4)} label={mastery.blastChecks > 0 ? `${mastery.blastChecks} blast-radius check(s) logged` : "the habit part"} last />
      </ol>

      {/* ---- Phase 1 ---- */}
      <Phase n={1} title="Fast orientation" sub="What it is, where it starts, how it looks whole.">
        <div className={`grid sm:grid-cols-2 gap-2 p-3 ${report ? "" : "opacity-50 pointer-events-none"}`}>
          {PHASE1_PROMPTS.map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => ask(p.question, p.id)}
              title={done(p.id) ? `Run again · first done ${whenDone(p.id)}` : p.detail}
              className={`text-left rounded-xl border p-3 transition-colors ${done(p.id) ? "border-emerald-300 bg-emerald-50/50" : "border-stone-200 bg-white hover:border-emerald-300 hover:bg-emerald-50/40"}`}
            >
              <span className="flex items-center gap-2">
                <span aria-hidden>{p.icon}</span>
                <span className="font-semibold text-[13px] text-stone-900 flex-1">{p.title}</span>
                {done(p.id) && <Check className="h-4 w-4 text-emerald-700 shrink-0" aria-label={`done${whenDone(p.id) ? ` ${whenDone(p.id)}` : ""}`} />}
              </span>
              <span className="block text-[11px] text-stone-500 mt-1">{p.detail}</span>
            </button>
          ))}
        </div>
        {/* the checkpoint — your words first, CIE's after */}
        <div className="mx-3 mb-3 rounded-xl border border-dashed border-emerald-300 bg-emerald-50/40 p-3">
          <p className="text-[13px] font-bold text-stone-900 flex items-center gap-1.5"><Clipboard className="h-3.5 w-3.5 text-emerald-700 shrink-0" /> The 2-minute checkpoint — YOUR words first</p>
          <p className="text-[11px] text-stone-500 mt-0.5">
            Explain this project to a colleague, in your own words. CIE&apos;s version stays hidden until yours exists — then you compare, and both versions live in the notes.
          </p>
          <textarea
            value={yourDraft ?? mastery.yourWords}
            onChange={(e) => setYourDraft(e.target.value)}
            placeholder="In your own words — what does this project do? Who is it for? What would break first?"
            className="mt-2 w-full min-h-20 rounded-lg border border-stone-200 bg-white p-2.5 text-[13px] text-stone-800 placeholder:text-stone-400 focus:outline-none focus:ring-2 focus:ring-emerald-600/25"
          />
          <div className="mt-2 flex items-center gap-2 flex-wrap">
            <button type="button" onClick={saveCheckpoint} className="h-8 px-3 rounded-lg bg-stone-900 text-white text-xs font-bold hover:bg-stone-700 inline-flex items-center gap-1.5">
              <PenLine className="h-3.5 w-3.5" /> {mastery.yourWords ? "Update my words" : "Save my words"}
            </button>
            {mastery.yourWords && <span className="text-[11px] text-stone-500 inline-flex items-center gap-1"><Check className="h-3 w-3 text-emerald-700" /> saved {whenDone("checkpoint")} — CIE&apos;s side revealed:</span>}
          </div>
          {mastery.yourWords && cieWords && (
            <div className="mt-2 grid sm:grid-cols-2 gap-2">
              <div className="rounded-lg bg-white border border-stone-200 p-2.5">
                <p className="text-[10px] font-bold tracking-wide text-stone-500">YOUR WORDS</p>
                <p className="text-[12px] text-stone-700 mt-1 whitespace-pre-wrap">{mastery.yourWords}</p>
              </div>
              <div className="rounded-lg bg-white border border-stone-200 p-2.5">
                <p className="text-[10px] font-bold tracking-wide text-emerald-800">CIE&apos;S ANSWER — from the engine facts</p>
                <div className="text-[12px] text-stone-700 mt-1"><Formatted text={cieWords} /></div>
              </div>
            </div>
          )}
        </div>
      </Phase>

      {/* ---- Phase 2 ---- */}
      <Phase n={2} title="Deep dive into the critical flows" sub="Trace them step by step — then verify with your own debugger.">
        <div className={`p-3 space-y-2.5 ${report ? "" : "opacity-50 pointer-events-none"}`}>
          <div className="flex items-center gap-2 flex-wrap">
            <button
              type="button"
              onClick={() => ask("Show the top flow candidates.", "flows")}
              className="h-8 px-3 rounded-lg bg-emerald-700 text-white text-xs font-bold hover:bg-emerald-800 inline-flex items-center gap-1.5"
            >
              <Target className="h-3.5 w-3.5" /> Get the flow candidates
            </button>
            <span className="text-[11px] text-stone-500">The chat shows ranked candidates — pick one, and its trace card opens with the verify loop.</span>
          </div>
          {traceEntries.map(([key, t]) => {
            const res = traceFlow(report, t.entryId);
            const total = res?.steps.length ?? 0;
            const you = Object.keys(t.steps).length;
            return (
              <div key={key} className="rounded-xl border border-stone-200 bg-white p-2.5 flex items-center gap-2 flex-wrap">
                <span className="text-[12.5px] font-semibold text-stone-900 font-mono truncate max-w-64">{t.title}</span>
                <span className={`rounded-full px-2 py-0.5 text-[10px] font-bold shrink-0 ${t.confirmedAt || (total > 0 && you >= total) ? "bg-sky-100 text-sky-800 border border-sky-200" : "bg-amber-100 text-amber-800 border border-amber-200"}`}>
                  {t.confirmedAt || (total > 0 && you >= total) ? "✅ YOU CONFIRMED" : `${you}/${total} you-confirmed`}
                </span>
                <button type="button" onClick={() => setView("notes")} className="text-[11px] text-emerald-700 hover:underline font-semibold ml-auto">in the notes →</button>
              </div>
            );
          })}
        </div>
      </Phase>

      {/* ---- Phase 3 — links, not forks ---- */}
      <Phase n={3} title="Structure — and where the risk lives" sub="The journey links to the real views; nothing is re-implemented.">
        <div className={`grid sm:grid-cols-3 gap-2 p-3 ${report ? "" : "opacity-50 pointer-events-none"}`}>
          <ViewChip icon={<Layers className="h-4 w-4" />} title="Fan-in / fan-out" detail="The tightest couplings — who pulls what" onClick={() => { setView("deps"); markSeen("deps"); }} />
          <ViewChip icon={<GitMerge className="h-4 w-4" />} title="What changes together" detail={cochange?.edges?.length ? `${cochange.edges.length} pair(s) mined from ${cochange.sample_commits} commit(s)` : "needs git history — a shallow clone honestly has almost none"} onClick={openCochange} disabled={!cochange?.edges?.length} />
          <ViewChip icon={<Waves className="h-4 w-4" />} title="Seams & overlap" detail={overlap?.pairs?.length ? `${overlap.pairs.length} overlapping pair(s) — where a port wants extracting` : "needs the concepts export"} onClick={openOverlap} disabled={!overlap?.pairs?.length} />
        </div>
        {report && (
          <div className="mx-3 mb-3 rounded-xl border border-stone-200 bg-white overflow-visible relative">
            <p className="px-3 pt-2.5 pb-1 text-[13px] font-bold text-stone-900 flex items-center gap-1.5"><Zap className="h-3.5 w-3.5 text-emerald-700 shrink-0" /> Where the risk lives — the roll-up</p>
            <p className="px-3 text-[10.5px] text-stone-400 pb-2">
              co-change times in history × calls going out × problems listed — every column real, sources: cochange.json + report.json
              {cochange && cochange.sample_commits <= 1 ? " · history is thin here (shallow clone or fresh repo) — the co-change column is honestly “—”" : ""}
            </p>
            <table className="w-full text-[12px]">
              <thead className="text-[10px] text-stone-400 text-left">
                <tr>
                  <th className="py-1.5 px-3 font-bold">File (worst first)</th>
                  <th className="py-1.5 px-3 font-bold">co-changes</th>
                  <th className="py-1.5 px-3 font-bold">calls out</th>
                  <th className="py-1.5 px-3 font-bold">problems</th>
                </tr>
              </thead>
              <tbody>
                {computeRisk(report, cochange).slice(0, 5).map((r) => (
                  <tr key={r.file} className="border-t border-stone-100 hover:bg-emerald-50/50">
                    <td className="py-1.5 px-3 font-mono text-[11px] truncate max-w-[24rem]" title={r.file}>{r.file}</td>
                    <td className="py-1.5 px-3">{r.cochangeTimes || "—"}</td>
                    <td className="py-1.5 px-3">{r.callsOut}</td>
                    <td className="py-1.5 px-3">{r.findings}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className="px-3 pb-2.5 pt-1">
              <button type="button" onClick={() => markSeen("risk")} className="text-[11px] font-semibold text-emerald-700 hover:text-emerald-900 underline inline-flex items-center gap-1">
                <Sparkles className="h-3 w-3" /> count this table as seen for station 3
              </button>
            </div>
          </div>
        )}
      </Phase>

      {/* ---- Phase 4 ---- */}
      <Phase n={4} title="Lock it in" sub="Turn the journey into the doc the next hire actually reads.">
        <div className={`p-3 space-y-2 ${report ? "" : "opacity-50 pointer-events-none"}`}>
          <div className="flex items-center gap-2 flex-wrap">
            <button
              type="button"
              onClick={() => ask("Write the 5 things a new developer should know.", "fivethings")}
              className="h-8 px-3 rounded-lg border border-stone-300 bg-white text-xs font-bold text-stone-700 hover:bg-stone-100 inline-flex items-center gap-1.5"
            >
              <Sparkles className="h-3.5 w-3.5 text-emerald-700" /> Draft “5 things a new dev must know”
            </button>
            <button type="button" onClick={() => doExport("ARCHITECTURE.md")} className="h-8 px-3 rounded-lg bg-stone-900 text-white text-xs font-bold hover:bg-stone-700 inline-flex items-center gap-1.5">
              <Download className="h-3.5 w-3.5" /> Export ARCHITECTURE.md
            </button>
            <button type="button" onClick={() => doExport("NOTES.md")} className="h-8 px-3 rounded-lg border border-stone-300 bg-white text-xs font-bold text-stone-700 hover:bg-stone-100 inline-flex items-center gap-1.5">
              <FileText className="h-3.5 w-3.5" /> Export NOTES.md
            </button>
          </div>
          <p className="text-[11px] text-stone-500 flex items-start gap-1.5">
            <PartyPopper className="h-3 w-3 mt-px shrink-0" />
            ARCHITECTURE.md = the assembled doc (facts + your words + traced flows + corrections, diagrams as Mermaid). NOTES.md = the whole living document. Both are assembled in your browser from this journey only — nothing is uploaded. {done("fivethings") && "The “5 things” draft is pinned to the notes."}
          </p>
        </div>
      </Phase>

      {/* ---- Phase 5 ---- */}
      <Phase n={5} title="Ongoing" sub="The habit: check the blast radius before you change, trace when confused, feed the corrections.">
        <div className="p-3 space-y-2 text-[12.5px] text-stone-700">
          <p>1. Open any code the panel shows (or a trace row) and press <b>⚡ Blast radius</b> — recorded callers, the downstream walk, history coupling, and — when the folder itself is connected {sourceRoot ? "(yours is)" : "(a connected local folder makes this part work)"} — the live engine walk with the minimum tests to run.</p>
          <p>2. Ask the chat to <b>trace</b> anything. Every step cites its own evidence and stays marked inferred until you check it.</p>
          <p>3. What you correct is logged, not smoothed over — the notes stay honest about machine-said vs human-verified.</p>
          <p className="flex items-center gap-2 flex-wrap">
            {mastery.blastChecks > 0 ? (
              <span className="rounded-full bg-emerald-100 text-emerald-800 border border-emerald-200 px-2 py-0.5 text-[10.5px] font-bold">{mastery.blastChecks} blast-radius check(s) logged</span>
            ) : (
              <span className="text-[11px] text-stone-400">no blast-radius checks logged yet</span>
            )}
            <button type="button" onClick={() => setView("notes")} className="text-[11px] text-emerald-700 hover:underline font-semibold inline-flex items-center gap-1">
              <FileText className="h-3 w-3" /> → Mastery notes
            </button>
          </p>
        </div>
      </Phase>

      <p className="text-[10.5px] text-stone-400 px-1">
        Progress, your words, traces and corrections live only in this browser&apos;s localStorage (keyed per project), never sent anywhere. Export is the only way out — a button you press.
      </p>
    </div>
  );
}

// ---------- local bits ----------

function Phase({ n, title, sub, children, tag }: { n: number; title: string; sub: string; children: React.ReactNode; tag?: string }) {
  return (
    <section className="rounded-2xl border border-stone-200 bg-white overflow-hidden" aria-label={`Phase ${n}: ${title}`}>
      <div className="flex items-center gap-2.5 px-3.5 py-2.5 border-b border-stone-100 flex-wrap">
        <span className="h-6 w-6 rounded-full bg-stone-900 text-white text-[11px] font-bold grid place-items-center shrink-0">{n}</span>
        <h3 className="text-[14.5px] font-bold text-stone-900">{title}</h3>
        <span className="text-[11.5px] text-stone-500 hidden md:inline">{sub}</span>
        {tag && <span className="ml-auto rounded-full bg-amber-100 border border-amber-200 text-amber-800 px-2 py-0.5 text-[10px] font-bold">{tag}</span>}
      </div>
      {children}
    </section>
  );
}

function Station({ n, title, frac, label, last }: { n: number; title: string; frac: number; label: string; last?: boolean }) {
  const pct = Math.round(Math.max(0, Math.min(1, frac)) * 100);
  const complete = pct >= 99.5;
  return (
    <li className="flex items-center gap-3 px-3.5 py-2">
      <span className={`h-6 w-6 rounded-full text-[11px] font-bold grid place-items-center shrink-0 ${complete ? "bg-emerald-700 text-white" : pct > 0 ? "bg-emerald-100 text-emerald-800" : "bg-stone-100 text-stone-400"}`}>
        {complete ? <Check className="h-3.5 w-3.5" /> : n}
      </span>
      <span className="font-semibold text-[13px] text-stone-900 w-36 shrink-0 truncate">{title}</span>
      <span className="text-[11.5px] text-stone-500 flex-1 truncate">{label}</span>
      <span className="hidden sm:block h-1.5 rounded-full bg-stone-100 w-28 overflow-hidden shrink-0" aria-hidden>
        <span className="block h-full rounded-full bg-emerald-600" style={{ width: `${pct}%` }} />
      </span>
      {!last && <span className="text-stone-200 hidden sm:block" aria-hidden>·</span>}
    </li>
  );
}

function ViewChip({ icon, title, detail, onClick, disabled }: { icon: React.ReactNode; title: string; detail: string; onClick: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="text-left rounded-xl border border-stone-200 bg-white p-3 transition-colors hover:border-emerald-300 hover:bg-emerald-50/40 disabled:opacity-50 disabled:pointer-events-none"
    >
      <span className="text-stone-500">{icon}</span>
      <span className="block text-[12.5px] font-bold text-stone-900 mt-1">{title}</span>
      <span className="block text-[11px] text-stone-500 mt-0.5">{detail}</span>
    </button>
  );
}
