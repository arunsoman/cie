const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path'),{spawnSync}=require('node:child_process'),{Worker}=require('node:worker_threads');
test('captured application and framework boundaries share exact symbol selection across Architect charts',async()=>{
 const root=await fs.mkdtemp(path.join(os.tmpdir(),'atlas-architecture-')),worker=new Worker(path.resolve(__dirname,'../scripts/storage-worker.cjs'));let seq=0,core,storage;const originalFetch=global.fetch;
 const call=request=>new Promise((resolve,reject)=>{const id=++seq;const listener=m=>{if(m.id!==id)return;worker.off('message',listener);m.response.ok?resolve(m.response.result):reject(Object.assign(Error(m.response.error.message),m.response.error));};worker.on('message',listener);worker.postMessage({id,request});});
 try{
  await require('./helpers/atlas-api-fixture.cjs')(root);await fs.mkdir(path.join(root,'other'));await require('./helpers/atlas-api-fixture.cjs')(path.join(root,'other'));
  await fs.writeFile(path.join(root,'other','pom.xml'),(await fs.readFile(path.join(root,'other','pom.xml'),'utf8')).replaceAll('payments','other'));
  await fs.mkdir(path.join(root,'empty'));await fs.writeFile(path.join(root,'empty','pom.xml'),'<project><modelVersion>4.0.0</modelVersion><groupId>qa</groupId><artifactId>empty</artifactId><version>1</version></project>');
  const run=spawnSync(path.resolve(__dirname,'../../target/debug/cie'),['ingest-polyglot',root,'--web-out',path.join(root,'capture.json')],{encoding:'utf8'});assert.equal(run.status,0,run.stderr);
  await require('./helpers/atlas-quality-fixture.cjs')(root);
  const snapshot=JSON.parse(await fs.readFile(path.join(root,'.cie/revs/1/snapshot.json'),'utf8'));
  const captured=snapshot.symbols.find(s=>s.qual_name==='Payments.pay');assert.ok(captured);
  const bundlePath=path.join(root,'.cie/revs/1/web-bundle.json'),bundle=JSON.parse(await fs.readFile(bundlePath,'utf8'));
  bundle.drift={claims:[{id:7,text:'Payment must be saved',loc:{doc:'README.md',line:1}}],mappings:[{claim:7,symbols:[captured.id]}],verifications:[{claim:7,status:'Partial',symbols:[captured.id],reason:'QA recorded verification',checker:'synthetic fixture'}],complete:false};
  await fs.writeFile(bundlePath,JSON.stringify(bundle));
  global.fetch=async(_url,init)=>{try{return new Response(JSON.stringify(await call(JSON.parse(init.body))),{status:200});}catch(e){return new Response(JSON.stringify({error:e.message,code:e.code}),{status:400});}};
  const load=require('./helpers/load-ts.cjs')(),get=n=>load(path.resolve(__dirname,'../src/lib/atlas-core/'+n+'.ts'));storage=get('atlas-storage').atlasStorage(root,'1');
  const base={select:{kind:'symbol'}},raw=await storage.retrieve(base,[{id:'rows',output:{entities:{page:{limit:100}}}}],new AbortController().signal),catalog=get('atlas-catalog').createAtlasCatalog();
  core=get('atlas-core').createAtlasCore({pin:{repository:raw.repository,revision:'1'},base,storage,views:catalog.registry.viewsFor(catalog.plans.resolve('architect').views),registry:catalog.registry,plans:catalog.plans,persona:'architect'});
  const ready=async()=>{const deadline=Date.now()+12000;while(Object.values(core.getState().views).some(v=>v.status==='loading')){assert.ok(Date.now()<deadline);await new Promise(r=>setTimeout(r,5));}for(const [id,v] of Object.entries(core.getState().views))assert.equal(v.status,'ready',id+': '+v.error);};
  await core.start();await ready();assert.equal(core.getState().views.scorecard.spec.items[0].fields.file,'Payments.java');assert.equal(core.getState().views.scorecard.spec.items[0].fields.risk,90);
  assert.equal(core.getState().views.containers.spec.items.length,3);
  const claim=core.getState().views.driftboard.spec.items[0];assert.ok(claim);assert.equal(claim.fields.status,'Partial');assert.equal(claim.ref.evidence.length,2);
  core.dispatch({type:'toggle',item:claim.ref});await ready();assert.deepEqual(core.getState().views.symbols.spec.items.map(i=>i.label),['Payments.pay']);core.dispatch({type:'toggle',item:claim.ref});await ready();

  assert.equal(core.getState().views.depgraph.spec.semantic.kind,'dependency');assert.ok(core.getState().views.depgraph.spec.links.length>0,'captured manifests declare dependencies');
  assert.ok(core.getState().views.depgraph.spec.items.some(i=>i.ref.label==='empty'),'manifest-only zero-edge module stays visible at root');
  const dependency=core.getState().views.depgraph.spec.items.find(i=>i.ref.entityKind==='dependency');assert.ok(dependency);core.dispatch({type:'toggle',item:dependency.ref});await ready();assert.ok(core.getState().views.symbols.spec.items.length>0,'declaration selects owning code without requiring imports');core.dispatch({type:'toggle',item:dependency.ref});await ready();
  const unit=core.getState().views.containers.spec.items.find(i=>i.ref.label==='payments');assert.ok(unit);
  const expected=raw.outputs[0].data.items.filter(e=>e.fields.file.path==='Payments.java').map(e=>e.id).sort();
  core.dispatch({type:'toggle',item:unit.ref});await ready();assert.ok(core.getState().views.scorecard.spec.items.every(i=>i.fields.file==='Payments.java'));assert.deepEqual(core.getState().views.symbols.spec.items.map(i=>i.key).sort(),expected,'longest application directory excludes nested app members');
  assert.equal(core.getState().views.containers.spec.items.filter(i=>i.role==='member').length,1);
  const component=core.getState().views.components.spec.items.find(i=>i.ref.label==='Payments'&&i.role==='member');assert.ok(component,'captured controller has a component with recorded members');
  core.dispatch({type:'toggle',item:component.ref});await ready();assert.ok(core.getState().views.symbols.spec.items.every(i=>i.detail==='Payments.java'));assert.ok(!core.getState().views.symbols.spec.items.some(i=>i.label==='App'));
  core.dispatch({type:'openEvidence',item:component.ref});while(core.getState().evidence.status==='loading')await new Promise(r=>setTimeout(r,5));assert.equal(core.getState().evidence.status,'ready');assert.match(core.getState().evidence.source.text,/class Payments/);
  core.dispatch({type:'toggle',item:component.ref});await ready();assert.deepEqual(core.getState().views.symbols.spec.items.map(i=>i.key).sort(),expected);
  const score=core.getState().views.scorecard.spec.items[0];core.dispatch({type:'toggle',item:score.ref});await ready();assert.ok(core.getState().views.symbols.spec.items.every(i=>i.detail==='Payments.java'));core.dispatch({type:'toggle',item:score.ref});await ready();
  core.dispatch({type:'placeView',area:'hero',viewId:'components'});
  core.dispatch({type:'persona',persona:'reviewer'});await ready();assert.equal(core.getState().workspace.placements.hero,undefined,'Architect placements do not overwrite Reviewer layout');
  core.dispatch({type:'placeView',area:'hero',viewId:'files'});
  core.dispatch({type:'persona',persona:'architect'});await ready();assert.equal(core.getState().selection.contributions.length,1);assert.equal(core.getState().workspace.placements.hero,'components','Architect remembers its own placement');
  const session=core.exportSession();core.restoreSession(session);await ready();assert.equal(core.getState().workspace.layoutPlacements['reviewer.reference'].hero,'files');
  const invalid=JSON.parse(JSON.stringify(session));invalid.workspace.layoutPlacements['architect.default']={invalid:'symbols'};assert.throws(()=>core.restoreSession(invalid),/placement/);

 }finally{core?.dispose();if(!core)await storage?.close();global.fetch=originalFetch;await worker.terminate();await fs.rm(root,{recursive:true,force:true});}
});
