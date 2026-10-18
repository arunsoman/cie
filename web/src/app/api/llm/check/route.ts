// Connection check for the chosen LLM provider — used by the "Who answers"
// settings dialog's "Check connection" button. Lists the models the
// service actually offers (or, for the built-in AI, sends a tiny ping).

import { NextRequest, NextResponse } from "next/server";
import {
  LlmError,
  listOllamaModels,
  listOpenAiModels,
  pingBuiltin,
  sanitizeLlmCfg,
} from "@/lib/llm-providers";

export const runtime = "nodejs";
export const maxDuration = 60;

export async function POST(req: NextRequest) {
  let body: { llm?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ ok: false, error: "Could not read the request." }, { status: 400 });
  }

  const llm = sanitizeLlmCfg(body.llm);
  try {
    if (llm.provider === "ollama") {
      const models = await listOllamaModels(llm.ollama);
      return NextResponse.json({ ok: true, provider: "ollama", models });
    }
    if (llm.provider === "openai") {
      const models = await listOpenAiModels(llm.openai);
      return NextResponse.json({ ok: true, provider: "openai", models, note: models.length ? undefined : "No model list — you can still type the model name." });
    }
    await pingBuiltin();
    return NextResponse.json({ ok: true, provider: "builtin", models: [] });
  } catch (err) {
    if (err instanceof LlmError) {
      console.error(`[api/llm/check] ${llm.provider}:`, err.technical);
      return NextResponse.json({ ok: false, error: err.plain, technical: err.technical });
    }
    console.error("[api/llm/check] failed:", err);
    return NextResponse.json({ ok: false, error: "The check itself failed. Please try again." });
  }
}
