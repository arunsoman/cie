const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path'),{spawnSync}=require('node:child_process'),{Worker}=require('node:worker_threads');
test('captured application and framework boundaries share exact symbol selection across Architect charts',async()=>{
 const root=await fs.mkdtemp(path.join(os.tmpdir(),'atlas-architecture-')),worker=new Worker(path.resolve(__dirname,'../scripts/storage-worker.cjs'));let seq=0,core,storage;const originalFetch=global.fetch;
 const call=request=>new Promise((resolve,reject)=>{const id=++seq;const listener=m=>{if(m.id!==id)return;worker.off('message',listener);m.response.ok?resolve(m.response.result):reject(Object.assign(Error(m.response.error.message),m.response.error));};worker.on('message',listener);worker.postMessage({id,request});});
 try{
  await require('./helpers/atlas-api-fixture.cjs')(root);await fs.mkdir(path.join(root,'other'));await require('./helpers/atlas-api-fixture.cjs')(path.join(root,'other'));
  await fs.writeFile(path.join(root,'other','pom.xml'),(await fs.readFile(path.join(root,'other','pom.xml'),'utf8')).replaceAll('payments','other'));
  const run=spawnSync(path.resolve(__dirname,'../../target/debug/cie'),['ingest-polyglot',root,'--web-out',path.join(root,'capture.json')],{encoding:'utf8'});assert.equal(run.status,0,run.stderr);
  global.fetch=async(_url,init)=>{try{return new Response(JSON.stringify(await call(JSON.parse(init.body))),{status:200});}catch(e){return new Response(JSON.stringify({error:e.message,code:e.code}),{status:400});}};
  const load=require('./helpers/load-ts.cjs')(),get=n=>load(path.resolve(__dirname,'../src/lib/atlas-core/'+n+'.ts'));storage=get('atlas-storage').atlasStorage(root,'1');
  const base={select:{kind:'symbol'}},raw=await storage.retrieve(base,[{id:'rows',output:{entities:{page:{limit:100}}}}],new AbortController().signal),catalog=get('atlas-catalog').createAtlasCatalog();
  core=get('atlas-core').createAtlasCore({pin:{repository:raw.repository,revision:'1'},base,storage,views:catalog.registry.viewsFor(catalog.plans.resolve('architect').views),registry:catalog.registry,plans:catalog.plans,persona:'architect'});
  const ready=async()=>{const deadline=Date.now()+12000;while(Object.values(core.getState().views).some(v=>v.status==='loading')){assert.ok(Date.now()<deadline);await new Promise(r=>setTimeout(r,5));}for(const [id,v] of Object.entries(core.getState().views))assert.equal(v.status,'ready',id+': '+v.error);};
  await core.start();await ready();assert.equal(core.getState().views.containers.spec.items.length,2);
  const unit=core.getState().views.containers.spec.items.find(i=>i.ref.label==='payments');assert.ok(unit);
  const expected=raw.outputs[0].data.items.filter(e=>e.fields.file.path==='Payments.java').map(e=>e.id).sort();
  core.dispatch({type:'toggle',item:unit.ref});await ready();assert.deepEqual(core.getState().views.symbols.spec.items.map(i=>i.key).sort(),expected,'longest application directory excludes nested app members');
  assert.equal(core.getState().views.containers.spec.items.filter(i=>i.role==='member').length,1);
  const component=core.getState().views.components.spec.items.find(i=>i.ref.label==='Payments'&&i.role==='member');assert.ok(component,'captured controller has a component with recorded members');
  core.dispatch({type:'toggle',item:component.ref});await ready();assert.ok(core.getState().views.symbols.spec.items.every(i=>i.detail==='Payments.java'));assert.ok(!core.getState().views.symbols.spec.items.some(i=>i.label==='App'));
  core.dispatch({type:'openEvidence',item:component.ref});while(core.getState().evidence.status==='loading')await new Promise(r=>setTimeout(r,5));assert.equal(core.getState().evidence.status,'ready');assert.match(core.getState().evidence.source.text,/class Payments/);
  core.dispatch({type:'toggle',item:component.ref});await ready();assert.deepEqual(core.getState().views.symbols.spec.items.map(i=>i.key).sort(),expected);
  core.dispatch({type:'placeView',area:'hero',viewId:'components'});
  core.dispatch({type:'persona',persona:'reviewer'});await ready();assert.equal(core.getState().workspace.placements.hero,undefined,'Architect placements do not overwrite Reviewer layout');
  core.dispatch({type:'placeView',area:'hero',viewId:'files'});
  core.dispatch({type:'persona',persona:'architect'});await ready();assert.equal(core.getState().selection.contributions.length,1);assert.equal(core.getState().workspace.placements.hero,'components','Architect remembers its own placement');
  const session=core.exportSession();core.restoreSession(session);await ready();assert.equal(core.getState().workspace.layoutPlacements['reviewer.reference'].hero,'files');
  const invalid=JSON.parse(JSON.stringify(session));invalid.workspace.layoutPlacements['architect.orientation']={invalid:'symbols'};assert.throws(()=>core.restoreSession(invalid),/placement/);

 }finally{core?.dispose();if(!core)await storage?.close();global.fetch=originalFetch;await worker.terminate();await fs.rm(root,{recursive:true,force:true});}
});
