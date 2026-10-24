# QA contracts — behavioural baseline (Stage 0)

This directory holds the **independent source-oracle** for the behavioural
evidence redesign. Its baseline fixture pins what an analyst can prove from
the SMPP SMS gateway source alone, and `scripts/qa-behaviour-baseline.py`
replays those six expectations read-only against a captured bundle — the
revision snapshot artifact CIE actually persisted — with three distinct
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

## History: the failing baseline, then the fix

**2026-10-24, CIE `5267564`/`7b7b062` (fixture revision 1):** `arunsoman/smpp-mls`
was cloned and ingested, and the oracle returned `0 pass / 6 fail / 0 invalid`
— every expectation failed for its diagnosed reason:

```
result   : 0 pass / 6 fail / 0 invalid
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

**2026-10-24, post-fix engine (fixture revision 2):** the six root causes were
fixed in the production pipeline (see the fixture's revision 2 entry for the
full change set), the fixture's expectation moved to `6 pass / 0 fail /
0 invalid`, and a fresh ingestion of the same source satisfies the oracle:

```
result   : 6 pass / 0 fail / 0 invalid   (fixture expected 6 / 0 / 0)
verdict  : oracle satisfied as the fixture currently expects.
```

The green capture's substance, spot-checked:

| Expectation | Layer | What the fixed capture now records |
|---|---|---|
| `BB1-identity-one-app` | identity | ONE app record for the root (`name smpp-mls`, framework spring) whose `manifests` evidence list carries BOTH `build.gradle` (gradle) and `pom.xml` (maven) |
| `BB2-dispatch-qualified-resolution` | dispatch | `submissionService.submit`: `declaration: resolved` (receiver type `SubmissionService` declared in-tree), `dispatch: candidate` (Spring-proxied virtual dispatch), `observation: unobserved` — three independent fields per call |
| `BB3-effects-receiver-provenance` | effects | `parse_cstring`'s `data.find` demoted to a typed `db_read` lexical hint (zero db effects); 34 db claims all carry `receiver` + `resolution_basis` (`JdbcTemplate`, `SmsOutboundRepository` via `type-resolved-api-identity` — real access the old spelling classifier missed entirely) |
| `BB4-auth-lexical-hint-not-fact` | semantic claims | `verify_all` carries a typed `lexical_hints` record (`auth`, `lexical-name-match`, gap note) instead of an `auth` tag; zero `MustAuthorize` contracts; zero FCA authorization concepts |
| `BB5-callsite-occurrence-identity` | facts to store | every call site is a distinct record with `span` (real byte range), `occurrence`, `arguments`, `guard`; the two guarded `outboundRepository.save` sites in `SubmissionService.submit` survive separately |
| `BB6-diagram-source-order` | diagrams | 2281 call records carry `source_order` + `statement_index`; repeats are separate occurrences; 28 typed `no_evidenced_interactions` records with their reason live in the snapshot |

To reproduce the green state:

```bash
git clone --depth 1 https://github.com/arunsoman/smpp-mls
cargo build -p cie-cli
rm -f "/tmp/cie-polyglot-$(python3 -c "h=0xcbf29ce484222325
for b in __import__('os').path.realpath('smpp-mls').encode():
    h^=b; h=(h*0x100000001b3)&0xFFFFFFFFFFFFFFFF
print(f'{h:x}.db')")"*   # purge the engine's per-root scratch db
rm -rf smpp-mls/.cie
./target/debug/cie ingest smpp-mls --web-out /tmp/smpp-bundle
python3 scripts/qa-behaviour-baseline.py /tmp/smpp-bundle
```

To reproduce the **historical 0/6/0 defect baseline** instead: check out the
rev-1 fixture (`git log -- tests/qa-contracts/smpp-behaviour-baseline.json`),
build CIE at `5267564`, ingest, and replay — pre-fix captures replay as
DRIFT (exit 1) against the current fixture by design.

Exit codes: `0` observed counts equal the fixture's expectation; `1` drift
(behaviour changed against the contract — investigate, never re-run until it
"goes green"); `2` harness/usage error.

## The harness is generic; the fixture is the oracle

All application specifics — anchors (`SmsController.submitSms`), manifest
builds, lexical spellings, authorization annotation/callee patterns, FCA
intent translations — live in `smpp-behaviour-baseline.json`. The Python
harness contains **no application-specific allowlist fixes**; it implements
six generic check kinds driven entirely by fixture params. Two of those
kinds (`app_identity_evidence_split`, `auth_evidence`) were revised with
fixture revision 2 to evaluate the claims *positively* on the fixed shapes
(one merged app record carrying both manifests; typed lexical hints with
basis + gap note) — while self-test 1 proves the legacy shapes still fail
all six, and self-test 3 proves the fixed shape satisfies all six.

## Migration caveats

- **The synthetic self-tests are not the real capture.** `--self-test`
  embeds a legacy-*shaped* bundle (must reproduce `0/6/0` — the diagnosed
  defects stay caught), a degenerate bundle (zero silent passes on absent
  evidence), and a fixed-*shaped* bundle (must satisfy `6/0/0` — the fixed
  engine's shape is recognizably honest). The real-capture claims in the
  fixture's `baseline.expected_result` block were produced by replaying
  actual captures; the synthetics exist so the harness can be
  regression-checked without possessing them.
- **Do not edit expectations to make a run pass.** Fixture revisions are
  explicit and dated (the `revisions` block carries the full chain: rev 0
  defect baseline → rev 1 real-capture shape corrections → rev 2 the fixes
  land and the expectation flips). Oracle claims, anchors and
  counterexamples never changed.
- **Old bundles stay honestly readable — as drift.** Snapshots captured
  before the fixes replay against the current fixture as `0/6/0` observed vs
  `6/0/0` expected → exit 1 with every failure named. That is the honest
  reading: the artifacts genuinely carry the defects. They are never
  relabelled as newly verified.
- **Ingest caches are keyed per root, including the engine's scratch db.**
  The engine keeps a per-root scratch database at
  `/tmp/cie-polyglot-<fnv(realpath)>.db`; a same-day re-ingest after an
  engine upgrade MUST purge it (and the project's `.cie`) or the stale
  captured facts silently serve the new behavior stage — this was caught
  live during the green verification and is why the reproduction commands
  include both purges.
- **Read-only is a hard guarantee.** The harness opens and parses; it never
  writes, re-ingests, mutates revision data, or reconstructs cleanup paths.
  It is safe to point at a production capture directory.
- **Scope.** Stage 0 is a diagnostic subset: the oracle checks data
  sufficiency in the captured bundle, not renderer behaviour — the shared
  viewport/readability contracts and the 12-CSS-px floor are later stages
  (the legacy `mastery.ts`/`concept-diagrams.ts` derivations now have the
  source-ordered records they need, and get rewritten onto them in stage 4).
- **Counterexamples are load-bearing.** The `data.find` → `db_read` case,
  the `verify_all` → `authorization` promotion and the 9/22 no-message
  sequence panels are the negative fixtures these expectations are built
  around; any future check implementation that cannot express them is wrong
  even if its counts look right.
