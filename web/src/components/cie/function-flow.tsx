'use client';
import {useEffect,useState} from 'react';
import type {CieReport,SymbolInfo} from '@/lib/cie-types';
import type {ChatChartSpec} from '@/lib/chat-chart';
import {storageFor,readFunctionIr,readCapturedSource} from '@/lib/storage-ui';
import {output,type EntityPage} from '@/lib/storage-client';
import {cfgChart} from '@/lib/behaviour-views';
import {ChatChart} from './chat-chart';
export function FunctionFlow({root,report,symbol}:{root:string|null;report:CieReport;symbol:SymbolInfo}) {
  const [state,setState]=useState<{key:string;spec?:ChatChartSpec;error?:string}>({key:''});
  const file=report.files[symbol.loc.file],key=JSON.stringify([root,report.rev,file,symbol.loc.start,symbol.loc.end]);
  useEffect(()=>{const abort=new AbortController();setState({key});
    void(async()=>{try{
      if(!root)throw new Error('Captured function IR is unavailable for this workspace.');
      const selected=await storageFor(root).retrieve({query:{select:{kind:'symbol',filter:{and:[{field:{field:'qualified_name',op:'eq',value:symbol.qual_name}},{field:{field:'file.path',op:'eq',value:file}}]}}},outputs:[{id:'rows',output:{entities:{page:{limit:20}}}}]},abort.signal);
      const page=output<EntityPage>(selected,'rows','entities');
      const matches=page.items.filter(e=>{const loc=e.fields.location as {start:number;end:number};return loc.start===symbol.loc.start&&loc.end===symbol.loc.end;});
      if(matches.length!==1)throw new Error('No unique captured declaration for this function.');
      const [ir,source]=await Promise.all([readFunctionIr(root,matches[0].id,abort.signal),readCapturedSource(root,file,symbol.loc.start,symbol.loc.end,abort.signal)]);
      if(!abort.signal.aborted)setState({key,spec:cfgChart(ir,source,report,Number(symbol.id))});
    }catch(error){if(!abort.signal.aborted)setState({key,error:error instanceof Error?error.message:String(error)});}})();
    return()=>abort.abort();
  },[key,root,report.rev,file,symbol.qual_name,symbol.loc.start,symbol.loc.end]);
  if(state.key!==key||(!state.spec&&!state.error))return <p role="status">Loading captured control flow…</p>;
  if(state.error)return <p role="status">Control flow unavailable: {state.error}</p>;
  return <ChatChart spec={state.spec!}/>;
}
