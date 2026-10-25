'use client';
import {useEffect,useState} from 'react';
import {AtlasLayoutView} from './atlas-layout-view';
import {AtlasWorkspaceControls} from './atlas-workspace-controls';
import {AtlasDisplayContext} from './atlas-display';
import {useAtlas} from './atlas-provider';
import {useCie} from '@/lib/cie-store';
/** Persona composition only; all data, selection and chart rendering stay shared. */
export function ArchitectAtlas(){
 const [details,setDetails]=useState(false),{core}=useAtlas();
 useEffect(()=>{core.dispatch({type:'workspace',patch:{dockOpen:false}});useCie.getState().setChatOpen(false);},[core]);
 return <AtlasDisplayContext.Provider value={details}><div className="p-4 space-y-3" data-testid="atlas-core-architect" data-display={details?'details':'charts'}>
  <div className="flex justify-end"><button className="rounded-lg border px-3 py-2 text-sm hover:bg-stone-50" aria-expanded={details} onClick={()=>setDetails(!details)}>{details?'Hide controls and details':'Show controls and details'}</button></div>
  {details&&<><h2 className="font-semibold">Architect · captured architecture orientation</h2>
   <p className="text-sm">Select any chart element to filter all charts. Click again to remove it; double-click opens captured evidence. OR combines values in a dimension; AND combines dimensions. Exclude subtracts a contribution.</p>
   <p className="text-xs text-stone-600">Declared dependencies are not runtime calls. Bounded contexts are candidates. Layer rules, literal clone ribbons/FCA overlap, paired drift comparison, revision timeline, quality collection and cohesion remain incomplete. Missing results are not zero.</p>
   <AtlasWorkspaceControls />
  </>}
  <AtlasLayoutView />
 </div></AtlasDisplayContext.Provider>;
}
