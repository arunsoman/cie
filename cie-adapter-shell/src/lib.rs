//! Shell adapter (v1.2.1): a hand-rolled line scanner — no grammar, no
//! guesses. Honesty rules:
//! * what it reads: functions (including `test_*` → Test), the files they
//!   `source`, the external commands they run (including one level of
//!   `$(…)`), and the effects those commands are known to have (file
//!   reads/writes, network, database, process execution, env, sleep) —
//!   classified from the command NAME only, never from guessing at
//!   arguments;
//! * what it does NOT do: it doesn't run the script or expand variables —
//!   commands behind `$VARS` stay unknown, unknown commands stay
//!   `Unresolved("external_command")`, heredoc bodies are skipped with a
//!   note (FR-4: named, never silently dropped).
//!
//! Semantics tables (§16, vouched here — FR-3):
//! - dispatch: `Dynamic` — commands resolve against $PATH at runtime
//! - error model: `ErrorCode` — exit-status conventions (`$?`), like C
//! - numeric: default row — arithmetic happens in `$(())` we don't model

use cie_adapters::{DispatchKind, ErrorModel, LanguageAdapter};
use cie_core::types::{SourceLocation, SymbolId};
use cie_semantic::facts::{
    AsyncKind, AsyncSite, BuiltinKind, Callee, CallSite, ImportFact, LangFacts, SymbolFact,
    SymbolKind,
};
use cie_semantic::LangId;

pub struct ShellAdapter;

/// One logical command line: (first physical line number, joined text).
struct ScriptLine {
    line: u32,
    text: String,
}

/// Split source into logical lines: comments stripped (word-initial `#`
/// only, outside quotes), backslash continuations joined, heredoc bodies
/// skipped (with a note so the skip is visible).
fn logical_lines(src: &str, skipped: &mut Vec<String>) -> Vec<ScriptLine> {
    let mut out = vec![];
    let mut heredoc_delim: Option<String> = None;
    let mut heredocs = 0usize;
    let mut heredoc_start = 0u32;

    let mut join: Option<(String, u32)> = None;
    for (i, raw) in src.lines().enumerate() {
        let n = (i + 1) as u32;
        if let Some(delim) = &heredoc_delim {
            if raw.trim_end() == delim.as_str() {
                heredoc_delim = None;
            }
            continue;
        }
        // strip comment (word-initial `#` outside quotes)
        let mut text = String::new();
        let mut quote: Option<char> = None;
        let mut chars = raw.chars().peekable();
        let mut cut = false;
        while let Some(c) = chars.next() {
            if let Some(q) = quote {
                if c == q {
                    quote = None;
                }
                text.push(c);
                continue;
            }
            if c == '\'' || c == '"' {
                quote = Some(c);
                text.push(c);
                continue;
            }
            if c == '\\' {
                match chars.peek() {
                    Some('\n') | None => {
                        cut = true;
                        break; // line continuation — join with the next line
                    }
                    Some(&nx) => {
                        text.push(c);
                        text.push(nx);
                        chars.next();
                        continue;
                    }
                }
            }
            if c == '#' && (text.is_empty() || text.ends_with(' ')) {
                break; // comment to end of line
            }
            text.push(c);
        }
        if cut {
            match &mut join {
                Some((buf, _)) => buf.push_str(text.trim_end()),
                None => join = Some((text.trim_end().to_string(), n)),
            }
            continue;
        }
        let (buf, start) = match join.take() {
            Some((mut b, s)) => {
                b.push_str(&text);
                (b, s)
            }
            None => (text, n),
        };
        // heredoc start? `<<DELIM` outside quotes
        if !inside_quotes(&buf, buf.find("<<")) {
            if let Some(pos) = buf.find("<<") {
                let after = &buf[pos + 2..];
                let after = after.trim_start_matches('-');
                let delim: String = after
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\'' || *c == '"')
                    .collect();
                let delim = delim.trim_matches(|c| c == '\'' || c == '"');
                if !delim.is_empty() {
                    heredoc_delim = Some(delim.to_string());
                    if heredocs == 0 {
                        heredoc_start = start;
                    }
                    heredocs += 1;
                }
            }
        }
        let t = buf.trim();
        if !t.is_empty() {
            out.push(ScriptLine { line: start, text: t.to_string() });
        }
    }
    if heredocs > 0 {
        skipped.push(format!(
            "line {heredoc_start}: {heredocs} heredoc body(s) skipped — inline data, not commands"
        ));
    }
    out
}

/// Is byte position `at` (from `find`) inside a quoted span of `s`?
fn inside_quotes(s: &str, at: Option<usize>) -> bool {
    let Some(at) = at else { return false };
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        if i >= at {
            break;
        }
        if c == '\'' || c == '"' {
            if quote == Some(c) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(c);
            }
        }
    }
    quote.is_some()
}

/// Split a logical line into command segments at `|`, `&&`, `||`, `;`
/// (quote-aware). A trailing `&` ends the last segment (background).
fn segments(line: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            cur.push(c);
            continue;
        }
        match c {
            '\'' | '"' => {
                quote = Some(c);
                cur.push(c);
            }
            '|' => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            '&' => {
                if chars.peek() == Some(&'&') {
                    chars.next();
                }
                out.push(std::mem::take(&mut cur));
            }
            ';' => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

/// Strip leading env assignments (`FOO=1 BAR=x cmd`) and prefixes like
/// `sudo`, `nice`, `env`, `time`, `command`, `exec`, `nohup`, `xargs`;
/// return the command word.
fn command_word(seg: &str) -> Option<String> {
    let mut toks = seg.trim().split_whitespace().peekable();
    while let Some(t) = toks.peek() {
        let lower = t.to_ascii_lowercase();
        let is_env_assign = t.contains('=')
            && !t.starts_with('$')
            && !t.starts_with('"')
            && !t.starts_with('\'');
        let is_prefix = matches!(
            lower.as_str(),
            "sudo" | "nice" | "env" | "time" | "command" | "exec" | "nohup" | "xargs" | "stdbuf"
                | "timeout"
        );
        if is_env_assign || is_prefix {
            toks.next();
            continue;
        }
        let word: String = t
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
            .collect();
        if word.is_empty() {
            return None;
        }
        return Some(word);
    }
    None
}

fn builtin_kind(cmd: &str) -> Option<BuiltinKind> {
    let lower = cmd.to_ascii_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    let base = base.split('.').next().unwrap_or(base);
    Some(match base {
        "curl" | "wget" | "nc" | "ncat" | "netcat" | "ssh" | "scp" | "sftp" | "ftp" | "telnet"
        | "ping" => BuiltinKind::Network,
        "psql" | "mysql" | "sqlite3" | "pg_dump" | "pg_restore" | "mongosh" | "mongo"
        | "redis-cli" | "sqlcmd" | "sqlplus" | "duckdb" => BuiltinKind::DbExecute,
        "cat" | "less" | "more" | "head" | "tail" | "grep" | "egrep" | "fgrep" | "rg" | "awk"
        | "gawk" | "sed" | "cut" | "sort" | "uniq" | "wc" | "diff" | "stat" | "file" | "find"
        | "du" | "df" | "jq" | "xxd" | "od" | "md5sum" | "sha256sum" => BuiltinKind::FileRead,
        "rm" | "mv" | "cp" | "mkdir" | "rmdir" | "touch" | "chmod" | "chown" | "chgrp" | "ln"
        | "dd" | "tee" | "truncate" | "install" | "tar" | "zip" | "unzip" | "gzip" | "gunzip"
        | "rsync" => BuiltinKind::FileWrite,
        "echo" | "printf" => BuiltinKind::Print,
        "sleep" => BuiltinKind::Sleep,
        "export" | "unset" => BuiltinKind::Env,
        "eval" => BuiltinKind::Eval,
        "git" | "make" | "cmake" | "cargo" | "npm" | "npx" | "bun" | "yarn" | "pnpm" | "go"
        | "python" | "python3" | "pip" | "pip3" | "pytest" | "java" | "javac" | "mvn"
        | "gradle" | "dotnet" | "docker" | "kubectl" | "helm" | "systemctl" | "service" | "apt"
        | "apt-get" | "yum" | "brew" | "ninja" | "meson" | "node" | "ruby" | "php" | "perl" => {
            BuiltinKind::ProcessExec
        }
        "date" => BuiltinKind::Time,
        _ => return None,
    })
}

/// Shell keywords that open blocks — never command words.
fn is_block_keyword(w: &str) -> bool {
    matches!(
        w.to_ascii_lowercase().as_str(),
        "if" | "then" | "else" | "elif" | "fi" | "for" | "while" | "until" | "do" | "done"
            | "case" | "esac" | "in" | "function" | "{" | "}" | "(" | ")" | "break" | "continue"
            | "set" | "shopt" | "alias" | "declare" | "typeset" | "readonly" | "select"
    )
}

/// Function-definition name on a line, if any:
/// `name() {`, `function name {`, `function name() {`.
fn function_def(t: &str) -> Option<String> {
    let lower = t.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("function ") {
        let name = t[t.len() - rest.len()..]
            .trim()
            .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
            .next()
            .unwrap_or("");
        if !name.is_empty() && !is_block_keyword(name) {
            return Some(name.to_string());
        }
        return None;
    }
    let paren = t.find("()")?;
    let head = t[..paren].trim();
    if !head.is_empty()
        && !head.contains(' ')
        && head.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !is_block_keyword(head)
    {
        return Some(head.to_string());
    }
    None
}

struct ShellCtx {
    facts: LangFacts,
    module: SymbolId,
    /// symbol the following commands belong to (module or function)
    owner: SymbolId,
    next_id: u32,
    /// every function name defined anywhere in this file (pass-1 collected),
    /// so calls to later-defined functions still resolve by name
    funcs: Vec<String>,
}

impl ShellCtx {
    fn id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }

    fn push_import(&mut self, module: &str, line: u32) {
        let module = module.trim_matches(|c| c == '"' || c == '\'');
        if module.is_empty() {
            return;
        }
        self.facts.imports.push(ImportFact {
            module_symbol: self.module,
            module: module.to_string(),
            names: vec![],
            alias: None,
            line,
        });
    }

    fn push_call(&mut self, cmd: &str, line: u32) {
        let callee = if self.funcs.iter().any(|f| f == cmd) {
            Callee::Unresolved("pending".into()) // resolution.rs links by name
        } else if let Some(kind) = builtin_kind(cmd) {
            Callee::Builtin(kind)
        } else {
            Callee::Unresolved("external_command".into())
        };
        self.facts.calls.push(CallSite {
            caller: self.owner,
            callee_text: cmd.to_string(),
            callee,
            line,
            in_loop: false,
            arg_vars: vec![],
        });
    }
}

fn extract_shell(path: &str, src: &str) -> LangFacts {
    let mut skipped: Vec<String> = vec![];
    let lines = logical_lines(src, &mut skipped);

    // pass 1: collect every function name so calls to later-defined
    // functions are still marked resolvable (resolution.rs finishes the job)
    let funcs: Vec<String> = lines.iter().filter_map(|sl| function_def(&sl.text)).collect();

    let module = path
        .trim_end_matches(".sh")
        .trim_end_matches(".bash")
        .trim_end_matches(".zsh")
        .replace('/', ".");
    let mut ctx = ShellCtx {
        facts: LangFacts::new(path, LangId::Shell, "line-scan:shell"),
        module: SymbolId(0),
        owner: SymbolId(0),
        next_id: 1,
        funcs,
    };
    ctx.facts.symbols.push(SymbolFact {
        id: ctx.module,
        kind: SymbolKind::Module,
        name: module.clone(),
        qual_name: module.clone(),
        loc: SourceLocation::span_bytes(cie_core::types::FileId(0), 0, src.len() as u32),
        lang: LangId::Shell,
        parent: None,
        rev: 0,
        doc: None,
    });

    for sl in &lines {
        let t = sl.text.as_str();
        let lower = t.to_ascii_lowercase();

        // `source file` / `. file` — import, not a call
        if let Some(rest) = lower.strip_prefix("source ") {
            if let Some(f) = t[t.len() - rest.len()..].split_whitespace().next() {
                ctx.push_import(f, sl.line);
            }
            continue;
        }
        if t.starts_with(". ") {
            if let Some(f) = t[2..].split_whitespace().next() {
                ctx.push_import(f, sl.line);
            }
            continue;
        }

        // function definition?
        if let Some(name) = function_def(t) {
            let id = ctx.id();
            let is_test = name.starts_with("test_") || name.ends_with("_test");
            ctx.facts.symbols.push(SymbolFact {
                id,
                kind: if is_test { SymbolKind::Test } else { SymbolKind::Function },
                name: name.clone(),
                qual_name: format!("{module}.{name}"),
                loc: SourceLocation::span_bytes(cie_core::types::FileId(0), 0, 0),
                lang: LangId::Shell,
                parent: Some(ctx.module),
                rev: 0,
                doc: None,
            });
            // one-line body? `name() { cmd; cmd2; }` — commands stay in the fn
            let one_line = t.contains('{') && t.rfind('}').map_or(false, |b| b > t.find('{').unwrap_or(b));
            if one_line {
                let open = t.find('{').unwrap_or(0);
                let close = t.rfind('}').unwrap_or(t.len());
                let body = &t[open + 1..close];
                let prev = ctx.owner;
                ctx.owner = id;
                for seg in segments(body) {
                    if let Some(cmd) = command_word(&seg) {
                        if !is_block_keyword(&cmd) {
                            ctx.push_call(&cmd, sl.line);
                        }
                    }
                }
                ctx.owner = prev;
            } else {
                ctx.owner = id;
            }
            continue;
        }

        // only a lone `}` closes a function body (fi/done/esac do not)
        if t == "}" {
            ctx.owner = ctx.module;
            continue;
        }

        // command segments on this line
        let segs = segments(t);
        let background = t.ends_with('&') || t.trim_end().ends_with("&");
        for seg in &segs {
            let Some(cmd) = command_word(seg) else { continue };
            let blocked = is_block_keyword(&cmd);
            if !blocked {
                if cmd == "source" || cmd == "." {
                    // `source` mid-line (rare) — still an import
                    if let Some(f) = seg.split_whitespace().nth(1) {
                        ctx.push_import(f, sl.line);
                    }
                } else {
                    ctx.push_call(&cmd, sl.line);
                }
            }
            // one level of command substitution: $(cmd …) — real commands,
            // scanned even on block-keyword lines (`for i in $(seq …); do`)
            let mut rest = seg.as_str();
            while let Some(pos) = rest.find("$(") {
                let inner = &rest[pos + 2..];
                if let Some(cmd2) = command_word(inner.split(')').next().unwrap_or(inner)) {
                    if !is_block_keyword(&cmd2) && cmd2 != cmd {
                        ctx.push_call(&cmd2, sl.line);
                    }
                }
                rest = &rest[pos + 2..];
            }
        }
        if background {
            ctx.facts.async_facts.push(AsyncSite {
                symbol: ctx.owner,
                kind: AsyncKind::Spawn,
                line: sl.line,
            });
        }
    }

    for note in skipped {
        ctx.facts.unsupported.push(note);
    }
    ctx.facts
        .unsupported
        .push("variables are not expanded — commands behind $VARS are counted as unknown".to_string());
    ctx.facts
}

impl LanguageAdapter for ShellAdapter {
    fn id(&self) -> LangId {
        LangId::Shell
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["sh", "bash", "zsh"]
    }
    fn analyzer(&self) -> &'static str {
        "line-scan:shell"
    }
    fn extract(&self, path: &str, src: &str) -> LangFacts {
        extract_shell(path, src)
    }
    fn dispatch_kind(&self) -> DispatchKind {
        DispatchKind::Dynamic
    }
    /// Exit-status conventions (`$?`) — commands report failure by code.
    fn error_model(&self) -> ErrorModel {
        ErrorModel::ErrorCode
    }
}

/// One-line registration hook for the composition-root registry
/// (`cie-ingest::default_registry`).
pub fn adapter() -> Box<dyn LanguageAdapter> {
    Box::new(ShellAdapter)
}

#[test]
fn shell_adapter_facts() {
    let adapter = ShellAdapter;
    assert_eq!(adapter.id(), LangId::Shell);
    let src = r#"#!/usr/bin/env bash
# deploy: migrate + verify
set -euo pipefail

source ./lib/common.sh

wait_ready() {
  for i in $(seq 1 30); do
    curl -fsS http://localhost:8080/health >/dev/null || sleep 1
  done
}

run_migrations() {
  psql "$DATABASE_URL" -f db/schema.sql
  migrate up
  echo "migrated"
}

test_wait_ready() {
  wait_ready
}

run_migrations &
wait_ready
git rev-parse HEAD > build/commit.txt
"#;
    let facts = adapter.extract("scripts/deploy.sh", src);
    let names = |k: SymbolKind| -> Vec<String> {
        facts.symbols.iter().filter(|s| s.kind == k).map(|s| s.name.clone()).collect()
    };
    assert_eq!(names(SymbolKind::Function), vec!["wait_ready", "run_migrations"]);
    assert_eq!(names(SymbolKind::Test), vec!["test_wait_ready"]);
    // sourced file
    assert!(facts.imports.iter().any(|i| i.module == "./lib/common.sh"));
    // vouched effects by command name
    assert!(facts.calls.iter().any(|c| c.callee_text == "psql" && matches!(c.callee, Callee::Builtin(BuiltinKind::DbExecute))));
    assert!(facts.calls.iter().any(|c| c.callee_text == "curl" && matches!(c.callee, Callee::Builtin(BuiltinKind::Network))));
    assert!(facts.calls.iter().any(|c| c.callee_text == "git" && matches!(c.callee, Callee::Builtin(BuiltinKind::ProcessExec))));
    // `seq` captured from $(seq 1 30)
    assert!(facts.calls.iter().any(|c| c.callee_text == "seq"));
    // in-tree calls marked pending (later-defined too: run_migrations called before def? here after, and wait_ready call after def)
    assert!(facts.calls.iter().any(|c| c.callee_text == "wait_ready" && matches!(c.callee, Callee::Unresolved(ref r) if r == "pending")));
    assert!(facts.calls.iter().any(|c| c.callee_text == "migrate" && matches!(c.callee, Callee::Unresolved(ref r) if r == "external_command")));
    // psql call belongs to run_migrations, not the module
    let psql_caller = facts.calls.iter().find(|c| c.callee_text == "psql").map(|c| c.caller);
    let run_mig_id = facts.symbols.iter().find(|s| s.name == "run_migrations").map(|s| s.id);
    assert_eq!(psql_caller, run_mig_id);
    // background job
    assert!(!facts.async_facts.is_empty());
    // honesty note about variable expansion
    assert!(facts.unsupported.iter().any(|u| u.contains("variables are not expanded")));
}
