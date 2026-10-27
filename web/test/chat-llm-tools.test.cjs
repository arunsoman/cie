// The tools-capable provider callers: argument normalization across the
// OpenAI-compatible, Ollama and builtin surfaces, verified over stubbed
// fetch — no network, no SDK.

const { test } = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader();
const providers = load(path.resolve(__dirname, "../src/lib/llm-providers.ts"));

function withFetch(stub) {
  const real = globalThis.fetch;
  globalThis.fetch = stub;
  return () => { globalThis.fetch = real; };
}

test("chatOpenAiTools normalizes the OpenAI tool_calls shape", async () => {
  const restore = withFetch(async () => new Response(JSON.stringify({
    choices: [{ message: {
      content: "thinking aloud",
      tool_calls: [
        { id: "call_a", function: { name: "cie_describe", arguments: "{\"profile\":\"Compact\"}" } },
        { id: "call_b", function: { name: "cie_retrieve", arguments: { context: "ctx1" } } }, // args as OBJECT still normalizes
        { garbage: true }, // dropped
      ],
    }}],
  }), { status: 200, headers: { "Content-Type": "application/json" } }));
  try {
    const r = await providers.chatOpenAiTools(
      { baseUrl: "http://llm.test/v1", apiKey: "k", model: "m" },
      [{ role: "user", content: "q" }],
      [{ type: "function", function: { name: "cie_describe" } }],
    );
    assert.equal(r.content, "thinking aloud");
    assert.equal(r.toolCalls.length, 2);
    assert.equal(r.toolCalls[0].id, "call_a");
    assert.equal(r.toolCalls[0].name, "cie_describe");
    assert.equal(r.toolCalls[1].arguments, '{"context":"ctx1"}', "object arguments normalize to a JSON string");
  } finally { restore(); }
});

test("chatOllamaTools normalizes Ollama's object-argument calls and synthesizes ids", async () => {
  const restore = withFetch(async () => new Response(JSON.stringify({
    message: {
      content: "",
      tool_calls: [{ function: { name: "cie_describe", arguments: { profile: "Full" } } }],
    },
  }), { status: 200, headers: { "Content-Type": "application/json" } }));
  try {
    const r = await providers.chatOllamaTools(
      { baseUrl: "http://ollama.test", model: "local" },
      [{ role: "user", content: "q" }],
      [{ type: "function", function: { name: "cie_describe" } }],
    );
    assert.equal(r.toolCalls.length, 1);
    assert.ok(r.toolCalls[0].id.startsWith("call_"), "a synthesized id exists — the loop's tool turn needs one");
    assert.equal(r.toolCalls[0].name, "cie_describe");
    assert.equal(r.toolCalls[0].arguments, '{"profile":"Full"}');
  } finally { restore(); }
});

test("a tools call that errors maps to the plain-words failure (route falls back)", async () => {
  const restore = withFetch(async () => new Response('{"error":"nope"}', { status: 400 }));
  try {
    await assert.rejects(
      () => providers.chatOpenAiTools({ baseUrl: "http://llm.test/v1", model: "m" }, [], []),
      (e) => e instanceof providers.LlmError,
    );
  } finally { restore(); }
});