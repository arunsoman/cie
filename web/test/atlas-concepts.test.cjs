const {test}=require('node:test'),assert=require('node:assert/strict'),path=require('node:path');
const load=require('./helpers/load-ts.cjs')();
const {conceptView}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-concepts.ts'));
const {applyEvidencePolicy}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-policy.ts'));
const {compileSemanticView}=load(path.resolve(__dirname,'../src/lib/atlas-core/atlas-visualization.ts'));
const pin={repository:'r',revision:'1'},query={select:{kind:'concept'}};
const e=(id,label)=>({id,fields:{label,kind:'Service'}}),edge=(source,target,relation='child_concept_of')=>({source,target,relation});
function project({nodes=[e('child','Payments'),e('parent','Wallet'),e('extra','Security')],edges=[edge('child','parent'),edge('child','extra')],sources=[],sourceEdges=[],truncated=false}={}){
 const data={members:{items:[e('child','Payments')],truncated:false},parents:{nodes,edges,truncated},evidence:{nodes:sources,edges:sourceEdges,truncated:false}};
 return conceptView.project({...pin,matches:1,outputs:Object.entries(data).map(([id,data])=>({id,data,schema_version:2,status:'ready'}))},{pin,query,universe:'concept',version:7});
}
test('concept graph preserves primary and extra parents, labels context and never claims verified concepts',()=>{
 const spec=project();assert.equal(spec.semantic.kind,'dependency');assert.equal(spec.semantic.relations.length,2);assert.equal(spec.items.find(n=>n.key==='parent').role,'context');assert.equal(spec.items.find(n=>n.key==='child').role,'member');
 assert.ok(spec.items.every(n=>n.authority==='Inference'));assert.match(spec.gaps.join(' '),/not verified business facts/);
 const compiled=compileSemanticView(spec.semantic).view;assert.equal(compiled.edges.length,2);
 const filtered=applyEvidencePolicy(spec,{factsOnly:true});assert.equal(filtered.semantic.nodes.length,0);assert.equal(filtered.semantic.relations.length,0);
});
test('concept selection contains recorded descendants without using label or numeric ID guesses',()=>{
 const ref=project().items.find(n=>n.key==='child').ref;assert.equal(ref.entityKind,'concept');assert.equal(ref.operand.union[0].select.filter.ids[0],'child');const follow=ref.operand.union[1].follow;assert.equal(follow.relation,'child_concept_of');assert.equal(follow.incoming,true);assert.equal(follow.max_depth,8);
});
test('representative captured source must have a recorded concept membership edge',()=>{
 const symbol={id:'symbol',fields:{location:{file:'pay.py',start:10,end:25}}};
 const noEdge=project({sources:[symbol]});assert.equal(noEdge.items[0].ref.evidence.length,0);
 const spec=project({sources:[symbol],sourceEdges:[edge('symbol','child','member_of_concept')]});assert.deepEqual(spec.items[0].ref.evidence[0],{file:'pay.py',label:'Representative member of Payments',startByte:10,endByte:25});assert.equal(spec.items[0].authority,'Inference');
});
test('empty and truncated concept data is disclosed, malformed labels are not invented',()=>{
 const empty=project({nodes:[],edges:[]});assert.match(empty.gaps.join(' '),/No generated concepts/);
 const spec=project({nodes:[e('child','')],truncated:true});assert.equal(spec.items.length,0);assert.match(spec.gaps.join(' '),/Unlabelled concept withheld/);assert.match(spec.gaps.join(' '),/limited to 100/);
});
test('low confidence and incomplete coverage stay visible as warnings without inventing confidence',()=>{
 const low=e('child','Payments');low.fields.confidence=0.2;low.fields.coverage_complete=false;
 const spec=project({nodes:[low,e('parent','Wallet')]});
 assert.match(spec.gaps.join(' '),/1 displayed concept records have confidence below 0.5/);
 assert.match(spec.gaps.join(' '),/1 displayed concept records have no numeric confidence/);
 assert.match(spec.gaps.join(' '),/1 displayed concept records report incomplete coverage/);
 assert.equal(spec.items.length,2);
});
test('missing storage outputs fail closed rather than retaining a fabricated concept graph',()=>{
 assert.throws(()=>conceptView.project({...pin,matches:1,outputs:[]},{pin,query,universe:'concept',version:1}),/Concept output unavailable/);
});
