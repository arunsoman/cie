"use client";
import { readStoredImpact } from "@/lib/storage-ui";

// "Ask CIE" — the chat that lives on the right side of the workspace.
//
// Layout lineage (github.com/arunsoman/code-intelligence apps/web ChatPanel):
//   the conversation is a permanent right-hand panel, not a page you visit —
//   so a chart the assistant draws can jump straight to the middle canvas,
//   and a bubble picked on the map becomes a referent chip above the
//   composer ("selected elements become the 'these' in your next message").
//
// Everything else is CIE's own honesty rules, unchanged: answers come only
// from the loaded report, "I don't know" is allowed, failed sends show a
// plain-words reason plus collapsible technical details, and every chart
// names its engine and layout on the footnote.

import { useEffect, useMemo, useRef } from "react";
import { Button } from "@/components/ui/button";
import { BrainCog, Compass, Send, Settings2, Sparkles, Trash2, TriangleAlert, X, Zap } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { ContextChips } from "./context-chips";
import { buildDigest } from "@/lib/chat-digest";
// Scoped answering composes both halves (see scope-digest.ts): the inputs are
// restricted first (scopeDigest, RFC-0023 stage 5), then the built digest is
// sliced to the chips' code space (scopeChatDigest, RFC-0024 stage 5).
import { scopeDigest, scopeChatDigest } from "@/lib/scope-digest";
import { parseChartReply, isDiagramKind, type ChatChartSpec } from "@/lib/chat-chart";
import { fillDiagram } from "@/lib/draw-anything";
import { buildCompanionChart, resolveConceptFocus } from "@/lib/quality";
import { buildTraceAnswer } from "@/lib/mastery";
import { MasteryCard } from "@/components/cie/mastery-cards";
import { providerLabel, presetFor, type LlmConfig } from "@/lib/llm-config";
// One persona across the system (RFC-0024 §14): the top-bar Atlas persona
// also picks who answers in Ask CIE — the chat's system prompt, starters
// and status line all follow it (chat-persona.ts is the honest core + the
// five voices).
import { CHAT_PERSONAS, DEFAULT_CHAT_PERSONA, sanitizePersonaId, voiceInstant } from "@/lib/chat-persona";

/** Is the configured provider actually answerable, or would its turn just
 *  error? builtin always ships; ollama/openai need a model (and openai a
 *  key when its preset needs one) — without those the hybrid stays with
 *  the instant answer instead of stacking an error bubble on it. */
const llmUsable = (cfg: LlmConfig): boolean =>
  cfg.provider === "builtin" ||
  (cfg.provider === "ollama"
    ? cfg.ollama.model.trim().length > 0
    : cfg.openai.model.trim().length > 0 &&
      (!presetFor(cfg.openai.preset).needsKey || cfg.openai.apiKey.trim().length > 0));

const GREETING = CHAT_PERSONAS[DEFAULT_CHAT_PERSONA].greeting; // fallback if a voice omitted its greeting

// Tiny safe renderer for CIE's answers: **bold** → <strong>, lines that start
// with "-" become bullet lists. No dangerouslySetInnerHTML, no other markdown.
function renderInline(text: string, keyPrefix: string): React.ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*)/g).map((part, i) =>
    part.startsWith("**") && part.endsWith("**") && part.length > 4 ? (
      <strong key={`${keyPrefix}-${i}`}>{part.slice(2, -2)}</strong>
    ) : (
      <span key={`${keyPrefix}-${i}`}>{part}</span>
    )
  );
}

export function Formatted({ text }: { text: string }) {
  const out: React.ReactNode[] = [];
  let bullets: React.ReactNode[] = [];
  const flush = (key: string | number) => {
    if (bullets.length) {
      out.push(
        <ul key={`ul-${key}`} className="list-disc pl-4 space-y-1 my-1.5">
          {bullets}
        </ul>
      );
      bullets = [];
    }
  };
  text.split("\n").forEach((line, i) => {
    const bullet = line.match(/^\s*[-•]\s+(.*)$/);
    if (bullet) {
      bullets.push(<li key={i}>{renderInline(bullet[1], `b${i}`)}</li>);
    } else {
      flush(i);
      if (line.trim()) {
        out.push(
          <p key={i} className="first:mt-0 last:mb-0">
            {renderInline(line, `p${i}`)}
          </p>
        );
      }
    }
  });
  flush("end");
  return <div className="space-y-0.5">{out}</div>;
}

export function ChatPanel() {
  const {
    report, viewspec, concepts, label, stack, chat, chatBusy, pushChat, setChatBusy, clearChat,
    llm, setLlmSettingsOpen, openChartOnCanvas, openConceptOnCanvas, mapSelected, setMapSelected,
    deps, runtime, quality, cochange, overlap, drift, pendingAsk, setPendingAsk, sourceRoot: liveSourceRoot,
    pendingTrace, setPendingTrace, updateMastery, loadMastery, chatContext, persona,
  } = useCie();
  const sourceRoot = useCie(s => s.storageRoot) ?? liveSourceRoot;
  const voice = CHAT_PERSONAS[sanitizePersonaId(persona)];
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const answerData = useMemo(() => {
    const scoped = scopeDigest(report, viewspec, concepts, chatContext);
    // Aggregate exports have no selection-level attestation. Do not pass them
    // to the answer pipeline in a scoped answer.
    return {...scoped, quality:scoped.scope ? null : quality,
      stack:scoped.scope ? null : stack, deps:scoped.scope ? null : deps,
      runtime:scoped.scope ? null : runtime, cochange:scoped.scope ? null : cochange,
      overlap:scoped.scope ? null : overlap, drift:scoped.scope ? null : drift};
  }, [report, viewspec, concepts, chatContext, quality, stack, deps, runtime, cochange, overlap, drift]);
  const digest = useMemo(() => {
    const d = answerData;
    const facts = buildDigest(d.report, d.viewspec, d.concepts, label, d.deps, d.runtime, d.quality, d.cochange, d.overlap, d.drift);
    return facts && {...facts, answer_scope:d.scope};
  }, [answerData, label]);

  // RFC-0024 stage 5 — the chips scope the digest (R5): chips present →
  // ONLY the selection's code space is sent, with the honest scope line.
  const scopedDigest = useMemo(
    () => scopeChatDigest(digest as Record<string, unknown> | null, chatContext),
    [digest, chatContext]
  );

  // Keep the newest message in view.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [chat, chatBusy]);

  // Click-to-chat: any chart/card can hand a question here. The chat asks
  // it exactly as if the user had typed it, then clears the hand-off.
  useEffect(() => {
    if (pendingAsk && !chatBusy) {
      const q = pendingAsk;
      setPendingAsk(null);
      void send(q);
    }
  }, [pendingAsk]);

  // The journey's flow candidates hand a trace here — pushed as a real user
  // turn so the conversation keeps telling the truth about what was asked.
  useEffect(() => {
    if (pendingTrace && !chatBusy && report) {
      const t = pendingTrace;
      setPendingTrace(null);
      pushChat({ role: "user", content: `Trace ${t.title}, end to end` });
      const ans = answerData.scope ? null : buildTraceAnswer(report, t.entryId);
      if (ans) {
        pushChat({ role: "assistant", content: voiceInstant(persona, "mastery", ans.answer), card: ans.card, instant: true });
        markMasteryPrompt(ans.promptKey);
      }
    }
  }, [pendingTrace]);

  /** the journey's progress chips update when their prompt was actually answered */
  const markMasteryPrompt = (key: string | undefined) => {
    if (!key) return;
    loadMastery(); // ensures the right person's state on a fresh session
    const m = useCie.getState().mastery;
    if (m && !m.prompts[key]) updateMastery((mm) => ({ ...mm, prompts: { ...mm.prompts, [key]: Date.now() } }));
  };

  const send = async (text: string) => {
    const {report, viewspec, concepts, quality, stack, deps, runtime, cochange, overlap, drift, scope} = answerData;
    const question = text.trim();
    if (!question || chatBusy) return;

    pushChat({ role: "user", content: question });
    if (inputRef.current) inputRef.current.value = "";

    // Core behaviour: EVERY question is handled by the configured LLM —
    // one answering path, no instant fast paths. The server runs the store
    // agent (cie_charts + cie_describe + cie_retrieve): the data the answer
    // is formed from comes from the store; when it cannot find what the
    // question needs, the model must say so plainly; when it can, the model
    // also picks the complementary chart — only among the kinds cie_charts
    // marked possible — and the app fills that kind's boxes and arrows from
    // the loaded records, so nothing is invented anywhere in between.
    const respond = (msg: {
      content: string;
      chart?: ChatChartSpec;
      error?: boolean;
      technical?: string;
      providerIssue?: boolean;
    }) => {
      const content = scope ? `${scope.header}\n\n${msg.content}` : msg.content;
      pushChat({ role: "assistant", ...msg, content });
      if (msg.chart) openChartOnCanvas(msg.chart);
    };

    // Companion chart: when the model's answer carries no picture (it chose
    // none from the possible list, or answered plain text), the most
    // relevant chart that IS grounded in the loaded exports accompanies the
    // answer — centred on what the question is about (the tapped map
    // bubble, or a symbol named in the question). Never invented.
    const withChart = (c: ChatChartSpec | null | undefined) =>
      c ?? buildCompanionChart(question, quality, concepts, report, mapSelected, cochange, overlap, drift) ?? undefined;

    if (!llmUsable(llm)) {
      respond({
        content:
          llm.provider === "builtin"
            ? "The built-in answering model failed to load. Check the connection settings and try again."
            : `No answering model is configured yet — set the ${providerLabel(llm)} model in the connection settings, then ask again.`,
        providerIssue: llm.provider !== "builtin",
      });
      return;
    }

    setChatBusy(true);

    // History for the model: everything so far, minus the old error bubbles.
    const history = [
      ...(scope ? [] : chat.filter((m) => !m.error)),
      { role: "user" as const, content: question },
    ].map((m) => ({ role: m.role, content: m.content }));

    try {
      const res = await fetch("/api/chat", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          messages: history,
          // When a store is connected, the SERVER lets the configured LLM
          // drive the store's read-only tools (cie_charts + cie_describe +
          // cie_retrieve) — the model fetches what the answer needs, then
          // answers with the chart kind it picked from the possible list.
          root: sourceRoot || undefined,
          // The payload carries BOTH scope halves: `scope` is the input-level
          // attestation (RFC-0023 — the route refuses scoped questions without
          // selection-filtered facts), and the digest is SLICED to the chips'
          // code space with its binding selection_scope manifest (RFC-0024
          // stage 5 — the system prompt holds the model to it).
          digest: scopedDigest?.scoped ?? digest,
          scope,
          persona: voice.id, // the top-bar persona answers here too — the system prompt, starters and status line follow it
          llm: {
            provider: llm.provider,
            ollama: { baseUrl: llm.ollama.baseUrl.trim(), model: llm.ollama.model.trim() },
            openai: {
              baseUrl: llm.openai.baseUrl.trim(),
              apiKey: llm.openai.apiKey.trim(),
              model: llm.openai.model.trim(),
            },
          },
        }),
      });
      const data = (await res.json().catch(() => ({}))) as { reply?: string; error?: string; technical?: string; providerIssue?: boolean; toolsUsed?: string[]; hops?: number };
      if (!res.ok || !data.reply) {
        pushChat({
          role: "assistant",
          content: data.error ?? "CIE had trouble answering just now. Please try again.",
          error: true,
          technical: undefined,
          providerIssue: data.providerIssue,
        });
      } else {
        // The model answers with JSON { answer, chart? }; the tolerant parser
        // falls back to plain text if it doesn't cooperate. A text-only
        // answer still gets its grounded companion chart.
        const parsed = parseChartReply(data.reply);
        // Diagrams: the model only picks the KIND. Boxes and arrows are
        // filled from the loaded records, so nothing can be invented.
        let chart = parsed.chart;
        if (chart && isDiagramKind(chart.kind) && !chart.nodes?.length && !chart.tree?.length && !chart.tiles?.length) {
          const focusId = resolveConceptFocus(concepts, report, question, mapSelected);
          chart = fillDiagram(chart.kind, report, viewspec, concepts, chart.title, chart.tell, quality, focusId, stack) ?? chart;
        }
        const grounded = data.toolsUsed?.length
          ? `\n\n*(The model looked up ${data.toolsUsed.length} thing(s) in the store itself — ${[...data.toolsUsed].join(", ")} — before answering.)*`
          : "";
        respond({ content: parsed.answer + grounded, chart: withChart(chart) });
      }
    } catch {
      pushChat({
        role: "assistant",
        content: "The connection dropped before CIE could answer. Please check your internet and try again.",
        error: true,
      });
    } finally {
      setChatBusy(false);
    }
  };

  const canAsk = !!report;

  return (
    <section className="chat flex flex-col h-full min-h-0" aria-label="Ask CIE">
      {/* header */}
      <div className="flex items-center justify-between gap-2 px-4 py-3 border-b border-stone-200 bg-white">
        <h2 className="text-sm font-bold text-stone-900 flex items-center gap-2">
          <Sparkles className="h-4 w-4 text-emerald-700" /> Ask CIE
          <span className="rounded-full bg-stone-100 px-2 py-0.5 text-[10.5px] font-medium text-stone-500" title="Set in the Atlas menu — changes who answers here">
            {voice.voiceTag}
          </span>
        </h2>
        {chat.length > 0 && (
          <button
            type="button"
            onClick={clearChat}
            disabled={chatBusy}
            className="text-xs text-stone-500 hover:text-stone-800 disabled:opacity-50"
            aria-label="Start a new conversation"
          >
            ＋ New conversation
          </button>
        )}
      </div>

      {/* conversation — docked (desktop) or capped (mobile) */}
      <div ref={scrollRef} className="flex-1 min-h-0 overflow-y-auto max-lg:max-h-[440px] p-4 space-y-4 bg-stone-50/60">
        {chat.length === 0 && (
          <div className="h-full flex flex-col items-center justify-center text-center px-2">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-100 mb-3">
              <Compass className="h-6 w-6 text-emerald-700" />
            </div>
            <p className="font-semibold text-stone-900">{voice.greeting ?? GREETING}</p>
            <p className="text-sm text-stone-500 mt-1 max-w-[260px]">
              {voice.tagline} Charts I draw appear here, and any chart can
              go big on the canvas.
            </p>
            <div className="mt-4 flex flex-col w-full max-w-[280px] gap-1.5">
              {voice.starters.map((s) => (
                <button
                  key={s}
                  type="button"
                  onClick={() => void send(s)}
                  disabled={!canAsk || chatBusy}
                  className="rounded-full border border-stone-200 bg-white px-3 py-1.5 text-xs text-stone-700 hover:border-emerald-300 hover:bg-emerald-50 hover:text-emerald-900 transition-colors disabled:opacity-50"
                >
                  {s}
                </button>
              ))}
            </div>
          </div>
        )}

        {chat.map((m, i) => (
          <div key={i} className={`flex ${m.role === "user" ? "justify-end" : "justify-start"}`}>
            <div
              className={`rounded-2xl px-3.5 py-2.5 text-sm leading-relaxed ${
                m.role === "user"
                  ? "max-w-[88%] bg-emerald-700 text-white rounded-br-md"
                  : m.error
                    ? "max-w-[92%] bg-rose-50 text-rose-900 border border-rose-200 rounded-bl-md"
                    : "w-full bg-white text-stone-800 border border-stone-200 rounded-bl-md shadow-sm"
              }`}
            >
              {m.error && (
                <span className="flex items-center gap-1.5 font-semibold mb-1">
                  <TriangleAlert className="h-3.5 w-3.5" /> Hmm, that didn&apos;t work
                </span>
              )}
              {m.role === "user" ? m.content : <Formatted text={m.content} />}
              {m.card && (
                <div className="mt-2">
                  <MasteryCard card={m.card} chart={m.chart} />
                </div>
              )}
              {m.instant && (
                <p className="mt-2 pt-1.5 border-t border-stone-100 text-[10px] text-stone-400 flex items-center gap-1">
                  <Zap className="h-3 w-3 text-amber-500" /> Drawn instantly from your loaded report — no AI involved, nothing invented.
                </p>
              )}
              {m.technical && (
                <details className="mt-1.5 text-[11px] text-stone-400">
                  <summary className="cursor-pointer select-none hover:text-stone-500">Technical details</summary>
                  <p className="mt-1 font-mono break-all whitespace-pre-wrap">{m.technical}</p>
                </details>
              )}
              {m.providerIssue && (
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className="mt-2 h-8 border-rose-200 text-rose-800 hover:bg-rose-50"
                  onClick={() => setLlmSettingsOpen(true)}
                >
                  <Settings2 className="h-3.5 w-3.5" /> Check the connection settings
                </Button>
              )}
              {/* The chart itself lives only on the canvas — opened automatically
                  the moment this answer arrives, nothing duplicated here and
                  no click needed to see it, not even a link back to it. */}
            </div>
          </div>
        ))}

        {chatBusy && (
          <div className="flex justify-start">
            <div className="bg-white border border-stone-200 rounded-2xl rounded-bl-md px-4 py-3 flex items-center gap-1.5 shadow-sm" aria-label="CIE is thinking">
              {[0, 1, 2].map((i) => (
                <span
                  key={i}
                  className="h-2 w-2 rounded-full bg-stone-400 animate-bounce"
                  style={{ animationDelay: `${i * 0.15}s` }}
                />
              ))}
            </div>
          </div>
        )}
      </div>

      {/* RFC-0024 stage 4 — the R5a CONTEXT CHIPS row lives in the Ask-CIE
          dock, above the composer (§1); the payload carries the scope at
          stage 5. A stray second mount used to render every chip twice. */}
      <ContextChips />

      {/* composer */}
      <div className="p-3.5 bg-white border-t border-stone-200">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void send(inputRef.current?.value ?? "");
          }}
          className="flex items-center gap-2"
        >
          <input
            ref={inputRef}
            placeholder={
              chatBusy
                ? "CIE is thinking…"
                : scopedDigest?.scopeLine
                  ? `Ask about your selection — ${scopedDigest.scopeLine}`
                  : mapSelected
                    ? `Ask about ${mapSelected.label}…`
                    : "Type your question…"
            }
            disabled={chatBusy || !canAsk}
            maxLength={2000}
            aria-label="Your question"
            className="flex-1 h-10 rounded-xl border border-stone-200 bg-stone-50 px-3.5 text-sm text-stone-900 placeholder:text-stone-400 focus:outline-none focus:ring-2 focus:ring-emerald-600/30 focus:border-emerald-300 disabled:opacity-60"
          />
          <Button
            type="submit"
            size="icon"
            disabled={chatBusy || !canAsk}
            aria-label="Send question"
            className="h-10 w-10 rounded-xl bg-emerald-700 hover:bg-emerald-800 text-white shrink-0"
          >
            <Send className="h-4 w-4" />
          </Button>
          {chat.length > 0 && (
            <Button
              type="button"
              size="icon"
              variant="outline"
              disabled={chatBusy}
              onClick={clearChat}
              aria-label="Clear conversation"
              title="Clear conversation"
              className="h-10 w-10 rounded-xl shrink-0"
            >
              <Trash2 className="h-4 w-4 text-stone-500" />
            </Button>
          )}
        </form>
        <div className="mt-2 flex items-center justify-between gap-2">
          <p className="text-[10.5px] text-stone-400 leading-snug flex items-center gap-1 min-w-0">
            <BrainCog className="h-3 w-3 shrink-0" />
            <span className="truncate">
              Answered by <span className="font-medium text-stone-500">{providerLabel(llm)}</span> · {voice.voiceTag} — from the loaded report only.
            </span>
          </p>
          <button
            type="button"
            onClick={() => setLlmSettingsOpen(true)}
            className="text-[10.5px] text-emerald-700 hover:text-emerald-900 shrink-0 font-medium"
          >
            Change
          </button>
        </div>
      </div>
    </section>
  );
}
