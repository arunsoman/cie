// Server-side LLM callers for "Ask CIE". One function per provider, plus a
// model-list checker, all mapping failures to plain-words reasons.
//
// NOTE ON PRIVATE HOSTS: unlike the repo-analyzer (which refuses private
// addresses because it fetches arbitrary internet URLs), these callers
// DELIBERATELY allow localhost / private networks — pointing CIE at Ollama
// or LM Studio on your own machine is the whole point. Only http(s) URLs
// are accepted.

export type Provider = "builtin" | "ollama" | "openai";

export interface OllamaCfg {
  baseUrl: string;
  model: string;
}
export interface OpenAICfg {
  baseUrl: string;
  apiKey: string;
  model: string;
}
export interface LlmCfg {
  provider: Provider;
  ollama: OllamaCfg;
  openai: OpenAICfg;
}

export class LlmError extends Error {
  plain: string;
  technical: string;
  constructor(plain: string, technical: string) {
    super(plain);
    this.plain = plain;
    this.technical = technical;
  }
}

function normalizeBase(raw: string, what: string): string {
  const base = (raw || "").trim().replace(/\/+$/, "");
  if (!base) throw new LlmError(`No ${what} address was given. Open the connection settings and add one.`, "empty baseUrl");
  let url: URL;
  try {
    url = new URL(base);
  } catch {
    throw new LlmError(`That ${what} address doesn't look right — check it for typos.`, `invalid URL: ${base}`);
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new LlmError(`The ${what} address must start with http:// or https://.`, `bad protocol: ${url.protocol}`);
  }
  return base;
}

async function fetchJson(url: string, init: RequestInit & { signal: AbortSignal }): Promise<Response> {
  try {
    return await fetch(url, { ...init, signal: init.signal, cache: "no-store" });
  } catch (e) {
    const err = e as Error;
    if (err.name === "AbortError") {
      throw new LlmError(
        "The AI took too long to answer. If you're running a local model, it may be busy — try again, or pick a smaller model.",
        `timeout/abort: ${url}`
      );
    }
    throw new LlmError(
      "CIE couldn't reach the AI service. If it's on this computer, is it running? Check the address in the connection settings.",
      `fetch failed: ${err.message} → ${url}`
    );
  }
}

async function readErrorBody(res: Response): Promise<string> {
  try {
    const text = await res.text();
    return text.slice(0, 300);
  } catch {
    return "(no body)";
  }
}

// ---------- model listing / connection test ----------

export async function listOllamaModels(cfg: OllamaCfg): Promise<string[]> {
  const base = normalizeBase(cfg.baseUrl, "Ollama");
  const res = await fetchJson(`${base}/api/tags`, { signal: AbortSignal.timeout(8000) });
  if (!res.ok) {
    throw new LlmError(
      "Ollama answered, but not the way CIE expected. Is that really an Ollama server?",
      `GET ${base}/api/tags → ${res.status}: ${await readErrorBody(res)}`
    );
  }
  const data = (await res.json().catch(() => null)) as { models?: { name?: string; model?: string }[] } | null;
  const models = (data?.models ?? []).map((m) => m.name || m.model || "").filter(Boolean);
  return models;
}

export async function listOpenAiModels(cfg: OpenAICfg): Promise<string[]> {
  const base = normalizeBase(cfg.baseUrl, "AI service");
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (cfg.apiKey) headers.Authorization = `Bearer ${cfg.apiKey}`;
  const res = await fetchJson(`${base}/models`, { headers, signal: AbortSignal.timeout(12000) });
  if (!res.ok) {
    const body = await readErrorBody(res);
    if (res.status === 401 || res.status === 403) {
      throw new LlmError("The service rejected that key. Double-check the API key — and that it belongs to this service.", `GET ${base}/models → ${res.status}: ${body}`);
    }
    throw new LlmError(
      "The service answered, but CIE couldn't read its model list. You can still type the model name by hand below.",
      `GET ${base}/models → ${res.status}: ${body}`
    );
  }
  const data = (await res.json().catch(() => null)) as { data?: { id?: string }[] } | null;
  const models = (data?.data ?? []).map((m) => m.id || "").filter(Boolean);
  return models;
}

// ---------- chat completion ----------

export interface ChatTurn {
  role: "user" | "assistant";
  content: string;
}

/** Returns the assistant's raw text (expected to be the strict-JSON contract). */
export async function chatOllama(cfg: OllamaCfg, messages: ChatTurn[], timeoutMs: number): Promise<string> {
  const base = normalizeBase(cfg.baseUrl, "Ollama");
  if (!cfg.model) {
    throw new LlmError("No model was picked for Ollama yet. Open the connection settings and choose one.", "ollama.model empty");
  }
  const res = await fetchJson(`${base}/api/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ model: cfg.model, messages, stream: false }),
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!res.ok) {
    const body = await readErrorBody(res);
    if (res.status === 404) {
      throw new LlmError(
        `Ollama doesn't have “${cfg.model}” yet. Pull it with:  ollama pull ${cfg.model}  — or pick another model in the connection settings.`,
        `POST ${base}/api/chat → 404: ${body}`
      );
    }
    throw new LlmError("Ollama started but couldn't finish the answer. Try again — or try a smaller model.", `POST ${base}/api/chat → ${res.status}: ${body}`);
  }
  const data = (await res.json().catch(() => null)) as { message?: { content?: string } } | null;
  const text = data?.message?.content?.trim();
  if (!text) throw new LlmError("Ollama returned an empty answer. Please try again.", "empty message.content");
  return text;
}

export async function chatOpenAi(cfg: OpenAICfg, messages: ChatTurn[], timeoutMs: number): Promise<string> {
  const base = normalizeBase(cfg.baseUrl, "AI service");
  if (!cfg.model) {
    throw new LlmError("No model name was set. Open the connection settings and pick or type one.", "openai.model empty");
  }
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (cfg.apiKey) headers.Authorization = `Bearer ${cfg.apiKey}`;
  const res = await fetchJson(`${base}/chat/completions`, {
    method: "POST",
    headers,
    body: JSON.stringify({ model: cfg.model, messages, temperature: 0.2, stream: false }),
    signal: AbortSignal.timeout(timeoutMs),
  });
  if (!res.ok) {
    const body = await readErrorBody(res);
    if (res.status === 401 || res.status === 403) {
      throw new LlmError("The service rejected that key. Double-check the API key in the connection settings.", `POST ${base}/chat/completions → ${res.status}: ${body}`);
    }
    if (res.status === 404) {
      throw new LlmError(`The service doesn't know the model “${cfg.model}”. Check the spelling in the connection settings.`, `POST ${base}/chat/completions → 404: ${body}`);
    }
    if (res.status === 429) {
      throw new LlmError("The service says CIE is going too fast (rate limit or out of credit). Wait a moment and try again.", `POST ${base}/chat/completions → 429: ${body}`);
    }
    throw new LlmError("The AI service had trouble answering. Please try again in a moment.", `POST ${base}/chat/completions → ${res.status}: ${body}`);
  }
  const data = (await res.json().catch(() => null)) as { choices?: { message?: { content?: string } }[] } | null;
  const text = data?.choices?.[0]?.message?.content?.trim();
  if (!text) throw new LlmError("The service returned an empty answer. Please try again.", "empty choices[0].message.content");
  return text;
}

/** One small honest ping to prove the built-in credentials work right now. */
export async function pingBuiltin(): Promise<void> {
  try {
    const { default: ZAI } = await import("z-ai-web-dev-sdk");
    const zai = await ZAI.create();
    const completion = await zai.chat.completions.create({
      messages: [{ role: "user", content: "Reply with the single word OK." }],
      thinking: { type: "disabled" },
    });
    const text = completion.choices?.[0]?.message?.content?.trim();
    if (!text) throw new LlmError("The built-in AI answered with nothing. It may be having a bad day — try again.", "builtin ping: empty reply");
  } catch (e) {
    if (e instanceof LlmError) throw e;
    throw new LlmError(
      "The built-in AI isn't reachable right now. Switch to Ollama or your own service below — everything else keeps working.",
      `builtin ping failed: ${(e as Error).message}`
    );
  }
}

/** Basic shape-check for the llm config attached to API requests. */
export function sanitizeLlmCfg(raw: unknown): LlmCfg {
  const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  const oll = (o.ollama && typeof o.ollama === "object" ? o.ollama : {}) as Record<string, unknown>;
  const oa = (o.openai && typeof o.openai === "object" ? o.openai : {}) as Record<string, unknown>;
  const str = (v: unknown, max: number) => (typeof v === "string" ? v.slice(0, max) : "");
  return {
    provider: o.provider === "ollama" || o.provider === "openai" ? o.provider : "builtin",
    ollama: { baseUrl: str(oll.baseUrl, 300).trim(), model: str(oll.model, 120).trim() },
    openai: { baseUrl: str(oa.baseUrl, 300).trim(), apiKey: str(oa.apiKey, 400).trim(), model: str(oa.model, 120).trim() },
  };
}
