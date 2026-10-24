import type {AtlasPin,AtlasQuery,AtlasStorage,AtlasViewDefinition,AtlasState,AtlasCommand,AtlasViewState,AtlasItemRef} from './atlas-types';
import {assertItem,initialSelection,samePin,selectionQuery,toggle,setOperation,validateQuery} from './atlas-selection';
import {assertDefinition,assertView} from './atlas-conformance';
export * from './atlas-types';
/** One instance per repository revision. Persona switches never recreate it. */
export function createAtlasCore(options:{pin:AtlasPin;base:AtlasQuery;storage:AtlasStorage;views:AtlasViewDefinition[];persona:string}) {
  validateQuery(options.base);
  const definitions=new Map<string,AtlasViewDefinition>();
  for(const definition of options.views){assertDefinition(definition);if(definitions.has(definition.id))throw new Error('Duplicate Atlas view');definitions.set(definition.id,definition);}
  // Own every item and state snapshot: callers cannot mutate identity or selection indirectly.
  const copy=<T,>(value:T):T=>JSON.parse(JSON.stringify(value));
  const freeze=<T,>(value:T):T=>{if(value&&typeof value==='object'){Object.values(value).forEach(freeze);Object.freeze(value);}return value;};
  const base=freeze(copy(options.base));
  let state:AtlasState=freeze({pin:copy(options.pin),persona:options.persona,version:0,selection:initialSelection(),views:{},evidence:{status:'closed'}});
  const listeners=new Set<()=>void>(),pages=new Map<string,(string|null)[]>();
  let disposed=false,controller:AbortController|undefined,evidenceController:AbortController|undefined;
  let epoch=0,evidenceEpoch=0;
  const publish=(next:AtlasState)=>{if(disposed)return;state=freeze(next);listeners.forEach(l=>l());};
  const selected=(id:string)=>state.selection.contributions.some(c=>c.item.id===id);
  async function refresh(reset=false){
    if(disposed)return;
    controller?.abort();controller=new AbortController();const signal=controller.signal,requestEpoch=++epoch;
    if(reset)pages.clear();
    const version=state.version,views={...state.views};
    definitions.forEach(v=>{views[v.id]={...views[v.id],binding:v.binding,status:'loading',version,page:pages.get(v.id)?.length?pages.get(v.id)!.length-1:0};});
    publish({...state,views});
    // Each binding uses ONE storage request for all its projections. UI commits
    // the entire result batch together so counts/tables cannot mix query versions.
    const query=selectionQuery(base,state.selection);
    const batches=await Promise.all((['selection','repository'] as const).map(async binding=>{
      const defs=[...definitions.values()].filter(v=>v.binding===binding);if(!defs.length)return [];
      const boundQuery=binding==='repository'?base:query;
      try{
        const outputs=defs.flatMap(d=>d.outputs(pages.get(d.id)?.slice(-1)[0]??null).map(o=>({id:`${d.id}__${o.id}`,output:o.output})));
        const result=await options.storage.retrieve(boundQuery,outputs,signal);
        if(!samePin(state.pin,{repository:result.repository,revision:result.revision}))throw new Error('Storage returned a mixed repository/revision');
        return defs.map(d=>{
          try{
            const local={...result,outputs:result.outputs.filter(o=>o.id.startsWith(`${d.id}__`)).map(o=>({...o,id:o.id.slice(d.id.length+2)}))};
            const spec=d.project(local,{pin:state.pin,query:boundQuery,version});assertView(spec,state.pin);
            return [d.id,{binding,status:'ready',version,page:pages.get(d.id)?.length?pages.get(d.id)!.length-1:0,spec}] as [string,AtlasViewState];
          }catch(e){return [d.id,{...views[d.id],status:'error',error:String(e)}] as [string,AtlasViewState];}
        });
      }catch(e){return defs.map(d=>[d.id,{...views[d.id],status:'error',error:String(e)}] as [string,AtlasViewState]);}
    }));
    if(disposed||signal.aborted||requestEpoch!==epoch)return;
    publish({...state,views:Object.fromEntries(batches.flat())});
  }
  async function openEvidence(item:AtlasItemRef,index?:number){
    assertItem(item,state.pin);evidenceController?.abort();const requestEpoch=++evidenceEpoch;
    if(!item.evidence.length){publish({...state,evidence:{status:'unavailable',item,error:'No captured source or evidence location for this item.'}});return;}
    if(index==null&&item.evidence.length>1){publish({...state,evidence:{status:'choose',item}});return;}
    const evidence=item.evidence[index??0];if(!evidence){publish({...state,evidence:{status:'unavailable',item,error:'Invalid evidence choice'}});return;}
    evidenceController=new AbortController();const signal=evidenceController.signal;
    publish({...state,evidence:{status:'loading',item}});
    try{const source=await options.storage.source(evidence,signal);
      if(source.revision!==state.pin.revision)throw new Error('Captured source belongs to another revision');
      if(!signal.aborted&&!disposed&&requestEpoch===evidenceEpoch)publish({...state,evidence:{status:'ready',item,source}});
    }catch(e){if(!signal.aborted&&!disposed&&requestEpoch===evidenceEpoch)publish({...state,evidence:{status:'unavailable',item,error:String(e)}});}
  }
  const core={
    getState:()=>state,
    subscribe(listener:()=>void){if(disposed)throw new Error('Atlas disposed');listeners.add(listener);return()=>{listeners.delete(listener);};},
    isSelected:selected,
    dispatch(command:AtlasCommand){
      if(disposed)throw new Error('Atlas disposed');
      if((command.type==='mode'||command.type==='operation')&&!['union','intersection','exclude'].includes(command.operation))throw new Error('Invalid selection operation');
      switch(command.type){
        case 'openEvidence':assertItem(command.item,state.pin);void openEvidence(copy(command.item),command.index);return;
        case 'closeEvidence':evidenceController?.abort();++evidenceEpoch;publish({...state,evidence:{status:'closed'}});return;
        case 'persona':publish({...state,persona:command.persona});return;
        case 'mode':publish({...state,selection:{...state.selection,mode:command.operation}});return;
        case 'refresh':void refresh();return;
        case 'page':{
          const v=state.views[command.viewId];if(!v||v.status!=='ready')return;
          const history=pages.get(command.viewId)??[null];
          if(command.direction==='next'){if(!v.spec?.continuation)return;pages.set(command.viewId,[...history,v.spec.continuation]);}
          else {if(history.length===1)return;pages.set(command.viewId,history.slice(0,-1));}
          void refresh();return;
        }
      }
      let selection=state.selection;
      if(command.type==='toggle'){assertItem(command.item,state.pin);selection=toggle(selection,copy(command.item));}
      else if(command.type==='remove')selection={...selection,contributions:selection.contributions.filter(c=>c.item.id!==command.itemId)};
      else if(command.type==='restoreContribution'){
        if(command.contribution){assertItem(command.contribution.item,state.pin);if(command.contribution.item.id!==command.itemId)throw new Error('Contribution identity mismatch');}
        selection={...selection,contributions:[...selection.contributions.filter(c=>c.item.id!==command.itemId),...(command.contribution?[copy(command.contribution)]:[])]};
      }
      else if(command.type==='clear')selection={...selection,contributions:[]};
      else if(command.type==='operation')selection=setOperation(selection,command.itemId,command.operation);
      publish({...state,version:state.version+1,selection});void refresh(true);
    },
    exportSession:()=>copy({schemaVersion:1 as const,pin:state.pin,persona:state.persona,selection:state.selection}),
    restoreSession(session:{schemaVersion:number;pin:AtlasPin;persona:string;selection:AtlasState['selection']}){
      if(disposed)throw new Error('Atlas disposed');
      if(session.schemaVersion!==1||!samePin(session.pin,state.pin))throw new Error('Atlas session requires explicit revision revalidation');
      if(!['union','intersection','exclude'].includes(session.selection.mode))throw new Error('Invalid selection operation');
      const seen=new Set<string>();
      for(const c of session.selection.contributions){assertItem(c.item,state.pin);if(seen.has(c.item.id)||!['union','intersection','exclude'].includes(c.operation))throw new Error('Invalid session contribution');seen.add(c.item.id);}
      publish({...state,version:state.version+1,persona:session.persona,selection:copy(session.selection)});void refresh(true);
    },
    start:()=>refresh(),
    dispose(){if(disposed)return;disposed=true;controller?.abort();evidenceController?.abort();listeners.clear();void options.storage.close().catch(()=>{});},
  };
  return core;
}
export type AtlasCore=ReturnType<typeof createAtlasCore>;
