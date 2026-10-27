// The store agent — the model drives the store's own read-only tools
// (cie_describe + cie_retrieve, the MCP server's declared surface) and
// answers with the chart kind already picked. Tested hermetically: the
// storage bridge is a mock, the model is a script.

const { test } = require("node:test");
const assert = require("node:assert");
const path = require("node:path");
const loader = require("./helpers/load-ts.cjs");

const load = loader({
  "./storage-native": {
    storageCall: async (request) => {
      // the real bridge contract: {ok:true,...} comes back on success
      if (request.op === "describe") {
        return {
          revision: 7,
          context: "ctx_test_1",
          capabilities: { entities: ["symbol", "file"] },
          schemas: { request: { type: "object" }, response: { type: "object" } },
          example: { select: { kind: "symbol", where: [] }, outputs: [] },
          evidence_policy: "cite returned ids",
        };
      }
      if (request.op === "retrieve") {
        if (request.context !== "ctx_test_1") throw new (class extends Error {})("unknown context");
        return { results: [{ id: "e1", kind: "symbol", name: "send_sms" }] };
      }
      throw new Error("unsupported op " + request.op);
    },
    StorageFailure: class StorageFailure extends Error {
      constructor(code, message) { super(message); this.code = code; }
    },
  },
});

const ct = load(path.resolve(__dirname, "../src/lib/chat-tools.ts"));

// ---------- the tool defs ----------

test("the store tools are the MCP server's own two read-only tools", () => {
  const defs = ct.storeToolDefs();
  assert.deepEqual(defs.map((d) => d.function.name), ["cie_describe", "cie_retrieve"]);
  assert.ok(JSON.stringify(defs).includes("read-only"));
  // the model is told to call describe FIRST — the store's own schemas
  // teach the retrieve DSL, no hardcoding here
  assert.ok(JSON.stringify(defs[0].function.description).includes("FIRST"));
});

// ---------- the executor ----------
// the digest the route receives: chart_possibilities built from the loaded
// datasets (chat-digest.ts) — what cie_charts serves
const DIGEST = {
  chart_possibilities: [
    { kind: "uml", shows: "structure", grounded_in: "the report", data_from: "store" },
    { kind: "sequence", shows: "one call step by step", grounded_in: "call records", data_from: "store" },
    { kind: "hbar", shows: "bars", grounded_in: "findings", data_from: "model" },
  ],
};

test("the agent toolset is the store's two read-only tools PLUS cie_charts", () => {
  const defs = ct.agentToolDefs();
  assert.deepEqual(defs.map((d) => d.function.name), ["cie_describe", "cie_retrieve", "cie_charts"]);
  assert.ok(JSON.stringify(defs[2].function.description).includes("possible_charts"));
});

test("cie_charts names which charts are possible, and opens the pinned context too", async () => {
  const r = await ct.executeStoreTool("/repo", DIGEST, "cie_charts", {});
  const parsed = JSON.parse(r.content);
  assert.deepEqual(parsed.possible_charts, DIGEST.chart_possibilities, "the list is the digest's own possibilities, passed verbatim");
  assert.equal(parsed.store.context, "ctx_test_1", "the pinned context comes along — the model can go retrieve next");
  assert.ok(parsed.rule.includes("possible_charts"), "the pick is constrained to the possible list");
  assert.ok(parsed.rule.includes("data_from"), "store-filled kinds vs model-supplied counts are told apart");
});

test("cie_charts without a store says the digest is the only data", async () => {
  const r = await ct.executeStoreTool(null, DIGEST, "cie_charts", {});
  const parsed = JSON.parse(r.content);
  assert.deepEqual(parsed.possible_charts, DIGEST.chart_possibilities);
  assert.ok(parsed.store.note.includes("no repository store is connected"));
});

test("describe/retrieve without a store return the teaching error, never a crash", async () => {
  const d = await ct.executeStoreTool(null, DIGEST, "cie_describe", {});
  const r = await ct.executeStoreTool(null, DIGEST, "cie_retrieve", {});
  for (const x of [d, r]) {
    const parsed = JSON.parse(x.content);
    assert.ok(parsed.error.includes("no repository store is connected"));
    assert.ok(parsed.error.includes("could not find"), "the honesty wording is taught to the model");
  }
});

test("describe opens the pinned context through the store bridge", async () => {
  const r = await ct.executeStoreTool("/repo", DIGEST, "cie_describe", { profile: "Compact" });
  assert.ok(r.ok);
  const parsed = JSON.parse(r.content);
  assert.equal(parsed.context, "ctx_test_1");
  assert.ok(parsed.schemas, "the describe result carries the store's self-description");
});

test("retrieve is refused (as a tool result, not a crash) without a context", async () => {
  const r = await ct.executeStoreTool("/repo", DIGEST, "cie_retrieve", {});
  const parsed = JSON.parse(r.content);
  assert.ok(parsed.error.includes("cie_describe"));
});

test("retrieve threads the model's request to the store, context stripped", async () => {
  const r = await ct.executeStoreTool("/repo", DIGEST, "cie_retrieve", {
    context: "ctx_test_1",
    select: { kind: "symbol", where: [] },
    outputs: [{ id: "symbols" }],
  });
  const parsed = JSON.parse(r.content);
  assert.ok(Array.isArray(parsed.results), "the store answered with rows");
  assert.equal(parsed.results[0].name, "send_sms");
});

test("an unknown tool and a storage refusal both come back AS tool results", async () => {
  const bad = await ct.executeStoreTool("/repo", DIGEST, "cie_write", {});
  assert.ok(!bad.ok && JSON.parse(bad.content).error.includes("unknown tool"));
  const refuse = await ct.executeStoreTool("/repo", DIGEST, "cie_retrieve", { context: "ctx_other" });
  assert.ok(!refuse.ok, "the storage refusal is returned, never thrown");
});

// ---------- the loop ----------

function scriptedModel(steps) {
  // steps: [{toolCalls?: [...], content?}] — consumed in order
  let i = 0;
  const callsSeen = [];
  return {
    chatWithTools: async (turns) => {
      callsSeen.push([...turns]);
      const step = steps[Math.min(i, steps.length - 1)];
      i++;
      // after the scripted steps run out, answer plainly
      return step.toolCalls
        ? { content: step.content ?? null, toolCalls: step.toolCalls }
        : { content: step.content ?? '{"answer":"done","chart":null}', toolCalls: [] };
    },
    callsSeen,
    hopCount: () => i,
  };
}

test("a two-hop tool call loops and lands on the one-JSON answer", async () => {
  const model = scriptedModel([
    { toolCalls: [{ id: "c1", name: "cie_describe", arguments: "{}" }] },
    { toolCalls: [{ id: "c2", name: "cie_retrieve", arguments: '{"context":"ctx_test_1"}' }] },
    { content: '{"answer":"send_sms is called by handle_sms","chart":{"kind":"sequence"}}' },
  ]);
  const r = await ct.runStoreAgentTurn(
    model.chatWithTools,
    [{ role: "user", content: "q" }],
    (name, args) => ct.executeStoreTool("/repo", DIGEST, name, args),
  );
  assert.equal(r.hops, 2);
  assert.deepEqual(r.toolsUsed, ["cie_describe", "cie_retrieve"]);

  // the LAST modeled call received: system+user, assistant(tool_calls),
  // tool(describe result), assistant(tool_calls), tool(retrieve result)
  const final = model.callsSeen[model.callsSeen.length - 1];
  assert.equal(final[final.length - 1].role, "tool");
  assert.equal(final[final.length - 2].role, "assistant");
  assert.equal(final[final.length - 2].tool_calls.length, 1);
  const toolResults = final.filter((t) => t.role === "tool");
  assert.ok(toolResults[0].content.includes("ctx_test_1"), "the describe result the model read carries the pinned context");
  assert.ok(toolResults[1].content.includes("send_sms"), "the retrieve result the model read carries the store's rows");
});

test("malformed tool arguments are fed back as a readable tool result", async () => {
  const model = scriptedModel([
    { toolCalls: [{ id: "c1", name: "cie_retrieve", arguments: "{not json" }] },
    { content: '{"answer":"ok","chart":null}' },
  ]);
  const r = await ct.runStoreAgentTurn(
    model.chatWithTools,
    [{ role: "user", content: "q" }],
    (name, args) => ct.executeStoreTool("/repo", DIGEST, name, args),
  );
  assert.equal(r.hops, 1);
  const final = model.callsSeen[model.callsSeen.length - 1];
  const toolTurn = final.filter((t) => t.role === "tool")[0];
  assert.ok(toolTurn.content.includes("not valid JSON"));
});

test("the hop budget ends the loop with a forced no-tools answer", async () => {
  let turnsSeen;
  const model = {
    chatWithTools: async (turns) => {
      turnsSeen = turns;
      return { content: null, toolCalls: [{ id: `c${turns.length}`, name: "cie_describe", arguments: "{}" }] };
    },
  };
  // the no-tools closer is a SEPARATE call — capture what it received
  let plainTurns;
  const chatPlain = async (turns) => {
    plainTurns = turns;
    return '{"answer":"budget answer","chart":null}';
  };
  const r = await ct.runStoreAgentTurn(
    model.chatWithTools,
    [{ role: "user", content: "q" }],
    (name, args) => ct.executeStoreTool("/repo", DIGEST, name, args),
    { maxHops: 2, chatPlain },
  );
  assert.equal(r.hops, 2);
  assert.ok(plainTurns[plainTurns.length - 1].content.includes("budget"), "the forced answer knows the budget is spent");
  assert.ok(plainTurns[plainTurns.length - 1].content.includes("one-JSON"), "the forced answer keeps the answer contract");
});

test("an oversized tool result is refused with the narrowing fix, never truncated", async () => {
  // a storage bridge that answers with a huge payload:
  const loader2 = loader({
    "./storage-native": {
      storageCall: async () => ({ blob: "x".repeat(30_000) }),
      StorageFailure: class extends Error {},
    },
  });
  const ct2 = loader2(path.resolve(__dirname, "../src/lib/chat-tools.ts"));
  const r = await ct2.executeStoreTool("/repo", null, "cie_describe", {});
  const parsed = JSON.parse(r.content);
  assert.ok(parsed.error.includes("read budget"), "the budget is named, counted");
  assert.ok(parsed.how_to_fix.includes("Narrow"), "the honest fix is the narrowing move");
  assert.ok(!parsed.blob, "nothing oversized leaks through half-cut");
});