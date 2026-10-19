"use client";

// The Menu view — two honest shortcuts on one page (rendered by the stage):
//
// 1. One-tap charts: pre-canned questions whose charts are computed straight
//    from the loaded report (no typing, no waiting). Same renderer as the
//    Ask-CIE charts, so the fisheye lens, keyboard parity and engine/layout
//    disclosure carry over.
// 2. All 33 remaining registry stories — each shown with REAL data from the
//    sample project: measured by real tools (profilers, coverage, git, the
//    hosting API, an ELF parser, pytest, git itself) or recorded from a real
//    walkthrough (the brief, the sandbox commits, the patch, the approvals).
//    Every card shows the data, where it came from, how to extract the same
//    data from any other repo (extract → transform → load), and what honest
//    work remains. And tapping "Ask CIE about this" starts that conversation
//    in the chat.

import { useMemo, useState } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Activity,
  BarChart3,
  ChevronDown,
  Gauge,
  ListFilter,
  Map as MapIcon,
  MessageCircleQuestion,
  Package,
  Shapes,
  ShieldCheck,
  Sparkles,
  Timer,
} from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { buildCannedCharts } from "@/lib/canned-charts";
import { ChatChart } from "@/components/cie/chat-chart";
import { NOT_YET, NOT_YET_COUNT, THEMES, REGISTRY_TOTAL, REGISTRY_DELIVERED, REGISTRY_PARTIAL, type ThemeId } from "@/lib/whats-next";
import { buildStoryCards, sourceLine, etlLine, type StoryCard as StoryCardT } from "@/lib/stories";

const ICONS = {
  list: ListFilter,
  gauge: Gauge,
  shapes: Shapes,
  shield: ShieldCheck,
  map: MapIcon,
  activity: Activity,
  package: Package,
  timer: Timer,
} as const;

const GROUPS = ["Problems", "Promises", "The map", "What the code does", "Dependencies", "Watching it run", "Health", "Quality metrics"] as const;

export function MenuView() {
  const { report, viewspec, concepts, deps, runtime, stories, quality, setPendingAsk } = useCie();

  const charts = useMemo(
    () => buildCannedCharts(report, viewspec, concepts, deps, runtime, quality),
    [report, viewspec, concepts, deps, runtime, quality]
  );

  const [activeId, setActiveId] = useState<string>(charts[0]?.id ?? "");
  const active = charts.find((c) => c.id === activeId) ?? charts[0];

  const [theme, setTheme] = useState<ThemeId | "all">("all");
  const [openId, setOpenId] = useState<string | null>(null);

  // The 33 story cards: NOT_YET gives theme + plain-words framing; the
  // collected stories.json gives the real demo data (when the project has it).
  const storyCards: (StoryCardT & { theme: ThemeId; get: string })[] = useMemo(() => {
    const collected = buildStoryCards(stories);
    const cards: (StoryCardT & { theme: ThemeId; get: string })[] = [];
    for (const s of NOT_YET) {
      const c = collected.find((x) => x.story.id === s.id);
      if (c) cards.push({ ...c, theme: s.theme, get: s.get });
    }
    return cards;
  }, [stories]);

  const shown = useMemo(
    () => (theme === "all" ? storyCards : storyCards.filter((s) => s.theme === theme)),
    [theme, storyCards]
  );
  const themeCount = (t: ThemeId) => storyCards.filter((s) => s.theme === t).length;
  const hasData = storyCards.length > 0;

  if (!report) return null;

  return (
    <div className="space-y-6">
      {/* ---------- One-tap charts ---------- */}
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
          <BarChart3 className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
          <p>
            <span className="font-semibold">Charts with zero typing. </span>
            Tap a question below and the picture appears instantly — worked out
            directly from your loaded report, no waiting. The <span className="font-medium">Health</span> and{" "}
            <span className="font-medium">Quality metrics</span> groups come from the project&apos;s collected
            quality data (extract → transform → load, formulas shown). Tap any element inside a chart
            to start a conversation about it. For anything more open-ended, the{" "}
            <span className="font-medium">chat on the right</span> is still there.
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

      {/* ---------- All 33, shown with the sample project's real data ---------- */}
      <div className="pt-2 border-t border-stone-200" />

      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
          <Sparkles className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
          <div>
            <p>
              <span className="font-semibold">All {NOT_YET_COUNT} of them — shown, not just promised. </span>
              The engine tracks {REGISTRY_TOTAL} user stories; {REGISTRY_DELIVERED} are delivered and
              one more carries a named, visible gap. These {NOT_YET_COUNT} are the rest — and this
              sample project carries real data for every one of them{" "}
              {stories ? (
                <>
                  ({stories.counts.measured} measured by real tools, {stories.counts.recorded} recorded
                  from a real walkthrough of the workflow).
                </>
              ) : (
                "(this project has no story data collected — the honest “what you'd get” is shown instead)."
              )}{" "}
              Nothing here is hidden, and nothing on the other pages is pretended.
            </p>
            <p className="mt-1.5 text-stone-500 text-xs">
              {hasData
                ? "Tap any card: the data first, then where it came from, how to pull the same data from any other repo, and what work honestly remains."
                : "Tap any card to see what you would get and why it isn't here yet."}
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

      {/* story cards — data first, then the sourcing, the recipe, the remainder */}
      <div className="grid gap-2.5 md:grid-cols-2 items-start">
        {shown.map((card) => {
          const s = card.story;
          const open = openId === s.id;
          const kindTone =
            s.kind === "measured"
              ? "bg-emerald-50 text-emerald-800 border-emerald-200"
              : "bg-sky-50 text-sky-800 border-sky-200";
          return (
            <Card
              key={s.id}
              className={`border-stone-200 transition-colors ${open ? "border-emerald-300 md:col-span-2 shadow-sm" : "hover:border-emerald-200"}`}
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
                    <Badge variant="outline" className={`text-[10px] font-semibold border ${kindTone}`}>
                      {s.kind === "measured" ? "measured" : "recorded"}
                    </Badge>
                    <Badge variant="outline" className="text-[10px] font-mono text-stone-400 border-stone-200">
                      {s.id}
                    </Badge>
                    <ChevronDown className={`h-4 w-4 text-stone-400 transition-transform ${open ? "rotate-180" : ""}`} />
                  </span>
                </span>
                <span className="block text-xs text-stone-400 mt-1">
                  {THEMES.find((t) => t.id === card.theme)?.label}
                  <span className="text-stone-500"> · {sourceLine(s)}</span>
                </span>
                {open && (
                  <span className="block mt-3 text-sm leading-relaxed">
                    <span className="block text-stone-700">{card.get}</span>
                  </span>
                )}
              </button>

              {open && (
                <div className="px-4 pb-4 space-y-4">
                  {/* the data itself */}
                  {card.chart ? (
                    <ChatChart spec={card.chart} />
                  ) : (
                    <div className="h-[160px] flex items-center justify-center text-sm text-stone-400 bg-stone-50 rounded-lg border border-dashed border-stone-200 px-6 text-center">
                      No data collected for this one yet — CIE won&apos;t invent any.
                    </div>
                  )}

                  {/* where the numbers came from */}
                  <div className="rounded-lg border border-stone-200 bg-stone-50/70 p-3 text-xs text-stone-600 space-y-1">
                    <p>
                      <span className="font-semibold text-stone-700">Where these numbers came from: </span>
                      {s.source.label} — {s.source.detail}
                    </p>
                    <p>
                      <span className="font-semibold text-stone-700">From your repo: </span>
                      extract {s.etl.extract}; transform — {s.etl.transform}; load — {s.etl.load}.
                    </p>
                    {s.real_world_equivalents && s.real_world_equivalents.length > 0 && (
                      <p>
                        <span className="font-semibold text-stone-700">Same data on production projects: </span>
                        {s.real_world_equivalents.join(", ")}.
                      </p>
                    )}
                  </div>

                  {/* the honest remainder */}
                  <div className="text-xs text-stone-500 space-y-1">
                    <p>
                      <span className="font-semibold text-amber-700">Still honest work left: </span>
                      {s.gap}
                    </p>
                    <p className="text-stone-400">{s.honesty}</p>
                  </div>

                  {/* click-to-chat */}
                  <button
                    type="button"
                    onClick={() =>
                      setPendingAsk(
                        `Tell me about “${s.title}” (${s.id}). What does the sample data show, how was it collected, and how would this work on my own repo?`
                      )
                    }
                    className="inline-flex items-center gap-2 rounded-full border border-emerald-300 bg-emerald-50 px-3.5 py-1.5 text-xs font-semibold text-emerald-900 hover:bg-emerald-100 transition-colors"
                  >
                    <MessageCircleQuestion className="h-3.5 w-3.5" />
                    Ask CIE about this
                  </button>
                </div>
              )}
            </Card>
          );
        })}
      </div>
    </div>
  );
}
