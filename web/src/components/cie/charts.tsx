"use client";

// The charting engine: turns the loaded CIE data into simple, friendly charts.
// Rendered by the stage's "Numbers" view (see stage.tsx).
// Rules: every chart gets a plain-words title, a one-line "what this tells you"
// caption, and honest empty states. No technical words on the axes.

import { useMemo } from "react";
import {
  Bar,
  BarChart,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { BarChart3 } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { findingTitle, severityPlain, effectPlain, certaintyPlain, authorityPlain } from "@/lib/plain-words";
import type { CieReport, CieViewSpec, Finding } from "@/lib/cie-types";

// ---- palette (matches the rest of the dashboard; never colour-alone: charts
// always carry text labels too) ----
const C = {
  rose: "#e11d48",
  roseSoft: "#fb7185",
  amber: "#d97706",
  emerald: "#059669",
  stone: "#78716c",
  stoneLight: "#a8a29e",
};

const TOOLTIP_STYLE = {
  borderRadius: 10,
  border: "1px solid #e7e5e4",
  boxShadow: "0 4px 16px rgba(0,0,0,0.06)",
  fontSize: 12.5,
  padding: "8px 12px",
  background: "#ffffff",
} as const;

function truncate(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

// ---- shared bits ----

function ChartCard({
  title,
  tell,
  children,
}: {
  title: string;
  tell: string;
  children: React.ReactNode;
}) {
  return (
    <Card className="border-stone-200">
      <CardHeader className="pb-1">
        <CardTitle className="text-base">{title}</CardTitle>
        <CardDescription>{tell}</CardDescription>
      </CardHeader>
      <CardContent className="pt-2">{children}</CardContent>
    </Card>
  );
}

function EmptyChart() {
  return (
    <div className="h-[230px] flex items-center justify-center text-sm text-stone-400 bg-stone-50 rounded-lg border border-dashed border-stone-200">
      Not enough data to draw this yet.
    </div>
  );
}

// Horizontal bars with plain-word labels on the left.
function HBar({
  data,
  height = 230,
  labelWidth = 168,
}: {
  data: { name: string; count: number; color: string; full: string }[];
  height?: number;
  labelWidth?: number;
}) {
  if (!data.length) return <EmptyChart />;
  return (
    <div style={{ height }}>
      <ResponsiveContainer width="100%" height="100%">
        <BarChart data={data} layout="vertical" margin={{ top: 4, right: 28, bottom: 0, left: 0 }}>
          <XAxis type="number" allowDecimals={false} tick={{ fontSize: 11, fill: "#78716c" }} axisLine={false} tickLine={false} />
          <YAxis
            type="category"
            dataKey="name"
            width={labelWidth}
            tick={{ fontSize: 12, fill: "#44403c" }}
            axisLine={false}
            tickLine={false}
          />
          <Tooltip
            cursor={{ fill: "rgba(0,0,0,0.04)" }}
            contentStyle={TOOLTIP_STYLE}
            formatter={(value: number | string) => [`${value}`, "Count"]}
            labelFormatter={(_, payload) => payload?.[0]?.payload?.full ?? ""}
          />
          <Bar dataKey="count" radius={[0, 5, 5, 0]} barSize={16}>
            {data.map((d, i) => (
              <Cell key={i} fill={d.color} />
            ))}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

// Round pie with a centre label, for two- or three-slice stories.
function Donut({
  data,
  centerTop,
  centerBottom,
}: {
  data: { name: string; value: number; color: string }[];
  centerTop: string;
  centerBottom: string;
}) {
  const total = data.reduce((a, d) => a + d.value, 0);
  if (!total) return <EmptyChart />;
  return (
    <div className="flex flex-col sm:flex-row items-center gap-2 sm:gap-6">
      <div className="relative h-[190px] w-[190px] shrink-0">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={data}
              dataKey="value"
              nameKey="name"
              innerRadius={58}
              outerRadius={86}
              paddingAngle={2}
              strokeWidth={2}
              stroke="#ffffff"
            >
              {data.map((d, i) => (
                <Cell key={i} fill={d.color} />
              ))}
            </Pie>
            <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(value: number | string, name) => [`${value}`, String(name)]} />
          </PieChart>
        </ResponsiveContainer>
        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
          <span className="text-2xl font-bold text-stone-900">{centerTop}</span>
          <span className="text-[11px] text-stone-500 text-center px-6 leading-tight">{centerBottom}</span>
        </div>
      </div>
      <ul className="space-y-1.5 text-sm w-full">
        {data.map((d) => (
          <li key={d.name} className="flex items-center gap-2">
            <span className="h-2.5 w-2.5 rounded-full shrink-0" style={{ background: d.color }} aria-hidden />
            <span className="text-stone-700 flex-1">{d.name}</span>
            <span className="font-semibold text-stone-900 tabular-nums">{d.value}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

// ---- data shaping ----

const SEV_ORDER = ["Critical", "High", "Medium", "Low"] as const;

function severityData(report: CieReport) {
  return SEV_ORDER.map((sev) => {
    const count = report.findings.filter((f) => f.severity === sev).length;
    const p = severityPlain(sev);
    return {
      name: p.label,
      full: `${p.label} (${sev}) — ${p.hint}`,
      count,
      color: sev === "Critical" ? C.rose : sev === "High" ? C.roseSoft : sev === "Medium" ? C.amber : C.stoneLight,
    };
  });
}

function kindData(report: CieReport) {
  const byKind = new Map<string, number>();
  for (const f of report.findings) byKind.set(f.kind, (byKind.get(f.kind) ?? 0) + 1);
  const fake = (kind: string) => ({ kind, id: "", severity: "Low", symbol: 0, symbol_name: "", message: "", evidence_lines: [], remediation: "" } as Finding);
  return [...byKind.entries()]
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([kind, count]) => ({
      name: truncate(findingTitle(fake(kind)), 24),
      full: findingTitle(fake(kind)),
      count,
      color: C.emerald,
    }));
}

function fileData(report: CieReport) {
  const byFile = new Map<number, number>();
  for (const f of report.findings) {
    const sym = report.symbols.find((s) => s.id === f.symbol);
    if (sym) byFile.set(sym.loc.file, (byFile.get(sym.loc.file) ?? 0) + 1);
  }
  return [...byFile.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([idx, count]) => {
      const path = report.files[idx] ?? `File ${idx}`;
      const base = path.split("/").pop() ?? path;
      return { name: truncate(base, 22), full: path, count, color: C.amber };
    });
}

function effectData(report: CieReport) {
  const byEffect = new Map<string, number>();
  for (const s of report.summaries) for (const e of s.effects ?? []) byEffect.set(e, (byEffect.get(e) ?? 0) + 1);
  return [...byEffect.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([e, count]) => ({
      name: truncate(effectPlain(e), 26),
      full: effectPlain(e),
      count,
      color: C.stone,
    }));
}

function promiseData(report: CieReport) {
  const written = report.contracts.filter((c) => c.authority === "Explicit").length;
  const guessed = report.contracts.filter((c) => c.authority !== "Explicit").length;
  return [
    { name: authorityPlain("Explicit").label, value: written, color: C.emerald },
    { name: authorityPlain("InferredCandidate").label, value: guessed, color: C.amber },
  ];
}

function certaintyData(viewspec: CieViewSpec | null) {
  if (!viewspec) return [];
  const tally = new Map<string, number>();
  for (const e of viewspec.elements) tally.set(e.displayMode, (tally.get(e.displayMode) ?? 0) + 1);
  for (const e of viewspec.edges) tally.set(e.displayMode, (tally.get(e.displayMode) ?? 0) + 1);
  return (["Fact", "Inference", "Hypothesis"] as const)
    .map((m) => ({
      name: certaintyPlain(m).label,
      value: tally.get(m) ?? 0,
      color: m === "Fact" ? C.emerald : m === "Inference" ? C.amber : C.stoneLight,
      full: `${certaintyPlain(m).label} — ${certaintyPlain(m).hint}`,
    }))
    .filter((d) => d.value > 0);
}

// ---- the tab ----

export function NumbersView() {
  const { report, viewspec } = useCie();

  const sev = useMemo(() => (report ? severityData(report) : []), [report]);
  const kinds = useMemo(() => (report ? kindData(report) : []), [report]);
  const files = useMemo(() => (report ? fileData(report) : []), [report]);
  const effects = useMemo(() => (report ? effectData(report) : []), [report]);
  const promises = useMemo(() => (report ? promiseData(report) : []), [report]);
  const certainty = useMemo(() => certaintyData(viewspec), [viewspec]);

  if (!report) return null;
  const fixSoon = report.findings.filter((f) => f.severity === "Critical" || f.severity === "High").length;

  return (
    <div className="space-y-4">
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
          <BarChart3 className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
          <p>
            <span className="font-semibold">Your project in pictures. </span>
            Same facts as the other pages, drawn as charts — so you can see the shape of things at a
            glance. {fixSoon > 0 ? `${fixSoon} ${fixSoon === 1 ? "thing needs" : "things need"} fixing soon — that's the red bar below.` : "Nothing needs urgent fixing — enjoy the green."}
          </p>
        </CardContent>
      </Card>

      <div className="grid gap-4 md:grid-cols-2 items-start">
        <ChartCard title="How urgent are the problems?" tell="Red bars first: that's what to look at before anything else.">
          <HBar data={sev} />
        </ChartCard>

        <ChartCard title="What kinds of problems?" tell="Each bar is a different kind of trouble, in plain words.">
          <HBar data={kinds} />
        </ChartCard>

        <ChartCard title="Where do the problems live?" tell="Which of your files has the most things to look at.">
          <HBar data={files} />
        </ChartCard>

        <ChartCard title="What does the code actually do?" tell="How many named things touch the outside world, and how.">
          <HBar data={effects} />
        </ChartCard>

        <ChartCard title="Promises: who wrote them?" tell="Green promises are written by you. Amber ones are CIE's guesses — worth a look.">
          <Donut
            data={promises}
            centerTop={String(report.contracts.length)}
            centerBottom="promises found"
          />
        </ChartCard>

        <ChartCard
          title="How sure is CIE about the map?"
          tell="Every bubble and connection on the Map page, sorted by certainty."
        >
          {certainty.length ? (
            <Donut
              data={certainty}
              centerTop={String(certainty.reduce((a, d) => a + d.value, 0))}
              centerBottom="things on the map"
            />
          ) : (
            <EmptyChart />
          )}
        </ChartCard>
      </div>
    </div>
  );
}
