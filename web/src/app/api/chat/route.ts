// "Ask CIE" backend — receives the chat history plus a digest of the loaded
// project (built client-side by src/lib/chat-digest.ts) and answers using the
// z-ai-web-dev-sdk. Backend only: the SDK is never imported on the client.

import { NextRequest, NextResponse } from "next/server";
import ZAI from "z-ai-web-dev-sdk";

export const runtime = "nodejs";
export const maxDuration = 60;

const MAX_HISTORY = 14; // messages sent to the model (excluding system)
const MAX_CONTENT = 6000; // characters kept per message

interface ChatTurn {
  role: "user" | "assistant";
  content: string;
}

function sanitizeHistory(raw: unknown): ChatTurn[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter((m): m is ChatTurn => {
      if (!m || typeof m !== "object") return false;
      const t = m as ChatTurn;
      return (t.role === "user" || t.role === "assistant") && typeof t.content === "string" && t.content.trim().length > 0;
    })
    .map((m) => ({ role: m.role, content: m.content.slice(0, MAX_CONTENT) }))
    .slice(-MAX_HISTORY);
}

function systemPrompt(digest: unknown): string {
  const facts = JSON.stringify(digest, null, 1);
  return [
    "You are CIE's friendly explainer. CIE is a tool that reads a software project and turns it into a plain-English health report.",
    "You are chatting with the project's owner. They may be completely non-technical — a founder, a manager, a student. Be warm, patient and clear.",
    "",
    "The ONLY facts you may use are in this JSON digest of their project:",
    "----- BEGIN PROJECT FACTS -----",
    facts,
    "----- END PROJECT FACTS -----",
    "",
    "Strict rules:",
    "1. Answer ONLY from the JSON facts above. If the answer is not there, say plainly: \"I don't know that from the report I have.\" Then suggest what they could check next (for example: run another CIE command, or look at a file named in the report).",
    "2. Never invent file names, numbers, findings, promises or statistics. Guessing silently is the one unforgivable sin here.",
    "3. Use simple words and short sentences. Avoid jargon. If a technical word is unavoidable, explain it in brackets in everyday language.",
    "4. Prefer the plain titles already in the facts (e.g. 'Strangers could trick the database') over raw technical kinds (e.g. SqlInjection).",
    "5. Be honest about certainty. Facts marked as proven can be relied on; things described as CIE's guess should be presented as 'worth checking', never as certain.",
    "6. When asked what to fix first, follow the report's own ordering: 'fix soon' items first, then 'please look', then the rest. Summarise the how_to_fix hints — do not invent new remediation steps.",
    "7. Never give advice that would weaken security, and never provide code that exploits a weakness.",
    "8. Keep answers under about 150 words unless the user clearly asks for more detail. A few short paragraphs or a small bullet list is perfect. Do not use markdown headings.",
    "9. If the user asks something totally unrelated to their project, answer briefly and steer back to what CIE can tell them.",
  ].join("\n");
}

export async function POST(req: NextRequest) {
  let body: { messages?: unknown; digest?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Could not read the request. Please try again." }, { status: 400 });
  }

  const history = sanitizeHistory(body.messages);
  const last = history[history.length - 1];
  if (!last || last.role !== "user") {
    return NextResponse.json({ error: "No question was sent. Please type a question first." }, { status: 400 });
  }
  if (body.digest == null || typeof body.digest !== "object") {
    return NextResponse.json(
      { error: "No project report is loaded yet, so CIE has nothing to answer from. Load the sample project or upload your report first." },
      { status: 400 }
    );
  }

  try {
    const zai = await ZAI.create();
    const completion = await zai.chat.completions.create({
      messages: [
        { role: "assistant", content: systemPrompt(body.digest) },
        ...history,
      ],
      thinking: { type: "disabled" },
    });

    const reply = completion.choices?.[0]?.message?.content?.trim();
    if (!reply) {
      return NextResponse.json(
        { error: "CIE couldn't come up with an answer. Please try asking again." },
        { status: 502 }
      );
    }
    return NextResponse.json({ reply });
  } catch (err) {
    console.error("[api/chat] failed:", err);
    return NextResponse.json(
      { error: "CIE had trouble answering just now. Please try again in a moment." },
      { status: 500 }
    );
  }
}
