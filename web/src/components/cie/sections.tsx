"use client";

// All stage views for the CIE workspace, written in plain words.
// Each renders only when the stage shows it (see stage.tsx).

import { useMemo, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { CommitProvenance, ObligationEvidence, StateTransitionEvidence, DriftProvenance, FaultEvidence } from "./rfc0020-evidence";
import MapView from "@/components/cie/map-view";
import { useCie } from "@/lib/cie-store";
import type { CieReport, Finding } from "@/lib/cie-types";
import { KNOWN_UNSUPPORTED } from "@/lib/language-profiles";
import { buildCoverageHeatmapSpec, buildConceptMindmapSpec } from "@/lib/quality";
import { ChatChart } from "@/components/cie/chat-chart";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import {
  severityPlain,
  findingTitle,
  obligationTitle,
  authorityPlain,
  certaintyPlain,
  elementKindPlain,
  DICTIONARY,
} from "@/lib/plain-words";
import {
  FileSearch,
  ShieldCheck,
  Map as MapIcon,
  Wand2,
  AlertTriangle,
  BadgeCheck,
  HelpCircle,
  ChevronDown,
  BookOpen,
  FileCode2,
  ListChecks,
  Languages,
  HeartPulse,
  Gauge,
} from "lucide-react";

// ---------- small shared pieces ----------

export function SeverityChip({ sev }: { sev: string }) {
  const p = severityPlain(sev);
  const cls =
    p.tone === "rose"
      ? "bg-rose-100 text-rose-800 border-rose-200"
      : p.tone === "amber"
        ? "bg-amber-100 text-amber-800 border-amber-200"
        : "bg-stone-100 text-stone-700 border-stone-200";
  return <Badge variant="outline" className={`${cls} font-semibold`}>{p.label}</Badge>;
}

export function CertaintyChip({ mode }: { mode: string }) {
  const c = certaintyPlain(mode);
  const cls =
    mode === "Fact"
      ? "bg-emerald-50 text-emerald-800 border-emerald-200"
      : mode === "Inference"
        ? "bg-amber-50 text-amber-800 border-amber-200"
        : "bg-stone-100 text-stone-700 border-stone-200";
  return <Badge variant="outline" className={cls}>{c.label}</Badge>;
}

function CoverageRing({ pct }: { pct: number }) {
  const r = 34, c = 2 * Math.PI * r;
  const filled = (Math.min(100, Math.max(0, pct)) / 100) * c;
  return (
    <svg width="88" height="88" viewBox="0 0 88 88" role="img" aria-label={`Checked: ${pct}%`}>
      <circle cx="44" cy="44" r={r} fill="none" stroke="#f5f5f4" strokeWidth="9" />
      <circle
        cx="44" cy="44" r={r} fill="none" stroke="#059669" strokeWidth="9"
        strokeLinecap="round" strokeDasharray={`${filled} ${c - filled}`}
        transform="rotate(-90 44 44)"
      />
      <text x="44" y="49" textAnchor="middle" fontSize="17" fontWeight="700" fill="#1c1917">
        {Math.round(pct)}%
      </text>
    </svg>
  );
}

function ruleKindPlain(k?: string): string {
  if (!k) return "Rule";
  const map: Record<string, string> = {
    must_authorize: "Check permission first",
    must_not_leak_secrets: "Keep secrets secret",
    must_validate_input: "Check user input before using it",
  };
  return map[k] ?? k.replace(/_/g, " ");
}

function ruleStatusPlain(s?: string): { label: string; cls: string } {
  const v = (s ?? "").toLowerCase();
  if (v === "explicit") return { label: "Written by you", cls: "bg-emerald-50 text-emerald-800 border-emerald-200" };
  if (v === "inferred_candidate" || v === "inferredcandidate") return { label: "CIE's guess", cls: "bg-amber-50 text-amber-800 border-amber-200" };
  return { label: s ?? "", cls: "bg-stone-100 text-stone-700 border-stone-200" };
}

// ---------- Loading ----------

export function LoadingView() {
  return (
    <div className="space-y-6">
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {[0, 1, 2, 3].map((i) => (
          <Card key={i}><CardContent className="p-6"><Skeleton className="h-16 w-full" /></CardContent></Card>
        ))}
      </div>
      <Card><CardContent className="p-6"><Skeleton className="h-32 w-full" /></CardContent></Card>
    </div>
  );
}

// ---------- Tab: Start here ----------

export function StartView() {
  const { report, quality, setPendingAsk, setView } = useCie();
  if (!report) return null;
  const criticals = report.findings.filter((f) => f.severity === "Critical").length;
  const pct = report.coverage.symbols_total
    ? (report.coverage.symbols_analyzed / report.coverage.symbols_total) * 100
    : 0;

  const stats = [
    { icon: FileSearch, label: "Files read", value: report.coverage.files_analyzed, sub: `of ${report.coverage.files_total} in the project`, tone: "text-stone-700" },
    { icon: FileCode2, label: "Named things found", value: report.symbols.length, sub: "functions, modules and more", tone: "text-stone-700" },
    { icon: ShieldCheck, label: "Promises found", value: report.contracts.length, sub: "things the code must always do", tone: "text-emerald-700" },
    { icon: AlertTriangle, label: "Things to look at", value: report.findings.length, sub: criticals ? `${criticals} need fixing soon` : "none are urgent", tone: criticals ? "text-rose-700" : "text-stone-700" },
  ];

  return (
    <div className="space-y-6">
      {/* 3-step story */}
      <div className="grid gap-4 md:grid-cols-3">
        {[
          { n: "1", icon: FileSearch, t: "CIE reads your code", d: "It looks at every file and remembers what each part does — like reading a book and taking notes." },
          { n: "2", icon: Wand2, t: "It checks the promises", d: "Code makes silent promises, like 'check the password first'. CIE finds them and tests them." },
          { n: "3", icon: ListChecks, t: "It tells you, in plain words", d: "What's fine, what's a guess, and what to fix first. No confusing jargon — ever." },
        ].map((s) => (
          <Card key={s.n} className="border-stone-200">
            <CardContent className="p-6 flex gap-4">
              <div className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-emerald-100 text-emerald-800 font-bold text-lg">
                {s.n}
              </div>
              <div>
                <p className="font-semibold text-stone-900 mb-1 flex items-center gap-2">
                  <s.icon className="h-4 w-4 text-emerald-700" /> {s.t}
                </p>
                <p className="text-sm text-stone-600 leading-relaxed">{s.d}</p>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* health snapshot */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-5 items-stretch">
        {stats.map((s) => (
          <Card key={s.label} className="border-stone-200">
            <CardContent className="p-5">
              <s.icon className={`h-5 w-5 mb-2 ${s.tone}`} />
              <p className="text-3xl font-bold text-stone-900">{s.value}</p>
              <p className="text-sm font-medium text-stone-700 mt-1">{s.label}</p>
              <p className="text-xs text-stone-500 mt-0.5">{s.sub}</p>
            </CardContent>
          </Card>
        ))}
        <Card className="border-stone-200">
          <CardContent className="p-5 flex items-center gap-4">
            <CoverageRing pct={pct} />
            <div>
              <p className="text-sm font-semibold text-stone-800">Checked</p>
              <p className="text-xs text-stone-500 mt-1 leading-relaxed">
                {report.coverage.complete ? "Everything CIE can read, it read." : "Some parts were skipped."}
              </p>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* health snapshot — real collected numbers from the quality ETL export;
          every tile answers to a tap (click-to-chat), the button opens the
          full health charts */}
      {quality && (
        <Card className="border-stone-200">
          <CardHeader className="pb-1">
            <CardTitle className="text-base flex items-center gap-2">
              <HeartPulse className="h-4 w-4 text-emerald-700" /> Health snapshot
              <span className="text-xs font-normal text-stone-400">— collected {quality.collected_at.slice(0, 10)}, every tile answers to a tap</span>
            </CardTitle>
            <CardDescription>
              The basic health of the system, from the extract → transform → load collector. Formulas live on the
              Quality metrics page.
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
            {(() => {
              const tiles = [
                {
                  label: "Security findings",
                  value: String(quality.security.issues.length),
                  sub: `${quality.security.issues.filter((i) => i.severity === "Critical").length} critical — rules disclosed`,
                  ask: "What are the security findings and where are they?",
                },
                {
                  label: "Duplicated code",
                  value: `${quality.duplication.duplicated_lines} lines`,
                  sub: `${quality.duplication.blocks_found} repeated block(s) across files`,
                  ask: "Where is the same code written twice?",
                },
                {
                  label: "Test coverage",
                  value: `${quality.coverage_heatmap.totals?.percent ?? 0}%`,
                  sub: "measured statements touched by the tests",
                  ask: "Show me a heat map of test coverage",
                },
                {
                  label: "Most twisted logic",
                  value: `CC ${quality.complexity.totals.max_cc}`,
                  sub: `avg ${quality.complexity.totals.avg_cc} over ${quality.complexity.totals.functions} functions`,
                  ask: "Where is the most twisted logic?",
                },
                {
                  label: "Crash rate (probed)",
                  value: `${quality.crash_probe.crash_rate != null ? Math.round(quality.crash_probe.crash_rate * 100) : 0}%`,
                  sub: `${quality.crash_probe.crashes} of ${quality.crash_probe.sessions} seeded sessions crashed`,
                  ask: "What share of sessions crashes?",
                },
              ];
              return tiles.map((t) => (
                <button
                  key={t.label}
                  type="button"
                  onClick={() => setPendingAsk(t.ask)}
                  title={`${t.label} — tap to ask CIE about this`}
                  className="text-left rounded-lg border border-stone-200 bg-white p-3 hover:border-emerald-300 hover:bg-emerald-50/40 transition-colors"
                >
                  <p className="text-xl font-bold text-stone-900">{t.value}</p>
                  <p className="text-xs font-semibold text-stone-700 mt-0.5">{t.label}</p>
                  <p className="text-[11px] text-stone-500 mt-0.5 leading-snug">{t.sub}</p>
                </button>
              ));
            })()}
          </CardContent>
          <CardContent className="flex flex-wrap items-center gap-2 pt-0">
            <Button
              size="sm"
              variant="outline"
              className="h-8 border-emerald-200 text-emerald-800 hover:bg-emerald-50"
              onClick={() => setView("quality")}
            >
              <Gauge className="h-3.5 w-3.5" /> The eight quality metrics
            </Button>
            <Button
              size="sm"
              variant="outline"
              className="h-8 border-emerald-200 text-emerald-800 hover:bg-emerald-50"
              onClick={() => setView("menu")}
            >
              <HeartPulse className="h-3.5 w-3.5" /> All the health charts
            </Button>
            {(() => {
              const heat = buildCoverageHeatmapSpec(quality);
              return heat ? (
                <details className="w-full mt-1">
                  <summary className="text-xs text-stone-500 hover:text-stone-800 cursor-pointer select-none">
                    Peek at the coverage heat map right here
                  </summary>
                  <div className="mt-2 rounded-lg border border-stone-200 bg-white p-3">
                    <ChatChart spec={heat} />
                  </div>
                </details>
              ) : null;
            })()}
          </CardContent>
        </Card>
      )}

      {/* dictionary */}
      <Card className="border-stone-200 bg-stone-50/60">
        <CardHeader className="pb-2">
          <CardTitle className="text-base flex items-center gap-2">
            <BookOpen className="h-4 w-4 text-stone-500" /> Words CIE uses
          </CardTitle>
          <CardDescription>Every word on this site, explained simply.</CardDescription>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {DICTIONARY.map((d) => (
            <div key={d.word} className="rounded-lg border border-stone-200 bg-white p-3">
              <p className="text-sm font-semibold text-stone-900">{d.word}</p>
              <p className="text-sm text-stone-600 mt-0.5 leading-relaxed">{d.means}</p>
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

// ---------- View: Map (a canvas view — the stage puts the camera and the
// fisheye lens around it; the picked bubble becomes the chat's referent).
// The bubble's details are a STAGE overlay (MapInfoCard), so they stay a
// constant size while the map itself zooms. ----------

export function MapViewSection() {
  const { viewspec, mapSelected: sel, setMapSelected: setSel } = useCie();
  if (!viewspec) return null;

  return (
    <div className="p-4 sm:p-6 pb-16 w-full">
      <Card className="border-stone-200 shadow-sm bg-white w-full">
        <CardContent className="p-4 sm:p-6">
          <MapView spec={viewspec} selected={sel} onSelect={setSel} />
        </CardContent>
      </Card>
    </div>
  );
}

/** Fixed-size overlay: the plain-words facts about the picked bubble. */
export function MapInfoCard() {
  const { mapSelected: sel, setMapSelected: setSel } = useCie();
  return (
    <Card className="border-stone-200 shadow-md bg-white/95 backdrop-blur w-[250px]" data-lens-ui>
      <CardHeader className="pb-1.5 px-4 py-3">
        <CardTitle className="text-sm break-words">{sel ? sel.label : "Pick a bubble"}</CardTitle>
        <CardDescription className="text-xs">
          {sel ? elementKindPlain(sel.elementKind) : "Tap any bubble — it becomes the “these” in your next chat message."}
        </CardDescription>
      </CardHeader>
      {sel && (
        <CardContent className="space-y-2.5 text-xs px-4 pb-4">
          <div>
            <p className="font-medium text-stone-800 mb-1">How sure is CIE?</p>
            <CertaintyChip mode={sel.displayMode} />
            <p className="text-stone-600 mt-1 leading-relaxed">{certaintyPlain(sel.displayMode).hint}</p>
          </div>
          {sel.salience?.whyShown && (
            <div>
              <p className="font-medium text-stone-800 mb-1">Why is it on the map?</p>
              <p className="text-stone-600 leading-relaxed">{sel.salience.whyShown.replace("(s)", "s")}</p>
            </div>
          )}
          <div>
            <p className="font-medium text-stone-800 mb-1">Where in your code?</p>
            <p className="text-stone-600 font-mono text-[10px] bg-stone-100 rounded px-1.5 py-0.5 inline-block break-all">{sel.evidence.file}</p>
          </div>
          {sel.claimId && <p className="text-[10px] text-stone-400">Reference number: {sel.claimId}</p>}
          <div className="flex gap-2 pt-0.5">
            <button
              type="button"
              onClick={() => setSel(null)}
              className="text-[11px] text-stone-500 hover:text-stone-800 underline"
            >
              Clear pick
            </button>
          </div>
        </CardContent>
      )}
    </Card>
  );
}

// ---------- Tab: What your app does (concepts) ----------

export function ConceptsView() {
  const { concepts, report } = useCie();
  if (!concepts || !report) return null;
  const roots = concepts.nodes.filter((n) => n.parent === null);
  const modules = concepts.nodes.filter((n) => n.kind === "Module");
  const symName = (id: number) => report.symbols.find((s) => s.id === id)?.qual_name ?? "?";

  const ruleList = (node: (typeof concepts.nodes)[number]) => (
    <>
      {node.behaviors.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mb-2">
          {node.behaviors.map((b, i) => (
            <Badge key={i} variant="outline" className="bg-emerald-50 text-emerald-800 border-emerald-200 font-normal">
              Does: {b.kind}
            </Badge>
          ))}
        </div>
      )}
      {node.rules && node.rules.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {node.rules.map((r, i) => {
            const st = ruleStatusPlain(String(r.authority ?? "").toLowerCase());
            const rid = typeof r.contract === "number" ? `C-${String(r.contract).padStart(3, "0")}` : "";
            return (
              <Badge key={i} variant="outline" className={`${st.cls} font-normal`}>
                Rule {rid}: {ruleKindPlain(r.kind)} · {st.label}
              </Badge>
            );
          })}
        </div>
      )}
    </>
  );

  return (
    <div className="space-y-4">
      <Card className="border-stone-200">
        <CardHeader className="pb-3">
          <CardTitle className="text-base">The big picture</CardTitle>
          <CardDescription>What your whole project does, in one glance.</CardDescription>
        </CardHeader>
        <CardContent>
          {roots.map((r) => (
            <div key={r.id}>
              {ruleList(r)}
              <p className="text-xs text-stone-500 mt-2">
                Covers {r.symbols.length} named things, e.g. {r.symbols.slice(0, 4).map(symName).join(", ")}
                {r.symbols.length > 4 ? "…" : ""}
              </p>
            </div>
          ))}
        </CardContent>
      </Card>

      <div className="grid gap-4 md:grid-cols-2">
        {modules.map((m) => (
          <Card key={m.id} className="border-stone-200">
            <CardHeader className="pb-2">
              <CardTitle className="text-base font-mono">{m.label}</CardTitle>
              <CardDescription>A code file · {m.symbols.length} named things inside</CardDescription>
            </CardHeader>
            <CardContent>{ruleList(m)}</CardContent>
          </Card>
        ))}
      </div>
    </div>
  );
}

// ---------- Tab: Mindmap (every concept found under one chosen concept) ----------

export function MindmapView() {
  const { concepts } = useCie();
  const [focusId, setFocusId] = useState<number | null>(null);
  if (!concepts?.nodes?.length) return null;

  const root = concepts.nodes.find((n) => n.parent === null) ?? null;
  const options = [...concepts.nodes]
    .sort((a, b) => (a.id === root?.id ? -1 : b.id === root?.id ? 1 : a.label.localeCompare(b.label)))
    .slice(0, 300);
  const selectedId = focusId ?? root?.id ?? options[0]?.id ?? null;
  const spec = buildConceptMindmapSpec(concepts, selectedId);

  return (
    <div className="space-y-4">
      <Card className="border-stone-200">
        <CardHeader className="pb-3">
          <CardTitle className="text-base">Mindmap</CardTitle>
          <CardDescription>Pick any concept — everything the engine found underneath it fans out from the centre.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <Select value={selectedId != null ? String(selectedId) : undefined} onValueChange={(v) => setFocusId(Number(v))}>
            <SelectTrigger className="w-full sm:w-80">
              <SelectValue placeholder="Pick a concept" />
            </SelectTrigger>
            <SelectContent>
              {options.map((n) => (
                <SelectItem key={n.id} value={String(n.id)}>
                  {n.kind}: {n.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          {spec ? <ChatChart key={selectedId} spec={spec} /> : <p className="text-sm text-stone-500">Nothing to draw yet.</p>}
        </CardContent>
      </Card>
    </div>
  );
}

// ---------- Tab: Things to look at (findings) ----------

function FindingCard({ f, file }: { f: Finding; file: string }) {
  const [open, setOpen] = useState(false);
  const p = severityPlain(f.severity);
  return (
    <Card className={`border ${p.tone === "rose" ? "border-rose-200" : p.tone === "amber" ? "border-amber-200" : "border-stone-200"}`}>
      <CardContent className="p-5 space-y-2.5">
        <div className="flex items-center gap-2 flex-wrap">
          <SeverityChip sev={f.severity} />
          <span className="text-xs text-stone-400">{p.hint}</span>
        </div>
        <p className="font-semibold text-stone-900">{findingTitle(f)}</p>
        <p className="text-sm text-stone-600 leading-relaxed">{f.message}</p>
        <CommitProvenance commits={f.introduced}/>
        {f.remediation && (
          <p className="text-sm text-emerald-900 bg-emerald-50 border border-emerald-100 rounded-lg px-3 py-2 leading-relaxed">
            <span className="font-semibold">How to fix: </span>{f.remediation}
          </p>
        )}
        <div className="flex items-center justify-between flex-wrap gap-2">
          <span className="text-xs text-stone-500 font-mono">{file}{f.evidence_lines?.length ? ` · line ${f.evidence_lines[0]}` : ""}</span>
          <Collapsible open={open} onOpenChange={setOpen}>
            <CollapsibleTrigger className="text-xs text-stone-500 hover:text-stone-800 flex items-center gap-1">
              <HelpCircle className="h-3.5 w-3.5" /> {open ? "Hide" : "Show"} technical words
              <ChevronDown className={`h-3 w-3 transition-transform ${open ? "rotate-180" : ""}`} />
            </CollapsibleTrigger>
            <CollapsibleContent>
              <p className="text-xs text-stone-500 mt-2 bg-stone-50 border border-stone-100 rounded p-2 font-mono">
                {f.kind} · {f.id}
              </p>
            </CollapsibleContent>
          </Collapsible>
        </div>
      </CardContent>
    </Card>
  );
}

export function FindingsView() {
  const { report } = useCie();
  const sorted = useMemo(() => {
    if (!report) return [];
    const order = { Critical: 0, High: 1, Medium: 2, Low: 3 } as Record<string, number>;
    return [...report.findings].sort((a, b) => (order[a.severity] ?? 9) - (order[b.severity] ?? 9));
  }, [report]);
  if (!report) return null;

  const groups: { key: string; title: string; desc: string }[] = [
    { key: "Critical", title: "Fix soon", desc: "These could let the wrong people in or lose data." },
    { key: "High", title: "Please look", desc: "Not an emergency, but worth your attention." },
    { key: "Medium", title: "Worth checking", desc: "Could cause trouble later." },
    { key: "Low", title: "Good to know", desc: "No rush — just so you know." },
  ];

  if (!report.findings.length) {
    return (
      <div>
        <Card className="border-emerald-200 bg-emerald-50">
          <CardContent className="p-8 text-center">
            <BadgeCheck className="h-10 w-10 text-emerald-700 mx-auto mb-3" />
            <p className="font-semibold text-emerald-900 text-lg">Nothing to worry about</p>
            <p className="text-sm text-emerald-800 mt-1">CIE looked through everything and found no problems to report. Well done!</p>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {groups.map((g) => {
        const items = sorted.filter((f) => f.severity === g.key);
        if (!items.length) return null;
        return (
          <section key={g.key} aria-label={g.title} className="space-y-3">
            <div>
              <h3 className="font-semibold text-stone-900">{g.title} <span className="text-stone-400 font-normal">({items.length})</span></h3>
              <p className="text-sm text-stone-500">{g.desc}</p>
            </div>
            {items.map((f) => (
              <FindingCard key={f.id} f={f} file={report.files[f.symbol ? report.symbols.find((s) => s.id === f.symbol)?.loc.file ?? 0 : 0] ?? ""} />
            ))}
          </section>
        );
      })}
    </div>
  );
}

// ---------- Tab: Promises (contracts) ----------

export function PromisesView() {
  const { report, openSourceViewer } = useCie();
  if (!report) return null;
  return (
    <div className="space-y-4">
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 text-sm text-stone-700 leading-relaxed">
          <span className="font-semibold">What is a promise?</span> Code makes silent promises, like
          &ldquo;always check the password before letting someone in&rdquo;. CIE finds these promises and
          tells you who has to keep them — so nothing important gets forgotten.
        </CardContent>
      </Card>
      <StateTransitionEvidence report={report} onOpen={id=>{const s=report.symbols.find(s=>s.id===id);if(s)openSourceViewer({file:report.files[s.loc.file],byteStart:s.loc.start,byteEnd:s.loc.end,elName:s.qual_name});}}/>
      {report.contracts.map((c) => {
        const a = authorityPlain(c.authority);
        const keepers = c.symbols.map((id) => report.symbols.find((s) => s.id === id)?.qual_name).filter(Boolean);
        return (
          <Card key={c.id} className="border-stone-200">
            <CardContent className="p-5 space-y-2">
              <div className="flex items-center gap-2 flex-wrap">
                <BadgeCheck className="h-5 w-5 text-emerald-700" />
                <p className="font-semibold text-stone-900">{obligationTitle(c.obligation)}</p>
                <Badge variant="outline" className={a.label === "Written by you" ? "bg-emerald-50 text-emerald-800 border-emerald-200" : "bg-amber-50 text-amber-800 border-amber-200"}>
                  {a.label}
                </Badge>
              </div>
              <p className="text-sm text-stone-600 leading-relaxed">&ldquo;{c.expr}&rdquo;</p>
              {keepers.length > 0 && (
                <p className="text-sm text-stone-700">
                  <span className="font-medium">Who must keep it: </span>
                  <span className="font-mono text-xs bg-stone-100 rounded px-1.5 py-0.5">{keepers.join(", ")}</span>
                </p>
              )}
              <ObligationEvidence contract={c} report={report} onOpen={id=>{const s=report.symbols.find(s=>s.id===id);if(s)openSourceViewer({file:report.files[s.loc.file],byteStart:s.loc.start,byteEnd:s.loc.end,elName:s.qual_name});}}/>
              {a.hint && <p className="text-xs text-stone-400">{a.hint}</p>}
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}

// ---------- Tab: Can I trust this? (evidence) ----------

export function TrustView() {
  const { report, viewspec, repoScan, drift, openSourceViewer } = useCie();
  if (!report) return null;
  const ev = report.evidence;
  return (
    <div className="space-y-4">
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 text-sm text-stone-700 leading-relaxed">
          <span className="font-semibold">Why this page exists: </span>
          a tool that only says &ldquo;everything is fine&rdquo; is dangerous. CIE shows you
          exactly what it knows, what it guessed, and what it couldn't check — so you can
          decide how much to trust it.
        </CardContent>
      </Card>

      <StateTransitionEvidence report={report} onOpen={id=>{const s=report.symbols.find(s=>s.id===id);if(s)openSourceViewer({file:report.files[s.loc.file],byteStart:s.loc.start,byteEnd:s.loc.end,elName:s.qual_name});}}/>
      <DriftProvenance claims={drift?.claims}/>
      <FaultEvidence report={report}/>
      {/* language honesty — only exists for connect-a-repo flows */}
      {repoScan && (
        <Card className={repoScan.unsupported.length ? "border-amber-200" : "border-emerald-200"}>
          <CardHeader className="pb-2">
            <CardTitle className="text-base flex items-center gap-2">
              <Languages className={`h-4 w-4 ${repoScan.unsupported.length ? "text-amber-700" : "text-emerald-700"}`} />
              Languages in your repo — what CIE read, and what it couldn't
            </CardTitle>
            <CardDescription>Taken from a quick look inside {repoScan.name} before the check started.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <div className="flex flex-wrap items-center gap-1.5">
              {repoScan.languages.map((l) => (
                <span key={l.id} className="inline-flex items-center rounded-full bg-emerald-100 text-emerald-800 px-2.5 py-1 text-xs font-medium">
                  {l.name} · {l.files} file{l.files === 1 ? "" : "s"} read
                </span>
              ))}
              {repoScan.unsupported.map((l) => (
                <span key={l.id} className="inline-flex items-center rounded-full bg-amber-100 text-amber-800 px-2.5 py-1 text-xs font-medium">
                  {l.name} · {l.files} file{l.files === 1 ? "" : "s"} skipped
                </span>
              ))}
            </div>
            {repoScan.unsupported.length > 0 ? (
              <ul className="space-y-1.5 text-sm text-stone-600">
                {repoScan.unsupported.map((l) => {
                  const meta = KNOWN_UNSUPPORTED.find((k) => k.id === l.id);
                  return (
                    <li key={l.id} className="flex gap-2 leading-relaxed">
                      <span className="text-amber-600 mt-0.5">•</span>
                      <span>
                        <span className="font-medium text-stone-800">{l.name}</span> — {meta?.why ?? "CIE skips these files for now."}
                      </span>
                    </li>
                  );
                })}
                <li className="flex gap-2 leading-relaxed text-stone-600">
                  <span className="text-amber-600 mt-0.5">•</span>
                  <span>Anything in those files is missing from every page here. CIE would rather leave an honest gap than guess about code it can&apos;t read.</span>
                </li>
              </ul>
            ) : (
              <p className="text-sm text-emerald-800 flex items-center gap-1.5">
                <BadgeCheck className="h-4 w-4 shrink-0" /> Every code file in this repo uses a language CIE can read.
              </p>
            )}
          </CardContent>
        </Card>
      )}

      <div className="grid gap-4 md:grid-cols-2">
        <Card className="border-amber-200">
          <CardHeader className="pb-2">
            <CardTitle className="text-base flex items-center gap-2"><Wand2 className="h-4 w-4 text-amber-700" /> Things CIE assumed</CardTitle>
            <CardDescription>Shortcuts CIE took. Usually fine — but you should know.</CardDescription>
          </CardHeader>
          <CardContent>
            <ul className="space-y-2 text-sm text-stone-600">
              {ev.assumptions.length ? ev.assumptions.map((a, i) => (
                <li key={i} className="flex gap-2 leading-relaxed"><span className="text-amber-600 mt-0.5">•</span>{a}</li>
              )) : <li className="text-stone-400">None. Nothing was assumed.</li>}
            </ul>
          </CardContent>
        </Card>

        <Card className="border-stone-200">
          <CardHeader className="pb-2">
            <CardTitle className="text-base flex items-center gap-2"><HelpCircle className="h-4 w-4 text-stone-500" /> Names CIE couldn't match</CardTitle>
            <CardDescription>Calls to things CIE couldn't find in this project.</CardDescription>
          </CardHeader>
          <CardContent>
            <ul className="space-y-2 text-sm text-stone-600 max-h-48 overflow-y-auto">
              {ev.unresolved.length ? ev.unresolved.map((a, i) => (
                <li key={i} className="flex gap-2 leading-relaxed font-mono text-xs"><span className="text-stone-400 mt-0.5">•</span>{a}</li>
              )) : <li className="text-stone-400">None. Every name was matched.</li>}
            </ul>
          </CardContent>
        </Card>

        <Card className="border-stone-200">
          <CardHeader className="pb-2">
            <CardTitle className="text-base flex items-center gap-2"><MapIcon className="h-4 w-4 text-stone-500" /> Notes from the check</CardTitle>
            <CardDescription>Technical limits, translated as honestly as we can.</CardDescription>
          </CardHeader>
          <CardContent>
            <ul className="space-y-2 text-sm text-stone-600">
              {report.coverage.notes.map((a, i) => (
                <li key={i} className="flex gap-2 leading-relaxed"><span className="text-stone-400 mt-0.5">•</span>{a}</li>
              ))}
            </ul>
          </CardContent>
        </Card>

        <Card className="border-stone-200">
          <CardHeader className="pb-2">
            <CardTitle className="text-base flex items-center gap-2"><BadgeCheck className="h-4 w-4 text-emerald-700" /> Paper trail</CardTitle>
            <CardDescription>Proof of where these results came from.</CardDescription>
          </CardHeader>
          <CardContent className="text-sm text-stone-600 space-y-1.5">
            <p>Engine: <span className="font-mono text-xs bg-stone-100 rounded px-1.5 py-0.5">{ev.provenance}</span></p>
            <p>Snapshot number: <span className="font-mono text-xs bg-stone-100 rounded px-1.5 py-0.5">rev {report.rev}</span></p>
            {viewspec?.semanticHash && (
              <p>Map fingerprint: <span className="font-mono text-xs bg-stone-100 rounded px-1.5 py-0.5 break-all">{viewspec.semanticHash.slice(0, 24)}…</span></p>
            )}
            {viewspec?.footer?.caption && <p className="text-xs text-stone-400 pt-1">&ldquo;{viewspec.footer.caption}&rdquo;</p>}
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
