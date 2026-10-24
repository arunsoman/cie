"use client";
import { readCapturedFile } from "@/lib/storage-ui";

// RFC-0024 stage 3 — the code viewer popup (ux-spec §2 single + §2.2
// paired). Replaces jump-to-source as the FIRST response to a click that
// resolves to a file/function/line: an anchored card near the clicked
// element (centered modal when there is no room), Esc closes, Enter opens
// the full source panel (the popup is a preview, the panel is the
// territory). The popup invents nothing: the same /api/fs + report locs
// the full panel reads.
//
// §2.3: the chip was ALREADY added by the click itself (auto-chip, stage
// 4); the popup's toggle button is the REMOVAL affordance and says so.

import { useEffect, useMemo, useRef, useState } from "react";
import { Check, Copy, CornerDownLeft, FileCode, Loader2, X } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { loadDemoSources } from "./source-panel";

export interface PopupLoc {
  file: string;
  byteStart?: number;
  byteEnd?: number;
  line?: number;
}

export interface PopupRequest {
  /** The primary location (the element's own evidence). */
  primary: PopupLoc;
  /** Header text: qual_name or the row's natural name. */
  label: string;
  /** Plain-words element kind ("component", "route", "decision"…). */
  kind?: string;
  /** The recorded certainty chip: prowen/good guess/not sure. */
  certainty?: "Fact" | "Inference" | "Hypothesis";
  /** §2.2 — the SECOND recorded location (drift doc↔code, dep from↔to). */
  also?: (PopupLoc & { side: "DOC" | "CODE" | "FROM" | "TO"; note?: string }) | null;
  /** The status chip for the paired code column (drift verification). */
  alsoStatus?: string;
  /** Chip state + the removal toggle (§2.3) — null when chips are off. */
  chipped?: boolean;
  onToggleChip?: () => void;
  onAsk?: () => void;
}

interface Loaded {
  content: string;
  from: string;
}

function PopupColumn({
  loc,
  side,
  status,
  certainty,
}: {
  loc: PopupLoc;
  side: string;
  status?: string;
  certainty?: string;
}) {
  const { sourceRoot: liveSourceRoot, cacheSource, demoKind } = useCie();
  const sourceRoot = useCie((s) => s.storageRoot) ?? liveSourceRoot;
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const source = useCie((s) => s.source);

  useEffect(() => {
    let dead = false;
    if (sourceRoot) {
      readCapturedFile(sourceRoot, loc.file)
        .then((d: { content?: string; error?: string }) => {
          if (dead) return;
          if (d.content != null) {
            cacheSource(loc.file, d.content);
            setLoaded({ content: d.content, from: `captured source from the loaded repository revision` });
          } else setFailed(d.error ?? "couldn't read that file");
        })
        .catch(error => !dead && setFailed(error instanceof Error?error.message:"Captured source unavailable"));
    } else if (source === "demo") {
      void loadDemoSources(demoKind ?? "polyglot").then((files) => {
        if (dead) return;
        const content = files[loc.file];
        if (content != null) setLoaded({ content, from: "bundled with the sample project" });
        else setFailed("not part of the bundled sample sources");
      });
    } else {
      setFailed("No captured source is available for this imported workspace. Reconnect the repository to enable source retrieval.");
    }
    return () => {
      dead = true;
    };
  }, [loc.file, sourceRoot, source, demoKind, cacheSource]);

  // Evidence lines = the recorded byte range, resolved to line numbers;
  // ±8 context lines around them, styled apart (context is never fact).
  const { lines, evidenceSet, startNo } = useMemo(() => {
    if (!loaded) return { lines: [] as string[], evidenceSet: new Set<number>(), startNo: 1 };
    const all = loaded.content.split("\n");
    const starts = loc.line
      ? [loc.line]
      : loc.byteStart != null
        ? lineOfByte(loaded.content, loc.byteStart)
        : [];
    let lo = Math.max(1, (starts[0] ?? 1) - 8);
    let hi = Math.min(all.length, (starts[0] ?? 1) + 8);
    if (loc.byteStart != null && loc.byteEnd != null && loc.byteEnd > loc.byteStart) {
      const first = lineOfByte(loaded.content, loc.byteStart)[0] ?? 1;
      const last = lineOfByte(loaded.content, loc.byteEnd)[0] ?? first;
      lo = Math.max(1, first - 8);
      hi = Math.min(all.length, last + 2);
    }
    const ev = new Set<number>();
    if (loc.byteStart != null && loc.byteEnd != null) {
      const first = lineOfByte(loaded.content, loc.byteStart)[0] ?? 0;
      const last = lineOfByte(loaded.content, loc.byteEnd)[0] ?? first;
      for (let i = first; i <= last; i++) ev.add(i);
    } else if (starts[0]) ev.add(starts[0]);
    return { lines: all.slice(lo - 1, hi), evidenceSet: ev, startNo: lo };
  }, [loaded, loc.line, loc.byteStart, loc.byteEnd]);

  const copyPath = (line: number) => {
    const text = `${loc.file}:${line}`;
    void navigator.clipboard?.writeText(text);
    setCopied(text);
    window.setTimeout(() => setCopied(null), 1400);
  };

  const certaintyColor =
    certainty === "Fact" ? "#059669" : certainty === "Hypothesis" ? "#e11d48" : "#d97706";

  return (
    <div className="min-w-0 flex-1">
      <div className="flex items-center gap-1.5 border-b border-stone-200 px-3 py-2">
        <FileCode className="h-3.5 w-3.5 shrink-0 text-stone-400" />
        <span className="truncate text-[11px] font-medium text-stone-700" title={loc.file}>
          {loc.file}
        </span>
        <span className="ml-auto flex items-center gap-1">
          {side && (
            <span className="rounded bg-stone-100 px-1 py-0.5 text-[9px] font-semibold tracking-wide text-stone-500">
              {side}
            </span>
          )}
          {status && <span className="text-[10px] text-stone-500">{status}</span>}
          {certainty && (
            <span className="rounded-full px-1.5 py-0.5 text-[9px] font-semibold text-white" style={{ background: certaintyColor }}>
              {certainty}
            </span>
          )}
        </span>
      </div>
      {failed ? (
        <p className="px-3 py-4 text-[11px] text-stone-500">{failed}</p>
      ) : !loaded ? (
        <p className="flex items-center gap-1.5 px-3 py-4 text-[11px] text-stone-400">
          <Loader2 className="h-3 w-3 animate-spin" /> reading {loc.file}…
        </p>
      ) : (
        <div className="max-h-72 overflow-auto px-1 py-1.5">
          {lines.map((text, i) => {
            const no = startNo + i;
            const isEv = evidenceSet.has(no);
            return (
              <div
                key={no}
                onClick={() => copyPath(no)}
                title={`copy ${loc.file}:${no}`}
                className={`flex cursor-pointer items-start gap-2 rounded px-1.5 py-[1px] font-mono text-[10.5px] leading-[1.5] ${
                  isEv ? "bg-emerald-50" : "text-stone-400"
                } hover:bg-stone-100`}
              >
                <span className={`w-8 shrink-0 select-none text-right ${isEv ? "text-emerald-700" : "text-stone-300"}`}>{no}</span>
                <span className={`min-w-0 whitespace-pre-wrap break-all ${isEv ? "text-stone-800" : ""}`}>{text || " "}</span>
              </div>
            );
          })}
        </div>
      )}
      {copied && (
        <p className="flex items-center gap-1 px-3 pb-1.5 text-[10px] text-emerald-700">
          <Check className="h-3 w-3" /> copied {copied}
        </p>
      )}
    </div>
  );
}

/** 1-based line number of a 0-based byte offset. */
function lineOfByte(content: string, byte: number): number[] {
  const upto = content.slice(0, Math.max(0, Math.min(byte, content.length)));
  return [upto.split("\n").length];
}

export function CodePopup({ req, onClose, onOpenFull }: {
  req: PopupRequest;
  onClose: () => void;
  onOpenFull: (loc: PopupLoc) => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const { sourceRoot: liveSourceRoot } = useCie();
  const sourceRoot = useCie((s) => s.storageRoot) ?? liveSourceRoot;

  // Esc closes; Enter opens the full source (primary column).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
      } else if (e.key === "Enter") {
        e.preventDefault();
        onOpenFull(req.primary);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [onClose, onOpenFull, req.primary]);

  // click-outside closes
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [onClose]);

  const pathLine = `${req.primary.file}${req.primary.line ? `:${req.primary.line}` : ""}`;
  const paired = !!req.also;

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center bg-stone-900/10 p-4" role="dialog" aria-modal="true" aria-label={`Code preview — ${req.label}`}>
      <div
        ref={ref}
        className="w-full max-w-[560px] overflow-hidden rounded-xl border border-stone-200 bg-white shadow-xl"
        onKeyDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-start gap-2 border-b border-stone-100 px-3 py-2">
          <div className="min-w-0 flex-1">
            <p className="truncate text-xs font-semibold text-stone-900">{req.label}</p>
            <p className="text-[10px] text-stone-400">
              {req.kind ? `${req.kind} · ` : ""}recorded evidence, ±8 context lines plainly styled
            </p>
          </div>
          <button type="button" onClick={onClose} aria-label="Close (Esc)" className="rounded p-1 text-stone-400 hover:bg-stone-100">
            <X className="h-3.5 w-3.5" />
          </button>
        </div>

        <div className={`flex ${paired ? "flex-col sm:flex-row sm:divide-x sm:divide-stone-100" : ""}`}>
          {paired && req.also ? (
            <>
              <PopupColumn loc={req.also} side={req.also.side} certainty="Fact" />
              <PopupColumn
                loc={req.primary}
                side={req.also.side === "DOC" ? "CODE" : "TO"}
                status={req.alsoStatus}
                certainty={req.certainty}
              />
            </>
          ) : (
            <PopupColumn loc={req.primary} side="" certainty={req.certainty} />
          )}
        </div>

        <div className="flex flex-wrap items-center gap-1.5 border-t border-stone-100 px-3 py-2">
          {req.onToggleChip && (
            <button
              type="button"
              onClick={req.onToggleChip}
              className={`rounded-md px-2 py-1 text-[10.5px] font-medium ${
                req.chipped ? "bg-emerald-50 text-emerald-800" : "bg-stone-100 text-stone-600 hover:bg-stone-200"
              }`}
            >
              {req.chipped ? "In context ✓ (click to remove)" : "Add to context"}
            </button>
          )}
          {req.onAsk && (
            <button type="button" onClick={req.onAsk} className="rounded-md bg-stone-100 px-2 py-1 text-[10.5px] font-medium text-stone-600 hover:bg-stone-200">
              Ask CIE about this
            </button>
          )}
          <button
            type="button"
            onClick={() => onOpenFull(req.primary)}
            className="rounded-md bg-stone-100 px-2 py-1 text-[10.5px] font-medium text-stone-600 hover:bg-stone-200"
            title="Enter"
          >
            <CornerDownLeft className="mr-1 inline h-3 w-3" /> Open full source
          </button>
          <button
            type="button"
            onClick={() => void navigator.clipboard?.writeText(pathLine)}
            className="ml-auto flex items-center gap-1 rounded-md px-2 py-1 text-[10.5px] text-stone-500 hover:bg-stone-100"
            title={`copy ${pathLine}`}
          >
            <Copy className="h-3 w-3" /> {pathLine}
          </button>
          {paired && sourceRoot && (
            <button
              type="button"
              onClick={() => {
                if (req.also) onOpenFull(req.also);
                onOpenFull(req.primary);
              }}
              className="rounded-md px-2 py-1 text-[10.5px] text-stone-500 hover:bg-stone-100"
            >
              Open both in source
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
