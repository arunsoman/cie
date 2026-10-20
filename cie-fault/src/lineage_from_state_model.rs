//! RFC-0020 static-model hypothesis planning. No workload is executed.
use crate::ldfi::*;
use cie_contract::obligations::{evidenced_model, StateModel};
use serde::{Serialize,Deserialize};
use std::collections::{BTreeSet,VecDeque};

pub fn lineage_from_evidenced(model:&StateModel)->LineageGraph {
    let model=evidenced_model(model);let mut g=LineageGraph::empty();
    for t in &model.edges {
        let p=t.provenance.iter().map(|p|format!("{}:{}",p.symbol.0,p.effect)).collect::<Vec<_>>().join(",");
        g.add(&format!("static:{}:{}:{} [{p}]",t.from.as_str(),t.event.as_str(),t.to.as_str()),LineageKind::StateUpdate,t.from.as_str(),t.to.as_str());
    }
    g
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct FaultAnalysis {
    pub revision:u64,
    pub status:String,
    pub unknown_reason:Option<String>,
    pub property:Option<String>,
    pub lineage:LineageGraph,
    pub experiments:Vec<ExperimentResult>,
    pub execution_performed:bool,
    pub runtime_observed:bool,
    pub notes:Vec<String>,
}
/// Adapter for the planner's legacy success marker. Its reached states come
/// from the supplied graph; `balance_update` is ONLY an internal completion
/// marker required by LdfiPlanner, never an extracted business event.
struct StaticModelInjector { start:String, goal:String }
impl RuntimeFaultInjector for StaticModelInjector {
 fn run(&mut self,lineage:&LineageGraph,faults:&[Fault])->SimRun {
    if faults.iter().any(|f|matches!(f,Fault::CrashScheduler)){return SimRun{reached:vec![],anomalies:vec!["synthetic scheduler interruption".into()]};}
    let mut queue=VecDeque::from([self.start.clone()]);let mut seen=BTreeSet::new();let mut anomalies=vec![];
    while let Some(state)=queue.pop_front(){
        if !seen.insert(state.clone()){continue;}
        for edge in lineage.edges.iter().filter(|e|e.from==state){
            if faults.iter().any(|f|matches!(f,Fault::Drop(e) if e.id==edge.id)){continue;}
            if faults.iter().any(|f|matches!(f,Fault::Duplicate(e) if e.id==edge.id)){anomalies.push(format!("simulated duplicate of {}",edge.id));}
            queue.push_back(edge.to.clone());
        }
    }
    let complete=seen.contains(&self.goal);let mut reached:Vec<_>=seen.into_iter().collect();
    if complete {reached.push("balance_update".into());}
    SimRun{reached,anomalies}
 }
}
pub fn analyze_model(revision:u64,model:Option<&StateModel>,limits:RoundLimits)->FaultAnalysis {
    let mut report=FaultAnalysis{revision,status:"Unknown".into(),unknown_reason:None,property:None,lineage:LineageGraph::empty(),experiments:vec![],execution_performed:false,runtime_observed:false,notes:vec!["Static payment-template hypothesis simulation only; no live faults, captured execution, provider effects or recovery guarantee.".into(),"Property selection: first evidenced transition by (from, event). Other obligations are not checked by this run.".into(),"The planner's balance_update success marker is internal bookkeeping, not an extracted event. Experiment at_ns values are logical sequence counters, not observed wall-clock times.".into()]};
    let Some(model)=model else {report.unknown_reason=Some("NoEvidencedStateModel: no evidenced state model for this revision".into());return report};
    let model=evidenced_model(model);report.lineage=lineage_from_evidenced(&model);
    let Some(t)=model.edges.iter().min_by_key(|t|(t.from.as_str(),t.event.as_str())) else {report.unknown_reason=Some("NoLineage".into());return report};
    if limits.max_faults==0||limits.max_faults>2 {report.unknown_reason=Some("UnsupportedFaultBound: use one or two faults".into());return report;}
    let property=Property{expr:format!("In the static model, {} can reach {} via {} without duplicate modeled effects",t.from.as_str(),t.to.as_str(),t.event.as_str())};
    report.property=Some(property.expr.clone());
    let mut planner=LdfiPlanner::new(report.lineage.clone(),StaticModelInjector{start:t.from.as_str().into(),goal:t.to.as_str().into()});
    match planner.run(&property,limits) {
        Ok(round)=>{
            // The template contains state edges, not an evidenced scheduler.
            // Do not display the planner's unconditional scheduler hypothesis.
            let timed_out=round.results.iter().any(|e|matches!(e.outcome,ExperimentOutcome::Unknown(_)));
            report.experiments=round.results.into_iter().filter(|r|!r.faults.iter().any(|f|matches!(f,Fault::CrashScheduler))).collect();
            if timed_out {report.unknown_reason=Some("TimedOut".into());}
            else {report.status="Simulated".into();}
        }
        Err(e)=>report.unknown_reason=Some(format!("{e:?}")),
    }
    report
}
#[cfg(test)]
mod tests {
 use super::*;
 use cie_contract::obligations::*;
 use cie_core::SymbolId;
 fn model()->StateModel { StateModel{created_ns:0,edges:vec![Transition{from:State::Pending,event:Event::Authorize,to:State::Authorized,created_ns:0,provenance:vec![EffectProvenance{symbol:SymbolId(1),effect:"print".into(),external_write:false,reads:vec![],writes:vec![]}]}]} }
 #[test]
 fn real_model_edges_drive_simulation_and_missing_evidence_is_unknown(){
    let m=model();let g=lineage_from_evidenced(&m);assert_eq!(g.edges.len(),1);assert!(g.edges[0].id.contains("1:print"));
    let r=analyze_model(3,Some(&m),RoundLimits::default());assert_eq!(r.status,"Simulated");assert!(!r.execution_performed);assert!(!r.runtime_observed);assert_eq!(r.revision,3);assert!(r.experiments.iter().any(|e|e.property_violated));
    assert!(r.experiments.iter().all(|e|!e.faults.iter().any(|f|matches!(f,Fault::CrashScheduler))));
    let mut placeholder=m.clone();placeholder.edges[0].provenance[0].symbol=SymbolId(u32::MAX);
    assert!(lineage_from_evidenced(&placeholder).edges.is_empty());
    assert_eq!(analyze_model(3,Some(&placeholder),RoundLimits::default()).unknown_reason.as_deref(),Some("NoLineage"));
    assert!(analyze_model(3,None,RoundLimits::default()).unknown_reason.unwrap().contains("NoEvidencedStateModel"));
    assert_eq!(analyze_model(3,Some(&m),RoundLimits{max_faults:2,budget_ns:0}).status,"Unknown");
 }
}
