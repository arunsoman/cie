'use client';
import { useEffect, useState } from 'react';
import { useAtlas, useAtlasState } from './atlas-provider';
import { loadNamedViews, saveNamedView, sessionPermalink, readPermalink } from '@/lib/atlas-core/atlas-workspace';
import { atlasRevisions, compareAtlasRevisions } from '@/lib/atlas-core/atlas-revisions';
import { atlasStorage } from '@/lib/atlas-core/atlas-storage';
import { useCie } from '@/lib/cie-store';
export function AtlasWorkspaceControls() {
    const { core, root, interaction } = useAtlas(), state = useAtlasState();
    const [query, setQuery] = useState(''), [name, setName] = useState(''), [saved, setSaved] = useState<string[]>([]), [message, setMessage] = useState(''), [revisions, setRevisions] = useState<string[]>([]), [base, setBase] = useState('');
    const action = (fn: () => void | Promise<void>) => { void Promise.resolve().then(fn).catch(e => setMessage(String(e))); };
    useEffect(() => {
        try {
            setSaved(Object.keys(loadNamedViews(localStorage, state.pin)));
        }
        catch (e) {
            setMessage(String(e));
        }
        let cancelled = false;
        void atlasRevisions(root).then(data => { if (!cancelled) {
            setRevisions(data.items.map(i => i.revision));
            core.dispatch({ type: 'freshness', value: { status: 'known', indexedRevision: state.pin.revision, headRevision: data.items.find(i => i.head)?.revision, reason: 'Revision identity is known; parse coverage and commit lag were not supplied.' } });
        } }).catch(e => { if (!cancelled)
            setMessage(String(e)); });
        return () => { cancelled = true; };
    }, [core, root, state.pin]);
    const cube = (dimensions: string[], rows: string[] = [], columns: string[] = []) => core.dispatch({ type: 'cube', viewId: 'kinds', controls: { dimensions, rows, columns, measures: ['count'] } });
    return <section className="rounded-xl border p-3 space-y-2" aria-label="Atlas workspace controls">
  <p className="text-xs">Captured revision {state.pin.revision} · head {state.freshness.headRevision ?? 'unknown'} · parse completeness {state.freshness.parsedRatio == null ? 'not supplied' : `${Math.round(state.freshness.parsedRatio * 100)}%`}</p>
  {state.notice && <p role="status">{state.notice}</p>}
  <div className="flex flex-wrap gap-3 items-center">
   <form onSubmit={e => { e.preventDefault(); core.dispatch({ type: 'search', spec: { text: query } }); }}><label>Find symbol <input className="border rounded px-2" value={query} onChange={e => setQuery(e.target.value)}/></label><button className="underline ml-2" type="submit">Search</button></form>
   <label><input type="checkbox" checked={state.policy.factsOnly} disabled={state.policy.locked} onChange={e => core.dispatch({ type: 'policy', factsOnly: e.target.checked })}/> Facts only{state.policy.locked ? ' (required)' : ''}</label>
   <button className="underline" onClick={() => { core.dispatch({ type: 'workspace', patch: { dockOpen: !state.workspace.dockOpen } }); useCie.getState().setChatOpen(!state.workspace.dockOpen); }}>Toggle dock</button>
   <button className="underline" disabled={!state.workspace.breadcrumbs.length} onClick={() => core.dispatch({ type: 'back' })}>Back one scope</button>
  </div>
  {state.search.status === 'loading' && <p role="status">Searching captured symbols…</p>}
  {state.search.status === 'error' && <p role="alert">{state.search.error}</p>}
  {state.search.status === 'ready' && <div><p>{state.search.total} search matches</p><ul className="max-h-40 overflow-auto">{state.search.items.map(item => <li key={item.id} className="flex gap-2"><button className="underline" aria-pressed={core.isSelected(item.id)} onClick={e => interaction.click(item, e.detail)} onDoubleClick={() => interaction.doubleClick(item)}>{item.label}</button><button onClick={() => interaction.open(item)}>Evidence</button><button onClick={() => core.dispatch({ type: 'search', spec: { calls: { id: item.id, incoming: true } } })}>Callers</button><button onClick={() => core.dispatch({ type: 'scope', scope: { id: item.id, label: item.label, kind: item.entityKind ?? 'symbol', query: item.operand } })}>Scope</button></li>)}</ul>{state.search.continuation && <button onClick={() => core.dispatch({ type: 'search', spec: state.search.spec!, continuation: state.search.continuation })}>More search results</button>}</div>}
  <details><summary>Workspace, cube and revision controls</summary><div className="space-y-3 pt-2">
   <div className="flex flex-wrap gap-3"><label>View name <input className="border" value={name} onChange={e => setName(e.target.value)}/></label><button onClick={() => action(() => { saveNamedView(localStorage, name, core.exportSession()); setSaved(Object.keys(loadNamedViews(localStorage, state.pin))); setMessage('View saved in this browser.'); })}>Save view</button>
    <select aria-label="Saved Atlas views" defaultValue="" onChange={e => { if (e.target.value)
        action(() => core.restoreSession(loadNamedViews(localStorage, state.pin)[e.target.value])); }}><option value="">Restore saved view…</option>{saved.map(n => <option key={n}>{n}</option>)}</select>
    <button onClick={() => action(() => { history.replaceState(null, '', sessionPermalink(core.exportSession())); setMessage('Share the current address; it contains references, not captured source code.'); })}>Create permalink</button>
    <button onClick={() => action(() => { core.restoreSession(readPermalink(location.hash)); setMessage('Permalink restored.'); })}>Restore permalink</button></div>
   <div className="flex gap-3 flex-wrap"><span>Aggregate selected contributors:</span><button className="underline" onClick={() => cube(['kind'])}>By kind</button><button className="underline" onClick={() => cube(['language'])}>By language</button><button className="underline" onClick={() => cube(['language', 'kind'], ['language'], ['kind'])}>Language × kind cells</button></div>
   <div className="flex gap-3 flex-wrap"><label>Captured revision <select value={state.pin.revision} onChange={e => action(async () => { await core.changeRevision({ ...state.pin, revision: e.target.value }, atlasStorage(root, e.target.value)); })}>{revisions.map(rev => <option key={rev}>{rev}</option>)}</select></label>
    <label>Compare from <select value={base} onChange={e => setBase(e.target.value)}><option value="">Choose revision…</option>{revisions.map(rev => <option key={rev}>{rev}</option>)}</select></label>
    <button disabled={!base} onClick={() => action(async () => { const context = await compareAtlasRevisions(root, { ...state.pin, revision: base }, state.pin, new AbortController().signal); core.dispatch({ type: 'changeContext', context }); setMessage(`${context.changedFiles.length} changed files; ${context.deletedFiles?.length ?? 0} deleted; ${context.unknownFiles?.length ?? 0} lack captured hashes.`); })}>Apply captured change context</button>
    <button onClick={() => core.dispatch({ type: 'changeContext', context: undefined })}>Clear change context</button></div>
  </div></details>
  {state.changeContext && <p className="text-xs">Change scope: {state.changeContext.base.revision} → {state.changeContext.head.revision}; {state.changeContext.changedFiles.length} current files changed. Deleted/unknown files are disclosed separately, not silently mapped to current symbols.</p>}
  {message && <p role="status" className="text-xs">{message}</p>}
 </section>;
}
