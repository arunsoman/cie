import type {AtlasViewDefinition, AtlasItemRef, ProjectionContext} from './atlas-types';
import {binding, type DependencyView} from './atlas-semantic';
import {semanticEnvelope} from './atlas-semantic-producers';
interface Entity {id:string;kind:'container'|'dependency';fields:Record<string,unknown>}
function ref(e:Entity,c:ProjectionContext):AtlasItemRef {
    const location=e.fields.location as {file?:string;start?:number;end?:number}|undefined;
    const file=location?.file??(typeof e.fields.source==='string'?e.fields.source:undefined);
    return {id:e.id,pin:c.pin,entityKind:e.kind,group:e.kind,label:String(e.fields.name??e.id),operand:{select:{kind:e.kind,filter:{ids:[e.id]}}},
        evidence:file?[{file,...(location?.start!=null&&location.end!>location.start?{startByte:location.start,endByte:location.end}:{})}]:[],
        provenance:[{kind:'stored-fact',pin:c.pin,artifact:e.kind==='container'?'stack.apps':'deps',recordId:e.id}]};
}
export const dependencyView:AtlasViewDefinition={
    id:'depgraph',binding:'selection',targetKind:'container',unfilteredTargetUniverse:true,
    outputs:()=>[{id:'graph',output:{graph:{max_nodes:100,max_edges:300,expansion:{relations:['declares_dependency'],max_depth:1}}}}],
    project(response,c){
        const output=response.outputs.find(o=>o.id==='graph');
        if(!output?.data||output.error)throw Error(output?.error?.message??'Captured dependencies unavailable');
        const graph=output.data as {nodes:Entity[];edges:{source:string;target:string;relation:string}[];truncated:boolean};
        const nodes:DependencyView['nodes']=graph.nodes.filter(e=>['container','dependency'].includes(e.kind)).map(e=>({...binding(e.id,
            e.kind==='container'?`${e.fields.name??e.id} · Unclassified`:`${e.fields.name??e.id} ${e.fields.version_req??''} · ${e.fields.ecosystem??'unknown ecosystem'}`,
            ref(e,c),'Fact',e.kind==='container'?'member':'context'),subjectKind:e.kind}));
        const byId=new Map(nodes.map(n=>[n.visualId,n]));
        const seen=new Set<string>();
        const relations:DependencyView['relations']=graph.edges.filter(e=>e.relation==='declares_dependency'&&byId.has(e.source)&&byId.has(e.target)).filter(e=>{const key=`${e.source}:${e.target}`;if(seen.has(key))return false;seen.add(key);return true;}).map(e=>{
            const target=byId.get(e.target)!,id=`${e.source}:declares:${e.target}`;
            return {...binding(id,'Declares',{...target.ref,id,label:`${byId.get(e.source)!.ref.label} declares ${target.ref.label}`},'Fact'),from:e.source,to:e.target,relation:e.relation};
        });
        const gaps=['Declared build dependencies from captured manifests; these edges do not prove runtime calls or transitive resolution.',
            'Selecting a dependency selects its declaring modules. Double-click opens captured manifest source where available.',
            'Layer: Unclassified. Repository layer rules and inferred layer assignments are not indexed (ENG-05).',
            'Modules with no linked declarations remain visible. No linked declarations does not prove that every manifest was parsed.'];
        if(!nodes.length)gaps.push('No captured application manifests are linked to this selection.');
        if(graph.truncated)gaps.push('Bounded to 100 nodes and 300 declarations; the graph is incomplete.');
        return semanticEnvelope({kind:'dependency',title:'Module dependency graph',nodes,relations,gaps,population:{pin:c.pin,version:c.version,universe:'container',expression:c.query,matches:response.matches,coverage:response.coverage??null}});
    },
};
