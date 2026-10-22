"use client";

// Everything the cie-polyglot run (PRD v2.0 — docs/prd-v2.0.md) recorded
// for this project, read straight from the SQLite scratch store — never
// from report.json/stack.json/useCie() (this is a separate engine with its
// own fresh store per run, see src/lib/polyglot-db.ts).
//
// A quick-glance stat strip (/api/polyglot/summary) up top, then
// PolyglotTableBrowser for every table the schema creates — not a curated
// subset — grouped by module, each showing its row count (0 is a fact about
// this codebase, e.g. no recognized manifest → 0 C4 containers, not a
// missing feature) and expandable into its raw rows.
//
// Duplicate detection additionally gets two dedicated views below the
// browser: duplicate-clusters-view (exact-match families as a graph,
// canonical tier only) and duplicate-analysis-view (a flat, filterable,
// name-resolved table across every tier) — richer than the raw `duplicates`
// row dump the browser also offers.

import { useCallback, useEffect, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { PolyglotTableBrowser } from "./polyglot-table-browser";
import { FunctionIrView } from "./function-ir-view";
import { DuplicateClustersView } from "./duplicate-clusters-view";
import { DuplicateAnalysisView } from "./duplicate-analysis-view";
import type { PolyglotSummary } from "@/lib/polyglot-db";

function StatTile({ label, value }: { label: string; value: number | string }) {
  return (
    <div className="rounded-md border border-stone-200 px-3 py-2 min-w-[108px]">
      <div className="text-lg font-semibold text-stone-900 leading-tight">
        {typeof value === "number" ? value.toLocaleString() : value}
      </div>
      <div className="text-[11px] text-stone-500">{label}</div>
    </div>
  );
}

export function PolyglotAdminView() {
  const { sourceRoot } = useCie();
  const [summary, setSummary] = useState<PolyglotSummary | null>(null);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const load = useCallback(async (root: string) => {
    const mine = ++reqId.current;
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/polyglot/summary", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ root }),
      });
      const d = await res.json();
      if (mine !== reqId.current) return;
      setAvailable(!!d.available);
      setSummary(d.summary ?? null);
    } catch {
      if (mine === reqId.current) setError("Couldn't load the polyglot run.");
    } finally {
      if (mine === reqId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (sourceRoot) void load(sourceRoot);
  }, [sourceRoot, load]);

  if (!sourceRoot) {
    return (
      <p className="text-xs text-stone-500 px-1">
        Load a local-folder project first — the polyglot run is keyed by that folder&apos;s path.
      </p>
    );
  }
  if (loading) {
    return (
      <p className="text-xs text-stone-500 px-1 flex items-center gap-1.5">
        <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> Loading the polyglot run…
      </p>
    );
  }
  if (error) return <p className="text-xs text-red-600 px-1">{error}</p>;
  if (!available || !summary) {
    return (
      <p className="text-xs text-stone-500 px-1">
        No <code className="bg-stone-100 px-1 rounded">cie polyglot</code> store found for this project yet —
        run <code className="bg-stone-100 px-1 rounded">cie polyglot {sourceRoot}</code>, then reload this page.
      </p>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        <StatTile label="Files parsed" value={summary.ingest.files} />
        <StatTile label="Symbols" value={summary.ingest.symbols} />
        <StatTile
          label="Calls (resolved/unresolved)"
          value={`${summary.ir.callsResolved.toLocaleString()} / ${summary.ir.callsUnresolved.toLocaleString()}`}
        />
      </div>
      <p className="text-[11px] text-stone-500 px-1">
        Store: <code className="text-[10px]">{summary.dbPath}</code>
      </p>

      <PolyglotTableBrowser root={sourceRoot} />

      <div className="pt-2">
        <FunctionIrView root={sourceRoot} />
      </div>

      <div className="pt-2">
        <h4 className="text-xs font-semibold uppercase tracking-wide text-stone-500 pb-2">
          Duplicate-code clusters (exact IR-hash matches)
        </h4>
        <DuplicateClustersView />
      </div>

      <div className="pt-2">
        <DuplicateAnalysisView />
      </div>
    </div>
  );
}
