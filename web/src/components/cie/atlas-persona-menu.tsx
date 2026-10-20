"use client";

// RFC-0024 stage 2 — the persona sub-menu (ux-spec §14).
//
// `Atlas ▾` in the top bar, labeled with the ACTIVE persona. Five human
// entries, each with its one-line purpose under the label; check-mark on
// the active; 1–5 shortcuts ONLY while the menu is open (never global
// hotkeys); arrow/Enter/Esc keyboard parity. The agent persona is
// HONESTLY ABSENT (ux-spec §9): an agent has no Atlas screen — it
// consumes these same presets as structured claims over MCP. The menu
// never offers a screen it cannot render, and the footer of the menu
// says so in plain words.

import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Bot } from "lucide-react";
import { PERSONA_ORDER, PERSONA_PRESETS, type PersonaId } from "@/lib/atlas-presets";

export function AtlasPersonaMenu({
  active,
  onSwitch,
}: {
  active: PersonaId;
  onSwitch: (id: PersonaId) => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const preset = PERSONA_PRESETS[active];

  // Click-outside closes; Esc closes; 1–5 shortcuts ONLY while open.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        return;
      }
      const n = Number(e.key);
      if (Number.isInteger(n) && n >= 1 && n <= PERSONA_ORDER.length) {
        e.preventDefault();
        onSwitch(PERSONA_ORDER[n - 1]);
        setOpen(false);
      }
    };
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
    };
  }, [open, onSwitch]);

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-1.5 rounded-md border border-stone-200 bg-white px-2.5 py-1.5 text-xs font-medium text-stone-700 hover:bg-stone-50"
        title="Atlas persona — re-plans the dashboard, no reload"
      >
        Atlas: {preset.label}
        <ChevronDown className="h-3.5 w-3.5 text-stone-400" />
      </button>

      {open && (
        <div
          role="menu"
          aria-label="Atlas personas"
          className="absolute left-0 z-50 mt-1 w-80 rounded-xl border border-stone-200 bg-white p-1.5 shadow-lg"
        >
          {PERSONA_ORDER.map((id, i) => {
            const p = PERSONA_PRESETS[id];
            const isActive = id === active;
            return (
              <button
                key={id}
                role="menuitemradio"
                aria-checked={isActive}
                onClick={() => {
                  onSwitch(id);
                  setOpen(false);
                }}
                className={`flex w-full items-start gap-2.5 rounded-lg px-2.5 py-2 text-left hover:bg-stone-50 ${isActive ? "bg-stone-50" : ""}`}
              >
                <span className="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center">
                  {isActive ? <Check className="h-3.5 w-3.5 text-emerald-600" /> : null}
                </span>
                <span className="min-w-0">
                  <span className="flex items-center gap-1.5 text-xs font-semibold text-stone-800">
                    {p.label}
                    <span className="rounded border border-stone-200 px-1 text-[9px] font-normal text-stone-400">
                      {i + 1}
                    </span>
                  </span>
                  <span className="mt-0.5 block text-[11px] leading-snug text-stone-500">{p.purpose}</span>
                </span>
              </button>
            );
          })}
          <div className="mt-1 flex items-start gap-2 border-t border-stone-100 px-2.5 py-2 text-[10px] leading-snug text-stone-400">
            <Bot className="mt-0.5 h-3 w-3 shrink-0" />
            The AI agent consumes these same presets over MCP — it has no screen here, by design.
          </div>
        </div>
      )}
    </div>
  );
}
