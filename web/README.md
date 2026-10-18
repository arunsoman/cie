# CIE Web — Your code, in plain words

A friendly dashboard for the CIE engine, designed so a **non-technical person**
can look at a codebase and understand it: what the code does, which promises it
makes, what to fix first, and how much to trust the tool.

Every technical term from the engine is translated to plain English in
`src/lib/plain-words.ts` — that file is the product's language policy.

## Two ways to see YOUR project

1. **Connect a repo** — paste a public git link (GitHub / GitLab / Bitbucket),
   **or point at a folder on this computer** ("On this computer" — read-only,
   analysed in place, nothing copied; system roots are refused). Local folders
   can be typed **or picked with a real Browse button**: an inline folder
   walker (`GET /api/fs/browse`, directories only — never file contents) with
   quick-jump chips for Home and common roots.
   The server clones it (`--depth 1`), runs `cie ingest → report → viz →
   concepts` in a background job, and deletes the clone the moment it's done.
   The dialog shows every state in plain words: *Getting your code → Taking
   inventory → Reading every file → Drawing the map → Listing what it does*,
   with per-step timings, honest failure cards ("That repo looks private (or
   the address is wrong)…") and a *Show my project* button at the end. The job
   keeps running if you close the dialog and the panel resumes when you reopen
   it.
   - Server pieces: `POST /api/analyze`, `GET /api/analyze/status?job=ID`,
     `GET /api/analyze/result?job=ID` (in-memory job store,
     `src/lib/analyze-jobs.ts`; URLs are validated, private/loopback hosts
     are refused).
   - The engine binary is found via `CIE_BIN`, then `target/debug|release/cie`,
     then `cie` on PATH.
   - Repos with no supported source files fail honestly instead of pretending.

   **Language inventory (the honest part).** Before the engine runs — and as a
   live preview when a local folder is picked — CIE takes inventory
   (`POST /api/repo/scan` + `src/lib/repo-scan.ts`): it counts files by
   extension using tables that **mirror the engine's adapter registry exactly**
   (`src/lib/language-profiles.ts`), reads the small manifests
   (package.json / pom.xml / build.gradle / Cargo.toml / go.mod /
   requirements.txt / composer.json / *.csproj) to name frameworks, and then
   says, in plain words:
   - **what it reads** — e.g. "Spring Boot: CIE reads every Java file —
     controllers, services, repositories — as ordinary Java" — always with an
     honesty note ("it doesn't start Spring, so the framework's magic stays
     out of the map"); same for Next.js and Rust;
   - **what it can't read yet** — e.g. "Kotlin · 12 — common in Android apps
     and Gradle builds — CIE skips these files for now", plus the sentence
     "Anything in those files won't appear in the map or the lists — and CIE
     won't guess about them";
   - the same summary shows in the status panel's *Taking inventory* step, in
     the failure card when nothing is readable, and on the **Can I trust
     this?** tab as a permanent "Languages in your repo — what CIE read, and
     what it couldn't" card.
   CIE currently reads **10 languages**: Python, JavaScript, TypeScript, Java,
   Rust, Go, C, C++, C# and PHP.
2. **Upload files** — drop in the three JSON files the `cie` CLI exports;
   nothing ever leaves your machine.

## What you get

| Tab | What it shows (plain words) |
|-----|------------------------------|
| **Start here** | 3-step story + health cards (files read, named things, promises, things to look at, % checked) + dictionary |
| **Map** | The sealed `ViewSpec` drawn as a layered picture (domain → files → use-cases → rules); tap a bubble for details |
| **What it does** | Concept/behaviour tree: "Does: Authorize", "Rule C-000: Check permission first · Written by you" |
| **Things to look at** | Findings grouped by urgency: Fix soon / Please look / Worth checking / Good to know, each with "How to fix" |
| **Promises** | Contracts with plain titles ("Check permission first") and who must keep them |
| **Numbers** | The charting engine: six plain-English charts — urgency bars, problem kinds, findings per file, what the code touches, promises donut (written by you vs CIE's guess), map-certainty donut (proven / good guess). Built with Recharts in `src/components/cie/charts.tsx` |
| **Ask CIE** | A chat about *your* project. The client compiles a capped, plain-words digest of the loaded report (`src/lib/chat-digest.ts`) and posts it with the conversation to `POST /api/chat`, which answers via the **chosen LLM provider** (see below). The system prompt forbids inventing anything: if it isn't in the report, CIE says "I don't know that from the report I have". **Answers can carry a complementary chart** the AI picks from a chat-feasible slice of the CIE chart catalog (US-020): hbar / donut / stat / table, with engine + layout disclosed on a footnote (`src/lib/chat-chart.ts` + `chat-chart.tsx`). Bars get a hover-expansion **fisheye lens** modelled on US-023 — magnify locally, base geometry never mutates, keyboard parity via arrow keys (US-140), labels always visible (US-142) |
| **Menu** | Two shortcuts on one page. **One-tap charts**: 13 pre-canned questions (grouped Problems / Promises / The map / What the code does) whose charts are computed deterministically from the loaded report — no typing, no AI round-trip; rendered by the same `ChatChart` component, so engine + layout disclosure, the fisheye lens and keyboard parity carry over (`src/lib/canned-charts.ts`). **What CIE can't do yet**: all 33 remaining Planned stories from the engine's own registry (147 stories: 113 delivered + 1 honest partial + 33 planned), hand-translated into plain words with "what you'd get" and "why it's not here yet", filterable by 12 themes (`src/lib/whats-next.ts`) |
| **Can I trust this?** | Assumptions, unmatched names, coverage notes, provenance — plus the **language honesty card** (what CIE read in your repo, and which languages it skipped) when the project came from Connect a repo. Honesty first |

## Run it

```bash
cd web
bun install        # or npm install
bun run dev        # http://localhost:3000
```

The **Ask CIE** tab works with any of three answer sources — pick via the
**“Answered by: … / Change”** bar at the top of the tab (settings live in
`src/lib/llm-config.ts` + `src/lib/llm-providers.ts`):

| Provider | What it is | Setup |
|----------|------------|-------|
| **Built-in AI** | The `z-ai-web-dev-sdk` that ships with this workspace | none — but if it's ever unreachable, connect your own below |
| **Ollama** | Your local or hosted Ollama server (native `/api/tags` + `/api/chat`) | address (default `http://localhost:11434`) + model — **Check connection** lists the models you actually have |
| **Another AI service** | Anything that speaks the OpenAI protocol: OpenAI, OpenRouter, Groq, Gemini's OpenAI layer, LM Studio (`http://localhost:1234/v1`), vLLM, llama.cpp server | preset + address + API key + model |

The **Check connection** button (`POST /api/llm/check`) honestly tests the
service and lists its real models as one-tap chips. Settings (including the
API key) live **only in your browser's localStorage** and are sent per-request
to CIE's own endpoints — never stored or logged server-side. Every provider
gets the same strict grounding prompt and the same plain-words error cards
("couldn't reach it — is it running?", "the key was rejected", "that model
isn't pulled yet — `ollama pull …`") with a collapsible technical-details
toggle; local model calls get a generous 3-minute timeout.

Without any setup the built-in AI answers; if it fails, the chat tab shows a
friendly error bubble (plus a shortcut to the connection settings) and every
other tab keeps working.

The **sample project** (a 4-file banking app) ships pre-loaded in
`public/data/` — generated by the real engine:

```bash
cie report examples/bankapp --json  > report.json
cie viz    examples/bankapp --out   viewspec.json
cie concepts examples/bankapp --format json > concepts.json
```

## Use it on your own project

Click **"Use your own"** in the header and add the JSON files the engine
creates with the two commands shown in the dialog. Everything runs in the
browser — no server, no upload, your code never leaves your machine.

## Stack

Next.js 16 (App Router) · TypeScript · Tailwind CSS 4 · shadcn/ui · Zustand.
No graph library: the map is plain SVG because the ViewSpec already encodes
the hierarchy (see `src/components/cie/map-view.tsx`). The charts use
**Recharts** (already in the shadcn ecosystem) so the numbers pages stay
declarative and accessible; the AI chat uses **z-ai-web-dev-sdk** server-side
only (`src/app/api/chat/route.ts`).
