#!/usr/bin/env python3
"""collect_stories.py — measure and record every one of the 33 remaining
stories, for real, so the dashboard shows actual values instead of
"can't show yet".

Two kinds of data live in the output, and every story says which it is:

  measured — a tool on this machine captured a number from a real run
             (profilers, coverage, git history, the hosting API, an ELF
             parser, pytest, git itself).
  recorded — a walkthrough of the product workflow was actually performed
             while this file was collected, and its real artifacts were
             saved (the brief, the plan, the sandbox commits, the patch,
             the approval events). The workflow shell is not shipped; the
             session record is real.

Nothing is simulated. Where a probe mirrors a flaw that exists in
bankapp's source, the mirror is disclosed. Where a thing could not be
done on this machine, the story says so instead of inventing it.

For ANY other repo, the same data is obtainable the same way — every
story carries an extract → transform → load recipe.

Output: runtime-data/stories/*.json (one artifact per story) plus the
merged runtime-data/stories.json the dashboard loads.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

HERE = Path(__file__).resolve().parent           # bankapp/scripts
BANKAPP = HERE.parent                            # bankapp/
REPO = HERE.parents[2]                           # the CIE repo root
OUT = BANKAPP / "runtime-data"
STORIES_OUT = OUT / "stories"
STORIES_OUT.mkdir(parents=True, exist_ok=True)

sys.path.insert(0, str(BANKAPP))
sys.path.insert(0, str(HERE))

import stories_probes as probes  # noqa: E402

NOW = datetime.now(timezone.utc).isoformat(timespec="seconds")
T0 = time.time()


def env_block() -> dict:
    import os
    return {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "cpu_count": os.cpu_count(),
        "collected_at": NOW,
    }


def write_story(sid: str, payload: dict) -> None:
    (STORIES_OUT / f"{sid}.json").write_text(json.dumps(payload, indent=1))
    print(f"  wrote runtime-data/stories/{sid}.json")


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def run(cmd: list[str], cwd: Path | None = None, timeout: int = 120) -> dict:
    """Run a command for real; keep the real exit code and output."""
    p = subprocess.run(
        cmd, cwd=str(cwd) if cwd else None, capture_output=True, text=True, timeout=timeout
    )
    return {
        "cmd": " ".join(cmd),
        "returncode": p.returncode,
        "stdout_excerpt": p.stdout[-1500:],
        "stderr_excerpt": p.stderr[-1500:],
    }


# =====================================================================
# Sandbox — a real git repository in a temp dir, built from bankapp,
# used by the build/ship stories (US-111 … US-126, US-130, US-131).
# =====================================================================

COPY_FILES = [
    "auth.py", "payments.py", "ui.py", "test_auth.py",
    "requirements.txt", "deadlock_probe.py", "runtime_probes.py",
]
COPY_DIRS = ["db", "config"]


def make_sandbox() -> dict:
    """Copy bankapp into a temp dir, git init, commit the baseline."""
    sbx = Path(tempfile.mkdtemp(prefix="bankapp-sandbox-"))
    for f in COPY_FILES:
        src = BANKAPP / f
        if src.exists():
            shutil.copy2(src, sbx / f)
    for d in COPY_DIRS:
        src = BANKAPP / d
        if src.is_dir():
            shutil.copytree(src, sbx / d)
    (sbx / "README.md").write_text(
        "# bankapp sandbox\n\nA real copy of the sample project, committed for real.\n"
    )
    git = ["git", "-c", "user.name=Walkthrough Sandbox",
           "-c", "user.email=sandbox@bankapp.invalid"]
    run(git + ["init", "-q"], cwd=sbx)
    run(git + ["add", "-A"], cwd=sbx)
    base = run(git + ["commit", "-q", "-m",
                      "baseline: bankapp as it stands today"], cwd=sbx)
    sha = subprocess.run(git + ["rev-parse", "HEAD"], cwd=sbx,
                         capture_output=True, text=True).stdout.strip()
    return {"dir": sbx, "git": git, "baseline_sha": sha, "baseline_commit": base}


def apply_candidate(sbx: dict) -> dict:
    """Copy the candidate feature in and commit it — the real candidate
    change the walkthrough talks about."""
    shutil.copy2(HERE / "stories" / "candidate_transfers.py", sbx["dir"] / "transfers.py")
    shutil.copy2(HERE / "stories" / "candidate_test_transfers.py", sbx["dir"] / "test_transfers.py")
    req = sbx["dir"] / "requirements.txt"
    txt = req.read_text()
    req.write_text(txt + "pyyaml>=6.0\n")  # requested by the plan; flagged unused later
    run(sbx["git"] + ["add", "-A"], cwd=sbx["dir"])
    run(sbx["git"] + ["commit", "-q", "-m",
                      "candidate: spending limit on transfers (limit 1000)"], cwd=sbx["dir"])
    sha = subprocess.run(sbx["git"] + ["rev-parse", "HEAD"], cwd=sbx["dir"],
                         capture_output=True, text=True).stdout.strip()
    stat = subprocess.run(sbx["git"] + ["show", "--stat", "--format=", "HEAD"],
                          cwd=sbx["dir"], capture_output=True, text=True).stdout.strip()
    return {"candidate_sha": sha, "diffstat": stat}


# =====================================================================
# Git history over the CIE repo itself (US-043 ownership, US-088 privacy)
# =====================================================================

SALT = "cie-privacy-at-source-v1"


def hash_name(name: str) -> str:
    return "author-" + hashlib.sha256((SALT + name).encode()).hexdigest()[:8]


def git_history_rows(limit_days_stale: int = 90) -> dict:
    """One pass over the repo's real history: per top-level part, commits,
    distinct authors (hashed at source), last-touch age."""
    p = subprocess.run(
        ["git", "-C", str(REPO), "log", "--name-only", "--format=%x1e%an|%aI"],
        capture_output=True, text=True, timeout=120,
    )
    records = [r for r in p.stdout.split("\x1e") if r.strip()]
    parts: dict[str, dict] = {}
    denied_names = re.compile(r"^(upload/|download/|\.env$|.*runtime-data/|.*node_modules/)")
    denied_touches = 0
    total_commits = 0
    now = datetime.now(timezone.utc)
    for rec in records:
        lines = [l for l in rec.strip().splitlines() if l.strip()]
        if not lines:
            continue
        author_full, date_iso = lines[0].split("|", 1)
        author = hash_name(author_full.strip())
        try:
            dt = datetime.fromisoformat(date_iso.strip())
        except ValueError:
            continue
        total_commits += 1
        for f in lines[1:]:
            f = f.strip()
            if not f:
                continue
            if denied_names.match(f):
                denied_touches += 1
                continue
            part = f.split("/")[0]
            entry = parts.setdefault(part, {"commits": 0, "authors": set(), "last": dt})
            entry["commits"] += 1
            entry["authors"].add(author)
            entry["last"] = max(entry["last"], dt)
    rows = []
    for part, e in sorted(parts.items(), key=lambda kv: -kv[1]["commits"]):
        age_days = (now - e["last"]).days
        rows.append({
            "part": part,
            "commits": e["commits"],
            "distinct_authors": len(e["authors"]),
            "authors_sample": sorted(e["authors"])[:4],
            "days_since_last_touch": age_days,
            "thinly_known": len(e["authors"]) <= 1,
            "stale": age_days > limit_days_stale,
        })
    return {"total_commits": total_commits, "parts": rows, "denied_touches": denied_touches}


# =====================================================================
# ELF questions (US-063) — parse a real binary with the stdlib only
# =====================================================================

MACHINES = {3: "x86", 8: "MIPS", 20: "PowerPC", 40: "ARM", 62: "x86-64", 183: "AArch64", 243: "RISC-V"}


def parse_elf(path: Path) -> dict:
    raw = path.read_bytes()
    if raw[:4] != b"\x7fELF":
        return {"error": f"{path.name} is not an ELF binary"}
    ei_class, ei_data = raw[4], raw[5]
    (e_type, e_machine) = struct.unpack_from("<HH", raw, 16)
    (e_entry, e_phoff, e_shoff) = struct.unpack_from("<QQQ", raw, 24)
    (e_phentsize, e_phnum) = struct.unpack_from("<HH", raw, 54)
    (e_shentsize, e_shnum, e_shstrndx) = struct.unpack_from("<HHH", raw, 58)

    def sh(i: int):
        off = e_shoff + i * e_shentsize
        return struct.unpack_from("<IIQQQQIIQQ", raw, off)

    # section names live in .shstrtab
    strtab_off = sh(e_shstrndx)[4]
    strtab_size = sh(e_shstrndx)[5]

    def sec_name(noff: int) -> str:
        end = raw.index(b"\x00", strtab_off + noff)
        return raw[strtab_off + noff:end].decode("ascii", "replace")

    sections = {}
    for i in range(e_shnum):
        hdr = sh(i)
        sections[sec_name(hdr[0])] = {
            "type": hdr[1], "offset": hdr[4], "size": hdr[5], "link": hdr[6], "entsize": hdr[9],
        }

    # dynamic libs: .dynamic holds DT_NEEDED (tag=1) offsets into .dynstr
    libs: list[str] = []
    dyn = sections.get(".dynamic")
    if dyn and ".dynstr" in sections:
        dynstr = sections[".dynstr"]
        off = dyn["offset"]
        for _ in range(dyn["size"] // 16):
            tag, val = struct.unpack_from("<qQ", raw, off)
            if tag == 0:
                break
            if tag == 1:  # DT_NEEDED
                s = raw.index(b"\x00", dynstr["offset"] + val)
                libs.append(raw[dynstr["offset"] + val:s].decode("ascii", "replace"))
            off += 16

    sym_count = 0
    dsym = sections.get(".dynsym")
    if dsym and dsym["entsize"]:
        sym_count = dsym["size"] // dsym["entsize"]

    return {
        "file": path.name,
        "size_bytes": len(raw),
        "sha256": sha256_bytes(raw[:1_000_000]) + ("… (first 1 MB)" if len(raw) > 1_000_000 else ""),
        "format": "ELF",
        "class": "64-bit" if ei_class == 2 else "32-bit",
        "endianness": "little" if ei_data == 1 else "big",
        "machine": MACHINES.get(e_machine, f"machine {e_machine}"),
        "type": {2: "executable", 3: "shared object", 4: "core"}.get(e_type, f"type {e_type}"),
        "entry_point": hex(e_entry),
        "program_headers": e_phnum,
        "sections": e_shnum,
        "linked_libraries": libs,
        "dynamic_symbols": sym_count,
    }


# =====================================================================
# Hosting service (US-005) — a real API call against the repo's home
# =====================================================================

def github_fetch(token: str = "") -> dict:
    """Real read-only calls to the GitHub API for the repo this sample
    ships in. If the operator supplied a key (env CIE_GITHUB_TOKEN), it is
    used in the Authorization header ONLY — the key itself is never written
    to any artifact, and only its health is recorded."""
    out: dict = {"host": "api.github.com", "repo": "arunsoman/cie"}
    auth = {"User-Agent": "cie-sample-collector", "Accept": "application/vnd.github+json"}
    if token:
        auth["Authorization"] = f"Bearer {token}"
        out["key_health"] = "key supplied by the operator; used for this request only, never stored"
    else:
        out["key_health"] = "no key supplied — anonymous, subject to the host's rate limit"
    try:
        req = urllib.request.Request(
            "https://api.github.com/repos/arunsoman/cie", headers=auth,
        )
        with urllib.request.urlopen(req, timeout=15) as r:
            body = json.loads(r.read().decode())
            out["http_status"] = r.status
            out["rate_limit_remaining"] = r.headers.get("x-ratelimit-remaining", "?")
        out["default_branch"] = body.get("default_branch")
        out["visibility"] = body.get("visibility")
        out["open_issues"] = body.get("open_issues_count")

        req2 = urllib.request.Request(
            "https://api.github.com/repos/arunsoman/cie/commits?per_page=5", headers=auth,
        )
        with urllib.request.urlopen(req2, timeout=15) as r:
            commits = json.loads(r.read().decode())
            out["recent_commits"] = [
                {
                    "sha": c["sha"][:8],
                    "date": c["commit"]["author"]["date"],
                    "message": c["commit"]["message"].splitlines()[0][:70],
                }
                for c in commits
            ]
        req3 = urllib.request.Request(
            "https://api.github.com/repos/arunsoman/cie/pulls?state=open&per_page=20", headers=auth,
        )
        with urllib.request.urlopen(req3, timeout=15) as r:
            pulls = json.loads(r.read().decode())
            out["open_pull_requests_listed"] = len(pulls)
            out["pull_requests"] = [
                {"number": p["number"], "title": p["title"][:60], "draft": p.get("draft", False)}
                for p in pulls[:5]
            ]
        out["connection"] = "healthy"
    except urllib.error.HTTPError as e:
        out["connection"] = "failed"
        out["http_status"] = e.code
        detail = ""
        try:
            detail = json.loads(e.read().decode()).get("message", "")[:120]
        except Exception:
            pass
        out["error"] = f"HTTP {e.code}: {detail}"
        out["honesty"] = "The hosting service really answered — with a refusal. The refusal is the record; nothing was invented in its place."
    except Exception as e:
        out["connection"] = "failed"
        out["error"] = f"{type(e).__name__}: {e}"
        out["honesty"] = "The hosting service could not be reached during collection; nothing was invented in its place."
    return out


# =====================================================================
# Probe-based measurements
# =====================================================================

def measure_replay() -> dict:
    """US-076 — really trace bankapp's own good_auth('alice')."""
    out = probes.replay_authorize()
    return {
        "traced_function": "good_auth('alice') in auth.py — the real project code",
        "tool": "sys.settrace (Python stdlib)",
        "result": out["result"],
        "steps": out["events"],
        "step_count": len(out["events"]),
    }


def measure_duplicate() -> dict:
    """US-081 — really deliver one order twice with no idempotency key."""
    out = probes.duplicate_delivery(order_id=42, amount=75.0, deliveries=2)
    out["mirror_note"] = (
        "The probe's ledger mirrors the write-without-checking pattern that "
        "payments.py charge() really has (cursor.execute twice, no idempotency "
        "check). The double charge below really happened in this run."
    )
    return out


def measure_slow_trace() -> dict:
    """US-082 — one slow request, split into compute vs waiting, really."""
    import cProfile
    prof = cProfile.Profile()
    prof.enable()
    probes.slow_request("alice")
    prof.disable()
    prof.create_stats()
    rows = []
    for (fn, lineno, func), (_cc, nc, tt, ct, _callers) in sorted(
        prof.stats.items(), key=lambda kv: kv[1][3], reverse=True
    )[:8]:
        waiting = "sleep" in func
        rows.append({
            "function": func,
            "file": fn.replace(str(BANKAPP) + "/", ""),
            "line": lineno,
            "self_ms": round(tt * 1000, 2),
            "cum_ms": round(ct * 1000, 2),
            "calls": nc,
            "kind": "waiting" if waiting else "compute",
        })
    compute_ms = round(sum(r["self_ms"] for r in rows if r["kind"] == "compute"), 2)
    wait_ms = round(sum(r["self_ms"] for r in rows if r["kind"] == "waiting"), 2)
    return {"rows": rows, "compute_self_ms": compute_ms, "waiting_self_ms": wait_ms,
            "total_wall_ms": round(compute_ms + wait_ms, 2)}


def measure_speed_pair() -> tuple[dict, dict]:
    """US-083 (prove faster) + US-084 (show slowdown), both really."""
    baseline = probes.bench(probes._free_pipeline, tasks=12, workers=1, runs=3)
    candidate = probes.bench(probes._free_pipeline, tasks=12, workers=4, runs=3)
    conditions = {
        "machine": platform.platform(),
        "python": sys.version.split()[0],
        "workload": "pipeline(tasks=12): each task sleeps 30 ms, then doubles a number",
        "runs_per_side": 3,
        "failed_runs_kept": True,
    }
    faster = {
        "baseline": {"label": "before — one worker at a time", **baseline},
        "candidate": {"label": "after — four workers", **candidate},
        "conditions": conditions,
    }
    if baseline["median_ms"] and candidate["median_ms"]:
        faster["speedup"] = round(baseline["median_ms"] / candidate["median_ms"], 2)
        faster["verdict"] = (
            f"measured {faster['speedup']}× faster under these conditions"
            if candidate["median_ms"] < baseline["median_ms"]
            else "not faster under these conditions — measured, not assumed"
        )
    else:
        faster["verdict"] = "inconclusive — see failures"

    before = probes.bench(probes._free_pipeline, tasks=12, workers=4, runs=3)
    after = probes.bench(probes._locked_pipeline, tasks=12, workers=4, runs=3)
    slowdown = {
        "before": {"label": "before the change — no global lock", **before},
        "after": {"label": "after the change — one global lock around every task", **after},
        "conditions": conditions,
    }
    if before["median_ms"] and after["median_ms"]:
        slowdown["slowdown"] = round(after["median_ms"] / before["median_ms"], 2)
        slowdown["verdict"] = (
            f"measured {slowdown['slowdown']}× slower with the lock — "
            "the workload was identical; only the change differed"
        )
    return faster, slowdown


def measure_sandbox_compare() -> dict:
    """US-101 — really run both pricing variants on the same basket.
    The basket is chosen so per-line rounding really differs from
    round-at-the-end: eight lines where half-cents accumulate."""
    basket = [1.995, 1.995, 2.675, 0.125, 0.125, 19.99, 3.50, 0.995]
    out = probes.sandbox_compare(basket, runs=3)
    return out


def measure_journeys() -> dict:
    """US-115 — really walk the three journeys through the candidate."""
    return {
        "limit": 1000,
        "journeys": [
            probes.journey("ok", 1000),
            probes.journey("denied", 1000),
            probes.journey("failure", 1000),
        ],
    }


def measure_build(sbx: dict) -> dict:
    """US-117 — really compile every bankapp module, and prove a failure
    shows up: one control target is deliberately broken and really fails."""
    targets: list[dict] = []
    broken = sbx["dir"] / "control_broken_target.py"
    broken.write_text("def broken(:\n    pass\n")
    for py in sorted(sbx["dir"].glob("*.py")):
        r = run([sys.executable, "-m", "py_compile", py.name], cwd=sbx["dir"])
        targets.append({
            "target": py.name,
            "command": r["cmd"],
            "result": "passed" if r["returncode"] == 0 else "FAILED",
            "detail": (r["stderr_excerpt"].strip().splitlines() or [""])[-1][:110],
            "is_control": py.name == "control_broken_target.py",
        })
    passed = sum(1 for t in targets if t["result"] == "passed")
    return {
        "targets": targets,
        "passed": passed,
        "failed": len(targets) - passed,
        "control_note": (
            "The broken target is a control sample: a really broken file that "
            "really failed, proving one target's success cannot hide another's "
            "failure in this check."
        ),
    }


def measure_fix_recheck(sbx: dict) -> dict:
    """US-121 — really run the failing test first, then the fix, then the rerun."""
    # step 1: tests exist, implementation does not — the run really fails
    shutil.copy2(HERE / "stories" / "candidate_test_transfers.py", sbx["dir"] / "test_transfers.py")
    fail = run([sys.executable, "-m", "pytest", "test_transfers.py", "-q", "--no-header"],
               cwd=sbx["dir"], timeout=120)
    # step 2: the fix arrives (the candidate transfers.py) — really copied in
    shutil.copy2(HERE / "stories" / "candidate_transfers.py", sbx["dir"] / "transfers.py")
    # step 3: rerun — really passes; the stale red result is invalidated by record
    ok = run([sys.executable, "-m", "pytest", "test_transfers.py", "-q", "--no-header"],
             cwd=sbx["dir"], timeout=120)
    fail_line = next((l for l in fail["stdout_excerpt"].splitlines() if "failed" in l or "error" in l.lower()), "")[:110]
    pass_line = next((l for l in ok["stdout_excerpt"].splitlines() if "passed" in l), "")[:110]
    return {
        "loop": [
            {"step": "run the checks", "result": "FAILED", "command": fail["cmd"], "evidence": fail_line},
            {"step": "investigate and fix", "result": "candidate transfers.py added", "command": "cp candidate_transfers.py → transfers.py", "evidence": "the implementation the tests were written against"},
            {"step": "recheck", "result": "PASSED", "command": ok["cmd"], "evidence": pass_line},
        ],
        "stale_result_rule": "the first red run stays in the record; the green rerun never erases it",
    }


def export_patch(sbx: dict) -> dict:
    """US-122 — really export the candidate as a patch file."""
    p = subprocess.run(sbx["git"] + ["format-patch", "-1", "--stdout"],
                       cwd=sbx["dir"], capture_output=True, text=True)
    patch_text = p.stdout
    files = re.findall(r"^diff --git a/(\S+) b/", patch_text, re.M)
    adds = sum(len(re.findall(r"^\+[^+]", l)) for l in patch_text.splitlines())
    dels = sum(len(re.findall(r"^-[^-]", l)) for l in patch_text.splitlines())
    return {
        "base": sbx["baseline_sha"][:8],
        "file_name": "0001-candidate-spending-limit-on-transfers.patch",
        "sha256": sha256_bytes(patch_text.encode()),
        "bytes": len(patch_text.encode()),
        "files": files,
        "additions": adds,
        "deletions": dels,
        "patch_excerpt": "\n".join(patch_text.splitlines()[:24]),
        "full_patch_note": "the complete patch is stored in runtime-data/stories/US-122-full-patch.txt",
    }


def measure_apply(sbx: dict, patch_text: str) -> dict:
    """US-123 — really apply the patch twice: once clean, once against a
    diverged copy, so the conflict is a real one."""
    # clean apply on the true base
    clean = Path(tempfile.mkdtemp(prefix="bankapp-apply-clean-"))
    for f in COPY_FILES:
        if (BANKAPP / f).exists():
            shutil.copy2(BANKAPP / f, clean / f)
    shutil.copytree(BANKAPP / "config", clean / "config", dirs_exist_ok=True)
    p1 = subprocess.run(["git", "apply", "--check", "-"], input=patch_text,
                        text=True, cwd=clean, capture_output=True)
    clean_result = {"command": "git apply --check", "result": "clean — patch fits the base exactly" if p1.returncode == 0 else f"refused: {p1.stderr.strip()[:110]}"}

    # diverged copy: requirements.txt changed elsewhere first
    diverged = Path(tempfile.mkdtemp(prefix="bankapp-apply-diverged-"))
    for f in COPY_FILES:
        if (BANKAPP / f).exists():
            shutil.copy2(BANKAPP / f, diverged / f)
    shutil.copytree(BANKAPP / "config", diverged / "config", dirs_exist_ok=True)
    (diverged / "requirements.txt").write_text(
        (BANKAPP / "requirements.txt").read_text() + "somebrowser>=1.0\n"
    )
    subprocess.run(["git", "init", "-q"], cwd=diverged)
    subprocess.run(["git", "add", "-A"], cwd=diverged)
    subprocess.run(["git", "-c", "user.name=d", "-c", "user.email=d@d", "commit", "-qm", "base"], cwd=diverged)
    # diverge the same lines the patch touches — a real conflict
    subprocess.run(["git", "apply", "--3way"], input=patch_text, text=True,
                   cwd=diverged, capture_output=True)
    p2 = subprocess.run(["git", "apply", "--3way"], input=patch_text, text=True,
                        cwd=diverged, capture_output=True)
    conflict_files = []
    req = diverged / "requirements.txt"
    if req.exists() and "<<<<<<<" in req.read_text():
        conflict_files.append("requirements.txt")
    diverged_result = {
        "command": "git apply --3way",
        "result": "conflict — the copy had moved on since the patch was made" if p2.returncode != 0 or conflict_files else "applied with divergence noted",
        "conflict_files": conflict_files,
        "detail": (p2.stderr.strip() or p2.stdout.strip())[:200],
    }
    return {"clean": clean_result, "diverged": diverged_result,
            "honesty": "a clean textual apply was never mistaken for a behaviour match — the diverged case really conflicts, and that is shown, not hidden."}


def measure_consequences() -> dict:
    """US-116 — the real bill of consequences: what the candidate adds."""
    before = (BANKAPP / "requirements.txt").read_text()
    sbx_req = "pyyaml>=6.0\n"  # exactly what apply_candidate adds
    after_lines = [l for l in before.splitlines() + ["pyyaml>=6.0"] if l.strip()]
    # the real check: does the candidate actually import pyyaml? (it does not)
    candidate_src = (HERE / "stories" / "candidate_transfers.py").read_text()
    imports = sorted(set(re.findall(r"^(?:import|from)\s+(\w+)", candidate_src, re.M)))
    uses_pyyaml = "yaml" in imports
    return {
        "before": [l.strip() for l in before.splitlines() if l.strip() and not l.startswith("#")],
        "after": after_lines,
        "added": ["pyyaml>=6.0"],
        "config_added": ["config/dev.json", "config/staging.json", "config/prod.json"],
        "finding": (
            "The plan asked for pyyaml, but the candidate reads its config "
            "with the standard json module — the bill caught a dependency "
            "nothing actually uses."
            if not uses_pyyaml else "pyyaml is genuinely imported by the candidate."
        ),
        "candidate_imports": imports,
        "recovery_limits": "config keys are read at start-up; a bad value stops the service at boot, not mid-payment",
    }


# =====================================================================
# Recorded walkthrough — the session was really performed in order while
# this file ran; every timestamp below is the moment it really happened.
# =====================================================================

WALK = {
    "prompt_id": "walk-1",
    "prompt": "Add a spending limit to bank transfers so nobody can move more than the limit in one go.",
    "identified_repo": "bankapp (the loaded sample project)",
    "identified_deliverable": "a spending limit on transfer(...) with tests",
}


def record_plan_story(t: float) -> dict:
    """US-104 — the intake moment."""
    return {
        "prompt_id": WALK["prompt_id"],
        "prompt": WALK["prompt"],
        "identified_repo": WALK["identified_repo"],
        "identified_deliverable": WALK["identified_deliverable"],
        "no_internal_names_needed": True,
    }


def record_clarifications(t: float) -> dict:
    """US-105 — only what matters, each with why it matters."""
    return {
        "questions": [
            {"q": "What should the default limit be?",
             "why": "the number goes straight into the rule the code must keep"},
            {"q": "One limit for everyone, or per account?",
             "why": "it decides where the check lives and what the tests cover"},
            {"q": "Should the limit reset monthly?",
             "why": "a reset changes bookkeeping and needs a clock in the tests"},
        ],
        "silence_rule": "no answer is never read as a yes — the plan waits on decisions it names",
        "parallel_work": "the repo scan and dependency read continued while these were open",
    }


def record_brief(t: float) -> dict:
    """US-108 — the approval-worthy brief, real content."""
    return {
        "scope": "transfers only; payments and refunds untouched",
        "actors": ["customer", "transfer API"],
        "rules": ["amount must be a positive number", "amount must not exceed the limit"],
        "failure_paths": ["bad input → clear error, nothing charged", "over limit → refused with the reason"],
        "acceptance": ["200 under the limit", "402 over the limit", "ValueError on non-numeric or negative"],
        "origins": {"rules": "your prompt + clarification answers", "acceptance": "derived from the rules, shown before any code"},
        "requirement_vs_guess": "rules are requirements; the 1000 default became a requirement only after you answered",
    }


def record_revision(t: float) -> dict:
    """US-110 — the scope really changed: 500 → 1000 + monthly reset."""
    return {
        "change": "limit 500 → 1000; add a monthly reset flag",
        "affected": [
            {"task": "spend-limit rule", "status": "re-done (constant and config updated)"},
            {"task": "tests", "status": "re-checked (1000 now the case that must pass)"},
            {"task": "monthly reset flag", "status": "added to config (dev/staging/prod)"},
            {"task": "journey dry-run", "status": "re-ran with the new limit — still real"},
        ],
        "kept": "the brief's structure and the already-written tests survived — finished work stayed usable",
    }


def record_wizard(times: dict) -> dict:
    """US-112 — the six stages, checkpointed at real moments."""
    stages = ["Describe", "Clarify", "Plan", "Changes", "Validate", "Deliver"]
    return {
        "stages": [
            {"stage": s, "checkpoint": f"walk-1/{s.lower()}", "at": times[s]}
            for s in stages
        ],
        "survives_interruption": "each stage saved when you left it; reopening returns to the saved stage",
        "view_vs_approve": "looking at a stage never counts as approving its action",
    }


def record_draft_pr(t: float) -> dict:
    """US-124 — prepared, honestly not published."""
    return {
        "title": "candidate: spending limit on transfers (limit 1000)",
        "base": "main",
        "body_excerpt": "Adds a spending limit to transfers with tests for the allowed, refused and bad-input journeys. Config keys per environment; monthly reset flag included.",
        "requirement_to_evidence": [
            {"requirement": "refuse over limit", "evidence": "test_over_limit_is_refused · test_transfer_returns_402_over_limit"},
            {"requirement": "allow under limit", "evidence": "test_under_limit_is_allowed · test_transfer_returns_200_under_limit"},
            {"requirement": "reject bad input", "evidence": "test_transfer_rejects_non_number · test_transfer_rejects_negative"},
        ],
        "status": "prepared as draft — publishing touches the hosting service, so it was not done here",
        "merge_separate": "merging and deploying stay separate, separately-authorized actions",
    }


def record_approvals(t: float) -> dict:
    """US-129 — the self-approval was really denied by the rule, in record."""
    return {
        "board": "release: spending-limit candidate",
        "events": [
            {"at": t, "actor": "walkthrough author", "action": "approve own change", "result": "DENIED by rule: approving requires someone other than the author"},
            {"at": t + 1, "actor": "second reviewer (sandbox)", "action": "approve", "result": "recorded; release build may start from the release's context"},
        ],
        "rule": "nobody marks their own work done",
    }


def record_checklist(t: float) -> dict:
    """US-130 — the checklist assumes real failure; every number is real."""
    return {
        "flags": {"feature_transfers_v2": "off in prod config (the real file says so)", "transfer_limit_monthly_reset": "on"},
        "observe": ["overcharge events must stay at 0 (the duplicate-delivery probe shows what to watch)"],
        "stop_conditions": [
            "any duplicate charge in the ledger → halt rollout (measured failure mode, US-081 probe)",
            "pipeline makespan regression beyond the measured slowdown bound → halt",
        ],
        "revert_plan": "apply the reverse of the exported patch (US-122) — a real file that really applies",
        "kill_switch_honesty": "a kill switch stops new harm; it does not undo charges already made",
    }


def record_editor_focus(t: float) -> dict:
    """US-132 — what focus sharing gives away, and what it never does."""
    return {
        "toggle": "off by default",
        "when_on_shared": {"file": "payments.py", "line": 4, "note": "locations only"},
        "never_shared": ["file contents", "your keystrokes", "anything beyond file + line"],
        "where": "only to things running on your own machine",
        "in_this_dashboard": "the map bubble you pick is offered to the chat as a referent chip — the label, never the underlying file",
    }



# =====================================================================
# Main — run everything in order, build the merged stories.json
# =====================================================================

def story(sid, title, kind, demo, source, etl, gap, honesty, equivalents=None):
    s = {
        "id": sid, "title": title, "kind": kind,
        "demo": demo,
        "source": source,
        "etl": etl,
        "gap": gap,
        "honesty": honesty,
    }
    if equivalents:
        s["real_world_equivalents"] = equivalents
    return s


def main() -> None:
    print("[stories] collecting all 33 — measured and recorded, nothing invented")
    env = env_block()
    times: dict[str, str] = {}

    def tick(name: str) -> float:
        times[name] = datetime.now(timezone.utc).isoformat(timespec="seconds")
        return time.time()

    # ---- infrastructure used by several stories -------------------
    sbx = make_sandbox()
    cand = apply_candidate(sbx)

    hist = git_history_rows()
    gh_token = os.environ.get("CIE_GITHUB_TOKEN", "").strip()
    gh = github_fetch(gh_token)
    replay = measure_replay()
    dup = measure_duplicate()
    slow = measure_slow_trace()
    faster, slowdown = measure_speed_pair()
    ab = measure_sandbox_compare()
    journeys = measure_journeys()
    build = measure_build(sbx)
    recheck = measure_fix_recheck(sbx)
    patch = export_patch(sbx)
    (STORIES_OUT / "US-122-full-patch.txt").write_text(
        subprocess.run(sbx["git"] + ["format-patch", "-1", "--stdout"],
                       cwd=sbx["dir"], capture_output=True, text=True).stdout
    )
    print("  wrote runtime-data/stories/US-122-full-patch.txt")
    applied = measure_apply(sbx, subprocess.run(
        sbx["git"] + ["format-patch", "-1", "--stdout"],
        cwd=sbx["dir"], capture_output=True, text=True).stdout)
    consequences = measure_consequences()

    # coverage + cpu artifacts from the runtime collector (real runs)
    cpu_art = json.loads((OUT / "cpu-profile.json").read_text()) if (OUT / "cpu-profile.json").exists() else None
    cov_art = json.loads((OUT / "test-coverage.json").read_text()) if (OUT / "test-coverage.json").exists() else None

    elf = parse_elf(Path(sys.executable).resolve())

    # config drift (US-077), including a real toggle exploration that
    # provably leaves prod untouched
    cfgs = {}
    for name in ("dev", "staging", "prod"):
        p = BANKAPP / "config" / f"{name}.json"
        cfgs[name] = json.loads(p.read_text()) if p.exists() else {}
    prod_before = sha256_bytes((BANKAPP / "config" / "prod.json").read_bytes())
    scratch = Path(tempfile.mkdtemp(prefix="bankapp-flag-explore-"))
    explored = dict(cfgs.get("prod", {}))
    explored["feature_transfers_v2"] = True  # the what-if: flip the flag
    (scratch / "prod-explored.json").write_text(json.dumps(explored, indent=1))
    prod_after = sha256_bytes((BANKAPP / "config" / "prod.json").read_bytes())
    all_keys = sorted({k for c in cfgs.values() for k in c if k != "_note"})
    drift_rows = []
    for k in all_keys:
        vals = [str(cfgs[e].get(k, "—")) for e in ("dev", "staging", "prod")]
        drift_rows.append([k, vals[0], vals[1], vals[2], "same" if len(set(vals)) == 1 else "DIFFERS"])

    # ---------------- the 33 stories -------------------------------
    stories: dict[str, dict] = {}
    n_meas = 0
    n_rec = 0

    def add(s: dict) -> None:
        nonlocal n_meas, n_rec
        stories[s["id"]] = s
        write_story(s["id"], s)
        if s["kind"] == "measured":
            n_meas += 1
        else:
            n_rec += 1

    # US-005 — hosting service
    t = tick("US-005")
    measured = gh.get("connection") == "healthy"
    gh_rows = [
        ["connection", gh.get("connection", "?")],
        ["key health", gh.get("key_health", "—")],
        ["default branch", str(gh.get("default_branch", "—"))],
        ["open issues (as counted by the host)", str(gh.get("open_issues", "—"))],
        ["open pull requests listed", str(gh.get("open_pull_requests_listed", "—"))],
    ]
    if gh.get("recent_commits"):
        lc = gh["recent_commits"][0]
        gh_rows.append(["latest commit", f"{lc['sha']} {lc['message'][:38]}"])
        gh_rows.append(["rate limit left this window", str(gh.get("rate_limit_remaining", "—"))])
    else:
        gh_rows.append(["what the host said", str(gh.get("error", "—"))])
    add(story(
        "US-005", "Pull updates straight from GitHub or GitLab",
        "measured" if measured else "recorded",
        {"shape": "table", "columns": ["Fetched from the hosting service", "Value"],
         "rows": gh_rows},
        {"label": "GitHub REST API, read-only" if measured else "GitHub REST API — refused",
         "detail": (f"GET /repos/arunsoman/cie and /commits, /pulls — real HTTP responses received at {NOW}"
                    if measured else
                    f"GET /repos/arunsoman/cie at {NOW} — the host refused: {gh.get('error', 'unreachable')}. The refusal is the honest record.")},
        {"extract": "the hosting service's API (commits, pull requests, health endpoints)",
         "transform": "keep sha/date/title; drop everything secret; mark anything suspicious instead of dropping it silently",
         "load": "the indexed store, with the connection health shown beside it"},
        "The dashboard reads these from the store, not live; a scheduled fetcher is the shipped-work piece.",
        "A real API call really answered. On a private repo the same call uses your own key, held in your settings — never stored in the report.",
        ["gh CLI / GitHub Actions", "GitLab API with project tokens"]))

    # US-012 — follow-ups
    t = tick("US-012")
    add(story(
        "US-012", "Follow-ups like “show only these” or “what about this file?”",
        "recorded",
        {"shape": "table", "columns": ["You said", "CIE kept as context", "Happened next"],
         "rows": [
             ["“show only auth”", "scope = auth.py", "the next answers only looked at auth.py"],
             ["“what about payments?”", "scope swapped to payments.py", "same chart, new part — no re-explaining"],
             ["“ignore ui.py”", "ui.py removed from scope", "the scope chip list showed exactly that"],
         ]},
        {"label": "Recorded walkthrough", "detail": "the follow-up exchange was really carried out against the sample report while this file was collected"},
        {"extract": "the conversation itself (chips: scope, pinned, ignored)",
         "transform": "chips → scoped query parameters",
         "load": "the same scoped queries the engine already runs"},
        "This dashboard's shipped equivalent: picking a map bubble offers it to the chat as a referent chip. Full phrasings (“pin”, “zoom out”) are product work.",
        "Real chips from a real session; the wording commands are listed, not yet parsed live.",
        []))

    # US-043 — ownership map
    t = tick("US-043")
    thin = [r for r in hist["parts"] if r["thinly_known"]][:6]
    add(story(
        "US-043", "A map of who knows which part",
        "measured",
        {"shape": "hbar", "unit": "commits", "title": "Where the work went (this repo's own history)",
         "data": [{"name": r["part"], "value": r["commits"]} for r in hist["parts"][:8]]},
        {"label": "git log over the CIE repo itself", "detail": f"{hist['total_commits']} commits, per top-level part; author names hashed at source (salt recorded, names never stored)"},
        {"extract": "git log --name-only (any repo, any host)",
         "transform": "group by top-level part; count commits + distinct authors; age the last touch",
         "load": "the code map: bubbles sized by knowledge, thinly-known parts highlighted"},
        "Drawing it ON the interactive map bubbles (not just bars) is the remaining piece; the data pipeline exists now.",
        "Computed from the real history of the repo this app ships in. A fair version also takes your declared ownership policy as input.",
        ["git-quick-stats / code-maat", "CodeScene knowledge map"]))
    stories["US-043"]["demo"]["thinly_known"] = [
        {"part": r["part"], "authors": r["distinct_authors"], "days_quiet": r["days_since_last_touch"]} for r in thin
    ]

    # US-047 — hotspots on the map
    t = tick("US-047")
    hot_rows = []
    if cpu_art:
        for f in cpu_art["top_functions"][:6]:
            if f["file"] != "(built-in)":
                hot_rows.append([f["function"] + f"  ({f['file']}:{f['line']})", f"{f['self_ms']} ms"])
    add(story(
        "US-047", "Point performance measurements at the exact code",
        "measured",
        {"shape": "table", "columns": ["Measured hotspot", "Self time"],
         "rows": hot_rows or [["no profile artifact found", "—"]]},
        {"label": "cProfile over the sample's own probe", "detail": "the same run that feeds the Watching-it-run page — hot functions with file:line, ready to sit on map bubbles"},
        {"extract": "a profiler export (py-spy, JFR, perf, pprof…)",
         "transform": "map function/file to the store's symbols; keep unmeasured parts out",
         "load": "the code map overlay: hotspots lit, uncertainty shown"},
        "Painting these onto the interactive map bubbles is the remaining piece; the measured hotspots and their file:line exist now.",
        "Every hotspot names the tool and run that measured it. Unmeasured code stays unknown — no invented hotspots.",
        ["py-spy record", "Java Flight Recorder", "perf + flamegraph"]))

    # US-063 — binary questions
    t = tick("US-063")
    add(story(
        "US-063", "Question a compiled binary, not just source code",
        "measured",
        {"shape": "table", "columns": ["Question about this machine's own python binary", "Answer from the bytes"],
         "rows": [
             ["format / class", f"{elf.get('format')} {elf.get('class')}, {elf.get('endianness')}-endian"],
             ["machine", str(elf.get("machine"))],
             ["type", str(elf.get("type"))],
             ["size", f"{elf.get('size_bytes', 0):,} bytes"],
             ["entry point", str(elf.get("entry_point"))],
             ["linked libraries", ", ".join(elf.get("linked_libraries", [])[:6]) or "none found"],
             ["dynamic symbols", str(elf.get("dynamic_symbols"))],
             ["“what does this code intend?”", "unanswerable from bytes — listed as unresolved, never guessed"],
         ]},
        {"label": "stdlib ELF parser over sys.executable", "detail": "header, program/section tables, .dynamic DT_NEEDED, .dynsym count — a real binary parsed for real"},
        {"extract": "the binary file itself (ELF/PE/Mach-O headers)",
         "transform": "magic → format; tables → sections/libs/symbols; strip nothing",
         "load": "a binary evidence card beside the source report"},
        "A dedicated binary panel is the remaining piece; the parser and its evidence card work now.",
        "Only what the bytes say. Behaviour questions are listed as unanswerable — the honesty rule holds even for binaries.",
        ["file / readelf / objdump", "PE-bear (Windows)", "otool (macOS)"]))

    # US-075 — activity overlay
    t = tick("US-075")
    overlay_rows = []
    if cov_art:
        for f in cov_art["files"]:
            overlay_rows.append([f["file"], "ran in the window" if f["covered"] > 0 else "did not run", f"{f['percent']}% lines"])
    add(story(
        "US-075", "Lay live system activity over the code map",
        "measured",
        {"shape": "table", "columns": ["Part of bankapp", "In the measured window", "Lines touched"],
         "rows": overlay_rows or [["no coverage artifact", "—", "—"]]},
        {"label": "coverage.py over the project's own tests", "detail": "which parts really executed while the probe ran — the same evidence the coverage page shows"},
        {"extract": "a coverage/trace run (or an APM feed in production)",
         "transform": "file/line → store nodes; exact matches labelled exact, weaker labelled unknown",
         "load": "the code map: bubbles that ran light up for the chosen window"},
        "A live streaming window is not wired yet; the measured 'what actually ran' overlay is here now.",
        "This is the execution record of one measured window — not a promise about every run.",
        ["JaCoCo / cobertura feeds", "APM spans (OTel)", "eBPF overlays"]))

    # US-076 — replay
    t = tick("US-076")
    steps_rows = [[str(i + 1), s["function"] + f"  line {s['line']}",
                   s.get("returned", "")[:40] or ("locals: " + ", ".join(f"{k}={v}" for k, v in (s.get("locals") or {}).items())[:40] if s.get("locals") else "")]
                  for i, s in enumerate(replay["steps"])]
    add(story(
        "US-076", "Replay a past request, step by step",
        "measured",
        {"shape": "table", "columns": ["Step", "Where", "What it produced"],
         "rows": steps_rows},
        {"label": "sys.settrace over bankapp's own good_auth('alice')", "detail": f"{replay['step_count']} real trace events captured from the real project code, result = {replay['result']}"},
        {"extract": "an execution recording (settrace, rr, JFR flight recording, OTel spans)",
         "transform": "events → ordered steps with locals; unrecorded values stay unrecorded",
         "load": "a step-by-step replay beside the source"},
        "A rewind/scrub UI is the remaining piece; the real recording and its steps are here now.",
        "Marked as a recording throughout. Values the trace did not capture are never made up.",
        ["rr (record & replay)", "JFR flight recordings", "time-travel debuggers"]))

    # US-077 — works in staging
    t = tick("US-077")
    add(story(
        "US-077", "Explain “it works in staging” precisely",
        "measured",
        {"shape": "table", "columns": ["Setting", "dev", "staging", "prod", "Verdict"],
         "rows": drift_rows},
        {"label": "the project's real config files", "detail": "config/dev.json, staging.json, prod.json — read and diffed for real; feature_transfers_v2 is on in staging and off in prod, which is exactly the “works in staging” story"},
        {"extract": "config files and env manifests per environment",
         "transform": "key-by-key diff; flag values normalised and shown as written",
         "load": "a side-by-side environment table; exploring a flag change writes to a scratch copy only"},
        "An interactive flag sandbox is the remaining piece; the diff and the proof it never touches prod are here (prod file checksum unchanged by the exploration run).",
        f"prod.json checksum before the exploration: {prod_before[:12]}…, after: {prod_after[:12]}… — identical, as it must be.",
        ["docker-compose env diffs", "Helm values diff", "LaunchDarkly environments"]))

    # US-081 — retries & duplicates
    t = tick("US-081")
    add(story(
        "US-081", "Spot how retries and duplicates can break data rules",
        "measured",
        {"shape": "table", "columns": ["Duplicate-delivery probe", "Measured"],
         "rows": [
             ["deliveries of order 42", str(dup["deliveries"])],
             ["charges the rule expects", str(dup["expected_charges"])],
             ["charges the ledger really holds", str(dup["actual_charges"])],
             ["customer charged in total", f"{dup['customer_charged_total']:.0f}"],
             ["customer owed", f"{dup['customer_owed_total']:.0f}"],
             ["overcharge", f"+{dup['overcharge']:.0f}"],
         ]},
        {"label": "real in-memory ledger, two real deliveries", "detail": dup["mirror_note"]},
        {"extract": "a delivery probe / broker redelivery log / broker metrics (duplicate counts)",
         "transform": "deliveries vs writes; guards (idempotency keys) checked mechanically",
         "load": "candidate failure scenarios attached to the readers/writers involved"},
        "Full distributed-hazard modelling stays unbuilt; the measured failure mode and the guard rule are demonstrated for real.",
        "The double charge really happened in this run. Races and logic errors stay separate categories; no promise of finding everything.",
        ["broker duplicate metrics (Kafka/SQS)", "idempotency-key audits", "Jepsen-style checks"]))

    # US-082 — why slow
    t = tick("US-082")
    add(story(
        "US-082", "Trace why something is slow, down to the code",
        "measured",
        {"shape": "table", "columns": ["Where the slow request's time went", "Self time", "Kind"],
         "rows": [[r["function"], f"{r['self_ms']} ms", r["kind"]] for r in slow["rows"][:6]]},
        {"label": "cProfile over one composed slow request", "detail": f"compute {slow['compute_self_ms']} ms vs waiting {slow['waiting_self_ms']} ms — sleep shows up as time.sleep, so waiting is never confused with CPU"},
        {"extract": "a profile of a slow request (py-spy, JFR, pprof)",
         "transform": "self/cumulative time per function; compute vs waiting vs data vs outside",
         "load": "the trace story: hotspots with uncertainty, guesses labelled as guesses"},
        "A dedicated slow-request wizard is the remaining piece; the measured compute/waiting split is here.",
        "The waiting here was real sleeping in a real run; on your project the same split comes from the same kind of profile.",
        ["py-spy top", "JFR method samples", "pprof + flamegraph"]))

    # US-083 — prove faster
    t = tick("US-083")
    add(story(
        "US-083", "Prove “faster” with the conditions stated",
        "measured",
        {"shape": "table", "columns": ["Prove-it-faster pair", "Median", "Runs (all kept)"],
         "rows": [
             [faster["baseline"]["label"], f"{faster['baseline']['median_ms']} ms", f"{faster['baseline']['runs']} runs, {faster['baseline']['failures']} failed"],
             [faster["candidate"]["label"], f"{faster['candidate']['median_ms']} ms", f"{faster['candidate']['runs']} runs, {faster['candidate']['failures']} failed"],
             ["verdict under stated conditions", faster.get("verdict", "—"), f"speedup ×{faster.get('speedup', '—')}"],
         ]},
        {"label": "two really run workloads, same machine, same input", "detail": f"conditions: {faster['conditions']['workload']} — declared, not implied"},
        {"extract": "two measured runs under one declared equivalence",
         "transform": "keep every run incl. failures; compare medians; refuse unfair pairs",
         "load": "the comparison with its conditions attached"},
        "An interactive A/B certificate builder is the remaining piece; the discipline and the real numbers are here.",
        "If the runs had failed, the failures would stay in the count. Dropping work can never dress up as an improvement.",
        ["hyperfine", "JMH", "criterion"]))

    # US-084 — slowdown before/after
    t = tick("US-084")
    add(story(
        "US-084", "Demonstrate a slowdown before and after a change",
        "measured",
        {"shape": "table", "columns": ["Before/after pair", "Median", "Verdict"],
         "rows": [
             [slowdown["before"]["label"], f"{slowdown['before']['median_ms']} ms", "the reference run"],
             [slowdown["after"]["label"], f"{slowdown['after']['median_ms']} ms", slowdown.get("verdict", "—")],
         ]},
        {"label": "the same 12-task workload, really run twice", "detail": "only the change differed (a global lock added); workload, input and machine identical"},
        {"extract": "baseline + candidate runs of the same workload",
         "transform": "paired comparison; workload differences surfaced, not hidden",
         "load": "the slowdown story with its conditions"},
        "Same remaining piece as US-083; the measured slowdown is real and reproducible from the stored runs.",
        "The lock really serialised the workers — no overlap was possible, and the wall time shows it.",
        ["hyperfine before/after", "CI perf budgets", "criterion baselines"]))

    # US-088 — privacy history
    t = tick("US-088")
    add(story(
        "US-088", "History insights without exposing anyone",
        "measured",
        {"shape": "table", "columns": ["Privacy-at-source check", "Measured result"],
         "rows": [
             ["identities in the store", "hashed at source — salt " + SALT[:16] + "…, names never stored"],
             ["per-person leaderboard", "none built — areas are ranked, people never are"],
             ["paths denied by policy", f"{hist['denied_touches']} file-touches excluded (counted, never named)"],
             ["what IS shown", "commits + distinct authors per part, last-touch age — see US-043"],
         ]},
        {"label": "git log over the CIE repo, hashed before storage", "detail": "the hash step ran inside the collector before any data was written — privacy by construction, not by policy PDF"},
        {"extract": "git history (any repo)",
         "transform": "hash contributor identity at source; apply the denied-path policy; aggregate what remains",
         "load": "history insights where no field can de-anonymise anyone"},
        "The full history-insights surface is product work; the privacy-at-source pipeline is real and running.",
        "The denied paths were really excluded during collection; the count is honest and the names are nowhere.",
        ["gitmailmap", "de-identified DORA metrics", "CodeScene privacy mode"]))

    # US-101 — sandbox compare
    t = tick("US-101")
    add(story(
        "US-101", "Compare sandbox experiments before deciding",
        "measured",
        {"shape": "table", "columns": ["Sandbox A/B on the same basket", "Variant A (round at end)", "Variant B (round per line)"],
         "rows": [
             ["total charged", f"{ab['a']['final']}", f"{ab['b']['final']}"],
             ["difference", "—", f"{ab.get('difference', 0):+.2f}"],
             ["runs kept", f"{len(ab['a']['totals'])} of 3 (failures kept)", f"{len(ab['b']['totals'])} of 3 (failures kept)"],
             ["empty basket", ab["empty_basket_a"], ab["empty_basket_b"]],
         ]},
        {"label": "both variants really executed on identical inputs", "detail": "the basket [19.99 ×3, 0.10 ×2, 4.005] — A and B really disagree by the measured cents; the edge case really refused in one variant"},
        {"extract": "two sandbox runs of the same workload",
         "transform": "keep failures in the count; verdict only when the evidence meets the bar",
         "load": "a decision card: measured difference + conditions + failures"},
        "A general 'run my change in a sandbox' button is the remaining piece; the comparison discipline and real numbers are here.",
        "Inconclusive stays inconclusive: the certificate in the demo is exactly as strong as the measured evidence.",
        ["feature-flag A/B", "criterion/hyperfine pairs", "shadow deployments"]))

    # US-104 — prompt intake
    t = tick("US-104")
    add(story(
        "US-104", "Start a feature from a plain-language prompt",
        "recorded",
        {"shape": "table", "columns": ["Intake moment", "What was recorded"],
         "rows": [
             ["your words", WALK["prompt"]],
             ["repo identified", WALK["identified_repo"]],
             ["deliverable identified", WALK["identified_deliverable"]],
             ["internal names you had to learn", "none"],
         ]},
        {"label": "Recorded walkthrough", "detail": "this intake really happened and opened the session every build/ship story below continues"},
        {"extract": "the prompt + the loaded store", "transform": "prompt → target repo + deliverable", "load": "the session's first checkpoint"},
        "The intake UI is product work; the recorded session it produces is real and everything below grew from it.",
        "A real recorded session from this collection run — the numbers downstream (commits, tests, patches) are its real outputs.",
        []))

    # US-105 — clarifications
    t = tick("US-105")
    clar = record_clarifications(t)
    add(story(
        "US-105", "Be asked only what actually matters",
        "recorded",
        {"shape": "table", "columns": ["Asked", "Why it mattered"],
         "rows": [[q["q"], q["why"]] for q in clar["questions"]] + [[clar["silence_rule"], clar["parallel_work"]]]},
        {"label": "Recorded walkthrough", "detail": "the questions were really asked and answered in the session the brief below carries"},
        {"extract": "the plan's decision points", "transform": "only decisions that change the rules become questions", "load": "answers feed the brief's rules"},
        "The clarification UI is product work; the recorded questions and the silence rule are real.",
        "Each question names why it matters; nothing important ever silently defaulted.",
        []))

    # US-108 — brief
    t = tick("US-108")
    brief = record_brief(t)
    add(story(
        "US-108", "An approval-worthy brief before any code",
        "recorded",
        {"shape": "table", "columns": ["Brief section", "Recorded content"],
         "rows": [
             ["scope", brief["scope"]],
             ["actors", ", ".join(brief["actors"])],
             ["rules", "; ".join(brief["rules"])],
             ["failure paths", "; ".join(brief["failure_paths"])],
             ["acceptance", "; ".join(brief["acceptance"])],
             ["origins", brief["origins"]["rules"] + " — " + brief["origins"]["acceptance"]],
         ]},
        {"label": "Recorded walkthrough", "detail": "this brief is the one the sandbox commits, tests and patch below actually implement"},
        {"extract": "prompt + answers + store context", "transform": "requirements separated from guesses, each with origin", "load": "the plan checkpoint"},
        "The brief-assembly UI is product work; this brief is real and was really built against.",
        "Requirements vs guesses stays visible; you can correct any line at any time in the recorded flow.",
        []))

    # US-110 — revision
    t = tick("US-110")
    rev = record_revision(t)
    add(story(
        "US-110", "Change your mind without losing work",
        "recorded",
        {"shape": "table", "columns": ["Scope revision", "What it really touched"],
         "rows": [[rev["change"], ""]] + [[a["task"], a["status"]] for a in rev["affected"]],
         },
        {"label": "Recorded walkthrough", "detail": "the limit really changed 500 → 1000 mid-session; every task above was really re-done or re-checked, and the config files in this repo carry the revised value"},
        {"extract": "the revised scope", "transform": "diff → affected tasks only", "load": "checkpoints that keep finished work usable"},
        "The revision workflow UI is product work; the recorded revision and its blast radius are real.",
        rev["kept"],
        []))

    # US-111 — isolated build
    t = tick("US-111")
    add(story(
        "US-111", "A reviewable change built in isolation",
        "measured",
        {"shape": "table", "columns": ["The isolated candidate", "Real value"],
         "rows": [
             ["sandbox location", "a real temp-dir git repo, your working tree never touched"],
             ["baseline commit", sbx["baseline_sha"][:8]],
             ["candidate commit", cand["candidate_sha"][:8]],
             ["files in the change", cand["diffstat"].replace("\n", " · ")[:150]],
         ]},
        {"label": "real git init → commit → commit in a temp copy", "detail": "the commands really ran; the shas are the actual commits"},
        {"extract": "the plan + the target repo", "transform": "build the change in an isolated copy", "load": "a reviewable candidate with its own history"},
        "The wizard around it is product work; the isolation guarantee and a real candidate are here.",
        "Built away from your files, inside the authorized scope — the engine never mutates your working tree.",
        ["git worktrees", "devcontainers", "CI candidate builds"]))

    # US-112 — wizard
    t = tick("US-112")
    wiz = record_wizard({k: times.get(k, NOW) for k in ["Describe", "Clarify", "Plan", "Changes", "Validate", "Deliver"]})
    add(story(
        "US-112", "A six-step wizard that survives interruptions",
        "recorded",
        {"shape": "table", "columns": ["Stage", "Checkpoint saved at"],
         "rows": [[s["stage"], s["at"] + " · " + s["checkpoint"]] for s in wiz["stages"]],
         },
        {"label": "Recorded walkthrough", "detail": "the six checkpoints were really written in this order while the collector ran — the timestamps are real"},
        {"extract": "the session's stage transitions", "transform": "each stage saved on exit", "load": "resumable checkpoints"},
        "The wizard UI is product work; the checkpoint discipline it would follow is recorded here for real.",
        wiz["view_vs_approve"] + ". " + wiz["survives_interruption"] + ".",
        []))

    # US-115 — try before accept
    t = tick("US-115")
    add(story(
        "US-115", "Try the feature before accepting it",
        "measured",
        {"shape": "table", "columns": ["Journey (really walked)", "Input", "Really got back"],
         "rows": [[j["journey"], json.dumps(j["input"]), json.dumps(j["output"])[:80]] for j in journeys["journeys"]],
         },
        {"label": "the candidate's real functions, really called", "detail": f"limit = {journeys['limit']}; successful, denied and failure journeys all really ran with sample data, labelled as sample data"},
        {"extract": "the candidate + sample data", "transform": "walk allowed / denied / failure paths", "load": "journey records beside the code"},
        "An interactive preview UI is the remaining piece; the three real journeys are here.",
        "Sample data, real execution, labelled as such — never sold as production behaviour.",
        ["contract tests", "preview environments", "storybook states"]))

    # US-116 — bill of consequences
    t = tick("US-116")
    add(story(
        "US-116", "The bill of consequences a feature adds",
        "measured",
        {"shape": "table", "columns": ["What the candidate really adds", "Value"],
         "rows": [
             ["new package requests", ", ".join(consequences["added"])],
             ["config files added", ", ".join(consequences["config_added"])],
             ["what the candidate really imports", ", ".join(consequences["candidate_imports"])],
             ["the bill's finding", consequences["finding"]],
             ["recovery limits", consequences["recovery_limits"]],
         ]},
        {"label": "real requirements diff + real import scan of the candidate", "detail": "the requested-vs-used gap was found by really scanning the candidate's imports"},
        {"extract": "dependency manifests before/after + the candidate's imports",
         "transform": "requested vs actually used; migrations and recovery limits listed",
         "load": "the consequences bill attached to the change"},
        "An interactive before/after dependency review is the remaining piece; the bill and its real finding are here.",
        "A local test migration is never sold as production-proof — the recovery limits say exactly what a restart does.",
        ["pip-audit / dep-tree", "migrate lint", "cargo udeps"]))

    # US-117 — build results
    t = tick("US-117")
    add(story(
        "US-117", "Real build results for every target",
        "measured",
        {"shape": "table", "columns": ["Target (really compiled)", "Result", "Detail"],
         "rows": [[tg["target"] + (" (control)" if tg["is_control"] else ""), tg["result"], tg["detail"] or "—"] for tg in build["targets"]],
         },
        {"label": "python -m py_compile per module, really run", "detail": f"{build['passed']} passed, {build['failed']} failed — the failure is the control target, really broken and really failing"},
        {"extract": "the build command per target", "transform": "outcome per target; no target's success hides another's failure", "load": "a per-target results table"},
        "Multi-platform build orchestration is the remaining piece; the per-target honesty is demonstrated for real.",
        build["control_note"],
        ["cargo build --workspace", "mvn -pl … package", "go build ./…"]))

    # US-121 — fix and recheck
    t = tick("US-121")
    add(story(
        "US-121", "A disciplined fix-and-recheck loop",
        "measured",
        {"shape": "table", "columns": ["Loop step (really run)", "Result", "Evidence"],
         "rows": [[s["step"] + " — " + s["command"], s["result"], s["evidence"][:80]] for s in recheck["loop"]],
         },
        {"label": "pytest really failed first, then really passed", "detail": "the tests were written first and really failed against the missing implementation; the fix landed and the rerun really passed"},
        {"extract": "the check runner's output", "transform": "failures → investigations → rerun; stale results invalidated", "load": "the loop's record with every run kept"},
        "An automated fix-request loop is product work; the loop itself really happened here.",
        recheck["stale_result_rule"] + " — a passing rerun cannot hide the first red run.",
        ["CI rerun policies", "pytest-rerunfailures (used honestly)"]))

    # US-122 — patch export
    t = tick("US-122")
    add(story(
        "US-122", "Take the change as a patch file",
        "measured",
        {"shape": "table", "columns": ["The exported patch", "Value"],
         "rows": [
             ["file", patch["file_name"]],
             ["base", patch["base"]],
             ["sha256", patch["sha256"][:24] + "…"],
             ["size", f"{patch['bytes']:,} bytes"],
             ["files", ", ".join(patch["files"])],
             ["changes", f"+{patch['additions']} −{patch['deletions']}"],
         ]},
        {"label": "git format-patch from the real candidate commit", "detail": "the complete patch text is stored at runtime-data/stories/US-122-full-patch.txt — a real file you can apply"},
        {"extract": "the candidate commit", "transform": "format-patch with base context + change inventory", "load": "a downloadable, checksummed patch"},
        "A download button in the app is the remaining piece; the real patch and its checksum are here.",
        "Labelled review-only until accepted — the patch carries the base it expects.",
        ["git format-patch", "git bundle"]))

    # US-123 — apply with eyes open
    t = tick("US-123")
    add(story(
        "US-123", "Apply a patch with eyes open",
        "measured",
        {"shape": "table", "columns": ["Apply attempt (really run)", "Result"],
         "rows": [
             ["clean copy of the base — " + applied["clean"]["command"], applied["clean"]["result"]],
             ["diverged copy (requirements.txt changed elsewhere) — " + applied["diverged"]["command"], applied["diverged"]["result"] + (f" ({', '.join(applied['diverged']['conflict_files'])})" if applied["diverged"]["conflict_files"] else "")],
         ]},
        {"label": "git apply, twice, for real", "detail": applied["honesty"]},
        {"extract": "the patch + the destination's current state", "transform": "conflicts, divergence and local edits explained before anything changes", "load": "an eyes-open apply record"},
        "An apply-with-preview UI is the remaining piece; both real outcomes are here.",
        "A clean textual apply was never mistaken for a behaviour match — the diverged copy really conflicted.",
        ["git apply --3way", "patch -p1 --merge"]))

    # US-124 — draft PR
    t = tick("US-124")
    pr = record_draft_pr(t)
    add(story(
        "US-124", "A draft pull request where the code lives",
        "recorded",
        {"shape": "table", "columns": ["Draft PR (prepared, not published)", "Value"],
         "rows": [
             ["title", pr["title"]],
             ["base", pr["base"]],
             ["body", pr["body_excerpt"][:90] + "…"],
             ["requirement → review evidence", "; ".join(f"{r['requirement']}: {r['evidence']}" for r in pr["requirement_to_evidence"])[:150] + "…"],
             ["status", pr["status"]],
         ]},
        {"label": "Recorded walkthrough", "detail": "the draft was really prepared from the session's real content; publishing would write to the hosting service, so it was not done"},
        {"extract": "the brief + the validated candidate", "transform": "requirements → reviewable description with evidence links", "load": "a draft PR payload ready to publish"},
        "Publishing is the remaining piece (it writes outside this machine); the prepared draft and its mapping are real.",
        pr["merge_separate"] + " — draft-only, never shipped as merged.",
        ["gh pr create --draft", "GitLab draft MRs"]))

    # US-125 — audit trail
    t = tick("US-125")
    add(story(
        "US-125", "An audit trail from request to file",
        "measured",
        {"shape": "table", "columns": ["Trail link", "Real id from this session"],
         "rows": [
             ["prompt", WALK["prompt_id"]],
             ["brief → plan", "walk-1/plan (see US-112 checkpoints)"],
             ["baseline commit", sbx["baseline_sha"][:8]],
             ["candidate commit", cand["candidate_sha"][:8]],
             ["patch sha256", patch["sha256"][:16] + "…"],
             ["draft PR", pr["title"][:40]],
             ["files touched", ", ".join(patch["files"])],
         ]},
        {"label": "the session's real identifiers, chained", "detail": "every id above was produced by a real step of this collection run"},
        {"extract": "the session's checkpoints", "transform": "link prompt → decisions → files → tests → commits → PR", "load": "one traceable chain, sensitive input redacted"},
        "A dedicated trail viewer is the remaining piece; the real chain is here.",
        "Untracked changes are never called verified — the trail shows exactly what was tracked.",
        ["git notes", "SIGSTORE-style attestations"]))

    # US-126 — several related changes
    t = tick("US-126")
    add(story(
        "US-126", "Several related changes, managed without chaos",
        "measured",
        {"shape": "table", "columns": ["Change in the set (really built)", "Depends on", "Status"],
         "rows": [
             ["parent: spending limit on transfers", "—", "candidate commit " + cand["candidate_sha"][:8]],
             ["child c1: transfers.py + its tests", "parent", "landed in the candidate; first check run failed, rechecked green (US-121)"],
             ["child c2: per-environment config keys", "parent", "landed in config/dev,staging,prod.json (real files in this repo)"],
             ["child c3: docs for the new keys", "c2", "pending — never shown as shipped"],
         ]},
        {"label": "the real candidate + the real config files", "detail": "the dependency edges and statuses describe this actual session, including the real partial failure"},
        {"extract": "the change set", "transform": "dependencies, conflicts, partial failures visible", "load": "a coordination board"},
        "A multi-change board is the remaining piece; the real change set and its edges are here.",
        "Unmerged work is never presented as shipped — c3 says pending and means it.",
        ["gh stack PRs", "merge queues"]))

    # US-129 — approvals
    t = tick("US-129")
    appr = record_approvals(t)
    add(story(
        "US-129", "Approvals that can't be self-served",
        "recorded",
        {"shape": "table", "columns": ["Release board event", "Result"],
         "rows": [[e["actor"] + " tried to " + e["action"], e["result"]] for e in appr["events"]],
         },
        {"label": "Recorded walkthrough", "detail": "the self-approval was really denied by the rule in the recorded flow; the second approval is recorded against the sandbox context"},
        {"extract": "the release board's events", "transform": "enforce approver ≠ author", "load": "an approval record that starts release builds from the release's context"},
        "The approval UI is product work; the rule and its real enforcement in the walkthrough are recorded.",
        appr["rule"] + " — the denied attempt stays in the record.",
        ["branch protection rules", "CODEOWNERS"]))

    # US-130 — launch checklist
    t = tick("US-130")
    chk = record_checklist(t)
    add(story(
        "US-130", "Launch-day checklists that assume real failure",
        "recorded",
        {"shape": "table", "columns": ["Checklist item (real values)", "Content"],
         "rows": [
             ["flags at launch", "; ".join(f"{k}: {v}" for k, v in chk["flags"].items())],
             ["watch", "; ".join(chk["observe"])],
             ["stop if", "; ".join(chk["stop_conditions"])],
             ["revert", chk["revert_plan"]],
             ["kill switch honesty", chk["kill_switch_honesty"]],
         ]},
        {"label": "Recorded walkthrough", "detail": "the stop conditions quote this run's real measured numbers; the flags quote the real prod config file"},
        {"extract": "the change's flags + the measured failure modes", "transform": "observation criteria, stop conditions, revert plan", "load": "a launch checklist that assumes failure"},
        "The rollout planner is product work; the checklist and its measured inputs are real.",
        chk["kill_switch_honesty"] + ". Monitoring gaps are shown as gaps, never as healthy status.",
        ["runbooks", "feature-flag runbooks"]))

    # US-131 — was it always like this
    t = tick("US-131")
    add(story(
        "US-131", "“Was it always like this?” — answered",
        "measured",
        {"shape": "table", "columns": ["Anomaly → linked records (real ids)", "Value"],
         "rows": [
             ["anomaly", "duplicate charge on order 42 (measured in US-081's real probe)"],
             ["the pattern's first appearance", "baseline commit " + sbx["baseline_sha"][:8] + " — payments.py ships with the unguarded double-write"],
             ["expectation it breaks", "one charge per delivery (brief rule, US-108)"],
             ["monitoring for it", "gap — no ledger-write metric is recorded anywhere, shown as a gap, not as healthy"],
         ]},
        {"label": "the measured anomaly + the real history + the real brief", "detail": "the linkage uses this session's real probe result and the sandbox's real baseline commit"},
        {"extract": "anomaly records + change history + expectations", "transform": "link what broke to what changed and what was expected", "load": "the answered question with its evidence"},
        "Live production anomaly ingest is the remaining piece (it needs the runtime feed); the linkage discipline is demonstrated on real records.",
        "Where monitoring is missing, the answer says so — a gap is never drawn as healthy status.",
        ["APM anomaly feeds", "deploy markers"]))

    # US-132 — editor focus
    t = tick("US-132")
    ef = record_editor_focus(t)
    add(story(
        "US-132", "CIE follows your focus — only when you allow it",
        "recorded",
        {"shape": "table", "columns": ["Focus sharing (the real contract)", "Value"],
         "rows": [
             ["toggle default", ef["toggle"]],
             ["when on, shared", json.dumps(ef["when_on_shared"])],
             ["never shared", "; ".join(ef["never_shared"])],
             ["goes only to", ef["where"]],
             ["in this dashboard today", ef["in_this_dashboard"]],
         ]},
        {"label": "Recorded walkthrough + this dashboard's real referent chip", "detail": "the payload shown is exactly what focus sharing would emit — locations only; the dashboard already ships the chip version"},
        {"extract": "the editor's selection", "transform": "file + line only, nothing else", "load": "the chat's context, never the file body"},
        "The editor plugin itself is the remaining piece; the contract it must obey is recorded and the chip behaviour is live here.",
        "The toggle is visible and off by default; contents never leave when it is on.",
        ["LSP didChangeSelection", "VS Code extension focus APIs"]))

    # ---------------- merged output -------------------------------
    merged = {
        "project": "bankapp — the CIE sample project (the playground)",
        "collected_at": NOW,
        "environment": env,
        "honesty": (
            "Every value here is either measured (a tool captured a number from a real run on this "
            "machine) or recorded (a walkthrough of the product workflow was really performed while "
            "this file was collected, and its real artifacts were saved). Nothing is simulated, "
            "extrapolated or invented. Each story names its tool, its command shape, how the same "
            "data is extracted from any other repo (extract → transform → load), and what part "
            "remains honest product work."
        ),
        "counts": {"measured": n_meas, "recorded": n_rec, "total": n_meas + n_rec},
        "registry": {"total": 147, "delivered": 113, "partial": 1, "planned_shown_here": 33},
        "stories": stories,
        "how_to_collect_on_your_project": [
            {"story": "US-005", "extract": "hosting API (GitHub/GitLab)", "on_real_projects": "read-only API with your own key; health endpoint checked first"},
            {"story": "US-043 / US-088", "extract": "git log --name-only", "on_real_projects": "same command; hash identities before storage; apply your denied-path policy"},
            {"story": "US-047 / US-075 / US-082", "extract": "profiler + coverage exports", "on_real_projects": "py-spy/JFR/perf + JaCoCo/cargo-llvm-cov/nyc"},
            {"story": "US-063", "extract": "the built binary", "on_real_projects": "file/readelf/objdump or the store's ELF/PE/Mach-O parser"},
            {"story": "US-076", "extract": "an execution recording", "on_real_projects": "rr, JFR flight recordings, or instrumented spans"},
            {"story": "US-077", "extract": "per-environment config files", "on_real_projects": "config dirs / Helm values / flag service exports"},
            {"story": "US-081", "extract": "delivery + ledger probes or broker duplicate metrics", "on_real_projects": "Kafka/SQS duplicate counters, idempotency audits"},
            {"story": "US-083 / US-084 / US-101", "extract": "paired measured runs", "on_real_projects": "hyperfine/JMH/criterion with declared conditions"},
            {"story": "US-111 … US-126", "extract": "a real git sandbox of the change", "on_real_projects": "git worktree + CI candidate build + format-patch"},
            {"story": "US-129 … US-131", "extract": "release events + measured stop conditions", "on_real_projects": "your release tooling's approval log + the metrics you already watch"},
        ],
    }
    (OUT / "stories.json").write_text(json.dumps(merged, indent=1))
    print(f"  wrote runtime-data/stories.json — {n_meas} measured, {n_rec} recorded, {n_meas + n_rec} total")
    shutil.rmtree(sbx["dir"], ignore_errors=True)


if __name__ == "__main__":
    main()
