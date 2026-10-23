"use client";

// The charting engine: turns the loaded CIE data into simple, friendly charts.
// Rendered by the stage's "Numbers" view (see stage.tsx).
// Rules: every chart gets a plain-words title, a one-line "what this tells you"
// caption, and honest empty states. No technical words on the axes.
//
// XFILTER — every chart here is BOTH a filter emitter and a filter receiver
// (lib/chart-filter.ts). Clicking a bar/slice stores one cross-chart filter
// in the store; every chart whose rows carry that dimension re-aggregates
// from the filtered rows (the same builders — one source of truth, so a
// click's key can never disagree with a receiver's predicate), prints its
// surviving count, and names its empty results. Clicking the same bucket
// again releases it; clicking any other bucket replaces the filter.

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
import { BarChart3, Filter, RotateCcw } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { severityPlain } from "@/lib/plain-words";
import {
  countText,
  certaintyRows,
  effectRows,
  fileRows,
  filterApplies,
  filteredContracts,
  filteredFindings,
  filteredSummaries,
  filterDestination,
  filterNoun,
  kindRows,
  promiseRows,
  severityRows,
  sliceMapSpec,
  type ChartFilter,
  type ChartRow,
  type DonutRow,
} from "@/lib/chart-filter";

// ---- shared bits ----

function ChartCard({
  title,
  tell,
  note,
  children,
}: {
  title: string;
  tell: string;
  note?: string; // the surviving-count disclosure ("12 of 40") when filtered
  children: React.ReactNode;
}) {
  return (
    <Card className="border-stone-200" data-chart-card={title}>
      <CardHeader className="pb-1">
        <CardTitle className="text-base flex items-center gap-2">
          {title}
          {note && (
            <span className="rounded-full bg-emerald-100 px-2 py-0.5 text-[10.5px] font-semibold text-emerald-800 tabular-nums">
              {note}
            </span>
          )}
        </CardTitle>
        <CardDescription>{tell}</CardDescription>
      </CardHeader>
      <CardContent className="pt-2">{children}</CardContent>
    </Card>
  );
}

function EmptyChart({ named }: { named?: string }) {
  return (
    <div className="h-[230px] flex flex-col items-center justify-center gap-1 text-sm text-stone-400 bg-stone-50 rounded-lg border border-dashed border-stone-200 px-6 text-center">
      <span>{named ?? "Not enough data to draw this yet."}</span>
      {named && <span className="text-[11px]">Clear the filter in the banner above to see the full picture.</span>}
    </div>
  );
}

// Horizontal bars with plain-word labels on the left. Clickable when the
// chart is a filter emitter (onPick): the selected bucket gets the drilldown
// treatment — dark outline + bold tick, never colour alone.
function HBar({
  data,
  height = 230,
  labelWidth = 168,
  selected,
  onPick,
}: {
  data: ChartRow[];
  height?: number;
  labelWidth?: number;
  selected?: string | null;
  onPick?: (value: string) => void;
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
            contentStyle={{ borderRadius: 10, border: "1px solid #e7e5e4", boxShadow: "0 4px 16px rgba(0,0,0,0.06)", fontSize: 12.5, padding: "8px 12px", background: "#ffffff" }}
            formatter={(value: number | string) => [`${value}`, "Count"]}
            labelFormatter={(_, payload) => payload?.[0]?.payload?.full ?? ""}
          />
          <Bar dataKey="count" radius={[0, 5, 5, 0]} barSize={16} isAnimationActive={false}>
            {data.map((d, i) => {
              const is = selected != null && d.value === selected;
              return (
                <Cell
                  key={i}
                  fill={d.color}
                  stroke={is ? "#065f46" : undefined}
                  strokeWidth={is ? 2 : 0}
                  style={{ cursor: onPick ? "pointer" : "default" }}
                  data-value={d.value}
                  aria-label={onPick ? `${d.full}${is ? " — click again to clear the filter" : " — click to filter the other charts"}` : undefined}
                  onClick={onPick ? () => onPick(d.value) : undefined}
                />
              );
            })}
          </Bar>
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}

// Round pie with a centre label, for two- or three-slice stories. Clickable
// like the bars; the legend entries are real buttons (keyboard parity).
function Donut({
  data,
  centerTop,
  centerBottom,
  selected,
  onPick,
}: {
  data: DonutRow[];
  centerTop: string;
  centerBottom: string;
  selected?: string | null;
  onPick?: (key: string) => void;
}) {
  const total = data.reduce((a, d) => a + d.value, 0);
  if (!total) return <EmptyChart />;
  const pieClick = (entry: unknown) => {
    if (!onPick) return;
    const key = String((entry as { key?: string })?.key ?? (entry as { payload?: { key?: string } })?.payload?.key ?? "");
    if (key) onPick(key);
  };
  return (
    <div className="flex flex-col sm:flex-row items-center gap-2 sm:gap-6">
      <div className="relative h-[190px] w-[190px] shrink-0">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={data}
              dataKey="value"
              nameKey="key"
              innerRadius={58}
              outerRadius={86}
              paddingAngle={2}
              strokeWidth={2}
              stroke="#ffffff"
              isAnimationActive={false}
              style={onPick ? { cursor: "pointer" } : undefined}
              onClick={onPick ? pieClick : undefined}
            >
              {data.map((d, i) => {
                const is = selected != null && d.key === selected;
                return (
                  <Cell
                    key={i}
                    fill={d.color}
                    stroke={is ? "#065f46" : "#ffffff"}
                    strokeWidth={is ? 2.5 : 2}
                  />
                );
              })}
            </Pie>
            <Tooltip contentStyle={{ borderRadius: 10, border: "1px solid #e7e5e4", boxShadow: "0 4px 16px rgba(0,0,0,0.06)", fontSize: 12.5, padding: "8px 12px", background: "#ffffff" }} formatter={(value: number | string, name) => [`${value}`, String(name)]} />
          </PieChart>
        </ResponsiveContainer>
        <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
          <span className="text-2xl font-bold text-stone-900">{centerTop}</span>
          <span className="text-[11px] text-stone-500 text-center px-6 leading-tight">{centerBottom}</span>
        </div>
      </div>
      <ul className="space-y-1.5 text-sm w-full">
        {data.map((d) => {
          const is = selected != null && d.key === selected;
          const body = (
            <>
              <span className="h-2.5 w-2.5 rounded-full shrink-0" style={{ background: d.color }} aria-hidden />
              <span className={`flex-1 ${is ? "font-semibold text-emerald-900" : "text-stone-700"}`}>{d.name}</span>
              <span className="font-semibold text-stone-900 tabular-nums">{d.value}</span>
            </>
          );
          return (
            <li key={d.key} className="flex items-center gap-2">
              {onPick ? (
                <button
                  type="button"
                  onClick={() => onPick(d.key)}
                  className="flex w-full items-center gap-2 text-left"
                  title={is ? "Click again to clear the filter" : `Filter the other charts by ${d.name}`}
                >
                  {body}
                </button>
              ) : (
                body
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

// ---- the tab ----

export function NumbersView() {
  const { report, viewspec, chartFilter, setChartFilter, clearChartFilter, setView } = useCie();

  // Receivers first: each row universe under the active filter. A dimension
  // a universe doesn't carry leaves its rows untouched (filterApplies).
  const findings = useMemo(() => (report ? filteredFindings(report, chartFilter) : []), [report, chartFilter]);
  const summaries = useMemo(() => (report ? filteredSummaries(report, chartFilter) : []), [report, chartFilter]);
  const contracts = useMemo(() => (report ? filteredContracts(report, chartFilter) : []), [report, chartFilter]);
  // The certainty chart counts map rows — under a file/certainty filter it
  // counts the SAME sliced spec the map page renders.
  const mapSlice = useMemo(
    () => (viewspec && chartFilter ? sliceMapSpec(viewspec, chartFilter) : null),
    [viewspec, chartFilter],
  );

  // The charts: emitters and receivers share these rows.
  const sev = useMemo(() => severityRows(findings), [findings]);
  const kinds = useMemo(() => kindRows(findings), [findings]);
  const files = useMemo(() => (report ? fileRows(findings, report) : []), [report, findings]);
  const effects = useMemo(() => effectRows(summaries), [summaries]);
  const promises = useMemo(() => promiseRows(contracts), [contracts]);
  const certainty = useMemo(() => certaintyRows(mapSlice ? mapSlice.spec : viewspec), [mapSlice, viewspec]);

  if (!report) return null;

  // Per-universe surviving counts for the banner + per-chart notes.
  const findingTotal = report.findings.length;
  const summaryTotal = report.summaries.length;
  const contractTotal = report.contracts.length;
  const elementTotal = viewspec?.elements.length ?? 0;
  const findingsAffected = filterApplies(chartFilter, "findings");
  const summariesAffected = filterApplies(chartFilter, "summaries");
  const contractsAffected = filterApplies(chartFilter, "contracts");
  const elementsAffected = filterApplies(chartFilter, "elements");
  const sevTotal = sev.reduce((a, d) => a + d.count, 0);
  const kindTotal = kinds.reduce((a, d) => a + d.count, 0);
  const fileTotal = files.reduce((a, d) => a + d.count, 0);
  const selOf = (dimension: ChartFilter["dimension"]) =>
    chartFilter && chartFilter.dimension === dimension ? chartFilter.value : null;
  const emit = (
    dimension: ChartFilter["dimension"],
    value: string,
    label: string,
    origin: string,
    originLabel: string,
  ) => setChartFilter({ dimension, value, label, origin, originLabel });

  const fixSoon = findings.filter((f) => f.severity === "Critical" || f.severity === "High").length;
  const bannerSurviving =
    chartFilter && (chartFilter.dimension === "effect"
      ? summaries.length
      : chartFilter.dimension === "authority"
        ? contracts.length
        : chartFilter.dimension === "certainty"
          ? mapSlice?.kept ?? elementTotal
          : findings.length);
  const bannerTotal =
    chartFilter && (chartFilter.dimension === "effect"
      ? summaryTotal
      : chartFilter.dimension === "authority"
        ? contractTotal
        : chartFilter.dimension === "certainty"
          ? elementTotal
          : findingTotal);

  return (
    <div className="space-y-4">
      {chartFilter && bannerTotal != null && bannerSurviving != null ? (
        <Card className="border-emerald-300 bg-emerald-50/70" data-testid="chart-filter-banner">
          <CardContent className="p-4 sm:p-5 flex flex-wrap items-center gap-x-3 gap-y-2 text-sm text-stone-700">
            <Filter className="h-4 w-4 text-emerald-700 shrink-0" aria-hidden />
            <p className="min-w-0 flex-1 leading-relaxed">
              <span className="font-semibold">Filtered to {chartFilter.label}</span> —{" "}
              {countText(bannerSurviving, bannerTotal)} {filterNoun(chartFilter.dimension)} match, from
              &ldquo;{chartFilter.originLabel}&rdquo;. The sibling charts and the{" "}
              {filterNoun(chartFilter.dimension) === "promises" ? "Promises" : "Fixes"} list show only what matches;
              the map follows file and certainty filters too.
            </p>
            <button
              type="button"
              onClick={() => setView(filterDestination(chartFilter.dimension))}
              className="shrink-0 rounded-lg border border-emerald-300 bg-white px-3 py-1.5 text-[12.5px] font-medium text-emerald-800 hover:bg-emerald-100"
            >
              See the {filterNoun(chartFilter.dimension)} list
            </button>
            <button
              type="button"
              onClick={clearChartFilter}
              className="shrink-0 flex items-center gap-1.5 rounded-lg border border-stone-300 bg-white px-3 py-1.5 text-[12.5px] font-medium text-stone-700 hover:bg-stone-100"
            >
              <RotateCcw className="h-3.5 w-3.5" aria-hidden /> Clear filter
            </button>
          </CardContent>
        </Card>
      ) : (
        <Card className="border-stone-200 bg-stone-50/60">
          <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
            <BarChart3 className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
            <p>
              <span className="font-semibold">Your project in pictures. </span>
              Same facts as the other pages, drawn as charts — so you can see the shape of things at a
              glance. {fixSoon > 0 ? `${fixSoon} ${fixSoon === 1 ? "thing needs" : "things need"} fixing soon — that's the red bar below.` : "Nothing needs urgent fixing — enjoy the green."}{" "}
              Tap any bar or slice to filter the other charts by it; tap it again to release.
            </p>
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-2 items-start">
        <ChartCard
          title="How urgent are the problems?"
          tell="Red bars first: that's what to look at before anything else."
          note={findingsAffected ? countText(sevTotal, findingTotal) : undefined}
        >
          {findingsAffected && !sevTotal ? (
            <EmptyChart named={`No ${filterNoun("severity")} match “${chartFilter?.label}”.`} />
          ) : (
            <HBar
              data={sev}
              selected={selOf("severity")}
              onPick={(value) =>
                emit("severity", value, severityPlain(value).label, "severity", "How urgent are the problems?")
              }
            />
          )}
        </ChartCard>

        <ChartCard
          title="What kinds of problems?"
          tell="Each bar is a different kind of trouble, in plain words."
          note={findingsAffected ? countText(kindTotal, findingTotal) : undefined}
        >
          {findingsAffected && !kindTotal ? (
            <EmptyChart named={`No ${filterNoun("kind")} match “${chartFilter?.label}”.`} />
          ) : (
            <HBar
              data={kinds}
              selected={selOf("kind")}
              onPick={(value) =>
                emit("kind", value, kinds.find((k) => k.value === value)?.full ?? value, "kind", "What kinds of problems?")
              }
            />
          )}
        </ChartCard>

        <ChartCard
          title="Where do the problems live?"
          tell="Which of your files has the most things to look at."
          note={findingsAffected ? countText(fileTotal, findingTotal) : undefined}
        >
          {findingsAffected && !fileTotal ? (
            <EmptyChart named={`No ${filterNoun("file")} match “${chartFilter?.label}”.`} />
          ) : (
            <HBar
              data={files}
              labelWidth={150}
              selected={selOf("file")}
              onPick={(value) =>
                emit("file", value, files.find((f) => f.value === value)?.full ?? value, "file", "Where do the problems live?")
              }
            />
          )}
        </ChartCard>

        <ChartCard
          title="What does the code actually do?"
          tell="How many named things touch the outside world, and how."
          note={summariesAffected ? countText(effects.reduce((a, d) => a + d.count, 0), summaryTotal) : undefined}
        >
          {summariesAffected && !effects.length ? (
            <EmptyChart named={`No ${filterNoun("effect")} match “${chartFilter?.label}”.`} />
          ) : (
            <HBar
              data={effects}
              selected={selOf("effect")}
              onPick={(value) =>
                emit("effect", value, effects.find((e) => e.value === value)?.full ?? value, "effect", "What does the code actually do?")
              }
            />
          )}
        </ChartCard>

        <ChartCard
          title="Promises: who wrote them?"
          tell="Green promises are written by you. Amber ones are CIE's guesses — worth a look."
          note={contractsAffected ? countText(promises.reduce((a, d) => a + d.value, 0), contractTotal) : undefined}
        >
          {contractsAffected && !promises.reduce((a, d) => a + d.value, 0) ? (
            <EmptyChart named={`No ${filterNoun("authority")} match “${chartFilter?.label}”.`} />
          ) : (
            <Donut
              data={promises}
              centerTop={String(promises.reduce((a, d) => a + d.value, 0))}
              centerBottom={contractsAffected ? "promises match the filter" : "promises found"}
              selected={selOf("authority")}
              onPick={(key) =>
                emit("authority", key, key === "explicit" ? "Written by you" : "CIE's guess", "authority", "Promises: who wrote them?")
              }
            />
          )}
        </ChartCard>

        <ChartCard
          title="How sure is CIE about the map?"
          tell="Every bubble and connection on the Map page, sorted by certainty."
          note={elementsAffected ? countText(certainty.reduce((a, d) => a + d.value, 0), elementTotal) : undefined}
        >
          {certainty.length ? (
            <Donut
              data={certainty}
              centerTop={String(certainty.reduce((a, d) => a + d.value, 0))}
              centerBottom={elementsAffected ? "map things match the filter" : "things on the map"}
              selected={selOf("certainty")}
              onPick={(key) =>
                emit("certainty", key, certainty.find((c) => c.key === key)?.name ?? key, "certainty", "How sure is CIE about the map?")
              }
            />
          ) : (
            <EmptyChart named={elementsAffected ? `No ${filterNoun("certainty")} match “${chartFilter?.label}”.` : undefined} />
          )}
        </ChartCard>
      </div>
    </div>
  );
}
