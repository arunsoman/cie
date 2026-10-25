import {presentations} from './atlas-presentation';
import type {AtlasViewDefinition, AtlasItemRef, ProjectionContext} from './atlas-types';
interface Entity {id:string;fields:Record<string,any>}
function rows(response:Parameters<AtlasViewDefinition['project']>[0]) {
    const out=response.outputs.find(o=>o.id==='rows');
    if(!out?.data||out.error)throw Error(out?.error?.message??'Captured records unavailable');
    return out.data as {items:Entity[];total:number;continuation:string|null;truncated:boolean};
}
function evidence(location:any){return location?.file?[{file:String(location.file),...(Number.isSafeInteger(location.start)&&location.end>location.start?{startByte:location.start,endByte:location.end}:{})}]:[];}
export const duplicationView:AtlasViewDefinition={
    id:'duplication',binding:'selection',targetKind:'duplicate_pair',outputs:continuation=>[{id:'rows',output:{entities:{page:{limit:30,continuation}}}}],
    project(r,c){const page=rows(r);return {kind:'table',title:'Duplication',total:page.total,continuation:page.continuation,
        columns:[{id:'left',label:'Source A'},{id:'right',label:'Source B'},{id:'similarity',label:'Similarity'},{id:'tier',label:'Tier'}],
        items:page.items.map(e=>{const a=e.fields.left_source,b=e.fields.right_source;
            const ref:AtlasItemRef={id:e.id,label:`${a?.name??'Unknown'} ↔ ${b?.name??'Unknown'}`,pin:c.pin,entityKind:'duplicate_pair',group:'duplicate_pair',operand:{select:{kind:'duplicate_pair',filter:{ids:[e.id]}}},evidence:[...evidence(a?.location),...evidence(b?.location)],provenance:[{kind:'stored-fact',pin:c.pin,artifact:'duplicates',recordId:e.id}]};
            return {key:e.id,label:ref.label,ref,authority:'Inference' as const,role:'member' as const,fields:{left:a?.name??'Source unavailable',right:b?.name??'Source unavailable',similarity:e.fields.similarity??'not measured',tier:e.fields.tier??'unclassified'}};
        }),gaps:['Stored structural duplicate candidates; these are not the literal quality-artifact clone pairs or FCA capability overlap. Both source declarations contribute to selection. Double-click offers captured source locations.',...(page.total?[]:['No stored duplicate candidates in this selection; this is not proof that a complete clone scan ran.']),'Literal clone table, chord ribbons, FCA overlap tab and duplication-tier facets remain pending.']};},
};
export const driftBoardView:AtlasViewDefinition={
    id:'driftboard',binding:'selection',targetKind:'finding',filter:{field:{field:'origin',op:'eq',value:'captured_document_drift'}},
    outputs:continuation=>[{id:'rows',output:{entities:{page:{limit:30,continuation}}}}],
    project(r,c){const page=rows(r);return {kind:'table',title:'Documentation drift',total:page.total,continuation:page.continuation,
        columns:[{id:'claim',label:'Document claim'},{id:'status',label:'Status'},{id:'reason',label:'Verification'}],
        items:page.items.map(e=>{const doc=e.fields.document,ref:AtlasItemRef={id:e.id,label:String(e.fields.claim),pin:c.pin,entityKind:'finding',group:'drift_claim',operand:{select:{kind:'finding',filter:{ids:[e.id]}}},
            evidence:[...(doc?.doc?[{file:String(doc.doc),...(doc.line>0?{line:Number(doc.line)}:{})}]:[]),...(e.fields.sources??[]).flatMap((s:any)=>evidence(s.location))],provenance:[{kind:'stored-fact',pin:c.pin,artifact:'drift',recordId:e.id}]};
            return {key:e.id,label:ref.label,ref,authority:'Inference' as const,role:'member' as const,fields:{claim:e.fields.claim,status:e.fields.status??'Unknown',reason:e.fields.reason??'No reason captured'}};
        }),gaps:['Recorded verification results are interpretations, not independent proof. Only claims with captured symbol mappings participate in this scope. Double-click offers document and code evidence; simultaneous DOC/CODE comparison remains pending.',...(page.items.some(e=>e.fields.complete!==true)?['Captured verification is incomplete.']:[]),...(page.total?[]:['No mapped document verification is available in this selection. Verification may not have run; this does not mean no drift exists.'])]};},
};
export const timelineView:AtlasViewDefinition={id:'timeline',binding:'selection',outputs:()=>[{id:'rows',output:{entities:{page:{limit:1}}}}],
    project:()=>({kind:'table',title:'Revision timeline',availability:'not-collected',items:[],total:0,continuation:null,gaps:['Revision timeline not yet indexed (ENG-04). No revision markers or fabricated trend are shown. Use captured revision controls when available.']})};
export const riskTreemapView:AtlasViewDefinition={id:'risktreemap',binding:'selection',targetKind:'file',filter:{field:{field:'quality.excluded',op:'eq',value:false}},
    outputs:()=>[{id:'rows',output:{entities:{fields:['path','quality'],order:[{field:'quality.hotspot_risk',direction:'desc'}],page:{limit:500}}}}],
    project(r,c){const page=rows(r);return {kind:'treemap',presentation:presentations.get('metric.treemap','metric'),renderer:'atlas-treemap',title:'Hotspot risk treemap',total:page.total,continuation:null,
        items:page.items.map(e=>{const file=String(e.fields.path),q=e.fields.quality??{},ref:AtlasItemRef={id:e.id,label:file,pin:c.pin,entityKind:'file',group:'file.path',operand:{select:{kind:'file',filter:{ids:[e.id]}}},evidence:[{file}],provenance:[{kind:'stored-fact',pin:c.pin,artifact:'captured_file_quality',recordId:e.id}]};
            return {key:e.id,label:file,ref,role:'member' as const,authority:'Fact' as const,value:typeof q.hotspot_risk==='number'?q.hotspot_risk:undefined};}),
        gaps:['Rectangle area represents positive captured hotspot risk. Zero and unknown scores have no invented area and remain selectable below. The captured risk formula is not recomputed during retrieval.',...(page.truncated?['Only the first 500 files are represented; this is a bounded treemap.']:[])]};},
};
/** Binary area packing: every rectangle has area proportional to its measured value. */
export function riskRectangles(items:{key:string;value?:number}[]) {
    const result:{key:string;x:number;y:number;width:number;height:number}[]=[];
    const positive=items.filter(i=>Number.isFinite(i.value)&&i.value!>0).sort((a,b)=>b.value!-a.value!);
    function pack(values:typeof positive,x:number,y:number,width:number,height:number):void {
        if(!values.length)return;
        if(values.length===1){result.push({key:values[0].key,x,y,width,height});return;}
        const total=values.reduce((n,v)=>n+v.value!,0);let sum=0,split=0;
        while(split<values.length-1&&sum<total/2)sum+=values[split++].value!;
        const fraction=sum/total;
        if(width>=height){pack(values.slice(0,split),x,y,width*fraction,height);pack(values.slice(split),x+width*fraction,y,width*(1-fraction),height);}
        else{pack(values.slice(0,split),x,y,width,height*fraction);pack(values.slice(split),x,y+height*fraction,width,height*(1-fraction));}
    }
    pack(positive,0,0,100,100);return result;
}
