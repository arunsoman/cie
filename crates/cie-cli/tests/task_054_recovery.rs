//! TASK-054 subset validation — recovery-lite findings from existing facts:
//! ignored error values (`_ = f()`) and bare excepts with no re-raise.

use cie_cli::engine::{self, IngestOptions};
use cie_core::findings::FindingKind;
use std::fs;
use std::path::PathBuf;

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cie-t054-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

fn opts() -> IngestOptions {
    IngestOptions::default()
}

#[test]
fn task_054_ignored_error_value_flagged() {
    let dir = tmp_dir("ignored");
    fs::write(
        dir.join("worker.py"),
        "def run():\n    _ = risky_call()\n    _status = other_call()\n    return True\n",
    )
    .unwrap();
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let ignored = outcome
        .snapshot
        .findings
        .iter()
        .filter(|f| f.kind == FindingKind::IgnoredErrorValue)
        .count();
    assert_eq!(ignored, 2, "both throwaway assignments flagged; got {ignored}");
    // `_status` starts with underscore → also a throwaway (len > 1 rule).
    assert!(outcome
        .snapshot
        .findings
        .iter()
        .any(|f| f.kind == FindingKind::IgnoredErrorValue && f.message.contains("_status")));
}

#[test]
fn task_054_bare_except_without_reraise_flagged() {
    let dir = tmp_dir("bare");
    fs::write(
        dir.join("svc.py"),
        "def handle():\n    try:\n        work()\n    except:\n        print(\"nope\")\n\ndef re-raiser():\n    try:\n        work()\n    except:\n        raise\n",
    )
    .unwrap();
    let outcome = engine::run_ingest(&dir, &opts()).unwrap();
    let recovery = outcome
        .snapshot
        .findings
        .iter()
        .filter(|f| {
            f.kind == FindingKind::SwallowedException && f.rule == "sec/SEC-005"
        })
        .count();
    assert!(recovery >= 1, "bare except without re-raise flagged (SEC-005)");
}
