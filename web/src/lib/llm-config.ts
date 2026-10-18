// LLM connection settings for "Ask CIE" — who answers your questions.
//
// Three providers:
//   builtin — the z-ai-web-dev-sdk that ships with this app (no setup)
//   ollama  — a local or hosted Ollama server (native /api tags + /api/chat)
//   openai  — any OpenAI-compatible service (OpenAI, OpenRouter, Groq,
//             Gemini's OpenAI layer, LM Studio, vLLM, llama.cpp server…)
//
// Config lives ONLY in the browser (localStorage). The API key — if any — is
// sent per-request to our own /api endpoints and is never stored or logged
// server-side.

export type LlmProvider = "builtin" | "ollama" | "openai";

export interface LlmConfig {
  provider: LlmProvider;
  ollama: { baseUrl: string; model: string };
  openai: { preset: string; baseUrl: string; apiKey: string; model: string };
}

export const LLM_STORAGE_KEY = "cie.llm.v1";

export const DEFAULT_LLM: LlmConfig = {
  provider: "builtin",
  ollama: { baseUrl: "http://localhost:11434", model: "" },
  openai: { preset: "openai", baseUrl: "https://api.openai.com/v1", apiKey: "", model: "" },
};

// Ready-made destinations for the "any other AI service" option. All speak
// the OpenAI chat-completions protocol.
export const OPENAI_PRESETS: { id: string; label: string; baseUrl: string; needsKey: boolean; hint: string }[] = [
  { id: "openai", label: "OpenAI", baseUrl: "https://api.openai.com/v1", needsKey: true, hint: "gpt-4o-mini and friends — needs an API key from platform.openai.com" },
  { id: "openrouter", label: "OpenRouter", baseUrl: "https://openrouter.ai/api/v1", needsKey: true, hint: "one key, hundreds of models (including free ones)" },
  { id: "groq", label: "Groq", baseUrl: "https://api.groq.com/openai/v1", needsKey: true, hint: "very fast hosted models — free tier available" },
  { id: "gemini", label: "Google Gemini", baseUrl: "https://generativelanguage.googleapis.com/v1beta/openai", needsKey: true, hint: "Google's models via their OpenAI-compatible layer" },
  { id: "lmstudio", label: "LM Studio (this computer)", baseUrl: "http://localhost:1234/v1", needsKey: false, hint: "if you run LM Studio, point CIE at it here" },
  { id: "custom", label: "Something else", baseUrl: "", needsKey: true, hint: "vLLM, llama.cpp server, Together, Mistral… anything that speaks the OpenAI protocol" },
];

export function presetFor(id: string) {
  return OPENAI_PRESETS.find((p) => p.id === id) ?? OPENAI_PRESETS[OPENAI_PRESETS.length - 1];
}

/** Merge saved settings over the defaults, tolerating partial/corrupt data. */
export function loadLlmConfig(): LlmConfig {
  if (typeof window === "undefined") return DEFAULT_LLM;
  try {
    const raw = window.localStorage.getItem(LLM_STORAGE_KEY);
    if (!raw) return DEFAULT_LLM;
    const saved = JSON.parse(raw) as Partial<LlmConfig>;
    const cfg: LlmConfig = {
      provider: saved.provider === "ollama" || saved.provider === "openai" ? saved.provider : "builtin",
      ollama: {
        baseUrl: typeof saved.ollama?.baseUrl === "string" && saved.ollama.baseUrl.trim() ? saved.ollama.baseUrl.trim() : DEFAULT_LLM.ollama.baseUrl,
        model: typeof saved.ollama?.model === "string" ? saved.ollama.model : "",
      },
      openai: {
        preset: typeof saved.openai?.preset === "string" ? saved.openai.preset : "openai",
        baseUrl: typeof saved.openai?.baseUrl === "string" ? saved.openai.baseUrl : DEFAULT_LLM.openai.baseUrl,
        apiKey: typeof saved.openai?.apiKey === "string" ? saved.openai.apiKey : "",
        model: typeof saved.openai?.model === "string" ? saved.openai.model : "",
      },
    };
    return cfg;
  } catch {
    return DEFAULT_LLM;
  }
}

export function saveLlmConfig(cfg: LlmConfig): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(LLM_STORAGE_KEY, JSON.stringify(cfg));
  } catch {
    /* storage full or blocked — settings just won't persist */
  }
}

/** What a provider is called in the status bar, in plain words. */
export function providerLabel(cfg: LlmConfig): string {
  switch (cfg.provider) {
    case "ollama":
      return cfg.ollama.model ? `Ollama · ${cfg.ollama.model}` : "Ollama";
    case "openai":
      return cfg.openai.model ? `${presetFor(cfg.openai.preset).label} · ${cfg.openai.model}` : presetFor(cfg.openai.preset).label;
    default:
      return "Built-in AI";
  }
}

/** Shrink the config for a POST body (trims whitespace; keeps the key —
 *  the chat and check endpoints are ours and run on this machine). */
export function llmConfigForRequest(cfg: LlmConfig) {
  return {
    provider: cfg.provider,
    ollama: {
      baseUrl: cfg.ollama.baseUrl.trim(),
      model: cfg.ollama.model.trim(),
    },
    openai: {
      baseUrl: cfg.openai.baseUrl.trim(),
      apiKey: cfg.openai.apiKey.trim(),
      model: cfg.openai.model.trim(),
    },
  };
}

export type LlmConfigForRequest = ReturnType<typeof llmConfigForRequest>;
