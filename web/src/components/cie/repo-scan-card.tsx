"use client";

// Two building blocks for the "On this computer" flow:
//
//   FolderBrowser — a real Browse button: an inline panel that walks the
//   local folders via /api/fs/browse (dirs only, never file contents).
//
//   ScanCard — the honest "what's in this folder" report from /api/repo/scan:
//   which languages the repo uses, which of them CIE reads, which it can't
//   yet, and what the manifests say about frameworks.

import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  AlertTriangle,
  ArrowUp,
  BookOpen,
  CheckCircle2,
  ChevronDown,
  CornerLeftUp,
  Folder,
  FolderSearch,
  House,
  Loader2,
} from "lucide-react";
import {
  SUPPORTED_LANGUAGES,
  KNOWN_UNSUPPORTED,
  type RepoScan,
} from "@/lib/language-profiles";

// ---------- FolderBrowser ----------

interface BrowseData {
  path: string;
  parent: string | null;
  name: string;
  home: string;
  roots: string[];
  entries: { name: string }[];
  truncated: boolean;
}

export function FolderBrowser({
  onPick,
  onClose,
}: {
  onPick: (path: string) => void;
  onClose: () => void;
}) {
  const [data, setData] = useState<BrowseData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const reqId = useRef(0);

  const load = useCallback(async (target?: string) => {
    const mine = ++reqId.current;
    setLoading(true);
    setError("");
    try {
      const qs = target ? `?path=${encodeURIComponent(target)}` : "";
      const res = await fetch(`/api/fs/browse${qs}`);
      const json = (await res.json().catch(() => ({}))) as Partial<BrowseData> & { error?: string };
      if (mine !== reqId.current) return; // a newer navigation superseded this one
      if (!res.ok || !json.path) {
        setError(json.error ?? "We couldn't open that folder. Please try another one.");
        return;
      }
      setData(json as BrowseData);
    } catch {
      if (mine === reqId.current) setError("We couldn't reach the server. Please try again.");
    } finally {
      if (mine === reqId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="rounded-xl border border-stone-200 bg-stone-50/70 p-3 space-y-2.5" role="dialog" aria-label="Choose a folder">
      {/* current path + up */}
      <div className="flex items-center gap-2">
        <p className="flex-1 min-w-0 truncate font-mono text-xs text-stone-700 bg-white border border-stone-200 rounded-lg px-2.5 py-1.5" title={data?.path ?? ""}>
          {data?.path ?? "…"}
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={!data?.parent || loading}
          onClick={() => data?.parent && void load(data.parent)}
          className="h-8 shrink-0"
          aria-label="Go up one folder"
        >
          <ArrowUp className="h-3.5 w-3.5" /> Up
        </Button>
      </div>

      {/* quick jumps */}
      {data && data.roots.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {data.roots.slice(0, 5).map((r) => (
            <button
              key={r}
              type="button"
              onClick={() => void load(r)}
              className={`inline-flex items-center gap-1 rounded-full border px-2.5 py-1 text-xs transition-colors ${r === data.path ? "border-emerald-300 bg-emerald-50 text-emerald-800" : "border-stone-200 bg-white text-stone-600 hover:border-stone-300"}`}
            >
              {r === data.home ? <House className="h-3 w-3" /> : <CornerLeftUp className="h-3 w-3" />}
              {r === data.home ? "Home" : r}
            </button>
          ))}
        </div>
      )}

      {/* folder list */}
      <div className="max-h-56 overflow-y-auto rounded-lg border border-stone-200 bg-white divide-y divide-stone-100" aria-live="polite">
        {loading && (
          <p className="flex items-center gap-2 text-sm text-stone-500 px-3 py-4">
            <Loader2 className="h-4 w-4 animate-spin" /> Listing folders…
          </p>
        )}
        {!loading && error && (
          <p className="flex items-start gap-2 text-sm text-rose-700 px-3 py-3">
            <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0" /> {error}
          </p>
        )}
        {!loading && !error && data && data.entries.length === 0 && (
          <p className="text-sm text-stone-500 px-3 py-4">No folders inside this one.</p>
        )}
        {!loading && !error && data && data.entries.map((e) => (
          <button
            key={e.name}
            type="button"
            onClick={() => void load(data.path === "/" ? `/${e.name}` : `${data.path}/${e.name}`)}
            className="w-full text-left flex items-center gap-2.5 px-3 py-2 hover:bg-emerald-50/60 transition-colors"
          >
            <Folder className="h-4 w-4 text-emerald-700 shrink-0" />
            <span className="text-sm text-stone-800 truncate">{e.name}</span>
          </button>
        ))}
        {!loading && !error && data?.truncated && (
          <p className="text-xs text-stone-400 px-3 py-2">Showing the first 500 folders — keep navigating to go deeper.</p>
        )}
      </div>

      {/* actions */}
      <div className="flex items-center justify-between gap-2 pt-0.5">
        <button type="button" onClick={onClose} className="text-xs text-stone-500 hover:text-stone-700 underline underline-offset-2">
          Type the path instead
        </button>
        <Button
          type="button"
          disabled={!data || loading || !!error}
          onClick={() => data && onPick(data.path)}
          className="h-9 bg-emerald-700 hover:bg-emerald-800 text-white rounded-lg px-4"
        >
          <CheckCircle2 className="h-4 w-4 mr-1.5" /> Use this folder
        </Button>
      </div>
    </div>
  );
}

// ---------- ScanCard ----------

function Chip({ label, tone }: { label: string; tone: "read" | "cant" }) {
  return (
    <span
      className={`inline-flex items-center rounded-full px-2.5 py-1 text-xs font-medium ${tone === "read" ? "bg-emerald-100 text-emerald-800" : "bg-amber-100 text-amber-800"}`}
    >
      {label}
    </span>
  );
}

export function ScanCard({ scan, busy, error, compact }: { scan?: RepoScan | null; busy?: boolean; error?: string; compact?: boolean }) {
  const [langsOpen, setLangsOpen] = useState(false);

  if (busy) {
    return (
      <div className="rounded-xl border border-stone-200 bg-stone-50/70 px-4 py-3 flex items-center gap-2.5 text-sm text-stone-600">
        <Loader2 className="h-4 w-4 animate-spin text-emerald-700" /> Looking inside the folder…
      </div>
    );
  }
  if (error) {
    return (
      <div className="rounded-xl border border-amber-200 bg-amber-50 px-4 py-3 flex items-start gap-2.5 text-sm text-amber-900">
        <AlertTriangle className="h-4 w-4 mt-0.5 shrink-0" />
        <span>We couldn&apos;t peek inside: {error} You can still run the check — CIE will look properly.</span>
      </div>
    );
  }
  if (!scan) return null;

  return (
    <div className={`rounded-xl border border-stone-200 bg-stone-50/70 ${compact ? "p-3" : "p-4"} space-y-3`} data-testid="scan-card">
      <div className="flex items-center justify-between gap-2 flex-wrap">
        <p className="text-sm font-semibold text-stone-900 flex items-center gap-2">
          <FolderSearch className="h-4 w-4 text-emerald-700" /> What&apos;s in this folder
        </p>
        <p className="text-xs text-stone-500">
          {scan.fileCount} file{scan.fileCount === 1 ? "" : "s"}
          {scan.otherFiles > 0 ? ` · ${scan.otherFiles} notes/configs/styles skipped` : ""}
        </p>
      </div>

      {/* frameworks the manifests reveal */}
      {scan.frameworks.length > 0 && (
        <div className="space-y-1.5">
          {scan.frameworks.slice(0, 4).map((f) => (
            <div key={f.id} className="text-xs leading-relaxed">
              <span className="inline-flex items-center rounded-md bg-emerald-700 text-white px-2 py-0.5 font-medium mr-1.5">{f.name}</span>
              <span className="text-stone-700">{f.plain}</span>
              <span className="block text-stone-500 italic mt-0.5">{f.honesty}</span>
            </div>
          ))}
        </div>
      )}

      {/* the honest language split */}
      {scan.languages.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-xs font-medium text-stone-600 mr-0.5">CIE reads:</span>
          {scan.languages.map((l) => (
            <Chip key={l.id} label={`${l.name} · ${l.files}`} tone="read" />
          ))}
        </div>
      )}
      {scan.unsupported.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-xs font-medium text-stone-600 mr-0.5">Can&apos;t read yet:</span>
          {scan.unsupported.map((l) => (
            <Chip key={l.id} label={`${l.name} · ${l.files}`} tone="cant" />
          ))}
        </div>
      )}

      {scan.unsupported.length > 0 && (
        <ul className="space-y-1">
          {scan.unsupported.slice(0, 4).map((l) => {
            const meta = KNOWN_UNSUPPORTED.find((k) => k.id === l.id);
            return (
              <li key={l.id} className="text-xs text-stone-600 leading-relaxed flex gap-1.5">
                <span className="text-amber-600 shrink-0">•</span>
                <span>
                  <span className="font-medium text-stone-800">{l.name}</span> — {meta?.why ?? "CIE skips these files for now."}
                </span>
              </li>
            );
          })}
          {scan.unsupported.length > 4 && (
            <li className="text-xs text-stone-500 pl-3.5">…and {scan.unsupported.length - 4} more language{scan.unsupported.length - 4 === 1 ? "" : "s"} CIE skips.</li>
          )}
        </ul>
      )}

      <p className={`text-sm leading-relaxed ${scan.nothingReadable ? "text-rose-800 font-medium" : "text-stone-700"}`}>
        {scan.nothingReadable ? (
          <>
            <AlertTriangle className="h-4 w-4 inline mr-1 -mt-0.5 text-rose-700" />
            Nothing here uses a language CIE can read yet. {scan.readPlain}
          </>
        ) : (
          scan.readPlain
        )}
      </p>
      {scan.unsupportedPlain && <p className="text-sm text-stone-600 leading-relaxed">{scan.unsupportedPlain}</p>}
      {scan.allReadable && (
        <p className="text-sm text-emerald-800 leading-relaxed flex items-center gap-1.5">
          <CheckCircle2 className="h-4 w-4 shrink-0" /> Good news — CIE can read every language in this folder.
        </p>
      )}

      {/* what CAN CIE read — collapsible, from the engine-mirrored table */}
      <button
        type="button"
        onClick={() => setLangsOpen((o) => !o)}
        className="text-xs text-stone-500 hover:text-stone-700 flex items-center gap-1"
        aria-expanded={langsOpen}
      >
        <BookOpen className="h-3 w-3" />
        All languages CIE can read ({SUPPORTED_LANGUAGES.length})
        <ChevronDown className={`h-3 w-3 transition-transform ${langsOpen ? "rotate-180" : ""}`} />
      </button>
      {langsOpen && (
        <ul className="grid gap-x-4 gap-y-1 sm:grid-cols-2 text-xs text-stone-600 border-t border-stone-200 pt-2">
          {SUPPORTED_LANGUAGES.map((l) => (
            <li key={l.id} className="flex gap-1.5">
              <CheckCircle2 className="h-3 w-3 text-emerald-600 mt-0.5 shrink-0" />
              <span>
                <span className="font-medium text-stone-800">{l.name}</span>{" "}
                <span className="font-mono text-stone-400">.{l.extensions.join(" .")}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
