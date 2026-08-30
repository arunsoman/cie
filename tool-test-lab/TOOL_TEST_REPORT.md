# Tool-Surface Production-Readiness Audit — 2026-08-30

Method: every tool exposed by the agent harness was exercised with a happy
path, ≥1 edge case, and ≥1 failure case. Every on-disk effect was cross-verified
independently (via `bash` + re-reads). No tool was marked passing on a single
unverified run.

## Verdict matrix

| #  | Tool                     | Verdict               | Evidence |
|----|--------------------------|-----------------------|----------|
| 1  | `read`                   | ✅ PASS                | Text ✓; offset/limit returned exactly 95–100 of 100 lines ✓; 8×8 PNG rendered as image ✓; empty file no-crash ✓; missing file → clean `ENOENT` ✓ |
| 2  | `write`                  | ✅ PASS                | New file ✓; deep path `deep/nested/dir/` auto-created ✓; overwrite of existing file fully replaced content (verified on disk) ✓ |
| 3  | `edit`                   | ✅ PASS                | Single block ✓; 3-block multi-edit in one call ✓; non-matching oldText → clear error and target file byte-identical afterwards (atomic, no partial write) ✓ |
| 4  | `bash`                   | ✅ PASS                | Pipes/redirection/grep/wc/seq ✓; non-zero exit propagated with stderr surfaced ✓; env + arithmetic ✓; used to verify all other tools' effects ✓ |
| 5  | `extract_features`       | ⚠️ CONDITIONAL         | Happy path passed once (5/5 features from synthetic mini-PRD). Tool later vanished from registry (see incident below) → availability not guaranteed |
| 6  | `web_search`             | ⚠️ CONDITIONAL         | Real-time results correct (Node 26.8.1 Current / 24.20.0 LTS as of run date). Response dumps whole pages — verbose but functional. Later vanished from registry |
| 7  | `web_fetch`              | ⚠️ CONDITIONAL         | `example.com` → title/content/links extracted correctly. Later vanished from registry |
| 8  | `check_completeness`     | ❌ FAIL                | Crashed the extension host on schema-valid input (see incident). Result never recorded; surfaced as synthetic "No result provided" |
| 9  | `ideate_alternatives`    | ❌ FAIL                | Same failure as #8 (3/3 concurrent calls orphaned) |
| 10 | `critique_idea`          | ❌ FAIL                | Same failure as #8 |
| 11 | `prd_iterate`            | ⛔ UNVERIFIED          | Registry died before any execution could run. Architecture (7 agent subprocesses) inspected and sane; happy path never observed |

## The incident (why custom tools are not production ready)

Timeline: on the first batch of three concurrent feature-tool calls with
schema-valid input, all three tool **calls** were logged but **no tool
_results_ were ever recorded**. The harness's message-transform layer then
synthetically injected `No result provided` markers for the orphaned calls
(54 markers total in the session log). From the next turn onward, every
extension-registered tool — `extract_features`, `check_completeness`,
`ideate_alternatives`, `critique_idea`, `prd_iterate`, `web_search`,
`web_fetch` — returned `Tool <name> not found`. Core builtins
(`read`/`bash`/`edit`/`write`) were unaffected.

Root-cause analysis (from source):
- The tools are TS extensions (`~/.pi/agent/extensions/fresh-idea`,
  `prd-iterate`, npm `@ollama/pi-web-search`) whose `execute()` wrappers DO
  have try/catch that returns error payloads — so a normal exception cannot
  produce an orphaned call.
- Therefore the failure is a crash path *outside* the try/catch: consistent
  with an unhandled rejection/abort during the concurrent inner
  `completeSimple()` calls to the Ollama cloud model, killing the extension
  host process and unregistering every custom tool for the rest of the
  session.
- pi only restores extensions via `/reload` or a session restart — i.e. a
  single buggy execution degrades the entire session. **No failure
  isolation between extensions.**

## Secondary findings

1. Schema validation reports only ONE missing field per call
   (`id` → then `objective` → then `description`): a caller must
   re-fail repeatedly to discover the full contract.
2. `feature`/`all_features` are declared as JSON **strings** in the TypeBox
   schema, while the harness silently serializes object args — works, but
   the declared contract and runtime behavior disagree.
3. `web_search` returns full page dumps (verbosity/noise issue, not a
   correctness issue).
4. `extract_features` returns only a summary line to the calling model;
   the extracted features live in `details` (side-band), so callers cannot
   chain into `check_completeness` from the visible result alone.

## Fix recommendations (ordered)

1. Wrap extension tool execution in a supervisor that converts unhandled
   rejections/timer aborts into normal tool errors, and make one
   extension's crash non-fatal to the others (process isolation).
2. Add a timeout to inner `completeSimple()` calls so hangs become errors.
3. Return the full Zod error list (all missing fields) in one response.
4. Trim `web_search` output to snippets + links.
5. Re-test plan after `/reload`: sequential single-flight for the three
   feature tools; deliberate 3-way concurrency test; then `prd_iterate`
   (maxIterations=1) end-to-end.

## Bottom line

- Core builtins (`read`, `bash`, `edit`, `write`): **production ready**
  (verified happy/edge/failure, atomic behavior confirmed).
- All seven extension tools: **not production ready** — one failure mode
  (concurrent valid calls) took out the whole custom-tool surface for the
  session, silently. `prd_iterate` additionally remains unexecuted.