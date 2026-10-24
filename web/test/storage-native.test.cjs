const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const { spawnSync } = require('node:child_process');
const { Worker } = require('node:worker_threads');
test('native storage serves revision-pinned UI payloads, model tools and captured source', async () => {
  const root=await fs.mkdtemp(path.join(os.tmpdir(),'cie-native-'));
  const bin=path.resolve(__dirname,'../../target/debug/cie');
  await fs.access(path.resolve(__dirname,'../../target/debug/cie_storage_node.node'));
  await fs.writeFile(path.join(root,'app.py'),'def save(x):\n    return x + 1\n\ndef pay(x):\n    return save(x)\n\ndef test_pay():\n    return pay(1)\n');
  const run=spawnSync(bin,['ingest-polyglot',root,'--web-out',path.join(root,'bundle.json')],{encoding:'utf8'});assert.equal(run.status,0,run.stderr);
  const worker=new Worker(path.resolve(__dirname,'../scripts/storage-worker.cjs'));
  let seq=0;
  const call=request=>new Promise((resolve,reject)=>{const id=++seq;const listener=message=>{if(message.id!==id)return;worker.off('message',listener);message.response.ok?resolve(message.response.result):reject(Object.assign(new Error(message.response.error.message),message.response.error));};worker.on('message',listener);worker.postMessage({id,request});});
  try {
    const {context}=await call({op:'open',root});
    const binding=require(path.resolve(__dirname,'../native/cie_storage_node.node'));
    assert.equal(binding.storageCancel('audit-cancel'),true);
    await assert.rejects(()=>call({op:'retrieve',context,cancellation_id:'audit-cancel',request:{query:{select:{kind:'file'}},outputs:[{id:'rows',output:{entities:{}}}]}}),e=>e.code==='Cancelled');
    const description=await call({op:'describe',context,request:{profile:'full'}});assert.equal(description.tools[1].name,'cie_retrieve');assert.ok(description.tools[1].inputSchema.$defs);
    const searched=await call({op:'retrieve',context,request:{query:{select:{kind:'symbol',filter:{field:{field:'name',op:'eq',value:'save'}}}},outputs:[{id:'symbols',output:{entities:{}}}]}});
    const symbol=searched.outputs[0].data.items[0];
    const ir=await call({op:'retrieve',context,request:{query:{select:{kind:'symbol',filter:{ids:[symbol.id]}}},outputs:[{id:'ir',output:{detail:{function_ir:{}}}}]}});assert.ok(ir.outputs[0].data.blocks.length);
    const inventory=await call({op:'diagnostics',context});assert.ok(inventory.datasets.find(dataset=>dataset.name==='symbols').count>=3);
    const records=await call({op:'diagnostics',context,dataset:'symbols',limit:10});assert.ok(records.columns.includes('qual_name'));
    await assert.rejects(()=>call({op:'compatibility',context,kind:'summary'}),e=>e.code==='InvalidRequest');
    const tool=await call({op:'tool_retrieve',context,request:{select:{kind:'symbol',predicates:[{field:{field:'name',op:'eq',value:'save'}}]},outputs:[{id:'rows',output:{entities:{fields:['qualified_name','location'],page:{limit:20}}}}]}});assert.equal(tool.matches,1);
    assert.equal(tool.outputs[0].data.type,'entities');assert.equal(tool.outputs[0].schema_version,2);
    const literal=await call({op:'tool_retrieve',context,request:{context,select:{kind:'symbol',where:[{field:'name',op:'eq',value:'save'}]},outputs:[{id:'rows',entities:{fields:['qualified_name','location'],page:{limit:1}}}],response:{profile:'compact',max_bytes:12000}}});assert.equal(literal.matches,1);
    await assert.rejects(()=>call({op:'tool_retrieve',context,request:{context:'ctx_wrong',select:{kind:'file'},outputs:[]}}),e=>e.code==='MixedRevisionReference');
    const openedDescription=await call({op:'describe',root,request:{topics:['symbol.fields','outputs.source','views.c4']}});
    assert.ok(openedDescription.context);assert.ok(openedDescription.schemas['outputs.source']);
    const repeatDescription=await call({op:'describe',request:{context:openedDescription.context,topics:['symbol.fields']}});assert.equal(repeatDescription.context,openedDescription.context);
    await call({op:'close',context:openedDescription.context});
    const plan=await call({op:'explain',context,request:{query:{select:{kind:'file',filter:{field:{field:'path',op:'eq',value:'app.py'}}}},outputs:[{id:'rows',output:{entities:{}}}]}});assert.equal(plan.strategy,'indexed_file_lookup');
    // Exercise the actual browser selectors over this same native transport.
    const originalFetch=global.fetch;
    const state={storageRoot:root,sourceRoot:root,report:{rev:1}};
    const store={getState:()=>state,subscribe:()=>()=>{}};
    const load=require('./helpers/load-ts.cjs')({'./cie-store':{useCie:store}});
    const ui=load(path.resolve(__dirname,'../src/lib/storage-ui.ts'));
    global.fetch=async (url,init)=>{
      assert.equal(url,'/api/storage');
      try{return new Response(JSON.stringify(await call(JSON.parse(init.body))),{status:200});}
      catch(error){return new Response(JSON.stringify({code:error.code,error:error.message}),{status:400});}
    };
    try {
      const results=await ui.searchStoredSymbols(root,'save');assert.equal(results.length,1);assert.equal(typeof results[0].id,'string');
      const lowered=await ui.readFunctionIr(root,results[0].id);assert.ok(lowered.blocks.length);assert.equal(lowered.symbol.id,results[0].id);
      const snippet=await ui.readCapturedSource(root,'app.py',results[0].startByte,results[0].endByte);assert.ok(snippet.text.startsWith('def save'));
      const duplicates=await ui.readDuplicates(root,{page:0});assert.ok(Array.isArray(duplicates.rows));
      const families=await ui.readDuplicateClusters(root);assert.ok(Array.isArray(families.clusters));
      const data=await ui.readStorageInventory(root);assert.ok(data.datasets.find(d=>d.name==='symbols').count>=3);
      const rows=await ui.readStorageRows(root,'symbols',0);assert.ok(rows.columns.includes('qual_name'));
      const impact=await ui.readStoredImpact(root,'app.save');assert.ok(impact.affected_symbols.includes('app.pay'));
      await assert.rejects(()=>ui.searchStoredSymbols(root+'-other','save'),e=>e.code==='MixedRevisionReference');
    } finally {await ui.storageFor(root).close();global.fetch=originalFetch;}
    const id=tool.outputs[0].data.items[0].id;
    await fs.unlink(path.join(root,'app.py'));
    const source=await call({op:'retrieve',context,request:{query:{select:{kind:'symbol',filter:{ids:[id]}}},outputs:[{id:'source',output:{detail:{source:{}}}}]}});assert.ok(source.outputs[0].data.text.startsWith('def save'));
    // Symbol resolution stays within the pinned revision; byte offsets are
    // zero-based UTF-8 offsets into the captured source.
    const found=await call({op:'resolve',context,lookup:{by_name:{file:'app.py',qualified_name:'app.save'}}});
    assert.equal(found.resolution,'found');assert.equal(found.id,id);
    const offset=source.outputs[0].data.returned.start;
    const atPosition=await call({op:'resolve',context,lookup:{at_position:{file:'app.py',byte_offset:offset}}});
    assert.ok(['found','ambiguous'].includes(atPosition.resolution),atPosition.resolution);
    await assert.rejects(()=>call({op:'resolve',context,lookup:{by_name:{file:'../escape.py',qualified_name:'app.save'}}}),e=>e.code==='InvalidRequest');
    // Revision listing is bounded and paginated without a context handle.
    const revisions=await call({op:'revisions',root,limit:10});
    assert.equal(revisions.total,1);assert.equal(revisions.items[0].head,true);
    await assert.rejects(()=>call({op:'tool_retrieve',context,request:{query:{select:{kind:'symbol'}},select:{kind:'symbol'},outputs:[]}}),e=>e.code==='InvalidRequest');
    const archive = root + '-retained';
    await call({op:'retain',root,destination:archive});
    try {
      const retained = await call({op:'open',root:archive});
      const facts = await call({op:'tool_retrieve',context:retained.context,request:{select:{kind:'symbol'},outputs:[{id:'rows',output:{entities:{page:{limit:20}}}}]}});
      assert.equal(facts.repository, tool.repository, 'repository identity survives relocation');
      assert.ok(facts.matches >= 3);
      await call({op:'close',context:retained.context});
    } finally { await fs.rm(archive,{recursive:true,force:true}); }
    await call({op:'close',context});await assert.rejects(()=>call({op:'describe',context}),e=>e.code==='CursorExpired');
  } finally {await worker.terminate();await fs.rm(root,{recursive:true,force:true,maxRetries:5});}
});
