---
Task ID: 8
Agent: main (Z.ai)
Task: Fix the "What CIE can't do yet (33)" section — the sample product must show actual values for every story, with a generic extract/transform/load recipe so any repo can collect the same data. PAT push, fallback ZIP.

Work Log:
- Sandbox had been reset; recovered the full project by cloning arunsoman/cie with the user's PAT (verified via GitHub API), then scrubbed the token from .git/config.
- Explored current state: web/src/lib/whats-next.ts (33 stories), menu-tab.tsx (the "can't do yet" section), cie-store.ts, chat-panel.tsx, examples/bankapp (existing measured runtime playground + collect_runtime.py).
- Built the story data pipeline (all under examples/bankapp/scripts/):
  - stories_probes.py — real probes: line-by-line replay of auth.py via sys.settrace; slow request split into compute (real pbkdf2) vs waiting (real sleeps); duplicate-delivery probe on a real in-memory ledger (measured +75 overcharge); free vs locked pipeline benches (real 3.99x speedup, real ~4x slowdown); sandbox A/B pricing variants with a real 1¢ divergence and a real empty-basket refusal; candidate spend-limit journeys (real 200/402/ValueError).
  - config/{dev,staging,prod}.json — real per-environment config; collector diffs them (feature_transfers_v2 on in staging, off in prod) and proves a flag exploration leaves prod checksum-identical.
  - scripts/stories/candidate_transfers.py + candidate_test_transfers.py — the walkthrough's candidate feature, really committed into a real temp-dir git sandbox.
  - collect_stories.py (~1500 lines) — measures/records all 33 stories: GitHub REST API with operator key in Authorization header ONLY (key health recorded, token never stored — leak-checked); git-history ownership + privacy-at-source (names salted-hashed before storage, denied paths counted never named) over the CIE repo itself; stdlib ELF parser over the real python binary (libs, symbols, entry point); coverage+profile overlays; real py_compile targets incl. a control that really fails; real pytest fail→fix→recheck loop; real git format-patch (full patch stored) + clean and conflicting apply attempts; real audit-trail ids (commit shas, patch sha256); recorded walkthrough artifacts (prompt, clarifications, brief, 500→1000 revision, wizard checkpoints with real timestamps, prepared draft PR, denied self-approval, launch checklist tied to measured stop conditions, focus-sharing contract). Output: runtime-data/stories/*.json + stories.json (23 measured + 10 recorded = 33). collect.sh now chains both collectors; RUNTIME.md documents it.
- Web rework:
  - cie-types.ts: CieStories/CieStory/StoryDemo types; cie-store.ts: loads /data/stories.json (optional), added pendingAsk + setPendingAsk.
  - lib/stories.ts: demo→ChatChartSpec builders (hbar/table reuse existing renderer w/ engine+layout disclosure, fisheye, keyboard parity) + tryAnswerStory local QA (matches US-id or title words, answers offline, nothing invented).
  - menu-tab.tsx: section renamed "All 33 of them — shown, not just promised"; cards now data-first: measured/recorded badge, source line, real chart, "Where these numbers came from", "From your repo" ETL line, real-world equivalents, "Still honest work left", honesty note, and an "Ask CIE about this" click-to-chat button; honest fallback to the old text when a project has no stories.json.
  - chat-panel.tsx: pendingAsk watcher (click-to-chat) + story answers run BEFORE the generic drawer (so "map of who knows..." hits its story, not the project map).
  - stage.tsx: menu caption updated.
  - Fixed two pre-existing TS errors in canned-charts.ts (cov/cpu possibly undefined).
- Verification: tsc clean, eslint clean, production build ✓, zero console errors after reload; agent-browser E2E at 1280px and 390px — US-043 ownership bars (web 167, crates 154...), US-005 GitHub table (connection healthy, latest commit 65dae897, rate limit 4993), US-077 config drift (DIFFERS rows), US-122 patch card, click-to-chat answer rendered from stories.json. Screenshots in download/: e2e-us043-card.png, e2e-us043-ask-chat.png, e2e-us005-github.png, e2e-mobile-us043.png, e2e-mobile-us116.png.
- Copied stories.json to web/public/data/. Committed as 4bbb0c6 and pushed to GitHub with the PAT one-shot (origin URL reset afterwards; token not persisted).

Stage Summary:
- The Menu tab no longer says "can't show": all 33 remaining stories render real collected data, each with its source, its extract→transform→load recipe for any repo, and the honest remainder.
- Click-to-chat works: any story card hands a question to Ask CIE, answered locally from stories.json (no AI round-trip, nothing invented).
- Push succeeded (4bbb0c6 on arunsoman/cie main); no ZIP fallback needed.
- Remaining honest gaps are named per-card inside the app (e.g. painting hotspots ON map bubbles, live streaming windows, PR publishing) — each is product-layer work on top of now-existing data pipelines.
