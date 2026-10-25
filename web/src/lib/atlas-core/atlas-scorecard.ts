import type {AtlasViewDefinition, AtlasItemRef} from './atlas-types';
interface FileRow {id:string;fields:{path:string;quality:Record<string,unknown>}}
/** Native file projection owns measurements, scan coverage and exclusion policy. */
export const scorecardView:AtlasViewDefinition={
    id:'scorecard',binding:'selection',targetKind:'file',filter:{field:{field:'quality.excluded',op:'eq',value:false}},
    outputs:continuation=>[{id:'rows',output:{entities:{fields:['path','quality'],order:[{field:'quality.hotspot_risk',direction:'desc'},{field:'path',direction:'asc'}],page:{limit:30,continuation}}}}],
    project(response,context){
        const output=response.outputs.find(o=>o.id==='rows');
        if(!output||output.schema_version!==2||output.error||!output.data||['failed','unavailable'].includes(output.status))throw Error(output?.error?.message??'Captured scorecard unavailable');
        const page=output.data as {items:FileRow[];total:number;continuation:string|null;truncated:boolean};
        const measured=(value:unknown,missing='not collected')=>typeof value==='number'&&Number.isFinite(value)?value:missing;
        return {kind:'table',title:'Module scorecard',total:page.total,continuation:page.continuation,
            columns:[{id:'file',label:'File'},{id:'coverage',label:'Coverage %'},{id:'complexity',label:'CC avg / max'},{id:'duplicates',label:'Duplicate pairs'},{id:'risk',label:'Hotspot risk'},{id:'security',label:'Security findings'},{id:'cohesion',label:'Cohesion / coupling'}],
            items:page.items.map(row=>{
                const q=row.fields.quality,file=row.fields.path;
                const ref:AtlasItemRef={id:row.id,label:file,pin:context.pin,entityKind:'file',group:'file.path',operand:{select:{kind:'file',filter:{ids:[row.id]}}},evidence:[{file}],provenance:[{kind:'stored-fact',pin:context.pin,artifact:'captured_file_quality',recordId:row.id}]};
                return {key:row.id,label:file,ref,role:'member' as const,authority:'Fact' as const,fields:{file,coverage:measured(q.coverage_percent,'not run'),complexity:typeof q.avg_cc==='number'&&typeof q.max_cc==='number'?`${q.avg_cc} / ${q.max_cc}`:'not collected',duplicates:measured(q.duplicate_pairs),risk:measured(q.hotspot_risk),security:measured(q.security_findings),cohesion:'not yet measured'}};
            }),
            gaps:[...Array.from(new Set(page.items.map(row=>row.fields.quality.collected_at).filter((value):value is string=>typeof value==='string'))).map(date=>`Captured quality collection: ${date}. Source-hash equivalence is not asserted by this projection.`),'Sorted by captured hotspot risk descending; missing scores follow measured values. Rows are paged, so large repositories are not mounted as thousands of DOM rows.',
                'Not run means no coverage result is captured for that file. Not collected is unknown, not zero. Duplicate/security zero requires explicit scanned_files coverage in the captured artifact; positive values count recorded findings/pairs.',
                'Generated/vendor exclusions use captured source scope plus fixed node_modules, vendor, target, dist, build, generated, __generated__, codegen and protobuf/codegen suffix rules. Other generated paths require ingestion classification.',
                'Cohesion / coupling is not yet measured (ENG-11). No metric is recomputed from source during retrieval.',
                ...(page.items.some(row=>!row.fields.quality.available)?['No quality artifact is captured for some displayed files. Collect and publish quality measurements to populate these columns.']:[]),
                ...(page.truncated?['More file rows are available on the next page.']:[])],
        };
    },
};
