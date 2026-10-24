"use client";
import { readStorageInventory, type StorageInventory } from "@/lib/storage-ui";

// Pinned storage diagnostics and typed evidence views for the selected workspace.

import { useCallback, useEffect, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { StorageTableBrowser } from "./storage-table-browser";
import { FunctionIrView } from "./function-ir-view";
import { DuplicateClustersView } from "./duplicate-clusters-view";
import { DuplicateAnalysisView } from "./duplicate-analysis-view";


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

export function StorageAdminView(){
  const key=useCie(state=>`${state.storageRoot??state.sourceRoot}:${state.report?.rev}`);
  return <StorageAdminPanel key={key}/>;
}
function StorageAdminPanel() {
  const { sourceRoot: liveSourceRoot } = useCie();
  const sourceRoot = useCie((s) => s.storageRoot) ?? liveSourceRoot;
  const [summary, setSummary] = useState<StorageInventory | null>(null);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const load = useCallback(async (root: string) => {
    const mine = ++reqId.current;
    setLoading(true);
    setError(null);
    try {
      const d = await readStorageInventory(root);
      if (mine !== reqId.current) return;
      setAvailable(true);
      setSummary(d);
    } catch {
      if (mine === reqId.current) setError("Couldn't load captured storage.");
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
        Load a project first to inspect its captured storage revision.
      </p>
    );
  }
  if (loading) {
    return (
      <p className="text-xs text-stone-500 px-1 flex items-center gap-1.5">
        <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> Loading captured storage…
      </p>
    );
  }
  if (error) return <p className="text-xs text-red-600 px-1">{error}</p>;
  if (!available || !summary) {
    return (
      <p className="text-xs text-stone-500 px-1">
        Captured storage is unavailable for this revision. Reconnect the repository to rebuild its captured facts.
      </p>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap gap-2">
        <StatTile label="Files parsed" value={summary.datasets.find(d=>d.name==="files")?.count ?? "unknown"} />
        <StatTile label="Symbols" value={summary.datasets.find(d=>d.name==="symbols")?.count ?? "unknown"} />
        <StatTile
          label="Captured calls"
          value={summary.datasets.find(d=>d.name==="calls")?.count ?? "unknown"}
        />
      </div>
      <p className="text-[11px] text-stone-500 px-1">
        Captured revision: <code className="text-[10px]">{summary.revision}</code>
      </p>

      <StorageTableBrowser root={sourceRoot} />

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
