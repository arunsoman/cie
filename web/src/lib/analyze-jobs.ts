// Server-only job system for "Connect a repo".
// A job = clone a public git repo → run the CIE engine over it → keep the
// compatible analysis artifacts for the client to fetch. One Rust process
// owns each ingestion; the client polls GET /api/analyze/status.
// This module must never be imported from client code.

import { storageCall } from "./storage-native";
import { spawn } from "child_process";
import { StringDecoder } from "string_decoder";
import { promises as fs } from "fs";
import path from "path";
import { randomUUID } from "crypto";
import type { CieReport, CieViewSpec, ConceptTree, CieDeps, CieRuntime, CieCochange, CieDrift, CieOverlap } from "./cie-types";
import type { TechStack } from "./stack-types";
import { isTechStack } from "./stack-types";
import { isPolyglotBundle, type PolyglotBundle } from "./polyglot-types";
import { scanRepoLanguages } from "./repo-scan";
import { SUPPORTED_SUMMARY } from "./language-profiles";
import type { RepoScan } from "./language-profiles";

// ---------- types ----------

export type StepKey = "clone" | "inventory" | "read" | "resolve" | "storage" | "publish" | "map" | "concepts" | "deps" | "stack" | "polyglot" | "cochange" | "overlap" | "drift";
export type StepState = "pending" | "running" | "done" | "failed" | "skipped";
export type JobState = "queued" | "running" | "awaiting_permission" | "done" | "failed";

export interface JobStep {
  key: StepKey;
  state: StepState;
  detail?: string; // short plain-words detail shown under the step
  startedAt?: number; // internal phase clock
  ms?: number; // how long it took, when finished
}

export interface AnalyzeJob {
  id: string;
  url: string; // web URL or "local://<abs path>"
  localPath?: string; // set when source is a folder on this machine
  source: "url" | "local";
  repoName: string;
  /** branch the user picked, in short display form: "dev", "origin/dev" */
  branch?: string;
  /** exact ref the analysis actually reads: for URL jobs the branch name
   *  itself; for local jobs a full ref ("refs/heads/dev") or plain name to
   *  resolve in priority order */
  branchRef?: string;
  /** set when the local job analysed a read-only git-archive snapshot of
   *  the picked branch (the user's folder was never checked out away). */
  viaSnapshot?: boolean;
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
    /** the tech-stack artifact from `cie stack` — null when not available */
    stack?: TechStack | null;
    cochange?: CieCochange | null;
    overlap?: CieOverlap | null;
    drift?: CieDrift | null;
    label: string;
    scan?: RepoScan | null;
    /** PRD v2.0 polyglot bundle (`cie polyglot --viewspec all`) — C4,
     *  state-machine, workflow and impact ViewSpecs, gate-checked on the
     *  engine side; null when the layer produced nothing nameable. */
    polyglotSpecs?: PolyglotBundle | null;
    // The absolute root the source panel may read from — set ONLY for
    // local-folder jobs (the folder the user themselves pointed at, read
    // in place). URL clones are deleted after analysis; uploads never
    // touched disk. Null there, and the panel says so honestly.
    sourceRoot?: string | null;
    storageRoot?: string | null;
  };
}

// ---------- constants ----------

// Scratch root for URL-clone jobs (clone target, viz output, per-job
// workspace). CIE_JOBS_ROOT overrides everything; the fallback is relative
// to the server's working directory — deliberately NOT a hardcoded absolute
// path tied to one developer's machine (issue #4: on any other box the old
// /home/z/... path made every URL job die with EACCES before cloning even
// started, hidden behind the generic "something went wrong" banner).
const JOBS_ROOT = process.env.CIE_JOBS_ROOT?.trim() || path.join(process.cwd(), ".cie-jobs");
const CLONE_TIMEOUT_MS = 120_000;
const CIE_TIMEOUT_MS = 240_000;
const JOB_TTL_MS = 45 * 60_000;
const MAX_JOBS = 12;

export const STEP_ORDER: { key: StepKey; label: string; hint: string }[] = [
  { key: "clone", label: "Getting your code", hint: "Copying the newest version of your repo — nothing in it is changed." },
  { key: "inventory", label: "Taking inventory", hint: "Listing the languages and tools in your repo — and flagging anything CIE can't read yet." },
  { key: "read", label: "Streaming source files", hint: "Read each supported file once, parse it and reuse unchanged cached facts." },
  { key: "resolve", label: "Connecting repository facts", hint: "Assemble symbols and resolve calls across files." },
  { key: "stack", label: "Analyzing frameworks", hint: "Identify frameworks, routes and runtime structure." },
  { key: "concepts", label: "Analyzing and grouping concepts", hint: "Analyze behavior, security and tests, then build FCA/Godin concept groups." },
  { key: "storage", label: "Building views and storing the revision", hint: "The in-process storage module builds C4, state and workflow views, groups duplicates and persists the revision." },
  { key: "polyglot", label: "Loading architecture views", hint: "Retrieve the stored polyglot views for the web app." },
  { key: "map", label: "Drawing the map", hint: "Prepare the repository map from captured evidence." },
  { key: "deps", label: "Listing dependencies", hint: "Combine package manifests with captured imports." },
  { key: "cochange", label: "Checking what changes together", hint: "Mining git history for files that tend to be edited in the same commit." },
  { key: "overlap", label: "Checking for overlap", hint: "Comparing every part's symbols against every other part — what's shared, what's not." },
  { key: "drift", label: "Checking docs against code", hint: "Reading your docs' own claims and verifying each one against what the code actually does." },
  { key: "publish", label: "Preparing your results", hint: "Publish the complete artifact bundle for the web app." },
];

const LOCAL_FIRST_STEP: { label: string; hint: string } = {
  label: "Finding your code",
  hint: "Checking the folder you pointed at — nothing is copied or uploaded.",
};

const LOCAL_SNAPSHOT_STEP: { label: string; hint: string } = {
  label: "Getting that branch",
  hint: "Taking a read-only copy of the branch you picked — your folder stays exactly as it is.",
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

export interface RunResult {
  code: number;
  stdout: string;
  stderr: string;
  timedOut: boolean;
  /** True when stdout data past the byte cap was captured but DROPPED.
   *  A truncated JSON payload must never be misreported as the engine
   *  "producing unparseable JSON" — the cut is on our side (issue #12). */
  stdoutTruncated: boolean;
}

/** A read-only git command run inside a folder (or plain git with dir=null).
 *  Only read commands are ever passed — nothing here can change a repo. */
async function runGit(dir: string | null, args: string[], timeoutMs: number, maxStdout = 4 * 1024 * 1024): Promise<RunResult> {
  return run("git", dir ? ["-C", dir, ...args] : args, { timeoutMs, maxStdout });
}

export function run(cmd: string, args: string[], opts: { cwd?: string; timeoutMs: number; maxStdout?: number; onStderr?: (text: string) => void }): Promise<RunResult> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, {
      cwd: opts.cwd,
      env: { ...process.env, GIT_TERMINAL_PROMPT: "0", GIT_ASKPASS: "echo", CI: "1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    let timedOut = false;
    let stdoutTruncated = false;
    const cap = opts.maxStdout ?? 256 * 1024 * 1024;
    const timer = setTimeout(() => {
      timedOut = true;
      child.kill("SIGKILL");
    }, opts.timeoutMs);
    // StringDecoder, not Buffer.toString(): a multi-byte UTF-8 character
    // split across pipe chunks must never be corrupted mid-JSON — chunk
    // boundaries are a property of the pipe, not of the text.
    const stdoutDecoder = new StringDecoder("utf8");
    const stderrDecoder = new StringDecoder("utf8");
    child.stdout.on("data", (d: Buffer) => {
      if (stdout.length + d.length > cap) {
        stdoutTruncated = true;
        return; // keep draining so the child never blocks on a full pipe
      }
      stdout += stdoutDecoder.write(d);
    });
    child.stderr.on("data", (d: Buffer) => {
      const text = stderrDecoder.write(d);
      opts.onStderr?.(text);
      if (stderr.length < 64 * 1024) stderr += text;
    });
    child.on("error", (err) => {
      clearTimeout(timer);
      resolve({ code: -1, stdout, stderr: `${stderr}\n${err.message}`, timedOut, stdoutTruncated });
    });
    child.on("close", (code) => {
      clearTimeout(timer);
      stdout += stdoutDecoder.end();
      stderr += stderrDecoder.end();
      resolve({ code: code ?? -1, stdout, stderr, timedOut, stdoutTruncated });
    });
  });
}

// ---------- branch names (plain-words reasons) ----------

const BRANCH_NAME_RE = /^[A-Za-z0-9._/-]{1,200}$/;

/** Git's ref-format rules reduced to the cases that matter to us: nothing
 *  that looks like an option, no traversal, no characters git itself bans. */
function badBranchName(v: string): boolean {
  if (!BRANCH_NAME_RE.test(v)) return true;
  if (v.startsWith("-") || v.startsWith(".") || v.startsWith("/")) return true;
  if (v.endsWith("/") || v.endsWith(".")) return true;
  if (v.endsWith(".lock")) return true;
  if (v.includes("..") || v.includes("//") || v.includes("/.")) return true;
  return false;
}

/** A branch a user picked for a remote URL job — "main", "feature/x", a tag. */
export function validateBranch(raw: string): { ok: true; name: string } | { ok: false; plain: string } {
  const v = String(raw ?? "").trim();
  if (!v) return { ok: false, plain: "Please pick or type the branch to read — or leave it empty for the repo's default branch." };
  if (v.length > 200) return { ok: false, plain: "That branch name is strangely long — please double-check it." };
  if (badBranchName(v))
    return { ok: false, plain: "That doesn't look like a git branch or tag name — for example main, dev, feature/speed-up or v1.2." };
  return { ok: true, name: v };
}

/** A branch for a LOCAL job: either a plain name ("dev") or a full ref the
 *  branch list offered ("refs/remotes/origin/dev"). Plain names are resolved
 *  in a fixed priority order when the snapshot is taken. */
export function validateBranchRef(raw: string): { ok: true; ref: string } | { ok: false; plain: string } {
  const v = String(raw ?? "").trim();
  if (!v) return { ok: false, plain: "Please pick or type the branch to read — or leave it empty to read the folder exactly as it is on disk." };
  if (v.length > 250) return { ok: false, plain: "That branch name is strangely long — please double-check it." };
  if (v.startsWith("refs/")) {
    const m = /^refs\/(heads|remotes|tags)\/(.+)$/.exec(v);
    if (!m || badBranchName(m[2]))
      return { ok: false, plain: "That doesn't look like a git branch — for example main, dev or origin/dev." };
    return { ok: true, ref: v };
  }
  if (badBranchName(v))
    return { ok: false, plain: "That doesn't look like a git branch — for example main, dev or origin/dev." };
  return { ok: true, ref: v };
}

/** Short display name for a ref: refs/heads/dev and dev → "dev",
 *  refs/remotes/origin/dev → "origin/dev" (reads as: origin's dev). */
function shortBranchName(ref: string): string {
  return ref.replace(/^refs\/(heads|tags)\//, "").replace(/^refs\/remotes\//, "");
}

// ---------- branch listing (read-only, for the branch picker) ----------

export interface BranchList {
  source: "local" | "url";
  /** local: does this folder hold a git repo at all? */
  git?: boolean;
  /** local: the branch checked out right now (null when detached). */
  current?: string | null;
  /** local: short hash checked out right now, when detached. */
  detachedSha?: string | null;
  /** local: this checkout's own branches (refs/heads/*). */
  local?: string[];
  /** local: remote-tracking branches, "origin/x" (refs/remotes/*). */
  remote?: string[];
  /** url: branch names on the remote repo. */
  heads?: string[];
  /** plain-words note when the list could not be fully read. */
  note?: string | null;
}

const MAX_LISTED = 200;

function capSort(names: string[]): string[] {
  return [...new Set(names)].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: "base" })).slice(0, MAX_LISTED);
}

// ls-remote failure → plain words (why a branch list might not come back).
function lsRemoteNote(stderr: string, timedOut: boolean): string {
  const s = stderr.toLowerCase();
  if (timedOut) return "Reading the branch list took too long — you can still type a branch name by hand.";
  if (s.includes("could not resolve host")) return "We couldn't reach that website to read its branches — is the address spelled right?";
  if (s.includes("authentication failed") || s.includes("could not read username") || s.includes("terminal prompts disabled"))
    return "That repo looks private, so only its owner can list its branches. You can still type a branch name by hand.";
  if (s.includes("empty repository")) return "That repo is empty — no branches yet.";
  return "We couldn't read the branch list from that address — you can still type a branch name by hand.";
}

/** Branch names for the picker. Local = walk refs inside the user's own
 *  folder (read-only; nothing is written). URL = git ls-remote (one small
 *  network read). A failed listing is a note, never a fatal error — the
 *  analysis itself can still choose a branch by name. */
export async function listBranches(
  opts: { source: "local"; localPath: string } | { source: "url"; url: string }
): Promise<BranchList> {
  if (opts.source === "url") {
    const ls = await runGit(null, ["ls-remote", "--heads", opts.url], 20_000);
    if (ls.timedOut || ls.code !== 0) {
      return { source: "url", heads: [], note: lsRemoteNote(ls.stderr || ls.stdout, ls.timedOut) };
    }
    const heads: string[] = [];
    for (const line of ls.stdout.split("\n")) {
      const m = /^[0-9a-f]+\s+refs\/heads\/(.+)$/.exec(line.trim());
      if (m) heads.push(m[1]);
    }
    return { source: "url", heads: capSort(heads), note: heads.length === 0 ? "That repo has no branches on the web side yet — it may be empty." : null };
  }

  // local: is there a git repo here?
  const p = opts.localPath;
  const isRepo = await runGit(p, ["rev-parse", "--is-inside-work-tree"], 10_000);
  if (isRepo.code !== 0 || isRepo.stdout.trim() !== "true") {
    return { source: "local", git: false };
  }
  // what's checked out right now?
  let current: string | null = null;
  let detachedSha: string | null = null;
  const head = await runGit(p, ["rev-parse", "--abbrev-ref", "HEAD"], 10_000);
  if (head.code === 0) {
    const v = head.stdout.trim();
    if (v && v !== "HEAD") current = v;
    else detachedSha = (await runGit(p, ["rev-parse", "--short", "HEAD"], 10_000)).stdout.trim() || null;
  }
  // this checkout's own branches + remote-tracking ones (read-only walks of refs)
  // Full refnames are requested so remote HEAD symrefs (refs/remotes/origin/HEAD,
  // which %(refname:short) flattens to a bare "origin") can be filtered out
  // explicitly before shortening.
  let local: string[] = [];
  let remote: string[] = [];
  const lb = await runGit(p, ["branch", "--format=%(refname)"], 15_000);
  if (lb.code === 0) {
    local = capSort(
      lb.stdout
        .split("\n")
        .map((l) => l.trim())
        .filter((l) => l.startsWith("refs/heads/"))
        .map((l) => shortBranchName(l))
    );
  }
  const rb = await runGit(p, ["branch", "-r", "--format=%(refname)"], 15_000);
  if (rb.code === 0) {
    remote = capSort(
      rb.stdout
        .split("\n")
        .map((l) => l.trim())
        .filter((l) => l.startsWith("refs/remotes/") && !l.endsWith("/HEAD"))
        .map((l) => shortBranchName(l))
    );
  }
  const note =
    lb.code !== 0 || rb.code !== 0
      ? "We could see it's a git repo but couldn't read its branch list — you can still type a branch name by hand."
      : null;
  return { source: "local", git: true, current, detachedSha, local, remote, note };
}

// ---------- engine binary ----------
//
// Resolution order (issue #7 — fail closed, never run an impostor):
//   1. CIE_BIN env var — explicit, always wins.
//   2. This checkout's cargo workspace targets, found by walking up from
//      process.cwd(): <root>/target/{debug,release}/cie. Works in any
//      clone on any machine — no hardcoded personal paths.
//   3. Nothing. The old blind PATH probe is gone: on a real user's
//      machine an unrelated tool that happens to be named `cie` (a
//      Python/Click CLI) answered --version with exit 0, got accepted as
//      "the engine", and its usage error was then misreported as a
//      language-support problem. A PATH binary is only accepted if its
//      --version output carries this engine's identity marker; otherwise
//      we fail closed with an honest "engine not found".

// clap prints "cie 1.2.0" (no comma). The unrelated same-named Click CLI
// prints "cie, version 0.1.4" — the marker rejects it.
const CIE_VERSION_MARKER = /^cie \d+\.\d+\.\d+\s*$/m;

const CIE_CANDIDATES = [
  process.env.CIE_BIN ?? "",
  ...walkUpRoots().flatMap((r) => [path.join(r, "target", "debug", "cie"), path.join(r, "target", "release", "cie")]),
];

/** The starting directory and up to five ancestors of it. */
function walkUpRoots(): string[] {
  const roots: string[] = [];
  let dir = process.cwd();
  for (let i = 0; i < 6; i++) {
    roots.push(dir);
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return roots;
}

let resolvedCieBin: string | null = null;

export async function resolveCieBin(): Promise<string | null> {
  if (resolvedCieBin) {
    try {
      await fs.access(resolvedCieBin);
      return resolvedCieBin;
    } catch {
      resolvedCieBin = null; // binary vanished (rebuild?) — resolve again
    }
  }
  for (const c of CIE_CANDIDATES) {
    if (!c) continue;
    try {
      await fs.access(c);
      resolvedCieBin = c;
      return c;
    } catch {
      /* keep looking */
    }
  }
  // Verified last resort: a `cie` on $PATH that can PROVE it is this
  // project's engine. Identity, not just exit code.
  const probe = await run("cie", ["--version"], { timeoutMs: 10_000, maxStdout: 4096 });
  const out = `${probe.stdout}\n${probe.stderr}`;
  if (probe.code === 0 && CIE_VERSION_MARKER.test(out.trim())) {
    resolvedCieBin = "cie";
    return "cie";
  }
  return null;
}

/** Honest "engine missing" technical detail: exactly where the resolver
 *  looked and what it found, so the UI's technical panel can answer
 *  "why does it say the engine is missing?" without server access. */
async function cieBinMissingTechnical(): Promise<string> {
  const lines: string[] = ["cie binary not found (or not provably this project's engine). Searched:"];
  for (const c of CIE_CANDIDATES) {
    if (!c) continue;
    let state: string;
    try {
      await fs.access(c);
      state = "exists but was not selected";
    } catch {
      state = "missing";
    }
    lines.push(`  ${c} — ${state}`);
  }
  lines.push("  `cie` on $PATH — either not found, or its --version output did not match this engine (a same-named unrelated tool must never be run silently)");
  lines.push("Set CIE_BIN to the full path of this project's engine binary and try again.");
  return lines.join("\n");
}

// ---------- plain-words error mapping ----------

/** Prefix technical details with the exact engine binary that ran, so a
 *  wrong/impostor binary is diagnosable from the UI alone (issue #7). */
function engineTechnical(bin: string, detail: string): string {
  return `engine binary: ${bin}\n${detail}`;
}

function cloneErrorPlain(stderr: string, timedOut: boolean, branch?: string): string {
  const s = stderr.toLowerCase();
  if (timedOut) return "Getting your code took too long. Try a smaller repo.";
  if (s.includes("could not resolve host")) return "We couldn't find that website — is the address spelled right?";
  if (s.includes("authentication failed") || s.includes("could not read username") || s.includes("terminal prompts disabled"))
    return "That repo looks private (or the address is wrong). CIE can only read public repos for now.";
  // before the generic "not found" check: "fatal: Remote branch x not found
  // in upstream origin" also contains "not found" — the message must name the
  // branch, not blame the link.
  if (s.includes("not found in upstream") || s.includes("could not find remote branch"))
    return `There's no branch or tag called "${branch ?? "that one"}" on this repo — the link itself is fine. Names are case-sensitive; double-check the spelling, or leave the branch empty for the default.`;
  if (s.includes("not found") || s.includes("404")) return "We couldn't find a repo at that address. Double-check the link — it should look like https://github.com/you/your-repo";
  if (s.includes("empty repository")) return "That repo is empty — there's no code in it yet.";
  return "We couldn't reach that repo. Check the address, and note it must be a public repo.";
}

// Engine failures in plain words. A crash here is NEVER a language claim:
// the inventory step already named every language in the repo, and the only
// honest "no code we can read" case (zero supported files / "no snapshot")
// has its own branch (UNSUPPORTED_PLAIN). Every other failure — non-zero
// exit, panic, malformed output — is on the engine, so the message says so
// and points at the technical details instead of guessing at a cause.
function engineErrorPlain(kind: "bin-missing" | "timeout" | "crash" | "bad-output"): string {
  if (kind === "bin-missing") return "The CIE engine isn't installed on this server yet — that's on us, not you.";
  if (kind === "timeout") return "This project took too long to check. Try a smaller repo for now.";
  if (kind === "bad-output")
    return "The CIE engine finished but produced output we couldn't understand — that's on us, not your project. Nothing needs fixing on your side; the technical details below show what went wrong.";
  return "The CIE engine hit an unexpected error while reading this project — that's on us, not you. It isn't about the languages in your repo (the inventory step already checked those). You can try again; the technical details below show exactly what happened.";
}

const UNSUPPORTED_PLAIN =
  "We read your repo, but couldn't find any code files CIE understands yet. " +
  `${SUPPORTED_SUMMARY} Everything else (notes, configs, docs, other languages) is skipped — and named honestly, never guessed.`;

// ---------- the pipeline ----------

function setStep(job: AnalyzeJob, key: StepKey, state: StepState, detail?: string, ms?: number) {
  const s = job.steps.find((x) => x.key === key);
  if (!s) return;
  if (state === "running" && s.state !== "running") s.startedAt = Date.now();
  if (["done", "skipped", "failed"].includes(state) && s.startedAt !== undefined) {
    s.ms = Date.now() - s.startedAt;
  }
  s.state = state;
  if (detail !== undefined) s.detail = detail;
  if (ms !== undefined) s.ms = ms;
}

function markRunningStepFailed(job: AnalyzeJob, plain: string, technical: string) {
  const running = job.steps.find((s) => s.state === "running");
  if (running) {
    setStep(job, running.key, "failed", plain);
  } else {
    // No step held "running" (engine missing before the pipeline started,
    // consent declined between steps, or an exception at a step boundary) —
    // mark the step that would have run next, so the step list shows where
    // it stopped instead of an all-pending list under a red banner.
    const next = job.steps.find((s) => s.state !== "done" && s.state !== "failed");
    if (next) setStep(job, next.key, "failed", plain);
  }
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
  branch?: string; // URL jobs: plain branch/tag name
  branchRef?: string; // local jobs: full ref or plain name (see validateBranchRef)
}): Promise<AnalyzeJob> {
  gc();
  const job: AnalyzeJob = {
    id: randomUUID(),
    url: opts.source === "local" ? `local://${opts.localPath}` : (opts.url ?? ""),
    localPath: opts.source === "local" ? opts.localPath : undefined,
    source: opts.source,
    repoName: opts.repoName,
    branchRef: opts.branch ?? opts.branchRef,
    branch: opts.branch ?? (opts.branchRef ? shortBranchName(opts.branchRef) : undefined),
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
  // Every job gets its own scratch dir (viz output, clone target). Local
  // folders are read in place — repoDir points at the user's own folder and
  // is never inside the scratch dir, so the finally-block cleanup can't
  // touch it. The scratch dir must exist even for local jobs, or `cie viz
  // --out` would try to write into a missing directory and the map would
  // silently fail on machines where JOBS_ROOT starts empty.
  //
  // Branch snapshot jobs (local + a branch other than what's on disk) are the
  // one exception: the analysis reads a temporary read-only copy made with
  // `git archive`, extracted into the scratch dir. The user's folder and its
  // .git are still never written to. repoDir ends up the snapshot; sourceRoot
  // stays null, and the UI says the source panel can't open a snapshot's files.
  const workDir = path.join(JOBS_ROOT, job.id, "scratch");
  let repoDir = isLocal ? (job.localPath as string) : path.join(workDir, "repo");
  job.state = "running";

  try {
    await fs.mkdir(workDir, { recursive: true });

    // 0. Engine present?
    const bin = await resolveCieBin();
    if (!bin) {
      markRunningStepFailed(job, engineErrorPlain("bin-missing"), await cieBinMissingTechnical());
      return;
    }

    // 1. clone (web) — verify (local) — or snapshot a local branch
    if (isLocal) {
      setStep(job, "clone", "running", "Checking the folder…");
      const t0 = Date.now();
      try {
        const st = await fs.stat(job.localPath as string);
        if (!st.isDirectory()) {
          markRunningStepFailed(job, "That path points at a file, not a folder. Please give me the project folder.", `not a directory: ${repoDir}`);
          return;
        }
      } catch {
        markRunningStepFailed(job, "We couldn't find that folder on this computer. Double-check the path (and spelling).", `stat failed: ${repoDir}`);
        return;
      }
      if (!job.branchRef) {
        setStep(job, "clone", "done", "Found it — reading right where it lives.", Date.now() - t0);
      } else {
        setStep(job, "clone", "running", `Reading branch ${job.branch}…`);
        // Resolve the picked ref to a commit first — honest failure names the
        // branch instead of git's raw error.
        const rev = await runGit(job.localPath as string, ["rev-parse", "--verify", "--quiet", `${job.branchRef}^{commit}`], 15_000);
        if (rev.code !== 0 || !rev.stdout.trim()) {
          markRunningStepFailed(
            job,
            `There's nothing to read at "${job.branch}" in this repo — the branch may have been deleted. Your folder was not touched.`,
            `git rev-parse ${job.branchRef}^{commit} failed: ${(rev.stderr || rev.stdout).slice(0, 300)}`
          );
          return;
        }
        const pickedSha = rev.stdout.trim();
        // Same commit as the working tree's HEAD? Analyze in place as usual.
        const head = await runGit(job.localPath as string, ["rev-parse", "HEAD"], 15_000);
        const headSha = head.code === 0 ? head.stdout.trim() : "";
        if (pickedSha === headSha) {
          job.branchRef = shortBranchName(job.branchRef);
          setStep(job, "clone", "done", `Found it — "${job.branch}" is what's on disk right now.`, Date.now() - t0);
        } else {
          // Read-only snapshot: git archive exports the branch's committed
          // files; tar unpacks them into our scratch dir. The user's repo is
          // only ever READ here — no checkout, no worktree, nothing written.
          const snapDir = path.join(workDir, "repo");
          await fs.mkdir(snapDir, { recursive: true });
          const tarPath = path.join(workDir, "branch.tar");
          const arch = await runGit(job.localPath as string, ["archive", "--format=tar", "-o", tarPath, job.branchRef], 90_000);
          if (arch.code !== 0) {
            markRunningStepFailed(
              job,
              `We couldn't take a read-only copy of branch "${job.branch}" — nothing in your folder was changed. You can try again or pick another branch.`,
              `git archive ${job.branchRef} exit ${arch.code}: ${(arch.stderr || arch.stdout).slice(0, 300)}`
            );
            return;
          }
          const untar = await run("tar", ["-xf", tarPath, "-C", snapDir], { timeoutMs: 90_000 });
          if (untar.code !== 0) {
            markRunningStepFailed(
              job,
              `We unpacked "${job.branch}" but couldn't open the copy on this server — nothing in your folder was changed. You can try again or pick the branch that's on disk right now.`,
              `tar -xf branch.tar exit ${untar.code}: ${(untar.stderr).slice(0, 300)}`
            );
            return;
          }
          await fs.rm(tarPath, { force: true }).catch(() => {});
          repoDir = snapDir;
          job.viaSnapshot = true;
          setStep(job, "clone", "done", "Got it — a read-only copy of that branch. Your folder was not touched.", Date.now() - t0);
        }
      }
    } else {
      await fs.mkdir(workDir as string, { recursive: true });
      setStep(job, "clone", "running", job.branch ? `Copying branch ${job.branch}…` : "Copying the newest version…");
      const t0 = Date.now();
      const cloneArgs = ["clone", "--depth", "1", "--single-branch"];
      if (job.branchRef) cloneArgs.push("--branch", job.branchRef);
      cloneArgs.push(job.url, repoDir);
      const clone = await run("git", cloneArgs, { timeoutMs: CLONE_TIMEOUT_MS });
      if (clone.code !== 0) {
        markRunningStepFailed(job, cloneErrorPlain(clone.stderr, clone.timedOut, job.branch), `git clone exit ${clone.code}: ${clone.stderr.slice(0, 400)}`);
        return;
      }
      setStep(job, "clone", "done", job.branch ? `Got it — branch ${job.branch}.` : "Got it.", Date.now() - t0);
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

    // One Rust process owns discovery, parsing, analysis and web exports.
    // Structured stage events keep the existing polling UI up to date.
    setStep(job, "read", "running", "Streaming files through the shared ingestion pipeline…");
    const output = path.join(workDir, "artifacts.json");
    let progressBuffer = "";
    const ing = await run(bin, ["ingest-polyglot", repoDir, "--web-out", output], {
      timeoutMs: CIE_TIMEOUT_MS,
      onStderr: (text) => {
        progressBuffer += text;
        const lines = progressBuffer.split("\n");
        progressBuffer = lines.pop() ?? "";
        if (progressBuffer.length > 64 * 1024) progressBuffer = "";
        for (const line of lines) {
          if (!line.startsWith("CIE_STAGE ")) continue;
          try {
            const event = JSON.parse(line.slice(10)) as { key: StepKey; state: StepState; detail?: string };
            if (STEP_ORDER.some((s) => s.key === event.key) && ["running", "done", "skipped", "failed"].includes(event.state)) {
              setStep(job, event.key, event.state, event.detail);
            }
          } catch { /* diagnostic text is not a stage event */ }
        }
      },
    });
    if (ing.timedOut || ing.code !== 0) {
      markRunningStepFailed(job, engineErrorPlain(ing.timedOut ? "timeout" : "crash"), engineTechnical(bin, ing.stderr.slice(-2000)));
      return;
    }
    let artifacts: NonNullable<AnalyzeJob["result"]>;
    try {
      artifacts = JSON.parse(await fs.readFile(output, "utf8")) as NonNullable<AnalyzeJob["result"]>;
      if (!artifacts.report || !Array.isArray(artifacts.report.symbols)) throw new Error("missing core report");
      if (artifacts.polyglotSpecs && !isPolyglotBundle(artifacts.polyglotSpecs)) artifacts.polyglotSpecs = null;
      if (artifacts.stack && !isTechStack(artifacts.stack)) artifacts.stack = null;
    } catch {
      markRunningStepFailed(job, engineErrorPlain("bad-output"), engineTechnical(bin, "ingest-polyglot produced an invalid artifact bundle"));
      return;
    }
    if (artifacts.report.files.length === 0) {
      setStep(job, "read", "failed", UNSUPPORTED_PLAIN);
      job.state = "failed";
      job.error = { plain: UNSUPPORTED_PLAIN, technical: "ingest-polyglot found no supported source files" };
      return;
    }
    let storageRoot = repoDir;
    if (!isLocal || job.viaSnapshot) {
      storageRoot = path.join(JOBS_ROOT, job.id, "storage");
      await storageCall({ op: "retain", root: repoDir, destination: storageRoot });
    }
    job.result = {
      ...artifacts,
      label: isLocal
        ? `Your project: ${job.repoName}${job.branch ? ` (${job.branch})` : ""} (from this computer${job.viaSnapshot ? " as a read-only copy" : ""})`
        : `Your project: ${job.repoName}${job.branch ? ` (${job.branch})` : ""} (from ${hostOf(job.url)})`,
      scan: job.scan ?? null,
      sourceRoot: isLocal && !job.viaSnapshot ? repoDir : null,
      storageRoot,
    };
    job.state = "done";
  } finally {
    // Don't keep clones or scratch files on disk — results live in memory
    // WITH one exception: the finished result itself is persisted once
    // (persistResultToDisk below) so "launch with the last opened
    // workspace" works after a server restart. The scratch workDir is
    // still wiped; result.json lands in its place.
    // Local folders are never touched (read-only analysis in place; the
    // user's folder is never inside workDir).
    await fs.rm(workDir, { recursive: true, force: true }).catch(() => {});
    await persistResultToDisk(job);
  }
}

// ---------- "launch with the last opened workspace" — disk half ----------
//
// The in-memory jobs map is wiped when the server restarts (and gc() prunes
// it after 45 minutes regardless). The browser's workspace record (localStorage,
// lib/workspace-memory.ts) keeps the JOB ID; this keeps the RESULT:
// result.json under the jobs root, written once at job end, rehydrated on
// demand by the result route. Stale copies are swept when a new job finishes.

const PERSISTED_RESULT_TTL_MS = 7 * 24 * 60 * 60_000; // a week — long enough to survive restarts, short enough to stay honest about staleness

async function persistResultToDisk(job: AnalyzeJob): Promise<void> {
  if (!job.result) return;
  const dir = path.join(JOBS_ROOT, job.id);
  try {
    await fs.mkdir(dir, { recursive: true });
    await fs.writeFile(path.join(dir, "result.json"), JSON.stringify(job.result));
    await sweepPersistedResults();
  } catch {
    /* memory-only then — the launch restore falls back to the sample */
  }
}

async function sweepPersistedResults(): Promise<void> {
  try {
    const now = Date.now();
    for (const name of await fs.readdir(JOBS_ROOT)) {
      const dir = path.join(JOBS_ROOT, name);
      const stat = await fs.stat(dir).catch(() => null);
      if (!stat?.isDirectory()) continue;
      const hasResult = await fs.stat(path.join(dir, "result.json")).then(() => true).catch(() => false);
      if (!hasResult) continue; // scratch of a running job — not ours to sweep
      const age = now - (stat.mtimeMs || now);
      if (age > PERSISTED_RESULT_TTL_MS) await fs.rm(dir, { recursive: true, force: true }).catch(() => {});
    }
  } catch { /* the jobs root may not exist yet */ }
}

// Rehydrate a persisted job back into the in-memory map (and answer from it).
// The rebuilt record is minimal on purpose — a restored job is DONE; nobody
// polls its steps, they fetch its result.
export async function rehydratePersistedResult(id: string): Promise<AnalyzeJob | null> {
  if (!/^[a-f0-9-]{8,64}$/i.test(id)) return null; // job ids are UUIDs — id-shaped paths only
  try {
    const raw = await fs.readFile(path.join(JOBS_ROOT, id, "result.json"), "utf8");
    const result = JSON.parse(raw) as AnalyzeJob["result"];
    if (!result || !result.report) return null;
    const stat = await fs.stat(path.join(JOBS_ROOT, id)).catch(() => null);
    const createdAt = stat?.mtimeMs ?? Date.now();
    if (Date.now() - createdAt > PERSISTED_RESULT_TTL_MS) {
      await fs.rm(path.join(JOBS_ROOT, id), { recursive: true, force: true }).catch(() => {});
      return null;
    }
    const job: AnalyzeJob = {
      id,
      url: "local://restored-from-persisted-result",
      source: "local",
      repoName: "",
      state: "done",
      createdAt,
      steps: [],
      scan: result.scan ?? undefined,
      result,
    };
    jobs.set(id, job);
    return job;
  } catch {
    return null;
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
  /** short display branch the job reads, when one was picked */
  branch?: string | null;
  /** true when a local job reads a read-only branch snapshot (not the disk copy) */
  viaSnapshot?: boolean;
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
    branch: job.branch ?? null,
    viaSnapshot: !!job.viaSnapshot,
    scan: job.scan,
    error: job.error,
    permission,
    steps: STEP_ORDER.map((meta0) => {
      let meta = meta0;
      if (meta.key === "clone") {
        if (job.source === "local") {
          meta = job.branchRef ? { ...meta0, ...LOCAL_SNAPSHOT_STEP } : { ...meta0, ...LOCAL_FIRST_STEP };
        } else if (job.branch) {
          meta = { ...meta0, hint: `Copying the newest version of branch ${job.branch} — nothing in it is changed, and the copy is deleted when the check ends.` };
        }
      }
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
