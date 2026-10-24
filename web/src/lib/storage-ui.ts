'use client';
// View-model selectors over typed storage outputs. No physical IDs, SQL,
// filesystem reads, compatibility RPCs, or alternate retrieval endpoints.
import { useCie } from './cie-store';
import { StorageClient, StorageError, output, type StorageEntity, type StorageGraph, type EntityPage } from './storage-client';
export { StorageError } from './storage-client';
let active:StorageClient|undefined;
const workspaceKey=(state:ReturnType<typeof useCie.getState>)=>JSON.stringify([state.storageRoot??state.sourceRoot,state.report?.rev]);
useCie.subscribe((state,previous)=>{if(workspaceKey(state)!==workspaceKey(previous)){const old=active;active=undefined;void old?.close().catch(()=>{});}});
export function storageFor(root:string):StorageClient{
  const state=useCie.getState();const selected=state.storageRoot??state.sourceRoot;
  if(root!==selected||state.report?.rev==null)throw new StorageError('MixedRevisionReference','Select the captured workspace before retrieving facts');
  const revision=String(state.report.rev);
  if(!active||active.isClosed||active.root!==root||active.revision!==revision){void active?.close().catch(()=>{});active=new StorageClient(root,revision);}
  return active;
}
export interface SourceData {type:'source';path:string;text:string;content_hash:string;returned:{start:number;end:number};line_starts:number[];revision:string;truncated:boolean;continuation:string|null}
export async function readCapturedSource(root:string,file:string,start?:number,end?:number,signal?:AbortSignal):Promise<SourceData>{
  if(start!=null&&(!Number.isSafeInteger(start)||!Number.isSafeInteger(end)||start<0||end!<=start||end!-start>256*1024))throw new StorageError('InvalidRequest','Select a non-empty source range of at most 256 KiB');
  const range=start==null?'whole_file':{bytes:{start,end}};
  const result=await storageFor(root).retrieve({query:{select:{kind:'file',filter:{field:{field:'path',op:'eq',value:file}}}},outputs:[{id:'source',output:{detail:{source:{range}}}}],budget:{max_response_bytes:4*1024*1024}},signal);
  if(!result.matches)throw new StorageError('SourceUnavailable','The file was not captured in this revision');
  return output<SourceData>(result,'source','source');
}
export async function readCapturedFile(root:string,file:string,signal?:AbortSignal){
  const source=await readCapturedSource(root,file,undefined,undefined,signal);
  if(source.truncated)throw new StorageError('BudgetExceeded','This file exceeds the bounded full-file view. Use a symbol source snippet to inspect a smaller range.');
  return {path:file,content:source.text,start:1,end:source.text.split('\n').length,totalLines:source.line_starts.length,truncated:source.truncated,revision:source.revision,origin:'captured_revision'};
}
export interface SymbolRef {id:string;qualName:string;kind:string;path:string;startByte:number;endByte:number}
const symbolRef=(entity:StorageEntity):SymbolRef=>{
  const fields=entity.fields,loc=fields.location as {file?:string;start?:number;end?:number}|undefined;
  return {id:entity.id,qualName:String(fields.qualified_name??fields.name??entity.id),kind:String(fields.kind??entity.kind),path:loc?.file??String((fields.file as {path?:string})?.path??''),startByte:Number(loc?.start??0),endByte:Number(loc?.end??0)};
};
export async function searchStoredSymbols(root:string,query:string,signal?:AbortSignal){
  const result=await storageFor(root).retrieve({query:{select:{kind:'symbol',filter:{field:{field:'qualified_name',op:'contains_fold',value:query}}}},outputs:[{id:'rows',output:{entities:{page:{limit:30}}}}]},signal);
  return output<EntityPage>(result,'rows','entities').items.map(symbolRef);
}
export async function readFunctionIr(root:string,id:string,signal?:AbortSignal){
  const result=await storageFor(root).retrieve({query:{select:{kind:'symbol',filter:{ids:[id]}}},outputs:[{id:'ir',output:{detail:{function_ir:{include_calls:true}}}}]},signal);
  const ir=output<{type:string;symbol:StorageEntity;blocks:{id:number;block_index:number;kind:string}[];instructions:{block_id:number;inst_index:number;op:string;span_start:number;span_end:number}[];edges:{from_block_id:number;to_block_id:number;kind:string}[];calls:{callee_text:string|null;resolution:string;call_kind:string;stmt_index:number}[]}>(result,'ir','function_ir');
  const indices=new Map(ir.blocks.map(block=>[block.id,block.block_index]));
  return {available:true,symbol:symbolRef(ir.symbol),blocks:ir.blocks.map(block=>({id:block.id,blockIndex:block.block_index,kind:block.kind,instructions:ir.instructions.filter(inst=>inst.block_id===block.id).map(inst=>({instIndex:inst.inst_index,op:inst.op,spanStart:inst.span_start,spanEnd:inst.span_end}))})),edges:ir.edges.map(edge=>({fromBlockIndex:indices.get(edge.from_block_id)!,toBlockIndex:indices.get(edge.to_block_id)!,kind:edge.kind})),calls:ir.calls.map(call=>({calleeText:call.callee_text,calleeQualName:null,resolution:call.resolution,callKind:call.call_kind,stmtIndex:call.stmt_index}))};
}
export interface DuplicateRow {id:string;similarity:number;tier:string;detection:string;epistemic:string;counterArgument:string|null;a:SymbolRef;b:SymbolRef;canonical:SymbolRef|null}
export async function readDuplicates(root:string,options:{tier?:string;detection?:string;epistemic?:string;page?:number},signal?:AbortSignal){
  const predicates=Object.entries(options).filter(([key,value])=>key!=='page'&&value).map(([field,value])=>({field:{field,op:'eq',value}}));
  const query={select:{kind:'duplicate_pair',filter:predicates.length?{and:predicates}:'all'}};
  const client=storageFor(root),pageSize=50;
  const {data}=await client.entities(query,options.page??0,pageSize,signal);
  const ids=data.items.map(item=>item.id);
  const result=await client.retrieve({query:{select:{kind:'duplicate_pair',filter:{ids}}},outputs:[{id:'graph',output:{graph:{max_nodes:500,max_edges:1000,expansion:{relations:['left_symbol','right_symbol','canonical_symbol']}}}}]},signal);
  const graph=output<StorageGraph>(result,'graph','graph');if(graph.truncated)throw new StorageError('BudgetExceeded','Duplicate relationships exceeded the view limit');
  const entities=new Map(graph.nodes.map(node=>[node.id,node]));
  const target=(id:string,relation:string)=>{const edge=graph.edges.find(edge=>edge.source===id&&edge.relation===relation);return edge&&entities.get(edge.target);};
  const rows:DuplicateRow[]=data.items.map(item=>{
    const a=target(item.id,'left_symbol'),b=target(item.id,'right_symbol'),canonical=target(item.id,'canonical_symbol');
    if(!a||!b)throw new StorageError('CorruptData','Duplicate evidence has missing symbol references');
    return {id:item.id,similarity:Number(item.fields.similarity),tier:String(item.fields.tier),detection:String(item.fields.detection),epistemic:String(item.fields.epistemic),counterArgument:item.fields.counter_argument==null?null:String(item.fields.counter_argument),a:symbolRef(a),b:symbolRef(b),canonical:canonical?symbolRef(canonical):null};
  });
  const facets=await client.retrieve({query:{select:{kind:'duplicate_pair'}},outputs:[{id:'facets',output:{facets:{dimensions:['tier','detection','epistemic'],page:{limit:500}}}}]},signal);
  const buckets=output<{items:{dimension:string;member:unknown}[];truncated:boolean}>(facets,'facets','facets');
  const values=(dimension:string)=>buckets.items.filter(bucket=>bucket.dimension===dimension).map(bucket=>String(bucket.member));
  return {available:true,rows,total:data.total,page:options.page??0,pageSize,facets:{tiers:values('tier'),detections:values('detection'),epistemics:values('epistemic')}};
}
export async function readDuplicateClusters(root:string,signal?:AbortSignal){
  const client=storageFor(root);
  const result=await client.retrieve({query:{select:{kind:'cluster'}},outputs:[{id:'clusters',output:{entities:{page:{limit:50}}}}]},signal);
  const page=output<EntityPage>(result,'clusters','entities');
  const ids=page.items.map(cluster=>cluster.id);
  if(!ids.length)return {available:true,tierUsed:'canonical',totalFamilies:0,totalSymbolsInvolved:0,clusters:[],truncated:false};
  const [related,totals]=await Promise.all([
    client.retrieve({query:{select:{kind:'cluster',filter:{ids}}},outputs:[{id:'graph',output:{graph:{max_nodes:2000,max_edges:5000,expansion:{relations:['member_of_cluster'],incoming:true}}}}]},signal),
    client.retrieve({query:{follow:{input:{select:{kind:'cluster'}},relation:'member_of_cluster',incoming:true}},outputs:[{id:'totals',output:{cube:{dimensions:[]}}}]},signal),
  ]);
  const graph=output<StorageGraph>(related,'graph','graph');
  const count=output<{grand_totals:{count:number}}>(totals,'totals','cube');
  const clusters=page.items.map(cluster=>{
    const refs=graph.edges.filter(edge=>edge.relation==='member_of_cluster'&&edge.target===cluster.id).map(edge=>graph.nodes.find(node=>node.id===edge.source)).filter((node):node is StorageEntity=>!!node).map(symbolRef);
    return {id:cluster.id,hub:refs[0],members:refs.slice(1),totalMembers:refs.length,truncated:graph.truncated};
  }).filter(cluster=>!!cluster.hub);
  return {available:true,tierUsed:'canonical',totalFamilies:page.total,totalSymbolsInvolved:count.grand_totals.count,clusters,truncated:page.truncated||graph.truncated};
}
export async function readStoredImpact(root:string,name:string,signal?:AbortSignal){
  const client=storageFor(root),result=await client.retrieve({query:{select:{kind:'symbol',filter:{or:[{field:{field:'qualified_name',op:'eq',value:name}},{field:{field:'name',op:'eq',value:name}}]}}},outputs:[{id:'rows',output:{entities:{page:{limit:100}}}}]},signal);
  const candidates=output<EntityPage>(result,'rows','entities');
  if(candidates.total!==1)throw new StorageError(candidates.total?'AmbiguousSymbol':'ArtifactUnavailable',candidates.total?'That symbol name is ambiguous; select its qualified name.':'Symbol was not captured in this revision.');
  const impact=await client.retrieve({query:{select:{kind:'symbol',filter:{ids:[candidates.items[0].id]}}},outputs:[{id:'impact',output:{detail:{impact:{availability:'build_if_missing'}}}}]},signal);
  return output<{type:'impact';symbol:string;affected_symbols:string[];minimum_test_set:string[];affected_concepts:string[];provenance:string;coverage:unknown}>(impact,'impact','impact');
}
export interface DiagnosticDataset {name:string;count:number|null}
export interface StorageInventory {datasets:DiagnosticDataset[];revision:string}
export async function readStorageInventory(root:string,signal?:AbortSignal){return storageFor(root).call<StorageInventory>('diagnostics',{},signal);}
export async function readStorageRows(root:string,dataset:string,page:number,signal?:AbortSignal){
  const data=await storageFor(root).call<{dataset:string;rows:Record<string,unknown>[];total:number;columns:string[]}>('diagnostics',{dataset,limit:50,offset:page*50},signal);
  return {available:true,...data,page,pageSize:50};
}
