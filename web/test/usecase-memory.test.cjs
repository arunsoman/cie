// The Usecases section's last story pick, per workspace (D3): a preference
// riding workspace memory, never a fact. The module fails soft everywhere —
// no localStorage, corrupted JSON, malformed entries, an empty workspace
// key — and every failure degrades to "no remembered pick", letting the
// section's deterministic default stand.
const test = require("node:test");
const assert = require("node:assert");
const loader = require("./helpers/load-ts.cjs");

function fakeStorage() {
  const m = new Map();
  return {
    getItem: (k) => (m.has(k) ? m.get(k) : null),
    setItem: (k, v) => m.set(k, String(v)),
    removeItem: (k) => m.delete(k),
  };
}

function withStorage(fn) {
  const storage = fakeStorage();
  global.window = { localStorage: storage };
  const mem = loader()("src/lib/usecase-memory.ts");
  fn(mem, storage);
}

test("the pick is remembered per workspace, and workspaces stay separate", () => {
  withStorage((mem) => {
    mem.rememberUsecasePick("demo:polyglot", 7);
    mem.rememberUsecasePick("project:acme", 3);
    assert.equal(mem.readUsecasePick("demo:polyglot"), 7);
    assert.equal(mem.readUsecasePick("project:acme"), 3);
    assert.equal(mem.readUsecasePick("demo:banking"), null, "a workspace never picked has no pick");
  });
});

test("overwriting a pick takes the newest value", () => {
  withStorage((mem) => {
    mem.rememberUsecasePick("demo:polyglot", 7);
    mem.rememberUsecasePick("demo:polyglot", 12);
    assert.equal(mem.readUsecasePick("demo:polyglot"), 12);
  });
});

test("a corrupted record is ignored, never thrown", () => {
  withStorage((mem, storage) => {
    storage.setItem("cie.usecase.last", "{not json at all");
    assert.equal(mem.readUsecasePick("demo:polyglot"), null);
    // and a remember after corruption heals the record
    mem.rememberUsecasePick("demo:polyglot", 5);
    assert.equal(mem.readUsecasePick("demo:polyglot"), 5);
  });
});

test("malformed entries are filtered, not trusted", () => {
  withStorage((mem, storage) => {
    storage.setItem(
      "cie.usecase.last",
      JSON.stringify({ "demo:x": "not-a-number", "": 4, "demo:y": 1.5, "demo:z": Number.NaN }),
    );
    assert.equal(mem.readUsecasePick("demo:x"), null);
    assert.equal(mem.readUsecasePick(""), null, "an empty key is never a workspace");
    assert.equal(mem.readUsecasePick("demo:y"), 1.5, "a finite number is a pick");
    assert.equal(mem.readUsecasePick("demo:z"), null, "NaN is not a pick");
  });
});

test("a non-object record (array, string, number) is ignored", () => {
  withStorage((mem, storage) => {
    for (const bad of ["[1,2]", '"pick"', "42", "null"]) {
      storage.setItem("cie.usecase.last", bad);
      assert.equal(mem.readUsecasePick("demo:polyglot"), null, `record ${bad} is not a map`);
    }
  });
});

test("an empty workspace key never writes or reads", () => {
  withStorage((mem, storage) => {
    mem.rememberUsecasePick("", 9);
    assert.equal(storage.getItem("cie.usecase.last"), null, "nothing was written");
    assert.equal(mem.readUsecasePick(""), null);
  });
});

test("missing localStorage (private mode) fails soft", () => {
  global.window = {}; // no localStorage at all
  const mem = loader()("src/lib/usecase-memory.ts");
  assert.doesNotThrow(() => mem.rememberUsecasePick("demo:polyglot", 7));
  assert.equal(mem.readUsecasePick("demo:polyglot"), null);
});
