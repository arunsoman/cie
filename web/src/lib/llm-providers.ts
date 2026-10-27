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

// ---------- tool-calling turn (the store agent) ----------

export interface ToolCallRequest {
  id: string;
  name: string;
  /** arguments in canonical form: a JSON string across all providers */
  arguments: string;
}
export interface ToolChatResponse {
  content: string | null;
  toolCalls: ToolCallRequest[];
}

/** One turn of the tool loop: plain chat turns, an assistant turn that
 *  asks for tools, or a tool turn answering one call. */
export type LoopTurn =
  | ChatTurn
  | {
      role: "assistant";
      content: string | null;
      tool_calls?: { id: string; type: "function"; function: { name: string; arguments: string } }[];
    }
  | { role: "tool"; tool_call_id: string; content: string; name?: string };

export type ToolSpec = Record<string, unknown>;

const TOOL_BODY_TIMEOUT = 45_000; // per hop; the loop itself bounds the hops

function toolCallRequestOf(raw: unknown, index: number): ToolCallRequest | null {
  if (!raw || typeof raw !== "object") return null;
  const tc = raw as Record<string, unknown>;
  const fn = (tc.function && typeof tc.function === "object" ? tc.function : {}) as Record<string, unknown>;
  const name = typeof fn.name === "string" && fn.name ? fn.name : null;
  if (!name) return null;
  const id = typeof tc.id === "string" && tc.id ? tc.id : `call_${index}_${name}`;
  const args = typeof fn.arguments === "string" ? fn.arguments : JSON.stringify(fn.arguments ?? {});
  return { id, name, arguments: args };
}

async function openAiStyleToolCall(base: string, headers: Record<string, string>, body: Record<string, unknown>, what: string): Promise<ToolChatResponse> {
  const res = await fetchJson(`${base}/chat/completions`, {
    method: "POST",
    headers,
    body: JSON.stringify(body),
    signal: AbortSignal.timeout(TOOL_BODY_TIMEOUT),
  });
  if (!res.ok) {
    const errBody = await readErrorBody(res);
    if (res.status === 401 || res.status === 403) throw new LlmError(`The service rejected that key. Double-check the API key in the connection settings.`, `POST ${base}/chat/completions → ${res.status}: ${errBody}`);
    if (res.status === 404) throw new LlmError(`The service doesn't know the model. Check the spelling in the connection settings.`, `POST ${base}/chat/completions → ${res.status}: ${errBody}`);
    if (res.status === 429) throw new LlmError("The service says CIE is going too fast (rate limit or out of credit). Wait a moment and try again.", `POST ${base}/chat/completions → ${res.status}: ${errBody}`);
    throw new LlmError(`${what} had trouble answering. Please try again in a moment.`, `POST ${base}/chat/completions → ${res.status}: ${errBody}`);
  }
  const data = (await res.json().catch(() => null)) as { choices?: { message?: Record<string, unknown> }[] } | null;
  const message = data?.choices?.[0]?.message ?? null;
  const calls = Array.isArray(message?.tool_calls)
    ? (message!.tool_calls as unknown[]).map(toolCallRequestOf)
    : [];
  return {
    content: typeof message?.content === "string" ? message.content.trim() : null,
    toolCalls: calls.filter((c): c is ToolCallRequest => c !== null),
  };
}

/** One OpenAI-compatible turn with tools — may answer (content) or ask
 *  for tool calls. */
export async function chatOpenAiTools(cfg: OpenAICfg, turns: LoopTurn[], tools: ToolSpec[]): Promise<ToolChatResponse> {
  const base = normalizeBase(cfg.baseUrl, "AI service");
  if (!cfg.model) throw new LlmError("No model name was set. Open the connection settings and pick or type one.", "openai.model empty");
  const headers: Record<string, string> = { "Content-Type": "application/json" };
  if (cfg.apiKey) headers.Authorization = `Bearer ${cfg.apiKey}`;
  return openAiStyleToolCall(base, headers, { model: cfg.model, messages: turns, temperature: 0.2, stream: false, tools }, "The AI service");
}

/** One Ollama turn with tools. Ollama carries tool-call arguments as an
 *  OBJECT — normalized to the canonical JSON-string form here. */
export async function chatOllamaTools(cfg: OllamaCfg, turns: LoopTurn[], tools: ToolSpec[]): Promise<ToolChatResponse> {
  const base = normalizeBase(cfg.baseUrl, "Ollama");
  if (!cfg.model) throw new LlmError("No model was picked for Ollama yet. Open the connection settings and choose one.", "ollama.model empty");
  const res = await fetchJson(`${base}/api/chat`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ model: cfg.model, messages: turns, stream: false, tools }),
    signal: AbortSignal.timeout(TOOL_BODY_TIMEOUT),
  });
  if (!res.ok) {
    const errBody = await readErrorBody(res);
    if (res.status === 404) throw new LlmError(`Ollama doesn't have the model yet. Pull it, or pick another model in the connection settings.`, `POST ${base}/api/chat → 404: ${errBody}`);
    throw new LlmError("Ollama started but couldn't finish the answer. Try again — or try a smaller model.", `POST ${base}/api/chat (tools) → ${res.status}: ${errBody}`);
  }
  const data = (await res.json().catch(() => null)) as { message?: { content?: string; tool_calls?: unknown[] } } | null;
  const message = data?.message ?? null;
  const calls = Array.isArray(message?.tool_calls)
    ? (message!.tool_calls as unknown[]).map((raw, i) => {
        if (!raw || typeof raw !== "object") return null;
        const fn = ((raw as Record<string, unknown>).function ?? {}) as Record<string, unknown>;
        const name = typeof fn.name === "string" && fn.name ? fn.name : null;
        if (!name) return null;
        return toolCallRequestOf({ id: `call_${i}_${name}`, function: fn }, i);
      })
    : [];
  return {
    content: typeof message?.content === "string" ? message.content.trim() : null,
    toolCalls: calls.filter((c): c is ToolCallRequest => c !== null),
  };
}

/** One built-in (Z.ai) turn with tools. The SDK's body forwards extra
 *  keys ([key: string]: any), so the standard OpenAI `tools` field rides
 *  along; the response shape is the OpenAI one. */
export async function chatBuiltinTools(turns: LoopTurn[], tools: ToolSpec[]): Promise<ToolChatResponse> {
  try {
    const { default: ZAI } = await import("z-ai-web-dev-sdk");
    const zai = await ZAI.create();
    // The SDK's declared message type is narrow (content: string, no tool
    // turns) while the body accepts extra keys ([key: string]: any) — the
    // cast is the same narrow-bridge the route's env-config path uses.
    const body = { messages: turns, tools, thinking: { type: "disabled" } };
    const completion = (await Promise.race([
      zai.chat.completions.create(body as unknown as never),
      new Promise<never>((_, rej) => setTimeout(() => rej(new LlmError("The built-in AI took too long to answer. Please try again.", "builtin tools call: timeout")), TOOL_BODY_TIMEOUT)),
    ])) as { choices?: { message?: Record<string, unknown> }[] };
    const message = completion.choices?.[0]?.message ?? null;
    const calls = Array.isArray(message?.tool_calls)
      ? (message!.tool_calls as unknown[]).map((raw, i) => toolCallRequestOf({ id: `call_${i}`, function: (raw as Record<string, unknown>).function }, i))
      : [];
    return {
      content: typeof message?.content === "string" ? message.content.trim() : null,
      toolCalls: calls.filter((c): c is ToolCallRequest => c !== null),
    };
  } catch (e) {
    if (e instanceof LlmError) throw e;
    throw new LlmError("The built-in AI isn't reachable right now. It may not support tool calls — the plain digest answer still works.", `builtin tools call failed: ${(e as Error).message}`);
  }
}

/** Provider-dispatched tools turn. */
export async function chatWithTools(cfg: LlmCfg, turns: LoopTurn[], tools: ToolSpec[]): Promise<ToolChatResponse> {
  if (cfg.provider === "ollama") return chatOllamaTools(cfg.ollama, turns, tools);
  if (cfg.provider === "openai") return chatOpenAiTools(cfg.openai, turns, tools);
  return chatBuiltinTools(turns, tools);
}

/** The built-in (Z.ai) plain turn — the same env-over-config bridge the
 *  route always used, now a function the plain and fallback paths share. */
export async function chatBuiltin(turns: ChatTurn[], timeoutMs: number): Promise<string> {
  const { default: ZAI } = await import("z-ai-web-dev-sdk");
  type ZaiLike = {
    chat: { completions: { create: (body: unknown) => Promise<{ choices?: { message?: { content?: string } }[] }> } };
  };
  const envKey = process.env.ZAI_API_KEY || process.env.ZAI_APIKEY;
  const envBase = process.env.ZAI_BASE_URL || process.env.ZAI_BASEURL;
  const zai = envKey
    ? new (ZAI as unknown as new (cfg: { baseUrl: string; apiKey: string }) => ZaiLike)({
        baseUrl: envBase || "https://api.z.ai/api/paas/v4",
        apiKey: envKey,
      })
    : await (async () => {
        try {
          return (await ZAI.create()) as unknown as ZaiLike;
        } catch (err) {
          // The SDK's config failure is a raw Error that used to surface as
          // a generic 500 — name the real fix instead.
          throw new LlmError(
            "The built-in AI isn't configured on this server — add a .z-ai-config file (or set ZAI_API_KEY), or switch the answering model in the connection settings, then try again.",
            `builtin: SDK config unavailable: ${(err as Error).message}`,
          );
        }
      })();
  const completion = (await Promise.race([
    zai.chat.completions.create({ messages: turns, thinking: { type: "disabled" } }),
    new Promise<never>((_, rej) => setTimeout(() => rej(new LlmError("The built-in AI took too long to answer. Please try again.", "builtin call: timeout")), timeoutMs)),
  ])) as { choices?: { message?: { content?: string } }[] };
  const text = completion.choices?.[0]?.message?.content?.trim() ?? "";
  if (!text) throw new LlmError("The built-in AI answered with nothing. Please try again.", "builtin: empty reply");
  return text;
}

/** Provider-dispatched plain turn — the loop's no-tools end and the
 *  non-tool fallback share it. */
export async function chatPlain(cfg: LlmCfg, turns: ChatTurn[], timeoutMs: number): Promise<string> {
  if (cfg.provider === "ollama") return chatOllama(cfg.ollama, turns, timeoutMs);
  if (cfg.provider === "openai") return chatOpenAi(cfg.openai, turns, timeoutMs);
  return chatBuiltin(turns, timeoutMs);
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
