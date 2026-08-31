# Demo production log — "cie, asked about itself" (2026-08-31)

> The full studio record for the README's 30-second demo
> (`docs/demo/cie-demo-30s.gif`). Owner-approved format A (one 30s GIF);
> owner waived final visual QC on delivery. The uncut proof ships
> alongside the GIF: `resolve-backend-uncut.cast` (agent take, asciinema
> v2) and `setup-uncut.cast` (clone → index → register → ✔ Connected).

## 1. The cut

| Scene | Time | Real content |
|---|---|---|
| 00 title | 2.0s | house card — tagline + "dogfooding: cie, asked about itself" |
| 01 index | 3.6s | `git clone github.com/kannamma-labs/cie` (real, public), `git checkout v0.1.4`, `cie index .` — 1,902 nodes · 6,581 edges · 4,169 calls, ~1.9s real time |
| 02 register | 3.0s | the README one-liner (`claude mcp add cie -- $(command -v cie-mcp) … --backend embedded --policy readonly`), `claude mcp list` → `cie … ✔ Connected` |
| 03 the question | 2.6s | typed live into the Claude Code TUI: "I want to change how resolve_backend picks a storage backend. What calls it, what breaks downstream, and which tests would catch a regression? Be specific with files and test names." |
| 04 impact + tests | 12.4s | the real 99.2s agent session, time-compressed: `callers` (first on screen at 56.5s), `affected_by` (60.1s), the answer — 7 pinning tests with line numbers, incl. `test_explicit_auto_flag_falls_through_to_detection (L58) — the specific bug fixed 2026-08-31`; ends on the completed screen |
| 05 snapshot | 4.0s | `cie export-html . --out snapshot.html`, then the real file:// page scrolled: overview → task & test chains → orphans (no test, no contract) |
| 06 end card | 2.0s | pinned facts + install line + "uncut session linked in the README" |

Total: **354 frames @ 12 fps = 29.5s**, 1000×585, palette GIF, **4.1 MB**.

## 2. Edit rule (the honesty contract)

Time is the only thing compressed. Every GIF frame is a REAL screen
state rendered from the uncut casts at a scheduled timestamp — content
is never retyped, trimmed mid-word, reordered, or synthesized. The
casts (asciinema v2, in this directory) are playable proof.

## 3. Production decisions & retakes

- **Corpus redirected mid-production by the owner**: originally
  psf/requests@5460f467 (per the approved draft); the owner switched to
  **cie itself** — dogfooding. Question re-targeted from `Session.close`
  to `resolve_backend` (the storage-selection rule shipped that day):
  13 callers, 30-hit blast radius on `cie/config.py`, 7 pinning tests —
  the most self-referential story available.
- **`cie init .` dropped from the cut**: its project-scope `.mcp.json`
  plus the local-scope one-liner makes Claude Code print a dual-scope
  warning during `claude mcp list` — demo noise. The one-liner carries
  the registration beat; init stays documented in the README.
- **Agent scoping**: `claude --allowedTools <the 4 cie tools>
  --disallowedTools Bash,Read,Edit,Write,Glob,Grep,WebFetch,WebSearch,
  NotebookEdit,Task,TodoWrite`. Take 1 of the agent session proved the
  TUI's `--allowedTools` GRANTS but does not RESTRICT (unlike `-p`
  mode): the agent went to `Bash`/grep — the exact anti-story — and
  hung on the permission prompt. With built-ins disallowed, cie's tools
  were the only path, and the session completed in 99.2s. The README
  caption states this scoping explicitly.
- Takes/retakes (S1S2): v1 cwd bug (recorder never set the child's
  working directory); v2 typing-serialization bug (a timed step
  overwrote the in-flight typing buffer — dropped chars, merged
  commands); v3 prompt-redraw race (first chars eaten between
  prompt-draw and bracketed-paste re-init → typing now waits for a
  quiet prompt) + a one-character PS1 bug that hid the prompt glyph
  from the gate; final keeper after cleaning a stale `claude mcp add`
  registration ("already exists") from a prior take.
- Takes/retakes (agent session): take 1 died on Claude Code's
  workspace-trust dialog — its default is "No, exit" and the watcher's
  Enter selected it (plus an Enter-spam loop) → correct key is
  ↓+Enter, and watchers got a 3s cooldown. Take 2 = keeper.
- Final visual QC: **waived by owner approval** (2026-08-31). Content
  QC was NOT waived and passed: index numbers, "Added stdio MCP
  server", `✔ Connected`, no dropped chars, no dual-scope warnings,
  no errors; the answer's test list verified against the cast.

## 4. The product bug this production caught

Dogfooding measurement on cie itself (v0.1.3): **exactly ONE TESTS edge
from a 308-test suite.** Root cause: testlink's heuristics gated on
naming convention and `@patch` — behavioral test names and `monkeypatch`
starve both. Fixed as **heuristic (4): direct-calls TESTS edges**
(test-glob files and `conftest.py` never targets; no duplicates),
released in **v0.1.4**: TESTS edges 1 → 562, total edges 5,992 → 6,553
on the dev tree (1,902/6,581 on the tag), `test_map(resolve_backend)`
0 → 7. The agent's on-screen answer — citing the regression test added
that same day — is the fix demoing itself.

## 5. Tooling (for reproducibility)

`docs/demo/prod/` holds the production scripts as run: the PTY recorder
(casts + pyte-rendered frames, prompt-gated typing, watcher cooldowns),
the takes, the post-production compositor (cards, lower-thirds,
scheduled cast re-renders), and the ffmpeg GIF assembly. Paths reference
`/tmp/prod` as run on 2026-08-31; adjust for a rerun. Machine: Linux
(pyte + Pillow for frames; system Chrome for the browser scene). No
screen-recording, no manual editing — deterministic re-render from the
casts.