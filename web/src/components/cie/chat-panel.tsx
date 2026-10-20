"use client";

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
import { parseChartReply, isDiagramKind, type ChatChartSpec } from "@/lib/chat-chart";
import { tryDrawAnything, fillDiagram } from "@/lib/draw-anything";
import { tryAnswerStory } from "@/lib/stories";
import { tryAnswerQuality, buildCompanionChart, resolveConceptFocus, findSymbolInQuestion } from "@/lib/quality";
import { buildTraceAnswer, tryAnswerMastery, type MasteryCtx } from "@/lib/mastery";
import { MasteryCard } from "@/components/cie/mastery-cards";
import type { CieImpact } from "@/lib/cie-types";
import { providerLabel } from "@/lib/llm-config";

// Blast-radius / impact questions are the one chat path that calls the real
// engine live, on demand (cie impact --symbol) — see the fast path below.
const IMPACT_WORDS = /\b(impact|blast radius|what breaks|what('| i)s affected|minimum tests?|which tests?|test set)\b/i;

function cut8(items: string[]): string {
  if (!items.length) return "none";
  const shown = items.slice(0, 4).join(", ");
  return items.length > 4 ? `${shown}, +${items.length - 4} more` : shown;
}

const STARTERS = [
  "What should I fix first?",
  "Show me a UML diagram of this project",
  "Are there any problems with secrets or passwords?",
  "Can I trust this report? What did CIE not check?",
];

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
    llm, setLlmSettingsOpen, openChartOnCanvas, mapSelected, setMapSelected, deps, runtime,
    stories, quality, cochange, overlap, drift, pendingAsk, setPendingAsk, sourceRoot,
    pendingTrace, setPendingTrace, updateMastery, loadMastery,
  } = useCie();
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // The digest is recomputed whenever the loaded project changes.
  const digest = useMemo(
    () => buildDigest(report, viewspec, concepts, label, deps, runtime, quality, cochange, overlap, drift),
    [report, viewspec, concepts, label, deps, runtime, quality, cochange, overlap, drift]
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
      const ans = buildTraceAnswer(report, t.entryId);
      if (ans) {
        pushChat({ role: "assistant", content: ans.answer, card: ans.card, instant: true });
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
    const question = text.trim();
    if (!question || chatBusy) return;

    pushChat({ role: "user", content: question });
    if (inputRef.current) inputRef.current.value = "";

    // Core behaviour: every answer carries a chart, and the chart shows up
    // on the canvas right away — never a click needed to see it. "respond"
    // is the one place that pushes an assistant turn, so this cannot be
    // forgotten by any of the answer paths below.
    const respond = (msg: {
      content: string;
      chart?: ChatChartSpec;
      instant?: boolean;
      error?: boolean;
      technical?: string;
      providerIssue?: boolean;
    }) => {
      pushChat({ role: "assistant", ...msg });
      if (msg.chart) openChartOnCanvas(msg.chart);
    };

    // De-facto companion chart: EVERY answer carries a chart drawn from the
    // loaded exports. If the answerer found none, the companion picker picks
    // the most relevant real chart for the question (never invented — see
    // buildCompanionChart), centred on whatever the question is actually
    // about (the tapped map bubble, or a symbol named in the question)
    // rather than a fixed, unscoped default. Error bubbles stay chart-less.
    const withChart = (c: ChatChartSpec | null | undefined) =>
      c ?? buildCompanionChart(question, quality, concepts, report, mapSelected, cochange, overlap, drift) ?? undefined;

    // ---- fast path 1: story questions are answered locally from the
    // collected stories.json — instant, honest, nothing invented. Runs
    // before the general drawer so "map of who knows which part" reaches
    // its own story instead of the project map. ----
    const storyAns = tryAnswerStory(question, stories);
    if (storyAns) {
      respond({ content: storyAns.answer, chart: withChart(storyAns.chart), instant: true });
      return;
    }

    // ---- fast path 1b: quality/health questions (and every chart-element
    // tap) are answered locally from the collected quality.json — the eight
    // metrics, security findings, duplication, hot spots, drift, coverage.
    // Instant, nothing invented; unmatched questions fall through. ----
    const qualityAns = tryAnswerQuality(question, quality, concepts);
    if (qualityAns) {
      respond({ content: qualityAns.answer, chart: withChart(qualityAns.chart), instant: true });
      return;
    }

    // ---- fast path 2: the journey's canonical prompts are answered
    // locally, from the loaded exports — instant, cited, interactive cards
    // (entry points / flows / trace with the verify loop / 5 things). ----
    const masteryCtx: MasteryCtx = { report, concepts, stack, deps, cochange, overlap, drift, label };
    const masteryAns = tryAnswerMastery(question, masteryCtx);
    if (masteryAns) {
      // every answered journey prompt keeps its chart in the MESSAGE; the
      // canvas opens for diagram-bearing answers — except the trace card's
      // sequence, which the card itself opens rebuilt with the live verify state
      const traceCard = masteryAns.card?.kind === "trace";
      pushChat({
        role: "assistant",
        content: masteryAns.answer || "(the trace card below is the answer)",
        card: masteryAns.card,
        chart: masteryAns.chart ?? undefined,
        instant: true,
      });
      if (!traceCard && masteryAns.chart) openChartOnCanvas(masteryAns.chart);
      markMasteryPrompt(masteryAns.promptKey);
      return;
    }

    // ---- fast path 2b: drawing requests are built locally from the loaded
    // store (draw-anything.ts) — instant, and nothing can be invented. ----
    const local = tryDrawAnything(question, report, viewspec, concepts, mapSelected, deps, runtime);
    if (local) {
      respond({ content: local.answer, chart: withChart(local.chart), instant: true });
      return;
    }

    // ---- fast path 2c: impact / blast-radius questions call the real
    // engine live (cie impact --symbol) — the one answer this chat can't
    // pre-compute, since it needs a specific symbol as a target. Only
    // possible when the project's files are still on disk (sourceRoot);
    // a cloned URL's checkout is deleted right after ingest. ----
    if (IMPACT_WORDS.test(question)) {
      const target = findSymbolInQuestion(report, question);
      if (target) {
        if (!sourceRoot) {
          respond({
            content: `Impact analysis for \`${target.qual_name}\` needs this project's files still on disk, and they aren't — it was loaded from a cloned link (deleted right after CIE read it) or uploaded files (never saved to disk). Point CIE at a local folder on this computer to ask this.`,
            instant: true,
          });
          return;
        }
        setChatBusy(true);
        try {
          const res = await fetch("/api/impact", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ sourceRoot, symbol: target.qual_name }),
          });
          const data = (await res.json().catch(() => ({}))) as Partial<CieImpact> & { error?: string; technical?: string };
          if (!res.ok || !data.symbol) {
            respond({ content: data.error ?? "CIE couldn't work out the impact just now. Please try again.", error: true, technical: data.technical });
          } else {
            const impact = data as CieImpact;
            const rows: (string | number)[][] = [
              ["Affected symbols", impact.affected_symbols.length, cut8(impact.affected_symbols)],
              ["Minimum test set", impact.minimum_test_set.length, cut8(impact.minimum_test_set)],
              ["Affected concepts", impact.affected_concepts.length, cut8(impact.affected_concepts)],
            ];
            const chart: ChatChartSpec = {
              kind: "table",
              engine: "grid",
              layout: "scroll-rows",
              title: `Blast radius of changing '${impact.symbol}'`,
              tell: "A real dependency-graph walk from this symbol (cie impact), run just now on your actual project — never estimated.",
              columns: ["What", "Count", "Which"],
              rows,
              source: "cie impact (on-demand, live)",
            };
            respond({
              content:
                `Changing **${impact.symbol}** touches **${impact.affected_symbols.length}** symbol(s) total` +
                (impact.minimum_test_set.length
                  ? `, and the minimum test set to catch a regression is **${impact.minimum_test_set.length}** test(s): ${cut8(impact.minimum_test_set)}.`
                  : ", and CIE found no tests that cover it — a change here has no safety net.") +
                (impact.affected_concepts.length ? ` It also reaches into: ${cut8(impact.affected_concepts)}.` : ""),
              chart,
              instant: true,
            });
          }
        } catch {
          respond({ content: "The connection dropped before CIE could work out the impact. Please check your internet and try again.", error: true });
        } finally {
          setChatBusy(false);
        }
        return;
      }
    }

    setChatBusy(true);

    // History for the model: everything so far, minus old error bubbles.
    const history = [
      ...chat.filter((m) => !m.error),
      { role: "user" as const, content: question },
    ].map((m) => ({ role: m.role, content: m.content }));

    try {
      const res = await fetch("/api/chat", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          messages: history,
          digest,
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
      const data = (await res.json().catch(() => ({}))) as { reply?: string; error?: string; technical?: string; providerIssue?: boolean };
      if (!res.ok || !data.reply) {
        pushChat({
          role: "assistant",
          content: data.error ?? "CIE had trouble answering just now. Please try again.",
          error: true,
          technical: data.technical,
          providerIssue: data.providerIssue,
        });
      } else {
        // The model answers with JSON { answer, chart? }; the tolerant parser
        // falls back to plain text if it doesn't cooperate. A text-only reply
        // still gets its companion chart — every answer draws something real.
        const parsed = parseChartReply(data.reply);
        // Diagrams: the model only picks the KIND. Boxes and arrows are
        // filled from the loaded store, so nothing can be invented.
        let chart = parsed.chart;
        if (chart && isDiagramKind(chart.kind) && !chart.nodes?.length && !chart.tree?.length && !chart.tiles?.length) {
          const focusId = resolveConceptFocus(concepts, report, question, mapSelected);
          chart = fillDiagram(chart.kind, report, viewspec, concepts, chart.title, chart.tell, quality, focusId) ?? chart;
        }
        respond({ content: parsed.answer, chart: withChart(chart) });
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
            <p className="font-semibold text-stone-900">Hi! I&apos;ve read your whole project.</p>
            <p className="text-sm text-stone-500 mt-1 max-w-[240px]">
              Ask me anything about it — in your own words. Charts I draw appear here, and any
              chart can go big on the canvas.
            </p>
            <div className="mt-4 flex flex-col w-full max-w-[260px] gap-1.5">
              {STARTERS.map((s) => (
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

      {/* referent — the map bubble you picked becomes the "these" in your question */}
      {mapSelected && (
        <div className="px-4 pt-3 bg-white border-t border-stone-200" role="group" aria-label="Picked on the map">
          <span
            className="inline-flex items-center gap-1.5 rounded-full bg-emerald-50 border border-emerald-200 pl-2.5 pr-1.5 py-1 text-xs text-emerald-900 max-w-full"
            title="Picked on the Map — CIE will read your next question as being about this"
          >
            <span className="truncate font-medium">{mapSelected.label}</span>
            <button
              type="button"
              aria-label={`Stop asking about ${mapSelected.label}`}
              onClick={() => setMapSelected(null)}
              className="shrink-0 h-4 w-4 grid place-items-center rounded-full hover:bg-emerald-100 text-emerald-700"
            >
              <X className="h-3 w-3" />
            </button>
          </span>
        </div>
      )}

      {/* RFC-0024 stage 4 — the R5a CONTEXT CHIPS row lives in the Ask-CIE
          dock, above the composer (§1); the payload carries the scope at
          stage 5. */}
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
            placeholder={chatBusy ? "CIE is thinking…" : mapSelected ? `Ask about ${mapSelected.label}…` : "Type your question…"}
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
              Answered by <span className="font-medium text-stone-500">{providerLabel(llm)}</span> — from the loaded report only.
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
