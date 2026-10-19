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
