import {gate,recomputeRenderHash,recomputeSemanticHash,type ViewSpec,type ViewElement,type ViewEdge} from 'viz-adapter';
import type {AtlasPin,AtlasViewSpec,AtlasItemRef,AtlasAuthority} from './atlas-types';
/** Bridge to the existing sealed ViewSpec. Never invent source spans for aggregates. */
export function atlasVisualization(spec:AtlasViewSpec,pin:AtlasPin):ViewSpec {
 const location=(ref:AtlasItemRef)=>{const e=ref.evidence.find(e=>e.startByte!=null&&e.endByte!=null);return e?{file:e.file,start:e.startByte!,end:e.endByte!}:null;};
 const mode=(a?:AtlasAuthority)=>a==='Fact'?'Fact':a==='Hypothesis'?'Hypothesis':a==='Inference'?'Inference':'Fog';
 const elements:ViewElement[]=spec.items.flatMap(i=>{const evidence=location(i.ref);if(!evidence)return [];const displayMode=mode(i.authority);return [{id:i.key,elementKind:spec.kind==='flow'?'process':'symbol',label:i.label,evidence,displayMode,
  ...(displayMode!=='Fact'?{claimId:`atlas:${i.key}`,counterArgument:'Static inference is not an observed execution.',fogReason:'Producer did not establish authority.'}:{}),
  salience:{factors:['selected-population'],weight:i.role==='member'?1:0.5,whyShown:i.role},fields:{atlasItemId:i.ref.id}}];});
 const ids=new Set(elements.map(i=>i.id));
 const edges:ViewEdge[]=(spec.links??[]).flatMap(e=>{const evidence=location(e.ref);return evidence&&ids.has(e.from)&&ids.has(e.to)?[{id:e.key,source:e.from,target:e.to,edgeKind:e.label,displayMode:mode(e.authority),evidence,fields:{atlasItemId:e.ref.id}}]:[];});
 const hidden=spec.items.filter(i=>!ids.has(i.key)).map(i=>({id:i.key,label:i.label,kind:'symbol',salienceWeight:1,reason:'No captured byte-span evidence; available in accessible outline.'}));
 const revision=Number(pin.revision);if(!Number.isSafeInteger(revision))throw new Error('Visualization adapter requires a numeric captured revision');
 const view:ViewSpec={elements,edges,hidden,gaps:[...spec.gaps.map((why,n)=>({id:`gap:${n}`,area:'coverage',why})),...(hidden.length?[{id:'evidence',area:'evidence',why:`${hidden.length} items have no captured source span.`}]:[])],
 footer:{facts:elements.filter(i=>i.displayMode==='Fact').length,inferences:elements.filter(i=>i.displayMode==='Inference').length,hypotheses:elements.filter(i=>i.displayMode==='Hypothesis').length,fog:elements.filter(i=>i.displayMode==='Fog').length,stale:0,caption:spec.title},
 scope:{revision,environments:[],coverage:{analyzed:false}},layoutHint:{strategy:spec.kind==='sequence'?'sequence-lanes':'dagre'},semanticHash:'',renderHash:''};
 view.semanticHash=recomputeSemanticHash(view);view.renderHash=recomputeRenderHash(view,'');gate(view);return view;
}
