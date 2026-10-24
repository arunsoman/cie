"use client";
import { readCapturedSource, readDuplicates, type SymbolRef } from "@/lib/storage-ui";

// Duplicate Analysis — flat, filterable view over every row of the
// `duplicates` table (all tiers, unlike DuplicateClustersView which is
// graph-shaped and canonical-only). Slice by tier/detection/epistemic;
// every row resolves symbol_a/symbol_b/canonical_symbol_id to a name, kind
// and file path instead of showing raw ids.

import { Fragment, useCallback, useEffect, useRef, useState } from "react";
import { useCie } from "@/lib/cie-store";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ChevronDown, ChevronRight, Loader2 } from "lucide-react";

interface DupRow {
  id: string;
  similarity: number;
  tier: string;
  detection: string;
  epistemic: string;
  counterArgument: string | null;
  a: SymbolRef;
  b: SymbolRef;
  canonical: SymbolRef | null;
}
interface DupResponse {
  available: boolean;
  rows?: DupRow[];
  total?: number;
  page?: number;
  pageSize?: number;
  facets?: { tiers: string[]; detections: string[]; epistemics: string[] };
}

const ALL = "__all__";

function SymbolCell({ s }: { s: SymbolRef }) {
  return (
    <div className="min-w-0">
      <div className="flex items-center gap-1.5">
        <Badge variant="outline" className="text-[10px] shrink-0">
          {s.kind}
        </Badge>
        <span className="text-sm font-medium text-stone-900 truncate" title={s.qualName}>
          {s.qualName}
        </span>
      </div>
      <div className="text-[11px] text-stone-400 truncate" title={s.path}>
        {s.path}
      </div>
    </div>
  );
}

const EPISTEMIC_TONE: Record<string, string> = {
  Fact: "bg-emerald-100 text-emerald-800",
  Inference: "bg-amber-100 text-amber-800",
  Hypothesis: "bg-stone-200 text-stone-700",
};

function SnippetPane({ root, member, title }: { root: string; member: SymbolRef; title: string }) {
  const [text, setText] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const reqId = useRef(0);

  useEffect(() => {
    const mine = ++reqId.current;
    setLoading(true);
    setText(null);
    (async () => {
      try {
        const d = await readCapturedSource(root, member.path, member.startByte, member.endByte);
        if (mine !== reqId.current) return;
        setText(d.text);
      } catch {
        if (mine === reqId.current) setText("(failed to load)");
      } finally {
        if (mine === reqId.current) setLoading(false);
      }
    })();
  }, [root, member.path, member.startByte, member.endByte]);

  return (
    <div className="min-w-0 flex-1">
      <div className="text-[11px] text-stone-500 mb-1 truncate" title={`${member.path} (${member.qualName})`}>
        {title}: <span className="font-medium text-stone-700">{member.qualName}</span>
        <div className="text-stone-400 truncate">{member.path}</div>
      </div>
      <pre className="text-[11px] bg-stone-900 text-stone-100 rounded-md p-2 overflow-auto max-h-72 whitespace-pre-wrap">
        {loading ? "loading…" : text}
      </pre>
    </div>
  );
}

export function DuplicateAnalysisView() {
  const { sourceRoot: liveSourceRoot } = useCie();
  const sourceRoot = useCie((s) => s.storageRoot) ?? liveSourceRoot;
  const [tier, setTier] = useState<string>(ALL);
  const [detection, setDetection] = useState<string>(ALL);
  const [epistemic, setEpistemic] = useState<string>(ALL);
  const [page, setPage] = useState(0);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [data, setData] = useState<DupResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const load = useCallback(
    async (root: string) => {
      const mine = ++reqId.current;
      setLoading(true);
      setError(null);
      try {
        const d = await readDuplicates(root, { tier: tier === ALL ? undefined : tier, detection: detection === ALL ? undefined : detection, epistemic: epistemic === ALL ? undefined : epistemic, page });
        if (mine === reqId.current) setData(d);
      } catch {
        if (mine === reqId.current) setError("Couldn't load duplicates.");
      } finally {
        if (mine === reqId.current) setLoading(false);
      }
    },
    [tier, detection, epistemic, page],
  );

  // Any filter change resets to page 0 and refetches.
  useEffect(() => {
    setPage(0);
  }, [tier, detection, epistemic]);

  useEffect(() => {
    setExpanded(new Set());
  }, [tier, detection, epistemic, page]);

  useEffect(() => {
    if (sourceRoot) void load(sourceRoot);
  }, [sourceRoot, load]);

  const toggleExpanded = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  if (!sourceRoot) {
    return (
      <p className="text-xs text-stone-500 px-1">
        Load a project first — this view reads the captured storage revision for that project.
      </p>
    );
  }

  const facets = data?.facets;
  const total = data?.total ?? 0;
  const pageSize = data?.pageSize ?? 50;
  const pageCount = total > 0 ? Math.ceil(total / pageSize) : 0;

  return (
    <Card className="border-stone-200">
      <CardHeader className="pb-3">
        <CardTitle className="text-base">Duplicate analysis</CardTitle>
        <CardDescription>
          Every row in the <code className="bg-stone-100 px-1 rounded">duplicates</code> table, any tier — slice by
          tier, detection method or confidence; symbols shown by name and kind, not id.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap gap-2">
          <Select value={tier} onValueChange={setTier}>
            <SelectTrigger className="w-full sm:w-48">
              <SelectValue placeholder="Tier" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL}>All tiers</SelectItem>
              {(facets?.tiers ?? []).map((t) => (
                <SelectItem key={t} value={t}>
                  {t}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={detection} onValueChange={setDetection}>
            <SelectTrigger className="w-full sm:w-56">
              <SelectValue placeholder="Detection" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL}>All detection methods</SelectItem>
              {(facets?.detections ?? []).map((t) => (
                <SelectItem key={t} value={t}>
                  {t}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
          <Select value={epistemic} onValueChange={setEpistemic}>
            <SelectTrigger className="w-full sm:w-48">
              <SelectValue placeholder="Epistemic" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value={ALL}>All confidence levels</SelectItem>
              {(facets?.epistemics ?? []).map((t) => (
                <SelectItem key={t} value={t}>
                  {t}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {loading && (
          <p className="text-xs text-stone-500 px-1 flex items-center gap-1.5">
            <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> Loading duplicates…
          </p>
        )}
        {error && <p className="text-xs text-red-600 px-1">{error}</p>}
        {!loading && !error && !data?.available && (
          <p className="text-xs text-stone-500 px-1">
            No captured storage revision found for this project yet —
            run it against this folder first, then reload this page.
          </p>
        )}
        {!loading && !error && data?.available && !data.rows?.length && (
          <p className="text-xs text-stone-500 px-1">No duplicates match these filters.</p>
        )}

        {!loading && !error && !!data?.rows?.length && (
          <>
            <p className="text-[11px] text-stone-500 px-1">
              {total.toLocaleString()} pair{total === 1 ? "" : "s"} match{total === 1 ? "es" : ""} this filter —
              showing page {page + 1} of {pageCount}.
            </p>
            <div className="overflow-x-auto rounded-md border border-stone-200">
              <table className="w-full text-sm">
                <thead className="bg-stone-50 text-[11px] uppercase text-stone-500">
                  <tr>
                    <th className="text-left font-medium px-3 py-2">Symbol A</th>
                    <th className="text-left font-medium px-3 py-2">Symbol B</th>
                    <th className="text-left font-medium px-3 py-2">Similarity</th>
                    <th className="text-left font-medium px-3 py-2">Tier</th>
                    <th className="text-left font-medium px-3 py-2">Detection</th>
                    <th className="text-left font-medium px-3 py-2">Epistemic</th>
                    <th className="text-left font-medium px-3 py-2">Canonical</th>
                    <th className="text-left font-medium px-3 py-2">Code</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-stone-100">
                  {data.rows.map((r) => {
                    const isOpen = expanded.has(r.id);
                    return (
                      <Fragment key={r.id}>
                        <tr className="align-top">
                          <td className="px-3 py-2 max-w-xs">
                            <SymbolCell s={r.a} />
                          </td>
                          <td className="px-3 py-2 max-w-xs">
                            <SymbolCell s={r.b} />
                          </td>
                          <td className="px-3 py-2 whitespace-nowrap">{r.similarity.toFixed(3)}</td>
                          <td className="px-3 py-2 whitespace-nowrap">
                            <Badge variant="secondary" className="text-[10px]">
                              {r.tier}
                            </Badge>
                          </td>
                          <td className="px-3 py-2 whitespace-nowrap text-stone-600">{r.detection}</td>
                          <td className="px-3 py-2 whitespace-nowrap">
                            <span
                              className={`text-[10px] px-1.5 py-0.5 rounded ${EPISTEMIC_TONE[r.epistemic] ?? "bg-stone-100 text-stone-700"}`}
                            >
                              {r.epistemic}
                            </span>
                          </td>
                          <td className="px-3 py-2 max-w-xs">
                            {r.canonical ? (
                              <SymbolCell s={r.canonical} />
                            ) : (
                              <span className="text-stone-300">—</span>
                            )}
                            {r.counterArgument && (
                              <div className="text-[10px] text-stone-400 mt-0.5" title={r.counterArgument}>
                                ⚠ {r.counterArgument}
                              </div>
                            )}
                          </td>
                          <td className="px-3 py-2 whitespace-nowrap">
                            <Button
                              variant="ghost"
                              size="sm"
                              className="h-7 px-2 text-[11px]"
                              onClick={() => toggleExpanded(r.id)}
                            >
                              {isOpen ? (
                                <ChevronDown className="h-3 w-3 mr-1" aria-hidden />
                              ) : (
                                <ChevronRight className="h-3 w-3 mr-1" aria-hidden />
                              )}
                              {isOpen ? "Hide code" : "Show code"}
                            </Button>
                          </td>
                        </tr>
                        {isOpen && sourceRoot && (
                          <tr>
                            <td colSpan={7} className="px-3 pb-3 pt-0 bg-stone-50">
                              <div className="flex gap-3 flex-wrap pt-2">
                                <SnippetPane root={sourceRoot} member={r.a} title="Symbol A" />
                                <SnippetPane root={sourceRoot} member={r.b} title="Symbol B" />
                              </div>
                            </td>
                          </tr>
                        )}
                      </Fragment>
                    );
                  })}
                </tbody>
              </table>
            </div>
            <div className="flex items-center gap-2 px-1">
              <Button
                variant="outline"
                size="sm"
                disabled={page === 0}
                onClick={() => setPage((p) => Math.max(0, p - 1))}
              >
                Previous
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={page + 1 >= pageCount}
                onClick={() => setPage((p) => p + 1)}
              >
                Next
              </Button>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
