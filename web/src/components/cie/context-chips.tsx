"use client";

// RFC-0024 stage 4 — the R5a CONTEXT CHIPS row (ux-spec §3-CHIP, §2.3).
//
// Plural, removable, capped at 24 (the 25th click evicts the oldest and
// says so). A repeat click never duplicates — the existing chip pulses
// once. Chips are the user's, not the persona's (G3): they survive
// persona switches and scope releases; the R4 pair follows them only.
//
// RFC-0023 stage 6 additions kept in the merge: the "Answers use these
// selections" line with the one-tap whole-project release, and the honest
// per-chip code-space tooltip (a chip with no recorded files says it
// contributes none).

import { useEffect, useRef } from "react";
import { X } from "lucide-react";
import { useCie } from "@/lib/cie-store";

const CAP = 24;

export function ContextChips() {
  const chatContext = useCie((s) => s.chatContext);
  const removeChip = useCie((s) => s.removeChip);
  const clearChips = useCie((s) => s.clearChips);
  const chipNotice = useCie((s) => s.chipNotice);

  // pulse the last-added (or re-picked) chip once — key-driven animation
  const lastId = chatContext.length ? chatContext[chatContext.length - 1].id : null;
  const prevLen = useRef(0);
  useEffect(() => {
    if (chatContext.length > prevLen.current && lastId) {
      const el = document.querySelector(`[data-chip="${CSS.escape(lastId)}"]`);
      el?.animate(
        [
          { boxShadow: "0 0 0 0 rgba(16,185,129,0.55)" },
          { boxShadow: "0 0 0 6px rgba(16,185,129,0)" },
        ],
        { duration: 650, easing: "ease-out" },
      );
    }
    prevLen.current = chatContext.length;
  }, [chatContext.length, lastId]);

  if (!chatContext.length) return null;

  return (
    <div className="border-b border-stone-100 px-3 pt-2 pb-1.5">
      <div className="flex items-center gap-1.5 mb-1">
        <p className="text-[10px] font-semibold uppercase tracking-wide text-stone-400">
          context · {chatContext.length}/{CAP}
        </p>
        <button
          type="button"
          onClick={clearChips}
          className="text-[10px] text-stone-400 underline decoration-dotted hover:text-stone-600"
        >
          clear all
        </button>
      </div>
      <p className="text-[10.5px] text-stone-500 mb-1">
        Answers use these selections — everything else is outside the answer&apos;s
        scope.
      </p>
      <div className="flex flex-wrap gap-1">
        {chatContext.map((c) => (
          <span
            key={c.id}
            data-chip={c.id}
            className="inline-flex max-w-full items-center gap-1 rounded-full border border-emerald-200 bg-emerald-50 py-0.5 pl-2 pr-1 text-[10.5px] text-emerald-900"
            title={`${c.kind}${c.files.length ? ` — ${c.files.join(", ")}` : " — no source files recorded; contributes no code space"}${c.sourceTitle ? ` · picked from ${c.sourceTitle}` : ""}`}
          >
            <span className="rounded-full bg-emerald-600/10 px-1 text-[9px] font-semibold uppercase tracking-wide text-emerald-700">
              {c.kind}
            </span>
            <span className="max-w-[180px] truncate">{c.label}</span>
            <button
              type="button"
              onClick={() => removeChip(c.id)}
              aria-label={`Remove ${c.label} from answer context`}
              className="rounded-full p-0.5 text-emerald-700/70 hover:bg-emerald-100 hover:text-emerald-900"
            >
              <X className="h-3 w-3" />
            </button>
          </span>
        ))}
      </div>
      {chipNotice && (
        <p className="mt-1 text-[10px] text-amber-700" role="status">
          {chipNotice}
        </p>
      )}
    </div>
  );
}
