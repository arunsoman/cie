const test=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const load=require('./helpers/load-ts.cjs')();
const {StorageClient,output}=load(path.resolve(__dirname,'../src/lib/storage-client.ts'));
const entity=id=>({id,kind:'symbol',fields:{qualified_name:id}});
const packet=(data,revision='7')=>({api_version:1,repository:'repo',revision,matches:2,outputs:[{id:'rows',kind:'entities',status:data.truncated?'truncated':'ready',schema_version:2,data}]});
test('client pins one reader and carries opaque cursors across pages',async()=>{
 const original=global.fetch,requests=[];
 global.fetch=async(url,init)=>{assert.equal(url,'/api/storage');const request=JSON.parse(init.body);requests.push(request);
  const value=request.op==='open'?{context:'pinned-seven'}:request.op==='close'?{closed:true}:packet({type:'entities',items:[entity(request.request.outputs[0].output.entities.page.continuation?'two':'one')],total:2,continuation:request.request.outputs[0].output.entities.page.continuation?null:'opaque-cursor',truncated:!request.request.outputs[0].output.entities.page.continuation});
  return new Response(JSON.stringify(value),{status:200});};
 const client=new StorageClient('/repo','7');
 try{const {data}=await client.entities({select:{kind:'symbol'}},1,1);assert.equal(data.items[0].id,'two');assert.equal(requests.filter(r=>r.op==='open').length,1);assert.equal(requests[0].revision,'7');assert.equal(requests[2].request.outputs[0].output.entities.page.continuation,'opaque-cursor');assert.equal(requests[2].context,'pinned-seven');}
 finally{await client.close();global.fetch=original;}
});
test('workspace closure during asynchronous open releases the returned handle',async()=>{
 const original=global.fetch;let resolveOpen;const requests=[];
 global.fetch=async(url,init)=>{const request=JSON.parse(init.body);requests.push(request);if(request.op==='open')return new Promise(resolve=>{resolveOpen=()=>resolve(new Response(JSON.stringify({context:'late-context'})));});return new Response(JSON.stringify({closed:true}));};
 const client=new StorageClient('/repo','7');const opening=client.context();const closing=client.close();resolveOpen();
 try{await assert.rejects(opening,e=>e.code==='CursorExpired');await closing;assert.equal(requests.filter(r=>r.op==='close').length,1);await assert.rejects(()=>client.context(),e=>e.code==='CursorExpired');}
 finally{global.fetch=original;}
});
test('typed output guards refuse incompatible schemas and storage errors',()=>{
 const good=packet({type:'entities',items:[],total:0,continuation:null,truncated:false});assert.deepEqual(output(good,'rows','entities').items,[]);
 assert.throws(()=>output({...good,outputs:[{...good.outputs[0],schema_version:1}]},'rows','entities'),e=>e.code==='SchemaMismatch');
 assert.throws(()=>output({...good,outputs:[{...good.outputs[0],status:'unavailable',data:null,error:{code:'ArtifactUnavailable',message:'Not captured'}}]},'rows'),e=>e.code==='ArtifactUnavailable');
});
