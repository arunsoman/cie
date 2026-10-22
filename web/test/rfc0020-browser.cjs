// Real production Next server and engine-generated sample artifacts; no API mocks.
const {spawn}=require('node:child_process'),path=require('node:path'),fs=require('node:fs'),assert=require('node:assert/strict');
const {chromium}=require(process.env.CIE_PLAYWRIGHT||'playwright');
(async()=>{
 const web=path.resolve(__dirname,'..');const port=4319;
 const server=spawn(process.execPath,['node_modules/next/dist/bin/next','start','--hostname','127.0.0.1','--port',String(port)],{cwd:web,stdio:['ignore','pipe','pipe'],env:{...process.env,NEXT_TELEMETRY_DISABLED:'1'}});
 let browser;const errors=[],passed=[];
 const step=async(name,fn)=>{await fn();passed.push(name);console.log('PASS '+name);};
 try{
  await new Promise((resolve,reject)=>{server.stdout.on('data',c=>{if(String(c).includes('Ready'))resolve()});server.once('exit',c=>reject(Error('server exited '+c)));setTimeout(()=>reject(Error('startup timeout')),20000).unref()});
  browser=await chromium.launch({headless:true,...(process.env.CIE_CHROMIUM?{executablePath:process.env.CIE_CHROMIUM}:{}),args:process.env.CIE_BROWSER_SINGLE_PROCESS==='1'?['--no-sandbox','--no-zygote','--single-process']:[]});
  const page=await browser.newPage({viewport:{width:1536,height:960}});page.setDefaultTimeout(10000);page.on('pageerror',e=>errors.push(e.message));await page.goto(`http://127.0.0.1:${port}`);
  // the demo's optional artifacts (stack.json among them) ride the loadDemo
  // fetch chain — under the system chromium a cold start can outlive the 10s
  // default before the panel can exist; every data-driven wait below allows
  // 30s, matching the repo's other browser smokes
  const edge=JSON.parse(fs.readFileSync(path.join(web,'public/data/polyglot/stack.json'))).crossService[0];
  await step('cross-service declarations and both source citations',async()=>{
   // "24" is prerendered static HTML, so this click can land BEFORE React
   // finishes hydrating the topbar — a lost click leaves the start view up
   // and the region never comes. Verify the atlas actually opened (its
   // breadcrumb root) and re-click when the first click was consumed early.
   const openAtlas=async()=>{
    for(let i=0;i<6;i++){
     await page.getByRole('button',{name:'Atlas',exact:true}).click().catch(()=>{});
     try{ await page.getByRole('button',{name:'System',exact:true}).waitFor({timeout:5000}); return; }catch{}
    }
    throw Error('the atlas view never opened (clicks lost to hydration?)');
   };
   await openAtlas();
   await page.getByRole('region',{name:'Cross-service calls',exact:true}).waitFor({timeout:30000});
   const panel=page.getByRole('region',{name:'Cross-service calls',exact:true});assert.match(await panel.innerText(),/not observed invocations/);
   await panel.getByRole('button',{name:edge.fromEvidence,exact:true}).click();await page.getByRole('dialog').waitFor();assert.match(await page.getByRole('dialog').innerText(),/@FeignClient/);await page.keyboard.press('Escape');
   await panel.getByRole('button',{name:edge.toEvidence,exact:true}).click();await page.getByRole('dialog').waitFor();assert.match(await page.getByRole('dialog').innerText(),/BillingController/);await page.keyboard.press('Escape');
  });
  await step('cross-service panel follows selected container',async()=>{
   // the persona-atlas rework changed how a container is selected: the atlas
   // scopes by SELECTION (atlasSelection), driven from the containers card's
   // text outline — the old `container: <id>` chip buttons are inert badges
   // now. Open the first card's outline and pick the search container there.
   await page.locator('summary').filter({hasText:'Text outline'}).first().click();
   await page.locator('details').filter({hasText:'Text outline'}).first().getByRole('button',{name:'search',exact:true}).click();
   const panel=page.getByRole('region',{name:'Cross-service calls',exact:true});
   assert.match(await panel.innerText(),/No uniquely resolved/,'scoped to search: its only cross-service row is none');
   assert.doesNotMatch(await panel.innerText(),/BillingClient\.java/,'the billing edge stays out of the search scope');
   await page.getByRole('button',{name:'System',exact:true}).click();
   await panel.getByRole('button',{name:edge.fromEvidence,exact:true}).waitFor({timeout:30000});
  });
  await page.getByRole('button',{name:'Use your own',exact:true}).click();await page.getByRole('tab',{name:/Upload files/}).click();await page.getByRole('button',{name:/More samples/}).click();await page.getByRole('menuitem',{name:/Small banking app/}).click();await page.getByRole('button',{name:/Sample project: a small banking/}).waitFor();
  const report=JSON.parse(fs.readFileSync(path.join(web,'public/data/report.json')));
  await step('promises show source-linked static transitions and test bindings',async()=>{await page.getByRole('button',{name:'Explore',exact:true}).click();await page.getByRole('menuitem',{name:'Promises',exact:true}).click();const panel=page.getByRole('region',{name:'Evidenced payment transitions'});await panel.waitFor({timeout:30000});assert.match(await panel.innerText(),/not observed execution/);const evidence=page.getByTestId('obligation-evidence');assert.ok(await evidence.count()>0);assert.match(await evidence.first().innerText(),/not executed coverage/);const c=report.contracts.find(c=>c.verified_by?.length);assert.ok(c,'sample needs a reaching test');const name=report.symbols.find(s=>s.id===c.verified_by[0]).qual_name;await evidence.getByRole('button',{name,exact:true}).first().click();await page.getByRole('dialog').waitFor();assert.match(await page.getByRole('dialog').innerText(),/test/);await page.keyboard.press('Escape');});
  await step('trust distinguishes model simulation from execution',async()=>{await page.getByRole('button',{name:'Trust',exact:true}).click();await page.getByRole('menuitem',{name:'Can I trust this?',exact:true}).click();const panel=page.getByRole('region',{name:'Static-model fault hypotheses'});await panel.waitFor({timeout:30000});assert.match(await panel.innerText(),/Execution performed: false/);assert.match(await panel.innerText(),/Runtime observed: false/);assert.match(await panel.innerText(),/no conclusion about live recovery/);await page.getByRole('region',{name:'Evidenced payment transitions'}).waitFor();});
  await step('no client exceptions',async()=>assert.deepEqual(errors,[]));
  console.log(JSON.stringify({passed:passed.length,browser:browser.version(),singleProcess:process.env.CIE_BROWSER_SINGLE_PROCESS==='1'}));
 }finally{if(browser)await browser.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
