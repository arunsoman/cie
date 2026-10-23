// PERF-L1 verification harness — drives the REAL web pipeline (analyze-jobs.ts
// startJob → runPipeline) against a fresh fixture copy, then dumps artifacts,
// per-step timings, wall time and the observed max concurrent "running" steps.
//
//   node scripts/perf-l1/harness.cjs <fixtureSourceDir> <outJson>
//
// The fixture is copied fresh every run (full ingest every time, identical
// starting state for baseline vs parallel), git-initialized so cochange has
// real history, and removed afterwards. Nothing in the fixture is modified.

const fs = require("node:fs");
const path = require("node:path");
const { execSync } = require("node:child_process");

const REPO = "/home/z/my-project";
const FIXTURE = "/home/z/my-project/.perf-fixture";

const [src, outRaw] = process.argv.slice(2);
const out = path.resolve(outRaw ?? "");
if (!src || !out) {
  console.error("usage: harness.cjs <fixtureSourceDir> <outJson>");
  process.exit(2);
}

// 1. fresh fixture copy — same starting state for every run
fs.rmSync(FIXTURE, { recursive: true, force: true });
execSync(`mkdir -p "${path.dirname(FIXTURE)}"`);
execSync(`rsync -a --exclude .cie --exclude .perf-fixture "${src}/" "${FIXTURE}/"`);
execSync(
  `cd "${FIXTURE}" && git init -q && git add -A && git -c user.email=cie@local -c user.name=cie commit -qm "seed for perf harness"`,
  { stdio: "pipe" }
);

// 2. load the real pipeline (TS, transpiled — same loader the web suites use)
process.chdir(REPO + "/web");
process.env.CIE_JOBS_ROOT = REPO + "/.perf-jobs";
fs.rmSync(REPO + "/.perf-jobs", { recursive: true, force: true });
const loadTs = require(path.join(REPO, "web/test/helpers/load-ts.cjs"));
const aj = loadTs()(path.join(REPO, "web/src/lib/analyze-jobs.ts"));

// 3. start the job and poll — startJob is async (fires runPipeline in the
// background and resolves with the tracked AnalyzeJob).
const t0 = Date.now();
let maxConcurrent = 0;

const done = awaitJob();

async function awaitJob() {
  const job = await aj.startJob({ source: "local", localPath: FIXTURE, repoName: "perf-fixture" });
  const hardStop = setTimeout(() => {
    const j = aj.jobs.get(job.id);
    console.error(`HARNESS TIMEOUT after 7min — job state=${j?.state} steps=${JSON.stringify(j?.steps?.map((s) => [s.key, s.state]))}`);
    process.exit(3);
  }, 420_000);
  const result = await new Promise((resolve) => {
    const poll = setInterval(() => {
      const j = aj.jobs.get(job.id);
      if (!j) return;
      if (j.state === "awaiting_permission") aj.resumeJob(job.id, true);
      const running = aj.jobStatus(j).steps.filter((s) => s.state === "running").length;
      if (running > maxConcurrent) maxConcurrent = running;
    }, 25);
    const wait = setInterval(() => {
      const j = aj.jobs.get(job.id);
      if (!j) return;
      if (j.state === "done" || j.state === "failed") {
        clearInterval(wait);
        clearInterval(poll);
        resolve(j);
      }
    }, 50);
  });
  clearTimeout(hardStop);
  return result;
}

done.then((j) => {
  const wallMs = Date.now() - t0;
  const status = aj.jobStatus(j);
  const dump = {
    state: j.state,
    error: j.error ?? null,
    wallMs,
    maxConcurrentRunning: maxConcurrent,
    steps: status.steps.map((s) => ({ key: s.key, state: s.state, ms: s.ms ?? null, detail: s.detail ?? null })),
    result: j.result ?? null,
  };
  fs.writeFileSync(out, JSON.stringify(dump, null, 1));
  console.log(`${j.state.toUpperCase()} wall=${(wallMs / 1000).toFixed(2)}s maxConcurrent=${maxConcurrent} -> ${out}`);
  if (j.state === "failed") {
    console.error("error:", j.error?.plain, "\n", j.error?.technical?.slice(0, 600));
    process.exit(1);
  }
  process.exit(0); // leave the fixture for the caller's inspection
});
