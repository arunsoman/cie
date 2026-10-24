# QA contracts — behavioural baseline (Stage 0)

This directory holds the **independent source-oracle** for the behavioural
evidence redesign. Its baseline fixture pins what an analyst can prove from
the SMPP SMS gateway source alone, and `scripts/qa-behaviour-baseline.py`
replays those six expectations read-only against a captured legacy bundle —
the revision snapshot artifact CIE actually persisted — with three distinct
outcomes:

| Status | Meaning |
|---|---|
| `pass` | the bundle carries the oracle-required records and satisfies the claim |
| `fail` | the evidence is present and demonstrably violates the claim (defect reproduced) |
| `invalid` | the bundle lacks the evidence needed to evaluate the claim (absent symbol/artifact) |

**Absence is never a pass.** A missing symbol, a missing `stack` artifact or
an empty record set yields `invalid`, and the fixture's expected result treats
`invalid` as a mismatch — silent passes are the one thing this harness cannot
produce. A check that raises is caught and recorded as `invalid` with its
error, never coerced into a softer verdict.

## Invocation

```bash
# the six-expectation replay against a captured bundle (file or artifact dir)
python3 scripts/qa-behaviour-baseline.py <bundle-path>

# machine-readable output (same exit codes)
python3 scripts/qa-behaviour-baseline.py <bundle-path> --json

# harness self-test — no bundle needed
python3 scripts/qa-behaviour-baseline.py --self-test
```

Accepted bundle shapes (also pinned in the fixture's `bundle_contract`):

1. a JSON file containing one RevSnapshot-shaped object
   (`rev`/`files`/`symbols`/`summaries`),
2. a JSON file wrapping exactly one snapshot under `"snapshots"`,
3. a JSON file holding multiple web artifacts where exactly one top-level
   value is snapshot-shaped (e.g. under `"report"`) — the shape `--web-out`
   actually produces; sibling `stack`/`concepts` fill gaps,
4. a directory of artifact JSONs — the snapshot-shaped file is used, and
   `stack`/`concepts` are merged from sibling artifacts when the snapshot
   itself lacks them.

Ambiguity is refused, never guessed: two snapshot-shaped files in one
directory, or a wrapper holding more than one snapshot, is a harness error
(exit 2), because replaying the oracle against the wrong capture would look
exactly like drift.

## Expected failing baseline

Against the **real capture** of the SMPP root taken at CIE `5267564`
(baseline recorded in `smpp-behaviour-baseline.json`), the harness returned:

```
0 pass / 6 fail / 0 invalid
```

**Verified in this repository on 2026-10-24:** `arunsoman/smpp-mls` was
cloned and ingested with CIE `7b7b062` (`cie ingest <root> --web-out
<bundle>`), and the oracle replayed against the exported web bundle
(snapshot under `'report'`):

```
result   : 0 pass / 6 fail / 0 invalid   (fixture expected 0 / 6 / 0)
verdict  : baseline REPRODUCED — every checkable expectation fails for its diagnosed reason.
```

Real evidence named by that run: the root split into `build.gradle` +
`pom.xml` app records (BB1); `SmsController.submitSms` with zero resolved
callees and only `virtual_dispatch` texts (BB2); 7/7 `DbRead`/`DbWrite`
claims receiver-blind, including `SMPPSession.parse_cstring` (a byte parser)
and `verify_database.DatabaseVerifier.verify_all` (BB3); two lexical-only
`verify_all` auth tags promoted into the FCA concept `authorization & prints`
AND two `MustAuthorize` contracts — "'verify_all' must perform real
authorization before returning success" (BB4); callee lists like
`verify_all`'s `[277 ×7]` with no occurrence identity (BB5); 95 resolved
callee records with zero source-order fields (BB6).

To reproduce:

```bash
git clone --depth 1 https://github.com/arunsoman/smpp-mls
./target/debug/cie ingest smpp-mls --web-out /tmp/smpp-bundle   # or cargo build -p cie-cli first
python3 scripts/qa-behaviour-baseline.py /tmp/smpp-bundle
```

| Expectation | Layer | Reproduced defect |
|---|---|---|
| `BB1-identity-one-app` | identity | one app per manifest: Maven + Gradle over one source root recorded as two applications |
| `BB2-dispatch-qualified-resolution` | dispatch | two-state resolved/unresolved model cannot state a solid declaration plus a qualified runtime dispatch (Spring proxy) |
| `BB3-effects-receiver-provenance` | effects | spelling-classified effects: `data.find` becomes `db_read` with no receiver/library provenance anywhere in the bundle |
| `BB4-auth-lexical-hint-not-fact` | semantic claims | `is_auth_name` matches `verify_`, stamps `auth`; FCA emits the `authorization & prints` concept and `conformance.rs` fabricates `MustAuthorize` contracts — normative security obligations from a spelling |
| `BB5-callsite-occurrence-identity` | facts to store | repeated calls joined by caller + callee text (e.g. `[277 ×7]` bare duplicates); for Java, call sites aren't first-class records at all (`virtual_dispatch` texts only) |
| `BB6-diagram-source-order` | diagrams | chronology derivable only by BFS + visited-symbol set + symbol-id tie-breaks; repeats collapse; "no evidenced interactions" has no typed record |

Exit codes: `0` observed counts equal the fixture's expectation; `1` drift
(behaviour changed against the contract — investigate, never re-run until it
"goes green"); `2` harness/usage error.

Note the Java-specific deepening the real capture revealed: for Java,
`submitSms`'s callees list is **empty** — every call survives only as
`virtual_dispatch` unresolved text, so call sites are not first-class records
at all in that language, which is strictly worse than the collapse BB5
describes for resolved callees.

## The harness is generic; the fixture is the oracle

All application specifics — anchors (`SmsController.submitSms`), manifest
builds, lexical spellings, authorization annotation/callee patterns, FCA
intent translations — live in `smpp-behaviour-baseline.json`. The Python
harness contains **no application-specific allowlist fixes**; it implements
six generic check kinds driven entirely by fixture params. That separation is
the Stage 0 exit gate: *fixtures fail for the diagnosed reasons; no
application-specific allowlist fixes*.

## Migration caveats

- **The synthetic self-test is not the real capture.** `--self-test` embeds a
  legacy-*shaped* bundle to prove the harness's statuses are distinct and
  that the diagnosed shapes reproduce `0/6/0`, plus a degenerate bundle to
  prove zero silent passes on absent evidence. The `0 pass / 6 fail /
  0 invalid` claim for the *real* capture is recorded in the fixture's
  `baseline.expected_result` block and was produced by replaying that
  capture; the synthetic exists so the harness can be regression-checked
  without possessing it.
- **Do not edit expectations to make a run pass.** When later stages fix a
  root cause, the fix is proven by re-running the same fixture and observing
  the corresponding expectation turn `pass` — with the *same* bundle shape.
  Changing oracle claims requires an explicit fixture revision (new expected
  counts, dated, with the reason), never an in-place silent edit.
- **Old bundles stay honestly readable.** Snapshots captured before later
  stages keep replaying as `0/6/0` — they are never relabelled as newly
  verified. Re-ingest old archives explicitly if you need oracle-passing
  records for them; AnalysisProfile/schema versions, not source hashes
  alone, decide whether a bundle can be re-evaluated at all.
- **Read-only is a hard guarantee.** The harness opens and parses; it never
  writes, re-ingests, mutates revision data, or reconstructs cleanup paths.
  It is safe to point at a production capture directory.
- **Scope.** Stage 0 is a diagnostic subset: immutable fixture/source-hash
  verification, additional-language positives, browser/task gates and the
  new export adapter remain required before stage 1's exit gate. The oracle
  checks data sufficiency in the captured bundle, not renderer behaviour —
  the shared viewport/readability contracts are later stages.
- **Counterexamples are load-bearing.** The `data.find` → `db_read` case,
  the `verify_all` → `authorization` promotion and the 9/22 no-message
  sequence panels are the negative fixtures these expectations are built
  around; any future check implementation that cannot express them is wrong
  even if its counts look right.
