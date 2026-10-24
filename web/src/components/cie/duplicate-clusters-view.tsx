"use client";
import { readCapturedSource, readDuplicateClusters, type SymbolRef as Member } from "@/lib/storage-ui";

// Duplicate-code clusters from the cie-polyglot run — exact-IR-hash
// families (tier='canonical', the one Fact-tier result; cosine tiers are
// Inference/Hypothesis and, on real data, prone to false positives on
// short simple methods — see src/lib/storage-ui.ts). One representative
// "hub" symbol per family, every byte-identical-IR copy attached to it as
// a node in a small star graph; click a node to pull up its actual source
// next to the hub's, so similarity is judged by reading real code, not a
// similarity score.

import { useCallback, useEffect, useRef, useState } from "react";
import type { Graph as G6Graph, IElementEvent } from "@antv/g6";
import { useCie } from "@/lib/cie-store";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Loader2 } from "lucide-react";

interface Cluster {
  hub: Member;
  members: Member[];
  totalMembers: number;
  truncated?: boolean;
}
interface ClustersResponse {
  available: boolean;
  tierUsed?: string;
  totalFamilies?: number;
  totalSymbolsInvolved?: number;
  clusters?: Cluster[];
  truncated?: boolean;
}

function SnippetPane({ root, member, title }: { root: string; member: Member; title: string }) {
  const [text, setText] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const reqId = useRef(0);

  const load = useCallback(async () => {
    const mine = ++reqId.current;
    setLoading(true);
    setText(null);
    try {
      const d = await readCapturedSource(root, member.path, member.startByte, member.endByte);
      if (mine !== reqId.current) return;
      setText(d.text);
    } catch {
      if (mine === reqId.current) setText("(failed to load)");
    } finally {
      if (mine === reqId.current) setLoading(false);
    }
  }, [root, member.path, member.startByte, member.endByte]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="min-w-0 flex-1">
      <div className="text-[11px] text-stone-500 mb-1 truncate" title={`${member.path} (${member.qualName})`}>
        {title}: <span className="font-medium text-stone-700">{member.qualName}</span>
        <div className="text-stone-400 truncate">{member.path}</div>
      </div>
      <pre className="text-[11px] bg-stone-900 text-stone-100 rounded-md p-2 overflow-auto max-h-64 whitespace-pre-wrap">
        {loading ? "loading…" : text}
      </pre>
    </div>
  );
}

function ClusterGraph({ cluster, onPick }: { cluster: Cluster; onPick: (m: Member) => void }) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const container = host.current;
    if (!container) return;
    let cancelled = false;
    let graph: G6Graph | undefined;
    void (async () => {
      const { Graph } = await import("@antv/g6");
      if (cancelled) return;
      const nodes = [
        {
          id: "hub",
          style: {
            size: 46,
            labelText: "hub",
            labelFontSize: 9,
            fill: "#2ea04333",
            stroke: "#2ea043",
            lineWidth: 2.2,
            cursor: "pointer" as const,
          },
        },
        ...cluster.members.map((m) => ({
          id: String(m.id),
          style: {
            size: 30,
            labelText: m.qualName.length > 16 ? `${m.qualName.slice(0, 14)}…` : m.qualName,
            labelFontSize: 8,
            labelPlacement: "bottom" as const,
            fill: "#d2992233",
            stroke: "#d29922",
            lineWidth: 1.3,
            cursor: "pointer" as const,
          },
        })),
      ];
      const edges = cluster.members.map((m) => ({
        id: `e:${m.id}`,
        source: "hub",
        target: String(m.id),
        style: { stroke: "#c9d1d9", lineWidth: 1, endArrow: false },
      }));
      graph = new Graph({
        container,
        autoResize: true,
        autoFit: "view",
        padding: 20,
        data: { nodes, edges },
        node: { type: "circle" },
        layout: { type: "circular", radius: Math.max(80, cluster.members.length * 9) },
        behaviors: ["zoom-canvas"],
      });
      graph.on("node:click", (e: IElementEvent) => {
        const id = e.target?.id;
        if (id == null) return;
        if (id === "hub") {
          onPick(cluster.hub);
          return;
        }
        const m = cluster.members.find((mm) => String(mm.id) === String(id));
        if (m) onPick(m);
      });
      await graph.render();
    })();
    return () => {
      cancelled = true;
      void graph?.destroy();
    };
  }, [cluster, onPick]);

  return (
    <div
      ref={host}
      className="w-full h-[220px]"
      role="img"
      aria-label="Duplicate-code cluster graph — hub in the center, exact copies attached around it"
    />
  );
}

function ClusterCard({ root, cluster, index }: { root: string; cluster: Cluster; index: number }) {
  const [picked, setPicked] = useState<Member | null>(null);
  return (
    <Card>
      <CardHeader className="pb-2">
        <div className="flex items-center gap-2 flex-wrap">
          <Badge variant="outline" className="text-[10px]">
            Group {index + 1}
          </Badge>
          <span className="text-sm font-semibold text-stone-900 truncate">{cluster.hub.qualName}</span>
          <span className="text-[11px] text-stone-400 truncate">{cluster.hub.path}</span>
          <Badge className="ml-auto text-[10px]" variant="secondary">
            {cluster.totalMembers} exact cop{cluster.totalMembers === 1 ? "y" : "ies"}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="space-y-2">
        <ClusterGraph cluster={cluster} onPick={setPicked} />
        <div className="flex gap-3 flex-wrap">
          <SnippetPane root={root} member={cluster.hub} title="Hub (the pick)" />
          {picked && picked.id !== cluster.hub.id && (
            <SnippetPane root={root} member={picked} title="Attached duplicate" />
          )}
        </div>
        {!picked && (
          <p className="text-[11px] text-stone-400 px-1">
            Click a node in the graph to compare its code against the hub.
          </p>
        )}
      </CardContent>
    </Card>
  );
}

export function DuplicateClustersView() {
  const { sourceRoot: liveSourceRoot } = useCie();
  const sourceRoot = useCie((s) => s.storageRoot) ?? liveSourceRoot;
  const [data, setData] = useState<ClustersResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const load = useCallback(async (root: string) => {
    const mine = ++reqId.current;
    setLoading(true);
    setError(null);
    try {
      const d = await readDuplicateClusters(root);
      if (mine === reqId.current) setData(d);
    } catch {
      if (mine === reqId.current) setError("Couldn't load duplicate clusters.");
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
        Load a project first — this view reads the captured repository revision.
      </p>
    );
  }
  if (loading) {
    return (
      <p className="text-xs text-stone-500 px-1 flex items-center gap-1.5">
        <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> Loading duplicate clusters…
      </p>
    );
  }
  if (error) return <p className="text-xs text-red-600 px-1">{error}</p>;
  if (!data?.available) {
    return (
      <p className="text-xs text-stone-500 px-1">
        No captured storage revision found for this project yet
        — run it against this folder first, then reload this page.
      </p>
    );
  }
  if (!data.clusters?.length) {
    return <p className="text-xs text-stone-500 px-1">No exact-duplicate families found.</p>;
  }

  return (
    <div className="space-y-3">
      <p className="text-[11px] text-stone-500 px-1">
        {data.totalFamilies} exact-IR-hash duplicate families across {data.totalSymbolsInvolved} symbols — the{" "}
        {data.clusters.length} biggest, each anchored on one representative symbol (its &ldquo;hub&rdquo;) with every
        byte-identical-IR copy attached to it.
      </p>
      {data.truncated && <p role="status" className="text-xs text-amber-700">This is a bounded view; some families or members are omitted.</p>}
      {data.clusters.map((c, i) => (
        <ClusterCard key={c.hub.id} root={sourceRoot} cluster={c} index={i} />
      ))}
    </div>
  );
}
