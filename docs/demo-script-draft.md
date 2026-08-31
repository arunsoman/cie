# 30-second README demo — script draft (NOTHING RECORDED/PUBLISHED YET)

> Status: **draft for owner approval** (2026-08-31, planning pass 24 —
> same gate pattern as `launch-post-draft.md` / `directory-listings-drafts.md`).
> Every fact below was verified live on 2026-08-31; the verification log
> is §8. Nothing gets recorded, embedded, or published until the owner
> approves a variant.

## 1. What the demo must prove (owner's brief)

- **USP:** "the only code graph that knows which tasks and tests
  actually implement your code" — i.e., an agent answering an
  impact-with-tests question that grep cannot, using ONLY cie's tools,
  on a real famous repo, in seconds.
- **UI capabilities:** the shareable single-file HTML snapshot
  (`cie export-html`) — Overview / Task & test chains / Orphans —
  plus the agent-native experience itself (tool calls visible in
  Claude Code).
- **Motto:** least friction — one install, one init, no Neo4j, no
  signup, offline.

## 2. Format options (pick one)

| Option | What | Trade-off |
|---|---|---|
| **A (recommended)** | ONE 30s GIF: terminal → index → init/Connected → agent question+answer (time-compressed) → HTML snapshot in browser → end card | Shows USP + UI + motto in one artifact; agent beat needs time-compression (content never edited) |
| B | Terminal-only 30s GIF (drop the browser beat, give the agent answer ~20s) | Strongest USP focus; loses the UI beat the owner asked for |
| C | Two GIFs: 30s terminal/agent + separate 10s HTML-snapshot tour | Each beat breathes; README gets two embeds (~larger) |
| D | asciinema player embed instead of GIF | Scrubbable/uncut by construction; but no autoplay jaw-drop and GitHub README needs hosted player iframes |

## 3. The corpus (pinned, real, already verified)

`psf/requests` @ `5460f467` — the same commit the v0.1.0 release
rehearsal and the semantic benchmark used. Famous, honest, and the
numbers are already measured on this machine (§8).

## 4. THE 30-SECOND BEAT SHEET (Option A)

| t | Beat | Exactly what's on screen | Real, verified content |
|---|---|---|---|
| 0.0–2.5 | **Cold open** | Fresh clone of `psf/requests` in a terminal; overlay: *"a real HTTP library. no setup."* | `git clone` output (can be pre-cloned; overlay states pinned facts only) |
| 2.5–6.0 | **Index** | `cie index .` — shown in REAL TIME | `nodes: 858  edges: 1822 (875 calls)` — measured **0.86s** (§8). Overlay: *"858 nodes · 1,822 edges · under a second"* |
| 6.0–9.0 | **One-click** | `cie init .` (fast-forwarded) → `claude mcp list` | `registered 'cie' stdio server` … `cie: … ✔ Connected` (verified §8) |
| 9.0–12.0 | **THE question** | Claude Code TUI; the user types: *"I want to change `Session.close`'s behavior. What calls it, what breaks downstream, and which existing tests would catch a regression?"* | exact wording from §5 |
| 12.0–24.0 | **Tool chain + answer** | Tool calls flash (`search_symbol` → `callers` → `affected_by` → `test_map`), then the answer's key sections scroll/highlight: **Who calls it** (3 callers w/ file:line + the `api.py` insight), **What it does downstream** (adapter close chain, `BaseAdapter.close()` raises), **Tests that would catch it** (the ONE targeted test + the coverage-gap finding) | Real 1m13s session (§8), **time-compressed only** — content byte-identical |
| 24.0–28.0 | **UI reveal** | `cie export-html . --out snapshot.html` → browser opens the file:// page | 224K single file, no server; sections: Overview / Task & test chains / Atomic tasks / Orphans (no test, no contract) / Indexed files |
| 28.0–30.0 | **End card** | Static card over the browser: *"135 read tools · zero-config · read-only by default · works offline · kannamma-labs/cie"* | every phrase is a pinned/verified fact |

Edit rule for beat 5: cuts and speed-ups only, **never** retype, trim
words within, or reorder the agent's answer — the uncut cast (§6) is
the proof.

## 5. The question + the answer we already captured (verbatim key lines)

Question (typed by the "user" in the recording, same words as §8's run):

> I want to change Session.close's behavior. What calls it, what breaks
> downstream, and which existing tests would catch a regression? Be
> specific with files and test names.

The agent — with **only** `search_symbol`, `callers`, `affected_by`,
`test_map` allowed (no file reads) — answered (2026-08-31, §8):

- **Who calls it:** `Session.__exit__` (`sessions.py:508-509`) — every
  `with requests.Session()`; `api.py:70` — the shared `request()`
  helper, so **every module-level `requests.get/post/...` triggers
  `Session.close()` on exit**; one direct call at
  `tests/test_requests.py:2216` (`test_session_close_proxy_clear`);
  nothing else in src/ — and it explicitly de-credited
  `Response.close()`/auth `r.close()` as unrelated look-alikes.
- **Downstream:** iterates mounted adapters → `HTTPAdapter.close()`
  (`adapters.py:555-563`, `poolmanager.clear()` + per-proxy clears) →
  `BaseAdapter.close()` **raises NotImplementedError**, so custom
  adapters without an override blow up.
- **Tests:** the ONE targeted test
  `TestSession::test_session_close_proxy_clear` (line 2209, its two
  `assert_called_once_with` named) + broad-but-shallow indirect
  exposure via every one-off call + the TLS/`testserver` suites that
  rely on connections actually terminating.
- **Bonus (the jaw-drop closer):** it found the **coverage gap** — no
  test covers `BaseAdapter.close()`'s raise path or `Session.__exit__`
  directly — and gave a bottom-line triage order.

## 6. Recording mechanics + prerequisites

- **v0.1.3 must be cut first** — the script's commands use
  `--backend embedded`, which exists only in the unreleased pass-23
  changes; the recording must show *shipped* behavior (uv-tool-install
  from the tag, so the demo binary = what a user gets).
- Recording stack (machine has: `asciinema` ✔; missing: `agg`/`vhs`):
  interactive Claude session via `asciinema rec`, then `agg` (cargo
  install) → GIF; browser beat via screen-region capture (OBS) →
  stitched with ffmpeg; OR single OBS screen recording → mp4 → GIF.
- GIF budget: ≤ 10 MB, ~1000px wide, 12–15 fps, 30s.
- **Uncut proof:** the full asciinema cast of the 1m13s agent session
  gets committed (e.g. `docs/demo/session-close-uncut.cast`) and linked
  under the GIF — "edited for time only."
- Terminal: consistent font/theme, ~100×30 window, `cie` from the
  uv-tool install (global), repo cloned fresh at `5460f467`.

## 7. README embed plan (after approval + recording)

Placement: directly under the title/badges, before "Try it in one
click…". Draft caption:

> **30-second demo** — psf/requests @ `5460f467`, indexed in 0.86s
> (858 nodes / 1,822 edges), then one agent question answered from
> cie's tools alone (`callers` → `affected_by` → `test_map`). GIF is
> edited for time only; the uncut 1:13 session is
> [`docs/demo/session-close-uncut.cast`](docs/demo/session-close-uncut.cast).
> Recorded 2026-08-31 on Linux, cie v0.1.3, Claude Code 2.1.251.

## 8. Verification log (all 2026-08-31, this machine, current tree)

- `cie index .` on requests@5460f467: **858 nodes, 1,822 edges (875
  calls), 37 files, 0.86s wall** (repo `.venv`, pass-23 tree).
- `cie init .`: registered project-scope `.mcp.json` (absolute
  `~/.local/bin/cie-mcp` + `--backend embedded --policy readonly`
  because the uv-tool install is on PATH); `claude mcp list`:
  **`cie … ✔ Connected`**.
- The §5 agent answer: real `claude -p` run, allowedTools restricted to
  the four cie tools, **1m13.6s wall** — hence the time-compression
  rule + uncut cast.
- `cie export-html . --out snapshot.html`: 224K single HTML file,
  `file://`-openable, sections Overview / Task & test chains / Atomic
  tasks / Orphans (no test, no contract) / Indexed files. No SVG graph
  inside — it is a dashboard snapshot, and the script says so (no
  graph-viz claim).
- Known wrinkle for the recording: with the uv-tool cie on PATH, `cie
  init` writes the absolute global path (good) — but that binary must
  be v0.1.3 (see §6 first bullet).

## 9. Owner approval checklist

**DECIDED 2026-08-31 (owner):**

1. Format: **A — approved.**
2. Question wording: **keep verbatim** (recommended + accepted — it is
   the exact wording of the verified §8 session; re-rolling risks a
   weaker capture).
3. End-card text: **keep as drafted** (recommended + accepted — every
   phrase is a pinned/verified fact).
4. Corpus: **psf/requests @ 5460f467** (recommended + accepted).
5. **v0.1.3 cut before recording** (required — `--backend` on screen
   must be shipped behavior; recommended + accepted).
6. Uncut cast: **committed in-repo** (`docs/demo/session-close-uncut.cast`,
   recommended + accepted — lives with the code, no external host).

Recording may proceed under these decisions; README embed + GIF
publishing happen in the same change as the recording artifacts.