# tool-test-lab — verification artifacts, not product code

Scratch-lab for the verification passes logged in `goal.md` (sessions
6–8). Nothing here is imported by the package; everything is either a
reproducible harness or an auditable artifact of a specific run.

## Reusable cie harnesses (run against any project)

- **`surface_conformance.py`** — full-surface conformance: enumerates
  EVERY tool a live `cie-mcp --embedded` server exposes, attempts a real
  call each, retries validation failures from the server's own
  "Field required" hints, and classifies every outcome (verified ok /
  graceful contract / unavailable-by-design / backend-gated / crash).
  Usage: `python surface_conformance.py <indexed-root> out.json`.
  ⚠ It pins `/home/arun/Downloads/cie` as PYTHONPATH so the sandbox
  server tests repo code, not a stale site-packages copy — edit for
  your checkout.
- **`dogfood_mcp.py`** — lighter live probes: tool-surface count per
  policy, `search_symbol` noise metric, `callers` spot check,
  `file_skeleton`, optional-backend degradation.

## Artifacts of record

- **`surface_results.json`** — the 2026-08-30 conformance run's full
  output (135 tools: 100 verified ok / 26 graceful / 5 unavailable /
  4 backend-gated / 0 crashes — the post-R1 surface, re-run same day;
  the prior pre-R1 snapshot (126 tools: 85/19/18/4) is superseded).
  Environment-specific paths inside; kept
  as the audited snapshot, not as a fixture.
- **`TOOL_TEST_REPORT.md`** — the 2026-08-30 audit of the *agent
  harness's* own tools (`read`/`write`/`edit`/`bash` + extension tools),
  separate concern from cie; retained because `goal.md` sessions 6–8
  reference it.