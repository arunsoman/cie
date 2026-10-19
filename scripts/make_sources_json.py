#!/usr/bin/env python3
"""Bundle the sample project's source files for the web source panel.

Reads exactly the files listed in web/public/data/report.json (repo-relative
paths, as the engine indexed them) from examples/bankapp/ and writes
web/public/data/sources.json = { generated_at, project, files: {path: content} }.

The demo dashboard has no filesystem of its own — the source panel reads this
bundle. Any path in report.json missing on disk is recorded as null so the
panel can say so honestly instead of pretending.

Run from the repo root:  python3 examples/bankapp/scripts/make_sources_json.py
(or python3 web/scripts/make_sources_json.py — it locates itself).
"""

import json
import sys
from datetime import datetime, timezone
from pathlib import Path


def find_repo() -> Path:
    """Walk up from this script until web/public/data/report.json appears."""
    here = Path(__file__).resolve().parent
    for cand in [here, *here.parents]:
        if (cand / "web" / "public" / "data" / "report.json").is_file():
            return cand
    raise SystemExit("could not locate the repo (no web/public/data/report.json above the script)")


REPO = find_repo()
REPORT = REPO / "web" / "public" / "data" / "report.json"
SAMPLE = REPO / "examples" / "bankapp"
OUT = REPO / "web" / "public" / "data" / "sources.json"
MAX_BYTES = 512 * 1024


def main() -> int:
    report = json.loads(REPORT.read_text(encoding="utf-8"))
    files: dict[str, str | None] = {}
    missing, too_big = [], []
    for rel in report.get("files", []):
        p = SAMPLE / rel
        if not p.is_file():
            files[rel] = None
            missing.append(rel)
            continue
        if p.stat().st_size > MAX_BYTES:
            files[rel] = None
            too_big.append(rel)
            continue
        files[rel] = p.read_text(encoding="utf-8")

    bundle = {
        "generated_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "project": "sample project (examples/bankapp)",
        "note": "The exact files the engine indexed, bundled verbatim so the demo's source panel can show them. Regenerate after editing the sample: see script header.",
        "files": files,
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(bundle, indent=1, ensure_ascii=False), encoding="utf-8")
    n = sum(1 for v in files.values() if v is not None)
    print(f"wrote {OUT.relative_to(REPO)}: {n} files bundled")
    if missing:
        print(f"  missing on disk (recorded null): {missing}", file=sys.stderr)
    if too_big:
        print(f"  over 512 kB (recorded null): {too_big}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
