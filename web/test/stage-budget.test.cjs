const test=require('node:test');
const assert=require('node:assert/strict');
const path=require('node:path');
const load=require('./helpers/load-ts.cjs')();
const {run}=load(path.resolve(__dirname,'../src/lib/analyze-jobs.ts'));

test('stage transitions leave a fresh budget for optional checks and publication',async()=>{
  const result=await run(process.execPath,['-e',`
    process.stderr.write('read\\n');
    setTimeout(()=>process.stderr.write('drift\\n'),350);
    setTimeout(()=>process.stderr.write('publish\\n'),700);
    setTimeout(()=>process.exit(0),1050);
  `],{timeoutMs:600,onStderr:(_text,renew)=>renew()});
  assert.equal(result.timedOut,false);
  assert.equal(result.code,0);
});

test('a stalled stage is still stopped even when it writes diagnostic chatter',async()=>{
  const result=await run(process.execPath,['-e',`setInterval(()=>process.stderr.write('still busy\\n'),10)`],{timeoutMs:120,onStderr:()=>{}});
  assert.equal(result.timedOut,true);
  assert.notEqual(result.code,0);
});
