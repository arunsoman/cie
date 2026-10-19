"use client";

// Mastery notes — the living document the whole journey writes into.
//
// Sections are PROVENANCE-TYPED, and the typing is enforced, not decorative:
//   ● auto       — derived fresh from the loaded engine exports every render;
//                  an edit turns it into a human section (stored verbatim,
//                  never regenerated over)
//   ◐ human      — yours (checkpoint text and edits); CIE may suggest a
//                  draft, never claims it, never overwrites it
//   ✎ correction — append-only log; the honest record of machine-said vs
//                  human-verified
// Exports (ARCHITECTURE.md / NOTES.md) assemble from these same sections in
// the browser — nothing is uploaded, nothing written to the user's project.

import { useEffect, useMemo, useState } from "react";
import { Download, FileText, Pencil, RotateCcw, X } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { Formatted } from "@/components/cie/chat-panel";
import {
  buildSequenceSpec,
  exportArchitecture,
  exportNotes,
  downloadTextFile,
  noteSections,
  traceFlow,
  type MasteryNoteCtx,
  type NoteSection,
} from "@/lib/mastery";

const KIND_BADGE: Record<NoteSection["kind"], { label: string; cls: string; title: string }> = {
  auto: {
    label: "● auto",
    cls: "bg-stone-100 text-stone-600 border-stone-200",
    title: "Derived from the engine exports — regenerable; editing it makes it yours (kept verbatim from then on)",
  },
  human: { label: "◐ you", cls: "bg-sky-100 text-sky-800 border-sky-200", title: "Written by you — CIE never rewrites it" },
  correction: { label: "✎ corrections", cls: "bg-amber-100 text-amber-800 border-amber-200", title: "Append-only — never silently rewritten, never deleted" },
};

export function MasteryNotesView() {
  const source = useCie((s) => s.source);
  const label = useCie((s) => s.label);
  const loadMastery = useCie((s) => s.loadMastery);
  const report = useCie((s) => s.report);
  const concepts = useCie((s) => s.concepts);
  const deps = useCie((s) => s.deps);
  const cochange = useCie((s) => s.cochange);
  const overlap = useCie((s) => s.overlap);
  const drift = useCie((s) => s.drift);
  const mastery = useCie((s) => s.mastery);
  const updateMastery = useCie((s) => s.updateMastery);
  const openChartOnCanvas = useCie((s) => s.openChartOnCanvas);
  const setView = useCie((s) => s.setView);
  const [editingId, setEditingId] = useState<string | null>(null);

  useEffect(() => {
    loadMastery();
  }, [source, label, loadMastery]);

  const sections = useMemo(() => {
    if (!mastery || !report) return [];
    return noteSections({ state: mastery, report, concepts, deps, cochange, overlap, drift, label });
  }, [mastery, report, concepts, deps, cochange, overlap, drift, label]);

  if (!mastery) {
    return <div className="py-16 text-center text-sm text-stone-400" role="status">Opening your notes…</div>;
  }

  const doExport = (name: "ARCHITECTURE.md" | "NOTES.md") => {
    if (!report) return;
    const ctx: MasteryNoteCtx = { state: mastery, report, concepts, deps, cochange, overlap, drift, label };
    downloadTextFile(name, name === "ARCHITECTURE.md" ? exportArchitecture(ctx) : exportNotes(ctx));
    updateMastery((m) => ({ ...m, exports: [...m.exports, { at: Date.now(), name }] }));
  };

  /** trace sections rebuild their sequence on demand — from the stored trace
   *  record + the loaded report, with the CURRENT verification state */
  const openSequenceFor = (sectionId: string) => {
    const t = mastery.traces[sectionId.slice("trace:".length)];
    if (!t || !report) return;
    const res = traceFlow(report, t.entryId);
    if (!res) return;
    const spec = buildSequenceSpec(`Sequence — ${t.title}`, res.steps, new Set(Object.keys(t.steps).map(Number)));
    if (spec) openChartOnCanvas(spec);
  };

  const saveEdit = (id: string, text: string) => {
    const body = text.trim();
    updateMastery((m) => ({
      ...m,
      humanEdits: body ? { ...m.humanEdits, [id]: body } : omit(m.humanEdits, id),
      // the checkpoint textarea and the notes section are one and the same thing
      yourWords: id === "your-words" ? body : m.yourWords,
    }));
    setEditingId(null);
  };

  const discardEdit = (id: string) => {
    updateMastery((m) => ({ ...m, humanEdits: omit(m.humanEdits, id) }));
    setEditingId(null);
  };

  return (
    <div className="max-w-3xl mx-auto space-y-4" aria-label="Mastery notes">
      <div className="flex items-start gap-3 flex-wrap">
        <div className="flex-1 min-w-0">
          <h2 className="text-xl font-bold text-stone-900 flex items-center gap-2">
            <span className="h-7 w-7 rounded-lg bg-stone-900 text-white grid place-items-center shrink-0"><FileText className="h-4 w-4" /></span>
            Mastery notes
          </h2>
          <p className="text-xs text-stone-500 mt-1">
            <span className="truncate">{label}</span> · <span className="font-semibold text-stone-600">● auto</span> derived & regenerable · <span className="font-semibold text-sky-700">◐ you</span> kept verbatim · <span className="font-semibold text-amber-700">✎ corrections</span> append-only
          </p>
        </div>
        {report && (
          <span className="flex items-center gap-1.5 flex-wrap shrink-0">
            <button type="button" onClick={() => doExport("ARCHITECTURE.md")} className="h-8 px-2.5 rounded-lg bg-stone-900 text-white text-xs font-bold hover:bg-stone-700 inline-flex items-center gap-1.5">
              <Download className="h-3.5 w-3.5" /> ARCHITECTURE.md
            </button>
            <button type="button" onClick={() => doExport("NOTES.md")} className="h-8 px-2.5 rounded-lg border border-stone-300 text-xs font-bold text-stone-700 hover:bg-stone-100 inline-flex items-center gap-1.5">
              <Download className="h-3.5 w-3.5" /> NOTES.md
            </button>
          </span>
        )}
      </div>

      {sections.length === 0 && (
        <div className="rounded-xl border border-amber-200 bg-amber-50 p-4 text-[13px] text-amber-900">
          {report
            ? "The notes fill as the journey runs — start with the Phase-1 chips."
            : "Nothing yet. Load a project (Use your own), and the journey fills these sections from the engine's read of it."}
          <button type="button" onClick={() => setView("mastery")} className="ml-1 underline font-semibold">Open the journey →</button>
        </div>
      )}

      {sections.map((s) => {
        const badge = KIND_BADGE[s.kind];
        return (
          <section key={s.id} className="rounded-xl border border-stone-200 bg-white overflow-hidden" aria-label={s.title}>
            <div className="flex items-start gap-2 px-3 py-2 border-b border-stone-100 flex-wrap">
              <span className={`rounded-full px-2 py-0.5 text-[9.5px] font-bold tracking-wide border shrink-0 ${badge.cls}`} title={badge.title}>
                {badge.label}
              </span>
              <p className="text-[12.5px] font-bold text-stone-900 flex-1 min-w-0">{s.title}</p>
              <span className="flex items-center gap-1 shrink-0">
                {s.id.startsWith("trace:") && !s.edited && (
                  <button type="button" onClick={() => openSequenceFor(s.id)} className="rounded-lg px-2 py-1 text-[10.5px] font-bold text-emerald-700 hover:bg-emerald-50">
                    open sequence
                  </button>
                )}
                {s.edited && s.id !== "corrections" && (
                  <button
                    type="button"
                    onClick={() => discardEdit(s.id)}
                    title="Drop your edit and let CIE regenerate this section — your edit is erased on purpose, never silently done"
                    className="rounded-lg px-2 py-1 text-[10.5px] font-bold text-stone-600 hover:bg-stone-100 inline-flex items-center gap-1"
                  >
                    <RotateCcw className="h-3 w-3" /> regenerate
                  </button>
                )}
                {s.id !== "corrections" && (
                  <button
                    type="button"
                    onClick={() => setEditingId(s.id)}
                    title={s.kind === "auto" && !s.edited ? "Edit — your version replaces the derived text and stays yours" : "Edit your words"}
                    className="rounded-lg px-2 py-1 text-[10.5px] font-bold text-stone-600 hover:bg-stone-100 inline-flex items-center gap-1"
                  >
                    <Pencil className="h-3 w-3" /> edit
                  </button>
                )}
              </span>
            </div>

            {editingId === s.id ? (
              <div className="p-2.5">
                <textarea
                  autoFocus
                  defaultValue={s.bodyMarkdown}
                  id={`notes-edit-${s.id}`}
                  className="w-full min-h-36 rounded-lg border border-stone-200 bg-stone-50 p-2.5 text-[12.5px] leading-relaxed text-stone-800 focus:outline-none focus:ring-2 focus:ring-emerald-600/25"
                  placeholder="Write it the way you'd tell a colleague…"
                />
                <div className="mt-2 flex items-center gap-2 flex-wrap">
                  <button
                    type="button"
                    onClick={() => {
                      const el = document.getElementById(`notes-edit-${s.id}`) as HTMLTextAreaElement | null;
                      saveEdit(s.id, el?.value ?? "");
                    }}
                    className="h-8 px-3 rounded-lg bg-emerald-700 text-white text-xs font-bold hover:bg-emerald-800"
                  >
                    Save
                  </button>
                  <button type="button" onClick={() => setEditingId(null)} className="h-8 px-3 rounded-lg border border-stone-300 text-xs text-stone-600 hover:bg-stone-100 inline-flex items-center gap-1">
                    <X className="h-3 w-3" /> Cancel
                  </button>
                  {s.kind === "auto" && !s.edited && (
                    <span className="text-[10.5px] text-stone-400">Saving turns this ● auto section into ◐ yours — CIE won&apos;t regenerate over it.</span>
                  )}
                </div>
              </div>
            ) : (
              <div className="px-3 py-2.5 text-[12.5px] leading-relaxed text-stone-700"><Formatted text={s.bodyMarkdown || "*(nothing here yet)*"} /></div>
            )}

            {s.kind === "correction" && (
              <div className="px-3 pb-2 text-[10px] text-stone-400">Corrections are only ever added — never deleted, never re-worded by CIE.</div>
            )}
          </section>
        );
      })}

      <p className="text-[10.5px] text-stone-400">
        Everything here lives in this browser only (localStorage, keyed to this project). Nothing is written back to the project folder — that&apos;s the read-only promise.
      </p>
    </div>
  );
}

function omit(obj: Record<string, string>, key: string): Record<string, string> {
  const { [key]: _drop, ...rest } = obj;
  return rest;
}