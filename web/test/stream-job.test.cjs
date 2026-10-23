const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const path = require("node:path");
const os = require("node:os");
const childProcess = require("node:child_process");

test("a local web ingestion launches one coordinator and consumes its artifacts", async () => {
  const bin = path.resolve(__dirname, "../../target/debug/cie");
  try { await fs.access(bin); } catch { assert.fail("Build the Rust CLI before running the ingestion integration test"); }
  const temp = await fs.mkdtemp(path.join(os.tmpdir(), "cie-web-stream-"));
  const repo = path.join(temp, "repo");
  await fs.mkdir(repo);
  await fs.writeFile(path.join(repo, "app.py"), "def save(x):\n    return x + 1\n");
  const previous = { bin: process.env.CIE_BIN, jobs: process.env.CIE_JOBS_ROOT };
  process.env.CIE_BIN = bin;
  process.env.CIE_JOBS_ROOT = path.join(temp, "jobs");
  const spawns = [];
  const load = require("./helpers/load-ts.cjs")({
    child_process: { ...childProcess, spawn(command, args, options) {
      spawns.push({ command, args });
      return childProcess.spawn(command, args, options);
    } },
  });
  try {
    const jobs = load(path.resolve(__dirname, "../src/lib/analyze-jobs.ts"));
    const job = await jobs.startJob({ source: "local", localPath: repo, repoName: "repo" });
    const deadline = Date.now() + 30000;
    while (!["done", "failed"].includes(job.state) && Date.now() < deadline) {
      await new Promise(resolve => setTimeout(resolve, 25));
    }
    assert.equal(job.state, "done", JSON.stringify(job.error));
    assert.ok(job.result.report.symbols.length > 0);
    assert.equal(job.result.polyglotSpecs.kind, "cie-polyglot-viewspec-bundle");
    assert.equal(job.result.sourceRoot, repo);
    const engineJobs = spawns.filter(x => ["ingest-polyglot", "ingest", "polyglot", "concepts", "viz", "report", "deps", "cochange", "drift", "overlap"].includes(x.args[0]));
    assert.equal(engineJobs.length, 1, JSON.stringify(engineJobs));
    assert.equal(engineJobs[0].args[0], "ingest-polyglot");
    assert.ok(engineJobs[0].args.includes("--web-out"));
    assert.equal(job.steps.find(s => s.key === "read").state, "done");
    assert.equal(job.steps.find(s => s.key === "concepts").state, "done");
  } finally {
    if (previous.bin === undefined) delete process.env.CIE_BIN; else process.env.CIE_BIN = previous.bin;
    if (previous.jobs === undefined) delete process.env.CIE_JOBS_ROOT; else process.env.CIE_JOBS_ROOT = previous.jobs;
    await fs.rm(temp, { recursive: true, force: true });
  }
});
