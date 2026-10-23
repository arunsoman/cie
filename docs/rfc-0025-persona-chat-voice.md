# RFC-0025 — one persona across the system: the chat adopts the atlas voice

Status: implemented. Depends on RFC-0024 (persona presets) and the Ask-CIE
chat (RFC-0023 §9, RFC-0024 §3 stage 5). Adds no RFC entity.

## 1 · Why

RFC-0024 §14 put five personas in the top bar — Onboarding (Fresher),
Reviewer, Architect/Maintainer, Auditor/Security, Consultant (foreign
repo) — and each re-planned the Atlas. The chat did not care: `/api/chat`
hardcoded ONE system prompt ("CIE's friendly explainer") for every persona
and every provider. A user switching to Auditor / Security in the top bar
got an exact Atlas and a chatty explainer in the dock.

This RFC connects the two: the selected persona now selects the chat voice
too. One persona, one intent, spoken by every part of the product that
speaks. Prompts change across the system according to the persona chosen.

## 2 · What changes per persona — and what never changes

The persona changes three things and nothing else:

1. **Who CIE answers as** (identity block: reviewer/architect/auditor/
   consultant keep the fresher's friendly explainer for onboarding).
2. **How it speaks** (audience, vocabulary, length budget, format; e.g. the
   auditor leads with severity order, the architect speaks structure and
   cites names, the teacher defines every term in brackets).
3. **What it prioritizes when reading the SAME facts** (auditor quotes
   runtime evidence tool-named and names absent datasets; consultant
   balances strengths and risks; the fresher adds no priorities at all).

The honesty core is **persona-invariant** and enforced by composition
order (`web/src/lib/chat-persona.ts`): the JSON answer contract, the chart
menu with its engine/layout claims, the chart rules ("the chart must never
lie") and the STRICT RULES are composed AFTER every persona fragment, and
fragment sizes are capped (`intro 5, voice 6, focus 6, chartBias 3,
extraRules 4` — slice-enforced). A persona can ADD rules (they continue
the STRICT RULES numbering) but cannot override, reorder or weaken a core
line. Scoped answering (RFC-0024 stage 5) also outranks the persona: the
selection-scope law is composed after the voice, unchanged for all five.

Deliberately persona-neutral: the deterministic fast paths (stories,
quality, polyglot/concept draws, draw-anything, impact) — they are instant
local re-reads marked "no AI involved, nothing invented"; a persona voice
would put words in canned evidence.

## 3 · Wire-up

| File | Change |
|---|---|
| `web/src/lib/chat-persona.ts` | NEW — `CHAT_PERSONAS` (five voices), `sanitizePersonaId`, `buildSystemPrompt(persona, digest, scoped)` (moved from the route), fragment caps |
| `web/src/app/api/chat/route.ts` | body gains `persona`; system prompt = `buildSystemPrompt(...)`; unknown/missing id ≠ error (falls back to fresher) |
| `web/src/components/cie/chat-panel.tsx` | POST carries `persona`; persona-specific starters + tagline; header persona tag; footer "Answered by … · Auditor voice" |
| `web/test/chat-persona.test.cjs` | NEW — core-invariance, composition order, 10 pairwise diffs, caps, fallback, fresher regression, scoped-per-persona |

First-run default stays `fresher` (ux-spec §14.2-6), whose voice reproduces
the original prompt's identity lines verbatim — a visitor who never opens
the persona menu sees the exact previous behavior.

## 4 · Acceptance

- For all five personas the built prompt contains every honesty-core
  marker (menu engine/layout strings, chart rules, strict rules, evidence
  dataset explanations) — asserted in `chat-persona.test.cjs`.
- All 10 persona-pair prompts differ in their identity line.
- Fresh-scoped: the scope law + honesty line survive for all personas.
- Fallback: `"bogus"`, null, 123, missing → byte-equal to fresher.
- Full web suite: 99/99 node tests green.