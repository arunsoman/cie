// One selection population shared by Atlas tables, diagrams and counts.
// IDs remain those of the pinned report; filtering never renumbers locations.
import type { CieReport, ConceptTree } from './cie-types';
import type { TechStack } from './stack-types';
import type { AtlasSelection } from './atlas';
import { containerOfPath } from './atlas';

export function atlasPopulation(report:CieReport, concepts:ConceptTree|null, stack:TechStack|null, selection:AtlasSelection) {
  const ref=selection.primary;
  if(!ref) return {report,concepts,stack,files:new Set(report.files),symbolIds:new Set(report.symbols.map(s=>Number(s.id))),label:'Repository',truncated:false};
  const byId=new Map(report.symbols.map(s=>[Number(s.id),s]));
  const sums=new Map(report.summaries.map(s=>[Number(s.sym),s]));
  const ids=new Set<number>();
  if(ref.kind==='module') {
    for(const s of report.symbols)if(report.files[s.loc.file]===ref.id)ids.add(Number(s.id));
  } else if(ref.kind==='symbol') {
    const seed=report.symbols.filter(s=>s.qual_name===ref.id && (!ref.loc?.file || report.files[s.loc.file]===ref.loc.file) && (ref.loc?.byteStart==null || s.loc.start===ref.loc.byteStart));
    // Duplicate qualified names are not interchangeable declarations.
    if(seed.length===1)ids.add(Number(seed[0].id));
  } else if(ref.kind==='concept') {
    for(const id of ref.symbolIds??[])if(byId.has(Number(id)))ids.add(Number(id));
  } else if(ref.kind==='container') {
    for(const s of report.symbols)if(containerOfPath(report.files[s.loc.file],stack?.apps)===ref.id)ids.add(Number(s.id));
  } else if(ref.loc?.file) {
    for(const s of report.symbols)if(report.files[s.loc.file]===ref.loc.file)ids.add(Number(s.id));
  }
  let truncated=false;
  if(ref.kind==='symbol') {
    const queue=[...ids];
    for(let i=0;i<queue.length;i++) {
      const sum=sums.get(queue[i]);
      const targets=[...(sum?.callees??[]).map(Number),...(sum?.call_sites??[]).flatMap(c=>c.target_symbols??[])];
      for(const id of targets)if(byId.has(id)&&!ids.has(id)) {
        if(ids.size>=1000){truncated=true;continue;}
        ids.add(id);queue.push(id);
      }
    }
  }
  const files=new Set([...ids].map(id=>report.files[byId.get(id)!.loc.file]));
  if(ref.kind==='module')files.add(ref.id);
  const scoped={...report,viewScope:{files:[...files],symbolIds:[...ids]},symbols:report.symbols.filter(s=>ids.has(Number(s.id))),summaries:report.summaries.filter(s=>ids.has(Number(s.sym)))};
  const nodes=concepts?.nodes??[];
  const keep=new Set(nodes.filter(c=>c.symbols.some(id=>ids.has(Number(id)))).map(c=>c.id));
  // Ancestors are context; their membership is clipped to the match population.
  for(const id of [...keep]){let p=nodes.find(c=>c.id===id)?.parent;let guard=0;while(p!=null&&guard++<nodes.length){keep.add(p);p=nodes.find(c=>c.id===p)?.parent;}}
  const structural=nodes.filter(c=>keep.has(c.id)&&(c.kind!=='Module'||files.has(c.label)));
  const retained=new Set(structural.map(c=>c.id));
  const scopedConcepts=concepts?{...concepts,nodes:structural.map(c=>({...c,symbols:c.symbols.filter(id=>ids.has(Number(id))),behaviors:c.behaviors?.filter(b=>ids.has(Number(b.summary))),parent:c.parent!=null&&retained.has(c.parent)?c.parent:null,children:c.children.filter(id=>retained.has(id))}))}:null;
  const routeMatches=(r:{rel:string;owner:string;method:string})=>files.has(r.rel)&&(ref.kind!=='symbol'||scoped.symbols.some(s=>s.qual_name===`${r.owner}.${r.method}`&&report.files[s.loc.file]===r.rel));
  const scopedStack=stack?{...stack,apps:stack.apps.filter(a=>[...files].some(f=>containerOfPath(f,stack.apps)===(a.dir||'(project root)'))),routes:stack.routes.filter(routeMatches),mains:stack.mains.filter(m=>files.has(m.rel))}:null;
  return {report:scoped,concepts:scopedConcepts,stack:scopedStack,files,symbolIds:ids,label:ref.label,truncated};
}
