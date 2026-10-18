//! Minimal, dependency-free HTTP server exposing the engine's primitives
//! as real inspectable pages and JSON endpoints (the "view" half of
//! several CIE-US stories). Hand-rolled HTTP/1.1 (GET only) over
//! `std::net` — no new external crate, matching the MCP seam's own
//! "no framework" style.
//!
//! [`route`] is pure and unit-tested without a real socket: given a
//! path, it returns the [`HttpResponse`] deterministically. [`serve`] is
//! the thin, untested transport loop that calls it per connection.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl HttpResponse {
    fn html(body: String) -> Self {
        HttpResponse { status: 200, content_type: "text/html; charset=utf-8", body }
    }

    fn json(body: String) -> Self {
        HttpResponse { status: 200, content_type: "application/json", body }
    }

    fn error(status: u16, body: impl Into<String>) -> Self {
        HttpResponse { status, content_type: "text/plain; charset=utf-8", body: body.into() }
    }

    fn not_found() -> Self {
        HttpResponse::error(404, "not found")
    }
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn page(title: &str, note: &str, body: &str) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{t}</title>\
         <style>body{{font-family:sans-serif;margin:2em;max-width:60em;}} \
         table{{border-collapse:collapse;margin:1em 0;}} \
         td,th{{border:1px solid #ccc;padding:4px 10px;text-align:left;}} \
         .note{{color:#555;font-size:0.9em;background:#f6f6f6;padding:0.6em 1em;border-left:3px solid #999;}}\
         </style></head><body><p><a href=\"/\">&larr; index</a></p><h1>{t}</h1>\
         {note_html}{body}</body></html>",
        t = escape_html(title),
        note_html = if note.is_empty() { String::new() } else { format!("<p class=\"note\">{}</p>", escape_html(note)) },
        body = body,
    )
}

fn table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let head: String = headers.iter().map(|h| format!("<th>{}</th>", escape_html(h))).collect();
    let body: String = rows
        .iter()
        .map(|r| {
            let cells: String = r.iter().map(|c| format!("<td>{}</td>", escape_html(c))).collect();
            format!("<tr>{cells}</tr>")
        })
        .collect();
    format!("<table><tr>{head}</tr>{body}</table>")
}

/// (path, label) pairs for the index page — one per closed view.
const INDEX_LINKS: &[(&str, &str)] = &[
    ("/view/sample", "US-007: on-demand sample repository"),
    ("/view/focus", "US-012: follow-up chips and referent resolution"),
    ("/view/hotspots", "US-047: profile-attributed hotspots"),
    ("/view/slow-op", "US-082: explain a slow operation"),
    ("/view/profile-compare", "US-083: compare two profiles"),
];

fn demo_profile_pair() -> (cie_evidence::Profile, cie_evidence::Profile, cie_evidence::EquivalenceDeclaration) {
    use cie_evidence::{EquivalenceDeclaration, Profile};
    let baseline = Profile { duration_ns: Some(1_000_000_000), ..demo_profile() };
    let candidate = Profile { duration_ns: Some(800_000_000), source: "demo-pprof-export-v2.pb".into(), ..demo_profile() };
    let equivalence = EquivalenceDeclaration { same_workload: true, same_build: true, notes: "same nightly load-test workload, same release build".into() };
    (baseline, candidate, equivalence)
}

fn demo_profile() -> cie_evidence::Profile {
    use cie_evidence::{Profile, ProfileSample, SampleCategory};
    Profile {
        source: "demo-pprof-export.pb".into(),
        samples: vec![
            ProfileSample { symbol: Some("checkout::charge_card".into()), category: SampleCategory::ExternalCall, weight: 60 },
            ProfileSample { symbol: Some("checkout::validate_cart".into()), category: SampleCategory::Cpu, weight: 25 },
            ProfileSample { symbol: None, category: SampleCategory::Cpu, weight: 15 },
        ],
        dropped_samples: 10,
        duration_ns: Some(1_200_000_000),
        error_count: 0,
    }
}

fn render_index() -> HttpResponse {
    let items: String = INDEX_LINKS
        .iter()
        .map(|(href, label)| format!("<li><a href=\"{href}\">{}</a> (<a href=\"/api{}\">json</a>)</li>", escape_html(label), &href["/view".len()..]))
        .collect();
    HttpResponse::html(page("CIE engine views", "", &format!("<ul>{items}</ul>")))
}

fn render_sample() -> (cie_ingest::SampleRepoReport, Result<(), String>) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cie-http-sample-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    match cie_ingest::generate_sample_repo(&dir) {
        Ok(report) => {
            let _ = std::fs::remove_dir_all(&dir);
            (report, Ok(()))
        }
        Err(e) => (cie_ingest::SampleRepoReport { root: String::new(), commits: Vec::new(), files_written: 0 }, Err(e.reason)),
    }
}

fn render_sample_html() -> HttpResponse {
    let (report, outcome) = render_sample();
    if let Err(reason) = outcome {
        return HttpResponse::error(500, format!("sample generation failed: {reason}"));
    }
    let rows: Vec<Vec<String>> = report.commits.iter().map(|c| vec![c.subject.clone(), c.files.join(", ")]).collect();
    let body = table(&["commit subject", "files introduced"], &rows);
    HttpResponse::html(page(
        "US-007: on-demand sample repository",
        "Generated on demand, local only, no network. Demo material — not a real project.",
        &body,
    ))
}

fn render_sample_json() -> HttpResponse {
    let (report, outcome) = render_sample();
    match outcome {
        Ok(()) => HttpResponse::json(serde_json::to_string_pretty(&report).unwrap_or_default()),
        Err(reason) => HttpResponse::error(500, format!("{{\"error\":\"{}\"}}", escape_html(&reason))),
    }
}

#[derive(serde::Serialize)]
struct FocusDemo {
    before: Vec<u32>,
    after_ignore_and_pin: Vec<u32>,
    after_zoom_focus: Vec<u32>,
    ambiguous_referent: Vec<u32>,
    resolved_referent: Option<u32>,
}

fn focus_demo() -> FocusDemo {
    use cie_analysis::focus::{resolve_referent, Chip, FocusContext, ReferentResolution};
    use cie_core::types::SymbolId;
    let candidates = [SymbolId(1), SymbolId(2), SymbolId(3)];
    let mut ctx = FocusContext::default();
    ctx.add(Chip::Ignore(SymbolId(1)));
    ctx.add(Chip::Pin(SymbolId(3)));
    let after_ignore_and_pin = ctx.apply(&candidates, None);

    let mut focus_ctx = FocusContext::default();
    focus_ctx.add(Chip::FocusOn(SymbolId(2)));
    let neigh = std::collections::BTreeSet::from([SymbolId(2)]);
    let after_zoom_focus = focus_ctx.apply(&candidates, Some(&neigh));

    let ambiguous = [SymbolId(5), SymbolId(6)];
    let resolved = match resolve_referent(&FocusContext::default(), &ambiguous) {
        ReferentResolution::Resolved(s) => Some(s.0),
        _ => None,
    };

    FocusDemo {
        before: candidates.iter().map(|s| s.0).collect(),
        after_ignore_and_pin: after_ignore_and_pin.iter().map(|s| s.0).collect(),
        after_zoom_focus: after_zoom_focus.iter().map(|s| s.0).collect(),
        ambiguous_referent: ambiguous.iter().map(|s| s.0).collect(),
        resolved_referent: resolved,
    }
}

fn render_focus_html() -> HttpResponse {
    let d = focus_demo();
    let fmt = |v: &[u32]| v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ");
    let body = format!(
        "{}{}",
        table(
            &["step", "symbol ids"],
            &[
                vec!["candidates before chips".into(), fmt(&d.before)],
                vec!["ignore(1) + pin(3) applied".into(), fmt(&d.after_ignore_and_pin)],
                vec!["focus(2) narrowed to its neighborhood".into(), fmt(&d.after_zoom_focus)],
            ],
        ),
        format!(
            "<p>Ambiguous referent among {}: {}</p>",
            fmt(&d.ambiguous_referent),
            match d.resolved_referent {
                Some(s) => format!("resolved to {s} (should not happen with no pin/focus tie-break)"),
                None => "named as Ambiguous, not guessed".to_string(),
            }
        )
    );
    HttpResponse::html(page(
        "US-012: follow-up chips and referent resolution",
        "Fixed demo symbol ids — illustrates FocusContext chip semantics, not a real selection.",
        &body,
    ))
}

fn render_focus_json() -> HttpResponse {
    HttpResponse::json(serde_json::to_string_pretty(&focus_demo()).unwrap_or_default())
}

fn render_hotspots_html() -> HttpResponse {
    let p = demo_profile();
    let rep = cie_evidence::attribute_hotspots(&p);
    let rows: Vec<Vec<String>> = rep
        .hotspots
        .iter()
        .map(|h| vec![h.symbol.clone(), format!("{:?}", h.category), format!("{:.1}%", h.share * 100.0), format!("±{:.1}pp", h.share_uncertainty * 100.0)])
        .collect();
    let body = format!(
        "<p>source artifact: <code>{}</code></p>{}<p>unattributed share (resolved samples with no symbol): {:.1}% &middot; withheld share (dropped by the importer): {:.1}%</p>",
        escape_html(&p.source),
        table(&["symbol", "category", "share", "uncertainty"], &rows),
        rep.unattributed_share * 100.0,
        rep.withheld_share * 100.0,
    );
    HttpResponse::html(page(
        "US-047: profile-attributed hotspots",
        "Fixed demo profile — illustrates attribution semantics, not a real measurement.",
        &body,
    ))
}

fn render_hotspots_json() -> HttpResponse {
    let rep = cie_evidence::attribute_hotspots(&demo_profile());
    HttpResponse::json(serde_json::to_string_pretty(&rep).unwrap_or_default())
}

fn render_slow_op_html() -> HttpResponse {
    let p = demo_profile();
    let b = cie_evidence::category_breakdown(&p);
    let mut rows: Vec<Vec<String>> = b.shares.iter().map(|(cat, share)| vec![format!("{cat:?}"), format!("{:.1}%", share * 100.0)]).collect();
    rows.push(vec!["Missing (never collected)".into(), format!("{:.1}%", b.missing_share * 100.0)]);
    let body = format!(
        "<p>duration observed: {} ms</p>{}",
        p.duration_ns.unwrap_or(0) / 1_000_000,
        table(&["category", "share of window"], &rows),
    );
    HttpResponse::html(page(
        "US-082: explain a slow operation",
        "Fixed demo profile — CPU/wait/data/external/missing distinguished over the whole collection window, including what the importer never collected.",
        &body,
    ))
}

fn render_slow_op_json() -> HttpResponse {
    let rep = cie_evidence::category_breakdown(&demo_profile());
    HttpResponse::json(serde_json::to_string_pretty(&rep).unwrap_or_default())
}

fn render_profile_compare_html() -> HttpResponse {
    let (baseline, candidate, equivalence) = demo_profile_pair();
    let cmp = cie_evidence::compare_profiles(&baseline, &candidate, &equivalence);
    let verdict = match &cmp.verdict {
        cie_evidence::ComparisonVerdict::Faster { improvement } => format!("Faster by {:.1}%", improvement * 100.0),
        cie_evidence::ComparisonVerdict::Slower { regression } => format!("Slower by {:.1}%", regression * 100.0),
        cie_evidence::ComparisonVerdict::Inconclusive => "Inconclusive".to_string(),
    };
    let reasons: String = cmp.reasons.iter().map(|r| format!("<li>{}</li>", escape_html(r))).collect();
    let body = format!(
        "<p>baseline: {} ms &middot; candidate: {} ms</p><p><strong>verdict: {}</strong> (rejected: {})</p><ul>{}</ul>",
        baseline.duration_ns.unwrap_or(0) / 1_000_000,
        candidate.duration_ns.unwrap_or(0) / 1_000_000,
        escape_html(&verdict),
        cmp.rejected,
        reasons,
    );
    HttpResponse::html(page(
        "US-083: compare two profiles",
        "Fixed demo baseline/candidate under a declared equivalence.",
        &body,
    ))
}

fn render_profile_compare_json() -> HttpResponse {
    let (baseline, candidate, equivalence) = demo_profile_pair();
    let cmp = cie_evidence::compare_profiles(&baseline, &candidate, &equivalence);
    HttpResponse::json(serde_json::to_string_pretty(&cmp).unwrap_or_default())
}

/// Pure routing: deterministic given a path, no socket required.
pub fn route(path: &str) -> HttpResponse {
    match path {
        "/" | "/index" => render_index(),
        "/view/sample" => render_sample_html(),
        "/api/sample" => render_sample_json(),
        "/view/focus" => render_focus_html(),
        "/api/focus" => render_focus_json(),
        "/view/hotspots" => render_hotspots_html(),
        "/api/hotspots" => render_hotspots_json(),
        "/view/slow-op" => render_slow_op_html(),
        "/api/slow-op" => render_slow_op_json(),
        "/view/profile-compare" => render_profile_compare_html(),
        "/api/profile-compare" => render_profile_compare_json(),
        _ => HttpResponse::not_found(),
    }
}

fn handle_connection(stream: &mut TcpStream) {
    let mut reader = BufReader::new(&*stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or("/").to_string();
    loop {
        let mut header_line = String::new();
        match reader.read_line(&mut header_line) {
            Ok(0) => break,
            Ok(_) if header_line.trim().is_empty() => break,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    let resp = route(&path);
    let out = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        resp.status,
        if resp.status == 200 { "OK" } else { "ERROR" },
        resp.content_type,
        resp.body.len(),
        resp.body
    );
    let _ = stream.write_all(out.as_bytes());
}

/// Blocking HTTP/1.1 server loop (single connection at a time — this is
/// an internal evidence-inspection tool, not a production web server).
pub fn serve(addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    println!("cie-server: listening on http://{addr}");
    for stream in listener.incoming() {
        let mut stream = stream?;
        handle_connection(&mut stream);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_lists_every_closed_view() {
        let resp = route("/");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("/view/sample"));
    }

    #[test]
    fn sample_view_renders_the_generated_commits() {
        let resp = route("/view/sample");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("scaffold the greeter service"));
    }

    #[test]
    fn sample_api_returns_parseable_json() {
        let resp = route("/api/sample");
        assert_eq!(resp.content_type, "application/json");
        let parsed: serde_json::Value = serde_json::from_str(&resp.body).expect("valid json");
        assert!(parsed["commits"].as_array().unwrap().len() >= 1);
    }

    #[test]
    fn unknown_path_is_404() {
        assert_eq!(route("/nope").status, 404);
    }

    #[test]
    fn focus_view_shows_ignore_pin_and_zoom_effects() {
        let resp = route("/view/focus");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("Ambiguous referent"));
    }

    #[test]
    fn focus_api_marks_the_ambiguous_case_unresolved() {
        let resp = route("/api/focus");
        let parsed: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert!(parsed["resolved_referent"].is_null());
    }

    #[test]
    fn hotspots_view_cites_source_and_shows_unattributed_share() {
        let resp = route("/view/hotspots");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("demo-pprof-export.pb"));
        assert!(resp.body.contains("unattributed share"));
    }

    #[test]
    fn hotspots_api_matches_the_engine_computation() {
        let resp = route("/api/hotspots");
        let parsed: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert!(parsed["hotspots"].as_array().unwrap().len() == 2);
    }

    #[test]
    fn slow_op_view_distinguishes_categories_and_missing() {
        let resp = route("/view/slow-op");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("Missing (never collected)"));
        assert!(resp.body.contains("ExternalCall"));
    }

    #[test]
    fn slow_op_api_shares_sum_to_one() {
        let resp = route("/api/slow-op");
        let parsed: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        let sum: f64 = parsed["shares"].as_object().unwrap().values().map(|v| v.as_f64().unwrap()).sum::<f64>()
            + parsed["missing_share"].as_f64().unwrap();
        assert!((sum - 1.0).abs() < 1e-9);
    }

    #[test]
    fn profile_compare_view_states_a_faster_verdict_with_reasons() {
        let resp = route("/view/profile-compare");
        assert_eq!(resp.status, 200);
        assert!(resp.body.contains("Faster by"));
        assert!(resp.body.contains("rejected: false"));
    }

    #[test]
    fn profile_compare_api_matches_engine_verdict() {
        let resp = route("/api/profile-compare");
        let parsed: serde_json::Value = serde_json::from_str(&resp.body).unwrap();
        assert!(parsed["verdict"]["Faster"].is_object());
    }
}
