'use client';
import {useEffect,useState} from 'react';
import {useCie} from '@/lib/cie-store';
import {storageChart,type CubeView} from '@/lib/storage-chart';
import {storageFor} from '@/lib/storage-ui';
import {output,type EntityPage} from '@/lib/storage-client';
import {ChatChart} from './chat-chart';
export function StorageCubeView(){
  const root=useCie(s=>s.storageRoot??s.sourceRoot),revision=useCie(s=>s.report?.rev);
  const [dimension,setDimension]=useState('language'),[history,setHistory]=useState<(string|null)[]>([null]),[groupPage,setGroupPage]=useState(0);
  const [view,setView]=useState<CubeView|null>(null),[error,setError]=useState('');
  const [descriptor,setDescriptor]=useState<string|null>(null),[rows,setRows]=useState<EntityPage|null>(null),[rowPage,setRowPage]=useState(0);
  useEffect(()=>{if(!root||revision==null)return;const abort=new AbortController();setView(null);setError('');setDescriptor(null);setRows(null);
    void storageFor(root).retrieve({query:{select:{kind:'symbol'}},outputs:[{id:'chart',output:{view:{kind:'hbar',source:{output:'cube'}}}},{id:'cube',output:{cube:{dimensions:[{id:'group',dimension:{field:dimension}}],page:{limit:30,continuation:history[groupPage]}}}}]},abort.signal)
      .then(data=>{if(!abort.signal.aborted)setView(output<CubeView>(data,'chart','view'));}).catch(e=>{if(!abort.signal.aborted)setError(String(e));});
    return()=>abort.abort();
  },[root,revision,dimension,history,groupPage]);
  useEffect(()=>{if(!root||!descriptor)return;const abort=new AbortController();setRows(null);setError('');
    void storageFor(root).entities({cell_contributors:{descriptor}},rowPage,30,abort.signal).then(({data})=>{if(!abort.signal.aborted)setRows(data);}).catch(e=>{if(!abort.signal.aborted)setError(String(e));});
    return()=>abort.abort();
  },[root,revision,descriptor,rowPage]);
  const inspect=(entity:EntityPage['items'][number])=>{const state=useCie.getState();const loc=entity.fields.location as {file?:string;start?:number;end?:number};const found=state.report?.symbols.find(s=>state.report?.files[s.loc.file]===loc?.file&&s.loc.start===loc.start&&s.loc.end===loc.end);if(found&&['Function','Method','Test'].includes(found.kind))state.selectFunction(Number(found.id));else if(loc?.file)state.openSourceViewer({file:loc.file,byteStart:loc.start,byteEnd:loc.end});};
  if(!root)return null;
  return <section className="space-y-3 rounded-xl border border-stone-200 p-4" aria-label="Storage exploration">
    <label>Group stored symbols by <select value={dimension} onChange={e=>{setDimension(e.target.value);setGroupPage(0);setHistory([null]);}} className="rounded border p-1"><option value="language">Language</option><option value="kind">Symbol kind</option><option value="file.path">File</option></select></label>
    {error&&<p role="alert">{error}</p>}
    {view?<><ChatChart spec={storageChart(view)}/><p className="text-xs text-stone-500">Select a group to inspect its symbols. Showing group page {groupPage+1}{view.cells.total!=null?` · ${view.cells.total} groups total`:''}.</p>
      <div className="flex flex-wrap gap-2">{view.cells.items.map((cell,i)=><button className="rounded border px-2 py-1 text-sm" key={i} disabled={!cell.contributors} onClick={()=>{setDescriptor(cell.contributors??null);setRowPage(0);}}>{String(cell.members.group??'Unknown')} · {cell.measures.count??''}</button>)}</div>
      <div className="flex gap-3"><button disabled={groupPage===0} onClick={()=>setGroupPage(p=>p-1)}>Previous groups</button><button disabled={!view.cells.continuation} onClick={()=>{setHistory(h=>[...h.slice(0,groupPage+1),view.cells.continuation!]);setGroupPage(p=>p+1);}}>Next groups</button></div>
    </>:<p role="status">Loading stored facts…</p>}
    {descriptor&&(rows?<><p role="status">Showing {rows.total?rowPage*30+1:0}–{Math.min(rowPage*30+rows.items.length,rows.total)} of {rows.total} symbols</p>
      <div className="overflow-auto"><table className="w-full text-sm"><thead><tr><th className="text-left">Symbol</th><th className="text-left">Kind</th><th className="text-left">File</th></tr></thead><tbody>{rows.items.map(row=>{const file=String((row.fields.file as {path?:string})?.path??'');return <tr key={row.id}><td><button className="text-left underline" onClick={()=>inspect(row)}>{String(row.fields.qualified_name??row.fields.name??row.id)}</button></td><td>{String(row.fields.kind??'')}</td><td><button className="text-left underline break-all" onClick={()=>{const s=useCie.getState();s.setAtlasSelection({kind:'module',id:file,label:file.split('/').pop()??file,loc:{file}});s.setView('atlas');}}>{file}</button></td></tr>;})}</tbody></table></div>
      <div className="flex gap-3"><button disabled={rowPage===0} onClick={()=>setRowPage(p=>p-1)}>Previous symbols</button><button disabled={!rows.continuation} onClick={()=>setRowPage(p=>p+1)}>Next symbols</button></div>
    </>:<p role="status">Loading contributor page…</p>)}
  </section>;
}
