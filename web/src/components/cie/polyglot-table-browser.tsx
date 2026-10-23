"use client";
import { repositoryFetch } from "@/lib/repository-fetch";

// A browser over every table cie-polyglot's schema creates (see
// POLYGLOT_TABLES in src/lib/polyglot-db.ts) — not just the curated summary
// PolyglotAdminView used to show. Grouped by the module that owns each
// table; each table shows its row count up front (0 is a fact about this
// codebase — e.g. no recognized manifest file means 0 C4 containers — not a
// missing feature) and expands into a paginated raw-row view on demand.

import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Loader2 } from "lucide-react";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { DataTable } from "./cie-admin-view";

interface TableInfo {
  name: string;
  group: string;
  note: string;
  count: number | null;
}
interface InventoryResponse {
  available: boolean;
  tables?: TableInfo[];
}
interface RowsResponse {
  available: boolean;
  columns?: string[];
  rows?: Record<string, unknown>[];
  total?: number;
  page?: number;
  pageSize?: number;
}

const GROUP_ORDER = [
  "Ingest & AST",
  "IR",
  "Frameworks",
  "C4 / Concepts",
  "State machines",
  "Workflows",
  "Duplicates",
  "Runtime (OTel)",
];

function TableRowBrowser({ root, table }: { root: string; table: string }) {
  const [page, setPage] = useState(0);
  const [data, setData] = useState<RowsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const reqId = useRef(0);

  const load = useCallback(async () => {
    const mine = ++reqId.current;
    setLoading(true);
    try {
      const res = await repositoryFetch("/api/polyglot/table-rows", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ root, table, page }),
      });
      const d = await res.json();
      if (mine === reqId.current) setData(d);
    } finally {
      if (mine === reqId.current) setLoading(false);
    }
  }, [root, table, page]);

  useEffect(() => {
    void load();
  }, [load]);

  const total = data?.total ?? 0;
  const pageSize = data?.pageSize ?? 50;
  const pageCount = total > 0 ? Math.ceil(total / pageSize) : 0;

  return (
    <div className="space-y-2">
      {loading && (
        <p className="text-xs text-stone-500 flex items-center gap-1.5">
          <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> Loading…
        </p>
      )}
      {!loading && data?.available && (
        <>
          <DataTable rows={data.rows ?? []} />
          {pageCount > 1 && (
            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" disabled={page === 0} onClick={() => setPage((p) => Math.max(0, p - 1))}>
                Previous
              </Button>
              <Button variant="outline" size="sm" disabled={page + 1 >= pageCount} onClick={() => setPage((p) => p + 1)}>
                Next
              </Button>
              <span className="text-[11px] text-stone-500">
                page {page + 1} of {pageCount}
              </span>
            </div>
          )}
        </>
      )}
      {!loading && !data?.available && <p className="text-xs text-stone-500">Couldn&apos;t load this table.</p>}
    </div>
  );
}

function TableEntry({ root, info }: { root: string; info: TableInfo }) {
  const [open, setOpen] = useState(false);
  const empty = info.count === 0;
  const missing = info.count === null;
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <button
          type="button"
          disabled={missing || empty}
          className="w-full flex items-center gap-2 px-3 py-2 text-left disabled:cursor-default"
        >
          {!missing && !empty ? (
            open ? (
              <ChevronDown className="h-3.5 w-3.5 text-stone-400 shrink-0" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 text-stone-400 shrink-0" />
            )
          ) : (
            <span className="w-3.5 shrink-0" />
          )}
          <code className="text-xs font-semibold text-stone-900">{info.name}</code>
          <Badge variant={empty || missing ? "outline" : "secondary"} className="text-[10px]">
            {missing ? "not in this store" : `${info.count!.toLocaleString()} row${info.count === 1 ? "" : "s"}`}
          </Badge>
          <span className="text-[11px] text-stone-400 truncate hidden sm:inline">{info.note}</span>
        </button>
      </CollapsibleTrigger>
      {!missing && !empty && (
        <CollapsibleContent>
          <div className="px-3 pb-3 pt-0">
            {open && <TableRowBrowser root={root} table={info.name} />}
          </div>
        </CollapsibleContent>
      )}
    </Collapsible>
  );
}

export function PolyglotTableBrowser({ root }: { root: string }) {
  const [tables, setTables] = useState<TableInfo[] | null>(null);
  const [available, setAvailable] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const load = useCallback(async (r: string) => {
    const mine = ++reqId.current;
    setLoading(true);
    setError(null);
    try {
      const res = await repositoryFetch("/api/polyglot/tables", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ root: r }),
      });
      const d = (await res.json()) as InventoryResponse;
      if (mine !== reqId.current) return;
      setAvailable(!!d.available);
      setTables(d.tables ?? null);
    } catch {
      if (mine === reqId.current) setError("Couldn't load the table inventory.");
    } finally {
      if (mine === reqId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (root) void load(root);
  }, [root, load]);

  if (loading) {
    return (
      <p className="text-xs text-stone-500 px-1 flex items-center gap-1.5">
        <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> Loading the table inventory…
      </p>
    );
  }
  if (error) return <p className="text-xs text-red-600 px-1">{error}</p>;
  if (!available || !tables) {
    return <p className="text-xs text-stone-500 px-1">No table inventory available.</p>;
  }

  const byGroup = new Map<string, TableInfo[]>();
  for (const t of tables) {
    const list = byGroup.get(t.group);
    if (list) list.push(t);
    else byGroup.set(t.group, [t]);
  }

  return (
    <div className="space-y-3">
      {GROUP_ORDER.filter((g) => byGroup.has(g)).map((group) => {
        const entries = byGroup.get(group)!;
        const populated = entries.filter((t) => (t.count ?? 0) > 0).length;
        return (
          <Card key={group}>
            <CardContent className="p-0">
              <div className="px-3 pt-3 pb-1 flex items-center gap-2">
                <h4 className="text-xs font-semibold uppercase tracking-wide text-stone-500">{group}</h4>
                <Badge variant="outline" className="text-[10px]">
                  {populated}/{entries.length} populated
                </Badge>
              </div>
              <div className="divide-y divide-stone-100">
                {entries.map((t) => (
                  <TableEntry key={t.name} root={root} info={t} />
                ))}
              </div>
            </CardContent>
          </Card>
        );
      })}
    </div>
  );
}
