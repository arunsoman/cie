"use client";

// "Master this codebase" — four sections, nothing else:
//   ① a paragraph describing the project — from the stack pass (build files,
//      framework classes read as written) + the engine's own numbers.
//      Inferred things stay tagged; every claim names its file and is
//      clickable into the real code.
//   ② the mains graph — Spring's ONE application class with everything it
//      boots (port, schedulers, listeners, the controllers), or, for stacks
//      CIE hasn't parsed yet, the engine's usual entry points — honestly.
//   ③ the routes table — every HTTP endpoint the framework code declares.
//   ④ the data-flow diagram — per route, controller → service → repository
//      → database → response, with every if/else and switch branch as
//      written, throws and returns included. Conditions are QUOTED, never
//      evaluated (CIE doesn't run the code).
//
// The chat keeps the old journey's fast paths; this page is the map.

import { useEffect, useMemo, useState } from "react";
import { Boxes, CheckCircle2, FileQuestion, Map as MapIcon, Route as RouteIcon, Workflow } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { computeEntryPoints, type MasteryRow } from "@/lib/mastery";
import type { FlowNode, TechStack } from "@/lib/stack-types";

// ============================================================
// ① the paragraph
// ============================================================

function StackParagraph({ stack, hasReport, noProject }: { stack: TechStack | null; hasReport: boolean; noProject: boolean }) {
  if (noProject) {
    return (
      <div className="rounded-xl border border-amber-200 bg-amber-50 p-4 text-[13px] text-amber-900 leading-relaxed">
        No project is loaded yet. <b>Use your own</b> — a repo link with its branch, or a folder on this computer — and CIE reads the build files, Docker files and framework code directly. The sample project also works, loaded by default.
      </div>
    );
  }
  if (!stack) {
    return (
      <p className="text-[13px] text-stone-600 leading-relaxed">
        This project was loaded without the tech-stack pass (an older analysis, or the engine&apos;s own JSON files), so the four sections below only have the engine&apos;s counts — reconnect the project to get the full read.
      </p>
    );
  }

  const parts: { text: string; evidence?: { rel?: string; line?: number } }[] = [];
  const jvm = stack.jvm ?? null;
  const buildName = stack.build.artifactName || "the project";
  const toolWords = { maven: "a Maven", gradle: "a Gradle", cargo: "a Cargo", npm: "an npm", go: "a Go", python: "a Python", mix: "an Elixir" }[stack.build.tool];
  if (toolWords) {
    parts.push({ text: `${buildName} is ${toolWords} build.` });
  }

  if (stack.frameworks.length > 0) {
    const f = stack.frameworks[0];
    parts.push({ text: ` ${f.name} — declared in`, evidence: { rel: f.evidence } });
    parts.push({
      text: ` ${f.modules.length ? `(${f.modules.slice(0, 4).join(", ")}${f.modules.length > 4 ? ", …" : ""})` : ""}`,
    });
  }

  if (jvm) {
    const oneRuntime = jvm.javaFiles > 0 && jvm.kotlinFiles > 0;
    if (oneRuntime) {
      parts.push({ text: ` ${jvm.javaFiles} java + ${jvm.kotlinFiles} kotlin files share ONE runtime — one family, two syntaxes.` });
    } else {
      parts.push({ text: ` ${jvm.scanned} java/kotlin file(s) scanned.` });
    }
    if (stack.mains.length > 0) {
      const m = stack.mains[0];
      parts.push({ text: ` ${m.name}`, evidence: { rel: m.rel, line: m.line } });
      parts.push({
        text: ` boots the framework: ${stack.routes.length} HTTP route(s) run through ${jvm.services.length} service(s) down to ${jvm.repositories.length} repositor${jvm.repositories.length === 1 ? "y" : "ies"} over ${jvm.entities.length} entit${jvm.entities.length === 1 ? "y" : "ies"}.`,
      });
    }
    if (jvm.scheduledIn.length > 0) parts.push({ text: ` ${jvm.scheduledIn.length} scheduled job(s) run on timers.` });
    if (jvm.listeners.length > 0) parts.push({ text: ` ${jvm.listeners.length} messaging listener(s) react to topics.` });
  } else {
    const langs = stack.languages.map((l) => `${l.files} ${l.family} file(s)`).join(" + ");
    if (langs) parts.push({ text: ` ${langs} counted.` });
    parts.push({
      text: ` No route/main parser for this stack yet — CIE counts files but doesn't name routes for it (that's the honest stub, not a guess).`,
    });
  }

  if (stack.docker.present) {
    const svc = stack.docker.compose?.services ?? [];
    parts.push({
      text: ` It ships as Docker: ${svc.length ? `compose with ${svc.length} service${svc.length === 1 ? "" : "s"} (${svc.slice(0, 4).map((s) => s.name).join(", ")}), ` : ""}${stack.docker.dockerfiles.length} Dockerfile(s).`,
    });
    parts.push({ text: " Read as written; CIE doesn't build or run containers." });
  }
  const cfg = stack.config[0];
  if (cfg?.port) {
    parts.push({ text: ` The code says it listens on port ${cfg.port}`, evidence: { rel: cfg.rel } });
    parts.push({ text: "." });
  }
  if (cfg?.datasource) {
    parts.push({ text: ` Its database URL is "${cfg.datasource}"`, evidence: { rel: cfg.rel } });
    parts.push({ text: "." });
  }

  const notes = stack.notes.filter((n) => !n.startsWith("[stack]"));
  return (
    <div className="space-y-2">
      <p className="text-[13px] text-stone-700 leading-relaxed">
        {parts.map((p, i) => (
          <span key={i}>
            {p.text}
            {p.evidence?.rel && (
              <EvidenceChip
                rel={p.evidence.rel}
                line={p.evidence.line}
              />
            )}
          </span>
        ))}
      </p>
      {notes.length > 0 && (
        <ul className="space-y-1">
          {notes.slice(0, 4).map((n, i) => (
            <li key={i} className="text-[11.5px] text-stone-500 flex items-start gap-1.5">
              <FileQuestion className="h-3 w-3 mt-0.5 shrink-0 text-amber-600" /> {n}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** the little file:line chip EVERY claim carries — the tag that stays */
function EvidenceChip({ rel, line }: { rel?: string | null; line?: number | null }) {
  const openSourceViewer = useCie((s) => s.openSourceViewer);
  if (!rel) return null;
  const short = rel.split("/").pop() ?? rel;
  return (
    <button
      type="button"
      onClick={() => openSourceViewer({ file: rel, line: line ?? undefined })}
      className="inline-flex items-center gap-0.5 mx-0.5 rounded bg-emerald-50 border border-emerald-200 px-1 py-0 text-[10px] font-semibold text-emerald-800 hover:bg-emerald-100 align-middle whitespace-nowrap"
      title={`Open ${rel}${line ? ` line ${line}` : ""} in the source panel`}
    >
      {short}
      {line ? `:${line}` : ""}
    </button>
  );
}

// ============================================================
// ② the mains graph
// ============================================================

function MainsGraph({ stack, hasReport }: { stack: TechStack | null; hasReport: boolean }) {
  const mains = stack?.mains ?? [];
  const jvm = stack?.jvm ?? null;

  if (mains.length > 0 && jvm) {
    const items: { label: string; sub: string; rel?: string; line?: number }[] = [];
    const cfg = stack?.config[0];
    if (cfg?.port) {
      items.push({ label: `listens on :${cfg.port}`, sub: cfg.rel.split("/").pop() ?? "", rel: cfg.rel });
    }
    if (jvm.scheduledIn.length > 0) {
      items.push({ label: `${jvm.scheduledIn.length} scheduled job(s)`, sub: jvm.scheduledIn.slice(0, 3).map((s) => `${s.cls}.${s.method}`).join(", ") + (jvm.scheduledIn.length > 3 ? ", …" : ""), rel: jvm.scheduledIn[0].rel, line: jvm.scheduledIn[0].line });
    }
    if (jvm.listeners.length > 0) {
      items.push({ label: `${jvm.listeners.length} listener(s)`, sub: jvm.listeners.slice(0, 3).map((s) => `${s.cls}.${s.method}${s.target ? ` → ${s.target}` : ""}`).join(", ") + (jvm.listeners.length > 3 ? ", …" : ""), rel: jvm.listeners[0].rel, line: jvm.listeners[0].line });
    }
    items.push({ label: `${stack?.routes.length ?? 0} HTTP route(s)`, sub: jvm.flows.map((f) => f.name).slice(0, 4).join(", "), rel: jvm.flows[0]?.rel });
    items.push({
      label: `${jvm.services.length} service(s) → ${jvm.repositories.length} repo(s)`,
      sub: jvm.entities.slice(0, 3).map((e) => e.name + (e.table ? ` (${e.table})` : "")).join(", ") || "—",
      rel: jvm.services[0]?.rel,
      line: jvm.services[0]?.line,
    });
    return <MainsTree mains={mains} items={items} langNote={jvm.javaFiles > 0 && jvm.kotlinFiles > 0 ? "one runtime, two syntaxes" : undefined} />;
  }

  return <EngineFallbackGraph hasReport={hasReport} />;
}

function MainsTree({
  mains,
  items,
  langNote,
}: {
  mains: TechStack["mains"];
  items: { label: string; sub: string; rel?: string; line?: number }[];
  langNote?: string;
}) {
  return (
    <div className="p-3">
      <div className="flex flex-col items-center gap-0">
        {/* the application class — the ONE boot point */}
        {mains.map((m) => (
          <div key={m.rel + m.name} className="flex flex-col items-center">
            <MainPill main={m} />
            <span className="w-px h-4 bg-stone-300" aria-hidden />
          </div>
        ))}
        {/* everything it boots */}
        <div className="grid sm:grid-cols-2 gap-2 w-full max-w-xl">
          {items.map((it, i) => (
            <div key={i} className="rounded-lg border border-stone-200 bg-white p-2.5 min-w-0">
              <p className="text-[12px] font-bold text-stone-800 truncate flex items-center gap-1">
                <Boxes className="h-3.5 w-3.5 text-emerald-700 shrink-0" /> {it.label}
              </p>
              <p className="text-[10.5px] text-stone-500 truncate mt-0.5">{it.sub || "—"}</p>
            </div>
          ))}
        </div>
      </div>
      {langNote && <p className="text-[11px] text-stone-400 text-center mt-2">☕ {langNote} — both syntaxes parsed into the same picture</p>}
      <p className="text-[10.5px] text-stone-400 text-center mt-1">Read from the code as written — click a claim&apos;s chip to see the file.</p>
    </div>
  );
}

function MainPill({ main }: { main: TechStack["mains"][number] }) {
  const openSourceViewer = useCie((s) => s.openSourceViewer);
  return (
    <button
      type="button"
      onClick={() => openSourceViewer({ file: main.rel, line: main.line })}
      className="rounded-xl border-2 border-emerald-600 bg-emerald-700 px-4 py-2 text-white shadow-sm hover:bg-emerald-800 text-center"
      title={`Open ${main.rel}`}
    >
      <span className="block text-[13px] font-bold">{main.name}</span>
      <span className="block text-[10px] opacity-80 font-mono">
        {main.rel.split("/").pop()}:{main.line} · @SpringBootApplication
      </span>
    </button>
  );
}

function EngineFallbackGraph({ hasReport }: { hasReport: boolean }) {
  const report = useCie((s) => s.report);
  const rows = useMemo(() => (report ? computeEntryPoints(report).slice(0, 6) : []), [report]);
  const openSourceViewer = useCie((s) => s.openSourceViewer);
  return (
    <div className="p-3">
      <p className="text-[12px] text-stone-600 mb-2">
        No application class parsed for this stack — the honest fallback is the engine&apos;s own reading of where the code starts: symbols nothing calls, that touch the outside world.
        {!hasReport && " (No engine report loaded.)"}
      </p>
      <div className="grid sm:grid-cols-2 gap-2">
        {rows.map((r, i) => (
          <EntryRow key={i} row={r} onClick={() => openSourceViewer({ file: r.file ?? "", byteStart: r.byteStart, byteEnd: r.byteEnd })} />
        ))}
      </div>
      {rows.length === 0 && hasReport && <p className="text-[11px] text-stone-400">The engine didn&apos;t record entry points for this project — the report&apos;s own numbers stay the only facts.</p>}
    </div>
  );
}

function EntryRow({ row, onClick }: { row: MasteryRow; onClick: () => void }) {
  const statusWord = row.status === "confirmed" ? "confirmed by the engine" : "inferred — stays tagged";
  return (
    <button type="button" onClick={onClick} className="text-left rounded-lg border border-stone-200 bg-white p-2.5 hover:border-emerald-300 hover:bg-emerald-50/40 min-w-0">
      <span className="flex items-center gap-1.5">
        <span className="text-[12px] font-semibold text-stone-900 truncate font-mono">{row.title}</span>
        {row.status === "confirmed" && <CheckCircle2 className="h-3.5 w-3.5 text-emerald-700 shrink-0" />}
      </span>
      <span className="block text-[10.5px] text-stone-500 truncate mt-0.5">
        {row.detail} · {statusWord}
      </span>
    </button>
  );
}

// ============================================================
// ③ the routes table
// ============================================================

function RoutesTable({ stack }: { stack: TechStack | null }) {
  const routes = stack?.routes ?? [];
  const openSourceViewer = useCie((s) => s.openSourceViewer);
  if (!stack) return <p className="p-3 text-[12px] text-stone-500">No stack report loaded — no routes to table.</p>;
  if (routes.length === 0) {
    return (
      <div className="p-3 space-y-1">
        <p className="text-[12px] text-stone-600">
          {stack.jvm
            ? "Spring is named in the build files, but no @RestController/@Controller endpoints were parsed in the scanned files — the table stays honestly empty rather than guessing."
            : "No framework routes parsed for this stack yet (the stub says exactly what CIE did and didn't read)."}
        </p>
        {stack.notes.filter((n) => !n.startsWith("[stack]") && n.includes("route")).slice(0, 1).map((n, i) => (
          <p key={i} className="text-[11px] text-stone-400">{n}</p>
        ))}
      </div>
    );
  }
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-[12px]">
        <thead className="text-[10px] text-stone-400 text-left border-b border-stone-100">
          <tr>
            <th className="py-1.5 px-3 font-bold">Verb</th>
            <th className="py-1.5 px-3 font-bold">Path</th>
            <th className="py-1.5 px-3 font-bold">Handler</th>
            <th className="py-1.5 px-3 font-bold">Params</th>
            <th className="py-1.5 px-3 font-bold">Returns</th>
            <th className="py-1.5 px-3 font-bold" />
          </tr>
        </thead>
        <tbody>
          {routes.map((r, i) => (
            <tr key={i} className="border-b border-stone-50 hover:bg-emerald-50/40">
              <td className="py-1.5 px-3">
                <span className={`rounded px-1.5 py-0.5 text-[10px] font-bold ${VERB_CLASS(r.verb)}`}>{r.verb}</span>
              </td>
              <td className="py-1.5 px-3 font-mono text-[11.5px] font-semibold" title={r.path}>{r.path}</td>
              <td className="py-1.5 px-3 text-[11.5px]">
                <span className="font-mono">{r.owner}</span>.<span className="font-semibold">{r.method}</span>()
              </td>
              <td className="py-1.5 px-3 text-[11px] text-stone-500" title={r.params.join(", ")}>
                {r.params.length ? r.params.join(", ") : "—"}
              </td>
              <td className="py-1.5 px-3 text-[11px] font-mono text-stone-600">{r.returns || "—"}</td>
              <td className="py-1.5 px-3">
                <EvidenceChip rel={r.rel} line={r.line} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="px-3 pb-2.5 pt-1.5 text-[10.5px] text-stone-400">
        {routes.length} route(s) as written in the annotations — every row carries its evidence; nothing inferred.
      </p>
    </div>
  );
}

function VERB_CLASS(verb: string): string {
  switch (verb) {
    case "GET": return "bg-sky-100 text-sky-800 border border-sky-200";
    case "POST": return "bg-emerald-100 text-emerald-800 border border-emerald-200";
    case "PUT": return "bg-amber-100 text-amber-800 border border-amber-200";
    case "DELETE": return "bg-rose-100 text-rose-800 border border-rose-200";
    case "PATCH": return "bg-violet-100 text-violet-800 border border-violet-200";
    default: return "bg-stone-100 text-stone-700 border border-stone-200";
  }
}

// ============================================================
// ④ the data-flow diagram — branch-capable
// ============================================================

const CANON_LANES = ["client", "controller", "service", "repository", "database", "response"] as const;
const LANE_COLORS: Record<string, string> = {
  client: "#57534e",
  controller: "#047857",
  service: "#1d4ed8",
  repository: "#6d28d9",
  database: "#b45309",
  response: "#0f766e",
};
const LANE_LABELS: Record<string, string> = {
  client: "client",
  controller: "controller",
  service: "service",
  repository: "repository",
  database: "database",
  response: "response",
};

type FlatRow =
  | { t: "arrow"; lane: string; to: string; label: string; sub?: string; kind: "call" | "throw" | "return" | "request"; node: FlowNode; armDepth: number }
  | { t: "self"; lane: string; label: string; node: FlowNode; armDepth: number }
  | { t: "decision"; lane: string; label: string; node: FlowNode; armDepth: number }
  | { t: "arm"; label: string; armDepth: number };

/// flatten a flow's body into lane rows — branches become `arm` rows whose
/// inner rows carry armDepth (one row per node, all evidence kept)
function flattenFlow(
  nodes: FlowNode[],
  lane: string,
  armDepth: number,
  out: FlatRow[],
  lanesUsed: Set<string>
) {
  for (const n of nodes) {
    if (out.length > 220) return; // hard cap, honest
    switch (n.kind) {
      case "decision": {
        out.push({ t: "decision", lane, label: n.label, node: n, armDepth });
        for (const child of n.children) {
          out.push({ t: "arm", label: child.label, armDepth });
          flattenFlow(child.nodes, lane, armDepth + 1, out, lanesUsed);
        }
        break;
      }
      case "throw": {
        lanesUsed.add("response");
        out.push({ t: "arrow", lane, to: "response", label: n.label, kind: "throw", node: n, armDepth });
        break;
      }
      case "return": {
        lanesUsed.add("response");
        out.push({ t: "arrow", lane, to: "response", label: n.label, kind: "return", node: n, armDepth });
        break;
      }
      case "request": {
        lanesUsed.add("client");
        lanesUsed.add("controller");
        out.push({ t: "arrow", lane: "client", to: "controller", label: n.label, kind: "request", node: n, armDepth });
        // the handler chain continues: request → controller → body …
        for (const child of n.children) {
          for (const sub of child.nodes) {
            if (sub.kind === "action" || sub.kind === "other") {
              // the controller's own action — a self chip, then its body
              out.push({ t: "self", lane: "controller", label: sub.label, node: sub, armDepth });
              for (const sub2 of sub.children) {
                flattenFlow(sub2.nodes, "controller", armDepth, out, lanesUsed);
              }
            } else {
              flattenFlow([sub], "controller", armDepth, out, lanesUsed);
            }
          }
        }
        break;
      }
      default: {
        // action — self chip when unresolvable ("other") or same lane
        const laneable = CANON_LANES.includes(n.layer as (typeof CANON_LANES)[number]) && n.layer !== lane && n.layer !== "other";
        if (laneable) {
          lanesUsed.add(n.layer);
          out.push({ t: "arrow", lane, to: n.layer, label: n.label, kind: "call", node: n, armDepth });
          const into = n.children[0];
          if (into) {
            flattenFlow(into.nodes, n.layer, armDepth, out, lanesUsed);
          }
        } else {
          out.push({ t: "self", lane, label: n.label, node: n, armDepth });
          const into = n.children[0];
          if (into) {
            flattenFlow(into.nodes, lane, armDepth, out, lanesUsed);
          }
        }
      }
    }
  }
}

const ARM_INDENT = 22;
const ROW_H = 46;

function FlowDiagram({ flow }: { flow: FlowNode }) {
  const openSourceViewer = useCie((s) => s.openSourceViewer);
  const uid = useSafeId();

  const rows = useMemo(() => {
    const out: FlatRow[] = [];
    const lanes = new Set<string>(["client", "controller"]);
    flattenFlow([flow], "client", 0, out, lanes);
    return { rows: out.slice(0, 220), lanes: [...lanes] };
  }, [flow]);

  const lanes = useMemo(
    () => CANON_LANES.filter((l) => rows.lanes.includes(l)),
    [rows.lanes]
  );
  const laneOf = (name: string) => 96 + (lanes.indexOf(name as (typeof CANON_LANES)[number]) * (720 - 96)) / Math.max(1, lanes.length - 1);

  const H = 64 + rows.rows.length * ROW_H + 26;
  return (
    <svg role="img" aria-label="Data flow of this route" viewBox={`0 0 820 ${H}`} width="100%" className="overflow-visible">
      <defs>
        <marker id={`${uid}-arrow`} viewBox="0 0 10 10" refX={9} refY={5} markerWidth={7} markerHeight={7} orient="auto-start-reverse">
          <path d="M0,0L10,5L0,10z" fill="#57534e" />
        </marker>
      </defs>
      {/* lifelines */}
      {lanes.map((l) => (
        <line key={`ll-${l}`} x1={laneOf(l)} y1={40} x2={laneOf(l)} y2={H - 14} stroke="#e7e5e4" strokeWidth={2} />
      ))}
      {lanes.map((l) => (
        <g key={`lb-${l}`}>
          <rect x={laneOf(l) - 46} y={6} width={92} height={24} rx={7} fill={LANE_COLORS[l] ?? "#78716c"} />
          <text x={laneOf(l)} y={22} textAnchor="middle" fontSize={11} fill="#fff" fontWeight={700}>
            {LANE_LABELS[l] ?? l}
          </text>
        </g>
      ))}
      {rows.rows.map((r, i) => renderRow(r, i))}
    </svg>
  );

  function renderRow(r: FlatRow, i: number): React.ReactNode {
    // arms render a full-width label chip; arrows shift inside arms
    const y = 58 + i * ROW_H;
    const indent = r.t === "arrow" || r.t === "self" || r.t === "decision" ? r.armDepth * ARM_INDENT : 0;
    switch (r.t) {
      case "arm":
        return (
          <g key={`r-${i}`}>
            <rect x={10 + r.armDepth * ARM_INDENT} y={y - 14} width={r.label.length * 6.1 + 26} height={20} rx={5} fill="#fef3c7" stroke="#fcd34d" />
            <text x={22 + r.armDepth * ARM_INDENT} y={y} fontSize={10.5} fontWeight={700} fill="#92400e">
              {r.label.length > 40 ? r.label.slice(0, 39) + "…" : r.label}
            </text>
          </g>
        );
      case "decision":
        return (
          <g key={`r-${i}`} className="cursor-pointer" onClick={() => clickSrc(r.node)}>
            <rect x={laneOf(r.lane) - 34} y={y - 12} width={68} height={24} rx={4} fill="#fffbeb" stroke="#d97706" transform={`translate(${indent},0)`} />
            <text x={laneOf(r.lane) + indent} y={y + 4} textAnchor="middle" fontSize={9.5} fontWeight={800} fill="#b45309">
              ◇
            </text>
            <text x={(laneOf(r.lane) + indent - 34) + 82} y={y + 4} fontSize={11} fontWeight={600} fill="#92400e">
              {r.label.length > 52 ? r.label.slice(0, 51) + "…" : r.label}
            </text>
          </g>
        );
      case "self":
        return (
          <g key={`r-${i}`} className="cursor-pointer" onClick={() => clickSrc(r.node)}>
            <rect x={laneOf(r.lane) + indent - 74} y={y - 13} width={148} height={26} rx={13} fill="#f5f5f4" stroke="#d6d3d1" />
            <text x={laneOf(r.lane) + indent} y={y + 4} textAnchor="middle" fontSize={10.5} fontWeight={600} fill="#57534e">
              {r.label.length > 22 ? r.label.slice(0, 21) + "…" : r.label}
            </text>
          </g>
        );
      case "arrow":
      default: {
        const x1 = laneOf(r.lane) + indent;
        const x2 = laneOf(r.to) + indent;
        const color = r.kind === "throw" ? "#e11d48" : r.kind === "return" ? "#0f766e" : LANE_COLORS[r.to] ?? "#57534e";
        const lbl = r.label.length > 58 ? r.label.slice(0, 57) + "…" : r.label;
        return (
          <g key={`r-${i}`} className={r.node.rel ? "cursor-pointer" : ""} onClick={() => clickSrc(r.node)}>
            <text x={(x1 + x2) / 2} y={y - 4} textAnchor="middle" fontSize={11} fontWeight={600} fill={color}>
              {lbl}
            </text>
            {r.node.rel && r.node.line && (
              <text x={(x1 + x2) / 2} y={y + 22} textAnchor="middle" fontSize={9.5} fill="#a8a29e" fontWeight={600}>
                {shortFile(r.node.rel)}:{r.node.line} ↗
              </text>
            )}
            <line x1={x1} y1={y} x2={x2} y2={y} stroke={color} strokeWidth={2.2} strokeDasharray={r.kind === "return" ? "6 3" : undefined} markerEnd={`url(#${uid}-arrow)`} />
          </g>
        );
      }
    }
  }

  function clickSrc(n: FlowNode) {
    if (!n.rel) return;
    openSourceViewer({ file: n.rel, line: n.line ?? undefined });
  }
}

function shortFile(rel: string): string {
  const base = rel.split("/").pop() ?? rel;
  return base.length > 26 ? base.slice(0, 24) + "…" : base;
}

function useSafeId(): string {
  const [id] = useState(() => `fd${Math.random().toString(36).slice(2, 8)}`);
  return id;
}

// ============================================================
// the view — strictly four sections
// ============================================================

export function MasteryView() {
  const source = useCie((s) => s.source);
  const label = useCie((s) => s.label);
  const loadMastery = useCie((s) => s.loadMastery);
  const report = useCie((s) => s.report);
  const stack = useCie((s) => s.stack);
  const setPendingAsk = useCie((s) => s.setPendingAsk);
  const [pickedController, setPickedController] = useState(0);
  const [pickedFlow, setPickedFlow] = useState(0);

  // (re)load whenever the loaded project changes — one effect, local, explicit
  useEffect(() => {
    loadMastery();
  }, [source, label, loadMastery]);

  const noProject = !report;
  const controllers = stack?.jvm?.flows ?? [];
  const safeController = Math.min(pickedController, Math.max(0, controllers.length - 1));
  const flowList = controllers[safeController]?.flows ?? [];
  const safeFlow = Math.min(pickedFlow, Math.max(0, flowList.length - 1));
  const picked = flowList[safeFlow] ?? null;

  return (
    <div className="max-w-4xl mx-auto space-y-5" aria-label="Master this codebase">
      {/* ---- header ---- */}
      <div className="flex items-start gap-3 flex-wrap">
        <div className="flex-1 min-w-0">
          <h2 className="text-xl font-bold text-stone-900 flex items-center gap-2">
            <span className="h-7 w-7 rounded-lg bg-emerald-700 text-white grid place-items-center shrink-0">
              <MapIcon className="h-4 w-4" />
            </span>
            Master this codebase
          </h2>
          <p className="text-xs text-stone-500 mt-1 truncate">
            {label}
            {report ? ` · ${report.files.length} files · ${report.symbols.length} named things` : ""} — read from your code, tagged where inferred.
          </p>
        </div>
      </div>

      {/* ---- ① the paragraph ---- */}
      <Section n={1} icon={<MapIcon className="h-3.5 w-3.5" />} title="What this project is" sub="Everything sourced: the stack pass + the engine's counts.">
        <div className="p-3.5">
          <StackParagraph stack={stack} hasReport={!!report} noProject={noProject} />
        </div>
      </Section>

      {/* ---- ② the mains graph ---- */}
      <Section n={2} icon={<Boxes className="h-3.5 w-3.5" />} title="Where it starts" sub="The boot point and everything it switches on — or, when the stack isn't parsed yet, the engine's own entry points.">
        <MainsGraph stack={stack} hasReport={!!report} />
      </Section>

      {/* ---- ③ the routes table ---- */}
      <Section n={3} icon={<RouteIcon className="h-3.5 w-3.5" />} title="Every route in, one table" sub="verb · path · handler · params → returns · the file that says so.">
        <RoutesTable stack={stack} />
      </Section>

      {/* ---- ④ the data-flow diagram ---- */}
      <Section n={4} icon={<Workflow className="h-3.5 w-3.5" />} title="How a request flows — end to end" sub="Pick a route; follow it through service → repository → database → response, branches included.">
        <FlowPicker controllers={controllers} controllerIdx={safeController} onController={(i) => { setPickedController(i); setPickedFlow(0); }} flowIdx={safeFlow} onFlow={setPickedFlow} />
        {picked ? (
          <div className="p-3 pt-0">
            <FlowDiagram flow={picked.root} />
            <p className="text-[10.5px] text-stone-400 mt-1 leading-relaxed">⚠ Honest limits — {picked.honest}</p>
          </div>
        ) : (
          <div className="p-3 pt-0">
            <p className="text-[12px] text-stone-600">
              {stack
                ? stack.jvm
                  ? "No handler flows were parsed — the routes table above is where every claim stops, with its evidence."
                  : "No data-flow parser for this stack yet — the sections above carry the honest read instead."
                : "Connect the project (or load the sample) to get the flows."}
            </p>
          </div>
        )}
      </Section>

      <p className="text-[10.5px] text-stone-400 px-1">
        Everything here was read from the project&apos;s own files — conditions quoted, never evaluated (CIE doesn&apos;t run the code).{" "}
        <button type="button" onClick={() => setPendingAsk("What is this system? Summarize it in plain words.")} className="underline text-emerald-700 hover:text-emerald-900">
          Ask CIE for the whole-system summary in chat
        </button>{" "}
        if you want the prose tour.
      </p>
    </div>
  );
}

function FlowPicker({
  controllers,
  controllerIdx,
  onController,
  flowIdx,
  onFlow,
}: {
  controllers: { name: string; flows: { method: string; verb: string; path: string }[] }[];
  controllerIdx: number;
  onController: (i: number) => void;
  flowIdx: number;
  onFlow: (j: number) => void;
}) {
  if (controllers.length === 0) return null;
  const flows = controllers[Math.min(controllerIdx, controllers.length - 1)]?.flows ?? [];
  return (
    <div className="px-3 py-2.5 space-y-1.5">
      <div className="flex items-center gap-1.5 flex-wrap">
        {controllers.map((c, i) => (
          <button
            key={i}
            type="button"
            onClick={() => onController(i)}
            className={`h-7 px-2.5 rounded-full text-[11px] font-bold font-mono border transition-colors ${i === controllerIdx ? "bg-emerald-700 text-white border-emerald-700" : "bg-white text-stone-600 border-stone-300 hover:border-emerald-400"}`}
          >
            {c.name}
          </button>
        ))}
      </div>
      <div className="flex items-center gap-1.5 flex-wrap">
        {flows.map((f, j) => (
          <button
            key={j}
            type="button"
            onClick={() => onFlow(j)}
            className={`h-7 px-2.5 rounded-full text-[10.5px] font-bold font-mono border transition-colors ${j === flowIdx ? "bg-stone-900 text-white border-stone-900" : "bg-white text-stone-600 border-stone-300 hover:border-stone-500"}`}
          >
            <span className={f.verb === "GET" ? "text-sky-600" : j === flowIdx ? "" : "text-stone-400"}>
              {j === flowIdx ? "" : ""}
            </span>
            {f.verb} {f.path}
          </button>
        ))}
      </div>
    </div>
  );
}

function Section({ n, icon, title, sub, children }: { n: number; icon: React.ReactNode; title: string; sub: string; children: React.ReactNode }) {
  return (
    <section className="rounded-2xl border border-stone-200 bg-white overflow-hidden" aria-label={title}>
      <div className="flex items-center gap-2.5 px-3.5 py-2.5 border-b border-stone-100 flex-wrap">
        <span className="h-6 w-6 rounded-full bg-stone-900 text-white text-[11px] font-bold grid place-items-center shrink-0">{n}</span>
        <span className="text-emerald-700">{icon}</span>
        <h3 className="text-[14.5px] font-bold text-stone-900">{title}</h3>
        <span className="text-[11.5px] text-stone-500 hidden md:inline">{sub}</span>
      </div>
      {children}
    </section>
  );
}