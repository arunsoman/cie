// "Ask CIE" backend — receives the chat history plus a digest of the loaded
// project (built client-side by src/lib/chat-digest.ts) and answers.
//
// The user picks WHO answers (src/lib/llm-config.ts):
//   builtin — the z-ai-web-dev-sdk that ships with this app
//   ollama  — their own local or hosted Ollama server
//   openai  — any OpenAI-compatible service (OpenAI, OpenRouter, Groq,
//             Gemini's OpenAI layer, LM Studio, vLLM, llama.cpp…)
// Every provider gets the SAME system prompt (strict JSON answer contract,
// grounded only in the loaded report) and the same plain-words error mapping.

import { NextRequest, NextResponse } from "next/server";
import {
  chatOllama,
  chatOpenAi,
  LlmError,
  sanitizeLlmCfg,
  type ChatTurn,
} from "@/lib/llm-providers";

export const runtime = "nodejs";
export const maxDuration = 300;

const MAX_HISTORY = 14; // messages sent to the model (excluding system)
const MAX_CONTENT = 6000; // characters kept per message
const TIMEOUT_BUILTIN = 55_000;
const TIMEOUT_OPENAI = 90_000;
const TIMEOUT_OLLAMA = 180_000; // local models on CPU can be slow

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
    "OUTPUT FORMAT — reply with ONE JSON object and nothing else (no code fences, no prose outside the JSON):",
    '{"answer": "your plain-words answer", "chart": null}',
    "or, when a chart genuinely helps:",
    '{"answer": "your plain-words answer", "chart": { ... }}',
    "",
    "CHART MENU — pick the kind that best answers the question, and set engine and layout EXACTLY as listed (they say how the chart will be drawn):",
    '1. kind "hbar", engine "custom-svg", layout "fisheye-rows", lens true — comparing counts across categories (urgency levels, problems per file, what the code touches, which parts have the most issues). Needs 3-8 rows. data: [{"name": "...", "value": 3, "color": "rose"}, ...] with color one of rose|amber|emerald|stone by meaning.',
    '2. kind "donut", engine "recharts", layout "donut+legend" — one whole split into 2-3 parts (promises written by the developer vs CIE\'s guesses; proven vs good guesses). data: [{"name": "...", "value": 6, "color": "emerald"}, ...], and "center": "short caption under the total".',
    '3. kind "stat", engine "card", layout "hero-number" — one number that answers the question (how much was checked, how many promises). data: [{"name": "what the number means", "value": 84}], optional "unit": "%".',
    '4. kind "table", engine "grid", layout "scroll-rows" — the answer is really rows x columns (which file has which problems, who must keep which promise). "columns": [...], "rows": [[...]] — at most 8 rows, at most 4 columns.',
    '5. kind "uml", engine "custom-svg", layout "uml-boxes" — they want to SEE the shape of their code: UML, class diagram, structure, architecture, module diagram, how the parts fit together. Do NOT supply any data — the app draws the boxes and arrows itself from the loaded map (you cannot see them). Set only "title" and "tell", and "data": null.',
    '6. kind "flow", engine "custom-svg", layout "call-flow" — they want the flow: who calls whom, call graph, sequence, pipeline. Same rule: no data from you, just "title" and "tell", "data": null.',
    "Set \"chart\": null when a chart would not help (explanations, trust questions, how-to questions). Never force a chart.",
    'You may add "catalog": the id of the closest family in the full CIE chart catalog (e.g. "G5" coverage map, "H3" metrics map, "H2" FMEA table) — it is shown as a small footnote.',
    "",
    "DEPENDENCIES + RUNTIME FACTS (when present in the JSON above):",
    "- dependencies: declared packages per ecosystem, versions as written. Draw a donut (per ecosystem) or a table (per manifest) from them. Say plainly that CIE reads the lists as written and does not resolve transitive trees.",
    "- runtime_evidence: measured numbers (CPU, memory, concurrency, deadlock, race, coverage), each naming its tool. Quote them only with the tool named. If runtime_evidence is absent or a dataset is missing, say honestly that there is no collected evidence — never invent a runtime number.",
    "",
    "CHART RULES (the chart must never lie):",
    "- Every name and number must come from the JSON facts above. Never invent, estimate or round up a number that is not there.",
    "- At most 8 rows; shortest honest chart wins.",
    "- Colours by meaning only: rose = urgent / fix soon, amber = worth checking / CIE's guess, emerald = good / proven / written by the developer, stone = neutral.",
    "- Chart titles and labels in the same simple words as the answer (e.g. 'Fix soon', not 'Critical').",
    "- If the question asks about FACTS the report doesn't contain, answer honestly and set chart to null.",
    "- BUT if they ask to SEE a picture of their code (diagram, UML, architecture, flow, drawing), never refuse — pick the closest kind above. The honest 'I don't have that' is for facts, never for drawings: the app can always draw the map, the call flow, bars, a donut, a big number or a table.",
    "",
    "STRICT RULES:",
    "1. Answer ONLY from the JSON facts above. If the answer is not there, say plainly: \"I don't know that from the report I have.\" Then suggest what they could check next (for example: run another CIE command, or look at a file named in the report).",
    "2. Never invent file names, numbers, findings, promises or statistics. Guessing silently is the one unforgivable sin here.",
    "3. Use simple words and short sentences in the answer. Avoid jargon. If a technical word is unavoidable, explain it in brackets in everyday language.",
    "4. Prefer the plain titles already in the facts (e.g. 'Strangers could trick the database') over raw technical kinds (e.g. SqlInjection).",
    "5. Be honest about certainty. Facts marked as proven can be relied on; things described as CIE's guess should be presented as 'worth checking', never as certain.",
    "6. When asked what to fix first, follow the report's own ordering: 'fix soon' items first, then 'please look', then the rest. Summarise the how_to_fix hints — do not invent new remediation steps.",
    "7. Never give advice that would weaken security, and never provide code that exploits a weakness.",
    "8. Keep the answer under about 150 words unless the user clearly asks for more detail. A few short paragraphs or a small bullet list is perfect (lines starting with '- ' become bullets; **word** becomes bold). No markdown headings.",
    "9. If the user asks something totally unrelated to their project, answer briefly and steer back to what CIE can tell them.",
  ].join("\n");
}

export async function POST(req: NextRequest) {
  let body: { messages?: unknown; digest?: unknown; llm?: unknown };
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

  const llm = sanitizeLlmCfg(body.llm);
  const messages: ChatTurn[] = [
    { role: "assistant", content: systemPrompt(body.digest) },
    ...history,
  ];

  try {
    let reply: string;
    if (llm.provider === "ollama") {
      reply = await chatOllama(llm.ollama, messages, TIMEOUT_OLLAMA);
    } else if (llm.provider === "openai") {
      reply = await chatOpenAi(llm.openai, messages, TIMEOUT_OPENAI);
    } else {
      const { default: ZAI } = await import("z-ai-web-dev-sdk");
      const zai = await ZAI.create();
      const completion = await zai.chat.completions.create({
        messages,
        thinking: { type: "disabled" },
      });
      reply = completion.choices?.[0]?.message?.content?.trim() ?? "";
      if (!reply) {
        return NextResponse.json(
          { error: "CIE couldn't come up with an answer. Please try asking again." },
          { status: 502 }
        );
      }
    }
    return NextResponse.json({ reply });
  } catch (err) {
    if (err instanceof LlmError) {
      console.error(`[api/chat] ${llm.provider} failed:`, err.technical);
      return NextResponse.json(
        { error: err.plain, technical: err.technical, providerIssue: llm.provider !== "builtin" },
        { status: 502 }
      );
    }
    console.error("[api/chat] failed:", err);
    return NextResponse.json(
      { error: "CIE had trouble answering just now. Please try again in a moment." },
      { status: 500 }
    );
  }
}
