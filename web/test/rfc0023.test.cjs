const test = require('node:test'), assert = require('node:assert/strict'), path = require('node:path');
const loader = require('./helpers/load-ts.cjs');
const src = name => path.resolve(__dirname, '../src', name);
const spec = () => ({elements:[{id:'a',label:'A',elementKind:'process',displayMode:'Fact',evidence:{file:'a.py',start:0,end:10},fields:{},salience:{factors:['STORE_RECORD'],weight:1,whyShown:'test'}}],edges:[],footer:{facts:1,inferences:0,hypotheses:0,fog:0,stale:0,caption:'test'},hidden:[],gaps:[],scope:{revision:1,environments:[],coverage:{analyzed:true}},semanticHash:'',renderHash:'',layoutHint:{strategy:'dagre'}});
test('G6 host supplies adapted nodes to canvas and suppresses selection feedback', async () => {
  let cfg, graph;
  class Graph {
    constructor(options){ cfg=options; graph=this; this.events={}; }
    async render(){} on(name,cb){this.events[name]=cb} off(){} destroy(){}
  }
  const load=loader({'@antv/g6':{Graph},'@antv/hierarchy':{compactBox(){throw Error('not a tree')}}});
  const {renderViaNegotiator}=load(src('lib/viz/g6-engine.ts'));
  const result=await renderViaNegotiator({},spec());
  assert.equal(cfg.data.nodes[0].id,'a');
  let calls=0; result.handle.onSelection(ids=>{calls++;result.handle.setSelected(ids,'programmatic')});
  graph.events['element:click']({target:{id:'a'}});
  assert.equal(calls,1); assert.equal(result.handle.layout.actual,'dagre');
  await result.handle.dispose();
});
test('scope slicing keeps ancestry, recomputes ledgers/hashes and filters inferred edges',()=>{
 const load=loader(), {sliceViewSpec}=load(src('lib/viz/slice.ts'));
 const v=spec();v.elements.push({...v.elements[0],id:'b',parentId:'a',displayMode:'Inference',claimId:'c'}, {...v.elements[0],id:'unrelated'});
 v.edges=[{id:'ab',source:'a',target:'b',edgeKind:'call',displayMode:'Inference',evidence:v.elements[0].evidence}];
 const before=JSON.stringify(v);
 const sliced=sliceViewSpec(v,{keepIds:new Set(['b'])});
 assert.deepEqual(sliced.spec.elements.map(e=>e.id),['a','b']);
 assert.equal(sliced.spec.footer.inferences,1);assert.equal(sliced.dropped,1);
 assert.equal(sliced.spec.hidden[0].id,'unrelated');assert.ok(sliced.spec.semanticHash);
 const facts=sliceViewSpec(v,{factsOnly:true});assert.equal(facts.spec.edges.length,0);
 assert.equal(JSON.stringify(v),before);
 v.elements[1].claimId=undefined;
 assert.equal(sliceViewSpec(v).spec,null);
});
