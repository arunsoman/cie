import type {AtlasResponse,AtlasViewDefinition,AtlasItemRef,ProjectionContext,AtlasQuery,EvidenceRef,AtlasDatum} from './atlas-types';
interface Entity { id:string; fields:Record<string,unknown> }
function data<T>(r:AtlasResponse,id:string):T {
  const o=r.outputs.find(o=>o.id===id);
  if(!o||o.schema_version!==2||o.error||['failed','unavailable'].includes(o.status)||!o.data)throw new Error(o?.error?.message??`Missing Atlas output ${id}`);
  return o.data as T;
}
const ids=(values:string[]):AtlasQuery=>({select:{kind:'symbol',filter:{ids:values}}});
function evidence(e:Entity):EvidenceRef[]{
  const loc=e.fields.location as {file?:string;start?:number;end?:number}|undefined;
  return loc?.file?[{file:loc.file,...(loc.start!=null&&loc.end!>loc.start?{startByte:loc.start,endByte:loc.end}: {})}]:[];
}
function entityItem(e:Entity,c:ProjectionContext):AtlasDatum {
  const label=String(e.fields.qualified_name??e.fields.name??e.id),loc=evidence(e);
  const file=loc[0]?.file;
  const detailRef=file?{id:`file:${file}`,pin:c.pin,label:file,group:'file.path',operand:{select:{kind:'symbol',filter:{field:{field:'file.path',op:'eq',value:file}}}} as AtlasQuery,evidence:[{file}]}:undefined;
  return {key:e.id,label,detail:file??String(e.fields.kind??''),detailRef,role:'member',ref:{id:e.id,pin:c.pin,label,group:'symbol',operand:ids([e.id]),evidence:loc}};
}
export const symbolTable:AtlasViewDefinition={
  id:'symbols',binding:'selection',
  outputs:continuation=>[{id:'rows',output:{entities:{page:{limit:30,continuation}}}}],
  project(r,c){const p=data<{items:Entity[];total:number;continuation:string|null;truncated:boolean}>(r,'rows');return {kind:'table',title:'Selected symbols',items:p.items.map(e=>entityItem(e,c)),total:p.total,continuation:p.continuation,gaps:p.truncated?['Additional symbols are available on the next page.']:[]};},
};
export function metricBars(id:string,field:string,title:string,binding:'selection'|'repository'='selection'):AtlasViewDefinition {
  return {id,binding,
    outputs:continuation=>[{id:'cube',output:{cube:{dimensions:[{id:'group',dimension:{field}}],measures:[{id:'count',measure:'count'}],drill_through:'enabled',page:{limit:30,continuation}}}}],
    project(r,c){const p=data<{items:{members:{group:unknown};measures:{count:number};contributors?:string}[];total:number;continuation:string|null;truncated:boolean}>(r,'cube');
      const items=p.items.map(cell=>{
        if(!cell.contributors)throw new Error('Aggregate missing exact contributor descriptor');
        const label=cell.members.group==null?'Unknown':String(cell.members.group);
        const ref:AtlasItemRef={id:field==='file.path'?`file:${label}`:`bucket:${field}:${JSON.stringify(cell.members.group)}`,label,pin:c.pin,group:field,
          operand:{cell_contributors:{descriptor:cell.contributors}},evidence:field==='file.path'&&cell.members.group!=null?[{file:label}]:[]};
        return {key:ref.id,label,value:cell.measures.count,role:'member' as const,ref};
      });
      return {kind:'bars',title,items,total:p.total,continuation:p.continuation,gaps:p.truncated?['Additional groups are available on the next page.']:[]};
    },
  };
}
export const callGraph:AtlasViewDefinition={
  id:'calls',binding:'selection',
  outputs:()=>[{id:'graph',output:{graph:{max_nodes:24,max_edges:60,expansion:{relations:['declared_calls'],max_depth:1}}}}],
  project(r,c){const graph=data<{nodes:Entity[];edges:{source:string;target:string;relation:string}[];truncated:boolean}>(r,'graph');
    // Expansion endpoints are context, not additional selected members. Exact
    // member IDs come from an explicit membership output in this same request.
    const membership=data<{items:Entity[];truncated:boolean}>(r,'members');
    const members=new Set(membership.items.map(e=>e.id));
    const items=graph.nodes.map(e=>({...entityItem(e,c),role:members.has(e.id)?'member' as const:'context' as const}));
    return {kind:'graph',title:'Declared call graph — static, not runtime execution',items,
      links:graph.edges.filter((e,index,all)=>e.relation==='declared_calls'&&all.findIndex(x=>x.source===e.source&&x.target===e.target&&x.relation===e.relation)===index).map(e=>{
        const a=items.find(n=>n.key===e.source)!,b=items.find(n=>n.key===e.target)!;
        const key=`edge:${e.source}:${e.relation}:${e.target}`;
        return {key,from:e.source,to:e.target,label:'Declared call',ref:{id:key,label:`${a.label} → ${b.label}`,pin:c.pin,group:'relationship',operand:ids([e.source,e.target]),evidence:[...a.ref.evidence,...b.ref.evidence].filter((v,i,all)=>all.findIndex(w=>JSON.stringify(w)===JSON.stringify(v))===i)}};
      }),total:r.matches,continuation:null,gaps:[...(graph.truncated?['Graph limited to 24 nodes / 60 edges.']:[]),...(membership.truncated?['Membership page limited; additional displayed nodes are conservatively labelled context.']:[])]};
  },
};
// Page membership uses the same population and request as graph expansion.
const graphOutputs=callGraph.outputs;
callGraph.outputs=()=>[...graphOutputs(null),{id:'members',output:{entities:{page:{limit:500}}}}];
export const reviewerViews:AtlasViewDefinition[]=[symbolTable,metricBars('files','file.path','Repository file contributors','repository'),metricBars('kinds','kind','Selected symbols by kind'),callGraph];
