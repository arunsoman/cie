import type { AtlasPin, AtlasViewDefinition, AtlasViewSpec } from './atlas-types';
import {assertItem} from './atlas-selection';
export function assertDefinition(view:AtlasViewDefinition){
  if(!/^[a-z][a-z0-9_-]*$/.test(view.id)||!['selection','repository'].includes(view.binding))throw new Error('Invalid Atlas view registration');
  const outputs=view.outputs(null),ids=outputs.map(o=>o.id);
  if(!ids.length||new Set(ids).size!==ids.length)throw new Error('View outputs must have unique local IDs');
}
export function assertView(spec:AtlasViewSpec,pin:AtlasPin){
  if(!spec.kind||!spec.title||!Number.isFinite(spec.total)||spec.total<0||!Array.isArray(spec.gaps))throw new Error('Invalid Atlas ViewSpec');
  const ids=new Set<string>();
  for(const item of spec.items){if(ids.has(item.key))throw new Error('Duplicate view element');ids.add(item.key);if(item.value!=null&&!Number.isFinite(item.value))throw new Error('Non-finite metric');assertItem(item.ref,pin);if(item.detailRef)assertItem(item.detailRef,pin);if(!['member','context'].includes(item.role))throw new Error('Missing membership role');}
  for(const link of spec.links??[]){assertItem(link.ref,pin);if(ids.has(link.key))throw new Error('Duplicate view element');ids.add(link.key);if(!spec.items.some(i=>i.key===link.from)||!spec.items.some(i=>i.key===link.to))throw new Error('Dangling Atlas edge');}
}
