// Run after npm run build: exercise the packaged server and its native worker.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const net = require('node:net');
const { spawn, spawnSync } = require('node:child_process');
(async () => {
  const root=await fs.mkdtemp(path.join(os.tmpdir(),'cie-standalone-'));
  let server;
  try {
    await fs.writeFile(path.join(root,'app.py'),'def smoke(x):\n    return x + 1\n');
    const ingest=spawnSync(path.resolve(__dirname,'../../target/debug/cie'),['ingest-polyglot',root,'--web-out',path.join(root,'bundle.json')],{encoding:'utf8'});
    assert.equal(ingest.status,0,ingest.stderr);
    const reservation=net.createServer();await new Promise(resolve=>reservation.listen(0,'127.0.0.1',resolve));const port=reservation.address().port;await new Promise(resolve=>reservation.close(resolve));
    server=spawn(process.execPath,[path.resolve(__dirname,'../scripts/start-standalone.cjs')],{cwd:path.resolve(__dirname,'..'),env:{...process.env,PORT:String(port),HOSTNAME:'127.0.0.1',NODE_ENV:'production'},stdio:['ignore','pipe','pipe']});
    let logs='';server.stdout.on('data',chunk=>logs+=chunk);server.stderr.on('data',chunk=>logs+=chunk);
    const base=`http://127.0.0.1:${port}`;
    let ready=false;
    for(let i=0;i<100;i++){
      try {const response=await fetch(base,{signal:AbortSignal.timeout(1000)});if(response.ok){ready=true;break;}}catch{}
      if(server.exitCode!=null)break;
      await new Promise(resolve=>setTimeout(resolve,100));
    }
    assert.ok(ready,logs);
    const post=async(url,body)=>{
      const response=await fetch(base+url,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body),signal:AbortSignal.timeout(10000)});
      const data=await response.json();assert.equal(response.status,200,JSON.stringify(data));return data;
    };
    const described=await post('/api/storage',{op:'describe',root,request:{topics:['symbol.fields','outputs.source']}});
    assert.ok(described.context);assert.ok(described.schemas['outputs.source']);
    const facts=await post('/api/storage',{op:'tool_retrieve',request:{context:described.context,select:{kind:'file',where:[{field:'path',op:'eq',value:'app.py'}]},outputs:[{id:'rows',entities:{fields:['path','language'],page:{limit:1}}}]}});
    assert.equal(facts.outputs[0].schema_version,2);assert.equal(facts.outputs[0].data.type,'entities');assert.equal(facts.outputs[0].data.items[0].fields.path,'app.py');
    await fs.unlink(path.join(root,'app.py'));
    const sourceResponse=await post('/api/storage',{op:'retrieve',context:described.context,request:{query:{select:{kind:'file',filter:{field:{field:'path',op:'eq',value:'app.py'}}}},outputs:[{id:'source',output:{detail:{source:{range:'whole_file'}}}}]}});
    assert.ok(sourceResponse.outputs[0].data.text.startsWith('def smoke'));assert.equal(sourceResponse.outputs[0].data.type,'source');
    await post('/api/storage',{op:'close',context:described.context});
    console.log('Packaged production server: HTTP discovery, context-bound typed retrieval, and captured source after deletion passed.');
  } finally {
    if(server && server.exitCode==null){const closed=new Promise(resolve=>server.once('exit',resolve));server.kill('SIGTERM');await closed;}
    await fs.rm(root,{recursive:true,force:true});
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
