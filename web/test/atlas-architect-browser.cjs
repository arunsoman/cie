/* npm run build first. ATLAS_BROWSER optionally selects a Chromium executable. */
const fs=require('node:fs/promises'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict'),{spawn,spawnSync}=require('node:child_process'),{chromium}=require('playwright');
(async()=>{
 const root=await fs.mkdtemp(path.join(os.tmpdir(),'atlas-browser-')),subject=path.join(root,'subject'),job=path.join(root,'jobs','cafecafe');let browser,server;const errors=[],checks=[];
 const check=(name,pass)=>{assert.ok(pass,name);checks.push({name,pass});console.log("PASS",name);};
 try{
  await fs.mkdir(subject,{recursive:true});await fs.mkdir(job,{recursive:true});
  await fs.writeFile(path.join(subject,'a.py'),'def save(x):\n    return x + 1\n\ndef pay(x):\n    return save(x)\n'+Array.from({length:40},(_,i)=>`\ndef f${i}(x):\n    return pay(x)\n`).join(''));
  await fs.mkdir(path.join(subject,'aux'));await fs.writeFile(path.join(subject,'aux','pyproject.toml'),'[project]\nname="aux"\nversion="0.1.0"\n');
  await fs.writeFile(path.join(subject,'aux','b.py'),'def refund(x):\n    return x - 1\n');
  await require('./helpers/atlas-api-fixture.cjs')(subject);
  const cli=path.resolve(__dirname,'../../target/debug/cie'),run=spawnSync(cli,['ingest-polyglot',subject,'--web-out',path.join(root,'capture.json')],{encoding:'utf8'});assert.equal(run.status,0,run.stderr);
  await require('./helpers/atlas-quality-fixture.cjs')(subject);
  await fs.writeFile(path.join(job,'result.json'),JSON.stringify({revision:'1',label:'Atlas core QA',sourceRoot:null,storageRoot:subject}));
  const port=process.env.ATLAS_PORT??'4402';server=spawn(process.execPath,['scripts/start-standalone.cjs'],{cwd:path.resolve(__dirname,'..'),env:{...process.env,PORT:port,HOSTNAME:'127.0.0.1',CIE_BIN:cli,CIE_JOBS_ROOT:path.join(root,'jobs')},stdio:['ignore','pipe','pipe']});
  let log='';server.stderr.on('data',b=>log+=b);
  await new Promise((resolve,reject)=>{const t=setTimeout(()=>reject(new Error('Startup timeout '+log)),20000);server.stdout.on('data',b=>{if(String(b).includes('Ready')){clearTimeout(t);resolve();}});server.on('exit',code=>{clearTimeout(t);reject(new Error('Server exit '+code+' '+log));});});
  browser=await chromium.launch({headless:true,...(process.env.ATLAS_BROWSER?{executablePath:process.env.ATLAS_BROWSER}:{}),args:['--no-sandbox','--no-zygote','--single-process','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const page=await browser.newPage({viewport:{width:1550,height:2100}});page.on('pageerror',e=>errors.push(String(e)));page.setDefaultTimeout(20000);
  await page.addInitScript(()=>localStorage.setItem('cie.workspace.last',JSON.stringify({kind:'job',jobId:'cafecafe'})));

  await page.goto(`http://127.0.0.1:${port}`);await page.getByRole('button',{name:'Atlas',exact:true}).click();await page.getByRole('button',{name:/Atlas: Onboarding/}).click();await page.getByRole('menuitemradio',{name:/Architect/}).click();
  const app=page.getByTestId('atlas-core-architect');await app.waitFor();
  const score=page.getByRole('region',{name:'Module scorecard',exact:true}),risk=page.getByRole('region',{name:'Hotspot risk treemap',exact:true});
  const ready=async()=>{await page.waitForFunction(()=>[...document.querySelectorAll('[data-testid="atlas-core-architect"] section[aria-busy]')].every(e=>e.getAttribute('aria-busy')==='false')&&document.querySelectorAll('[aria-label="Module scorecard"] tbody tr').length>0);};
  await ready();await page.waitForFunction(()=>document.querySelector('[role="img"][aria-label="Module dependency graph"]')?.getAttribute('data-render-state')==='ready');
  check('Architect defaults to charts, with workspace prose and graph outlines hidden',await page.getByRole('region',{name:'Atlas workspace controls',exact:true}).count()===0&&await page.getByRole('list',{name:'Selectable graph nodes'}).count()===0);
  for(const name of ['Module dependency graph','Bounded context candidates','Module scorecard','Duplication','Documentation drift','Revision timeline','Hotspot risk treemap'])await page.getByRole('region',{name,exact:true}).waitFor();
  check('all seven Architect PRD slots render',true);check('unindexed timeline is explicit',(await page.getByRole('region',{name:'Revision timeline',exact:true}).innerText()).includes('not yet indexed'));
  const out=process.env.ATLAS_QA_OUT??root;await fs.mkdir(out,{recursive:true});await app.evaluate(e=>e.scrollIntoView({block:'start'}));await page.screenshot({path:path.join(out,'architect-default.png'),fullPage:true});
  const rectangle=risk.getByRole('button',{name:/Payments.java/});await rectangle.click();await page.getByRole('button',{name:'Remove Payments.java',exact:true}).waitFor();
  await page.waitForFunction(()=>document.querySelector('[aria-label="Module scorecard"]')?.getAttribute('aria-busy')==='false'&&document.querySelectorAll('[aria-label="Module scorecard"] tbody tr').length===1);await ready();
  check('risk rectangle filters scorecard and keeps a visible removable constraint',await score.getByRole('button',{name:'Payments.java',exact:true}).count()===1);
  await app.evaluate(e=>e.scrollIntoView({block:'start'}));await page.screenshot({path:path.join(out,'architect-file-slice.png'),fullPage:true});
  await rectangle.click();await page.getByRole('button',{name:'Remove Payments.java',exact:true}).waitFor({state:'hidden'});await page.waitForFunction(()=>document.querySelectorAll('[aria-label="Module scorecard"] tbody tr').length===3);await ready();check('second chart click restores all three source files',true);
  await rectangle.dblclick();await page.getByRole('region',{name:'Captured evidence',exact:true}).getByText(/class Payments/).waitFor();check('risk double click opens captured source',true);await page.getByRole('button',{name:'Close evidence',exact:true}).click();
  await page.getByRole('button',{name:'Show controls and details',exact:true}).click();await page.getByRole('region',{name:'Atlas workspace controls',exact:true}).waitFor();check('controls and details are explicitly opt in',true);
  const graph=page.getByRole('region',{name:'Module dependency graph',exact:true});
  await graph.getByRole('list',{name:'Selectable graph nodes',exact:true}).getByRole('button',{name:/payments.*Unclassified/}).click();await page.getByRole('button',{name:'Remove payments',exact:true}).waitFor();await page.waitForFunction(()=>document.querySelectorAll('[aria-label="Module scorecard"] tbody tr').length===2);await ready();check('dependency module selection filters the same file population',true);
  await page.getByRole('button',{name:'Hide controls and details',exact:true}).click();check('hiding details preserves the module constraint',await page.getByRole('button',{name:'Remove payments',exact:true}).isVisible());
  check('no chart retrieval errors',(await app.locator('[role="alert"]').allTextContents()).every(t=>!t.trim()));check('no browser errors',errors.length===0);
  await fs.writeFile(path.join(out,'checks.json'),JSON.stringify({checks,errors},null,2));console.log(JSON.stringify({checks,out},null,2));
 }finally{await browser?.close();server?.kill();if(!process.env.ATLAS_KEEP_FIXTURE)await fs.rm(root,{recursive:true,force:true});}
})().catch(e=>{console.error(e);process.exitCode=1});
