const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const load=require('./helpers/load-ts.cjs')(),get=n=>load(path.resolve(__dirname,'../src/lib/atlas-core/'+n+'.ts'));
const {routeView,applicationEntryView}=get('atlas-entries'),{assertView}=get('atlas-conformance');
const pin={repository:'r',revision:'1'},context={pin,query:{select:{kind:'route'}},version:1};
const row=(overrides={})=>({id:'route:1',fields:{verb:'POST',path:'/pay',owner:'Payments',method:'pay',rel:'Payments.java',line:7,handler_resolution:'unique',source_symbol_ref:'symbol:pay',handler_location:{file:'Payments.java',start:10,end:20},...overrides}});
const result=items=>({...pin,matches:items.length,outputs:[{id:'rows',schema_version:2,status:'ready',data:{items,total:items.length,continuation:'opaque',truncated:true}}]});
test('API row scopes exactly the captured handler while its file cell scopes the file',()=>{
 const view=routeView.project(result([row()]),context),item=view.items[0];assertView(view,pin);
 assert.equal(item.ref.id,'symbol:pay');assert.equal(item.ref.entityKind,'symbol');assert.deepEqual(item.ref.operand.select.filter.ids,['symbol:pay']);
 assert.deepEqual(item.ref.evidence,[{file:'Payments.java',startByte:10,endByte:20}]);assert.equal(item.cellRefs.file.operand.select.filter.field.field,'file.path');assert.equal(item.cellRefs.file.operand.select.filter.field.value,'Payments.java');
 assert.equal(view.continuation,'opaque');assert.match(view.gaps.join(' '),/next page/);
});
test('ambiguous handlers never become a whole-file API selection',()=>{
 const item=routeView.project(result([row({handler_resolution:'ambiguous',source_symbol_ref:undefined,handler_location:undefined})]),context).items[0];
 assert.equal(item.ref.entityKind,'route');assert.equal(item.ref.operand.select.kind,'route');assert.equal(item.ref.id,'route:1');assert.match(item.fields.resolution,/no API scope/);
});
test('bootstrap class identity is preserved rather than relabelled as a function',()=>{
 const view=applicationEntryView.project(result([{id:'entry:1',fields:{name:'App',kind:'spring-boot',rel:'App.java',handler_resolution:'unique',source_symbol_ref:'class:App',handler_location:{file:'App.java',start:0,end:50}}}]),context);
 assert.equal(view.items[0].ref.id,'class:App');assert.match(view.gaps.join(' '),/class does not imply/);
});
test('cell references must have the same revision and a declared column',()=>{
 const view=routeView.project(result([row()]),context);view.items[0].cellRefs.file.pin={...pin,revision:'2'};assert.throws(()=>assertView(view,pin),/revision|pin/i);
 const valid=routeView.project(result([row()]),context);valid.items[0].cellRefs.missing=valid.items[0].ref;assert.throws(()=>assertView(valid,pin),/declared column/);
});

test('semantic tables retain cell identities and facts-only retains captured file bindings',()=>{
 const {semanticProjection,semanticBindings}=get('atlas-semantic'),{validateSemantic}=get('atlas-semantic-validation'),{applyEvidencePolicy}=get('atlas-policy');
 const view=routeView.project(result([row()]),context),semantic=semanticProjection(view,context,result([row()]));validateSemantic(semantic);
 assert.equal(semantic.rows[0].cellBindings.file.ref.label,'Payments.java');assert.equal(semanticBindings(semantic).length,2);
 const filtered=applyEvidencePolicy({...view,semantic},{factsOnly:true});assert.equal(filtered.semantic.rows[0].cellBindings.file.ref.label,'Payments.java');
});
