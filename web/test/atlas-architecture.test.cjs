const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const load=require('./helpers/load-ts.cjs')(),get=n=>load(path.resolve(__dirname,'../src/lib/atlas-core/'+n+'.ts'));
const {containerView,componentView}=get('atlas-architecture'),{validateSemantic}=get('atlas-semantic-validation'),{compileSemanticView}=get('atlas-visualization'),{applyEvidencePolicy}=get('atlas-policy');
const pin={repository:'r',revision:'1'},context={pin,query:{select:{kind:'container'}},version:1};
const unit={id:'unit',kind:'container',fields:{name:'Payments',framework:'spring',location:{file:'Payments.java',start:0,end:40}}};
function response(nodes,edges=[],members=nodes,truncated=false){return {...pin,matches:members.length,outputs:[{id:'members',schema_version:2,status:'ready',data:{items:members,truncated:false}},{id:'graph',schema_version:2,status:'ready',data:{nodes,edges,truncated}}]};}
test('C4 containers compile as container elements with explicit level, not relabelled dependency symbols',()=>{
 const spec=containerView.project(response([unit]),context);assert.equal(spec.semantic.kind,'architecture');assert.equal(spec.semantic.level,'container');validateSemantic(spec.semantic);
 const view=compileSemanticView(spec.semantic).view;assert.equal(view.elements[0].elementKind,'container');assert.deepEqual(view.elements[0].fields.architecture,{level:'container',role:'container',technology:'spring'});
 assert.equal(spec.items[0].authority,'Inference');assert.match(spec.gaps.join(' '),/deployment and runtime communication are unverified/);
 assert.equal(applyEvidencePolicy(spec,{factsOnly:true}).semantic.nodes.length,0);
});
test('component containment preserves its captured application context and contributor identity',()=>{
 const component={id:'c',kind:'component',fields:{label:'PaymentsController',kind:'controller',location:{file:'Payments.java',start:0,end:40}}};
 const spec=componentView.project(response([component,unit],[{source:'c',target:'unit',relation:'component_in_container'}],[component]),context);
 assert.equal(spec.semantic.nodes.find(n=>n.visualId==='unit').membership,'context');assert.equal(spec.semantic.nodes.find(n=>n.visualId==='c').boundaryRole,'component');
 assert.equal(spec.semantic.relations[0].relation,'component_in_container');assert.equal(spec.items.find(i=>i.key==='c').ref.entityKind,'component');
 assert.equal(compileSemanticView(spec.semantic).view.elements.find(e=>e.id==='c').elementKind,'component');
});
test('empty and bounded architecture records disclose gaps and never synthesize external systems',()=>{
 const empty=containerView.project(response([]),context);assert.equal(empty.items.length,0);assert.match(empty.gaps.join(' '),/No captured architecture membership/);
 const bounded=containerView.project(response([unit],[],[unit],true),context);assert.match(bounded.gaps.join(' '),/100 nodes/);assert.equal(bounded.semantic.relations.length,0);
 assert.deepEqual(bounded.items[0].ref.evidence,[{file:'Payments.java',startByte:0,endByte:40}]);
});
test('C4 validation rejects level substitution',()=>{
 const spec=containerView.project(response([unit]),context);spec.semantic.nodes[0].boundaryRole='component';assert.throws(()=>validateSemantic(spec.semantic),/C4 node role/);
});
