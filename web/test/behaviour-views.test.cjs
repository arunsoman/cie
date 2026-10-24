const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const load=require('./helpers/load-ts.cjs')();
const {atlasPopulation}=load(path.resolve(__dirname,'../src/lib/atlas-scope.ts'));
const {callSiteSequence,cfgChart}=load(path.resolve(__dirname,'../src/lib/behaviour-views.ts'));
function fixture(){
 const files=['src/api/Controller.java','src/service/Service.java','src/api/Unrelated.java'];
 const symbols=[{id:10,qual_name:'Controller.send',name:'send',kind:'Method',loc:{file:0,start:10,end:90}},{id:20,qual_name:'Service.submit',name:'submit',kind:'Method',loc:{file:1,start:5,end:80}},{id:30,qual_name:'Unrelated.status',name:'status',kind:'Method',loc:{file:2,start:0,end:70}}];
 const call=(order)=>({callee:'service.submit',target_symbols:[20],dispatch:'candidate',declaration:'resolved',observation:'unobserved',source_order:order,guard:'unconditional',span:{start:20+order*20,end:35+order*20}});
 const report={rev:7,files,symbols,summaries:[{sym:10,callees:[],call_sites:[call(0),call(1)]},{sym:20,callees:[],call_sites:[]},{sym:30,callees:[],call_sites:[]}]};
 const stack={apps:[{dir:'',name:'app'}],routes:[{rel:files[0],owner:'Controller',method:'send',path:'/send'},{rel:files[2],owner:'Unrelated',method:'status',path:'/status'}],mains:[]};return {report,stack};
}
test('file population excludes unrelated table/graph symbols without renumbering locations',()=>{
 const {report,stack}=fixture(),p=atlasPopulation(report,null,stack,{primary:{kind:'module',id:report.files[0],label:'Controller.java'},scope:'module'});
 assert.deepEqual(p.report.symbols.map(s=>s.id),[10]);assert.deepEqual(p.stack.routes.map(r=>r.path),['/send']);assert.equal(p.report.files[1],report.files[1]);assert.equal(report.symbols.length,3);
});
test('API population includes declared targets and excludes unrelated controllers',()=>{
 const {report,stack}=fixture(),p=atlasPopulation(report,null,stack,{primary:{kind:'symbol',id:'Controller.send',label:'send',loc:{file:report.files[0]}},scope:'symbol'});
 assert.deepEqual([...p.symbolIds],[10,20]);assert.deepEqual(p.stack.routes.map(r=>r.path),['/send']);assert.equal(p.truncated,false);
});
test('repeated occurrences survive; candidate navigation targets definition but remains inferred',()=>{
 const {report}=fixture(),calls=callSiteSequence(report,10).msgs.filter(m=>m.symbolId===20);
 assert.equal(calls.length,2);assert.ok(calls.every(m=>m.status==='inferred'));assert.ok(calls.every(m=>m.loc.file===report.files[1]&&m.loc.byteStart===5));assert.notDeepEqual(calls[0].callSiteLoc,calls[1].callSiteLoc);
});
test('path-local recursion guard preserves repeated calls and discloses budget frontiers',()=>{
 const {report}=fixture();report.summaries[1].call_sites=[{callee:'self.submit',target_symbols:[20],dispatch:'exact',source_order:0,guard:'if retry',span:{start:45,end:60}}];
 const spec=callSiteSequence(report,10);assert.equal(spec.msgs.filter(m=>m.label.includes('[recursion]')).length,2);assert.ok(spec.msgs.some(m=>m.label.includes('if retry')));assert.ok(callSiteSequence(report,10,2).tell.includes('omitted'));
});
test('ambiguous unanchored qualified names never select a different declaration',()=>{
 const {report,stack}=fixture();report.symbols.push({...report.symbols[0],id:40,loc:{file:2,start:1,end:20}});assert.equal(atlasPopulation(report,null,stack,{primary:{kind:'symbol',id:'Controller.send',label:'send'},scope:'symbol'}).symbolIds.size,0);
});
test('CFG preserves actual predicates, branches, returns and excludes unreachable blocks',()=>{
 const code='if (x > 0) return 1;',model={symbol:{qualName:'Example.run',path:'Example.java',startByte:0,endByte:code.length},blocks:[{id:1,blockIndex:0,kind:'entry',instructions:[{instIndex:0,op:'Cmp',spanStart:3,spanEnd:10}]},{id:2,blockIndex:1,kind:'normal',instructions:[{instIndex:0,op:'Ret',spanStart:11,spanEnd:20}]},{id:3,blockIndex:2,kind:'exit',instructions:[]},{id:4,blockIndex:3,kind:'normal',instructions:[]}],edges:[{fromBlockIndex:0,toBlockIndex:1,kind:'branch_true'},{fromBlockIndex:0,toBlockIndex:2,kind:'branch_false'},{fromBlockIndex:1,toBlockIndex:2,kind:'jump'}]};
 const spec=cfgChart(model,{text:code,returned:{start:0,end:code.length}});assert.ok(spec.nodes.some(n=>n.kind==='decision'&&n.title==='(x > 0)'));assert.ok(spec.links.some(e=>e.label==='Yes'));assert.ok(spec.links.some(e=>e.label==='No'));assert.ok(!spec.nodes.some(n=>n.id==='cfg:3'));assert.ok(spec.nodes.some(n=>n.title.startsWith('return')));
});

test('selected API clips the file universe and removes unrelated rolled-up module labels',()=>{
 const {report,stack}=fixture();
 const concepts={nodes:[{id:1,kind:'Domain',label:'project',parent:null,children:[2,3],symbols:[10,20,30],behaviors:[]},{id:2,kind:'Module',label:report.files[0],parent:1,children:[],symbols:[10],behaviors:[]},{id:3,kind:'Module',label:report.files[2],parent:1,children:[],symbols:[10,30],behaviors:[]}]};
 const p=atlasPopulation(report,concepts,stack,{primary:{kind:'symbol',id:'Controller.send',label:'send',loc:{file:report.files[0]}},scope:'symbol'});
 assert.deepEqual(p.report.viewScope.files,[report.files[0],report.files[1]]);
 assert.deepEqual(p.concepts.nodes.map(n=>n.id),[1,2]);
 assert.deepEqual(p.concepts.nodes[0].children,[2]);
});
