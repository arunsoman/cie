'use client';
import type { RefObject } from 'react';
import type { RenderHandle } from 'viz-adapter';
export function DiagramNavigation({handle, tight=false}:{handle:RefObject<RenderHandle|null>; tight?:boolean}) {
  const move=async(action:string)=>{if(handle.current){const {navigateDiagram}=await import('@/lib/viz/g6-engine');await navigateDiagram(handle.current,action);}};
  return <div role="group" aria-label="Diagram navigation" className={`flex flex-wrap items-center gap-1 text-xs ${tight ? "my-0.5" : "my-2"}`}>
    <span className="text-stone-500 mr-1">Readable scale · pan to explore</span>
    {([['left','←'],['right','→'],['up','↑'],['down','↓'],['in','+'],['out','−'],['readable','Readable'],['overview','Overview']] as const).map(([action,label])=><button key={action} type="button" aria-label={`Diagram ${action}`} className="rounded border px-2 py-1 hover:bg-stone-100" onClick={()=>void move(action)}>{label}</button>)}
  </div>;
}
