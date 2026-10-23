#!/usr/bin/env python3
"""DS-EVAL migration step 2 surgery (v2): move the Command::Polyglot body out
of the match arm into a run_polyglot_build() helper (build-then-rename).
Finds boundaries dynamically; removes the orphan from the arm; strips the
trailing arm-close brace from the body; inserts the helper before run_stories."""
import sys

MAIN = "/home/z/my-project/cie-cli/src/main.rs"

with open(MAIN, "r", encoding="utf-8") as f:
    lines = f.readlines()

# --- locate boundaries dynamically ---
idx_stories = next(
    i for i, ln in enumerate(lines)
    if ln.rstrip() == "        Command::Stories { cmd } => run_stories(cmd),"
)
if lines[idx_stories - 1].rstrip() != "        }":
    sys.exit(f"FATAL: line before Stories is not an arm close: {lines[idx_stories-1]!r}")
start_idx = next(
    i for i, ln in enumerate(lines)
    if ln.rstrip() == "            let static_res ="
)
if start_idx >= idx_stories:
    sys.exit("FATAL: orphan start after Stories line?")
body = "".join(lines[start_idx:idx_stories])
if not body.rstrip().endswith("        }"):
    sys.exit("FATAL: orphan does not end with arm-close brace")

# --- transform the body for Result<i32, String> ---
body = body.replace(
    """            let Ok((stats, lower)) = static_res else {
                eprintln!("error: {}", static_res.err().unwrap_or_default());
                return 1;
            };""",
    """            let Ok((stats, lower)) = static_res else {
                return Err(static_res.err().unwrap_or_default());
            };""",
)
body = body.replace(
    """                        Ok(bundle) => {
                            println!("{bundle}");
                            return 0;
                        }
                        Err(e) => {
                            eprintln!("error: {e}");
                            return 1;
                        }""",
    """                        Ok(bundle) => {
                            println!("{bundle}");
                            return Ok(0);
                        }
                        Err(e) => return Err(e),""",
)
body = body.replace(
    """                        if hits.is_empty() {
                            eprintln!(
                                "error: no spec matches '{want}' (available: {})",
                                map.keys().cloned().collect::<Vec<_>>().join(", ")
                            );
                            return 1;
                        }""",
    """                        if hits.is_empty() {
                            return Err(format!(
                                "no spec matches '{want}' (available: {})",
                                map.keys().cloned().collect::<Vec<_>>().join(", ")
                            ));
                        }""",
)
body = body.replace(
    """                        return 0;
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        return 1;
                    }""",
    """                        return Ok(0);
                    }
                    Err(e) => return Err(e),""",
)
body = body.replace("let runtime = otel\n                .as_deref()\n", "let runtime = otel\n")
body = body.replace("\n            0\n", "\n            Ok(0)\n")
for bad in ("return 1;", "return 0;"):
    if bad in body:
        sys.exit(f"FATAL: unconverted '{bad}' remains in body")

# re-indent from 12 to 8 spaces for the closure context
body = "\n".join(ln[4:] if ln.startswith("    ") else ln for ln in body.split("\n"))
# strip the trailing arm-close brace (now re-indented to 4 spaces) — the
# helper's `})();` closes the closure instead
blines = body.split("\n")
while blines and blines[-1].strip() == "":
    blines.pop()
if blines[-1] != "    }":
    sys.exit(f"FATAL: expected re-indented arm close as last body line, got {blines[-1]!r}")
blines.pop()
body = "\n".join(blines) + "\n"

helper = '''/// DS-EVAL §6.2 — build the polyglot store at `building`; the caller promotes
/// it over `db_path` by rename ONLY on success. The web opens `db_path`
/// read-only while a rebuild may run in another process, so promotion must be
/// atomic and the build must be complete-or-nothing.
fn run_polyglot_build(
    building: &std::path::Path,
    path: &std::path::Path,
    otel: Option<&std::path::Path>,
    viewspec: Option<&str>,
    json: bool,
    db_path: &std::path::Path,
) -> Result<i32, String> {
    let conn = cie_polyglot::schema::open_db(building)?;
    let rc = (|| -> Result<i32, String> {
''' + body + '''    })();
    // Best-effort: push every committed WAL frame into the main file so the
    // caller's rename can never orphan rows in a leftover `-wal` (a clean
    // close checkpoints anyway — this is the belt to its suspenders).
    let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    drop(conn); // clean close: SQLite checkpoints + deletes the -wal/-shm
    if rc.is_ok() {
        let wal = std::path::PathBuf::from(format!("{}-wal", building.display()));
        if wal.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            return Err(
                "polyglot WAL did not checkpoint cleanly; refusing to promote".to_string(),
            );
        }
    }
    rc
}

fn run_stories(cmd: StoriesCmd) -> i32 {'''

text = "".join(lines[:start_idx]) + "".join(lines[idx_stories:])
marker = "fn run_stories(cmd: StoriesCmd) -> i32 {"
if marker not in text:
    sys.exit("FATAL: run_stories marker not found")
text = text.replace(marker, helper, 1)

with open(MAIN, "w", encoding="utf-8") as f:
    f.write(text)
print("OK: orphan removed from arm; body moved into run_polyglot_build")
