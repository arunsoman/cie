use cie_ingest::{run_ingest,materialize,IngestOptions};
use cie_core::types::{now_ns};
use std::path::PathBuf;
fn fixture()->PathBuf {
 let root=std::env::temp_dir().join(format!("cie-behaviour-{}-{}",std::process::id(),now_ns()));std::fs::create_dir_all(&root).unwrap();
 std::fs::write(root.join("Controller.java"),"class Controller { private Service service; public void send(String req) { service.submit(req); service.submit(req); } }").unwrap();
 std::fs::write(root.join("Service.java"),"class Service { public void submit(String req) { System.out.println(req); } }").unwrap();root
}
#[test]
fn declaration_targets_preserve_occurrences_dispatch_and_revision_identity(){
 let root=fixture();let out=run_ingest(&root,&IngestOptions::default()).unwrap();
 let target=out.snapshot.symbols.iter().find(|s|s.qual_name=="Service.submit").unwrap().id;
 let sender=out.snapshot.symbols.iter().find(|s|s.qual_name=="Controller.send").unwrap().id;
 let calls=&out.snapshot.summaries.iter().find(|s|s.sym==sender).unwrap().call_sites;
 assert_eq!(calls.len(),2);assert_ne!(calls[0].span,calls[1].span);
 assert!(calls.iter().all(|c|c.target_symbols==vec![target]&&c.dispatch=="candidate"&&c.observation=="unobserved"));
 // Adding an earlier file changes revision-local numeric IDs. Cached links
 // must still navigate to Service.submit instead of an unrelated survivor.
 std::fs::write(root.join("AAA.java"),"class AAA { public void unrelated() {} }").unwrap();
 let next=materialize(run_ingest(&root,&IngestOptions::default()).unwrap()).unwrap();
 let sender=next.snapshot.symbols.iter().find(|s|s.qual_name=="Controller.send").unwrap().id;
 for call in &next.snapshot.summaries.iter().find(|s|s.sym==sender).unwrap().call_sites {
  assert_eq!(call.target_symbols.len(),1);assert_eq!(next.snapshot.symbols[call.target_symbols[0].0 as usize].qual_name,"Service.submit");
 }
 std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn changing_declared_receiver_invalidates_an_unchanged_method_body(){
 let root=fixture();run_ingest(&root,&IngestOptions::default()).unwrap();
 std::fs::write(root.join("Other.java"),"class Other { public void submit(String req) {} }").unwrap();
 std::fs::write(root.join("Controller.java"),"class Controller { private Other service; public void send(String req) { service.submit(req); service.submit(req); } }").unwrap();
 let out=run_ingest(&root,&IngestOptions::default()).unwrap();
 let sender=out.snapshot.symbols.iter().find(|s|s.qual_name=="Controller.send").unwrap().id;
 for call in &out.snapshot.summaries.iter().find(|s|s.sym==sender).unwrap().call_sites {
  assert_eq!(call.target_symbols.len(),1);assert_eq!(out.snapshot.symbols[call.target_symbols[0].0 as usize].qual_name,"Other.submit");
 }
 std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn producer_upgrade_recomputes_summaries_even_without_source_changes(){
 use cie_incremental::snapshot::SnapshotStoreExt;
 let root=fixture();let first=run_ingest(&root,&IngestOptions::default()).unwrap();
 let store=cie_store::Store::open(&root).unwrap();let mut old=store.load_snapshot(first.rev).unwrap().unwrap();
 old.concepts_tier=Some(cie_ingest::CONCEPTS_TIER_VERSION-1);
 for summary in &mut old.summaries{for call in &mut summary.call_sites{call.target_symbols.clear();}}
 store.save_snapshot(&old).unwrap();
 let refreshed=materialize(run_ingest(&root,&IngestOptions::default()).unwrap()).unwrap();
 assert!(refreshed.rev>first.rev);
 let sender=refreshed.snapshot.symbols.iter().find(|s|s.qual_name=="Controller.send").unwrap().id;
 assert!(refreshed.snapshot.summaries.iter().find(|s|s.sym==sender).unwrap().call_sites.iter().all(|c|!c.target_symbols.is_empty()));
 std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn repository_spelling_does_not_override_in_memory_declaration_evidence(){
 let root=fixture();std::fs::write(root.join("MemoryRepository.java"),"class MemoryRepository { public Object find(String value) { return value; } }").unwrap();
 std::fs::write(root.join("Controller.java"),"class Controller { private MemoryRepository memory; public Object send(String req) { return memory.find(req); } }").unwrap();
 let out=run_ingest(&root,&IngestOptions::default()).unwrap();let sender=out.snapshot.symbols.iter().find(|s|s.qual_name=="Controller.send").unwrap().id;
 let summary=out.snapshot.summaries.iter().find(|s|s.sym==sender).unwrap();
 assert!(!summary.effects.contains(&cie_summary::EffectKind::DbRead));assert!(!summary.effects.contains(&cie_summary::EffectKind::DbWrite));
 std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn api_entry_identity_survives_member_sorting_and_storage(){
 use cie_incremental::snapshot::SnapshotStoreExt;
 let root=fixture();
 std::fs::write(root.join("pom.xml"),"<project><dependencies><dependency><groupId>org.springframework.boot</groupId><artifactId>spring-boot-starter-web</artifactId></dependency></dependencies></project>").unwrap();
 std::fs::write(root.join("ZController.java"),"@RestController\nclass ZController {\n private Service service;\n @PostMapping(\"/send\")\n public void send(String req) { service.submit(req); }\n}").unwrap();
 let out=run_ingest(&root,&IngestOptions::default()).unwrap();
 let store=cie_store::Store::open(&root).unwrap();let saved=store.load_snapshot(out.rev).unwrap().unwrap();
 let node=saved.concepts.nodes.iter().find(|n|n.provenance=="api:declared-route-v1").expect("declared route");
 let entry=node.entry_symbol.expect("explicit entry identity");
 assert_eq!(saved.symbols[entry.0 as usize].qual_name,"ZController.send");
 assert!(node.symbols.iter().any(|id|*id<entry),"downstream symbol sorts before entry");
 std::fs::remove_dir_all(root).unwrap();
}
