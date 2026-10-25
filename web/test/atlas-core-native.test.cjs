const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path'),{spawnSync}=require('node:child_process'),{Worker}=require('node:worker_threads');
test('Atlas core evaluates real storage contributor sets, graph, cursors and captured source',async()=>{
 const root=await fs.mkdtemp(path.join(os.tmpdir(),'atlas-core-native-'));
 const worker=new Worker(path.resolve(__dirname,'../scripts/storage-worker.cjs'));let seq=0;
 const call=request=>new Promise((resolve,reject)=>{const id=++seq;const listener=m=>{if(m.id!==id)return;worker.off('message',listener);m.response.ok?resolve(m.response.result):reject(Object.assign(new Error(m.response.error.message),m.response.error));};worker.on('message',listener);worker.postMessage({id,request});});
 const originalFetch=global.fetch;let core,storage;
 try{
  await fs.writeFile(path.join(root,'a.py'),'def save(x):\n    return x + 1\n\ndef pay(x):\n    return save(x)\n');
  await fs.writeFile(path.join(root,'b.py'),'def refund(x):\n    return x - 1\n');
  const run=spawnSync(path.resolve(__dirname,'../../target/debug/cie'),['ingest-polyglot',root,'--web-out',path.join(root,'capture.json')],{encoding:'utf8'});assert.equal(run.status,0,run.stderr);
  global.fetch=async(_url,init)=>{try{return new Response(JSON.stringify(await call(JSON.parse(init.body))),{status:200});}catch(e){return new Response(JSON.stringify({error:e.message,code:e.code}),{status:400});}};
  const load=require('./helpers/load-ts.cjs')();
  const {atlasStorage}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-storage.ts'));
  const {createAtlasCore}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-core.ts'));
  const {reviewerViews}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-projections.ts'));
  const {hierarchyView,controlFlowView,sequenceView}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-semantic-producers.ts'));
  const {conceptView}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-concepts.ts'));
  const views=[...reviewerViews,hierarchyView,controlFlowView,sequenceView,conceptView];
  storage=atlasStorage(root,'1');const base={select:{kind:'symbol'}};
  const probe=await storage.retrieve(base,[{id:'rows',output:{entities:{page:{limit:1}}}}],new AbortController().signal);
  core=createAtlasCore({pin:{repository:probe.repository,revision:'1'},base,storage,views,persona:'reviewer'});
  const wait=async()=>{const deadline=Date.now()+8000;while(Object.values(core.getState().views).some(v=>v.status==='loading')){if(Date.now()>deadline)throw new Error('Atlas timed out');await new Promise(r=>setTimeout(r,5));}for(const v of Object.values(core.getState().views))assert.equal(v.status,'ready',v.error);};
  await core.start();await wait();
  const conceptRows=await storage.retrieve({select:{kind:'concept'}},[{id:'all',output:{entities:{page:{limit:500}}}}],new AbortController().signal);
  const capturedConcepts=conceptRows.outputs[0].data.items;
  assert.ok(capturedConcepts.length>0,'fixture ingestion generates captured concepts');
  const concept=core.getState().views.concepts.spec.items.find(i=>i.role==='member'&&capturedConcepts.find(e=>e.id===i.key)?.fields.symbols?.length);
  assert.ok(concept,'selectable concept with recorded symbol contributors');
  const rootConcept=capturedConcepts.find(e=>e.id===concept.key),descendants=new Set([rootConcept.fields.id]);
  for(let depth=0;depth<8;depth++){const previous=new Set(descendants);for(const e of capturedConcepts)if(previous.has(e.fields.parent)||(e.fields.extra_edges??[]).some(([p])=>previous.has(p)))descendants.add(e.fields.id);}
  const expectedSymbols=new Set(capturedConcepts.filter(e=>descendants.has(e.fields.id)).flatMap(e=>e.fields.symbols));
  core.dispatch({type:'toggle',item:concept.ref});await wait();
  assert.equal(core.getState().views.symbols.spec.total,expectedSymbols.size,'concept selection matches an independent captured membership oracle');
  assert.ok(core.getState().views.concepts.spec.semantic.nodes.every(n=>n.authority==='Inference'));
  core.dispatch({type:'toggle',item:concept.ref});await wait();
  const initial=core.getState(),files=initial.views.files.spec.items;assert.equal(files.length,2);
  const a=files.find(i=>i.label==='a.py'),b=files.find(i=>i.label==='b.py');assert.ok(a&&b);assert.equal(initial.views.symbols.spec.total,a.value+b.value);
  core.dispatch({type:'toggle',item:a.ref});await wait();assert.equal(core.getState().views.symbols.spec.total,a.value);
  assert.ok(core.getState().views.symbols.spec.items.every(i=>i.detail==='a.py'));
  core.dispatch({type:'toggle',item:concept.ref});await wait();
  assert.ok(core.getState().views.symbols.spec.items.every(i=>i.detail==='a.py'),'file and concept constraints intersect through symbol contributors');
  core.dispatch({type:'toggle',item:concept.ref});await wait();
  await assert.rejects(storage.retrieve({union:[{select:{kind:'symbol'}},{select:{kind:'file'}}]},[{id:'bad',output:{entities:{}}}],new AbortController().signal),/compatible entity kinds/);
  await assert.rejects(storage.retrieve({intersect:[{select:{kind:'symbol'}},{select:{kind:'file'}}]},[{id:'bad',output:{entities:{}}}],new AbortController().signal),/compatible entity kinds/);

  assert.equal(core.getState().views.files.spec.items.length,1,'file chart receives the same selection as every other chart');
  core.dispatch({type:'toggle',item:b.ref});await wait();assert.equal(core.getState().views.symbols.spec.total,a.value+b.value);
  core.dispatch({type:'remove',itemId:a.ref.id});await wait();assert.equal(core.getState().views.symbols.spec.total,b.value);
  core.dispatch({type:'clear'});await wait();
  const save=core.getState().views.symbols.spec.items.find(i=>i.label.endsWith('.save'));assert.ok(save);
  core.dispatch({type:'openEvidence',item:save.ref});const deadline=Date.now()+5000;while(core.getState().evidence.status==='loading'){assert.ok(Date.now()<deadline);await new Promise(r=>setTimeout(r,5));}
  assert.equal(core.getState().evidence.status,'ready',core.getState().evidence.error);assert.match(core.getState().evidence.source.text,/def save/);
  assert.ok(core.getState().views.calls.spec.links.length>0,'real call relation projects to graph');
  assert.equal(core.getState().views.hierarchy.spec.semantic.kind,'hierarchy');
  core.dispatch({type:'toggle',item:save.ref});await wait();
  assert.ok(core.getState().views.controlflow.spec.semantic.blocks.length>0,'real captured IR becomes control flow');
  core.dispatch({type:'clear'});await wait();const pay=core.getState().views.symbols.spec.items.find(i=>i.label.endsWith('.pay'));
  core.dispatch({type:'toggle',item:pay.ref});await wait();
  assert.ok(core.getState().views.sequence.spec.semantic.messages.length>0,'real call records become typed messages');

 }finally{core?.dispose();if(!core)await storage?.close();global.fetch=originalFetch;await worker.terminate();await fs.rm(root,{recursive:true,force:true});}
});
