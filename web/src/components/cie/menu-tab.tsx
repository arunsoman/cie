"use client";

// The Menu tab — two honest shortcuts on one page:
//
// 1. One-tap charts: pre-canned questions whose charts are computed straight
//    from the loaded report (no typing, no waiting). Same renderer as the
//    Ask-CIE charts, so the fisheye lens, keyboard parity and engine/layout
//    disclosure carry over.
// 2. What CIE can't do yet: all 33 remaining Planned stories from the
//    engine's own registry, in plain words — what you would get, and why
//    it's not here yet. Honesty as a feature, not a footnote.

import { useMemo, useState } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { TabsContent } from "@/components/ui/tabs";
import {
  Activity,
  BarChart3,
  ChevronDown,
  Gauge,
  ListFilter,
  Map as MapIcon,
  Shapes,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { buildCannedCharts } from "@/lib/canned-charts";
import { ChatChart } from "@/components/cie/chat-chart";
import { NOT_YET, NOT_YET_COUNT, THEMES, REGISTRY_TOTAL, REGISTRY_DELIVERED, REGISTRY_PARTIAL, type ThemeId } from "@/lib/whats-next";

const ICONS = {
  list: ListFilter,
  gauge: Gauge,
  shapes: Shapes,
  shield: ShieldCheck,
  map: MapIcon,
  activity: Activity,
} as const;

const GROUPS = ["Problems", "Promises", "The map", "What the code does"] as const;

export function MenuTab() {
  const { report, viewspec, concepts } = useCie();

  const charts = useMemo(
    () => buildCannedCharts(report, viewspec, concepts),
    [report, viewspec, concepts]
  );

  const [activeId, setActiveId] = useState<string>(charts[0]?.id ?? "");
  const active = charts.find((c) => c.id === activeId) ?? charts[0];

  const [theme, setTheme] = useState<ThemeId | "all">("all");
  const [openId, setOpenId] = useState<string | null>(null);

  const shown = useMemo(
    () => (theme === "all" ? NOT_YET : NOT_YET.filter((s) => s.theme === theme)),
    [theme]
  );
  const themeCount = (t: ThemeId) => NOT_YET.filter((s) => s.theme === t).length;

  if (!report) return null;

  return (
    <TabsContent value="menu" className="space-y-6">
      {/* ---------- One-tap charts ---------- */}
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
          <BarChart3 className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
          <p>
            <span className="font-semibold">Charts with zero typing. </span>
            Tap a question below and the picture appears instantly — worked out
            directly from your loaded report, no waiting. For anything more
            open-ended, the <span className="font-medium">Ask CIE</span> page is still there.
          </p>
        </CardContent>
      </Card>

      <div className="grid gap-4 lg:grid-cols-[minmax(280px,380px)_1fr] items-start">
        {/* question menu */}
        <div className="space-y-4">
          {GROUPS.map((group) => {
            const rows = charts.filter((c) => c.group === group);
            if (!rows.length) return null;
            return (
              <div key={group}>
                <p className="text-xs font-semibold uppercase tracking-wide text-stone-400 mb-2">{group}</p>
                <div className="space-y-1.5">
                  {rows.map((c) => {
                    const Icon = ICONS[c.icon];
                    const isActive = c.id === active?.id;
                    return (
                      <button
                        key={c.id}
                        type="button"
                        onClick={() => setActiveId(c.id)}
                        aria-pressed={isActive}
                        className={`w-full text-left rounded-xl border px-3.5 py-2.5 transition-colors ${
                          isActive
                            ? "border-emerald-300 bg-emerald-50 shadow-sm"
                            : "border-stone-200 bg-white hover:border-emerald-200 hover:bg-emerald-50/40"
                        }`}
                      >
                        <span className="flex items-start gap-2.5">
                          <Icon className={`h-4 w-4 mt-0.5 shrink-0 ${isActive ? "text-emerald-700" : "text-stone-400"}`} />
                          <span>
                            <span className={`block text-sm leading-snug ${isActive ? "font-semibold text-emerald-900" : "font-medium text-stone-800"}`}>
                              {c.question}
                            </span>
                            <span className="block text-xs text-stone-500 mt-0.5">{c.hint}</span>
                          </span>
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </div>

        {/* the chart itself */}
        <Card className="border-stone-200 lg:sticky lg:top-20">
          <CardContent className="p-5">
            {active ? (
              active.spec ? (
                <ChatChart spec={active.spec} />
              ) : (
                <div className="h-[220px] flex items-center justify-center text-sm text-stone-400 bg-stone-50 rounded-lg border border-dashed border-stone-200 px-6 text-center">
                  Not enough data in this project to draw “{active.question}” — CIE won&apos;t guess.
                </div>
              )
            ) : (
              <div className="h-[220px] flex items-center justify-center text-sm text-stone-400 bg-stone-50 rounded-lg border border-dashed border-stone-200">
                Load a project first.
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      {/* ---------- What CIE can't do yet ---------- */}
      <div className="pt-2 border-t border-stone-200" />

      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
          <Sparkles className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
          <div>
            <p>
              <span className="font-semibold">What CIE can&apos;t do yet — all {NOT_YET_COUNT} of them, in the open. </span>
              The engine tracks {REGISTRY_TOTAL} user stories; {REGISTRY_DELIVERED} are delivered and
              one more carries a named, visible gap. These {NOT_YET_COUNT} are the rest — honestly marked
              &ldquo;planned&rdquo; in CIE&apos;s own registry. Nothing here is hidden, and nothing on the other
              pages is pretended.
            </p>
            <p className="mt-1.5 text-stone-500 text-xs">
              Tap any card to see what you would get and why it isn&apos;t here yet.
            </p>
          </div>
        </CardContent>
      </Card>

      {/* theme filter */}
      <div className="flex flex-wrap gap-1.5 -mt-2">
        <button
          type="button"
          onClick={() => setTheme("all")}
          aria-pressed={theme === "all"}
          className={`rounded-full border px-3 py-1.5 text-xs transition-colors ${
            theme === "all"
              ? "border-emerald-300 bg-emerald-50 text-emerald-900 font-semibold"
              : "border-stone-200 bg-white text-stone-600 hover:border-emerald-200"
          }`}
        >
          Everything ({NOT_YET_COUNT})
        </button>
        {THEMES.filter((t) => themeCount(t.id) > 0).map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => setTheme(t.id)}
            aria-pressed={theme === t.id}
            className={`rounded-full border px-3 py-1.5 text-xs transition-colors ${
              theme === t.id
                ? "border-emerald-300 bg-emerald-50 text-emerald-900 font-semibold"
                : "border-stone-200 bg-white text-stone-600 hover:border-emerald-200"
            }`}
          >
            {t.label} ({themeCount(t.id)})
          </button>
        ))}
      </div>

      {/* story cards */}
      <div className="grid gap-2.5 md:grid-cols-2 items-start">
        {shown.map((s) => {
          const open = openId === s.id;
          return (
            <Card
              key={s.id}
              className={`border-stone-200 transition-colors ${open ? "border-emerald-200 md:col-span-2" : "hover:border-emerald-200"}`}
            >
              <button
                type="button"
                onClick={() => setOpenId(open ? null : s.id)}
                aria-expanded={open}
                className="w-full text-left p-4"
              >
                <span className="flex items-start justify-between gap-3">
                  <span className="font-semibold text-stone-900 text-sm leading-snug">{s.title}</span>
                  <span className="flex items-center gap-2 shrink-0">
                    <Badge variant="outline" className="text-[10px] font-mono text-stone-400 border-stone-200">
                      {s.id}
                    </Badge>
                    <ChevronDown className={`h-4 w-4 text-stone-400 transition-transform ${open ? "rotate-180" : ""}`} />
                  </span>
                </span>
                <span className="block text-xs text-stone-400 mt-1">
                  {THEMES.find((t) => t.id === s.theme)?.label}
                </span>
                {open && (
                  <span className="block mt-3 space-y-2.5 text-sm leading-relaxed">
                    <span className="block">
                      <span className="font-semibold text-emerald-800">What you&apos;d get: </span>
                      <span className="text-stone-700">{s.get}</span>
                    </span>
                    <span className="block">
                      <span className="font-semibold text-amber-700">Why it&apos;s not here yet: </span>
                      <span className="text-stone-700">{s.why}</span>
                    </span>
                  </span>
                )}
              </button>
            </Card>
          );
        })}
      </div>
    </TabsContent>
  );
}
