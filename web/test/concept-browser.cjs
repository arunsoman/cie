// Usecase-first + a story's diagrams (Phase C) — real production Next
// server, the real engine-generated demo artifacts, no mocks. Proves the
// client seam end to end: the Usecases section reads the engine's own
// usecase_view projection (chain words, member table, class cut), both
// draw buttons open the story's flowchart/sequence on the big gated
// canvas, and the CHAT draws a named story's diagram — while a named
// nothing is refused by name and a bare "draw the flowchart" is NOT
// hijacked away from draw-anything.
const {spawn}=require('node:child_process'),path=require('node:path'),fs=require('node:fs'),assert=require('node:assert/strict');
const {chromium}=require(process.env.CIE_PLAYWRIGHT||'playwright');
(async()=>{
 const web=path.resolve(__dirname,'..');const port=4324;
 const server=spawn(process.execPath,['node_modules/next/dist/bin/next','start','--hostname','127.0.0.1','--port',String(port)],{cwd:web,stdio:['ignore','pipe','pipe'],env:{...process.env,NEXT_TELEMETRY_DISABLED:'1'}});
 let browser;const errors=[],passed=[];
 const step=async(name,fn)=>{await fn();passed.push(name);console.log('PASS '+name);};
 try{
  await new Promise((resolve,reject)=>{server.stdout.on('data',c=>{if(String(c).includes('Ready'))resolve()});server.once('exit',c=>reject(Error('server exited '+c)));setTimeout(()=>reject(Error('startup timeout')),20000).unref()});
  browser=await chromium.launch({headless:true,args:process.env.CIE_BROWSER_SINGLE_PROCESS==='1'?['--no-sandbox','--no-zygote','--single-process']:[]});
  const page=await browser.newPage({viewport:{width:1536,height:960}});page.setDefaultTimeout(15000);page.on('pageerror',e=>errors.push(e.message));await page.goto(`http://127.0.0.1:${port}`);
  const con=JSON.parse(fs.readFileSync(path.join(web,'public/data/polyglot/concepts.json')));
  const usecases=con.nodes.filter(n=>n.kind==='UseCase');
  const story=usecases.find(n=>n.symbols.length>=10)||usecases[0];
  // the canvas render is async: wait until the note settles (Rendered or declined)
  const waitSettled=(sel)=>page.waitForFunction((s)=>{const el=document.querySelector(s);return el&&/Rendered:|declined/i.test(el.innerText);},sel,{timeout:30000});
  const openUsecases=async()=>{
   await page.getByRole('button',{name:'Explore',exact:true}).click();
   await page.getByRole('menuitem',{name:'Usecases — the stories, drawn',exact:true}).click();
   await page.locator('[data-view="usecases"].space-y-4').waitFor();
  };

  await step('the Usecases section opens with the engine\u2019s counted headline',async()=>{
   await openUsecases();
   const view=page.locator('[data-view="usecases"].space-y-4');
   const text=await view.innerText();
   assert.match(text,new RegExp(`${usecases.length} usecases`),'counted headline names every usecase the engine induced');
   assert.match(text,/member function/,'the resolved-member count is stated');
  });

  await step('a chained story shows its broader-story chain and resolved member table',async()=>{
   const view=page.locator('[data-view="usecases"].space-y-4');
   // default pick = the first label-sorted story with a chain — the polyglot
   // demo has 6 of 7 chained, so the words must be there
   assert.match(await view.innerText(),/part of /,'the chain words name the broader stories');
   await view.getByText(/Who does /).waitFor();
   const rows=await view.locator('ul.divide-y li').count();
   assert.ok(rows>0,'the member table has rows');
  });

  await step('"Draw the flowchart" opens the story\u2019s flowchart on the big canvas',async()=>{
   await page.locator('[data-view="usecases"].space-y-4').getByRole('button',{name:'Draw the flowchart'}).click();
   await page.getByRole('heading',{name:'Usecase diagram'}).waitFor();
   await waitSettled('[data-concept-diagram="flowchart"]');
   const text=await page.locator('[data-concept-diagram="flowchart"]').innerText();
   assert.match(text,/How sure:/);
   assert.match(text,/Text outline/);
   assert.match(text,/Flowchart — /);
  });

  await step('"Show the sequence diagram" opens the story\u2019s sequence likewise',async()=>{
   await openUsecases();
   await page.locator('[data-view="usecases"].space-y-4').getByRole('button',{name:'Show the sequence diagram'}).click();
   await page.getByRole('heading',{name:'Usecase diagram'}).waitFor();
   await waitSettled('[data-concept-diagram="sequence"]');
   assert.match(await page.locator('[data-concept-diagram="sequence"]').innerText(),/Sequence — /);
  });

  await step('chat: "draw the flowchart of <story>" answers instantly and opens the story\u2019s own diagram',async()=>{
   await page.getByLabel('Your question').fill(`draw the flowchart of ${story.label}`);
   await page.getByRole('button',{name:'Send question',exact:true}).click();
   await page.getByText(/is on the canvas — its flowchart/).waitFor();
   await page.getByRole('heading',{name:'Usecase diagram'}).waitFor();
   await waitSettled('[data-concept-diagram="flowchart"]');
  });

  await step('chat: a named nothing is refused by name, canvas untouched',async()=>{
   const diagramAnswers=await page.getByText(/is on the canvas — its flowchart/).count(); // step 5's bubble persists in history
   await page.getByLabel('Your question').fill('draw the flowchart of unicorn');
   await page.getByRole('button',{name:'Send question',exact:true}).click();
   await page.getByText(/No usecase named “unicorn”/).waitFor();
   assert.equal(await page.getByText(/is on the canvas — its flowchart/).count(),diagramAnswers,'no NEW story-diagram answer for a named nothing');
   assert.equal(await page.getByRole('heading',{name:'Usecase diagram'}).count(),1,'the canvas did not switch away');
  });

  await step('chat: a bare "draw the flowchart" is NOT hijacked — draw-anything keeps it',async()=>{
   const diagramAnswers=await page.getByText(/its flowchart: \d/).count(); // step 5's bubble persists in history
   await page.getByLabel('Your question').fill('draw the flowchart');
   await page.getByRole('button',{name:'Send question',exact:true}).click();
   await page.getByText(/Here is the call flow/).waitFor(); // draw-anything's whole-project wording
   assert.equal(await page.getByText(/its flowchart: \d/).count(),diagramAnswers,'the concept path did not fire for a bare ask');
  });

  if(errors.length)throw Error('page errors: '+errors.join(' | '));
  console.log(`\n${passed.length} steps PASS`);
 }catch(e){console.error('FAIL',e);process.exitCode=1;}
 finally{try{await browser?.close()}catch{};server.kill('SIGKILL');}
})();
