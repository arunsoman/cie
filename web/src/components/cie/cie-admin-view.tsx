"use client";

// CIE Admin — a raw query surface over every data structure CIE holds for
// the loaded project. Unlike Atlas (which only ever draws claims with a
// cited source span, gated for honesty) this page is explicitly the engine's
// own internal state: every array CIE exports, shown as a table, searchable
// by one free-text box across all of them at once. The G6 panel on top is a
// map of those data structures themselves (one node per table, sized by row
// count) — a navigation aid, not a claim about the code, so it draws
// straight through G6 rather than through the evidence-gated ViewSpec path.

import { useEffect, useMemo, useRef, useState } from "react";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "@/components/ui/table";
import { ChevronDown, ChevronRight, Search, Database } from "lucide-react";
import type { Graph as G6Graph, IElementEvent } from "@antv/g6";
import { useCie } from "@/lib/cie-store";
import {
  buildAdminSections,
  rowMatches,
  type AdminSection,
  type AdminCategory,
} from "@/lib/cie-admin";

const CATEGORY_LABEL: Record<AdminCategory, string> = {
  code: "report.json — code",
  stack: "stack.json — tech stack",
  concepts: "concepts.json — concept DAG",
  quality: "quality.json — the eight classics",
  deps: "deps.json — dependencies",
  runtime: "runtime.json — measured evidence",
  stories: "stories.json — registry demos",
  trust: "cochange / overlap / drift",
};

// One accent per category — used by both the G6 map and each section's
// left-rail so a node on the map and its table below read as the same thing.
const CATEGORY_COLOR: Record<AdminCategory, string> = {
  code: "#2ea043",
  stack: "#1783ff",
  concepts: "#8957e5",
  quality: "#d29922",
  deps: "#bf3989",
  runtime: "#0969da",
  stories: "#57606a",
  trust: "#cf222e",
};

const ROW_PAGE = 300;

function cellText(v: unknown): string {
  if (v == null) return "";
  if (typeof v === "object") {
    try {
      return JSON.stringify(v);
    } catch {
      return String(v);
    }
  }
  return String(v);
}

function DataTable({ rows }: { rows: Record<string, unknown>[] }) {
  const [showAll, setShowAll] = useState(false);
  if (!rows.length)
    return <p className="text-xs text-stone-500 py-3 px-1">0 rows.</p>;
  const columns = Array.from(new Set(rows.flatMap((r) => Object.keys(r))));
  const shown = showAll ? rows : rows.slice(0, ROW_PAGE);
  return (
    <div>
      <div className="max-h-[420px] overflow-auto border border-stone-200 rounded-md">
        <Table>
          <TableHeader className="sticky top-0 bg-stone-50 z-10">
            <TableRow>
              {columns.map((c) => (
                <TableHead key={c} className="whitespace-nowrap text-[11px] py-1.5">
                  {c}
                </TableHead>
              ))}
            </TableRow>
          </TableHeader>
          <TableBody>
            {shown.map((row, i) => (
              <TableRow key={i}>
                {columns.map((c) => {
                  const text = cellText(row[c]);
                  return (
                    <TableCell
                      key={c}
                      className="text-xs py-1 max-w-[280px] truncate"
                      title={text}
                    >
                      {text}
                    </TableCell>
                  );
                })}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>
      {rows.length > ROW_PAGE && (
        <button
          type="button"
          className="text-[11px] text-emerald-700 hover:underline mt-1.5 px-1"
          onClick={() => setShowAll((v) => !v)}
        >
          {showAll
            ? `Show first ${ROW_PAGE} only`
            : `Showing ${ROW_PAGE} of ${rows.length} — show all`}
        </button>
      )}
    </div>
  );
}

function SectionCard({
  section,
  query,
}: {
  section: AdminSection;
  query: string;
}) {
  const [open, setOpen] = useState(true);
  const rows = useMemo(
    () => section.rows.filter((r) => rowMatches(r, query)),
    [section.rows, query],
  );
  if (query && rows.length === 0) return null;
  return (
    <Card id={`section-${section.id}`} className="scroll-mt-14">
      <Collapsible open={open} onOpenChange={setOpen}>
        <CollapsibleTrigger asChild>
          <button
            type="button"
            className="w-full flex items-center gap-2 px-4 py-3 text-left"
            style={{ borderLeft: `3px solid ${CATEGORY_COLOR[section.category]}` }}
          >
            {open ? (
              <ChevronDown className="h-3.5 w-3.5 text-stone-400 shrink-0" />
            ) : (
              <ChevronRight className="h-3.5 w-3.5 text-stone-400 shrink-0" />
            )}
            <span className="text-sm font-semibold text-stone-900">{section.title}</span>
            <Badge variant="outline" className="text-[10px]">
              {query ? `${rows.length} of ${section.rows.length}` : section.rows.length} row
              {section.rows.length === 1 && !query ? "" : "s"}
            </Badge>
            <span className="text-[11px] text-stone-400 ml-auto">{section.source}</span>
          </button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <CardContent className="pt-0 pb-4">
            <DataTable rows={rows} />
          </CardContent>
        </CollapsibleContent>
      </Collapsible>
    </Card>
  );
}

/** The G6 overview map — one node per data structure, sized by row count,
 *  colored by category, laid out on a clean grid (G6's built-in `grid`
 *  layout, sorted by category so same-colored nodes cluster). Clicking a
 *  node scrolls the matching table into view below. Headless-safe: built
 *  only on the client, torn down on unmount/re-render like every other G6
 *  canvas in this app (see ChatChart). */
function OverviewMap({ sections }: { sections: AdminSection[] }) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = host.current;
    if (!container || !sections.length) return;
    let cancelled = false;
    let graph: G6Graph | undefined;
    void (async () => {
      const { Graph } = await import("@antv/g6");
      if (cancelled) return;
      const counts = sections.map((s) => s.rows.length);
      const maxCount = Math.max(1, ...counts);
      const sizeFor = (n: number) => 56 + Math.round((Math.sqrt(n) / Math.sqrt(maxCount)) * 40);
      // Pre-sorted by category so the grid layout's row-major placement
      // clusters same-colored nodes into visual bands — a "proper layout"
      // for a flat catalog like this, not a force-directed scatter.
      const ordered = [...sections].sort((a, b) =>
        a.category === b.category
          ? b.rows.length - a.rows.length
          : a.category.localeCompare(b.category),
      );
      const nodes = ordered.map((s) => ({
        id: s.id,
        style: {
          size: [sizeFor(s.rows.length), 34] as [number, number],
          labelText: `${s.title}\n${s.rows.length}`,
          labelFontSize: 9.5,
          labelPlacement: "center" as const,
          labelWordWrap: true,
          labelWordWrapWidth: sizeFor(s.rows.length) - 8,
          labelMaxLines: 2,
          radius: 5,
          fill: `${CATEGORY_COLOR[s.category]}1a`,
          stroke: CATEGORY_COLOR[s.category],
          lineWidth: 1.4,
          cursor: "pointer" as const,
        },
      }));
      graph = new Graph({
        container,
        autoResize: true,
        autoFit: "view",
        padding: 16,
        data: { nodes, edges: [] },
        node: { type: "rect" },
        layout: {
          type: "grid",
          cols: Math.max(4, Math.ceil(Math.sqrt(nodes.length * 2.4))),
          preventOverlap: true,
          nodeSize: 140,
        },
        behaviors: ["zoom-canvas"],
      });
      graph.on("node:click", (e: IElementEvent) => {
        const id = e.target?.id;
        if (id == null) return;
        document
          .getElementById(`section-${String(id)}`)
          ?.scrollIntoView({ behavior: "smooth", block: "start" });
      });
      await graph.render();
    })();
    return () => {
      cancelled = true;
      void graph?.destroy();
    };
  }, [sections]);

  return (
    <div
      ref={host}
      className="w-full h-[280px] rounded-lg border border-stone-200 bg-stone-50"
      role="img"
      aria-label="Map of every data structure CIE holds — click a box to jump to its table"
    />
  );
}

export function CieAdminView() {
  const { report, stack, concepts, quality, deps, runtime, stories, cochange, overlap, drift } =
    useCie();
  const [query, setQuery] = useState("");

  const sections = useMemo(
    () =>
      buildAdminSections({
        report,
        stack,
        concepts,
        quality,
        deps,
        runtime,
        stories,
        cochange,
        overlap,
        drift,
      }),
    [report, stack, concepts, quality, deps, runtime, stories, cochange, overlap, drift],
  );

  const totalRows = sections.reduce((n, s) => n + s.rows.length, 0);
  const matchedSections = query
    ? sections.filter((s) => s.rows.some((r) => rowMatches(r, query)))
    : sections;

  const byCategory = new Map<AdminCategory, AdminSection[]>();
  for (const s of matchedSections) {
    const list = byCategory.get(s.category) ?? [];
    list.push(s);
    byCategory.set(s.category, list);
  }

  if (!report) {
    return (
      <div className="p-6 w-full">
        <Card>
          <CardContent className="p-6 text-sm text-stone-500">
            Load a project first — Admin reads the same exports every other page uses.
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="p-4 sm:p-6 w-full space-y-4">
      <div className="flex items-start gap-3 flex-wrap">
        <div className="min-w-0">
          <h2 className="text-sm font-semibold text-stone-900 flex items-center gap-1.5">
            <Database className="h-4 w-4 text-emerald-700" aria-hidden /> CIE Admin
          </h2>
          <p className="text-xs text-stone-500 mt-0.5">
            {sections.length} data structures · {totalRows.toLocaleString()} recorded entities, every
            one shown below by default — query to narrow.
          </p>
        </div>
        <div className="relative ml-auto w-full sm:w-80">
          <Search className="h-3.5 w-3.5 text-stone-400 absolute left-2.5 top-1/2 -translate-y-1/2" aria-hidden />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Query every data structure at once…"
            className="pl-8 h-9 text-sm"
            aria-label="Search every CIE data structure"
          />
        </div>
      </div>

      <OverviewMap sections={matchedSections} />

      {query && matchedSections.length === 0 && (
        <p className="text-sm text-stone-500 px-1">No rows match “{query}”.</p>
      )}

      {Object.entries(CATEGORY_LABEL).map(([cat, label]) => {
        const list = byCategory.get(cat as AdminCategory);
        if (!list?.length) return null;
        return (
          <div key={cat} className="space-y-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-stone-500 pt-2">
              {label}
            </h3>
            {list.map((s) => (
              <SectionCard key={s.id} section={s} query={query} />
            ))}
          </div>
        );
      })}
    </div>
  );
}
