// Named diagrams (PRD v2.0 §25) — real production Next server, the real
// engine-generated demo bundle, no mocks. Proves the client seam end to
// end: the bundle loads with the sample, every artifact the engine emitted
// appears in the picker, the mapped specs render on the gated canvas, and
// a workspace without a bundle gets the honest empty state.
const {spawn}=require('node:child_process'),path=require('node:path'),fs=require('node:fs'),assert=require('node:assert/strict');
const {chromium}=require(process.env.CIE_PLAYWRIGHT||'playwright');
(async()=>{
 const web=path.resolve(__dirname,'..');const port=4323;
 const server=spawn(process.execPath,['node_modules/next/dist/bin/next','start','--hostname','127.0.0.1','--port',String(port)],{cwd:web,stdio:['ignore','pipe','pipe'],env:{...process.env,NEXT_TELEMETRY_DISABLED:'1'}});
 let browser;const errors=[],passed=[];
 const step=async(name,fn)=>{await fn();passed.push(name);console.log('PASS '+name);};
 try{
  await new Promise((resolve,reject)=>{server.stdout.on('data',c=>{if(String(c).includes('Ready'))resolve()});server.once('exit',c=>reject(Error('server exited '+c)));setTimeout(()=>reject(Error('startup timeout')),20000).unref()});
  browser=await chromium.launch({headless:true,args:process.env.CIE_BROWSER_SINGLE_PROCESS==='1'?['--no-sandbox','--no-zygote','--single-process']:[]});
  const page=await browser.newPage({viewport:{width:1536,height:960}});page.setDefaultTimeout(15000);page.on('pageerror',e=>errors.push(e.message));await page.goto(`http://127.0.0.1:${port}`);
  const bundle=JSON.parse(fs.readFileSync(path.join(web,'public/data/polyglot/polyglot.json')));
  // the canvas render is async (negotiate → engine → first layout): wait
  // until the note line settles, whichever way the negotiation lands.
  // (Not /layout/ — the export row's "Layout" select label contains that
  // word from the first paint. The note itself says "Rendered: <engine>"
  // or "… declined: …".)
  const waitSettled=(sel)=>page.waitForFunction((s)=>{const el=document.querySelector(s);return el&&/Rendered:|declined/i.test(el.innerText);},sel,{timeout:30000});

  await step('named diagrams view opens with every emitted artifact in the picker',async()=>{
   await page.getByRole('button',{name:'Explore',exact:true}).click();
   await page.getByRole('menuitem',{name:'Named diagrams — C4, machines, flows',exact:true}).click();
   const picker=page.getByRole('tablist',{name:'Named diagram picker'});
   await picker.waitFor();
   assert.equal(await picker.getByRole('tab').count(),bundle['spec-count'],'one tab per emitted spec');
  });

  await step('default artifact (c4) renders on the gated canvas with its epistemic footer',async()=>{
   const card=page.locator('[data-polyglot-artifact="c4"]');
   await card.waitFor();
   await waitSettled('[data-polyglot-artifact="c4"]');
   const text=await card.innerText();
   assert.match(text,/How sure:/);
   assert.match(text,/layout/,`note line missing "layout" — card text:\n${text}`);
   assert.match(text,/Text outline/);
  });

  await step('workflows artifact switches in and renders',async()=>{
   await page.getByRole('tab',{name:/Workflows — how calls flow/}).click();
   const card=page.locator('[data-polyglot-artifact="workflows"]');
   await card.waitFor();
   await waitSettled('[data-polyglot-artifact="workflows"]');
   assert.match(await card.innerText(),/How sure:/);
  });

  await step('an impact artifact renders with its disclosure when the producer counted gaps',async()=>{
   const impactTab=page.getByRole('tab',{name:/Impact — /}).first();
   await impactTab.click();
   const card=page.locator('[data-polyglot-artifact^="impact:"]').first();
   await card.waitFor();
   await page.waitForFunction(()=>{const el=document.querySelector('[data-polyglot-artifact^="impact:"]');return el&&/Rendered:|declined/i.test(el.innerText);},null,{timeout:30000});
   const text=await card.innerText();
   assert.match(text,/How sure:/);
   assert.match(text,/Impact — /);
  });

  await step('no client exceptions',async()=>assert.deepEqual(errors,[]));

  await step('a workspace without a bundle gets the honest empty state',async()=>{
   await page.getByRole('button',{name:'Use your own',exact:true}).click();
   await page.getByRole('tab',{name:/Upload files/}).click();
   await page.getByRole('button',{name:/More samples/}).click();
   await page.getByRole('menuitem',{name:/Small banking app/}).click();
   await page.getByRole('button',{name:/Sample project: a small banking/}).waitFor();
   await page.getByRole('button',{name:'Explore',exact:true}).click();
   await page.getByRole('menuitem',{name:'Named diagrams — C4, machines, flows',exact:true}).click();
   const empty=page.getByText('No named diagrams for this project');
   await empty.waitFor();
   await page.getByText(/What the engine can.t evidence, it doesn.t draw/).waitFor();
  });

  console.log(JSON.stringify({passed:passed.length,browser:browser.version(),singleProcess:process.env.CIE_BROWSER_SINGLE_PROCESS==='1'}));
 }finally{if(browser)await browser.close();server.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
