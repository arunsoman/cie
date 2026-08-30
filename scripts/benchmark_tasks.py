#!/usr/bin/env python3
"""benchmark_tasks.py — the reproducible core of docs/benchmarks*.md
(roadmap R9).

For ONE repository (pre-cloned at a pinned commit, pre-indexed with
`cie index .`), runs the three canonical task shapes BOTH ways and emits
a JSON + markdown fragment:

  1. easy definition     naive: `grep -rn "class X" SRC` (1 call)
                         cie:  `search_symbol("X")`       (1 call)
  2. ambiguous callers   naive: `grep -rn "<name>(" SRC`  — raw textual
                         matches, no receiver-type info
                         cie:  `callers("<name>")` — resolved edges +
                         the R7 `resolution` block (honest recall)
  3. large-file skeleton naive: read the whole file (byte count)
                         cie:  `file_skeleton(path)`     (byte count)

Token-per-query metric: chars of response payload (the honest,
tokenizer-free proxy — stated as such; no tokenizer is pinned, so the
number is a bytes comparison, labeled, not a vendor-style claim).

Usage:
  python benchmark_tasks.py <repo_root> --src-glob 'src/requests/*.py' \
      --class-name PreparedRequest --ambiguous-name close \
      --big-file src/requests/models.py --out out.json

Everything measured is printed; the markdown is assembled by the caller
(scripts/benchmark.sh) so a doc's numbers can be regenerated verbatim.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO = None  # scripts/benchmark_tasks.py is run as __main__, not imported


def _naive_bytes(repo: Path, command: list[str]) -> tuple[str, int]:
    proc = subprocess.run(command, cwd=repo, capture_output=True, text=True)
    return proc.stdout, len(proc.stdout.encode("utf-8"))


def _payload_bytes(envelope: dict) -> int:
    return len(json.dumps(envelope, default=str).encode("utf-8"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("repo", type=Path)
    parser.add_argument("--src-glob", required=True)
    parser.add_argument("--class-name", required=True)
    parser.add_argument("--ambiguous-name", required=True)
    parser.add_argument("--big-file", required=True)
    parser.add_argument(
        "--db", type=Path, default=None,
        help="The indexed graph file (default: `<repo>/.cie/graph.db` — "
             "pass `<repo>/src/.cie/graph.db` when the index was built "
             "against a subdirectory).",
    )
    parser.add_argument("--out", type=Path, default=None)
    args = parser.parse_args()

    from cie.factory import build_tool_service_embedded

    repo = args.repo.resolve()
    svc = build_tool_service_embedded(
        repo, db_path=args.db,
    )

    results: dict = {"repo": str(repo)}

    # -- task 1: easy definition ------------------------------------------
    naive_out, naive_bytes = _naive_bytes(
        repo, ["grep", "-rn", f"class {args.class_name}", str(repo)],
    )
    easy_env = svc.search_symbol(args.class_name)
    results["task1_easy_definition"] = {
        "naive_command": f'grep -rn "class {args.class_name}"',
        "naive_calls": 1,
        "naive_output_bytes": naive_bytes,
        "naive_files_found": len(set(
            line.split(":")[0] for line in naive_out.splitlines() if ":"
        )),
        "cie_calls": 1,
        "cie_tool": "search_symbol",
        "cie_payload_bytes": _payload_bytes(easy_env),
        "cie_ok": easy_env.get("ok"),
    }

    # -- task 2: ambiguous callers -----------------------------------------
    naive_out, naive_bytes = _naive_bytes(
        repo,
        ["bash", "-c", f'grep -rn "{args.ambiguous_name}(" {args.src_glob} | wc -l'],
    )
    grep_count = naive_out.strip()
    callers_env = svc.callers(args.ambiguous_name)
    results["task2_ambiguous_callers"] = {
        "name": args.ambiguous_name,
        "naive_command": f'grep -rn "{args.ambiguous_name}(" {args.src_glob}',
        "naive_matches": grep_count,
        "cie_calls": 1,
        "cie_tool": "callers",
        "cie_resolved_edges": len(callers_env.get("results") or []),
        "cie_resolution": callers_env.get("resolution"),
        "cie_honest_recall_note": (
            "resolved edges are the subset the receiver/same-file/import "
            "heuristics could attribute; resolution.unresolved_call_sites "
            "counts the rest — published, never hidden"
        ),
    }

    # -- task 3: large-file skeleton -----------------------------------------
    big = repo / args.big_file
    full_bytes = len(big.read_bytes())
    skel_env = svc.file_skeleton(args.big_file)
    results["task3_large_file_skeleton"] = {
        "file": args.big_file,
        "naive_calls": 1,
        "naive_read_bytes": full_bytes,
        "cie_calls": 1,
        "cie_tool": "file_skeleton",
        "cie_payload_bytes": _payload_bytes(skel_env),
        "compression": round(
            full_bytes / max(1, _payload_bytes(skel_env)), 2
        ),
    }

    out = args.out
    if out:
        out.write_text(json.dumps(results, indent=2))
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()