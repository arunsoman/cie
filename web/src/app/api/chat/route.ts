// "Ask CIE" backend — receives the chat history plus a digest of the loaded
// project (built client-side by src/lib/chat-digest.ts) and answers.
//
// The user picks WHO answers (src/lib/llm-config.ts):
//   builtin — the z-ai-web-dev-sdk that ships with this app
//   ollama  — their own local or hosted Ollama server
//   openai  — any OpenAI-compatible service (OpenAI, OpenRouter, Groq,
//             Gemini's OpenAI layer, LM Studio, vLLM, llama.cpp…)
// Every provider gets the SAME persona-composed system prompt. The persona
// is the top-bar Atlas persona (RFC-0024 §14, one persona across the
// system): it changes who CIE is, how it speaks and what it prioritizes —
// never the honesty core (src/lib/chat-persona.ts composes the evidence
// rules persona-invariant, AFTER any persona fragment).

import { NextRequest, NextResponse } from "next/server";
import { buildSystemPrompt, sanitizePersonaId } from "@/lib/chat-persona";
import {
  chatOllama,
  chatOpenAi,
  chatPlain,
  chatWithTools,
  LlmError,
  sanitizeLlmCfg,
  type ChatTurn,
} from "@/lib/llm-providers";
import {
  executeStoreTool,
  runStoreAgentTurn,
  agentToolDefs,
  toolPromptFragment,
  TOOL_LOOP_MAX_HOPS,
} from "@/lib/chat-tools";
import { validateLocalPath } from "@/lib/analyze-jobs";

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

export async function POST(req: NextRequest) {
  let body: { messages?: unknown; digest?: unknown; scope?: unknown; llm?: unknown; persona?: unknown; root?: unknown };
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
  const persona = sanitizePersonaId(body.persona);
  const scoped = body.scope != null;
  if (scoped && !(body.digest as Record<string, unknown>).answer_scope) {
    return NextResponse.json({error:"A scoped question requires selection-filtered facts."}, {status:400});
  }
  // The store agent needs a validated local repo root — the panel sends it
  // only when a store is actually connected. Without it the tools honestly
  // say so (their teaching error returns as a tool result) and the digest
  // stays the grounding.
  const rootCheck = body.root != null ? validateLocalPath(String(body.root)) : null;
  const root = rootCheck?.ok ? rootCheck.path : null;

  const systemPrompt = buildSystemPrompt(persona, body.digest, scoped);
  const messages: ChatTurn[] = [
    { role: "assistant", content: systemPrompt },
    ...(scoped ? [last] : history),
  ];

  // ---- the store agent: EVERY question is handled by the configured LLM.
  // It drives cie_charts (which charts are possible for what is loaded),
  // cie_describe + cie_retrieve (the store's own read-only tools), forms
  // the answer's data from what they returned (and says plainly when it
  // could not find an answer), and picks the complementary chart kind from
  // the possible list. Scoped questions go straight to the plain path —
  // the store has no selection filter, so scoped chats must not read past
  // their chips' attestation. The plain path below stays the fallback for
  // providers without tool support. ----
  const timeoutByProvider = llm.provider === "ollama" ? TIMEOUT_OLLAMA : llm.provider === "openai" ? TIMEOUT_OPENAI : TIMEOUT_BUILTIN;
  if (!scoped) {
    try {
      const agent = await runStoreAgentTurn(
        (turns) => chatWithTools(llm, turns, agentToolDefs()),
        [{ role: "assistant", content: systemPrompt + "\n\n" + toolPromptFragment() }, ...messages.slice(1)],
        (name, args) => executeStoreTool(root, (body.digest ?? null) as Record<string, unknown> | null, name, args),
        {
          maxHops: TOOL_LOOP_MAX_HOPS,
          chatPlain: (turns) => chatPlain(llm, turns as ChatTurn[], timeoutByProvider),
        },
      );
      if (agent.reply) {
        return NextResponse.json({
          reply: agent.reply,
          toolsUsed: agent.toolsUsed,
          hops: agent.hops,
        });
      }
      // empty final answer — fall through to the plain single call below
    } catch (err) {
      console.warn(`[api/chat] store agent fell back to the digest answer:`, err instanceof LlmError ? err.technical : (err as Error).message);
      // fall through: the digest-only path answers (works for models without
      // tool support; the tools were an upgrade, never a hard requirement)
    }
  }

  try {
    let reply: string;
    if (llm.provider === "ollama") {
      reply = await chatOllama(llm.ollama, messages, TIMEOUT_OLLAMA);
    } else if (llm.provider === "openai") {
      reply = await chatOpenAi(llm.openai, messages, TIMEOUT_OPENAI);
    } else {
      // .env.local can drive the built-in Z.ai path directly — server
      // env first (ZAI_API_KEY, ZAI_BASE_URL), the SDK's own .z-ai-config
      // file second. No SDK fork: the runtime constructor takes the config
      // object, the type surface just keeps it private, so a narrow cast
      // is the honest bridge; fail is as honest as the file path's.
      const { chatBuiltin } = await import("@/lib/llm-providers");
      reply = await chatBuiltin(messages, TIMEOUT_BUILTIN);
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
