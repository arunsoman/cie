//! Runtime evidence feeds (CIE-US US-032 / US-039 / US-064 engine close):
//! * [`parse_trace`] — raw stack-trace parsing (Python traceback, Rust
//!   panic+backtrace, Java, JavaScript frames) into typed [`TraceFrames`];
//! * [`ExceptionsInbox`] — counted, fingerprint-deduped exception entries
//!   with dismiss/investigate transitions (repeats collapse, never lost);
//! * [`suspects_from_trace`] — maps parsed frames onto a
//!   [`LineageGraph`]/symbol-name universe so the LDFI planner can rank
//!   fault candidates from real runtime evidence instead of nothing.
//!
//! Honesty rules: a parse failure is a named error, never a guessed frame;
//! the inbox never fabricates counts; suspects carry the evidence trail
//! (which frames matched what).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ldfi::LineageGraph;

/// One parsed stack frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceFrame {
    /// Function/method name as written in the trace (unresolved text).
    pub symbol_text: String,
    /// Source file if the trace carried one.
    pub file: Option<String>,
    /// Line if the trace carried one.
    pub line: Option<u32>,
}

/// A parsed exception/panic with its frame stack (outermost first).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceFrames {
    /// `python` | `rust` | `java` | `javascript` | `unknown`
    pub lang: &'static str,
    /// Exception/panic type text (e.g. `ValueError`, `panicked at`).
    pub exception: String,
    /// Message text after the exception type (may be empty).
    pub message: String,
    pub frames: Vec<TraceFrame>,
}

impl TraceFrames {
    /// Fingerprint: exception + innermost 3 symbols (innermost = end of
    /// stack). Repeats collapse onto the same fingerprint (US-064).
    pub fn fingerprint(&self) -> String {
        let tail: Vec<String> = self
            .frames
            .iter()
            .rev()
            .take(3)
            .map(|f| f.symbol_text.clone())
            .collect();
        format!("{}|{}", self.exception, tail.join(">"))
    }
}

/// A parse error is honest: which language family was attempted and why
/// the text does not match any known trace shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceParseError {
    pub reason: String,
}

/// Parse a pasted stack trace (US-032). Multi-format, deterministic.
pub fn parse_trace(text: &str) -> Result<TraceFrames, TraceParseError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(TraceParseError {
            reason: "empty trace".into(),
        });
    }
    if let Some(t) = parse_python(trimmed) {
        return Ok(t);
    }
    if let Some(t) = parse_rust(trimmed) {
        return Ok(t);
    }
    if let Some(t) = parse_java(trimmed) {
        return Ok(t);
    }
    if let Some(t) = parse_javascript(trimmed) {
        return Ok(t);
    }
    Err(TraceParseError {
        reason: "no known trace shape matched (python/rust/java/javascript)".into(),
    })
}

// Traceback (most recent call last):
//   File "app/pay.py", line 42, in charge
//     result = gateway.submit(order)
// ValueError: insufficient funds
fn parse_python(t: &str) -> Option<TraceFrames> {
    if !t.contains("Traceback (most recent call last)") {
        return None;
    }
    let mut frames = Vec::new();
    let mut exception = String::new();
    let mut message = String::new();
    for l in t.lines() {
        let l = l.trim_end();
        if let Some(rest) = l.strip_prefix("  File \"") {
            // File "path", line N, in symbol
            let mut it = rest.split('"');
            let file = it.next().unwrap_or("").to_string();
            let after = it.next().unwrap_or("");
            let line = after
                .split(',')
                .find_map(|p| p.trim().strip_prefix("line "))
                .and_then(|n| n.trim().parse().ok());
            let symbol = after
                .split(',')
                .find_map(|p| p.trim().strip_prefix("in "))
                .unwrap_or("")
                .trim()
                .to_string();
            frames.push(TraceFrame {
                symbol_text: symbol,
                file: Some(file),
                line,
            });
        } else if let Some((kind, msg)) = l.split_once(": ") {
            if !kind.contains(' ') || kind.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
            {
                // `ValueError: msg` — exception header
                exception = kind.to_string();
                message = msg.to_string();
            }
        }
    }
    if exception.is_empty() {
        // SyntaxError-style traces end with `SomeError: msg` too; if we saw
        // frames but no header, still an honest parse with unknown header.
        if frames.is_empty() {
            return None;
        }
        exception = "Unknown".into();
    }
    Some(TraceFrames {
        lang: "python",
        exception,
        message,
        frames,
    })
}

// thread 'main' panicked at src/pay.rs:42:9:
// insufficient funds
// note: run with `RUST_BACKTRACE=1` ...
//    0: pay::gateway::submit
//             at ./src/gateway.rs:88:13
fn parse_rust(t: &str) -> Option<TraceFrames> {
    let panic_line = t.lines().find(|l| l.contains("panicked at"))?;
    let exception = "panic".to_string();
    // message: the line(s) after the panicked-at line up to `note:` or a
    // frame line
    let mut message = String::new();
    let mut frames: Vec<TraceFrame> = Vec::new();
    let mut after_panic = false;
    for l in t.lines() {
        if l.contains("panicked at") {
            after_panic = true;
            continue;
        }
        if !after_panic {
            continue;
        }
        if l.trim_start().starts_with("note:") {
            break;
        }
        let trimmed = l.trim();
        if trimmed.is_empty() {
            continue;
        }
        // frame line: `N: path::to::fn` or `at file:line:col`
        if let Some(rest) = trimmed.strip_prefix("at ") {
            if let Some(f) = frames.last_mut() {
                let mut it = rest.split(':');
                f.file = it.next().map(String::from);
                f.line = it.next().and_then(|n| n.parse().ok());
            }
            continue;
        }
        if let Some((num, sym)) = trimmed.split_once(": ") {
            if num.trim().parse::<u32>().is_ok() {
                frames.push(TraceFrame {
                    symbol_text: sym.trim().to_string(),
                    file: None,
                    line: None,
                });
                continue;
            }
        }
        if message.is_empty() && !trimmed.starts_with("stack backtrace") {
            message = trimmed.to_string();
        }
    }
    let _ = panic_line;
    Some(TraceFrames {
        lang: "rust",
        exception,
        message,
        frames,
    })
}

// javax.naming.NameNotFoundException: something
//     at com.app.Pay.charge(Pay.java:42)
//     at com.app.Main.run(Main.java:10)
fn parse_java(t: &str) -> Option<TraceFrames> {
    let mut lines = t.lines();
    let header = lines.next()?.trim();
    let (exception, message) = header.split_once(": ")?;
    // Java exception types are almost always dotted FQNs; a bare `Error`
    // (JavaScript) must fall through to the JS parser. Deterministic and
    // documented: java matches when the type carries a dot.
    if !exception.contains('.') {
        return None;
    }
    let mut frames = Vec::new();
    for l in lines {
        let l = l.trim();
        let Some(rest) = l.strip_prefix("at ") else {
            continue;
        };
        // `com.app.Pay.charge(Pay.java:42)` — symbol before '('
        let (sym, loc) = rest.split_once('(').unwrap_or((rest, ""));
        let (file, line) = if loc.is_empty() {
            (None, None)
        } else {
            let loc = loc.trim_end_matches(')');
            match loc.rsplit_once(':') {
                Some((f, n)) => (
                    Some(f.to_string()),
                    n.parse().ok(),
                ),
                None => (Some(loc.to_string()), None),
            }
        };
        frames.push(TraceFrame {
            symbol_text: sym.trim().to_string(),
            file,
            line,
        });
    }
    if frames.is_empty() {
        return None;
    }
    Some(TraceFrames {
        lang: "java",
        exception: exception.to_string(),
        message: message.to_string(),
        frames,
    })
}

// Error: insufficient funds
//     at Gateway.submit (/app/pay.js:42:15)
//     at Main.run (/app/main.js:10:5)
fn parse_javascript(t: &str) -> Option<TraceFrames> {
    let mut lines = t.lines();
    let header = lines.next()?.trim();
    let (exception, message) = header.split_once(": ")?;
    let mut frames = Vec::new();
    for l in lines {
        let l = l.trim();
        let Some(rest) = l.strip_prefix("at ") else {
            continue;
        };
        if rest.starts_with("async ") || !rest.contains('(') {
            continue;
        }
        // `Gateway.submit (/app/pay.js:42:15)` — JS locs are file:line:col
        let (sym, loc) = rest.split_once('(').unwrap_or((rest, ""));
        let loc = loc.trim_end_matches(')').trim();
        // drop the trailing :col first, then read the line
        let (file, line) = match loc.rsplit_once(':') {
            Some((f, _col)) => match f.rsplit_once(':') {
                Some((f2, n)) => (Some(f2.to_string()), n.parse().ok()),
                None => (Some(f.to_string()), None),
            },
            None => (Some(loc.to_string()), None),
        };
        frames.push(TraceFrame {
            symbol_text: sym.trim().to_string(),
            file,
            line,
        });
    }
    if frames.is_empty() {
        return None;
    }
    Some(TraceFrames {
        lang: "javascript",
        exception: exception.to_string(),
        message: message.to_string(),
        frames,
    })
}

// ---------------------------------------------------------------- inbox

/// One inbox entry: a fingerprinted exception family with a counted
/// occurrence tally and a lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxEntry {
    pub fingerprint: String,
    pub exception: String,
    pub message: String,
    pub occurrences: u64,
    /// First-seen trace (outermost 3 frames) for quick triage.
    pub sample_frames: Vec<TraceFrame>,
    /// open | investigating | dismissed
    pub state: InboxState,
}

/// Entry lifecycle (US-064: any entry can be sent to investigation or
/// dismissed; states are explicit, never erased).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InboxState {
    Open,
    Investigating,
    Dismissed,
}

impl InboxState {
    pub fn as_str(&self) -> &'static str {
        match self {
            InboxState::Open => "open",
            InboxState::Investigating => "investigating",
            InboxState::Dismissed => "dismissed",
        }
    }
}

/// The exceptions inbox: repeats collapse into counted entries.
#[derive(Debug, Clone, Default)]
pub struct ExceptionsInbox {
    entries: BTreeMap<String, InboxEntry>,
    order: Vec<String>, // first-seen order, stable listing
}

impl ExceptionsInbox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one occurrence; repeats increment the tally (never lost).
    pub fn record(&mut self, trace: &TraceFrames) {
        let fp = trace.fingerprint();
        match self.entries.get_mut(&fp) {
            Some(e) => e.occurrences += 1,
            None => {
                self.entries.insert(
                    fp.clone(),
                    InboxEntry {
                        fingerprint: fp.clone(),
                        exception: trace.exception.clone(),
                        message: trace.message.clone(),
                        occurrences: 1,
                        sample_frames: trace.frames.iter().take(3).cloned().collect(),
                        state: InboxState::Open,
                    },
                );
                self.order.push(fp);
            }
        }
    }

    /// Stable first-seen listing; `include_dismissed=false` hides them.
    pub fn list(&self, include_dismissed: bool) -> Vec<&InboxEntry> {
        self.order
            .iter()
            .filter_map(|fp| self.entries.get(fp))
            .filter(|e| include_dismissed || e.state != InboxState::Dismissed)
            .collect()
    }

    pub fn get(&self, fingerprint: &str) -> Option<&InboxEntry> {
        self.entries.get(fingerprint)
    }

    pub fn investigate(&mut self, fingerprint: &str) -> bool {
        self.set_state(fingerprint, InboxState::Investigating)
    }

    pub fn dismiss(&mut self, fingerprint: &str) -> bool {
        self.set_state(fingerprint, InboxState::Dismissed)
    }

    fn set_state(&mut self, fingerprint: &str, state: InboxState) -> bool {
        match self.entries.get_mut(fingerprint) {
            Some(e) => {
                e.state = state;
                true
            }
            None => false,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ------------------------------------------------------- LDFI integration

/// One suspect with a factor-by-factor breakdown (US-032 obligation:
/// "why each is suspected" is per-factor, cited).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suspect {
    /// Node/edge id in the lineage graph that matched a frame.
    pub site: String,
    /// Which trace frames matched this site, with the frame evidence.
    pub matching_frames: Vec<String>,
    /// Static lineage role: how many lineage edges touch the site.
    pub lineage_degree: usize,
    /// Ranked score: frames matched (innermost weighted higher) + degree.
    pub score: u64,
}

/// Rank suspects from a parsed trace over a lineage graph's node ids
/// (US-032/US-064: the LDFI planner gets runtime evidence, not hunches).
///
/// Node ids in the graph are matched against frame symbol text by
/// substring (suffix match: `pay::gateway::submit` matches a frame named
/// `submit` or `gateway::submit`). Innermost frames (stack end) weigh 3,
/// middle 2, outermost 1.
pub fn suspects_from_trace(trace: &TraceFrames, lineage: &LineageGraph) -> Vec<Suspect> {
    let mut sites: BTreeMap<&str, Suspect> = BTreeMap::new();
    let n_frames = trace.frames.len();
    for (i, frame) in trace.frames.iter().enumerate() {
        // innermost = last frame
        let weight: u64 = if i + 1 == n_frames {
            3
        } else if i + 2 >= n_frames {
            2
        } else {
            1
        };
        for edge in &lineage.edges {
            for node in [&edge.from, &edge.to] {
                if node_matches_frame(node, &frame.symbol_text) {
                    let e = sites.entry(node).or_insert_with(|| Suspect {
                        site: node.to_string(),
                        matching_frames: Vec::new(),
                        lineage_degree: lineage_degree(lineage, node),
                        score: 0,
                    });
                    e.matching_frames.push(format!(
                        "{}:{}:{}",
                        frame.symbol_text,
                        frame.file.clone().unwrap_or_default(),
                        frame.line.map(|l| l.to_string()).unwrap_or_default()
                    ));
                    e.score += weight;
                }
            }
        }
    }
    let mut v: Vec<Suspect> = sites.into_values().collect();
    v.sort_by(|a, b| b.score.cmp(&a.score).then(a.site.cmp(&b.site)));
    v
}

fn node_matches_frame(node: &str, symbol: &str) -> bool {
    let node_n = node.rsplit("::").next().unwrap_or(node);
    let sym_tail = symbol.rsplit("::").next().unwrap_or(symbol);
    symbol.contains(node) || sym_tail == node_n
}

fn lineage_degree(lineage: &LineageGraph, node: &str) -> usize {
    lineage
        .edges
        .iter()
        .filter(|e| e.from == node || e.to == node)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ldfi::{LineageGraph, LineageKind};

    #[test]
    fn parses_python_traceback() {
        let t = "Traceback (most recent call last):\n  \
                 File \"app/main.py\", line 10, in run\n    \
                 run()\n  \
                 File \"app/pay.py\", line 42, in charge\n    \
                 result = gateway.submit(order)\n\
                 ValueError: insufficient funds\n";
        let tf = parse_trace(t).unwrap();
        assert_eq!(tf.lang, "python");
        assert_eq!(tf.exception, "ValueError");
        assert_eq!(tf.message, "insufficient funds");
        assert_eq!(tf.frames.len(), 2);
        assert_eq!(tf.frames[1].symbol_text, "charge");
        assert_eq!(tf.frames[1].file.as_deref(), Some("app/pay.py"));
        assert_eq!(tf.frames[1].line, Some(42));
        assert!(!tf.fingerprint().is_empty());
    }

    #[test]
    fn parses_rust_panic_with_backtrace() {
        let t = "thread 'main' panicked at src/pay.rs:42:9:\ninsufficient funds\n\
                 stack backtrace:\n   0: pay::main\n             at ./src/main.rs:10:5\n   \
                 1: pay::gateway::submit\n";
        let tf = parse_trace(t).unwrap();
        assert_eq!(tf.lang, "rust");
        assert_eq!(tf.exception, "panic");
        assert_eq!(tf.message, "insufficient funds");
        assert_eq!(tf.frames.len(), 2);
        assert_eq!(tf.frames[1].symbol_text, "pay::gateway::submit");
        assert_eq!(tf.frames[0].line, Some(10));
    }

    #[test]
    fn parses_java_stack() {
        let t = "javax.naming.NameNotFoundException: thing\n  \
                 at com.app.Pay.charge(Pay.java:42)\n  \
                 at com.app.Main.run(Main.java:10)\n";
        let tf = parse_trace(t).unwrap();
        assert_eq!(tf.lang, "java");
        assert_eq!(tf.exception, "javax.naming.NameNotFoundException");
        assert_eq!(tf.frames[0].symbol_text, "com.app.Pay.charge");
        assert_eq!(tf.frames[0].line, Some(42));
    }

    #[test]
    fn parses_js_stack() {
        let t = "Error: insufficient funds\n    \
                 at Gateway.submit (/app/pay.js:42:15)\n    \
                 at Main.run (/app/main.js:10:5)\n";
        let tf = parse_trace(t).unwrap();
        assert_eq!(tf.lang, "javascript");
        assert_eq!(tf.frames[0].symbol_text, "Gateway.submit");
        assert_eq!(tf.frames[0].line, Some(42));
    }

    #[test]
    fn unparseable_trace_is_a_named_error() {
        let e = parse_trace("hello world, nothing here").unwrap_err();
        assert!(e.reason.contains("no known trace shape"));
        assert!(parse_trace("").is_err());
    }

    #[test]
    fn inbox_collapses_repeats_into_counted_entries() {
        let t1 = "Traceback (most recent call last):\n  File \"a.py\", line 1, in f\n    f()\nValueError: boom\n";
        let tf = parse_trace(t1).unwrap();
        let mut inbox = ExceptionsInbox::new();
        inbox.record(&tf);
        inbox.record(&tf);
        inbox.record(&tf);
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox.list(true)[0].occurrences, 3);
        assert_eq!(inbox.list(true)[0].state, InboxState::Open);
        let fp = inbox.list(true)[0].fingerprint.clone();
        assert!(inbox.investigate(&fp));
        assert_eq!(inbox.list(true)[0].state, InboxState::Investigating);
        assert!(inbox.dismiss(&fp));
        assert_eq!(inbox.list(false).len(), 0); // hidden, not erased
        assert_eq!(inbox.list(true).len(), 1);
        assert!(!inbox.dismiss("no-such-fp"));
    }

    #[test]
    fn suspects_rank_innermost_higher_with_factor_evidence() {
        let mut g = LineageGraph::empty();
        g.add("e1", LineageKind::Message, "main", "charge");
        g.add("e2", LineageKind::Computation, "charge", "submit");
        g.add("e3", LineageKind::StateUpdate, "submit", "ledger");
        let t = "Traceback (most recent call last):\n  \
                 File \"m.py\", line 1, in main\n    x\n  \
                 File \"p.py\", line 5, in submit\n    x\n\
                 ValueError: boom\n";
        let tf = parse_trace(t).unwrap();
        let s = suspects_from_trace(&tf, &g);
        assert!(!s.is_empty());
        // `submit` matched innermost (weight 3) vs `main` outermost (weight 1)
        assert_eq!(s[0].site, "submit");
        assert!(s[0].score > s.last().unwrap().score);
        assert!(!s[0].matching_frames.is_empty());
        // lineage degree exposed per suspect
        assert_eq!(s[0].lineage_degree, 2);
    }
}
