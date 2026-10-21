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
test('plural context deduplicates, bounds and clears stale revisions',()=>{
 const {addContext}=loader()(src('lib/chat-context.ts'));
 let chips=[];for(let i=0;i<30;i++)chips=addContext(chips,{id:String(i),revision:1,files:[],label:String(i),kind:'test'});
 assert.equal(chips.length,24);
 chips=addContext(chips,{...chips[0],label:'updated'});assert.equal(chips.length,24);assert.equal(chips.at(-1).label,'updated');
 assert.equal(addContext(chips,{id:'new',revision:2,files:[],label:'new',kind:'test'}).length,1);
});
test('conductor broadcasts valid IDs once and detaches disposed charts',()=>{
 const load=loader(),{SelectionConductor,createSelectionHub}=load(path.resolve(__dirname,'../../viz-adapter/src/index.ts'));
 const hubA=createSelectionHub(),hubB=createSelectionHub();
 const a={onSelection:hubA.subscribe,setSelected:hubA.setSelected},b={onSelection:hubB.subscribe,setSelected:hubB.setSelected};
 const conductor=new SelectionConductor();let calls=0;
 conductor.attach(a,['a','b'],()=>calls++);const off=conductor.attach(b,['b'],()=>calls++);
 hubA.setSelected(['b'],'user');assert.deepEqual(hubB.selected(),['b']);assert.equal(calls,1);
 off();hubA.setSelected(['a'],'user');assert.deepEqual(hubB.selected(),['b']);
});
