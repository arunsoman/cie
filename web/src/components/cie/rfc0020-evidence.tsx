"use client";
import type { CieReport, CommitMeta, Contract, DriftClaim } from "@/lib/cie-types";
import type { TechStack } from "@/lib/stack-types";

type OpenSite = (file: string, line: number, label: string) => void;
export function CommitProvenance({ commits }: { commits?: CommitMeta[] | null }) {
  if (!commits?.length) return null;
  return <details className="text-xs text-stone-600"><summary>Introduced — cited Git provenance</summary>
    <p>Git blame at capture time, not proof of semantic introduction or a complete ancestry. Capture is bounded to 64 cited lines / five seconds.</p>
    <ul>{commits.map(c => <li key={c.id}><code>{c.id.slice(0,12)}</code> · {c.author} · {new Date(c.time*1000).toISOString()} · {c.subject}</li>)}</ul></details>;
}
export function ObligationEvidence({ contract, report, onOpen }: { contract: Contract; report: CieReport; onOpen?: (id: number) => void }) {
  return <div className="space-y-1 text-xs text-stone-600" data-testid="obligation-evidence">
    <CommitProvenance commits={contract.introduced}/>
    <p>Tests reaching this obligation: static call reachability within six edges; not executed coverage or proof that an assertion checks this rule.</p>
    {contract.verified_by === undefined ? <p>Test bindings not recorded in this older artifact.</p> : !contract.verified_by.length ? <p>No test reaches this, as far as CIE can see in the recorded graph and depth bound.</p> : <ul>{contract.verified_by.map(id => {
      const symbol=report.symbols.find(s=>s.id===id);
      return <li key={id}><button type="button" className="underline" disabled={!symbol || !onOpen} onClick={()=>onOpen?.(id)}>{symbol?.qual_name ?? `Symbol ${id} no longer available in this artifact`}</button></li>;
    })}</ul>}
  </div>;
}
export function StateTransitionEvidence({report,onOpen}: { report: CieReport; onOpen?: (id:number)=>void }) {
  const model=report.state_model;if(!model?.transitions.length)return null;
  return <section className="rounded-lg border border-stone-200 bg-white p-4 space-y-2" aria-label="Evidenced payment transitions">
    <h3 className="font-semibold">Evidenced payment/recovery transitions</h3><p className="text-xs text-stone-600">{model.domain_note}</p>
    <p className="text-xs">Fact labels refer to extracted static evidence. They do not establish successful runtime transitions.</p>
    <ul>{model.transitions.map((t,i)=><li key={`${t.from}:${t.event}:${i}`} className="py-2 border-t text-sm"><strong>{t.from} → {t.to}</strong> · {t.event} · {t.display_mode}
      <p className="text-xs">{t.provenance}</p><div>{t.evidence.map(p=>{const symbol=report.symbols.find(s=>s.id===p.symbol);return <button key={`${p.symbol}:${p.effect}`} type="button" className="underline text-xs mr-3" disabled={!symbol||!onOpen} onClick={()=>onOpen?.(p.symbol)}>{symbol?.qual_name ?? `Unmapped symbol ${p.symbol}`} · {p.effect}</button>;})}</div></li>)}</ul>
  </section>;
}
export function CrossServiceEvidence({stack,onOpen,scope}: { stack: TechStack | null; onOpen: OpenSite; scope?: string | null }) {
  if(!stack || stack.crossService===undefined)return null;
  const name=scope?stack.apps.find(a=>a.dir===scope)?.name:null;
  const edges=(stack.crossService||[]).filter(e=>!scope||e.fromApp===name||e.toApp===name||e.fromApp===scope||e.toApp===scope);
  const site=(e:string,label:string)=>{const m=e.match(/^(.*):(\d+)$/);return m?<button type="button" className="underline mr-3" onClick={()=>onOpen(m[1],Number(m[2]),label)}>{e}</button>:<span>{e}</span>;};
  return <section className="border border-stone-200 rounded-lg m-3 p-3 text-xs" aria-label="Cross-service calls"><h3 className="font-semibold">Cross-service calls — static declarations</h3>
    <p>Feign/service routing matches as written; not observed invocations. {scope?`Calls touching ${scope}.`:"Repository-wide scope."}</p>
    {!edges.length?<p>No uniquely resolved cross-service endpoint in this recorded scope; this does not establish absence of outbound calls.</p>:<ul>{edges.map((e,i)=><li key={i} className="border-t py-2"><strong>{e.fromApp} → {e.toApp}</strong> · {e.via} · {e.matchedPath}<p>{e.matchBasis}</p>{site(e.fromEvidence,"Outbound declaration")}{site(e.toEvidence,"Destination route")}</li>)}</ul>}
    <details><summary>Resolution gaps and stack notes</summary><ul>{stack.notes.map((n,i)=><li key={i}>{n}</li>)}</ul></details>
  </section>;
}
export function DriftProvenance({claims}: { claims?: DriftClaim[] }) {
  const cited=claims?.filter(c=>c.introduced?.length);if(!cited?.length)return null;
  return <section className="rounded-lg border p-4 space-y-2" aria-label="Document claim provenance"><h3 className="font-semibold">Document claims with cited Git provenance</h3>{cited.map(c=><div key={`${c.doc}:${c.id}`}><p className="text-sm">{c.doc}:{c.loc.line} · {c.text}</p><CommitProvenance commits={c.introduced}/></div>)}</section>;
}
export function FaultEvidence({report}: {report:CieReport}) {
 const fault=report.fault_analysis;if(!fault)return null;
 if(fault.revision!==report.rev)return <p>Fault artifact belongs to another revision; results withheld.</p>;
 return <section className="rounded-lg border p-4 space-y-2" aria-label="Static-model fault hypotheses"><h3 className="font-semibold">Static-model fault hypotheses — {fault.status}</h3><p>Execution performed: false. Runtime observed: false.</p>{fault.unknown_reason&&<p>Unknown: {fault.unknown_reason}</p>}<p>{fault.property}</p><ul>{fault.notes.map((n,i)=><li key={i} className="text-xs">{n}</li>)}</ul><p>{fault.experiments.length} simulated hypotheses; no conclusion about live recovery.</p><details><summary>Model experiment evidence</summary><ul>{fault.experiments.map(e=><li key={e.content_hash}>Modeled property violated: {String(e.property_violated)} · {JSON.stringify(e.outcome)}<p>{e.provenance.join("; ")}</p></li>)}</ul></details></section>;
}

export function FindingProvenance({report}: {report:CieReport | null}) {
 const findings=report?.findings.filter(f=>f.introduced?.length)||[];
 if(!findings.length)return null;
 return <section className="rounded-lg border p-4 space-y-2" aria-label="Finding commit provenance"><h3 className="font-semibold">Finding commit provenance</h3><p className="text-xs">Cited Git metadata does not change a finding’s authority or measured quality metrics.</p>{findings.slice(0,100).map(f=><div key={f.id}><p className="text-sm">{f.symbol_name} · {f.message}</p><CommitProvenance commits={f.introduced}/></div>)}{findings.length>100&&<p>{findings.length-100} further findings remain in the exported report.</p>}</section>;
}
