const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const load=require('./helpers/load-ts.cjs')();
const {createAtlasCore}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-core.ts'));
const {selectionQuery,initialSelection,toggle}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-selection.ts'));
const {createAtlasInteraction}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-interaction.ts'));
const {reviewerViews}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-projections.ts'));
const pin={repository:'repo',revision:'7'},base={select:{kind:'symbol'}};
const item=(id,group='file',entityIds=[id])=>({id,label:id,pin,group,operand:{select:{kind:'symbol',filter:{ids:entityIds}}},evidence:[{file:id+'.java',startByte:0,endByte:20}]});
function evaluate(q,universe=['a','b','c','d']){
 if(q.select)return new Set(q.select.filter?.ids??universe);
 if(q.union)return new Set(q.union.flatMap(i=>[...evaluate(i,universe)]));
 if(q.intersect)return q.intersect.map(i=>evaluate(i,universe)).reduce((a,b)=>new Set([...a].filter(x=>b.has(x))));
 const a=evaluate(q.difference.left,universe),b=evaluate(q.difference.right,universe);return new Set([...a].filter(x=>!b.has(x)));
}
const settle=()=>new Promise(r=>setImmediate(r));
const view=(id='table',binding='selection')=>({id,binding,outputs:()=>[{id:'rows',output:{entities:{page:{limit:30}}}}],project:r=>({kind:'table',title:id,items:[],total:r.matches,continuation:null,gaps:[]})});
function harness(views=[view()]){
 const calls=[];let closeCount=0;
 const storage={retrieve:(query,outputs,signal)=>new Promise((resolve,reject)=>calls.push({query,outputs,signal,resolve,reject})),source:async e=>({path:e.file,text:'captured',revision:'7',truncated:false,returned:{start:0,end:20}}),close:async()=>{closeCount++;}};
 const core=createAtlasCore({pin,base,storage,views,persona:'reviewer'});
 const reply=(n,matches=1,override={})=>calls[n].resolve({repository:'repo',revision:'7',matches,outputs:[],...override});
 return {core,calls,storage,reply,get closeCount(){return closeCount;}};
}
test('group OR, cross-group AND and explicit exclusion match an independent set oracle',()=>{
 let s=toggle(toggle(initialSelection(),item('A','file',['a','b'])),item('B','file',['b','c']));
 assert.deepEqual([...evaluate(selectionQuery(base,s))],['a','b','c']);
 s=toggle(s,item('write','effect',['b','d']));assert.deepEqual([...evaluate(selectionQuery(base,s))],['b']);
 s=toggle({...s,mode:'exclude'},item('test','exclude',['b']));assert.equal(evaluate(selectionQuery(base,s)).size,0);
});
test('deselect withdraws its contribution and preserves overlapping membership',()=>{
 let s=toggle(toggle(initialSelection(),item('A','file',['a','b'])),item('B','file',['b','c']));s=toggle(s,item('A','file',['a','b']));
 assert.deepEqual([...evaluate(selectionQuery(base,s))],['b','c']);s=toggle(s,item('B','file',['b','c']));assert.equal(evaluate(selectionQuery(base,s)).size,4);
});
test('explicit intersection does not become OR when operands share a group',()=>{
 let s=toggle(initialSelection(),item('A','file',['a','b']));s=toggle({...s,mode:'intersection'},item('B','file',['b','c']));assert.deepEqual([...evaluate(selectionQuery(base,s))],['b']);
});
test('core batches projections once per binding and commits all view states together',async()=>{
 const h=harness([view('one'),view('two'),view('reference','repository')]);const p=h.core.start();assert.equal(h.calls.length,2);assert.equal(h.calls[0].outputs.length,2);
 h.reply(0,3);await settle();assert.equal(h.core.getState().views.one.status,'loading');h.reply(1,4);await p;
 assert.equal(h.core.getState().views.one.spec.total,3);assert.equal(h.core.getState().views.reference.spec.total,4);h.core.dispose();
});
test('old response cannot overwrite a later selection even when transport ignores cancellation',async()=>{
 const h=harness();const p=h.core.start();h.core.dispatch({type:'toggle',item:item('a')});assert.ok(h.calls[0].signal.aborted);h.reply(1,1);await settle();h.reply(0,99);await p;assert.equal(h.core.getState().views.table.spec.total,1);h.core.dispose();
});
test('mixed pin is rejected; source pin is validated independently',async()=>{
 const h=harness();const p=h.core.start();h.reply(0,1,{revision:'8'});await p;assert.equal(h.core.getState().views.table.status,'error');
 h.storage.source=async()=>({path:'a.java',text:'wrong',revision:'8'});h.core.dispatch({type:'openEvidence',item:item('a')});await settle();assert.equal(h.core.getState().evidence.status,'unavailable');h.core.dispose();
});
test('all data items require a selectable identity and matching pin',()=>{
 const h=harness();assert.throws(()=>h.core.dispatch({type:'toggle',item:{...item('a'),pin:{...pin,revision:'8'}}}),/another/);
 assert.throws(()=>h.core.dispatch({type:'toggle',item:{...item('a'),operand:{sql:'DROP'}}}),/Unknown/);assert.throws(()=>h.core.dispatch({type:'toggle',item:{...item('a'),evidence:[{file:'../secret'}]}}),/path/);h.core.dispose();
});
test('persona changes preserve selection and do not issue another query',()=>{
 const h=harness();h.core.dispatch({type:'toggle',item:item('a')});h.core.dispatch({type:'persona',persona:'auditor'});assert.equal(h.calls.length,1);assert.equal(h.core.getState().selection.contributions.length,1);assert.equal(h.core.getState().persona,'auditor');h.core.dispose();
});
test('selection snapshots cannot be mutated; no selection cap evicts constraints',()=>{
 const h=harness();for(let i=0;i<10;i++)h.core.dispatch({type:'toggle',item:item('a'+i)});
 const s=h.core.getState();assert.equal(s.selection.contributions.length,10);assert.ok(Object.isFrozen(s.selection.contributions));const input=item('outside');h.core.dispatch({type:'toggle',item:input});input.label='changed';assert.equal(h.core.getState().selection.contributions.at(-1).item.label,'outside');h.core.dispose();
});
test('double-click cancels pointer toggle; keyboard selection and source opening are independent',async()=>{
 const h=harness(),i=createAtlasInteraction(h.core,10),a=item('a');i.click(a,1);i.click(a,2);i.doubleClick(a);await new Promise(r=>setTimeout(r,20));assert.equal(h.calls.length,0);assert.equal(h.core.getState().selection.contributions.length,0);assert.equal(h.core.getState().evidence.status,'ready');i.click(a,0);assert.equal(h.core.getState().selection.contributions.length,1);i.dispose();h.core.dispose();
});
test('different rapidly clicked items do not cancel each other',async()=>{
 const h=harness(),i=createAtlasInteraction(h.core,1);i.click(item('a'));i.click(item('b'));await new Promise(r=>setTimeout(r,10));assert.equal(h.core.getState().selection.contributions.length,2);i.dispose();h.core.dispose();
});
test('multiple evidence locations require a chooser and missing source is explicit',async()=>{
 const h=harness(),a={...item('a'),evidence:[{file:'a.java'},{file:'b.java'}]};h.core.dispatch({type:'openEvidence',item:a});assert.equal(h.core.getState().evidence.status,'choose');h.core.dispatch({type:'openEvidence',item:a,index:1});await settle();assert.equal(h.core.getState().evidence.source.path,'b.java');h.core.dispatch({type:'openEvidence',item:{...a,evidence:[]}});assert.equal(h.core.getState().evidence.status,'unavailable');h.core.dispose();
});
test('dispose aborts work, closes storage exactly once and prevents commands',()=>{
 const h=harness();h.core.start();h.core.dispose();h.core.dispose();assert.equal(h.closeCount,1);assert.ok(h.calls[0].signal.aborted);assert.throws(()=>h.core.dispatch({type:'clear'}),/disposed/);
});
test('registry refuses duplicate views and invalid emitted data references',async()=>{
 assert.throws(()=>harness([view(),view()]),/Duplicate/);const bad=view();bad.project=()=>({kind:'table',title:'bad',items:[{key:'a',label:'a',role:'member',ref:{}}],total:1,gaps:[],continuation:null});const h=harness([bad]);const p=h.core.start();h.reply(0);await p;assert.equal(h.core.getState().views.table.status,'error');h.core.dispose();
});
test('cube projections select exact contributor descriptors, never display labels',()=>{
 const v=reviewerViews.find(v=>v.id==='files'),response={matches:10,outputs:[{id:'cube',schema_version:2,status:'ready',data:{items:[{members:{group:'src/A.java'},measures:{count:10},contributors:'signed-cell'}],total:1,continuation:null}}]};
 const spec=v.project(response,{pin,query:base,version:1});assert.deepEqual(spec.items[0].ref.operand,{cell_contributors:{descriptor:'signed-cell'}});assert.deepEqual(spec.items[0].ref.evidence,[{file:'src/A.java'}]);
});
test('cursor history resets when selection changes; old query cursor is never reused',async()=>{
 const v=view();v.outputs=c=>[{id:'rows',output:{entities:{page:{limit:30,continuation:c}}}}];v.project=r=>({kind:'table',title:'table',items:[],total:r.matches,continuation:'cursor-1',gaps:[]});const h=harness([v]);const p=h.core.start();h.reply(0,40);await p;
 h.core.dispatch({type:'page',viewId:'table',direction:'next'});assert.equal(h.calls[1].outputs[0].output.entities.page.continuation,'cursor-1');h.reply(1,40);await settle();h.core.dispatch({type:'toggle',item:item('a')});assert.equal(h.calls[2].outputs[0].output.entities.page.continuation,null);h.core.dispose();
});
test('slow double-click restores its original contribution while preserving another selection',async()=>{
 const h=harness(),i=createAtlasInteraction(h.core,1),a=item('a');i.click(a);await new Promise(r=>setTimeout(r,5));h.core.dispatch({type:'toggle',item:item('b')});i.click(a,2);i.doubleClick(a);await settle();assert.deepEqual(h.core.getState().selection.contributions.map(c=>c.item.id),['b']);i.dispose();h.core.dispose();
});
test('session serialization is revision pinned and restores exact contribution operations',()=>{
 const h=harness();h.core.dispatch({type:'mode',operation:'exclude'});h.core.dispatch({type:'toggle',item:item('a')});const session=h.core.exportSession();h.core.dispatch({type:'clear'});h.core.restoreSession(session);assert.equal(h.core.getState().selection.contributions[0].operation,'exclude');assert.throws(()=>h.core.restoreSession({...session,pin:{...pin,revision:'8'}}),/revalidation/);h.core.dispose();
});
test('file cells and file bars share one canonical selection identity',()=>{
 const files=reviewerViews.find(v=>v.id==='files'),symbols=reviewerViews.find(v=>v.id==='symbols');
 const f=files.project({outputs:[{id:'cube',schema_version:2,status:'ready',data:{items:[{members:{group:'a.java'},measures:{count:1},contributors:'d'}],total:1}}]},{pin});
 const s=symbols.project({outputs:[{id:'rows',schema_version:2,status:'ready',data:{items:[{id:'s1',fields:{name:'run',location:{file:'a.java',start:1,end:20}}}],total:1}}]},{pin});
 assert.equal(f.items[0].ref.id,s.items[0].detailRef.id);
});
