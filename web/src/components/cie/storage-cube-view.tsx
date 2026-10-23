'use client';
import { useEffect, useState } from 'react';
import { useCie } from '@/lib/cie-store';
import { storageChart, type CubeView } from '@/lib/storage-chart';
import { ChatChart } from './chat-chart';
async function call<T>(body: Record<string, unknown>, signal?: AbortSignal): Promise<T> {
  const response = await fetch('/api/storage', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body), signal });
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || 'Storage request failed');
  return data;
}
export function StorageCubeView() {
  const root = useCie(s => s.storageRoot ?? s.sourceRoot);
  const revision = useCie(s => s.report?.rev);
  const [dimension, setDimension] = useState('language');
  const [view, setView] = useState<CubeView | null>(null);
  const [context, setContext] = useState<string | null>(null);
  const [error, setError] = useState('');
  const [rows, setRows] = useState<unknown[] | null>(null);
  useEffect(() => {
    if (!root || revision == null) return;
    let active = true; let opened: string | undefined;
    const abort = new AbortController();
    setView(null); setRows(null); setError(''); setContext(null);
    void (async () => {
      try {
        // Do not abort open: its handle must be received and closed on cleanup.
        const result = await call<{context: string}>({ op: 'open', root, revision: String(revision) });
        opened = result.context;
        if (!active) { await call({op:'close', context: opened}); return; }
        setContext(opened);
        const data = await call<{ outputs: Array<{ data: CubeView }> }>({ op:'retrieve', context: opened, request: {
          query: { select: { kind: 'symbol' } }, outputs: [
            { id:'chart', output:{view:{kind:'hbar',source:{output:'cube'}}} },
            { id:'cube', output:{cube:{dimensions:[{id:'group',dimension:{field:dimension}}],page:{limit:30}}} },
          ],
        } }, abort.signal);
        if (active) setView(data.outputs[0].data);
      } catch (e) { if (active) setError(e instanceof Error ? e.message : 'Storage unavailable'); }
    })();
    return () => { active = false; abort.abort(); if (opened) void call({op:'close',context:opened}).catch(() => {}); };
  }, [root, revision, dimension]);
  if (!root) return null;
  return <section className="space-y-3 rounded-xl border border-stone-200 p-4" aria-label="Storage exploration">
    <label>Group stored symbols by <select value={dimension} onChange={e => setDimension(e.target.value)} className="rounded border p-1">
      <option value="language">Language</option><option value="kind">Symbol kind</option><option value="file.path">File</option>
    </select></label>
    {error ? <p role="alert">{error}</p> : view ? <>
      <ChatChart spec={storageChart(view)} />
      <p className="text-xs text-stone-500">Select a group to retrieve its contributing symbols.</p>
      <div className="flex flex-wrap gap-2">{view.cells.items.map((cell,i) => <button className="rounded border px-2 py-1 text-sm" key={i} disabled={!context || !cell.contributors} onClick={() => {
        setError(''); void call<{outputs:Array<{data:{items:unknown[]}}>}>({op:'retrieve',context,request:{query:{cell_contributors:{descriptor:cell.contributors}},outputs:[{id:'rows',output:{entities:{fields:['qualified_name','file','kind'],page:{limit:30}}}}]}})
          .then(data => setRows(data.outputs[0].data.items)).catch(e => setError(String(e)));
      }}>{String(cell.members.group ?? 'Unknown')}</button>)}</div>
      {rows && <pre className="max-h-64 overflow-auto text-xs">{JSON.stringify(rows,null,2)}</pre>}
    </> : <p>Loading stored facts…</p>}
  </section>;
}
