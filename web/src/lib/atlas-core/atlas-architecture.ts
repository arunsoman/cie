import type {AtlasViewDefinition, AtlasResponse, ProjectionContext, AtlasItemRef} from './atlas-types';
import {binding, type ArchitectureView} from './atlas-semantic';
import {semanticEnvelope} from './atlas-semantic-producers';
interface Entity {id:string;kind:string;fields:Record<string,unknown>}
interface Edge {source:string;target:string;relation:string}
function read<T>(r:AtlasResponse,id:string):T {
    const output=r.outputs.find(o=>o.id===id);
    if(!output||output.schema_version!==2||output.error||!output.data||['failed','unavailable'].includes(output.status)) throw Error(output?.error?.message??`Architecture output unavailable: ${id}`);
    return output.data as T;
}
function reference(entity:Entity,c:ProjectionContext):AtlasItemRef {
    const kind=entity.kind==='container'?'container':'component',label=String(entity.fields.name??entity.fields.label??entity.id);
    const location=entity.fields.location as {file?:string;start?:number;end?:number}|undefined;
    const own={select:{kind,filter:{ids:[entity.id]}}};
    return {id:entity.id,label,pin:c.pin,entityKind:kind,group:kind,
        operand:kind==='component'?{union:[own,{follow:{input:own,relation:'child_component_of',incoming:true,min_depth:1,max_depth:8}}]}:own,
        evidence:location?.file?[{file:location.file,...(location.start!=null&&location.end!>location.start?{startByte:location.start,endByte:location.end}:{})}]:[],
        provenance:[{kind:'stored-fact',pin:c.pin,artifact:kind==='container'?'stack.apps':'captured_components',recordId:entity.id}]};
}
function architecture(id:string,target:'container'|'component'):AtlasViewDefinition {
    return {id,binding:'selection',targetKind:target,
        outputs:()=>[
            {id:'members',output:{entities:{page:{limit:500}}}},
            {id:'graph',output:{graph:{max_nodes:100,max_edges:300,expansion:{relations:target==='container'?['communicates_with']:['child_component_of','component_in_container'],max_depth:target==='container'?1:8}}}},
        ],
        project(r,c){
            const membership=read<{items:Entity[];truncated:boolean}>(r,'members'),graph=read<{nodes:Entity[];edges:Edge[];truncated:boolean}>(r,'graph');
            const members=new Set(membership.items.map(e=>e.id));
            const nodes:ArchitectureView['nodes']=graph.nodes.filter(e=>['container','component'].includes(e.kind)).map(e=>{
                const ref=reference(e,c),role=e.kind==='container'||e.fields.kind==='Container'?'container':e.fields.kind==='Domain'?'system':'component';
                const technology=String(e.fields.framework??e.fields.kind??'unknown');
                return {...binding(e.id,`${ref.label} · ${role}`,ref,'Inference',members.has(e.id)?'member':'context'),boundaryRole:role,technology};
            });
            const byId=new Map(nodes.map(n=>[n.visualId,n]));
            const allowed=target==='container'?['communicates_with']:['child_component_of','component_in_container'];
            const seen=new Set<string>(),relations:ArchitectureView['relations']=[];
            for(const edge of graph.edges.filter(e=>allowed.includes(e.relation)&&byId.has(e.source)&&byId.has(e.target))){
                const id=`${edge.source}:${edge.relation}:${edge.target}`;if(seen.has(id))continue;seen.add(id);
                const from=byId.get(edge.source)!,to=byId.get(edge.target)!,label=edge.relation==='communicates_with'?'Recorded service match':'Contained in';
                const ref:AtlasItemRef={...from.ref,id,label:`${from.ref.label} → ${to.ref.label}`,entityKind:target,group:'architecture-relation',operand:target==='container'?{union:[from.ref.operand,to.ref.operand]}:from.ref.operand,provenance:[{kind:'stored-fact',pin:c.pin,artifact:edge.relation,recordId:id}]};
                relations.push({...binding(id,label,ref,'Inference'),from:edge.source,to:edge.target,relation:edge.relation});
            }
            const gaps=[target==='container'?'C4 L2 application boundaries come from captured build records; deployment and runtime communication are unverified. Ownership uses the unique longest captured application directory.':'C4 L3 components combine captured framework roles and stored generated groupings; they are architectural interpretations, not verified business boundaries.',
                'Double-click previews a captured representative member declaration where available; this is not deployment evidence. Captured external Docker/image systems are not yet projected in this view; unrecorded relationships are not invented.'];
            if(target==='component')gaps.push('Relationships show recorded parent/container context, not a complete component dependency map; use the declared call graph for captured function calls.');
            if(!nodes.length)gaps.push('No captured architecture membership is available for this selection.');
            if(!relations.length)gaps.push('No relationships were recorded between the displayed architecture boundaries.');
            if(graph.truncated||membership.truncated)gaps.push('Architecture bounded to 100 nodes, 300 edges, and 500 membership records; parent traversal is capped at eight levels.');
            return semanticEnvelope({kind:'architecture',level:target,title:target==='container'?'C4 application containers':'C4 components',nodes,relations,gaps,population:{pin:c.pin,version:c.version,universe:target,expression:c.query,matches:r.matches,coverage:r.coverage??null}});
        }};
}
export const containerView=architecture('containers','container');
export const componentView=architecture('components','component');
