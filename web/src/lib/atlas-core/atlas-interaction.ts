import type {AtlasCore,AtlasItemRef,Contribution} from './atlas-core';
/** Shared pointer arbitration. Delay is configurable for the host's input
 * settings. Slow double clicks also restore the original contribution, without
 * disturbing another item's selection. Keyboard actions are immediate. */
export function createAtlasInteraction(core:AtlasCore,delayMs=600){
  const pending=new Map<string,ReturnType<typeof setTimeout>>();
  const before=new Map<string,Contribution|undefined>();
  let disposed=false;
  const cancel=(id:string)=>{const timer=pending.get(id);if(timer)clearTimeout(timer);pending.delete(id);};
  return {
    click(item:AtlasItemRef,detail=1){if(disposed)return;
      if(detail===0){core.dispatch({type:'toggle',item});return;}
      if(detail>1){cancel(item.id);return;}
      before.set(item.id,core.getState().selection.contributions.find(c=>c.item.id===item.id));
      cancel(item.id);const timer=setTimeout(()=>{pending.delete(item.id);if(!disposed)core.dispatch({type:'toggle',item});},delayMs);
      pending.set(item.id,timer);
    },
    doubleClick(item:AtlasItemRef){if(disposed)return;cancel(item.id);
      if(before.has(item.id)){
        const previous=before.get(item.id),current=core.getState().selection.contributions.find(c=>c.item.id===item.id);
        if(JSON.stringify(previous)!==JSON.stringify(current))core.dispatch({type:'restoreContribution',itemId:item.id,contribution:previous});
        before.delete(item.id);
      }
      core.dispatch({type:'openEvidence',item});
    },
    open(item:AtlasItemRef){if(!disposed)core.dispatch({type:'openEvidence',item});},
    dispose(){disposed=true;for(const id of pending.keys())cancel(id);before.clear();},
  };
}
