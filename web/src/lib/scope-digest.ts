import type { CieReport, CieViewSpec, ConceptTree } from './cie-types';
import type { ChatContextElement } from './chat-context';
export interface AnswerScope {
  revision: number;
  files: string[];
  symbols: number;
  selections: string[];
  header: string;
}
/** Restrict facts before ANY answerer sees them. File indexes remain stable. */
export function scopeDigest(report: CieReport | null, viewspec: CieViewSpec | null, concepts: ConceptTree | null, selection: ChatContextElement[]) {
  if (!report || !selection.length) return {report, viewspec, concepts, scope:null as AnswerScope | null};
  const files = new Set(selection.filter(c => c.revision === report.rev).flatMap(c => c.files).filter(f => report.files.includes(f)));
  const symbols = report.symbols.filter(s => files.has(report.files[s.loc.file]));
  const ids = new Set(symbols.map(s => s.id));
  const names = new Set(symbols.flatMap(s => [s.name,s.qual_name]));
  const scope: AnswerScope = {
    revision:report.rev, files:[...files], symbols:symbols.length, selections:selection.map(c=>c.label),
    header:`Drawn from ${files.size} files over ${symbols.length} named things, covered by your selections. Other project facts are outside this answer's scope.`,
  };
  const scopedReport: CieReport = {
    ...report,
    files:report.files.map(f => files.has(f) ? f : ''), symbols,
    summaries:report.summaries.filter(s=>ids.has(s.sym)).map(s=>({...s,callees:s.callees.filter(n=>names.has(n)),unresolved_callees:[]})),
    findings:report.findings.filter(f=>ids.has(f.symbol)),
    contracts:report.contracts.filter(c=>c.symbols.length && c.symbols.every(id=>ids.has(id))).map(c=>({...c,verified_by:c.verified_by?.filter(id=>ids.has(id))})),
    coverage:{...report.coverage,files_total:files.size,files_analyzed:files.size,symbols_total:symbols.length,symbols_analyzed:symbols.length,complete:false,notes:['Selection-filtered facts; project-wide coverage does not establish completeness of this slice.']},
    evidence:{...report.evidence,assumptions:[],unresolved:[],unsupported:[]}, warnings:[],
    fault_analysis:null, state_model:null,
  };
  const elements=viewspec?.elements.filter(e=>files.has(e.evidence?.file));
  const elementIds=new Set(elements?.map(e=>e.id));
  const scopedView=viewspec && elements ? {...viewspec,elements,edges:viewspec.edges.filter(e=>elementIds.has(e.source)&&elementIds.has(e.target)),hidden:[],gaps:[],footer:undefined,semanticHash:undefined,renderHash:undefined} : null;
  // Whole-domain behavior/rules can include other files: retain only nodes
  // entirely supported by the selected symbols, not broad ancestor rollups.
  const nodes=concepts?.nodes.filter(n=>n.symbols.length>0 && n.symbols.every(id=>ids.has(id)));
  const nodeIds=new Set(nodes?.map(n=>n.id));
  const scopedConcepts=concepts && nodes ? {...concepts,nodes:nodes.map(n=>({...n,parent:n.parent !== null && nodeIds.has(n.parent) ? n.parent : null,children:n.children.filter(id=>nodeIds.has(id))})),unattached_behaviors:[],unattached_rules:[]} : null;
  return {report:scopedReport,viewspec:scopedView,concepts:scopedConcepts,scope};
}
