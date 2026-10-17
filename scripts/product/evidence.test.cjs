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
 rows.forEach((r,i)=>{assert.equal(r.id,`US-${String(i+1).padStart(3,'0')}`);assert.ok(['complete','partial','pending'].includes(r.status));if(r.status!=='pending')assert.ok(r.evidence.length);if(r.status==='complete')assert.ok(['US-143'].includes(r.id));});
});
test('node:theme-contrast: text AA and essential controls meet contrast in all themes',()=>{
 const css=fs.readFileSync(path.join(__dirname,'../../cie-server/web/style.css'),'utf8');
 const luminance=hex=>{let h=hex.slice(1);if(h.length===3)h=[...h].map(c=>c+c).join('');return h.match(/../g).map(c=>parseInt(c,16)/255).map(v=>v<=.04045?v/12.92:((v+.055)/1.055)**2.4).reduce((sum,v,i)=>sum+v*[.2126,.7152,.0722][i],0);};
 const ratio=(a,b)=>{const [x,y]=[luminance(a),luminance(b)].sort((a,b)=>b-a);return(x+.05)/(y+.05);};
 const blocks=[...css.matchAll(/:root(?:\[data-theme=(?:dark|contrast)\])?\{([^}]+)\}/g)];assert.equal(blocks.length,3);
 for(const block of blocks){const values=Object.fromEntries([...block[1].matchAll(/--([a-z]+):(#[0-9a-f]+)/g)].map(m=>[m[1],m[2]]));for(const [a,b,min] of [['text','panel',4.5],['text','bg',4.5],['muted','panel',4.5],['muted','bg',4.5],['accent','panel',4.5],['warn','warnbg',4.5],['bad','panel',4.5],['line','panel',3],['focus','panel',3]])assert.ok(ratio(values[a],values[b])>=min,`${a}/${b} ${ratio(values[a],values[b])} < ${min}`);}
});
