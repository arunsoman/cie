import {StorageClient,output} from '../storage-client';
import type {AtlasStorage,AtlasQuery,CapturedEvidence} from './atlas-types';
/** Transport only; selection/projection logic never reaches into physical storage. */
export function atlasStorage(root:string,revision:string):AtlasStorage {
  const client=new StorageClient(root,revision);
  return {
    async retrieve(query,outputs,signal){return client.retrieve({query,outputs,best_effort:true},signal);},
    async source(evidence,signal){
      const range=evidence.startByte==null?'whole_file':{bytes:{start:evidence.startByte,end:evidence.endByte}};
      const query:AtlasQuery={select:{kind:'file',filter:{field:{field:'path',op:'eq',value:evidence.file}}}};
      const result=await client.retrieve({query,outputs:[{id:'source',output:{detail:{source:{range}}}}]},signal);
      if(!result.matches)throw new Error('No captured source in this revision');
      return output<CapturedEvidence>(result,'source','source');
    },
    close:()=>client.close(),
  };
}
