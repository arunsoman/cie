"use client";

// "Who answers when you ask?" — the LLM connection dialog.
//
// Three choices, in plain words:
//   Built-in AI  — ships with the app, no setup.
//   Ollama       — your own local or hosted Ollama server.
//   Other service — anything that speaks the OpenAI protocol (OpenAI,
//                  OpenRouter, Groq, Gemini, LM Studio, vLLM…).
//
// Every option has an honest "Check connection" button that lists the models
// the service really offers. Settings live in the browser only.

import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  CheckCircle2,
  Cpu,
  KeyRound,
  Loader2,
  RefreshCw,
  ShieldCheck,
  TriangleAlert,
  XCircle,
} from "lucide-react";
import {
  DEFAULT_LLM,
  OPENAI_PRESETS,
  presetFor,
  type LlmConfig,
} from "@/lib/llm-config";

type CheckState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "ok"; models: string[]; note?: string }
  | { kind: "fail"; error: string };

const PROVIDER_CARDS: {
  id: LlmConfig["provider"];
  icon: typeof Cpu;
  title: string;
  blurb: string;
}[] = [
  {
    id: "builtin",
    icon: ShieldCheck,
    title: "Built-in AI",
    blurb: "No setup — works out of the box in this workspace. If it's ever down, connect your own below.",
  },
  {
    id: "ollama",
    icon: Cpu,
    title: "Ollama",
    blurb: "Your own models on this computer or a server you run. Nothing leaves your machine.",
  },
  {
    id: "openai",
    icon: KeyRound,
    title: "Another AI service",
    blurb: "OpenAI, OpenRouter, Groq, Gemini, LM Studio… anything that speaks the OpenAI protocol.",
  },
];

export function LlmSettingsDialog({
  open,
  onOpenChange,
  config,
  onSave,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  config: LlmConfig;
  onSave: (cfg: LlmConfig) => void;
}) {
  // The body is keyed by `open`, so every open starts with a fresh draft
  // and a clean check state — no setState-in-effect needed.
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open && (
        <DialogContent className="sm:max-w-xl max-h-[85vh] overflow-y-auto">
          <SettingsBody config={config} onSave={onSave} onClose={() => onOpenChange(false)} />
        </DialogContent>
      )}
    </Dialog>
  );
}

function SettingsBody({
  config,
  onSave,
  onClose,
}: {
  config: LlmConfig;
  onSave: (cfg: LlmConfig) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState<LlmConfig>(config);
  const [check, setCheck] = useState<CheckState>({ kind: "idle" });
  const [showKey, setShowKey] = useState(false);

  const runCheck = async () => {
    setCheck({ kind: "checking" });
    try {
      const res = await fetch("/api/llm/check", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          llm: {
            provider: draft.provider,
            ollama: { baseUrl: draft.ollama.baseUrl.trim(), model: draft.ollama.model.trim() },
            openai: {
              baseUrl: draft.openai.baseUrl.trim(),
              apiKey: draft.openai.apiKey.trim(),
              model: draft.openai.model.trim(),
            },
          },
        }),
      });
      const data = (await res.json().catch(() => ({}))) as {
        ok?: boolean;
        models?: string[];
        note?: string;
        error?: string;
      };
      if (data.ok) {
        setCheck({ kind: "ok", models: data.models ?? [], note: data.note });
      } else {
        setCheck({ kind: "fail", error: data.error ?? "The check failed. Please try again." });
      }
    } catch {
      setCheck({ kind: "fail", error: "CIE couldn't run the check. Is the connection still up?" });
    }
  };

  const save = () => {
    onSave({
      provider: draft.provider,
      ollama: { baseUrl: draft.ollama.baseUrl.trim(), model: draft.ollama.model.trim() },
      openai: {
        preset: draft.openai.preset,
        baseUrl: draft.openai.baseUrl.trim(),
        apiKey: draft.openai.apiKey.trim(),
        model: draft.openai.model.trim(),
      },
    });
    onClose();
  };

  const models =
    check.kind === "ok" ? check.models : [];

  return (
    <>
      <DialogHeader>
        <DialogTitle>Who answers when you ask?</DialogTitle>
        <DialogDescription>
          Pick the brain behind Ask CIE. Your choice applies to every question — you can change it any time.
        </DialogDescription>
      </DialogHeader>

        <div className="space-y-2.5">
          {PROVIDER_CARDS.map((p) => {
            const Icon = p.icon;
            const active = draft.provider === p.id;
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => setDraft({ ...draft, provider: p.id })}
                aria-pressed={active}
                className={`w-full text-left rounded-xl border px-4 py-3 transition-colors ${
                  active ? "border-emerald-300 bg-emerald-50" : "border-stone-200 bg-white hover:border-emerald-200"
                }`}
              >
                <span className="flex items-start gap-3">
                  <Icon className={`h-5 w-5 mt-0.5 shrink-0 ${active ? "text-emerald-700" : "text-stone-400"}`} />
                  <span>
                    <span className={`block text-sm font-semibold ${active ? "text-emerald-900" : "text-stone-900"}`}>{p.title}</span>
                    <span className="block text-xs text-stone-500 mt-0.5 leading-relaxed">{p.blurb}</span>
                  </span>
                </span>
              </button>
            );
          })}
        </div>

        {/* ---------- per-provider settings ---------- */}
        {draft.provider === "ollama" && (
          <div className="space-y-3 rounded-xl border border-stone-200 bg-stone-50/60 p-4">
            <div>
              <label htmlFor="ollama-url" className="block text-xs font-semibold text-stone-600 mb-1">
                Where is Ollama?
              </label>
              <Input
                id="ollama-url"
                value={draft.ollama.baseUrl}
                onChange={(e) => setDraft({ ...draft, ollama: { ...draft.ollama, baseUrl: e.target.value } })}
                placeholder="http://localhost:11434"
                className="bg-white font-mono text-xs"
                autoComplete="off"
                spellCheck={false}
              />
              <p className="mt-1 text-[11px] text-stone-400">
                The usual address on this computer is <span className="font-mono">http://localhost:11434</span>.
              </p>
            </div>
            <div>
              <label htmlFor="ollama-model" className="block text-xs font-semibold text-stone-600 mb-1">
                Which model?
              </label>
              <Input
                id="ollama-model"
                value={draft.ollama.model}
                onChange={(e) => setDraft({ ...draft, ollama: { ...draft.ollama, model: e.target.value } })}
                placeholder="e.g. llama3.2 — or check below and pick from the list"
                className="bg-white font-mono text-xs"
                autoComplete="off"
                spellCheck={false}
              />
            </div>
          </div>
        )}

        {draft.provider === "openai" && (
          <div className="space-y-3 rounded-xl border border-stone-200 bg-stone-50/60 p-4">
            <div>
              <label className="block text-xs font-semibold text-stone-600 mb-1">Which service?</label>
              <div className="flex flex-wrap gap-1.5">
                {OPENAI_PRESETS.map((p) => (
                  <button
                    key={p.id}
                    type="button"
                    onClick={() =>
                      setDraft({
                        ...draft,
                        openai: { ...draft.openai, preset: p.id, baseUrl: p.baseUrl || draft.openai.baseUrl },
                      })
                    }
                    aria-pressed={draft.openai.preset === p.id}
                    className={`rounded-full border px-3 py-1.5 text-xs transition-colors ${
                      draft.openai.preset === p.id
                        ? "border-emerald-300 bg-emerald-50 text-emerald-900 font-semibold"
                        : "border-stone-200 bg-white text-stone-600 hover:border-emerald-200"
                    }`}
                  >
                    {p.label}
                  </button>
                ))}
              </div>
              <p className="mt-1.5 text-[11px] text-stone-400">{presetFor(draft.openai.preset).hint}</p>
            </div>
            <div>
              <label htmlFor="openai-url" className="block text-xs font-semibold text-stone-600 mb-1">
                Service address
              </label>
              <Input
                id="openai-url"
                value={draft.openai.baseUrl}
                onChange={(e) => setDraft({ ...draft, openai: { ...draft.openai, baseUrl: e.target.value } })}
                placeholder="https://api.openai.com/v1"
                className="bg-white font-mono text-xs"
                autoComplete="off"
                spellCheck={false}
              />
            </div>
            <div>
              <label htmlFor="openai-key" className="block text-xs font-semibold text-stone-600 mb-1">
                API key <span className="font-normal text-stone-400">(if the service needs one)</span>
              </label>
              <div className="flex gap-2">
                <Input
                  id="openai-key"
                  type={showKey ? "text" : "password"}
                  value={draft.openai.apiKey}
                  onChange={(e) => setDraft({ ...draft, openai: { ...draft.openai, apiKey: e.target.value } })}
                  placeholder="sk-…"
                  className="bg-white font-mono text-xs"
                  autoComplete="off"
                  spellCheck={false}
                />
                <Button type="button" variant="outline" size="sm" className="h-9 shrink-0" onClick={() => setShowKey(!showKey)}>
                  {showKey ? "Hide" : "Show"}
                </Button>
              </div>
              <p className="mt-1 text-[11px] text-stone-400">
                The key stays in this browser only — it is sent just to ask your chosen service for answers.
              </p>
            </div>
            <div>
              <label htmlFor="openai-model" className="block text-xs font-semibold text-stone-600 mb-1">
                Which model?
              </label>
              <Input
                id="openai-model"
                value={draft.openai.model}
                onChange={(e) => setDraft({ ...draft, openai: { ...draft.openai, model: e.target.value } })}
                placeholder="e.g. gpt-4o-mini — or check below and pick from the list"
                className="bg-white font-mono text-xs"
                autoComplete="off"
                spellCheck={false}
              />
            </div>
          </div>
        )}

        {/* ---------- check connection ---------- */}
        {draft.provider !== "builtin" && (
          <div className="flex items-center gap-3">
            <Button type="button" variant="outline" size="sm" onClick={() => void runCheck()} disabled={check.kind === "checking"}>
              {check.kind === "checking" ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
              Check connection
            </Button>
            <p className="text-xs text-stone-400">Looks for the service and lists the models it really has.</p>
          </div>
        )}

        {check.kind === "checking" && (
          <p className="text-xs text-stone-500 flex items-center gap-2">
            <Loader2 className="h-3.5 w-3.5 animate-spin" /> Knocking on the door…
          </p>
        )}
        {check.kind === "ok" && (
          <div className="rounded-lg border border-emerald-200 bg-emerald-50 px-3.5 py-3">
            <p className="text-xs font-semibold text-emerald-900 flex items-center gap-1.5">
              <CheckCircle2 className="h-3.5 w-3.5" /> Connected.
              {draft.provider === "builtin"
                ? " The built-in AI is answering."
                : check.models.length
                  ? ` ${check.models.length} model${check.models.length === 1 ? "" : "s"} found — pick one:`
                  : " Connected, but no model list was offered — type the model name above."}
            </p>
            {models.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1.5 max-h-36 overflow-y-auto">
                {models.slice(0, 40).map((m) => {
                  const chosen = draft.provider === "ollama" ? draft.ollama.model === m : draft.openai.model === m;
                  return (
                    <button
                      key={m}
                      type="button"
                      onClick={() =>
                        draft.provider === "ollama"
                          ? setDraft({ ...draft, ollama: { ...draft.ollama, model: m } })
                          : setDraft({ ...draft, openai: { ...draft.openai, model: m } })
                      }
                      className={`rounded-full border px-2.5 py-1 font-mono text-[11px] transition-colors ${
                        chosen
                          ? "border-emerald-500 bg-emerald-600 text-white font-semibold"
                          : "border-stone-200 bg-white text-stone-600 hover:border-emerald-300"
                      }`}
                    >
                      {m}
                    </button>
                  );
                })}
              </div>
            )}
            {check.note && <p className="mt-1.5 text-[11px] text-emerald-800/70">{check.note}</p>}
          </div>
        )}
        {check.kind === "fail" && (
          <div className="rounded-lg border border-rose-200 bg-rose-50 px-3.5 py-3 flex items-start gap-2">
            <XCircle className="h-4 w-4 text-rose-600 shrink-0 mt-0.5" />
            <p className="text-xs text-rose-900 leading-relaxed">{check.error}</p>
          </div>
        )}

        <div className="flex items-center justify-between gap-3 pt-1">
          <p className="text-[11px] text-stone-400 leading-relaxed">
            {draft.provider === "builtin"
              ? "Nothing to configure — the built-in AI is checked when you send a question."
              : "Settings are saved in this browser only."}
          </p>
          <div className="flex items-center gap-2 shrink-0">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => {
                setDraft({ ...DEFAULT_LLM, openai: { ...DEFAULT_LLM.openai } });
                setCheck({ kind: "idle" });
              }}
            >
              Reset
            </Button>
            <Button
              type="button"
              size="sm"
              className="bg-emerald-700 hover:bg-emerald-800 text-white"
              onClick={save}
              disabled={
                (draft.provider === "ollama" && !draft.ollama.model.trim()) ||
                (draft.provider === "openai" && !draft.openai.model.trim())
              }
            >
              Use this
            </Button>
          </div>
        </div>
    </>
  );
}
