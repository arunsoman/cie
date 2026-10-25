/* npm run build first. ATLAS_BROWSER optionally selects a Chromium executable. */
const fs=require('node:fs/promises'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict'),{spawn,spawnSync}=require('node:child_process'),{chromium}=require('playwright');
(async()=>{
 const root=await fs.mkdtemp(path.join(os.tmpdir(),'atlas-browser-')),subject=path.join(root,'subject'),job=path.join(root,'jobs','cafecafe');let browser,server;const errors=[],checks=[];
 const check=(name,pass)=>{assert.ok(pass,name);checks.push({name,pass});};
 try{
  await fs.mkdir(subject,{recursive:true});await fs.mkdir(job,{recursive:true});
  await fs.writeFile(path.join(subject,'a.py'),'def save(x):\n    return x + 1\n\ndef pay(x):\n    return save(x)\n'+Array.from({length:40},(_,i)=>`\ndef f${i}(x):\n    return pay(x)\n`).join(''));
  await fs.writeFile(path.join(subject,'b.py'),'def refund(x):\n    return x - 1\n');
  const cli=path.resolve(__dirname,'../../target/debug/cie'),run=spawnSync(cli,['ingest-polyglot',subject,'--web-out',path.join(root,'capture.json')],{encoding:'utf8'});assert.equal(run.status,0,run.stderr);
  await fs.writeFile(path.join(job,'result.json'),JSON.stringify({revision:'1',label:'Atlas core QA',sourceRoot:null,storageRoot:subject}));
  const port=process.env.ATLAS_PORT??'4402';server=spawn(process.execPath,['scripts/start-standalone.cjs'],{cwd:path.resolve(__dirname,'..'),env:{...process.env,PORT:port,HOSTNAME:'127.0.0.1',CIE_BIN:cli,CIE_JOBS_ROOT:path.join(root,'jobs')},stdio:['ignore','pipe','pipe']});
  let log='';server.stderr.on('data',b=>log+=b);
  await new Promise((resolve,reject)=>{const t=setTimeout(()=>reject(new Error('Startup timeout '+log)),20000);server.stdout.on('data',b=>{if(String(b).includes('Ready')){clearTimeout(t);resolve();}});server.on('exit',code=>{clearTimeout(t);reject(new Error('Server exit '+code+' '+log));});});
  browser=await chromium.launch({headless:true,...(process.env.ATLAS_BROWSER?{executablePath:process.env.ATLAS_BROWSER}:{}),args:['--no-sandbox','--no-zygote','--single-process','--disable-dev-shm-usage','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const page=await browser.newPage({viewport:{width:1550,height:1000}});page.on('pageerror',e=>errors.push(String(e)));page.setDefaultTimeout(20000);
  await page.addInitScript(()=>localStorage.setItem('cie.workspace.last',JSON.stringify({kind:'job',jobId:'cafecafe'})));
  await page.goto(`http://127.0.0.1:${port}`);await page.getByRole('button',{name:'Atlas',exact:true}).click();await page.getByRole('button',{name:/Atlas: Onboarding/}).click();await page.getByRole('menuitemradio',{name:/Reviewer/}).click();await page.getByTestId('atlas-core-reviewer').waitFor();
  const files=page.getByRole('region',{name:'File contributors',exact:true}),symbols=page.getByRole('region',{name:'Selected symbols',exact:true}),bar=files.getByRole('button',{name:/a.py ·/});await bar.waitFor();
  await bar.click();await page.waitForFunction(()=>document.querySelector('[aria-label="Atlas selection"]')?.textContent.includes('a.py')&&document.querySelector('[aria-label="Selected symbols"]')?.getAttribute('aria-busy')==='false');check('file selects shared contributors',await bar.getAttribute('aria-pressed')==='true');
  const firstRows=await symbols.locator('tbody').innerText();check('other file excluded',!firstRows.includes('b.py'));
  await symbols.getByRole('button',{name:'Next symbols',exact:true}).click();await page.waitForFunction(()=>document.querySelector('[aria-label="Selected symbols"]')?.getAttribute('aria-busy')==='false');check('cursor advances',(await symbols.locator('tbody').innerText())!==firstRows);
  await bar.click();await page.waitForFunction(()=>!document.querySelector('[aria-label="Atlas selection"]')?.textContent.includes('a.py'));check('second click deselects',await bar.getAttribute('aria-pressed')==='false');
  await bar.dblclick();await page.getByRole('region',{name:'Captured evidence',exact:true}).getByText(/def save/).waitFor();check('double click opens source without selecting',await bar.getAttribute('aria-pressed')==='false');
  await page.getByRole('button',{name:'Close evidence',exact:true}).click();await bar.click();await page.waitForFunction(()=>document.querySelector('[aria-label="Atlas selection"]')?.textContent.includes('a.py'));
  await page.getByRole('button',{name:'Atlas: Reviewer',exact:true}).click();await page.getByRole('menuitemradio',{name:/Architect/}).click();await page.getByRole('button',{name:/Atlas: Architect/}).click();await page.getByRole('menuitemradio',{name:/Reviewer/}).click();await page.getByTestId('atlas-core-reviewer').waitFor();check('persona round trip preserves selection',await page.getByRole('button',{name:'Remove a.py',exact:true}).isVisible());
  await symbols.getByRole('button',{name:'a.pay',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('[aria-label="Selected symbols"]')?.textContent.includes('1 matching symbols')&&document.querySelector('[aria-label="Selected symbols"]')?.getAttribute('aria-busy')==='false');
  check('function refines every chart to the same selected subject',await files.getByRole('button',{name:/a.py · 1$/}).count()===1);
  check('captured control-flow model is rendered',await page.getByRole('region',{name:'Control flow — a.pay',exact:true}).count()===1);
  check('typed sequence is rendered',await page.getByRole('region',{name:'Static sequence — a.pay',exact:true}).count()===1);
  await page.getByLabel('Chart in hero',{exact:true}).selectOption('files');
  await page.waitForFunction(()=>document.querySelectorAll('[aria-label="File contributors"]').length===2);
  check('chart can be placed in a different slot and duplicated',await page.getByRole('region',{name:'File contributors',exact:true}).count()===2);
  await page.getByLabel('Chart in hero',{exact:true}).selectOption('symbols');
  check('moving charts preserves active function constraint',await page.getByRole('button',{name:'Remove a.pay',exact:true}).isVisible());
  await symbols.getByRole('button',{name:'a.pay',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('[aria-label="Selected symbols"]')?.textContent.includes('43 matching symbols')&&document.querySelector('[aria-label="Selected symbols"]')?.getAttribute('aria-busy')==='false');
  check('function toggle restores file selection everywhere',await files.getByRole('button',{name:/a.py · 43$/}).count()===1);
  const out=process.env.ATLAS_QA_OUT??root;await fs.mkdir(out,{recursive:true});await page.screenshot({path:path.join(out,'atlas-core-reviewer.png'),fullPage:true});check('no browser errors',errors.length===0);await fs.writeFile(path.join(out,'checks.json'),JSON.stringify({checks,errors},null,2));console.log(JSON.stringify({checks,output:out},null,2));
 }finally{await browser?.close();server?.kill();if(!process.env.ATLAS_KEEP_FIXTURE)await fs.rm(root,{recursive:true,force:true});}
})().catch(e=>{console.error(e);process.exitCode=1});
