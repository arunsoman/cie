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
    const symbols=await call({op:'compatibility',context,kind:'symbols',args:{query:'save',limit:30}});assert.equal(symbols.length,1);
    const ir=await call({op:'compatibility',context,kind:'function_ir',args:{symbolId:symbols[0].symbolId}});assert.ok(ir.blocks.length);
    const summary=await call({op:'compatibility',context,kind:'summary',args:{}});assert.ok(summary.ingest.symbols>=3);
    const tables=await call({op:'compatibility',context,kind:'tables',args:{}});assert.ok(tables.some(t=>t.name==='symbols'));
    const duplicates=await call({op:'compatibility',context,kind:'duplicates',args:{}});assert.ok(Array.isArray(duplicates.rows));
    const clusters=await call({op:'compatibility',context,kind:'duplicate_clusters',args:{}});assert.equal(clusters.tierUsed,'canonical');
    const tool=await call({op:'tool_retrieve',context,request:{select:{kind:'symbol',predicates:[{field:{field:'name',op:'eq',value:'save'}}]},outputs:[{id:'rows',output:{entities:{fields:['qualified_name','location'],page:{limit:20}}}}]}});assert.equal(tool.matches,1);
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
