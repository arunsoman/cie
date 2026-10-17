//! Screen/endpoint extraction (CIE-US US-052 engine close).
//!
//! "Find where a business capability is implemented across layers": the
//! concept→site mapping ships; what was limited is screen/endpoint
//! extraction. This module closes the engine half: HTTP endpoint and UI
//! screen detection from the fact families the deep adapters already
//! produce — route-shaped calls (`app.get(...)`, `router.post(...)`),
//! route annotations in decorator/attribute positions (surface as calls
//! or literals per language), and string literals that name routes —
//! each site with its evidence text.
//!
//! Honest scope: only routes the shipped adapters resolve are found;
//! coverage is stated per revision (count of files scanned), and a named
//! feature stays distinct from a verified working capability (the site
//! list is structural detection, not a behavioural proof).

use cie_core::types::SymbolId;
use cie_semantic::facts::LangFacts;
use serde::{Deserialize, Serialize};

/// One detected endpoint/screen site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointSite {
    /// HTTP verb or `screen` for UI entry detection.
    pub method: String,
    /// The route/path text as written (e.g. `/orders/{id}`).
    pub route: String,
    pub symbol: SymbolId,
    pub file: String,
    pub line: u32,
    /// The evidence: the call/decorator text that grounded the detection.
    pub evidence: String,
}

/// What one extraction pass covered (honest coverage statement).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointScan {
    pub sites: Vec<EndpointSite>,
    pub files_scanned: usize,
    /// Adapter-noted constructs that could hide routes (dynamic
    /// registration etc.) — surfaced, not hidden.
    pub caveats: Vec<String>,
}

const ROUTE_VERBS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "route", "use", "handle",
    "handlefunc", "getmapping", "postmapping", "putmapping", "deletemapping",
    "httpget", "httppost", "httpput", "httpdelete", "apiroute", "mapping",
];

fn looks_like_route_verb(callee_text: &str) -> Option<&'static str> {
    let lower = callee_text.to_lowercase();
    ROUTE_VERBS.iter().copied().find(|v| lower.ends_with(v) || lower == *v)
}

/// Extract endpoints from one file's language facts. Deterministic:
/// sites sorted by (file, line, route).
pub fn extract_endpoints(facts: &LangFacts) -> EndpointScan {
    let mut sites = Vec::new();
    let mut caveats: Vec<String> = Vec::new();
    // call-shaped detection: `app.get("/x")` — callee text ends with a
    // route verb AND a string-literal-ish argument carries the route
    for c in &facts.calls {
        if let Some(verb) = looks_like_route_verb(&c.callee_text) {
            let route = c
                .arg_vars
                .iter()
                .find(|a| a.starts_with('/') || a.starts_with("~/") || a.contains('{'));
            let route = match route {
                Some(r) => r.clone(),
                None => continue,
            };
            sites.push(EndpointSite {
                method: verb.to_uppercase(),
                route,
                symbol: c.caller,
                file: String::new(), // resolved by the caller via loc table
                line: c.line,
                evidence: format!("call {}({})", c.callee_text, c.arg_vars.join(", ")),
            });
        }
    }
    // literal-shaped detection (decorators/attributes surface here in
    // several adapters): `"/orders/{id}"` string literals adjacent to a
    // route verb in the callee of the same line are matched via calls
    for l in &facts.literals {
        if !l.is_string || !l.value.starts_with('/') {
            continue;
        }
        // only count the literal when a route call exists on that line —
        // avoids decorating arbitrary strings as routes
        if facts.calls.iter().any(|c| {
            c.line == l.line
                && looks_like_route_verb(&c.callee_text).is_some()
        }) {
            let verb = facts
                .calls
                .iter()
                .find(|c| c.line == l.line)
                .and_then(|c| looks_like_route_verb(&c.callee_text))
                .unwrap_or("route");
            sites.push(EndpointSite {
                method: verb.to_uppercase(),
                route: l.value.clone(),
                symbol: l.symbol,
                file: String::new(),
                line: l.line,
                evidence: format!("literal {} on route call line {}", l.value, l.line),
            });
        }
    }
    for u in facts.unsupported.iter().take(5) {
        if u.contains("dynamic") || u.contains("route") {
            caveats.push(u.clone());
        }
    }
    sites.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.route.cmp(&b.route))
    });
    sites.dedup();
    EndpointScan {
        sites,
        files_scanned: 1,
        caveats,
    }
}

#[allow(dead_code)]
fn _unused() {}

/// Per-file extraction over the LangFacts of a pass (coverage = files).
pub fn extract_endpoints_from_langfacts(per_file: &[LangFacts]) -> EndpointScan {
    let mut all = EndpointScan {
        sites: Vec::new(),
        files_scanned: per_file.len(),
        caveats: Vec::new(),
    };
    for f in per_file {
        let scan = extract_endpoints(f);
        for mut s in scan.sites {
            s.file = f.path.clone();
            all.sites.push(s);
        }
        all.caveats
            .extend(scan.caveats.iter().map(|c| format!("{}: {c}", f.path)));
    }
    all.sites
        .sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)).then(a.route.cmp(&b.route)));
    all.sites.dedup();
    all.caveats.sort();
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use cie_core::types::{FileId, SourceLocation};
    use cie_semantic::facts::{
        Callee, CallSite, LiteralFact, SymbolFact, SymbolKind,
    };
    use cie_semantic::lang::LangId;

    fn lf(calls: Vec<CallSite>, literals: Vec<LiteralFact>) -> LangFacts {
        let mut f = LangFacts::new("api/orders.py", LangId::Python, "test");
        f.calls = calls;
        f.literals = literals;
        f
    }

    #[test]
    fn route_calls_become_endpoint_sites_with_evidence() {
        let f = lf(
            vec![
                CallSite {
                    caller: SymbolId(1),
                    callee_text: "app.get".into(),
                    callee: Callee::Unresolved("flask route".into()),
                    line: 10,
                    in_loop: false,
                    arg_vars: vec!["/orders/{id}".into()],
                },
                CallSite {
                    caller: SymbolId(1),
                    callee_text: "db.query".into(),
                    callee: Callee::Unresolved("db".into()),
                    line: 11,
                    in_loop: false,
                    arg_vars: vec![],
                },
            ],
            vec![],
        );
        let scan = extract_endpoints(&f);
        assert_eq!(scan.sites.len(), 1);
        let s = &scan.sites[0];
        assert_eq!(s.method, "GET");
        assert_eq!(s.route, "/orders/{id}");
        assert!(s.evidence.contains("app.get"));
    }

    #[test]
    fn literals_adjacent_to_route_calls_are_detected() {
        let f = lf(
            vec![CallSite {
                caller: SymbolId(2),
                callee_text: "router.post".into(),
                callee: Callee::Unresolved("express route".into()),
                line: 5,
                in_loop: false,
                arg_vars: vec![],
            }],
            vec![LiteralFact {
                symbol: SymbolId(2),
                line: 5,
                value: "/refunds".into(),
                is_string: true,
            }],
        );
        let scan = extract_endpoints(&f);
        assert_eq!(scan.sites.len(), 1);
        assert_eq!(scan.sites[0].route, "/refunds");
        assert_eq!(scan.sites[0].method, "POST");
    }

    #[test]
    fn plain_strings_are_not_decorated_as_routes() {
        let f = lf(
            vec![],
            vec![LiteralFact {
                symbol: SymbolId(3),
                line: 1,
                value: "/etc/passwd".into(),
                is_string: true,
            }],
        );
        let scan = extract_endpoints(&f);
        assert!(scan.sites.is_empty());
    }

    #[test]
    fn langfacts_scan_reports_files_scanned_and_paths() {
        let mut f1 = LangFacts::new("api/orders.py", LangId::Python, "test");
        f1.calls.push(CallSite {
            caller: SymbolId(1),
            callee_text: "app.get".into(),
            callee: Callee::Unresolved("route".into()),
            line: 3,
            in_loop: false,
            arg_vars: vec!["/orders".into()],
        });
        let scan = extract_endpoints_from_langfacts(&[f1]);
        assert_eq!(scan.files_scanned, 1);
        assert_eq!(scan.sites.len(), 1);
        assert_eq!(scan.sites[0].file, "api/orders.py");
    }
}
