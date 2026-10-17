//! Generated fixture repositories for the deep checks. Every check works on
//! its own tempdir so checks are order-independent and honest about state.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic nonce so fixture dirs never collide across processes or runs:
/// container pids recycle between back-to-back `cargo test` invocations, and
/// a recycled pid reviving a half-deleted `.cie` store turns a fresh ingest
/// into a ghost no-op (index without snapshot). Time-nonce kills that class.
static NONCE: AtomicU64 = AtomicU64::new(0);

pub fn tmp_dir(name: &str) -> PathBuf {
    let uniq = NONCE.fetch_add(1, Ordering::SeqCst);
    let d = std::env::temp_dir().join(format!(
        "cie-us-{name}-{}-{}",
        std::process::id(),
        cie_core::types::now_ns() + uniq,
    ));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).expect("tmp dir created");
    d
}

pub const APP_PY: &str = "\
from svc import charge


def checkout(user, amount):
    if amount <= 0:
        raise ValueError(\"bad amount\")
    return charge(user, amount)
";

pub const SVC_PY: &str = "\
def authorize(user):
    return True


def charge(user, amount):
    _ = authorize(user)
    return {\"user\": user, \"amount\": amount}


def refund(user, amount):
    _ = authorize(user)
    print(\"refunding\", user)
    return {\"user\": user, \"refund\": amount}
";

pub const TEST_PY: &str = "\
from app import checkout


def test_checkout():
    assert checkout(\"u\", 5)
";

/// The standard 3-file Python fixture repo: app → svc call chain + a test.
pub fn python_repo(name: &str) -> PathBuf {
    let d = tmp_dir(name);
    fs::write(d.join("app.py"), APP_PY).unwrap();
    fs::write(d.join("svc.py"), SVC_PY).unwrap();
    fs::write(d.join("test_app.py"), TEST_PY).unwrap();
    d
}

/// Requirement-style document whose claims map onto the python repo.
pub const SPEC_MD: &str = "\
# Payments spec

- The system **must** authorize the user before every charge.
- The charge function **shall** return a receipt with the user and amount.
- The checkout **should** reject non-positive amounts.
- The service **may** retry failed charges.
";

/// Ingest a fixture repo with default options.
pub fn ingest(name: &str) -> cie_ingest::IngestOutcome {
    let dir = python_repo(name);
    cie_ingest::run_ingest(&dir, &cie_ingest::IngestOptions::default())
        .expect("ingest succeeds on fixture")
}
