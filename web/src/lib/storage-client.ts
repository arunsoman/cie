// Browser transport for the same revision-pinned storage contract used by MCP.
export type EntityId = string;
export interface StorageEntity { id: EntityId; kind: string; fields: Record<string, unknown>; evidence_refs?: string[] }
export interface StorageOutput<T = Record<string, unknown>> { id: string; kind: string; schema_version: number; status: 'ready'|'empty'|'truncated'|'unavailable'|'failed'; data: T|null; error?: {code:string;message:string} }
export interface StorageResponse { api_version: number; repository: string; revision: string; matches: number; coverage: unknown; outputs: StorageOutput[] }
export interface EntityPage { type:'entities'; items:StorageEntity[]; total:number; continuation:string|null; truncated:boolean }
export interface StorageEdge { source:EntityId;target:EntityId;relation:string }
export interface StorageGraph {type:'graph';nodes:StorageEntity[];edges:StorageEdge[];truncated:boolean}
export class StorageError extends Error { constructor(public code:string,message:string){super(message);} }
export async function storageTransport<T>(body:Record<string,unknown>,signal?:AbortSignal):Promise<T>{
  const response=await fetch('/api/storage',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),signal});
  const data=await response.json();
  if(!response.ok)throw new StorageError(data.code||'Io',data.error||'Storage request failed');
  return data as T;
}
export function output<T>(response:StorageResponse,id:string,type?:string):T {
  const result=response.outputs.find(item=>item.id===id);
  if(!result)throw new StorageError('SchemaMismatch',`Missing output ${id}`);
  if(result.error||result.status==='failed'||result.status==='unavailable')throw new StorageError(result.error?.code||'ArtifactUnavailable',result.error?.message||'This output is unavailable in the selected revision');
  if(result.schema_version!==2||!result.data||typeof result.data!=='object'||(type&&result.data.type!==type))throw new StorageError('SchemaMismatch','Unsupported storage response schema');
  return result.data as T;
}
export class StorageClient {
  private opening?:Promise<{context:string}>;
  private closed=false;
  private readonly cursors=new Map<string,Map<number,string|null>>();
  constructor(readonly root:string,readonly revision:string) {if(!root||!revision)throw new StorageError('InvalidRequest','A repository and captured revision are required');}
  get isClosed(){return this.closed;}
  async context():Promise<string>{
    if(this.closed)throw new StorageError('CursorExpired','The workspace storage context has closed');
    this.opening??=storageTransport<{context:string}>({op:'open',root:this.root,revision:this.revision}).catch(error=>{this.opening=undefined;throw error;});
    const opened=await this.opening;
    if(this.closed)throw new StorageError('CursorExpired','The workspace changed while storage was opening');
    return opened.context;
  }
  async call<T>(op:string,args:Record<string,unknown>={},signal?:AbortSignal):Promise<T>{
    try{return await storageTransport<T>({op,context:await this.context(),...args},signal);}
    catch(error){if(error instanceof StorageError && error.code==='CursorExpired')void this.close().catch(()=>{});throw error;}
  }
  async retrieve(request:Record<string,unknown>,signal?:AbortSignal):Promise<StorageResponse>{
    const result=await this.call<StorageResponse>('retrieve',{request},signal);
    if(result.revision!==this.revision)throw new StorageError('MixedRevisionReference','Storage returned a different revision');
    return result;
  }
  // Cursors stay on the reader that issued them. No SQL offsets or numeric IDs.
  async entities(query:unknown,page:number,limit=50,signal?:AbortSignal):Promise<{response:StorageResponse;data:EntityPage}>{
    if(!Number.isSafeInteger(page)||page<0||page>2000)throw new StorageError('InvalidRequest','Invalid page');
    const key=JSON.stringify([query,limit]);let pages=this.cursors.get(key);
    if(!pages){if(this.cursors.size>=32)this.cursors.delete(this.cursors.keys().next().value!);pages=new Map([[0,null]]);this.cursors.set(key,pages);}
    let result:StorageResponse|undefined;
    for(let index=0;index<=page;index++){
      if(index<page&&pages.has(index+1))continue;
      const continuation=pages.get(index);
      if(index>0&&!continuation)throw new StorageError('CursorExpired','This result page is no longer available; reload the view');
      result=await this.retrieve({query,outputs:[{id:'rows',output:{entities:{page:{limit,continuation}}}}]},signal);
      const data=output<EntityPage>(result,'rows','entities');pages.set(index+1,data.continuation);
      if(index===page)return {response:result,data};
      if(!data.continuation)throw new StorageError('InvalidRequest','Requested page is beyond the result');
    }
    throw new StorageError('InvalidRequest','Invalid page');
  }
  async close():Promise<void>{
    if(this.closed)return;this.closed=true;this.cursors.clear();
    if(this.opening){const opened=await this.opening.catch(()=>null);if(opened)await storageTransport({op:'close',context:opened.context});}
  }
}
