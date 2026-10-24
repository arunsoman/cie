"use client";
import { readCapturedSource, readFunctionIr, searchStoredSymbols, type SymbolRef as SymbolResult } from "@/lib/storage-ui";

// Function IR viewer — pick a function by name, see its lowered CFG: blocks
// in order, each with its instructions (opcode + source span — `operands`
// isn't shown because the lowering pass never fills it in, see
// src/lib/storage-ui.ts), the control-flow edges between blocks, and the
// function's outgoing calls (name-resolved where resolvable). This is the
// per-function complement to the raw ir_blocks/ir_instructions rows the
// generic table browser shows — those have unresolved ids and no grouping.

import { useCallback, useEffect, useRef, useState } from "react";
import { Loader2, Search } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";

type FunctionIrResponse=Partial<Awaited<ReturnType<typeof readFunctionIr>>>&{available:boolean};
type FunctionIrEdge=Awaited<ReturnType<typeof readFunctionIr>>['edges'][number];

function SourcePane({ root, path, startByte, endByte }: { root: string; path: string; startByte: number; endByte: number }) {
  const [text, setText] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setText(null);
    (async () => {
      try {
        const d = await readCapturedSource(root, path, startByte, endByte);
        if (cancelled) return;
        setText(d.text);
      } catch {
        if (!cancelled) setText("(failed to load)");
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [root, path, startByte, endByte]);

  return (
    <pre className="text-[11px] bg-stone-900 text-stone-100 rounded-md p-2 overflow-auto max-h-72 whitespace-pre-wrap">
      {loading ? "loading…" : text}
    </pre>
  );
}

const BLOCK_KIND_TONE: Record<string, string> = {
  entry: "bg-emerald-100 text-emerald-800",
  exit: "bg-rose-100 text-rose-800",
};

function FunctionIrPanel({ root, id }: { root: string; id: string }) {
  const [data, setData] = useState<FunctionIrResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const reqId = useRef(0);

  useEffect(() => {
    const mine = ++reqId.current;
    setLoading(true);
    (async () => {
      try {
        const d = await readFunctionIr(root, id);
        if (mine === reqId.current) setData(d);
      } catch { if(mine===reqId.current)setData({available:false}); } finally {
        if (mine === reqId.current) setLoading(false);
      }
    })();
  }, [root, id]);

  if (loading) {
    return (
      <p className="text-xs text-stone-500 flex items-center gap-1.5 px-1">
        <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> Loading IR…
      </p>
    );
  }
  if (!data?.available || !data.symbol) {
    return <p className="text-xs text-stone-500 px-1">Couldn&apos;t load IR for this symbol.</p>;
  }

  const { symbol, blocks = [], edges = [], calls = [] } = data;
  const edgesByBlock = new Map<number, FunctionIrEdge[]>();
  for (const e of edges) {
    const list = edgesByBlock.get(e.fromBlockIndex);
    if (list) list.push(e);
    else edgesByBlock.set(e.fromBlockIndex, [e]);
  }

  return (
    <div className="space-y-3">
      <div>
        <div className="flex items-center gap-1.5 flex-wrap">
          <Badge variant="outline" className="text-[10px]">
            {symbol.kind}
          </Badge>
          <span className="text-sm font-semibold text-stone-900">{symbol.qualName}</span>
        </div>
        <div className="text-[11px] text-stone-400">{symbol.path}</div>
      </div>

      <SourcePane root={root} path={symbol.path} startByte={symbol.startByte} endByte={symbol.endByte} />

      <div>
        <h5 className="text-[11px] font-semibold uppercase tracking-wide text-stone-500 mb-1.5">
          Blocks ({blocks.length})
        </h5>
        <div className="space-y-2">
          {blocks.map((b) => (
            <div key={b.id} className="border border-stone-200 rounded-md p-2">
              <div className="flex items-center gap-1.5 mb-1">
                <span className="text-[11px] font-mono text-stone-500">#{b.blockIndex}</span>
                <span className={`text-[10px] px-1.5 py-0.5 rounded ${BLOCK_KIND_TONE[b.kind] ?? "bg-stone-100 text-stone-700"}`}>
                  {b.kind}
                </span>
                {(edgesByBlock.get(b.blockIndex) ?? []).map((e, i) => (
                  <span key={i} className="text-[10px] text-stone-400">
                    → #{e.toBlockIndex} ({e.kind})
                  </span>
                ))}
              </div>
              {b.instructions.length ? (
                <table className="w-full text-[11px]">
                  <tbody>
                    {b.instructions.map((inst) => (
                      <tr key={inst.instIndex} className="border-t border-stone-100 first:border-t-0">
                        <td className="py-0.5 pr-2 text-stone-400 font-mono w-8">{inst.instIndex}</td>
                        <td className="py-0.5 pr-2 font-mono text-stone-800 whitespace-nowrap">{inst.op}</td>
                        <td className="py-0.5 text-stone-400 font-mono">
                          [{inst.spanStart}, {inst.spanEnd})
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              ) : (
                <p className="text-[11px] text-stone-400">no instructions</p>
              )}
            </div>
          ))}
        </div>
      </div>

      {calls.length > 0 && (
        <div>
          <h5 className="text-[11px] font-semibold uppercase tracking-wide text-stone-500 mb-1.5">
            Outgoing calls ({calls.length})
          </h5>
          <div className="border border-stone-200 rounded-md overflow-hidden">
            <table className="w-full text-[11px]">
              <tbody>
                {calls.map((c, i) => (
                  <tr key={i} className="border-t border-stone-100 first:border-t-0">
                    <td className="py-1 px-2 text-stone-400 font-mono w-10">{c.stmtIndex}</td>
                    <td className="py-1 px-2 font-mono text-stone-800">{c.calleeQualName ?? c.calleeText ?? "?"}</td>
                    <td className="py-1 px-2">
                      <span
                        className={`text-[10px] px-1.5 py-0.5 rounded ${
                          c.resolution === "RESOLVED" ? "bg-emerald-100 text-emerald-800" : "bg-amber-100 text-amber-800"
                        }`}
                      >
                        {c.resolution}
                      </span>
                    </td>
                    <td className="py-1 px-2 text-stone-400">{c.callKind}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}

export function FunctionIrView({ root }: { root: string }) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SymbolResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError,setSearchError]=useState<string|null>(null);
  const [picked, setPicked] = useState<SymbolResult | null>(null);
  const reqId = useRef(0);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const search = useCallback(
    (q: string) => {
      const mine = ++reqId.current;
      setSearching(true);
      setSearchError(null);
      searchStoredSymbols(root, q)
        .then((d) => {
          if (mine === reqId.current) setResults(d);
        })
        .catch(error=>{if(mine===reqId.current)setSearchError(error instanceof Error?error.message:"Storage unavailable");})
        .finally(() => {
          if (mine === reqId.current) setSearching(false);
        });
    },
    [root],
  );

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    if (query.trim().length < 2) return;
    debounceRef.current = setTimeout(() => search(query), 250);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query, search]);

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="text-base">Function IR</CardTitle>
        <CardDescription>
          Pick a function to see its lowered CFG — blocks, instructions (opcode + source span), control-flow edges, and
          outgoing calls.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {searchError && <p role="alert" className="text-xs text-red-600">{searchError}</p>}
        <div className="relative">
          <Search className="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-stone-400" aria-hidden />
          <Input
            value={query}
            onChange={(e) => {
              const v = e.target.value;
              setQuery(v);
              setPicked(null);
              if (v.trim().length < 2) setResults([]);
            }}
            placeholder="Search by function/class name…"
            className="pl-8 h-9 text-sm"
          />
          {searching && <Loader2 className="absolute right-2.5 top-2.5 h-3.5 w-3.5 animate-spin text-stone-400" aria-hidden />}
        </div>

        {!picked && results.length > 0 && (
          <div className="border border-stone-200 rounded-md divide-y divide-stone-100 max-h-56 overflow-auto">
            {results.map((r) => (
              <button
                key={r.id}
                type="button"
                className="w-full text-left px-2.5 py-1.5 hover:bg-stone-50 flex items-center gap-1.5"
                onClick={() => {
                  setPicked(r);
                  setResults([]);
                }}
              >
                <Badge variant="outline" className="text-[10px] shrink-0">
                  {r.kind}
                </Badge>
                <span className="text-xs font-medium text-stone-900 truncate">{r.qualName}</span>
                <span className="text-[10px] text-stone-400 truncate ml-auto">{r.path}</span>
              </button>
            ))}
          </div>
        )}
        {!picked && query.trim().length >= 2 && !searching && results.length === 0 && (
          <p className="text-xs text-stone-400 px-1">No matching symbols.</p>
        )}

        {picked && <FunctionIrPanel root={root} id={picked.id} />}
      </CardContent>
    </Card>
  );
}
