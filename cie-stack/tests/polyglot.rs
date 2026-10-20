//! the tracked polyglot fixture (../samples/polyglot): one repo, several
//! apps, each a complete service in its own language + architecture style.
use cie_stack::stack_report;
use std::path::Path;

#[test]
fn routes_across_five_service_dirs() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../samples/polyglot");
    if !dir.exists() {
        eprintln!("fixture missing — skipped");
        return;
    }
    let r = stack_report(&dir);
    let services: std::collections::BTreeSet<&str> = r.routes.iter().map(|x| x.service.as_str()).collect();
    assert!(services.contains("berth-booking"), "java spring routes");
    assert!(services.contains("billing"), "kotlin spring routes");
    assert!(services.contains("tide-alerts"), "rust axum routes");
    assert!(services.contains("search"), "ts express routes");
    assert!(services.contains("webhooks"), "py flask routes");
    assert_eq!(r.routes.len(), 12, "2+3+1+2+2");
    assert_eq!(r.mains.len(), 5, "one boot per app");
    assert_eq!(r.flows.len(), 5, "one controller entry per app");
    assert_eq!(r.apps.len(), 5);
    // the JVM family is ONE family across BOTH spring apps — no duplicates
    // from the composite merge (this was a real bug: each merge step carried
    // the previous apps' jvm and re-extended it)
    let jvm = r.jvm.as_ref().expect("spring apps ⇒ a jvm family");
    let svc: std::collections::BTreeSet<&str> = jvm.services.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(svc.len(), jvm.services.len(), "no duplicate services");
    assert!(svc.contains("BookingService") && svc.contains("BillingService"), "both apps' services present");
    let repos: std::collections::BTreeSet<&str> = jvm.repositories.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(repos.len(), jvm.repositories.len(), "no duplicate repositories");
    assert!(repos.contains("InvoiceRepository"), "the DDD port interface is a repository");
}

#[test]
fn flows_carry_branches_and_database_nodes() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../samples/polyglot");
    if !dir.exists() {
        eprintln!("fixture missing — skipped");
        return;
    }
    let r = stack_report(&dir);
    let text: String = r.flows.iter().map(|c| serde_json::to_string(c).unwrap_or_default()).collect();
    assert!(text.contains("\"decision\""), "branch decisions exist");
    assert!(text.contains("\"database\""), "db nodes exist");
}

#[test]
fn feign_outbound_resolves_to_the_other_services_route() {
 let dir=Path::new(env!("CARGO_MANIFEST_DIR")).join("../samples/polyglot");
 let report=stack_report(&dir);
 let edge=report.cross_service.iter().find(|e|e.from_app=="berth-booking"&&e.to_app=="billing").expect("Feign declaration joins billing route");
 assert_eq!(edge.matched_path,"/invoices/{id}");
 assert!(edge.from_evidence.contains("BillingClient.java:"));
 assert!(edge.to_evidence.contains("BillingController.kt:"));
 let ob=report.jvm.as_ref().unwrap().outbound.iter().find(|o|o.cls=="BillingClient").unwrap();
 assert_eq!(ob.target.as_deref(),Some("billing"));assert!(ob.line>0);
 let mut broken=report.clone();broken.jvm.as_mut().unwrap().outbound.iter_mut().for_each(|o|o.target=Some("external.invalid".into()));
 cie_stack::cross_service::resolve_cross_service(&mut broken);assert!(broken.cross_service.is_empty());assert!(broken.notes.iter().any(|n|n.contains("no unique")));
 let mut ambiguous=report.clone();let route=ambiguous.routes.iter().find(|r|r.service=="billing"&&r.path=="/invoices/{id}").unwrap().clone();ambiguous.routes.push(route);
 cie_stack::cross_service::resolve_cross_service(&mut ambiguous);assert!(ambiguous.cross_service.is_empty());assert!(ambiguous.notes.iter().any(|n|n.contains("ambiguous")));
 let mut legacy=serde_json::to_value(report).unwrap();legacy.as_object_mut().unwrap().remove("crossService");
 legacy.as_object_mut().unwrap().remove("externalDocker");
 if let Some(services)=legacy.pointer_mut("/docker/compose/services").and_then(|v|v.as_array_mut()){for service in services {let s=service.as_object_mut().unwrap();s.remove("line");s.remove("build");s.remove("dockerfile");}}
 if let Some(apps)=legacy.get_mut("apps").and_then(|v|v.as_array_mut()){for app in apps {app.as_object_mut().unwrap().remove("docker");}}
 let old:cie_stack::types::StackReport=serde_json::from_value(legacy).unwrap();assert!(old.cross_service.is_empty());
}
