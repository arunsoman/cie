const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const path = require("node:path");
const os = require("node:os");
const childProcess = require("node:child_process");

test("a local web ingestion launches one coordinator and consumes its artifacts", async () => {
  const bin = path.resolve(__dirname, "../../target/debug/cie");
  try { await fs.access(bin); } catch { assert.fail("Build the Rust CLI before running the ingestion integration test"); }
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "cie-web-stream-"));
  const repo = path.join(temp, "repo");
  await fs.mkdir(repo);
  await fs.writeFile(path.join(repo, "app.py"), "def save(x):\n    return x + 1\n");
  const previous = { bin: process.env.CIE_BIN, jobs: process.env.CIE_JOBS_ROOT };
  process.env.CIE_BIN = bin;
  process.env.CIE_JOBS_ROOT = path.join(temp, "jobs");
  const spawns = [];
  const load = require("./helpers/load-ts.cjs")({
    child_process: { ...childProcess, spawn(command, args, options) {
      spawns.push({ command, args });
      return childProcess.spawn(command, args, options);
    } },
  });
  try {
    const jobs = load(path.resolve(__dirname, "../src/lib/analyze-jobs.ts"));
    const job = await jobs.startJob({ source: "local", localPath: repo, repoName: "repo" });
    const deadline = Date.now() + 30000;
    while (!["done", "failed"].includes(job.state) && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    assert.equal(job.state, "done", JSON.stringify(job.error));
    assert.equal(job.result.revision,"1");
    assert.equal(job.result.report,undefined,"jobs retain references rather than a second copy of captured facts");
    const binding=require(path.resolve(__dirname,'../native/cie_storage_node.node'));
    const dispatch=request=>{const response=JSON.parse(binding.storageDispatch(JSON.stringify(request)));assert.equal(response.ok,true,JSON.stringify(response));return response.result;};
    const {context}=dispatch({op:'open',root:job.result.storageRoot,revision:job.result.revision});
    const captured=dispatch({op:'retrieve',context,request:{query:{select:{kind:'file'}},outputs:[{id:'bootstrap',output:{artifact:{kind:'ui_bootstrap_v1'}}}]}});
    assert.ok(captured.outputs[0].data.document.report.symbols.length>0);
    assert.equal(captured.outputs[0].data.document.polyglotSpecs.kind,'cie-polyglot-viewspec-bundle');
    dispatch({op:'close',context});
    assert.equal(job.result.sourceRoot, repo);
    const engineJobs = spawns.filter(x => ["ingest-polyglot", "ingest", "polyglot", "concepts", "viz", "report", "deps", "cochange", "drift", "overlap"].includes(x.args[0]));
    assert.equal(engineJobs.length, 1, JSON.stringify(engineJobs));
    assert.equal(engineJobs[0].args[0], "ingest-polyglot");
    assert.ok(engineJobs[0].args.includes("--web-out"));
    assert.equal(job.steps.find(s => s.key === "read").state, "done");
    for (const key of ["resolve", "stack", "concepts", "storage", "publish"]) {
      assert.equal(job.steps.find(s => s.key === key).state, "done", key);
    }
    assert.ok(job.steps.every(s => ["done", "skipped"].includes(s.state)), JSON.stringify(job.steps));
    assert.ok(job.steps.find(s => s.key === "storage").ms >= 0);
  } finally {
    if (previous.bin === undefined) delete process.env.CIE_BIN; else process.env.CIE_BIN = previous.bin;
    if (previous.jobs === undefined) delete process.env.CIE_JOBS_ROOT; else process.env.CIE_JOBS_ROOT = previous.jobs;
    await fs.rm(temp, { recursive: true, force: true, maxRetries: 5, retryDelay: 50 });
  }
});

test("deferred drift still retains a URL job and serves an incomplete storage bootstrap", async () => {
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "cie-docs-retain-"));
  const fixture = path.join(temp, "fixture");
  await fs.mkdir(fixture);
  await fs.writeFile(path.join(fixture, "app.py"), "def pay(amount):\n    return amount + 1\n");
  await fs.writeFile(path.join(fixture, "README.md"), "The pay function must validate the amount.\n");
  const previous = { bin:process.env.CIE_BIN, jobs:process.env.CIE_JOBS_ROOT };
  process.env.CIE_BIN = path.resolve(__dirname,"../../target/debug/cie");
  process.env.CIE_JOBS_ROOT = path.join(temp,"jobs");
  const load = require('./helpers/load-ts.cjs')({child_process:{...childProcess,spawn(command,args,options){
    if(command==='git'&&args[0]==='clone') {
      // Substitute only the network clone. Ingestion, timeout stage events,
      // retention and storage retrieval all execute the real implementation.
      return childProcess.spawn(process.execPath,['-e',"require('fs').cpSync(process.argv[1],process.argv[2],{recursive:true})",fixture,args.at(-1)],options);
    }
    // RFC-0026 §10: drift is deferred at ingest unconditionally now, so
    // --docs-budget-ms no longer has any effect; kept passed through to
    // confirm the flag is still accepted, not that it changes anything.
    if(args[0]==='ingest-polyglot')args=[...args,'--docs-budget-ms','0'];
    return childProcess.spawn(command,args,options);
  }}});
  try {
    const jobs=load(path.resolve(__dirname,'../src/lib/analyze-jobs.ts'));
    const job=await jobs.startJob({source:'url',url:'https://example.invalid/demo',repoName:'demo'});
    const deadline=Date.now()+30000;
    while(!['done','failed'].includes(job.state)&&Date.now()<deadline)await new Promise(resolve=>setTimeout(resolve,25));
    assert.equal(job.state,'done',JSON.stringify(job.error));
    assert.equal(job.steps.find(s=>s.key==='drift').state,'skipped');
    assert.match(job.steps.find(s=>s.key==='drift').detail,/not checked/i);
    assert.equal(job.steps.find(s=>s.key==='publish').state,'done');
    assert.equal(job.result.sourceRoot,null);
    assert.equal(job.result.storageRoot,path.join(temp,'jobs',job.id,'storage'));
    const scratch=path.join(temp,'jobs',job.id,'scratch');
    for(let i=0;i<100 && await fs.access(scratch).then(()=>true,()=>false);i++)await new Promise(resolve=>setTimeout(resolve,10));
    await assert.rejects(fs.access(scratch));
    const native=load(path.resolve(__dirname,'../src/lib/storage-native.ts'));
    const response=await native.withRevision(job.result.storageRoot,context=>native.storageCall({op:'retrieve',context,request:{query:{select:{kind:'file'}},outputs:[{id:'bootstrap',output:{artifact:{kind:'ui_bootstrap_v1'}}}]}}),job.result.revision);
    const data=response.outputs[0].data.document;
    assert.ok(data.report.symbols.length>0);
    assert.equal(data.drift.complete,false);
    assert.equal(data.drift.incomplete_reason,'deferred_on_demand');
    const saved=path.join(temp,'jobs',job.id,'result.json');
    for(let i=0;i<100 && !await fs.access(saved).then(()=>true,()=>false);i++)await new Promise(resolve=>setTimeout(resolve,10));
    // The main result route now returns only `report` (RFC-0026 §12 — the
    // page shown right after a load, StartView, needs nothing else); the
    // deferred-drift placeholder is checked via the lazy extra route
    // instead, same as the dashboard's own fetchAndMergeExtra does.
    const route=load(path.resolve(__dirname,'../src/app/api/analyze/result/route.ts'));
    const extraRoute=load(path.resolve(__dirname,'../src/app/api/analyze/result/extra/route.ts'));
    const request={nextUrl:new URL('http://localhost/api/analyze/result?job='+job.id)};
    const extraRequest={nextUrl:new URL('http://localhost/api/analyze/result/extra?job='+job.id)};
    const first=await route.GET(request);
    assert.equal(first.status,200);
    assert.ok((await first.json()).report.symbols.length>0);
    const firstExtra=await extraRoute.GET(extraRequest);
    assert.equal(firstExtra.status,200);
    assert.equal((await firstExtra.json()).drift.complete,false);
    jobs.jobs.delete(job.id);
    const restored=await route.GET(request);
    assert.equal(restored.status,200,'saved storage reference remains usable after job-map loss');
    const restoredExtra=await extraRoute.GET(extraRequest);
    assert.equal(restoredExtra.status,200,'saved storage reference remains usable after job-map loss');
    assert.equal((await restoredExtra.json()).drift.incomplete_reason,'deferred_on_demand');
  } finally {
    if(previous.bin===undefined)delete process.env.CIE_BIN;else process.env.CIE_BIN=previous.bin;
    if(previous.jobs===undefined)delete process.env.CIE_JOBS_ROOT;else process.env.CIE_JOBS_ROOT=previous.jobs;
    if(globalThis.__cieStorageBridge) {
      await globalThis.__cieStorageBridge.worker.terminate();
      delete globalThis.__cieStorageBridge;
    }
    await fs.rm(temp,{recursive:true,force:true,maxRetries:5,retryDelay:50});
  }
});
