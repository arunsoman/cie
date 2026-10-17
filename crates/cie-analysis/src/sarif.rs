//! SARIF 2.1.0 export (CAP-2.11) for security tooling integration.

use cie_core::facts::SymbolStore;
use cie_core::findings::{Finding, FindingKind};
use serde_json::{json, Value};

pub fn findings_to_sarif(store: &SymbolStore, findings: &[Finding]) -> Value {
    let mut rules: Vec<Value> = vec![];
    let mut seen_rules: std::collections::BTreeMap<String, ()> = Default::default();
    let mut results: Vec<Value> = vec![];

    for f in findings {
        let rule_id = f.rule.clone();
        if !seen_rules.contains_key(&rule_id) {
            seen_rules.insert(rule_id.clone(), ());
            rules.push(json!({
                "id": rule_id,
                "name": f.kind.as_str(),
                "shortDescription": { "text": human_kind(&f.kind) },
                "defaultConfiguration": { "level": sarif_level(&f.severity) },
                "properties": {
                    "authority": f.authority.as_str(),
                    "tags": f.tags,
                }
            }));
        }
        let path = store
            .get(f.symbol)
            .map(|s| s.loc.path.clone())
            .unwrap_or_default();
        let start = f.evidence_lines.first().copied().unwrap_or(1);
        results.push(json!({
            "ruleId": rule_id,
            "level": sarif_level(&f.severity),
            "message": { "text": f.message },
            "locations": [{
                "physicalLocation": {
                    "artifactLocation": { "uri": path },
                    "region": { "startLine": start }
                }
            }],
            "partialFingerprints": { "findingId": f.id }
        }));
    }

    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "cie",
                    "informationUri": "https://github.com/arunsoman/cie",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": rules
                }
            },
            "results": results
        }]
    })
}

fn human_kind(k: &FindingKind) -> &'static str {
    match k {
        FindingKind::PrintOnlyAuthorization => "Print-only authorization",
        FindingKind::UnconditionalSuccess => "Unconditional success",
        FindingKind::IgnoredRejection => "Ignored rejection",
        FindingKind::SwallowedException => "Swallowed exception",
        FindingKind::IgnoredErrorValue => "Ignored error value",
        FindingKind::ExceptionToSuccess => "Exception converted to success",
        FindingKind::AuthzBypass => "Authorization bypass",
        FindingKind::HardcodedSecret => "Hardcoded secret",
        FindingKind::SqlInjection => "SQL injection",
        FindingKind::CommandInjection => "Command injection",
        FindingKind::CodeInjection => "Code injection",
        FindingKind::TaintFlow => "Taint flow",
        FindingKind::NPlusOneQuery => "N+1 query",
        FindingKind::DeadCode => "Dead code",
        FindingKind::OrphanCode => "Orphaned code",
        FindingKind::LayeringViolation => "Layering violation",
        FindingKind::CoverageGap => "Coverage gap",
        FindingKind::UnresolvedExternal => "Unresolved external",
    }
}

fn sarif_level(s: &cie_core::findings::Severity) -> &'static str {
    match s {
        cie_core::findings::Severity::Critical | cie_core::findings::Severity::High => "error",
        cie_core::findings::Severity::Medium => "warning",
        cie_core::findings::Severity::Low | cie_core::findings::Severity::Info => "note",
    }
}
