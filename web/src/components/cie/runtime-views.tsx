"use client";

// Two plain-words stage views added for the runtime playground:
//
//   DepsView     — what the project declares it depends on (pom.xml,
//                  build.gradle, package.json, Cargo.toml, requirements.txt,
//                  go.mod, composer.json, *.csproj), exactly as written,
//                  with the honest usage note per package.
//   RuntimeView  — the collected runtime evidence (CPU, memory, concurrency,
//                  deadlock, race, test coverage). Everything here was
//                  MEASURED by the project's own collector; when a project
//                  has none, this page says so and explains how to collect it.

import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useCie } from "@/lib/cie-store";
import type { CieDeps, CieRuntime, DeclaredDep } from "@/lib/cie-types";
import {
  Activity,
  Boxes,
  CheckCircle2,
  Clock,
  Cpu,
  FileCheck2,
  FlaskConical,
  Gauge,
  HelpCircle,
  MemoryStick,
  Package,
  ShieldAlert,
  Timer,
} from "lucide-react";

const cut = (s: string, n: number) => (s.length > n ? s.slice(0, n - 1) + "…" : s);

const ECO_PLAIN: Record<string, string> = {
  pypi: "Python (pip)",
  npm: "JavaScript / TypeScript (npm)",
  cargo: "Rust (Cargo)",
  go: "Go modules",
  maven: "Java (Maven)",
  gradle: "Java / Kotlin (Gradle)",
  composer: "PHP (Composer)",
  nuget: ".NET (NuGet)",
};

// ---------- Dependencies ----------

export function DepsView() {
  const { deps } = useCie();
  if (!deps || !deps.table || deps.table.files_scanned === 0) {
    return (
      <div className="space-y-4">
        <Card className="border-stone-200 bg-stone-50/60">
          <CardContent className="p-5 text-sm text-stone-700 leading-relaxed">
            <span className="font-semibold">What this page shows when there is data: </span>
            every package your project <em>declares</em> it needs — read from pom.xml, build.gradle,
            package.json, requirements.txt, pyproject.toml, Cargo.toml, go.mod, composer.json or
            *.csproj — with the version written exactly as the file says it.
          </CardContent>
        </Card>
        <Card className="border-stone-200">
          <CardContent className="p-8 text-center">
            <Package className="h-10 w-10 text-stone-300 mx-auto mb-3" />
            <p className="font-semibold text-stone-900 text-lg">No manifest files found</p>
            <p className="text-sm text-stone-600 mt-1 max-w-md mx-auto leading-relaxed">
              CIE looked for dependency lists in this project and found none it recognises. Nothing is
              guessed — when the project has one of those files, this page fills in with what it declares.
            </p>
          </CardContent>
        </Card>
      </div>
    );
  }

  const { deps: all, unparsed, files_scanned } = deps.table;
  const byEco = new Map<string, DeclaredDep[]>();
  for (const d of all) {
    const list = byEco.get(d.ecosystem) ?? [];
    list.push(d);
    byEco.set(d.ecosystem, list);
  }
  const usageFor = (name: string, eco: string) => deps.usage.find((u) => u.name === name && u.ecosystem === eco);

  return (
    <div className="space-y-4">
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 text-sm text-stone-700 leading-relaxed">
          <span className="font-semibold">What you&apos;re looking at: </span>
          {files_scanned} manifest file{files_scanned === 1 ? "" : "s"} read, {all.length} package
          {all.length === 1 ? "" : "s"} declared. CIE reads the lists as written — it doesn&apos;t download
          anything, resolve transitive trees, or judge compatibility. &ldquo;No import-backed usage&rdquo; is
          <span className="font-medium"> not </span> the same as &ldquo;unused&rdquo;: code can reach a package in
          ways static reading can&apos;t see.
        </CardContent>
      </Card>

      {unparsed.length > 0 && (
        <Card className="border-amber-200">
          <CardContent className="p-4 text-sm text-amber-900 leading-relaxed">
            <span className="font-semibold">Named honestly: </span>CIE couldn&apos;t follow {unparsed.length}{" "}
            manifest-looking file{unparsed.length === 1 ? "" : "s"}: {unparsed.slice(0, 4).map(cut).join(" · ")}
          </CardContent>
        </Card>
      )}

      {[...byEco.entries()].map(([eco, list]) => (
        <Card key={eco} className="border-stone-200">
          <CardHeader className="pb-2">
            <CardTitle className="text-base flex items-center gap-2">
              <Boxes className="h-4 w-4 text-emerald-700" />
              {ECO_PLAIN[eco] ?? eco}
              <Badge variant="outline" className="bg-stone-100 text-stone-600 border-stone-200 font-normal">
                {list.length} declared
              </Badge>
            </CardTitle>
            <CardDescription>Versions exactly as the manifest writes them.</CardDescription>
          </CardHeader>
          <CardContent>
            <ul className="divide-y divide-stone-100 max-h-96 overflow-y-auto pr-1">
              {list.map((d) => {
                const usage = usageFor(d.name, d.ecosystem);
                return (
                  <li key={`${d.source}-${d.name}`} className="py-2.5 flex flex-col gap-1">
                    <div className="flex items-baseline gap-2 flex-wrap">
                      <span className="font-mono text-sm font-medium text-stone-900 break-all">{d.name}</span>
                      {d.version_req && (
                        <span className="font-mono text-xs text-stone-500 bg-stone-100 rounded px-1.5 py-0.5">{d.version_req}</span>
                      )}
                      {d.scope === "dev" && (
                        <Badge variant="outline" className="bg-stone-50 text-stone-500 border-stone-200 text-[10px] font-normal">
                          dev / test
                        </Badge>
                      )}
                      {usage?.resolved && (
                        <Badge variant="outline" className="bg-emerald-50 text-emerald-700 border-emerald-200 text-[10px] font-normal">
                          imports seen
                        </Badge>
                      )}
                    </div>
                    <p className="text-xs text-stone-500 leading-relaxed">
                      declared in <span className="font-mono">{d.source}</span>
                      {d.licence ? ` · licence as written: ${d.licence}` : " · licence not declared in this manifest"}
                      {usage ? ` — ${usage.note}` : ""}
                    </p>
                  </li>
                );
              })}
            </ul>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

// ---------- Runtime evidence ----------

function DatasetShell(props: {
  icon: React.ReactNode;
  title: string;
  tool: string;
  honesty: string;
  equivalents: string[];
  children: React.ReactNode;
}) {
  return (
    <Card className="border-stone-200">
      <CardHeader className="pb-2">
        <CardTitle className="text-base flex items-center gap-2">
          {props.icon}
          {props.title}
        </CardTitle>
        <CardDescription className="font-mono text-[11px] leading-relaxed">{props.tool}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {props.children}
        <div className="flex flex-wrap gap-1.5 pt-1">
          {props.equivalents.map((e) => (
            <Badge key={e} variant="outline" className="bg-stone-50 text-stone-600 border-stone-200 text-[10px] font-normal">
              on real projects: {e}
            </Badge>
          ))}
        </div>
        <p className="text-xs text-stone-500 leading-relaxed border-t border-stone-100 pt-2">{props.honesty}</p>
      </CardContent>
    </Card>
  );
}

function MiniBar({ label, value, max, suffix, tone }: { label: string; value: number; max: number; suffix: string; tone?: string }) {
  const pct = Math.max(2, Math.min(100, (value / Math.max(max, 0.0001)) * 100));
  return (
    <div className="flex items-center gap-2">
      <span className="w-36 shrink-0 truncate font-mono text-xs text-stone-700" title={label}>
        {label}
      </span>
      <div className="flex-1 h-4 rounded bg-stone-100 overflow-hidden" role="img" aria-label={`${label}: ${value}${suffix}`}>
        <div className={`h-full rounded ${tone ?? "bg-emerald-500"}`} style={{ width: `${pct}%` }} />
      </div>
      <span className="w-20 shrink-0 text-right text-xs text-stone-600 tabular-nums">
        {value.toLocaleString()} {suffix}
      </span>
    </div>
  );
}

function fmtBytes(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)} MB`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)} kB`;
  return `${n} B`;
}

/** The thread-span timeline — a small honest gantt of who ran when. */
function SpanTimeline({ spans, makespan }: { spans: { thread: string; task: number; start_ms: number; end_ms: number }[]; makespan: number }) {
  if (!spans.length || makespan <= 0) return null;
  const threads = [...new Set(spans.map((s) => s.thread))].sort();
  const W = 560;
  const rowH = 16;
  const H = threads.length * rowH + 22;
  const x = (ms: number) => (ms / makespan) * (W - 8) + 4;
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className="w-full" role="img" aria-label="Thread timeline: each bar is one task on one thread">
      {threads.map((t, i) => (
        <g key={t}>
          <text x={4} y={i * rowH + 13} fontSize={9} fill="#78716c" className="font-mono">
            {t}
          </text>
        </g>
      ))}
      {spans.map((s) => {
        const row = threads.indexOf(s.thread);
        return (
          <rect
            key={`${s.thread}-${s.task}`}
            x={x(s.start_ms)}
            y={row * rowH + 2}
            width={Math.max(2, x(s.end_ms) - x(s.start_ms))}
            height={rowH - 5}
            rx={3}
            className="fill-emerald-500/80"
          >
            <title>{`${s.thread} · task ${s.task}: ${s.start_ms}–${s.end_ms} ms`}</title>
          </rect>
        );
      })}
      <text x={4} y={H - 4} fontSize={9} fill="#a8a29e" className="font-mono">
        0 ms → {Math.round(makespan)} ms
      </text>
    </svg>
  );
}

export function RuntimeView() {
  const { runtime } = useCie();
  const d = runtime?.datasets;

  if (!runtime || !d) {
    return (
      <div className="space-y-4">
        <Card className="border-stone-200 bg-stone-50/60">
          <CardContent className="p-5 text-sm text-stone-700 leading-relaxed">
            <span className="font-semibold">What this page shows when there is data: </span>
            runtime evidence that was actually measured — CPU hotspots, memory, thread timelines, a
            captured deadlock, observed lost updates, and test coverage — each naming the tool that
            produced it. CIE never invents runtime data, so a project without collected evidence gets
            this honest page instead.
          </CardContent>
        </Card>
        <Card className="border-stone-200">
          <CardContent className="p-8 text-center">
            <Activity className="h-10 w-10 text-stone-300 mx-auto mb-3" />
            <p className="font-semibold text-stone-900 text-lg">No collected runtime evidence for this project</p>
            <p className="text-sm text-stone-600 mt-1 max-w-lg mx-auto leading-relaxed">
              This is the honest empty state, not a broken chart. The sample project ships its own
              collector (scripts/collect_runtime.py) — run one on your project and this page fills with
              what you measured. Here is exactly what to collect, and with what:
            </p>
          </CardContent>
        </Card>
        <div className="grid gap-3 md:grid-cols-2">
          {[
            { icon: <Cpu className="h-4 w-4 text-emerald-700" />, t: "CPU hotspots", d: "which functions burn the time", demo: "cProfile", real: "py-spy · Java Flight Recorder · perf" },
            { icon: <MemoryStick className="h-4 w-4 text-emerald-700" />, t: "Memory", d: "where the bytes go, peaks tracked", demo: "tracemalloc", real: "heaptrack · memray · jemalloc · JFR" },
            { icon: <Timer className="h-4 w-4 text-emerald-700" />, t: "Concurrency timeline", d: "who ran when, thread by thread", demo: "thread spans via time.monotonic", real: "JFR thread events · async-profiler · eBPF" },
            { icon: <ShieldAlert className="h-4 w-4 text-emerald-700" />, t: "Deadlock detection", d: "captured stacks + waits-for cycle", demo: "faulthandler + cycle detection", real: "jstack analysers · py-spy dump" },
            { icon: <FlaskConical className="h-4 w-4 text-emerald-700" />, t: "Race evidence", d: "lost updates counted, honestly", demo: "observed read-add-write trials", real: "ThreadSanitizer · Go -race · Helgrind" },
            { icon: <FileCheck2 className="h-4 w-4 text-emerald-700" />, t: "Test coverage", d: "which lines the tests really touch", demo: "coverage.py", real: "JaCoCo · cargo-tarpaulin · go test -cover" },
          ].map((row) => (
            <Card key={row.t} className="border-stone-200">
              <CardContent className="p-4">
                <p className="text-sm font-semibold text-stone-900 flex items-center gap-2">
                  {row.icon} {row.t}
                </p>
                <p className="text-xs text-stone-600 mt-1">{row.d}</p>
                <p className="text-[11px] text-stone-500 mt-2 leading-relaxed">
                  this demo: <span className="font-mono">{row.demo}</span> · on real projects: {row.real}
                </p>
              </CardContent>
            </Card>
          ))}
        </div>
      </div>
    );
  }

  const covMax = Math.max(1, ...(d.coverage?.files.map((f) => f.percent) ?? [1]));
  const cpuMax = Math.max(0.01, ...(d.cpu?.top_functions.map((f) => f.cum_ms) ?? [1]));
  const memMax = Math.max(1, ...(d.memory?.top_sites.map((s) => s.bytes) ?? [1]));

  return (
    <div className="space-y-4">
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 text-sm text-stone-700 leading-relaxed space-y-1.5">
          <p>
            <span className="font-semibold">{runtime.project}</span> — collected{" "}
            <span className="font-mono text-xs">{runtime.collected_at}</span> on{" "}
            <span className="font-mono text-xs">{runtime.environment.platform}</span> (Python{" "}
            {runtime.environment.python}).
          </p>
          <p className="text-xs text-stone-500">{runtime.honesty}</p>
        </CardContent>
      </Card>

      {d.coverage && (
        <DatasetShell
          icon={<FileCheck2 className="h-4 w-4 text-emerald-700" />}
          title="Test coverage — what the tests really touch"
          tool={`${d.coverage.tool} — ${d.coverage.command}`}
          honesty={d.coverage.honesty}
          equivalents={d.coverage.real_world_equivalents}
        >
          <div className="space-y-1.5">
            {d.coverage.files.map((f) => (
              <MiniBar
                key={f.file}
                label={f.file}
                value={f.percent}
                max={covMax}
                suffix="%"
                tone={f.percent >= 70 ? "bg-emerald-500" : f.percent > 0 ? "bg-amber-500" : "bg-rose-400"}
              />
            ))}
          </div>
          {d.coverage.files.some((f) => f.note) && (
            <p className="text-xs text-stone-500 leading-relaxed">
              Files at 0 with a note were never imported by any test — listed honestly, not faked.
            </p>
          )}
          <p className="text-xs text-stone-600">
            Measured total: {d.coverage.totals.percent}% of the lines the run could see.
            {d.coverage.totals.note ? ` ${d.coverage.totals.note}` : ""}
          </p>
        </DatasetShell>
      )}

      {d.cpu && (
        <DatasetShell
          icon={<Cpu className="h-4 w-4 text-emerald-700" />}
          title="CPU hotspots — where the time went"
          tool={`${d.cpu.tool} — ${d.cpu.command}`}
          honesty={d.cpu.honesty}
          equivalents={d.cpu.real_world_equivalents}
        >
          <div className="space-y-1.5">
            {d.cpu.top_functions.slice(0, 8).map((f, i) => (
              <MiniBar key={`${f.function}-${i}`} label={cut(f.function, 24)} value={Math.round(f.cum_ms)} max={cpuMax} suffix="ms" />
            ))}
          </div>
        </DatasetShell>
      )}

      {d.memory && (
        <DatasetShell
          icon={<MemoryStick className="h-4 w-4 text-emerald-700" />}
          title="Memory — where the bytes went"
          tool={`${d.memory.tool} — ${d.memory.command}`}
          honesty={d.memory.honesty}
          equivalents={d.memory.real_world_equivalents}
        >
          <div className="flex flex-wrap gap-4 text-sm text-stone-700">
            <span>
              live at end: <span className="font-semibold tabular-nums">{fmtBytes(d.memory.allocated_bytes)}</span>
            </span>
            <span>
              peak: <span className="font-semibold tabular-nums">{fmtBytes(d.memory.peak_bytes)}</span>
            </span>
          </div>
          <div className="space-y-1.5">
            {d.memory.top_sites.map((s) => (
              <MiniBar key={s.where} label={s.where} value={s.bytes} max={memMax} suffix="" />
            ))}
          </div>
        </DatasetShell>
      )}

      {d.concurrency && (
        <DatasetShell
          icon={<Timer className="h-4 w-4 text-emerald-700" />}
          title="Concurrency — who ran when"
          tool={`${d.concurrency.tool} — ${d.concurrency.command}`}
          honesty={d.concurrency.honesty}
          equivalents={d.concurrency.real_world_equivalents}
        >
          <SpanTimeline spans={d.concurrency.spans} makespan={d.concurrency.makespan_ms} />
          <p className="text-xs text-stone-600">
            {d.concurrency.tasks} tasks across {d.concurrency.workers} threads finished in{" "}
            {d.concurrency.makespan_ms} ms wall time; the tasks sum to {d.concurrency.sum_of_task_ms} ms, so the
            observed overlap is <span className="font-semibold">{d.concurrency.observed_overlap}×</span> — measured,
            not claimed.
          </p>
        </DatasetShell>
      )}

      {d.deadlock && (
        <DatasetShell
          icon={<ShieldAlert className="h-4 w-4 text-emerald-700" />}
          title="Deadlock detection — captured, not imagined"
          tool={`${d.deadlock.tool} — ${d.deadlock.command}`}
          honesty={d.deadlock.honesty}
          equivalents={d.deadlock.real_world_equivalents}
        >
          {d.deadlock.detected ? (
            <div className="space-y-2">
              <p className="text-sm text-rose-800 bg-rose-50 border border-rose-200 rounded-lg px-3 py-2 leading-relaxed">
                <span className="font-semibold">Deadlock detected:</span> {d.deadlock.cycle.join(" ⇄ ")} — each thread
                holds one lock and waits forever for the other. The cycle was computed from the captured
                waits-for data; both stacks were really captured.
              </p>
              <ul className="space-y-1.5">
                {d.deadlock.threads.map((t) => (
                  <li key={t.thread} className="text-xs text-stone-600 font-mono bg-stone-50 border border-stone-100 rounded px-2 py-1.5">
                    {t.thread}: holds {t.holds} → waits {t.wants}
                    {t.blocked_at ? ` · blocked at ${t.blocked_at.file} line ${t.blocked_at.line}` : ""}
                    {t.stack_captured ? " · stack captured" : " · stack missing (named honestly)"}
                  </li>
                ))}
              </ul>
            </div>
          ) : (
            <p className="text-sm text-stone-600 flex items-center gap-2">
              <CheckCircle2 className="h-4 w-4 text-emerald-600" /> No deadlock captured in this run — said honestly; it does not mean the code is deadlock-free.
            </p>
          )}
        </DatasetShell>
      )}

      {d.race && (
        <DatasetShell
          icon={<FlaskConical className="h-4 w-4 text-emerald-700" />}
          title="Race evidence — lost updates, counted"
          tool={`${d.race.tool} — ${d.race.command}`}
          honesty={d.race.honesty}
          equivalents={d.race.real_world_equivalents}
        >
          <div className="grid gap-2 sm:grid-cols-2">
            <div className="rounded-lg border border-stone-200 bg-white p-3">
              <p className="text-xs font-medium text-stone-700 flex items-center gap-1.5">
                <Clock className="h-3.5 w-3.5 text-stone-400" /> natural window
              </p>
              <p className="text-2xl font-bold text-stone-900 tabular-nums mt-1">{d.race.lost_natural.toLocaleString()}</p>
              <p className="text-[11px] text-stone-500 leading-relaxed">
                updates lost across {d.race.natural_trials.length} unguarded runs. Zero is an honest result — races are timing-dependent, and a clean run proves nothing.
              </p>
            </div>
            <div className="rounded-lg border border-rose-200 bg-rose-50/60 p-3">
              <p className="text-xs font-medium text-rose-800 flex items-center gap-1.5">
                <Gauge className="h-3.5 w-3.5 text-rose-400" /> disclosed widened window
              </p>
              <p className="text-2xl font-bold text-rose-900 tabular-nums mt-1">{d.race.lost_widened.toLocaleString()}</p>
              <p className="text-[11px] text-rose-700/80 leading-relaxed">
                updates lost when the probe yields the GIL between read and write — the window real code
                leaves open. This is the proof the sequence is not atomic.
              </p>
            </div>
          </div>
        </DatasetShell>
      )}

      <Card className="border-stone-200 bg-stone-50/60">
        <CardHeader className="pb-2">
          <CardTitle className="text-base flex items-center gap-2">
            <HelpCircle className="h-4 w-4 text-stone-500" /> How to collect this on your project
          </CardTitle>
          <CardDescription>Same shapes of evidence; the tools change per language.</CardDescription>
        </CardHeader>
        <CardContent>
          <ul className="space-y-2 text-sm text-stone-700">
            {(runtime.how_to_collect_on_your_project ?? []).map((h) => (
              <li key={h.dataset} className="flex flex-col gap-0.5 border-b border-stone-100 pb-2 last:border-0">
                <span className="font-medium capitalize flex items-center gap-1.5">
                  <Activity className="h-3.5 w-3.5 text-emerald-700" /> {h.dataset}
                </span>
                <span className="text-xs text-stone-600 leading-relaxed">
                  here: {h.this_demo} · on real projects: {h.on_real_projects}
                </span>
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>
    </div>
  );
}
