"use client";

// "Ask CIE" — a chat where the owner asks questions about their project and
// CIE answers only from the loaded report. Kept in the shared store so the
// conversation survives tab switches.

import { useEffect, useMemo, useRef, useState } from "react";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { TabsContent } from "@/components/ui/tabs";
import { MessageCircle, Send, Sparkles, Trash2, TriangleAlert } from "lucide-react";
import { useCie } from "@/lib/cie-store";
import { buildDigest } from "@/lib/chat-digest";
import { parseChartReply } from "@/lib/chat-chart";
import { ChatChart } from "@/components/cie/chat-chart";

const STARTERS = [
  "What should I fix first?",
  "Which files have the most problems?",
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

export function AskTab() {
  const { report, viewspec, concepts, label, chat, chatBusy, pushChat, setChatBusy, clearChat } = useCie();
  const [input, setInput] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);

  // The digest is recomputed whenever the loaded project changes.
  const digest = useMemo(
    () => buildDigest(report, viewspec, concepts, label),
    [report, viewspec, concepts, label]
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
    setInput("");
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
        body: JSON.stringify({ messages: history, digest }),
      });
      const data = (await res.json().catch(() => ({}))) as { reply?: string; error?: string };
      if (!res.ok || !data.reply) {
        pushChat({
          role: "assistant",
          content: data.error ?? "CIE had trouble answering just now. Please try again.",
          error: true,
        });
      } else {
        // The model answers with JSON { answer, chart? }; the tolerant parser
        // falls back to plain text if it doesn't cooperate.
        const parsed = parseChartReply(data.reply);
        pushChat({ role: "assistant", content: parsed.answer, chart: parsed.chart });
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

  return (
    <TabsContent value="ask" className="space-y-4">
      <Card className="border-stone-200 bg-stone-50/60">
        <CardContent className="p-5 flex items-start gap-3 text-sm text-stone-700 leading-relaxed">
          <MessageCircle className="h-5 w-5 text-emerald-700 shrink-0 mt-0.5" />
          <p>
            <span className="font-semibold">Ask anything about your project. </span>
            CIE answers using only the report you have loaded — it will tell you honestly when it
            doesn&apos;t know, and it never invents an answer.
          </p>
        </CardContent>
      </Card>

      <Card className="border-stone-200">
        <CardContent className="p-0">
          {/* conversation */}
          <div ref={scrollRef} className="h-[440px] overflow-y-auto p-4 sm:p-5 space-y-4">
            {chat.length === 0 && (
              <div className="h-full flex flex-col items-center justify-center text-center px-4">
                <div className="flex h-12 w-12 items-center justify-center rounded-full bg-emerald-100 mb-3">
                  <Sparkles className="h-6 w-6 text-emerald-700" />
                </div>
                <p className="font-semibold text-stone-900">Hi! I&apos;ve read your whole project.</p>
                <p className="text-sm text-stone-500 mt-1 max-w-sm">
                  Ask me anything about it — in your own words. Or start with one of these:
                </p>
                <div className="mt-4 flex flex-wrap justify-center gap-2">
                  {STARTERS.map((s) => (
                    <button
                      key={s}
                      type="button"
                      onClick={() => void send(s)}
                      className="rounded-full border border-stone-200 bg-white px-3.5 py-2 text-sm text-stone-700 hover:border-emerald-300 hover:bg-emerald-50 hover:text-emerald-900 transition-colors"
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
                  className={`rounded-2xl px-4 py-2.5 text-sm leading-relaxed ${
                    m.role === "user"
                      ? "max-w-[85%] sm:max-w-[75%] bg-emerald-700 text-white rounded-br-md"
                      : m.error
                        ? "max-w-[85%] sm:max-w-[75%] bg-rose-50 text-rose-900 border border-rose-200 rounded-bl-md"
                        : m.chart
                          ? "w-full sm:max-w-[92%] bg-stone-100 text-stone-800 border border-stone-200 rounded-bl-md"
                          : "max-w-[85%] sm:max-w-[75%] bg-stone-100 text-stone-800 border border-stone-200 rounded-bl-md"
                  }`}
                >
                  {m.error && (
                    <span className="flex items-center gap-1.5 font-semibold mb-1">
                      <TriangleAlert className="h-3.5 w-3.5" /> Hmm, that didn&apos;t work
                    </span>
                  )}
                  {m.role === "user" ? m.content : <Formatted text={m.content} />}
                  {m.chart && <ChatChart spec={m.chart} />}
                </div>
              </div>
            ))}

            {chatBusy && (
              <div className="flex justify-start">
                <div className="bg-stone-100 border border-stone-200 rounded-2xl rounded-bl-md px-4 py-3 flex items-center gap-1.5" aria-label="CIE is thinking">
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

          {/* composer */}
          <div className="border-t border-stone-200 p-3 sm:p-4">
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void send(input);
              }}
              className="flex items-center gap-2"
            >
              <input
                value={input}
                onChange={(e) => setInput(e.target.value)}
                placeholder={chatBusy ? "CIE is thinking…" : "Type your question…"}
                disabled={chatBusy}
                maxLength={2000}
                aria-label="Your question"
                className="flex-1 h-11 rounded-xl border border-stone-200 bg-white px-4 text-sm text-stone-900 placeholder:text-stone-400 focus:outline-none focus:ring-2 focus:ring-emerald-600/30 focus:border-emerald-300 disabled:opacity-60"
              />
              <Button
                type="submit"
                size="icon"
                disabled={chatBusy || !input.trim()}
                aria-label="Send question"
                className="h-11 w-11 rounded-xl bg-emerald-700 hover:bg-emerald-800 text-white shrink-0"
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
                  className="h-11 w-11 rounded-xl shrink-0"
                >
                  <Trash2 className="h-4 w-4 text-stone-500" />
                </Button>
              )}
            </form>
            <p className="mt-2 text-[11px] text-stone-400 leading-relaxed">
              CIE only knows what&apos;s in the report you loaded ({label || "no project yet"}). For fresh
              answers about new changes, export the files again and upload them above.
            </p>
          </div>
        </CardContent>
      </Card>
    </TabsContent>
  );
}
