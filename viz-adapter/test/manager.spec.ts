import { it, expect } from 'vitest';
import { planSheet, defaultRegistry } from '../src/index';
import type { ViewSpec } from '../src/types';
const view = (): ViewSpec => ({elements:[{id:'a',elementKind:'module',label:'A',displayMode:'Fact',evidence:{file:'a',start:0,end:1},salience:{factors:[],weight:1,whyShown:'test'},fields:{}},{id:'b',parentId:'a',elementKind:'function',label:'B',displayMode:'Inference',claimId:'c',evidence:{file:'b',start:0,end:1},salience:{factors:[],weight:1,whyShown:'test'},fields:{}}],edges:[{id:'e',source:'a',target:'b',edgeKind:'call',displayMode:'Fact',evidence:{file:'a',start:0,end:1}}],footer:{facts:1,inferences:1,hypotheses:0,fog:0,stale:0,caption:''},hidden:[],gaps:[],scope:{revision:1,environments:[],coverage:{analyzed:true}},semanticHash:'',renderHash:''});
it('plans from actual hierarchy/edges, deterministically, with canvas negotiation',()=>{
 const plans=planSheet(view(),defaultRegistry());
 expect(plans.map(p=>p.chartKind)).toEqual(['tree','component-board','flow']);
 expect(plans.every(p=>p.engineId==='g6')).toBe(true);
 expect(planSheet(view(),defaultRegistry())).toEqual(plans);
});
it('scope and facts preferences remove unrelated charts and elements',()=>{
 expect(planSheet(view(),defaultRegistry(),{factsOnly:true})[0].elementSubset).toEqual(['a']);
 expect(planSheet(view(),defaultRegistry(),{scope:[]})).toEqual([]);
 expect(planSheet(view(),defaultRegistry(),{viewport:{width:320,height:600}})).toHaveLength(1);
});
it('rejects ungated input rather than planning dishonest pixels',()=>{
 const v=view();v.elements[1].claimId=undefined;
 expect(()=>planSheet(v,defaultRegistry())).toThrow('NON_FACT_WITHOUT_CLAIM');
});
