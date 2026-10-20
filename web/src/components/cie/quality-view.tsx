"use client";

// The "Quality metrics" view — the eight classic measures, each shown with
// its real collected value, the formula it comes from, and exactly how the
// collector computed it on this project (extract → transform → load, so the
// same data can be pulled from any repo). Every card hands its question to
// Ask CIE. An absent metric renders an honest empty state, never a guess.

import { FindingProvenance } from "./rfc0020-evidence";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Gauge, MessageCircleQuestion, HeartPulse } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { ChatChart } from "@/components/cie/chat-chart";
import type { QualityMetricRow } from "@/lib/cie-types";

const METRIC_ICONS: Record<string, string> = {
  "defect-density": "defects found per thousand lines of code",
  "mttf-mtbf": "hours, on average, between two defect events",
  "test-coverage": "how much of the code the tests really touch",
  cyclomatic: "how many paths a reader must hold in mind",
  "arrival-rate": "how quickly defects show up over time",
  "code-churn": "how much code is being rewritten",
  "crash-rate": "how often real sessions end badly",
  mttr: "how fast a broken thing gets fixed",
};

function MetricCard({ m, onAsk }: { m: QualityMetricRow; onAsk: (m: QualityMetricRow) => void }) {
  const absent = m.value === null || m.value === undefined;
  return (
    <Card className="border-stone-200 flex flex-col" data-metric={m.id}>
      <CardHeader className="pb-1">
        <div className="flex items-start justify-between gap-2">
          <CardTitle className="text-base">{m.name}</CardTitle>
          <Badge variant="outline" className="text-[10px] font-mono text-stone-400 border-stone-200 max-w-[150px] truncate">
            {m.scope}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="pt-1 flex-1 flex flex-col">
        {absent ? (
          <div className="h-[92px] flex items-center justify-center text-sm text-stone-400 bg-stone-50 rounded-lg border border-dashed border-stone-200 px-4 text-center">
            Not computable from this project&apos;s collected data — CIE won&apos;t invent a number.
          </div>
        ) : (
          <div className="rounded-xl bg-stone-50 border border-stone-200 px-4 py-3 mb-3">
            <p className="text-xl font-bold text-stone-900 leading-snug">{m.value_text}</p>
            <p className="text-xs text-stone-500 mt-1">{METRIC_ICONS[m.id] ?? m.unit}</p>
          </div>
        )}
        <dl className="text-xs text-stone-600 space-y-1.5 flex-1">
          <div>
            <dt className="inline font-semibold text-stone-700">What it measures: </dt>
            <dd className="inline">{m.what_it_measures}.</dd>
          </div>
          <div>
            <dt className="inline font-semibold text-stone-700">Typical calculation: </dt>
            <dd className="inline">{m.typical_calculation}.</dd>
          </div>
          <div>
            <dt className="inline font-semibold text-stone-700">How it was computed here: </dt>
            <dd className="inline">{m.how_computed_here}</dd>
          </div>
          <div className="text-stone-400">
            <dt className="inline font-medium">Source: </dt>
            <dd className="inline">{m.source}</dd>
          </div>
        </dl>
        <button
          type="button"
          onClick={() => onAsk(m)}
          className="mt-3 self-start inline-flex items-center gap-2 rounded-full border border-emerald-300 bg-emerald-50 px-3.5 py-1.5 text-xs font-semibold text-emerald-900 hover:bg-emerald-100 transition-colors"
        >
          <MessageCircleQuestion className="h-3.5 w-3.5" />
          Ask CIE about this
        </button>
      </CardContent>
    </Card>
  );
}

export function QualityView() {
  const { report, quality, setPendingAsk, setView } = useCie();
  if (!quality) {
    return (
      <><FindingProvenance report={report}/><Card className="border-stone-200">
        <CardContent className="p-6 text-sm text-stone-600">
          <p className="font-semibold text-stone-900 flex items-center gap-2">
            <HeartPulse className="h-4 w-4 text-emerald-700" /> No quality data collected for this project.
          </p>
          <p className="mt-2 leading-relaxed">
            The quality measures load from <span className="font-mono text-xs bg-stone-100 rounded px-1">quality.json</span> —
            the export of the extract → transform → load collector. The sample project ships with it; your own
            project gets it by running the collector (see the Menu page, every card names its recipe). Nothing here
            is guessed.
          </p>
        </CardContent>
      </Card></>
    );
  }

  const askMetric = (m: QualityMetricRow) =>
    setPendingAsk(`Tell me about the "${m.name}" quality metric. What does the collected value "${m.value_text}" say, how was it computed, and what are the limits?`);

  return (
    <div className="space-y-5">
      <FindingProvenance report={report}/>
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
          <Gauge className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
          <div>
            <p>
              <span className="font-semibold">Eight classic quality measures, computed for real. </span>
              Each card shows the collected value, the formula it comes from, and how the collector produced it —
              so the same numbers can be pulled from any repository. The defect tracker was queried live and is
              empty; that fact is recorded, and the history-based measures say so next to their numbers.
            </p>
            <p className="mt-1.5 text-stone-500 text-xs">
              Collected {quality.collected_at.slice(0, 10)} · want the pictures? The{" "}
              <button type="button" className="underline text-emerald-800 hover:text-emerald-900" onClick={() => setView("menu")}>
                one-tap chart menu
              </button>{" "}
              has a Health group and a Quality metrics group; every chart element there answers to a tap.
            </p>
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-3 items-start">
        {quality.metrics.map((m) => (
          <MetricCard key={m.id} m={m} onAsk={askMetric} />
        ))}
      </div>

      {/* the two live probes, shown with their own data */}
      <div className="grid gap-4 lg:grid-cols-2 items-start">
        <Card className="border-stone-200">
          <CardHeader className="pb-1">
            <CardTitle className="text-base">The crash probe behind the rate</CardTitle>
            <p className="text-xs text-stone-500">{quality.crash_probe.tool}</p>
          </CardHeader>
          <CardContent className="pt-1">
            <ChatChart
              spec={{
                kind: "table",
                engine: "grid",
                layout: "scroll-rows",
                title: undefined,
                tell: quality.crash_probe.definition,
                columns: ["Entry point", "Input", "What happened"],
                rows: quality.crash_probe.crashes_detail.slice(0, 8).map((c) => [`${c.module}.${c.function}`, c.session.slice(0, 24), (c.reason || "timeout (hang)").slice(0, 40)]),
                source: "quality.json — crash probe",
              }}
            />
          </CardContent>
        </Card>
        <Card className="border-stone-200">
          <CardHeader className="pb-1">
            <CardTitle className="text-base">The repair loops behind MTTR</CardTitle>
            <p className="text-xs text-stone-500">{quality.repair_probe.tool}</p>
          </CardHeader>
          <CardContent className="pt-1">
            <ChatChart
              spec={{
                kind: "table",
                engine: "grid",
                layout: "scroll-rows",
                title: undefined,
                tell: "Three real fail → fix → recheck cycles, executed and timed during collection.",
                columns: ["Bug", "Detect (s)", "Repair (s)", "Total (s)"],
                rows: quality.repair_probe.iterations.map((i) => [i.bug.slice(0, 30), String(i.detect_s), String(i.repair_s), String(i.total_s)]),
                source: "quality.json — repair probe",
              }}
            />
          </CardContent>
        </Card>
      </div>

      {/* how to collect the same data on any repo — the ETL recipes */}
      <Card className="border-stone-200">
        <CardHeader className="pb-1">
          <CardTitle className="text-base">How to collect the same data on your project</CardTitle>
          <p className="text-xs text-stone-500">Extract → transform → load, per dataset. Same collector shape, your repo.</p>
        </CardHeader>
        <CardContent className="pt-1">
          <div className="grid gap-2.5 md:grid-cols-2">
            {quality.how_to_collect_on_your_project.map((r) => (
              <div key={r.dataset} className="rounded-lg border border-stone-200 bg-stone-50/70 p-3 text-xs">
                <p className="font-semibold text-stone-800">{r.dataset}</p>
                <p className="text-stone-600 mt-1">
                  <span className="font-medium text-stone-700">Here: </span>
                  {r.this_demo}
                </p>
                <p className="text-stone-600">
                  <span className="font-medium text-stone-700">On real projects: </span>
                  {r.on_real_projects}
                </p>
              </div>
            ))}
          </div>
          <p className="mt-3 text-[11px] text-stone-400 leading-relaxed">{quality.honesty}</p>
        </CardContent>
      </Card>
    </div>
  );
}
