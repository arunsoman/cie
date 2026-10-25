'use client';
import {useAtlasDetails} from './atlas-display';
import type { CSSProperties } from 'react';
import { useAtlas, useAtlasState } from './atlas-provider';
import { AtlasSelectionBar, AtlasRegisteredView, AtlasEvidencePanel } from './atlas-views';
export function AtlasLayoutView() {
    const { core, catalog } = useAtlas(), state = useAtlasState();
    const plan = catalog.plans.resolve(state.persona, !!state.changeContext), layout = plan.layout;
    const details=useAtlasDetails(), showSelection=details||state.selection.contributions.length>0;
    const areas = (rows: string[]) => rows.filter(r=>showSelection||!r.startsWith('r4')).map(r => `"${r}"`).join(' ');
    const expanded = state.workspace.expandedPanel;
    const choices = catalog.registry.viewIds();
    return <>
  <style>{`.atlas-semantic-grid{display:grid;grid-template-columns:repeat(8,minmax(0,1fr));grid-template-areas:var(--atlas-areas);grid-auto-rows:minmax(var(--atlas-row),auto);gap:12px}.atlas-semantic-slot{min-width:0;overflow:auto}.atlas-semantic-slot[data-chrome=canvas]{overflow:visible}.atlas-semantic-slot>section{height:100%}@media(max-width:1279px){.atlas-semantic-grid{grid-template-columns:repeat(2,minmax(0,1fr));grid-template-areas:var(--atlas-md)}}@media(max-width:767px){.atlas-semantic-grid{grid-template-columns:1fr;grid-template-areas:var(--atlas-base)}}`}</style>
  {expanded ? <><AtlasSelectionBar /><button className="underline" onClick={() => core.dispatch({ type: 'workspace', patch: { expandedPanel: null } })}>Show all panels</button><AtlasRegisteredView id={expanded}/></> : <div className="atlas-semantic-grid" style={{ '--atlas-areas': areas(layout.areas), '--atlas-md': areas(layout.areasMd), '--atlas-base': areas(layout.areasBase), '--atlas-row': `${layout.rowHeightPx}px` } as CSSProperties}>
   {showSelection&&<div style={{ gridArea: 'r4' }}><AtlasSelectionBar /></div>}
   {layout.slots.map(slot => {
                const viewId = state.workspace.placements?.[slot.area] ?? slot.viewId;
                return <div key={slot.area} className="atlas-semantic-slot" data-chrome={slot.chrome} style={{ gridArea: slot.area, minHeight: slot.minHeightPx }}>
    {details&&<><button className="text-xs underline" aria-label={`Expand ${viewId}`} onClick={() => core.dispatch({ type: 'workspace', patch: { expandedPanel: viewId } })}>Expand</button>
    <label className="text-xs ml-2">Chart <select aria-label={`Chart in ${slot.area}`} value={viewId} onChange={e => core.dispatch({ type: 'placeView', area: slot.area, viewId: e.target.value })}>{choices.map(id => <option key={id}>{id}</option>)}</select></label></>}
    <AtlasRegisteredView id={viewId}/>
   </div>;
            })}
  </div>}
  <AtlasEvidencePanel />
 </>;
}
