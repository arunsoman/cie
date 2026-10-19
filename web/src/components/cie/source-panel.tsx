"use client";

// The source panel — "click a chart element → see the code it came from".
//
// The engine exports always carried the link (quality.json has flat
// file+line per finding; report.json has symbols[].loc byte offsets plus a
// files[] table); the charts used to drop it one line before rendering. Now
// a tapped element with a location opens THIS panel:
//   - demo project  → the source bundle shipped with it (public/data/sources.json)
//   - local folder  → read in place via POST /api/fs/file — the folder the
//                     user themselves connected, read-only and bounded
//   - uploaded JSON / URL clones → honest empty state (no source on disk)
// Byte offsets are resolved to line numbers ONLY when real content is in
// hand — the panel never guesses a line number.
//
// Everything shown is exactly what's stored; nothing is edited, nothing is
// sent anywhere. The "Ask CIE about this" button re-uses the click-to-chat
// path, with the location attached so the answer is grounded in this code.

import { useCallback, useEffect, useRef, useState } from "react";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import oneLight from "react-syntax-highlighter/dist/esm/styles/prism/one-light";
import { FileCode2, MessageCircleQuestion, ShieldCheck, X } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { askAboutChartElement, snippetAround } from "@/lib/quality";
import { BlastRadiusButton } from "@/components/cie/mastery-cards";

// extension → prism language (kept honest: anything unlisted renders plain)
const LANG: Record<string, string> = {
  py: "python", sql: "sql", sh: "bash", bash: "bash",
  js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", tsx: "typescript",
  kt: "kotlin", kts: "kotlin", lua: "lua", rs: "rust", java: "java", go: "go",
  c: "c", h: "c", cpp: "cpp", hpp: "cpp", cc: "cpp", cs: "csharp", php: "php", rb: "ruby",
  md: "markdown", json: "json", toml: "ini", yml: "yaml", yaml: "yaml",
  html: "markup", xml: "markup", css: "css",
};
const langFor = (file: string) => LANG[file.split(".").pop()?.toLowerCase() ?? ""] ?? "";

/** UTF-8 byte offset → 0-based line index. The client-side twin of the
 * engine's FileTable::line_of — built from the real content, never guessed. */
function lineOfByte(content: string, byteOffset: number): number {
  const enc = new TextEncoder();
  const lines = content.split("\n");
  let acc = 0;
  for (let i = 0; i < lines.length; i++) {
    const span = enc.encode(lines[i]).length + 1; // +1 for the newline
    if (byteOffset < acc + span) return i;
    acc += span;
  }
  return Math.max(0, lines.length - 1);
}

// one lazy fetch of the demo source bundle, shared across opens — per sample kind
const demoSourcesByKind: Partial<Record<"banking" | "spring" | "polyglot", Promise<Record<string, string>>>> = {};
function loadDemoSources(kind: "banking" | "spring" | "polyglot"): Promise<Record<string, string>> {
  if (!demoSourcesByKind[kind]) {
    const url = kind === "spring" ? "/data/spring/sources.json" : kind === "polyglot" ? "/data/polyglot/sources.json" : "/data/sources.json";
    demoSourcesByKind[kind] = fetch(url)
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(String(r.status)))))
      .then((j: { files?: Record<string, string> }) => j.files ?? {})
      .catch(() => ({}));
  }
  return demoSourcesByKind[kind];
}

const WINDOW = 120; // lines of context shown around the target

export function SourcePanel() {
  const { sourceViewer, sourceRoot, source, demoKind, sourceCache, cacheSource, closeSourceViewer, setPendingAsk } = useCie();
  const [state, setState] = useState<
    | { kind: "loading" }
    | { kind: "ready"; content: string; from: string; line: number | null; truncated: boolean; totalLines: number | null }
    | { kind: "unavailable"; reason: string }
  >({ kind: "loading" });
  const bodyRef = useRef<HTMLDivElement>(null);

  const req = sourceViewer;

  // Reset to "loading" the render-phase way (the effect below only fills
  // state asynchronously) whenever a new element is opened.
  const [renderedReq, setRenderedReq] = useState(req);
  if (renderedReq !== req) {
    setRenderedReq(req);
    setState({ kind: "loading" });
  }

  const resolveLine = useCallback(
    (content: string): number | null => {
      if (!req) return null;
      if (req.line != null && req.line >= 1) return req.line;
      if (req.byteStart != null && req.byteStart >= 0) return lineOfByte(content, req.byteStart) + 1;
      return null;
    },
    [req]
  );

  useEffect(() => {
    if (!req) return;
    let dead = false;

    const finish = (content: string, from: string, totalLines: number | null, truncated: boolean) => {
      if (dead) return;
      cacheSource(req.file, content);
      setState({ kind: "ready", content, from, line: resolveLine(content), truncated, totalLines });
    };

    if (sourceRoot) {
      fetch("/api/fs/file", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ root: sourceRoot, path: req.file, all: true }),
      })
        .then((r) => r.json().catch(() => ({})))
        .then((d: { content?: string; error?: string; totalLines?: number; truncated?: boolean }) => {
          if (dead) return;
          if (d.content != null) finish(d.content, `read from ${sourceRoot} on this computer`, d.totalLines ?? null, d.truncated ?? false);
          else setState({ kind: "unavailable", reason: d.error ?? "We couldn't read that file from your project folder." });
        })
        .catch(() => {
          if (!dead) setState({ kind: "unavailable", reason: "The connection to your project folder dropped. Please try again." });
        });
    } else if (source === "demo") {
      loadDemoSources(demoKind ?? "polyglot").then((files) => {
        if (dead) return;
        const content = files[req.file];
        if (content != null) finish(content, "bundled with the sample project", content.split("\n").length, false);
        else setState({ kind: "unavailable", reason: "That file isn't part of the bundled sample sources." });
      });
    } else {
      // uploads / URL-cloned projects: no source on disk — said honestly
      void Promise.resolve().then(() => {
        if (dead) return;
        setState({
          kind: "unavailable",
          reason:
            "The source files aren't available for this project — only the indexed facts are loaded. Connect your project folder (Use your own → a folder on this computer) and the code appears here.",
        });
      });
    }
    return () => {
      dead = true;
    };
  }, [req, sourceRoot, source, cacheSource, resolveLine]);

  // Escape closes — keyboard parity with every other panel
  useEffect(() => {
    if (!req) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeSourceViewer();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [req, closeSourceViewer]);

  // scroll the highlighted line into view once rendered
  useEffect(() => {
    if (state.kind !== "ready" || state.line == null) return;
    const t = setTimeout(() => {
      bodyRef.current?.querySelector(`[data-line="${state.line}"]`)?.scrollIntoView({ block: "center" });
    }, 60);
    return () => clearTimeout(t);
  }, [state]);

  if (!req || !req.file) return null;

  const totalFromFile = state.kind === "ready" ? state.content.split("\n").length : 0;
  const target = state.kind === "ready" ? state.line : null;
  const winStart = target != null ? Math.max(1, target - Math.floor(WINDOW / 2)) : 1;
  const winEnd = target != null ? Math.min(totalFromFile, winStart + WINDOW - 1) : totalFromFile;
  const shown = state.kind === "ready" ? state.content.split("\n").slice(winStart - 1, winEnd).join("\n") : "";
  const lang = langFor(req.file);

  const ask = () => {
    if (!req.elName && !req.chartTitle) return;
    // ride the real code along when we hold it — verbatim, line-numbered,
    // so the chat answer is grounded in exactly what is on screen
    const cached = sourceCache[req.file];
    const line = target ?? req.line;
    const snippet = cached ? snippetAround(cached, line) : undefined;
    setPendingAsk(
      askAboutChartElement(req.chartTitle, {
        name: req.elName ?? req.file,
        value: req.elValue,
        loc: { file: req.file, line },
        snippet,
      })
    );
    closeSourceViewer();
  };

  return (
    <div
      role="dialog"
      aria-label={`Source code of ${req.file}`}
      className="fixed z-50 flex flex-col rounded-2xl border border-stone-200 bg-white shadow-2xl
                 right-3 top-16 bottom-24 w-[min(780px,calc(100vw-1.5rem))] max-lg:left-3 max-lg:w-auto overflow-hidden"
    >
      {/* header */}
      <div className="flex items-start gap-2 px-4 py-3 border-b border-stone-200 bg-stone-50/80">
        <FileCode2 className="h-4 w-4 mt-0.5 text-emerald-700 shrink-0" />
        <div className="min-w-0 flex-1">
          <p className="font-mono text-[13px] font-semibold text-stone-900 truncate" title={req.file}>
            {req.file}
            {target != null && <span className="ml-2 rounded-full bg-emerald-50 border border-emerald-200 px-2 py-0.5 text-[10px] font-sans font-semibold text-emerald-800">line {target}</span>}
          </p>
          {req.elName && (
            <p className="text-[11px] text-stone-500 mt-0.5 truncate">
              opened from the chart element <span className="font-medium text-stone-700">“{req.elName}”</span>
              {req.chartTitle ? ` (${req.chartTitle})` : ""}
            </p>
          )}
        </div>
        <button
          type="button"
          onClick={closeSourceViewer}
          aria-label="Close the source panel"
          className="h-7 w-7 grid place-items-center rounded-lg text-stone-400 hover:bg-stone-100 hover:text-stone-700 shrink-0"
        >
          <X className="h-4 w-4" />
        </button>
      </div>

      {/* body */}
      <div ref={bodyRef} className="flex-1 min-h-0 overflow-auto bg-[#fafafa]">
        {state.kind === "loading" && (
          <p className="p-6 text-sm text-stone-500" role="status">
            Reading the file…
          </p>
        )}
        {state.kind === "unavailable" && (
          <div className="p-6">
            <p className="text-sm text-stone-700 leading-relaxed">{state.reason}</p>
            <p className="mt-3 text-[11px] text-stone-400 flex items-start gap-1.5">
              <ShieldCheck className="h-3.5 w-3.5 shrink-0 mt-px" />
              CIE never pretends to have code it can&apos;t actually read — the indexed facts stay honest without it.
            </p>
          </div>
        )}
        {state.kind === "ready" && (
          <SyntaxHighlighter
            language={lang || undefined}
            style={oneLight}
            showLineNumbers
            startingLineNumber={winStart}
            wrapLines
            lineProps={(ln: number) =>
              ln === target
                ? { "data-line": ln, style: { display: "block", background: "#fef3c7", outline: "1px solid #fcd34d" } as React.CSSProperties }
                : { "data-line": ln, style: { display: "block" } as React.CSSProperties }
            }
            customStyle={{ margin: 0, background: "transparent", fontSize: 12.5, lineHeight: 1.55 }}
            codeTagProps={{ style: { fontFamily: "var(--font-mono, ui-monospace, monospace)" } }}
          >
            {shown}
          </SyntaxHighlighter>
        )}
      </div>

      {/* footer — honesty note + the grounded ask + the blast radius (Phase-5 habit: before you change it) */}
      <div className="px-4 py-3 border-t border-stone-200 bg-white flex items-center justify-between gap-3 flex-wrap">
        <p className="text-[10.5px] text-stone-400 leading-snug min-w-0">
          {state.kind === "ready" ? (
            <>
              {state.from}
              {state.truncated ? " · windowed, not the whole file" : ""}
              {state.totalLines != null ? ` · ${state.totalLines} lines total` : ""} · read-only, nothing is changed
            </>
          ) : (
            "shown exactly as collected — nothing added"
          )}
        </p>
        <span className="flex items-center gap-2 shrink-0 relative">
          <BlastRadiusButton symbolName={req.elName} file={req.file} byteStart={req.byteStart} />
          {(req.elName || req.chartTitle) && (
            <button
              type="button"
              onClick={ask}
              disabled={state.kind !== "ready"}
              className="h-8 rounded-xl border border-emerald-200 bg-emerald-50 px-3 text-xs font-medium text-emerald-900 hover:bg-emerald-100 disabled:opacity-50 inline-flex items-center gap-1.5"
            >
              <MessageCircleQuestion className="h-3.5 w-3.5" /> Ask CIE about this
            </button>
          )}
        </span>
      </div>
    </div>
  );
}
