// Real browser acceptance against the compiled local Rust server; no API mocks.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const os=require('node:os');
const path=require('node:path');
const {spawn}=require('node:child_process');
(async()=>{
 const dir=await fs.mkdtemp(path.join(os.tmpdir(),'cie-browser-'));
 const source=path.join(dir,'source');await fs.mkdir(source);
 await fs.writeFile(path.join(source,'auth.py'),'def allowed(value):\n    return value\n\ndef authorize(value):\n    return allowed(value)\n\ndef test_authorize():\n    assert authorize(True)\n');
 const binary=process.env.CIE_BINARY||path.resolve('target/debug/cie-workspace');
 const server=spawn(binary,['--allow-root',source,'--data',path.join(dir,'data'),'--port','0'],{stdio:['ignore','pipe','pipe']});
 let browser;const results=[];const errors=[];let stderr='';server.stderr.on('data',b=>stderr+=b);
 async function step(name,action){console.log(`RUN ${name}`);await action();results.push({name,status:'passed'});console.log(`PASS ${name}`);}
 try{
  const url=await new Promise((resolve,reject)=>{const timer=setTimeout(()=>reject(new Error('Server startup timeout: '+stderr)),15000);server.once('exit',code=>{clearTimeout(timer);reject(new Error(`Server exited ${code}: ${stderr}`));});server.stdout.on('data',b=>{const match=String(b).match(/http:\/\/127\.0\.0\.1:\d+/);if(match){clearTimeout(timer);resolve(match[0]);}});});
  browser=await chromium.launch({headless:true,...(process.env.CIE_CHROMIUM?{executablePath:process.env.CIE_CHROMIUM}:{}),args:['--no-sandbox', '--no-process-singleton', '--no-zygote', '--single-process']});
  const context=await browser.newContext({acceptDownloads:true});const page=await context.newPage();page.setDefaultTimeout(10000);page.on('pageerror',e=>errors.push(e.message));
  const click=async text=>page.getByRole('button',{name:text,exact:true}).click();
  await page.goto(url);await page.getByText('Local workspace ready. No cloud model is connected.',{exact:true}).waitFor();
  await step('browser:privacy-disclosure',async()=>{assert.match(await page.locator('header').innerText(),/no cloud/);await page.getByText('Privacy and limits',{exact:true}).click();assert.match(await page.locator('aside').innerText(),/No source changes are applied/);});
  await step('browser:repository-index-reload',async()=>{await page.locator('#repo-path').fill(source);await page.locator('#repo-name').fill('Browser fixture');await click('Connect repository');await click('Index snapshot');await page.locator('#jobs').getByText(/published/).waitFor({timeout:45000});await page.reload();await page.locator('#repositories').getByText(/1 snapshots/).waitFor();assert.match(await page.locator('#jobs').innerText(),/ready/);});
  await page.locator('#question').fill('overview');await click('Ask from indexed evidence');await page.locator('#answer h2').getByText('overview',{exact:true}).waitFor();
  await step('browser:representation-parity',async()=>{await click('Table');const names=await page.locator('#population tbody tr td:first-child button').allTextContents();assert.ok(names.length>=3);await click('Outline');assert.deepEqual(await page.locator('#population ol > li > button').allTextContents(),names);await click('Graph');assert.equal(await page.locator('#population svg g').count(),names.length);assert.match(await page.locator('#answer').innerText(),/codeObservation/);});
  await step('browser:keyboard-graph',async()=>{const graph=page.locator('#population svg');await graph.focus();await page.keyboard.press('End');await page.keyboard.press('Enter');assert.equal(await page.locator('#inspector').evaluate(e=>e.open),true);await page.keyboard.press('Escape');assert.equal(await page.locator('#inspector').evaluate(e=>e.open),false);});
  await step('browser:source-inspection',async()=>{await click('Table');await page.locator('#population tbody tr td:first-child button').first().click();await page.locator('#inspector pre').first().waitFor();assert.match(await page.locator('#inspector-body').innerText(),/auth.py:/);assert.match(await page.locator('#inspector-body pre').first().innerText(),/def /);await click('Close inspector');});
  await step('browser:selection-follow-up',async()=>{assert.match(await page.locator('#context').innerText(),/Selected:/);await page.locator('#intent').selectOption('callers');await page.locator('#question').fill('show callers');await click('Ask from indexed evidence');await page.locator('#answer h2').getByText('show callers',{exact:true}).waitFor();assert.match(await page.locator('#answer').innerText(),/Interpretation:/);});
  // Open the original overview again to review a known item.
  await page.locator('#answer-tabs button').filter({hasText:'overview'}).first().click();
  await step('browser:review-refutation',async()=>{await click('Table');const count=await page.locator('#population tbody tr').count();await page.locator('#population tbody tr td:first-child button').first().click();await page.locator('#review-reason').fill('This interpretation needs further evidence.');await page.locator('#review-verdict').selectOption('refute');await click('Save review');await page.getByText('1 refuted claims withheld; historical evidence retained.',{exact:true}).waitFor();assert.equal(await page.locator('#population tbody tr').count(),count-1);assert.match(await page.locator('#answer').innerText(),/challenged/);});
  await step('browser:export',async()=>{const promise=page.waitForEvent('download');await click('Export JSON');const download=await promise;const stream=await download.createReadStream();let text='';for await(const c of stream)text+=c;const result=JSON.parse(text);assert.equal(result.hiddenClaims,1);assert.equal(result.challenged,true);assert.ok(result.answer.revisionId);assert.ok(Array.isArray(result.answer.gaps));});
  await step('browser:workspace-reload',async()=>{await page.locator('#workspace-name').fill('Saved browser investigation');await click('Save workspace');await page.locator('#workspaces').getByRole('button',{name:/Saved browser investigation/}).waitFor();await page.reload();await page.locator('#workspaces').getByRole('button',{name:/Saved browser investigation/}).click();await page.locator('#answer h2').waitFor();assert.match(await page.locator('#answer').innerText(),/challenged/);});
  await step('browser:theme-persistence',async()=>{await page.locator('#theme').selectOption('contrast');await page.reload();assert.equal(await page.locator('html').getAttribute('data-theme'),'contrast');assert.equal(await page.locator('#theme').inputValue(),'contrast');});
  await step('browser:gallery-unavailable',async()=>{await click('View gallery');assert.match(await page.locator('#gallery').innerText(),/Unavailable: current snapshots have no imported latency/);});
  await step('browser:feature-planning',async()=>{await click('Feature planning');await page.locator('#feature-prompt').fill('Add authorization outcome audit');await page.locator('#feature-criteria').fill('Rejected authorization creates a structured audit record');await click('Save planning draft');await page.locator('#feature-drafts').getByText('Add authorization outcome audit',{exact:true}).waitFor();assert.match(await page.locator('#feature-drafts').innerText(),/No code was generated/);});
  await step('browser:story-register',async()=>{await click('Story status');assert.equal(await page.locator('#stories .story').count(),147);await page.locator('#story-filter').fill('US-147');assert.equal(await page.locator('#stories .story').count(),1);});
  await step('browser:staleness',async()=>{await fs.appendFile(path.join(source,'auth.py'),'\ndef changed():\n    return False\n');await click('Investigate');await click('Recheck freshness');await page.getByText(/freshness: stale/).waitFor();});
  await step('browser:no-client-errors',async()=>assert.deepEqual(errors,[]));
  console.log(JSON.stringify({passed:results.length,results},null,2));
 }finally{if(browser)await browser.close();server.kill('SIGTERM');await fs.rm(dir,{recursive:true,force:true});}
})().catch(e=>{console.error(e);process.exitCode=1;});
