const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path'),{spawnSync}=require('node:child_process'),{Worker}=require('node:worker_threads');
test('captured APIs select exact handlers, expand nested calls and retain ambiguous records without guessing',async()=>{
 const root=await fs.mkdtemp(path.join(os.tmpdir(),'atlas-entries-native-')),worker=new Worker(path.resolve(__dirname,'../scripts/storage-worker.cjs'));let seq=0,core,storage;const previousFetch=global.fetch;
 const call=request=>new Promise((resolve,reject)=>{const id=++seq;const listener=m=>{if(m.id!==id)return;worker.off('message',listener);m.response.ok?resolve(m.response.result):reject(Object.assign(Error(m.response.error.message),m.response.error));};worker.on('message',listener);worker.postMessage({id,request});});
 try{
  await require('./helpers/atlas-api-fixture.cjs')(root);
  const run=spawnSync(path.resolve(__dirname,'../../target/debug/cie'),['ingest-polyglot',root,'--web-out',path.join(root,'capture.json')],{encoding:'utf8'});assert.equal(run.status,0,run.stderr);
  global.fetch=async(_url,init)=>{try{return new Response(JSON.stringify(await call(JSON.parse(init.body))),{status:200});}catch(e){return new Response(JSON.stringify({error:e.message,code:e.code}),{status:400});}};
  const load=require('./helpers/load-ts.cjs')(),get=n=>load(path.resolve(__dirname,'../src/lib/atlas-core/'+n+'.ts'));
  storage=get('atlas-storage').atlasStorage(root,'1');const base={select:{kind:'symbol'}},signal=new AbortController().signal;
  const raw=await storage.retrieve({select:{kind:'route'}},[{id:'rows',output:{entities:{page:{limit:30}}}}],signal),records=raw.outputs[0].data.items;
  assert.equal(records.length,3);const ambiguous=records.find(r=>r.fields.path==='/ambiguous');assert.equal(ambiguous.fields.handler_resolution,'ambiguous');assert.equal(ambiguous.fields.source_symbol_ref,undefined);
  const catalog=get('atlas-catalog').createAtlasCatalog();
  core=get('atlas-core').createAtlasCore({pin:{repository:raw.repository,revision:'1'},base,storage,views:catalog.registry.viewsFor(catalog.plans.resolve('reviewer').views),registry:catalog.registry,plans:catalog.plans,persona:'reviewer'});
  const ready=async()=>{const deadline=Date.now()+10000;while(Object.values(core.getState().views).some(v=>v.status==='loading')){assert.ok(Date.now()<deadline);await new Promise(r=>setTimeout(r,5));}for(const [id,v] of Object.entries(core.getState().views))assert.equal(v.status,'ready',id+': '+v.error);};
  await core.start();await ready();assert.equal(core.getState().views.routes.spec.items.length,2,'only uniquely linked handlers enter analytical route scope');assert.equal(core.getState().views.entries.spec.items.length,1);
  const boot=core.getState().views.entries.spec.items[0];core.dispatch({type:'toggle',item:boot.ref});await ready();
  assert.equal(core.getState().views.symbols.spec.total,1);assert.equal(core.getState().views.controlflow.spec.semantic.blocks.length,0);assert.match(core.getState().views.controlflow.spec.gaps.join(' '),/not a function/);
  core.dispatch({type:'toggle',item:boot.ref});await ready();
  const route=core.getState().views.routes.spec.items.find(i=>i.label==='POST /pay');assert.ok(route);
  core.dispatch({type:'toggle',item:route.ref});await ready();
  assert.equal(core.getState().views.symbols.spec.total,1);assert.match(core.getState().views.symbols.spec.items[0].label,/Payments.pay$/);assert.equal(core.getState().views.routes.spec.items.length,1);
  assert.ok(core.getState().views.controlflow.spec.semantic.blocks.length>0,'handler IR is shown');
  const calls=core.getState().views.calls.spec.items.map(i=>i.label).join(' ');assert.match(calls,/Payments.save/);assert.match(calls,/Payments.audit/);assert.doesNotMatch(calls,/refund/);
  assert.ok(core.getState().views.sequence.spec.semantic.messages.length>=2,'nested captured static calls become ordered messages');
  core.dispatch({type:'openEvidence',item:route.ref});while(core.getState().evidence.status==='loading')await new Promise(r=>setTimeout(r,5));assert.equal(core.getState().evidence.status,'ready');assert.match(core.getState().evidence.source.text,/int pay/);assert.doesNotMatch(core.getState().evidence.source.text,/int refund/);
  core.dispatch({type:'toggle',item:route.ref});await ready();assert.equal(core.getState().views.routes.spec.items.length,2);
  const refund=core.getState().views.routes.spec.items.find(i=>i.label==='GET /refund');
  core.dispatch({type:'toggle',item:refund.ref});await ready();
  assert.match(core.getState().views.sequence.spec.gaps.join(' '),/withheld by Facts-only/);
  core.dispatch({type:'policy',factsOnly:false});await ready();
  assert.ok(core.getState().views.sequence.spec.semantic.messages.some(m=>m.dispatch==='unknown'),'overridable dispatch remains a boundary when interpretations are enabled');
  core.dispatch({type:'toggle',item:refund.ref});await ready();
  core.dispatch({type:'toggle',item:route.cellRefs.file});await ready();assert.ok(core.getState().views.symbols.spec.total>1,'file cell selects the file rather than its handler');
 }finally{core?.dispose();if(!core)await storage?.close();global.fetch=previousFetch;await worker.terminate();await fs.rm(root,{recursive:true,force:true});}
});
