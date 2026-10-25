const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path'),load=require('./helpers/load-ts.cjs')();
const {scorecardView}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-scorecard.ts'));
const pin={repository:'r',revision:'1'},context={pin,query:{select:{kind:'file'}},version:1};
const row=(name,q={})=>({id:name,fields:{path:name,quality:{available:true,...q}}});
const response=items=>({...pin,matches:items.length,outputs:[{id:'rows',schema_version:2,status:'ready',data:{items,total:items.length,continuation:null,truncated:false}}]});
test('scorecard preserves measured zero and states unknown instead of making up numbers',()=>{
 const view=scorecardView.project(response([row('zero.py',{coverage_percent:0,avg_cc:0,max_cc:0,duplicate_pairs:0,hotspot_risk:0,security_findings:0}),row('missing.py',{available:false})]),context);
 assert.equal(view.items[0].fields.coverage,0);assert.equal(view.items[0].fields.risk,0);assert.equal(view.items[0].fields.complexity,'0 / 0');assert.equal(view.items[1].fields.coverage,'not run');assert.equal(view.items[1].fields.risk,'not collected');assert.equal(view.items[1].fields.cohesion,'not yet measured');assert.match(view.gaps.join(' '),/No quality artifact/);
});
test('every scorecard cell selects the same file contributors and source',()=>{
 const item=scorecardView.project(response([row('pay.py')]),context).items[0];assert.equal(item.ref.entityKind,'file');assert.deepEqual(item.ref.operand,{select:{kind:'file',filter:{ids:['pay.py']}}});assert.deepEqual(item.ref.evidence,[{file:'pay.py'}]);
 const outputs=scorecardView.outputs('opaque');assert.equal(outputs[0].output.entities.page.continuation,'opaque');assert.equal(outputs[0].output.entities.order[0].field,'quality.hotspot_risk');assert.equal(scorecardView.filter.field.value,false);
});
