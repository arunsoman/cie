const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const Evidence = require('../../cie-server/web/app.js');
const answer = {question:'Q',repositoryId:'r',revisionId:'v',status:'partial',summary:'An inference',gaps:['Unresolved call'],items:[{id:'a',label:'alpha',kind:'function',claimClass:'codeObservation',citations:[{file:'a.ts',startLine:1,endLine:2,revisionId:'v',sourceHash:'hash'}]},{id:'b',label:'beta',kind:'concept',claimClass:'analyzerInference',citations:[]}],edges:[{from:'a',to:'b',kind:'staticCall',claimClass:'codeObservation'}]};
test('node:projection-layout: shared population, filtering, nonoverlap and evidence immutability',()=>{
 const before=JSON.stringify(answer),pop=Evidence.project(answer),layout=Evidence.layout(pop);
 assert.equal(pop.items.length,2);assert.equal(pop.edges.length,1);
 assert.equal(Evidence.project(answer,'alpha').edges.length,0);
 assert.equal(Evidence.project(answer,'',new Set(['b'])).items.length,1);
 assert.ok(layout.nodes[0].y+layout.nodes[0].height<layout.nodes[1].y);
 assert.match(layout.edges[0].path,/^M450 /);assert.equal(JSON.stringify(answer),before);
});
test('node:export-caveats: exact revision, unknowns and inference survive markdown',()=>{
 const text=Evidence.markdown({answer,freshness:'stale',challenged:true});
 for(const s of ['revision: v','freshness: stale','challenged: true','Unresolved call','analyzerInference','source hash']) assert.ok(text.includes(s)||s==='source hash'&&text.includes('hash hash'));
});
test('node:story-register: all 147 IDs, no unsupported completion, named checks',()=>{
 const rows=JSON.parse(fs.readFileSync(path.join(__dirname,'../../cie-server/web/stories.json')));
 assert.equal(rows.length,147);assert.equal(new Set(rows.map(r=>r.id)).size,147);
 rows.forEach((r,i)=>{assert.equal(r.id,`US-${String(i+1).padStart(3,'0')}`);assert.ok(['partial','pending'].includes(r.status));if(r.status==='partial')assert.ok(r.evidence.length);});
});
