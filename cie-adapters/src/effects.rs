//! Builtin/effect classification shared by all adapters: given the textual
//! callee (e.g. `cursor.execute`, `print`, `requests.get`) decide whether it
//! is a known builtin effect or left for cross-file resolution.

use cie_semantic::facts::{BuiltinKind, SinkKind};

/// Classify a callee text into a builtin kind, if it matches a known
/// library/builtin pattern. Ordered most-specific first.
pub fn classify_builtin(text: &str) -> Option<BuiltinKind> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    let last = lower.rsplit('.').next().unwrap_or(&lower);
    let head = lower.split('.').next().unwrap_or(&lower);

    // --- specific library prefixes first ---
    if lower.starts_with("os.system")
        || lower.starts_with("os.popen")
        || lower.starts_with("subprocess.")
        || lower.contains("child_process")
    {
        return Some(BuiltinKind::ProcessExec);
    }
    if lower.starts_with("requests.")
        || lower.starts_with("httpx.")
        || lower.starts_with("urllib.")
        || lower.starts_with("aiohttp.")
        || lower.starts_with("http.")
        || lower.starts_with("net/http")
        || lower.contains("urlopen")
    {
        return Some(BuiltinKind::Network);
    }
    if lower.starts_with("os.environ") || lower.starts_with("os.getenv") {
        return Some(BuiltinKind::Env);
    }
    if lower.starts_with("hashlib.")
        || lower.starts_with("hmac.")
        || lower.starts_with("secrets.")
        || lower.starts_with("crypto")
    {
        return Some(BuiltinKind::Crypto);
    }

    // --- last-segment classification ---
    let kind = match last {
        "print" | "pprint" | "echo" | "puts" | "println" => BuiltinKind::Print,
        "input" | "readline" | "stdin" => BuiltinKind::Other,
        "execute" | "executemany" | "executescript" | "executemutation" => BuiltinKind::DbExecute,
        "fetchone" | "fetchall" | "fetchmany" | "query" | "queryrow" | "select" | "find"
        | "find_one" | "find_many" | "get_collection" | "cursor" => BuiltinKind::DbQuery,
        "get" | "post" | "put" | "patch" | "delete" | "fetch" | "download" | "upload"
            if head == "requests"
                || head == "httpx"
                || head == "client"
                || head == "session"
                || head == "api"
                || head == "http"
                || head == "resp" =>
        {
            BuiltinKind::Network
        }
        "urlopen" | "getaddrinfo" | "connect" => BuiltinKind::Network,
        "open" => BuiltinKind::FileRead,
        "read" | "readlines" | "read_text" => BuiltinKind::FileRead,
        "write" | "writelines" | "write_text" | "save" => BuiltinKind::FileWrite,
        "hash" | "sha256" | "md5" | "hmac" | "encrypt" | "decrypt" | "digest" | "hashpw"
        | "checkpw" | "hash_password" | "verify_password" | "sign" | "verify_sig" => {
            BuiltinKind::Crypto
        }
        "random" | "randint" | "choice" | "randrange" | "uuid4" | "uuid" => BuiltinKind::Random,
        "getenv" | "environ" | "getenvint" => BuiltinKind::Env,
        "system" | "popen" | "spawn" | "exec_sync" | "command" => BuiltinKind::ProcessExec,
        "eval" | "exec" => BuiltinKind::Eval,
        "assert" | "assert_equal" | "assert_true" | "assertequals" => BuiltinKind::Assert,
        "sleep" => BuiltinKind::Sleep,
        "acquire" | "lock" | "rlock" => BuiltinKind::Lock,
        "time" | "now" | "timestamp" | "utcnow" | "monotonic" => BuiltinKind::Time,
        "debug" | "info" | "warning" | "warn" | "error" | "critical" | "exception" | "log"
            if lower.contains("log") || lower.starts_with("console") =>
        {
            BuiltinKind::Log
        }
        _ => {
            // logger-style prefix: log.debug / logger.info / console.log
            if head == "log"
                || head == "logger"
                || head == "logging"
                || head == "console"
                || head == "slog"
            {
                return Some(BuiltinKind::Log);
            }
            return None;
        }
    };
    Some(kind)
}

/// Classify a call as a taint sink.
pub fn classify_sink(text: &str, args_text: &str) -> Option<SinkKind> {
    let lower = text.to_ascii_lowercase();
    let args_lower = args_text.to_ascii_lowercase();
    if last_seg_is(
        &lower,
        &["execute", "executemany", "executescript", "raw", "query"],
    ) {
        return Some(SinkKind::SqlExecute);
    }
    if lower.contains("cursor.execute")
        || lower.contains("db.execute")
        || lower.contains("conn.execute")
    {
        return Some(SinkKind::SqlExecute);
    }
    if lower.starts_with("os.system")
        || lower.starts_with("os.popen")
        || lower.starts_with("subprocess.")
        || lower.contains("child_process.exec")
    {
        return Some(SinkKind::ShellExec);
    }
    if last_seg_is(&lower, &["eval", "exec"]) {
        return Some(SinkKind::CodeEval);
    }
    if last_seg_is(&lower, &["render_template_string"]) || lower.contains("markup(") {
        return Some(SinkKind::HtmlRender);
    }
    if last_seg_is(&lower, &["open", "write_text"]) && has_write_mode(&args_lower) {
        return Some(SinkKind::FileWrite);
    }
    None
}

/// Detect taint sources in an assignment RHS / call text.
pub fn classify_source(text: &str) -> Option<cie_semantic::facts::SourceKind> {
    use cie_semantic::facts::SourceKind;
    let lower = text.to_ascii_lowercase();
    if lower.contains("input(") {
        return Some(SourceKind::Stdin);
    }
    if lower.contains("request.args")
        || lower.contains("request.form")
        || lower.contains("request.json")
        || lower.contains("request.headers")
        || lower.contains("request.data")
        || lower.contains("request.get_json")
        || lower.contains("req.query")
        || lower.contains("req.body")
        || lower.contains("request.query")
    {
        return Some(SourceKind::RequestInput);
    }
    if lower.contains("os.environ") {
        return Some(SourceKind::EnvVar);
    }
    if lower.contains("sys.argv") || lower.contains("os.args") {
        return Some(SourceKind::Argv);
    }
    None
}

fn last_seg_is(lower_text: &str, names: &[&str]) -> bool {
    let last = lower_text.rsplit('.').next().unwrap_or(lower_text);
    let last = last.trim_end_matches(['(', '!']);
    names.contains(&last)
}

fn has_write_mode(args_lower: &str) -> bool {
    // crude but effective: "w" / "a" / "x" mode literals
    [
        "'w'", "\"w\"", "'a'", "\"a\"", "'x'", "\"x\"", "'wb'", "\"wb\"",
    ]
    .iter()
    .any(|m| args_lower.contains(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_basics() {
        assert_eq!(classify_builtin("print"), Some(BuiltinKind::Print));
        assert_eq!(
            classify_builtin("cursor.execute"),
            Some(BuiltinKind::DbExecute)
        );
        assert_eq!(classify_builtin("requests.get"), Some(BuiltinKind::Network));
        assert_eq!(
            classify_builtin("os.system"),
            Some(BuiltinKind::ProcessExec)
        );
        assert_eq!(classify_builtin("logger.info"), Some(BuiltinKind::Log));
        assert_eq!(
            classify_builtin("hashlib.sha256"),
            Some(BuiltinKind::Crypto)
        );
        assert_eq!(classify_builtin("os.environ.get"), Some(BuiltinKind::Env));
        assert_eq!(classify_builtin("my_func"), None);
    }

    #[test]
    fn classify_sinks() {
        assert_eq!(
            classify_sink("cursor.execute", "'...'"),
            Some(SinkKind::SqlExecute)
        );
        assert_eq!(classify_sink("os.system", "cmd"), Some(SinkKind::ShellExec));
        assert_eq!(classify_sink("eval", "x"), Some(SinkKind::CodeEval));
        assert_eq!(classify_sink("open", "'f', 'w'"), Some(SinkKind::FileWrite));
    }
}
