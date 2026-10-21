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
