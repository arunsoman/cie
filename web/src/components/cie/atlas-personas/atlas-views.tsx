'use client';
import type { ReactNode } from 'react';
import { AtlasCanvas } from './atlas-canvas';
import { useAtlas, useAtlasState } from './atlas-provider';
import { planPresentation } from '@/lib/atlas-core/atlas-presentation';
import type { AtlasItemRef, AtlasOperation } from '@/lib/atlas-core/atlas-types';
function Item({ item, children }: {
    item: AtlasItemRef;
    children: ReactNode;
}) {
    const { core, interaction } = useAtlas(), state = useAtlasState();
    const selected = state.selection.contributions.some(c => c.item.id === item.id);
    return <button type="button" aria-pressed={selected} title={`${item.label} · click to toggle; double-click or Enter to open evidence`} className={`text-left rounded border px-2 py-1 w-full break-words ${selected ? 'bg-emerald-100 border-emerald-600' : 'border-transparent hover:bg-stone-100'}`} onClick={e => interaction.click(item, e.detail)} onDoubleClick={() => interaction.doubleClick(item)} onKeyDown={e => { if (e.key === 'Enter') {
        e.preventDefault();
        interaction.open(item);
    }
    else if (e.key === ' ') {
        e.preventDefault();
        core.dispatch({ type: 'toggle', item });
    } }}>{children}</button>;
}
export function AtlasSelectionBar() {
    const { core } = useAtlas(), state = useAtlasState();
    return <section aria-label="Atlas selection" className="border rounded-xl p-3 space-y-2">
  <p className="text-sm">Click selects; click again removes; double-click opens captured evidence. Space selects; Enter opens evidence.</p>
  <label>Combine new selections <select aria-label="Selection operation" value={state.selection.mode} onChange={e => core.dispatch({ type: 'mode', operation: e.target.value as AtlasOperation })}>
    <option value="union">OR within group; AND across groups</option><option value="intersection">Intersect</option><option value="exclude">Exclude</option></select></label>
  <button className="underline ml-3" onClick={() => core.dispatch({ type: 'clear' })}>Clear selection</button>
  <ul className="flex flex-wrap gap-2">{state.selection.contributions.map(c => <li className="rounded border p-1" key={c.item.id}>
    <span>{c.item.label}</span> <select aria-label={`Operation for ${c.item.label}`} value={c.operation} onChange={e => core.dispatch({ type: 'operation', itemId: c.item.id, operation: e.target.value as AtlasOperation })}><option value="union">Group OR</option><option value="intersection">AND</option><option value="exclude">Exclude</option></select>
    <button aria-label={`Remove ${c.item.label}`} onClick={() => core.dispatch({ type: 'remove', itemId: c.item.id })}> ×</button></li>)}</ul>
  {!state.selection.contributions.length && <p className="text-xs text-stone-500">Whole captured repository. No active constraints.</p>}
    </section>;
}
export function AtlasRegisteredView({ id }: {
    id: string;
}) {
    const { core, interaction } = useAtlas(), state = useAtlasState(), view = state.views[id];
    if (!view)
        return <p role="status">Preparing {id}…</p>;
    const spec = view.spec;
    const profile = spec?.semantic ? planPresentation(spec.semantic, spec.presentation) : undefined;
    return <section aria-label={spec?.title ?? id} className="rounded-xl border p-3 min-w-0 space-y-2" aria-busy={view.status === 'loading'}>
    <h3 className="font-semibold">{spec?.title ?? id}</h3>
    <p className="text-xs text-stone-600">{view.binding === 'repository' ? 'Repository reference — does not narrow with selection' : 'Shared selection'} · revision {state.pin.revision} · selection version {view.version}</p>
    {view.status === 'loading' && <p role="status">Updating views together… Previous results are temporarily inactive.</p>}
    {view.status === 'error' && <p role="alert">{view.error} <button onClick={() => core.dispatch({ type: 'refresh' })}>Retry</button></p>}
    {spec && <fieldset disabled={view.status !== 'ready'} className="space-y-2">
      <p>{spec.total} {spec.kind === 'bars' || spec.kind === 'pivot' ? 'groups' : view.population?.universe === 'summary' ? 'related summary records' : 'matching symbols'}{spec.continuation ? ' · partial page' : ''}</p>
      {!spec.items.length && <p>No matching data in this captured population.</p>}
      {(spec.kind === 'table' || spec.kind === 'pivot') && !spec.columns && <div className="max-h-[520px] overflow-auto"><table className="w-full text-sm"><thead><tr><th className="text-left">Symbol</th><th className="text-left">File</th></tr></thead><tbody>{spec.items.map(i => <tr key={i.key}><td><Item item={i.ref}>{i.label}</Item></td><td className="break-all">{i.detailRef ? <Item item={i.detailRef}>{i.detail}</Item> : i.detail}</td></tr>)}</tbody></table></div>}
      {spec.columns && <div className="max-h-[520px] overflow-auto"><table className="w-full text-sm"><thead><tr>{spec.columns.map(column => <th key={column.id}>{column.label}{column.unit ? ` (${column.unit})` : ''}</th>)}</tr></thead><tbody>{spec.items.map(i => <tr key={i.key}>{spec.columns!.map(column => <td key={column.id}><Item item={i.ref}>{String(i.fields?.[column.id] ?? 'Unknown')}</Item></td>)}</tr>)}</tbody></table></div>}
      {spec.kind === 'bars' && <ul className="space-y-1">{spec.items.map(i => <li key={i.key}><Item item={i.ref}><span className="text-xs break-all">{i.label} · {i.value}</span><span aria-hidden="true" className="block h-3 bg-emerald-600 rounded" style={{ width: `${Math.max(1, 100 * (i.value ?? 0) / Math.max(1, ...spec.items.map(x => x.value ?? 0)))}%` }}/></Item></li>)}</ul>}
      {profile?.host === 'viz-adapter' && <>
        <div className="flex gap-2 text-xs"><span>Presentation:</span>{(spec.semantic?.kind === 'sequence' ? [] : ['TB', 'LR'] as const).map(direction => <button key={direction} className="underline" onClick={() => core.dispatch({ type: 'presentation', viewId: id, presentation: { ...profile!, layout: { ...profile!.layout, direction } } })}>{direction === 'TB' ? 'Top to bottom' : 'Left to right'}</button>)}<label><input type="checkbox" checked={spec.presentation?.features?.minimap ?? false} onChange={e => core.dispatch({ type: 'presentation', viewId: id, presentation: { ...profile!, features: { ...spec.presentation?.features, minimap: e.target.checked } } })}/> Minimap</label></div>
        <AtlasCanvas spec={spec} disabled={view.status !== 'ready'}/>
        <ul aria-label="Selectable graph nodes" className="max-h-48 overflow-auto">{spec.items.map(i => <li key={i.key}><Item item={i.ref}>{i.label}{i.role === 'context' ? ' · context' : ''}</Item></li>)}</ul>
        <p className="text-xs">{spec.semantic?.kind === 'dependency' ? 'Declared targets are static; runtime dispatch is unobserved.' : spec.semantic?.kind === 'sequence' ? 'Ordered static occurrences; runtime scheduling is unobserved.' : spec.semantic?.kind === 'control-flow' ? 'Blocks and branches come from captured IR.' : 'Recorded hierarchy; visual expansion does not change selection.'}</p>
        <ul aria-label="Selectable call relationships">{spec.links?.map(l => <li key={l.key}><Item item={l.ref}>{l.ref.label}</Item></li>)}</ul>
      </>}
      {spec.gaps.map(g => <p key={g} className="text-xs text-amber-800">{g}</p>)}
      <div className="flex gap-3 text-sm"><button disabled={view.page === 0} onClick={() => core.dispatch({ type: 'page', viewId: id, direction: 'previous' })}>Previous {id}</button><button disabled={!spec.continuation} onClick={() => core.dispatch({ type: 'page', viewId: id, direction: 'next' })}>Next {id}</button></div>
    </fieldset>}
  </section>;
}
export function AtlasEvidencePanel() {
    const { core } = useAtlas(), { evidence: e } = useAtlasState();
    if (e.status === 'closed')
        return null;
    return <section aria-label="Captured evidence" className="rounded-xl border p-3 space-y-2"><h3 className="font-semibold">Captured evidence · {e.item?.label}</h3>
    <button onClick={() => core.dispatch({ type: 'closeEvidence' })}>Close evidence</button>
    {e.status === 'choose' && <ul>{e.item!.evidence.map((ref, index) => <li key={index}><button className="underline" onClick={() => core.dispatch({ type: 'openEvidence', item: e.item!, index })}>{ref.file}{ref.line ? `:${ref.line}` : ''}{ref.startByte != null ? ` · bytes ${ref.startByte}–${ref.endByte}` : ''}</button></li>)}</ul>}
    {e.status === 'loading' && <p role="status">Loading captured source…</p>}
    {e.status === 'unavailable' && <p role="status">{e.error}</p>}
    {e.source && <><p>Revision {e.source.revision} · {e.source.path} · bytes {e.source.returned.start}–{e.source.returned.end}</p><pre className="overflow-auto max-h-[550px] text-xs bg-stone-50 p-3">{e.source.text}</pre>{e.source.truncated && <p>Source response truncated; showing the captured range returned by storage.</p>}</>}
  </section>;
}
