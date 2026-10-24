import type {AtlasViewSpec,AtlasEvidencePolicy} from './atlas-types';
/** Missing epistemic metadata is Unknown, never silently upgraded to a fact. */
export function applyEvidencePolicy(spec:AtlasViewSpec,policy:AtlasEvidencePolicy):AtlasViewSpec {
 if(!policy.factsOnly)return spec;
 const items=spec.items.filter(i=>i.authority==='Fact'),ids=new Set(items.map(i=>i.key));
 const links=spec.links?.filter(e=>e.authority==='Fact'&&ids.has(e.from)&&ids.has(e.to));
 const hidden=spec.items.length-items.length+(spec.links?.length??0)-(links?.length??0);
 // total remains a population count, explicitly separate from visible element count.
 return {...spec,items,links,gaps:[...spec.gaps,...(hidden?[`${hidden} elements withheld by Facts-only policy; population counts are unchanged.`]:[])]};
}
