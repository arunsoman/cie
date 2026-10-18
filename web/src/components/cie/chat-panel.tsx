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
import { BrainCog, Compass, Send, Settings2, Sparkles, Trash2, TriangleAlert, Maximize2, X, Zap } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { buildDigest } from "@/lib/chat-digest";
import { parseChartReply, isDiagramKind } from "@/lib/chat-chart";
import { tryDrawAnything, fillDiagram } from "@/lib/draw-anything";
import { ChatChart } from "@/components/cie/chat-chart";
import { providerLabel } from "@/lib/llm-config";

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

function Formatted({ text }: { text: string }) {
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
    report, viewspec, concepts, label, chat, chatBusy, pushChat, setChatBusy, clearChat,
    llm, setLlmSettingsOpen, openChartOnCanvas, mapSelected, setMapSelected, deps, runtime,
  } = useCie();
  const inputRef = useRef<HTMLInputElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  // The digest is recomputed whenever the loaded project changes.
  const digest = useMemo(
    () => buildDigest(report, viewspec, concepts, label, deps, runtime),
    [report, viewspec, concepts, label, deps, runtime]
  );

  // Keep the newest message in view.
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [chat, chatBusy]);

  const send = async (text: string) => {
    const question = text.trim();
    if (!question || chatBusy) return;

    pushChat({ role: "user", content: question });
    if (inputRef.current) inputRef.current.value = "";

    // ---- fast path: drawing requests are built locally from the loaded
    // store (draw-anything.ts) — instant, and nothing can be invented. ----
    const local = tryDrawAnything(question, report, viewspec, concepts, mapSelected, deps, runtime);
    if (local) {
      pushChat({
        role: "assistant",
        content: local.answer,
        chart: local.chart ?? undefined,
        instant: true,
      });
      return;
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
        // falls back to plain text if it doesn't cooperate.
        const parsed = parseChartReply(data.reply);
        // Diagrams: the model only picks the KIND. Boxes and arrows are
        // filled from the loaded store, so nothing can be invented.
        let chart = parsed.chart;
        if (chart && isDiagramKind(chart.kind) && !chart.nodes?.length) {
          chart = fillDiagram(chart.kind, report, viewspec, concepts, chart.title, chart.tell) ?? chart;
        }
        pushChat({ role: "assistant", content: parsed.answer, chart });
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
              {m.chart && (
                <>
                  {/* a small preview stays with the answer; the canvas carries the big one */}
                  <div className="mt-2 rounded-xl border border-stone-200 bg-stone-50/70 px-3 py-2">
                    <ChatChart spec={m.chart} />
                  </div>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="mt-2 h-8 border-emerald-200 text-emerald-800 hover:bg-emerald-50"
                    onClick={() => openChartOnCanvas(m.chart!)}
                  >
                    <Maximize2 className="h-3.5 w-3.5" /> Show this chart big on the canvas
                  </Button>
                </>
              )}
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
