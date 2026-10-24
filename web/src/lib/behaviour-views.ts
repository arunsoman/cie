import type { CieReport, SymbolInfo } from './cie-types';
import type { ChatChartSpec, SequenceMsg, UmlNode, UmlLink } from './chat-chart';
export interface FunctionModel {
  symbol:{qualName:string;path:string;startByte:number;endByte:number};
  blocks:{id:number;blockIndex:number;kind:string;instructions:{instIndex:number;op:string;spanStart:number;spanEnd:number}[]}[];
  edges:{fromBlockIndex:number;toBlockIndex:number;kind:string}[];
}
export function cfgChart(model:FunctionModel,source:{text:string;returned:{start:number;end:number}},report?:CieReport,symbolId?:number):ChatChartSpec {
  const bytes=new TextEncoder().encode(source.text),decoder=new TextDecoder();
  const snippet=(a:number,b:number)=>decoder.decode(bytes.slice(Math.max(0,a-source.returned.start),Math.max(0,b-source.returned.start))).trim().replace(/\s+/g,' ');
  const sites=report?.summaries.find(s=>Number(s.sym)===symbolId)?.call_sites??[];
  const nodes:UmlNode[]=[],links:UmlLink[]=[];
  const entry=model.blocks.find(b=>b.kind==='entry')?.blockIndex;
  const reachable=new Set<number>(entry==null?[]:[entry]);const queue=[...reachable];
  for(let i=0;i<queue.length;i++)for(const edge of model.edges)if(edge.fromBlockIndex===queue[i]&&!reachable.has(edge.toBlockIndex)){reachable.add(edge.toBlockIndex);queue.push(edge.toBlockIndex);}
  const first=new Map<number,string>(),last=new Map<number,string>();
  for(const b of model.blocks.filter(b=>reachable.has(b.blockIndex))){
    const header=`cfg:${b.blockIndex}`;first.set(b.blockIndex,header);let cursor=header;
    const loc={file:model.symbol.path,byteStart:model.symbol.startByte,byteEnd:model.symbol.endByte};
    nodes.push({id:header,title:b.kind==='entry'?'Start':b.kind==='exit'?'End':`Block ${b.blockIndex}`,kind:b.kind==='entry'?'start':b.kind==='exit'?'end':'function',certainty:'Fact',loc});
    for(const inst of b.instructions.filter(i=>!['Const','Load','BinOp'].includes(i.op))){
      const id=`cfg:${b.blockIndex}:${inst.instIndex}`;
      const site=sites.find(c=>c.span.start===inst.spanStart&&c.span.end===inst.spanEnd);
      const target=site?.target_symbols?.length===1?site.target_symbols[0]:undefined;
      nodes.push({id,title:snippet(inst.spanStart,inst.spanEnd)||inst.op,kind:inst.op==='Cmp'?'decision':inst.op==='Ret'?'end':'function',lines:[inst.op,...(site?.dispatch==='candidate'?['Declared target; runtime dispatch unobserved']:[])],certainty:'Fact',loc:{file:model.symbol.path,byteStart:inst.spanStart,byteEnd:inst.spanEnd},symbolId:target});
      links.push({from:cursor,to:id,label:'next',certainty:'Fact'});cursor=id;
    }
    last.set(b.blockIndex,cursor);
  }
  for(const edge of model.edges)if(last.has(edge.fromBlockIndex)&&first.has(edge.toBlockIndex))links.push({from:last.get(edge.fromBlockIndex)!,to:first.get(edge.toBlockIndex)!,label:edge.kind==='branch_true'?'Yes':edge.kind==='branch_false'?'No':edge.kind,certainty:'Fact'});
  return {kind:'flow',engine:'g6',layout:'dagre',title:`Control flow — ${model.symbol.qualName}`,tell:'Captured CFG blocks and source predicates. Static analysis, not an observed execution; external dispatch and unsupported constructs may remain incomplete. Select a call to inspect its declared function and control flow.',source:'Pinned storage API · function_ir + captured source',nodes,links};
}
export function callSiteSequence(report:CieReport,entryId:number,maxMessages=60,maxDepth=8):ChatChartSpec|null {
  const byId=new Map(report.symbols.map(s=>[Number(s.id),s])),sums=new Map(report.summaries.map(s=>[Number(s.sym),s]));
  const root=byId.get(entryId);if(!root)return null;
  const msgs:SequenceMsg[]=[];let omitted=0;
  const location=(s:SymbolInfo)=>({file:report.files[s.loc.file],byteStart:s.loc.start,byteEnd:s.loc.end});
  const lane=(s:SymbolInfo)=>report.files[s.loc.file];
  msgs.push({from:'selected entry',to:lane(root),label:root.qual_name,loc:location(root),symbolId:Number(root.id),status:'inferred'});
  const visit=(id:number,path:Set<number>,depth:number)=>{
    const caller=byId.get(id);if(!caller)return;
    const sites=sums.get(id)?.call_sites;if(!sites){omitted++;return;}
    for(const call of [...sites].sort((a,b)=>a.source_order-b.source_order)){
      const targets=call.target_symbols??[];
      const endpoints=targets.length?targets.map(id=>byId.get(Number(id))).filter((s):s is SymbolInfo=>!!s):[undefined];
      for(const target of endpoints){
        if(msgs.length>=maxMessages){omitted++;continue;}
        const recursive=target&&path.has(Number(target.id));
        const suffix=[call.guard!=='unconditional'?`[${call.guard}]`:'',call.dispatch==='candidate'?'[declared target; runtime unobserved]':'',recursive?'[recursion]':''].filter(Boolean).join(' ');
        msgs.push({from:lane(caller),to:target?lane(target):(call.receiver_type??call.receiver??'unresolved boundary'),label:`${target?.qual_name??call.callee}${suffix?' '+suffix:''}`,status:call.dispatch==='exact'?'confirmed':'inferred',loc:target?location(target):{file:lane(caller),byteStart:call.span.start,byteEnd:call.span.end},symbolId:target?Number(target.id):undefined,callSiteLoc:{file:lane(caller),byteStart:call.span.start,byteEnd:call.span.end}});
        if(target&&!recursive){if(depth<maxDepth)visit(Number(target.id),new Set([...path,Number(target.id)]),depth+1);else omitted++;}
      }
    }
  };
  visit(entryId,new Set([entryId]),0);
  return {kind:'sequence',engine:'g6',layout:'sequence-lanes',title:`Static call sequence — ${root.qual_name}`,tell:`Per-call occurrences with guards, repeated/self calls and candidate declaration boundaries. Not a runtime trace. Alternative paths are guard-labelled; dynamic scheduling and unsupported control flow are not reconstructed.${omitted?` ${omitted} frontier(s) omitted by limits or legacy coverage.`:''}`,source:'Pinned call-site records · definition and call-site anchors',sequenceCoverage:{omitted,limit:maxMessages},msgs};
}
