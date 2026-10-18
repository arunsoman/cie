// Server-only job system for "Connect a repo".
// A job = clone a public git repo → run the CIE engine over it → keep the
// three JSON exports in memory for the client to fetch. Jobs run in the
// background (child processes), the client polls GET /api/analyze/status.
// This module must never be imported from client code.

import { spawn } from "child_process";
import { promises as fs } from "fs";
import path from "path";
import { randomUUID } from "crypto";
import type { CieReport, CieViewSpec, ConceptTree, CieDeps, CieRuntime } from "./cie-types";
import { scanRepoLanguages } from "./repo-scan";
import { SUPPORTED_SUMMARY } from "./language-profiles";
import type { RepoScan } from "./language-profiles";

// ---------- types ----------

export type StepKey = "clone" | "inventory" | "read" | "map" | "concepts" | "deps";
export type StepState = "pending" | "running" | "done" | "failed" | "skipped";
export type JobState = "queued" | "running" | "awaiting_permission" | "done" | "failed";

export interface JobStep {
  key: StepKey;
  state: StepState;
  detail?: string; // short plain-words detail shown under the step
  ms?: number; // how long it took, when finished
}

export interface AnalyzeJob {
  id: string;
  url: string; // web URL or "local://<abs path>"
  localPath?: string; // set when source is a folder on this machine
  source: "url" | "local";
  repoName: string;
  state: JobState;
  createdAt: number;
  steps: JobStep[];
  scan?: RepoScan; // language inventory taken during the run (may be absent)
  // Ask-before-skipping: when the inventory finds files CIE can't read yet,
  // the pipeline pauses here and the user decides whether to go on. The
  // resolver is called with true (continue) or false (stop, nothing read).
  consent?: { resolve: (proceed: boolean) => void };
  error?: { plain: string; technical: string };
  result?: {
    report: CieReport;
    viewspec: CieViewSpec | null;
    concepts: ConceptTree | null;
    deps?: CieDeps | null;
    runtime?: CieRuntime | null;
    label: string;
    scan?: RepoScan | null;
  };
}

// ---------- constants ----------

const JOBS_ROOT = "/home/z/my-project/.cie-jobs";
const CLONE_TIMEOUT_MS = 120_000;
const CIE_TIMEOUT_MS = 240_000;
const JOB_TTL_MS = 45 * 60_000;
const MAX_JOBS = 12;

export const STEP_ORDER: { key: StepKey; label: string; hint: string }[] = [
  { key: "clone", label: "Getting your code", hint: "Copying the newest version of your repo — nothing in it is changed." },
  { key: "inventory", label: "Taking inventory", hint: "Listing the languages and tools in your repo — and flagging anything CIE can't read yet." },
  { key: "read", label: "Reading every file", hint: "CIE reads each file and takes notes on what it does. The busiest step." },
  { key: "map", label: "Drawing the map", hint: "Sketching how all the parts of your code connect." },
  { key: "concepts", label: "Listing what it does", hint: "Writing down what each part is responsible for." },
  { key: "deps", label: "Listing the dependencies", hint: "Reading pom.xml, build.gradle, package.json, Cargo.toml and friends — names and versions as written." },
];

const LOCAL_FIRST_STEP: { label: string; hint: string } = {
  label: "Finding your code",
  hint: "Checking the folder you pointed at — nothing is copied or uploaded.",
};

// ---------- global store (survives dev-server HMR) ----------

const g = globalThis as unknown as { __cieAnalyzeJobs?: Map<string, AnalyzeJob> };
export const jobs: Map<string, AnalyzeJob> = g.__cieAnalyzeJobs ?? (g.__cieAnalyzeJobs = new Map());

function gc() {
  const now = Date.now();
  for (const [id, j] of jobs) {
    if (now - j.createdAt > JOB_TTL_MS) jobs.delete(id);
  }
  while (jobs.size > MAX_JOBS) {
    const oldest = [...jobs.values()].sort((a, b) => a.createdAt - b.createdAt)[0];
    if (!oldest) break;
    jobs.delete(oldest.id);
  }
}

// ---------- URL validation (plain-words reasons) ----------

const BLOCKED_HOSTS = /^(localhost|127\.|0\.0\.0\.0|10\.|192\.168\.|169\.254\.|\[::1\]$|.*\.local$|.*\.internal$)/i;

// Local-folder validation — the server and the browser share this machine,
// so a path is as private as the machine itself. Nothing is copied.
const BLOCKED_LOCAL_ROOTS = ["/proc", "/sys", "/dev", "/run", "/boot", "/etc", "/root", "/bin", "/sbin", "/usr/lib", "/var"];

/** True when a resolved absolute path points into a protected system area. */
export function isBlockedLocalPath(p: string): boolean {
  return p === "/" || BLOCKED_LOCAL_ROOTS.some((r) => p === r || p.startsWith(r + "/"));
}

export function validateLocalPath(
  raw: string
): { ok: true; path: string; repoName: string } | { ok: false; plain: string } {
  const value = String(raw ?? "").trim();
  if (!value) return { ok: false, plain: "Please type the folder path first — for example /home/you/my-project" };
  if (value.length > 4096) return { ok: false, plain: "That path is strangely long — please double-check it." };
  if (!value.startsWith("/")) return { ok: false, plain: "Please use a full path starting with / — for example /home/you/my-project" };
  const p = path.resolve(value);
  if (p === "/" || BLOCKED_LOCAL_ROOTS.some((r) => p === r || p.startsWith(r + "/"))) {
    return { ok: false, plain: "That folder is off-limits (system files). Pick your project folder instead." };
  }
  const repoName = p.split("/").filter(Boolean).pop() ?? "project";
  if (!repoName || repoName.length > 100 || !/^[\w.-]+$/.test(repoName)) {
    return { ok: false, plain: "That path doesn't end in a folder name we recognise." };
  }
  return { ok: true, path: p, repoName };
}

export function validateRepoUrl(raw: string): { ok: true; url: string; repoName: string } | { ok: false; plain: string } {
  const value = String(raw ?? "").trim();
  if (!value) return { ok: false, plain: "Please paste a link to your repo first." };
  if (value.length > 2048) return { ok: false, plain: "That link is strangely long — please double-check it." };

  let u: URL;
  try {
    u = new URL(value);
  } catch {
    return { ok: false, plain: "That doesn't look like a web address. Something like https://github.com/you/your-repo" };
  }
  if (u.protocol !== "https:" && u.protocol !== "http:") {
    return { ok: false, plain: "Only http(s) links work here — something like https://github.com/you/your-repo" };
  }
  if (u.username || u.password) {
    return { ok: false, plain: "Please use a plain link without a username or password in it." };
  }
  if (BLOCKED_HOSTS.test(u.hostname)) {
    return { ok: false, plain: "That points at this server itself, not a public repo." };
  }
  if (!/^[a-z0-9.-]+$/i.test(u.hostname)) {
    return { ok: false, plain: "That web address has odd characters in it — please double-check." };
  }
  const clean = u.pathname.replace(/\.git$/, "").replace(/\/+$/, "");
  const repoName = clean.split("/").pop() ?? "repo";
  if (!repoName || repoName.length > 100 || !/^[\w.-]+$/.test(repoName)) {
    return { ok: false, plain: "That link doesn't end in a repo name we recognise." };
  }
  return { ok: true, url: `${u.protocol}//${u.host}${clean}`, repoName };
}

// ---------- process helper ----------

interface RunResult {
  code: number;
  stdout: string;
  stderr: string;
  timedOut: boolean;
}

function run(cmd: string, args: string[], opts: { cwd?: string; timeoutMs: number; maxStdout?: number }): Promise<RunResult> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, {
      cwd: opts.cwd,
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0", GIT_ASKPASS: "echo", CI: "1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    const cap = opts.maxStdout ?? 24 * 1024 * 1024;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, opts.timeoutMs);
    child.stdout.on("data", (d: Buffer) => {
      if (stdout.length < cap) stdout += d.toString();
    });
    child.stderr.on("data", (d: Buffer) => {
      if (stderr.length < 64 * 1024) stderr += d.toString();
    });
    child.on("error", (err) => {
      clearTimeout(timer);
      resolve({ code: -1, stdout, stderr: `${stderr}\n${err.message}`, timedOut });
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code: code ?? -1, stdout, stderr, timedOut });
    });
  });
}

// ---------- engine binary ----------

const CIE_CANDIDATES = [
  process.env.CIE_BIN ?? "",
  "/home/z/my-project/cie/target/debug/cie",
  "/home/z/my-project/cie/target/release/cie",
];

async function resolveCieBin(): Promise<string | null> {
  for (const c of CIE_CANDIDATES) {
    if (!c) continue;
    try {
      await fs.access(c);
      return c;
    } catch {
      /* keep looking */
    }
  }
  // Last resort: a `cie` on PATH (set CIE_BIN to override everything).
  const probe = await run("cie", ["--version"], { timeoutMs: 10_000, maxStdout: 4096 });
  return probe.code === 0 ? "cie" : null;
}

// ---------- plain-words error mapping ----------

function cloneErrorPlain(stderr: string, timedOut: boolean): string {
  const s = stderr.toLowerCase();
  if (timedOut) return "Getting your code took too long. Try a smaller repo.";
  if (s.includes("could not resolve host")) return "We couldn't find that website — is the address spelled right?";
  if (s.includes("authentication failed") || s.includes("could not read username") || s.includes("terminal prompts disabled"))
    return "That repo looks private (or the address is wrong). CIE can only read public repos for now.";
  if (s.includes("not found") || s.includes("404")) return "We couldn't find a repo at that address. Double-check the link — it should look like https://github.com/you/your-repo";
  if (s.includes("empty repository")) return "That repo is empty — there's no code in it yet.";
  return "We couldn't reach that repo. Check the address, and note it must be a public repo.";
}

function engineErrorPlain(step: StepKey, timedOut: boolean, binMissing: boolean): string {
  if (binMissing) return "The CIE engine isn't installed on this server yet — that's on us, not you.";
  if (timedOut) return "This project took too long to check. Try a smaller repo for now.";
  if (step === "read") return "CIE couldn't fully read this project. It may use a language CIE doesn't support yet.";
  return "CIE hit a snag while preparing your results. You can try again.";
}

const UNSUPPORTED_PLAIN =
  "We read your repo, but couldn't find any code files CIE understands yet. " +
  `${SUPPORTED_SUMMARY} Everything else (notes, configs, docs, other languages) is skipped — and named honestly, never guessed.`;

// ---------- the pipeline ----------

function setStep(job: AnalyzeJob, key: StepKey, state: StepState, detail?: string, ms?: number) {
  const s = job.steps.find((x) => x.key === key);
  if (!s) return;
  s.state = state;
  if (detail !== undefined) s.detail = detail;
  if (ms !== undefined) s.ms = ms;
}

function markRunningStepFailed(job: AnalyzeJob, plain: string, technical: string) {
  const running = job.steps.find((s) => s.state === "running");
  if (running) setStep(job, running.key, "failed", plain);
  job.state = "failed";
  job.error = { plain, technical };
}

/**
 * Ask before skipping: when the inventory found files CIE can't read yet,
 * the pipeline pauses and waits for the user's decision. True = continue
 * (unread files stay honestly skipped); false = stop right here — nothing
 * has been read yet beyond the inventory, and nothing will be.
 */
async function askPermission(job: AnalyzeJob, scan: RepoScan): Promise<boolean> {
  job.state = "awaiting_permission";
  const decision = await new Promise<boolean>((resolve) => {
    job.consent = { resolve };
  });
  job.consent = undefined;
  job.state = "running";
  return decision;
}

/** The user answered the ask-before-skipping question. */
export function resumeJob(id: string, proceed: boolean): AnalyzeJob | null {
  const job = jobs.get(id);
  if (!job || job.state !== "awaiting_permission" || !job.consent) return null;
  // Optimistic state so the answerer's immediate status snapshot reflects the
  // decision (the pipeline continuation confirms it a moment later).
  if (proceed) job.state = "running";
  job.consent.resolve(proceed);
  return job;
}

export async function startJob(opts: {
  source: "url" | "local";
  url?: string;
  localPath?: string;
  repoName: string;
}): Promise<AnalyzeJob> {
  gc();
  const job: AnalyzeJob = {
    id: randomUUID(),
    url: opts.source === "local" ? `local://${opts.localPath}` : (opts.url ?? ""),
    localPath: opts.source === "local" ? opts.localPath : undefined,
    source: opts.source,
    repoName: opts.repoName,
    state: "queued",
    createdAt: Date.now(),
    steps: STEP_ORDER.map((s) => ({ key: s.key, state: "pending" as StepState })),
  };
  jobs.set(job.id, job);
  // Fire and forget — the status endpoint reports progress.
  void runPipeline(job).catch((err) => {
    if (job.state !== "failed") {
      markRunningStepFailed(job, "Something went wrong. You can try again.", String(err?.stack ?? err));
    }
  });
  return job;
}

async function runPipeline(job: AnalyzeJob) {
  const isLocal = job.source === "local";
  const workDir = isLocal ? null : path.join(JOBS_ROOT, job.id);
  const repoDir = isLocal ? (job.localPath as string) : path.join(workDir as string, "repo");
  const vizOut = path.join(workDir ?? JOBS_ROOT, "viewspec.json");
  job.state = "running";

  try {
    // 0. Engine present?
    const bin = await resolveCieBin();
    if (!bin) {
      markRunningStepFailed(job, engineErrorPlain("read", false, true), "cie binary not found (looked at CIE_BIN, target/debug, target/release)");
      return;
    }

    // 1. clone (web) or verify (local)
    if (isLocal) {
      setStep(job, "clone", "running", "Checking the folder…");
      const t0 = Date.now();
      try {
        const st = await fs.stat(repoDir);
        if (!st.isDirectory()) {
          markRunningStepFailed(job, "That path points at a file, not a folder. Please give me the project folder.", `not a directory: ${repoDir}`);
          return;
        }
      } catch {
        markRunningStepFailed(job, "We couldn't find that folder on this computer. Double-check the path (and spelling).", `stat failed: ${repoDir}`);
        return;
      }
      setStep(job, "clone", "done", "Found it — reading right where it lives.", Date.now() - t0);
    } else {
      await fs.mkdir(workDir as string, { recursive: true });
      setStep(job, "clone", "running", "Copying the newest version…");
      const t0 = Date.now();
      const clone = await run("git", ["clone", "--depth", "1", "--single-branch", job.url, repoDir], { timeoutMs: CLONE_TIMEOUT_MS });
      if (clone.code !== 0) {
        markRunningStepFailed(job, cloneErrorPlain(clone.stderr, clone.timedOut), `git clone exit ${clone.code}: ${clone.stderr.slice(0, 400)}`);
        return;
      }
      setStep(job, "clone", "done", "Got it.", Date.now() - t0);
    }

    // 1.5. inventory — which languages does this repo use, which can CIE
    //      read, which can't it yet. Never fatal: a failed inventory just
    //      means no language card, the engine still runs.
    setStep(job, "inventory", "running", "Listing languages and tools…");
    const tInv = Date.now();
    try {
      const scan = await scanRepoLanguages(repoDir, job.repoName);
      job.scan = scan;
      const reads = scan.languages.map((l) => `${l.name} ${l.files}`).join(", ");
      const cant = scan.unsupported.map((l) => `${l.name} ${l.files}`).join(", ");
      const fw = scan.frameworks.map((f) => f.name.split(" (")[0]);
      const bits: string[] = [];
      if (fw.length) bits.push(fw.join(" · "));
      if (reads) bits.push(`CIE reads: ${reads}`);
      if (cant) bits.push(`Can't read yet: ${cant}`);
      setStep(job, "inventory", "done", bits.join(" — ") || "Nothing recognisable found.", Date.now() - tInv);
    } catch {
      setStep(job, "inventory", "done", "Couldn't take inventory — continuing anyway.", Date.now() - tInv);
    }

    // 1.6. ask before skipping — if CIE can't read some files here, the user
    //      decides whether to go on. Nothing has been read yet.
    if (job.scan && job.scan.unsupported.length > 0) {
      const proceed = await askPermission(job, job.scan);
      if (!proceed) {
        const cant = job.scan.unsupported.map((l) => `${l.name} (${l.files})`).join(", ");
        markRunningStepFailed(
          job,
          `Stopped, just like you asked. Nothing was read — the inventory is all CIE took. The files waiting were: ${cant}. When CIE learns those languages, this check will cover them too.`,
          "user declined consent after inventory (ask-before-skipping)"
        );
        return;
      }
      const skipped = job.scan.unsupported.map((l) => `${l.name} ${l.files}`).join(", ");
      setStep(job, "inventory", "done", `You said go on — ${skipped} stay honestly skipped.`, undefined);
    }

    // 2. read = ingest (two-pass contract: repo-before-recompute) + report
    setStep(job, "read", "running", "CIE is reading each file and taking notes…");
    const t1 = Date.now();
    const ing = await run(bin, ["ingest", repoDir], { timeoutMs: CIE_TIMEOUT_MS });
    if (ing.timedOut || ing.code !== 0) {
      markRunningStepFailed(job, engineErrorPlain("read", ing.timedOut, false), `cie ingest exit ${ing.code}: ${(ing.stderr || ing.stdout).slice(0, 400)}`);
      return;
    }
    if (/files 0 \|/.test(ing.stdout)) {
      markRunningStepFailed(job, UNSUPPORTED_PLAIN, `cie ingest found 0 supported files: ${ing.stdout.slice(0, 200)}`);
      return;
    }
    const rep = await run(bin, ["report", repoDir, "--json"], { timeoutMs: CIE_TIMEOUT_MS });
    if (rep.timedOut || rep.code !== 0) {
      const noSnapshot = (rep.stderr || "").includes("no snapshot");
      markRunningStepFailed(
        job,
        noSnapshot ? UNSUPPORTED_PLAIN : engineErrorPlain("read", rep.timedOut, false),
        `cie report exit ${rep.code}: ${(rep.stderr || rep.stdout).slice(0, 400)}`
      );
      return;
    }
    let report: CieReport;
    try {
      report = JSON.parse(rep.stdout);
    } catch {
      markRunningStepFailed(job, engineErrorPlain("read", false, false), "cie report produced unparseable JSON");
      return;
    }
    setStep(job, "read", "done", `${report.symbols.length} named things found.`, Date.now() - t1);

    // 3. viewspec (the map)
    setStep(job, "map", "running", "Sketching the connections…");
    const t2 = Date.now();
    const viz = await run(bin, ["viz", repoDir, "--out", vizOut], { timeoutMs: CIE_TIMEOUT_MS });
    let viewspec: CieViewSpec | null = null;
    if (!viz.timedOut && viz.code === 0) {
      try {
        viewspec = JSON.parse(await fs.readFile(vizOut, "utf8"));
      } catch {
        viewspec = null;
      }
    }
    // A missing map is not fatal — the dashboard degrades gracefully.
    setStep(job, "map", viewspec ? "done" : "failed", viewspec ? "Map ready." : "No map for this project — the other pages still work.", Date.now() - t2);

    // 4. concepts (what it does)
    setStep(job, "concepts", "running", "Writing down responsibilities…");
    const t3 = Date.now();
    const con = await run(bin, ["concepts", repoDir, "--format", "json"], { timeoutMs: CIE_TIMEOUT_MS });
    let concepts: ConceptTree | null = null;
    if (!con.timedOut && con.code === 0) {
      try {
        concepts = JSON.parse(con.stdout);
      } catch {
        concepts = null;
      }
    }
    setStep(job, "concepts", "done", concepts ? "List ready." : "Skipped — not every project has one.", Date.now() - t3);

    // 5. dependencies (pom.xml, build.gradle, package.json, Cargo.toml, …)
    //    Names and versions as written — never resolved, never guessed.
    setStep(job, "deps", "running", "Reading the manifest files…");
    const t4 = Date.now();
    let deps: CieDeps | null = null;
    const dep = await run(bin, ["deps", repoDir, "--json"], { timeoutMs: Math.min(CIE_TIMEOUT_MS, 60_000) });
    if (!dep.timedOut && dep.code === 0) {
      try {
        const parsed = JSON.parse(dep.stdout) as CieDeps;
        if (parsed?.table) deps = parsed;
      } catch {
        deps = null;
      }
    }
    setStep(
      job,
      "deps",
      deps && deps.table.files_scanned > 0 ? "done" : "skipped",
      deps && deps.table.files_scanned > 0
        ? `${deps.table.files_scanned} manifest file(s) read — ${deps.table.deps.length} dependencies declared as written.`
        : "No manifest files found — nothing to list.",
      Date.now() - t4
    );

    // 6. runtime evidence — only if the project ships its own collector
    //    output (runtime-data/runtime.json). CIE never invents runtime data.
    let runtime: CieRuntime | null = null;
    try {
      const rtPath = path.join(repoDir, "runtime-data", "runtime.json");
      const st = await fs.stat(rtPath);
      if (st.isFile() && st.size <= 4 * 1024 * 1024) {
        runtime = JSON.parse(await fs.readFile(rtPath, "utf8")) as CieRuntime;
      }
    } catch {
      runtime = null; // no runtime evidence — the dashboard says so honestly
    }

    job.result = {
      report,
      viewspec,
      concepts,
      deps,
      runtime,
      label: isLocal
        ? `Your project: ${job.repoName} (from this computer)`
        : `Your project: ${job.repoName} (from ${hostOf(job.url)})`,
      scan: job.scan ?? null,
    };
    job.state = "done";
  } finally {
    // Don't keep clones on disk — results live in memory. Local folders are
    // never touched (read-only analysis in place).
    if (workDir) await fs.rm(workDir, { recursive: true, force: true }).catch(() => {});
  }
}

function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return "the web";
  }
}

// ---------- public shape for the client ----------

export interface JobStatus {
  id: string;
  state: JobState;
  repoName: string;
  scan?: RepoScan;
  steps: { key: StepKey; label: string; hint: string; state: StepState; detail?: string; ms?: number }[];
  error?: { plain: string; technical: string };
  /** Set while state === "awaiting_permission": the plain-words question. */
  permission?: { plain: string; unreadable: { name: string; files: number }[] } | null;
}

export function jobStatus(job: AnalyzeJob): JobStatus {
  const permission =
    job.state === "awaiting_permission" && job.scan && job.scan.unsupported.length > 0
      ? {
          plain:
            `CIE can read: ${job.scan.languages.map((l) => `${l.name} (${l.files})`).join(", ") || "nothing here yet"}. ` +
            `CIE can't read yet: ${job.scan.unsupported.map((l) => `${l.name} (${l.files})`).join(", ")}. ` +
            "Go ahead with the ones CIE can read — the others stay honestly skipped?",
          unreadable: job.scan.unsupported.map((l) => ({ name: l.name, files: l.files })),
        }
      : null;
  return {
    id: job.id,
    state: job.state,
    repoName: job.repoName,
    scan: job.scan,
    error: job.error,
    permission,
    steps: STEP_ORDER.map((meta0) => {
      const meta = job.source === "local" && meta0.key === "clone" ? { ...meta0, ...LOCAL_FIRST_STEP } : meta0;
      const s = job.steps.find((x) => x.key === meta.key);
      return {
        key: meta.key,
        label: meta.label,
        hint: meta.hint,
        state: s?.state ?? "pending",
        detail: s?.detail,
        ms: s?.ms,
      };
    }),
  }
}
