"use client";

// "Use your own" dialog — two ways to bring your project in:
//   1. Connect a repo: paste a public git link; the server clones it and runs
//      the CIE engine in the background while this panel shows every state.
//   2. Upload files: drop in the three JSON files the `cie` CLI exports.
// All words are plain-English on purpose.

import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { useToast } from "@/hooks/use-toast";
import { useCie } from "@/lib/cie-store";
import type { CieReport, CieViewSpec, ConceptTree, CieDeps, CieRuntime, CieCochange, CieOverlap, CieDrift } from "@/lib/cie-types";
import type { RepoScan } from "@/lib/language-profiles";
import { FolderBrowser, ScanCard } from "@/components/cie/repo-scan-card";
import {
  CheckCircle2,
  Circle,
  CircleAlert,
  FileUp,
  FolderOpen,
  GitBranch,
  Globe,
  HardDrive,
  Link2,
  Loader2,
  PartyPopper,
  RotateCcw,
  ChevronDown,
  ShieldCheck,
  HelpCircle,
} from "lucide-react";

// ---------- shared validation helpers (mirrors the server) ----------

function isValidReport(o: unknown): o is CieReport {
  const r = o as CieReport;
  return !!r && Array.isArray(r.files) && Array.isArray(r.findings) && Array.isArray(r.symbols);
}
function isValidViewspec(o: unknown): o is CieViewSpec {
  const v = o as CieViewSpec;
  return !!v && Array.isArray(v.elements) && Array.isArray(v.edges);
}
function isValidConcepts(o: unknown): o is ConceptTree {
  const c = o as ConceptTree;
  return !!c && Array.isArray(c.nodes);
}

// ---------- job status shape (matches /api/analyze/status) ----------

type StepState = "pending" | "running" | "done" | "failed" | "skipped";
interface StepView {
  key: string;
  label: string;
  hint: string;
  state: StepState;
  detail?: string;
  ms?: number;
}
interface JobStatusView {
  id: string;
  state: "queued" | "running" | "awaiting_permission" | "done" | "failed";
  repoName: string;
  scan?: RepoScan | null;
  steps: StepView[];
  error?: { plain: string; technical: string };
  permission?: { plain: string; unreadable: { name: string; files: number }[] } | null;
}

// ---------- mode 1: connect a repo ----------

function StepIcon({ state }: { state: StepState }) {
  if (state === "done") return <CheckCircle2 className="h-5 w-5 text-emerald-600 shrink-0" />;
  if (state === "running") return <Loader2 className="h-5 w-5 text-emerald-700 animate-spin shrink-0" />;
  if (state === "failed") return <CircleAlert className="h-5 w-5 text-rose-600 shrink-0" />;
  if (state === "skipped") return <Circle className="h-5 w-5 text-stone-400 shrink-0" />;
  return <Circle className="h-5 w-5 text-stone-300 shrink-0" />;
}

// A poll can outlive its job: the server may have restarted (the in-memory
// job map is gone), the job expired, or it was evicted to cap the job count.
// When the API says the job is unknown — or stays unreachable — the panel
// fails honestly and stops polling. It must never sit on "working…" with
// the form hidden and no way out.
const LOST_JOB = {
  plain:
    "We lost track of this check — the server was restarted or the job expired. Nothing in your project was changed. Please start over.",
  technical: "status polling: job id no longer known to the server",
};
const LOST_CONNECTION = {
  plain:
    "We couldn't reach the server after several tries, so we stopped watching this check. Nothing in your project was changed — please start over and check again.",
  technical: "status polling failed repeatedly (server unreachable)",
};

function failWhereItStood(prev: JobStatusView, err: { plain: string; technical: string }): JobStatusView {
  if (prev.state === "failed" && prev.error) return prev;
  return {
    ...prev,
    state: "failed",
    error: err,
    steps: prev.steps.map((s) =>
      s.state === "running" ? { ...s, state: "failed" as StepState, detail: err.plain } : s
    ),
  };
}

function ConnectRepo() {
  const { setCustom, jobId, setJobId } = useCie();
  const { toast } = useToast();
  const [kind, setKind] = useState<"web" | "local">("web");
  const [url, setUrl] = useState("");
  const [localPath, setLocalPath] = useState("");
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState("");
  const [status, setStatus] = useState<JobStatusView | null>(null);
  const [technicalOpen, setTechnicalOpen] = useState(false);
  const [loadingResult, setLoadingResult] = useState(false);
  // folder browsing + the pre-flight language peek
  const [browsing, setBrowsing] = useState(false);
  const [scan, setScan] = useState<RepoScan | null>(null);
  const [scanBusy, setScanBusy] = useState(false);
  const [scanError, setScanError] = useState("");
  const scanReq = useRef(0);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const pollMisses = useRef(0);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  // Fetch the JSON exports and load them into the dashboard.
  const showProject = async (id: string) => {
    setLoadingResult(true);
    try {
      const res = await fetch(`/api/analyze/result?job=${encodeURIComponent(id)}`);
      const data = (await res.json().catch(() => ({}))) as {
        report?: CieReport;
        viewspec?: CieViewSpec | null;
        concepts?: ConceptTree | null;
        deps?: CieDeps | null;
        runtime?: CieRuntime | null;
        cochange?: CieCochange | null;
        overlap?: CieOverlap | null;
        drift?: CieDrift | null;
        label?: string;
        scan?: RepoScan | null;
        sourceRoot?: string | null;
        error?: string;
      };
      if (!res.ok || !isValidReport(data.report ?? null)) {
        toast({ title: "Almost there", description: data.error ?? "We couldn't fetch your results. Please try again.", variant: "destructive" });
        return;
      }
      const viewspec = data.viewspec && isValidViewspec(data.viewspec) ? data.viewspec : null;
      const concepts = data.concepts && isValidConcepts(data.concepts) ? data.concepts : null;
      const deps = data.deps && data.deps.table ? data.deps : null;
      const runtime = data.runtime && data.runtime.datasets ? data.runtime : null;
      setCustom(data.report ?? null, viewspec, concepts, data.label ?? "Your project", {
        repoScan: data.scan ?? null,
        deps,
        runtime,
        cochange: data.cochange ?? null,
        overlap: data.overlap ?? null,
        drift: data.drift ?? null,
        sourceRoot: data.sourceRoot ?? null,
      });
      setJobId(null);
      setStatus(null);
      setUrl("");
      toast({ title: "Your project is ready", description: "Every page below now shows your code — and Ask CIE knows it too." });
    } catch {
      toast({ title: "Connection problem", description: "We couldn't fetch your results. Please try again.", variant: "destructive" });
    } finally {
      setLoadingResult(false);
    }
  };

  // Resume an in-flight job after the dialog was closed and reopened.
  useEffect(() => {
    if (!jobId || status) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(`/api/analyze/status?job=${encodeURIComponent(jobId)}`);
        if (!cancelled && res.status === 404) {
          // The job is long gone (server restart / expiry / eviction) — clear
          // the stale id so the form comes back instead of re-probing a dead
          // job on every open.
          setJobId(null);
          return;
        }
        if (!cancelled && res.ok) {
          const data = (await res.json().catch(() => ({}))) as { status?: JobStatusView };
          if (data.status) setStatus(data.status);
        }
      } catch {
        /* the poll loop below will retry once the panel is showing */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [jobId, status, setJobId]);

  // Poll while the job is alive — including while it waits for the
  // ask-before-skipping answer, so the moment it resumes (or stops) the
  // panel follows without a stale snapshot. A dead job or an unreachable
  // server ends in an honest failure after a few tries — never a forever-spin.
  useEffect(() => {
    stopPolling();
    if (
      !jobId ||
      !status ||
      !(status.state === "queued" || status.state === "running" || status.state === "awaiting_permission")
    )
      return;
    pollRef.current = setInterval(async () => {
      try {
        const res = await fetch(`/api/analyze/status?job=${encodeURIComponent(jobId)}`);
        if (res.status === 404) {
          // The API says this job is unknown — mirror it in the panel.
          stopPolling();
          setStatus((prev) => (prev ? failWhereItStood(prev, LOST_JOB) : prev));
          return;
        }
        if (!res.ok) throw new Error(`status ${res.status}`);
        pollMisses.current = 0;
        const data = (await res.json().catch(() => ({}))) as { status?: JobStatusView };
        if (data.status) setStatus(data.status);
      } catch {
        // Transient hiccups are fine; several in a row mean the server is
        // genuinely unreachable — stop and say so.
        pollMisses.current += 1;
        if (pollMisses.current >= 5) {
          stopPolling();
          setStatus((prev) => (prev ? failWhereItStood(prev, LOST_CONNECTION) : prev));
        }
      }
    }, 1200);
    return stopPolling;
  }, [jobId, status, stopPolling]);

  useEffect(() => stopPolling, [stopPolling]);

  const start = async () => {
    setStartError("");
    setStarting(true);
    try {
      const payload = kind === "web" ? { repoUrl: url } : { localPath };
      const res = await fetch("/api/analyze", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      const data = (await res.json().catch(() => ({}))) as { jobId?: string; status?: JobStatusView; error?: string };
      if (!res.ok || !data.jobId) {
        setStartError(data.error ?? "We couldn't start the check. Please try again.");
        return;
      }
      setJobId(data.jobId);
      pollMisses.current = 0;
      setStatus(data.status ?? null);
    } catch {
      setStartError("We couldn't reach the server. Please try again.");
    } finally {
      setStarting(false);
    }
  };

  const resetJob = () => {
    stopPolling();
    pollMisses.current = 0;
    setJobId(null);
    setStatus(null);
    setStartError("");
    setTechnicalOpen(false);
  };

  // Answer the ask-before-skipping question. Yes → the check continues
  // (unread files stay honestly skipped). No → it stops; nothing was read.
  const [deciding, setDeciding] = useState(false);
  const answerPermission = async (proceed: boolean) => {
    if (!status) return;
    setDeciding(true);
    try {
      const res = await fetch("/api/analyze/consent", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ job: status.id, proceed }),
      });
      const data = (await res.json().catch(() => ({}))) as { status?: JobStatusView; error?: string };
      if (data.status) setStatus(data.status);
      else if (data.error) toast({ title: "Hmm", description: data.error, variant: "destructive" });
    } catch {
      toast({ title: "Connection problem", description: "We couldn't send your answer. Please try again.", variant: "destructive" });
    } finally {
      setDeciding(false);
    }
  };

  // Peek inside a local folder: languages used, which CIE can read, which it
  // can't yet. Runs whenever the typed path settles, and after a Browse pick.
  const peekInside = useCallback(async (path: string) => {
    const mine = ++scanReq.current;
    setScanBusy(true);
    setScanError("");
    try {
      const res = await fetch("/api/repo/scan", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ localPath: path }),
      });
      const data = (await res.json().catch(() => ({}))) as { scan?: RepoScan; error?: string };
      if (mine !== scanReq.current) return;
      if (res.ok && data.scan) {
        setScan(data.scan);
        setScanError("");
      } else {
        setScan(null);
        setScanError(data.error ?? "we couldn't look inside that folder.");
      }
    } catch {
      if (mine === scanReq.current) {
        setScan(null);
        setScanError("we couldn't reach the server.");
      }
    } finally {
      if (mine === scanReq.current) setScanBusy(false);
    }
  }, []);

  // Debounced auto-peek for hand-typed paths (Browse picks call peekInside
  // directly so the card appears instantly).
  useEffect(() => {
    if (kind !== "local") return;
    const p = localPath.trim();
    if (!p.startsWith("/")) {
      scanReq.current += 1; // invalidate any in-flight peek
      setScan(null);
      setScanError("");
      setScanBusy(false);
      return;
    }
    const t = setTimeout(() => void peekInside(p), 500);
    return () => clearTimeout(t);
  }, [localPath, kind, peekInside]);

  const active = status && (status.state === "queued" || status.state === "running");
  const asking = status?.state === "awaiting_permission";
  const finished = status?.state === "done";
  const failed = status?.state === "failed";
  const doneCount = status ? status.steps.filter((s) => s.state === "done").length : 0;
  const pct = status ? (failed ? 100 : Math.round(((doneCount + (finished ? 1 : 0)) / (status.steps.length + 1)) * 100)) : 0;

  return (
    <div className="space-y-4">
      {/* input form — hidden once a job is in flight */}
      {!status && (
        <>
          {/* where does the code live? */}
          <div className="grid grid-cols-2 gap-1 rounded-xl bg-stone-100 p-1" role="tablist" aria-label="Where is your code?">
            <button
              type="button"
              role="tab"
              aria-selected={kind === "web"}
              onClick={() => setKind("web")}
              className={`flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors ${kind === "web" ? "bg-white text-stone-900 shadow-sm" : "text-stone-500 hover:text-stone-700"}`}
            >
              <Globe className="h-3.5 w-3.5" /> On the web
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={kind === "local"}
              onClick={() => setKind("local")}
              className={`flex items-center justify-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition-colors ${kind === "local" ? "bg-white text-stone-900 shadow-sm" : "text-stone-500 hover:text-stone-700"}`}
            >
              <HardDrive className="h-3.5 w-3.5" /> On this computer
            </button>
          </div>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              void start();
            }}
            className="space-y-2"
          >
            <label htmlFor="repo-url" className="text-sm font-medium text-stone-800 flex items-center gap-2">
              {kind === "web" ? (
                <><Link2 className="h-4 w-4 text-emerald-700" /> Paste a link to your repo</>
              ) : (
                <><HardDrive className="h-4 w-4 text-emerald-700" /> Pick your project&apos;s folder</>
              )}
            </label>
            <div className="flex flex-col sm:flex-row gap-2">
              <input
                id="repo-url"
                type={kind === "web" ? "url" : "text"}
                value={kind === "web" ? url : localPath}
                onChange={(e) => (kind === "web" ? setUrl(e.target.value) : setLocalPath(e.target.value))}
                placeholder={kind === "web" ? "https://github.com/you/your-repo" : "/home/you/my-project"}
                disabled={starting}
                className="flex-1 h-11 rounded-xl border border-stone-200 bg-white px-4 text-sm text-stone-900 placeholder:text-stone-400 focus:outline-none focus:ring-2 focus:ring-emerald-600/30 focus:border-emerald-300 disabled:opacity-60"
              />
              {kind === "local" && (
                <Button
                  type="button"
                  variant="outline"
                  disabled={starting}
                  onClick={() => setBrowsing((o) => !o)}
                  className="h-11 rounded-xl px-4 border-stone-300 text-stone-700 hover:bg-stone-50"
                  aria-expanded={browsing}
                >
                  <FolderOpen className="h-4 w-4 mr-1.5 text-emerald-700" /> Browse folders…
                </Button>
              )}
              <Button
                type="submit"
                disabled={starting || (kind === "web" ? !url.trim() : !localPath.trim())}
                className="h-11 bg-emerald-700 hover:bg-emerald-800 text-white rounded-xl px-5"
              >
                {starting ? <Loader2 className="h-4 w-4 animate-spin mr-1.5" /> : <GitBranch className="h-4 w-4 mr-1.5" />}
                {starting ? "Starting…" : "Check my project"}
              </Button>
            </div>
            {startError && <p className="text-sm text-rose-700 bg-rose-50 border border-rose-200 rounded-lg px-3 py-2">{startError}</p>}
          </form>

          {/* folder browser (local mode only) */}
          {kind === "local" && browsing && (
            <FolderBrowser
              onPick={(p) => {
                setLocalPath(p);
                setBrowsing(false);
                void peekInside(p);
              }}
              onClose={() => setBrowsing(false)}
            />
          )}

          {/* the honest "what's in this folder" peek (local mode only) */}
          {kind === "local" && (scan || scanBusy || scanError) && (
            <ScanCard scan={scan} busy={scanBusy} error={scanError} />
          )}
          <ul className="text-xs text-stone-500 space-y-1.5">
            {kind === "web" ? (
              <>
                <li className="flex items-start gap-2"><Globe className="h-3.5 w-3.5 mt-0.5 shrink-0" /><span className="flex-1 text-left">Works with public GitHub, GitLab and Bitbucket links.</span></li>
                <li className="flex items-start gap-2"><ShieldCheck className="h-3.5 w-3.5 mt-0.5 shrink-0" /><span className="flex-1 text-left">We only ever <span className="font-medium">read</span> your code — nothing is changed, and your repo is deleted from our disk right after.</span></li>
                <li className="flex items-start gap-2"><FileUp className="h-3.5 w-3.5 mt-0.5 shrink-0" /><span className="flex-1 text-left">Private repo? Use the <span className="font-medium">Upload files</span> option instead — it never leaves your machine.</span></li>
              </>
            ) : (
              <>
                <li className="flex items-start gap-2"><FolderOpen className="h-3.5 w-3.5 mt-0.5 shrink-0" /><span className="flex-1 text-left">Tap <span className="font-medium">Browse folders…</span> to find your project — or type the full path, e.g. <span className="font-mono">/home/you/my-project</span>. Git repos work best.</span></li>
                <li className="flex items-start gap-2"><ShieldCheck className="h-3.5 w-3.5 mt-0.5 shrink-0" /><span className="flex-1 text-left">Nothing is copied or uploaded — CIE reads the folder <span className="font-medium">right where it lives</span>, read-only.</span></li>
                <li className="flex items-start gap-2"><Globe className="h-3.5 w-3.5 mt-0.5 shrink-0" /><span className="flex-1 text-left">Before checking, CIE peeks inside and tells you which languages it can read here — and which it can&apos;t yet.</span></li>
              </>
            )}
          </ul>
        </>
      )}

      {/* live status panel */}
      {status && (
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <p className="text-sm font-semibold text-stone-900 flex items-center gap-2">
              <GitBranch className="h-4 w-4 text-emerald-700" /> {status.repoName}
            </p>
            {!active && (
              <Button variant="ghost" size="sm" onClick={resetJob} className="text-stone-500 h-8">
                <RotateCcw className="h-3.5 w-3.5 mr-1" /> Start over
              </Button>
            )}
          </div>

          {/* progress bar */}
          <div className="h-2.5 rounded-full bg-stone-100 overflow-hidden" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
            <div
              className={`h-full rounded-full transition-all duration-500 ${failed ? "bg-rose-500" : finished ? "bg-emerald-600" : "bg-emerald-500"}`}
              style={{ width: `${Math.max(6, pct)}%` }}
            />
          </div>

          {/* steps */}
          <ol className="space-y-1" aria-live="polite">
            {status.steps.map((s, i) => (
              <li key={s.key} className={`flex items-start gap-3 rounded-xl px-3 py-2.5 ${s.state === "running" ? "bg-emerald-50/70 border border-emerald-100" : s.state === "failed" ? "bg-rose-50/70 border border-rose-100" : ""}`}>
                <StepIcon state={s.state} />
                <div className="min-w-0 flex-1">
                  <p className={`text-sm leading-tight ${s.state === "pending" ? "text-stone-400" : "font-medium text-stone-900"}`}>
                    <span className="text-stone-400 mr-1.5">{i + 1}.</span>
                    {s.label}
                    {s.state === "running" && <span className="text-emerald-700 font-normal"> — working…</span>}
                    {s.ms !== undefined && s.state === "done" && <span className="text-stone-400 font-normal text-xs"> · {(s.ms / 1000).toFixed(1)}s</span>}
                  </p>
                  {(s.state !== "pending" && s.detail) && (
                    <p className={`text-xs mt-0.5 leading-relaxed ${s.state === "failed" ? "text-rose-700 font-medium" : "text-stone-500"}`}>{s.detail}</p>
                  )}
                  {s.state === "pending" && <p className="text-xs text-stone-400 mt-0.5 leading-relaxed">{s.hint}</p>}
                </div>
              </li>
            ))}
          </ol>

          {/* ask before skipping — the user decides, nothing read yet */}
          {asking && status.permission && (
            <div className="rounded-xl border border-amber-300 bg-amber-50 p-4 space-y-3" role="alertdialog" aria-label="CIE can't read some files here">
              <p className="text-sm font-semibold text-amber-900 flex items-center gap-2">
                <HelpCircle className="h-4 w-4" /> Before CIE reads anything — it can&apos;t read some of your files
              </p>
              <p className="text-sm text-amber-800 leading-relaxed">{status.permission.plain}</p>
              <div className="flex flex-wrap gap-1.5">
                {status.permission.unreadable.map((l) => (
                  <span key={l.name} className="inline-flex items-center rounded-full bg-white border border-amber-300 text-amber-900 px-2.5 py-1 text-xs font-medium">
                    {l.name} · {l.files} file{l.files === 1 ? "" : "s"}
                  </span>
                ))}
              </div>
              <p className="text-xs text-amber-700 leading-relaxed">
                Nothing has been read yet. If you go ahead, those files stay out of every page — named, never guessed about.
              </p>
              <div className="flex flex-wrap gap-2 pt-0.5">
                <Button onClick={() => void answerPermission(true)} disabled={deciding} className="bg-emerald-700 hover:bg-emerald-800 text-white h-9">
                  {deciding ? <Loader2 className="h-4 w-4 animate-spin mr-1.5" /> : null}
                  Yes — read the rest
                </Button>
                <Button onClick={() => void answerPermission(false)} disabled={deciding} variant="outline" className="border-amber-400 text-amber-900 hover:bg-amber-100 h-9">
                  No — stop here
                </Button>
              </div>
            </div>
          )}

          {/* failure card — shown even if the error object itself went
              missing, so "Try again" is always reachable after a failure */}
          {failed && (
            <div className="rounded-xl border border-rose-200 bg-rose-50 p-4 space-y-2">
              <p className="text-sm font-semibold text-rose-900 flex items-center gap-2">
                <CircleAlert className="h-4 w-4" /> That didn&apos;t work
              </p>
              <p className="text-sm text-rose-800 leading-relaxed">
                {status.error?.plain ?? "The check stopped unexpectedly. Nothing in your project was changed — try again."}
              </p>
              {status.scan && (
                <ScanCard scan={status.scan} compact />
              )}
              {status.error?.technical && (
                <>
                  <button
                    type="button"
                    onClick={() => setTechnicalOpen((o) => !o)}
                    className="text-xs text-rose-700/80 hover:text-rose-900 flex items-center gap-1"
                  >
                    <ChevronDown className={`h-3 w-3 transition-transform ${technicalOpen ? "rotate-180" : ""}`} />
                    {technicalOpen ? "Hide" : "Show"} technical details
                  </button>
                  {technicalOpen && (
                    <pre className="text-[10px] leading-relaxed text-rose-900/70 bg-white/70 rounded-lg p-2 overflow-x-auto whitespace-pre-wrap break-all">
                      {status.error.technical}
                    </pre>
                  )}
                </>
              )}
              <Button onClick={resetJob} className="bg-rose-700 hover:bg-rose-800 text-white h-9">
                <RotateCcw className="h-4 w-4 mr-1.5" /> Try again
              </Button>
            </div>
          )}

          {/* success card */}
          {finished && (
            <div className="rounded-xl border border-emerald-200 bg-emerald-50 p-4 space-y-3">
              <p className="text-sm font-semibold text-emerald-900 flex items-center gap-2">
                <PartyPopper className="h-4 w-4" /> All done — your project is ready
              </p>
              <Button onClick={() => void showProject(status.id)} disabled={loadingResult} className="bg-emerald-700 hover:bg-emerald-800 text-white h-10 w-full sm:w-auto">
                {loadingResult ? <Loader2 className="h-4 w-4 animate-spin mr-1.5" /> : null}
                {loadingResult ? "Opening your project…" : "Show my project"}
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---------- mode 2: upload files (the original flow) ----------

function UploadFiles() {
  const { setCustom, reset } = useCie();
  const { toast } = useToast();
  const reportRef = useRef<HTMLInputElement>(null);
  const vizRef = useRef<HTMLInputElement>(null);
  const conceptsRef = useRef<HTMLInputElement>(null);
  const [got, setGot] = useState({ report: false, viewspec: false, concepts: false });
  const [parsed, setParsed] = useState<{ report: CieReport | null; viewspec: CieViewSpec | null; concepts: ConceptTree | null; name: string }>({
    report: null,
    viewspec: null,
    concepts: null,
    name: "",
  });

  const read = (file: File, slot: "report" | "viewspec" | "concepts") => {
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const obj = JSON.parse(String(reader.result));
        if (slot === "report" && isValidReport(obj)) {
          setParsed((p) => ({ ...p, report: obj, name: file.name }));
          setGot((g) => ({ ...g, report: true }));
        } else if (slot === "viewspec" && isValidViewspec(obj)) {
          setParsed((p) => ({ ...p, viewspec: obj, name: file.name }));
          setGot((g) => ({ ...g, viewspec: true }));
        } else if (slot === "concepts" && isValidConcepts(obj)) {
          setParsed((p) => ({ ...p, concepts: obj, name: file.name }));
          setGot((g) => ({ ...g, concepts: true }));
        } else {
          toast({ title: "That file doesn't look right", description: `Please choose the ${slot} file CIE created for you.`, variant: "destructive" });
        }
      } catch {
        toast({ title: "Couldn't read that file", description: "Please choose a file that ends in .json", variant: "destructive" });
      }
    };
    reader.readAsText(file);
  };

  const apply = () => {
    if (!parsed.report && !parsed.viewspec) {
      toast({ title: "Almost there", description: "Please add at least the report file (report.json).", variant: "destructive" });
      return;
    }
    setCustom(parsed.report, parsed.viewspec, parsed.concepts, parsed.name ? `Your data: ${parsed.name}` : "Your data");
    toast({ title: "Your data is loaded", description: "Everything below now shows your project." });
  };

  const slot = (label: string, required: boolean, filled: boolean, ref: React.RefObject<HTMLInputElement | null>, key: "report" | "viewspec" | "concepts") => (
    <button
      type="button"
      onClick={() => ref.current?.click()}
      className={`w-full text-left rounded-lg border p-4 flex items-center gap-3 transition-colors ${filled ? "border-emerald-300 bg-emerald-50" : "border-stone-200 bg-white hover:border-stone-300"}`}
    >
      {filled ? <CheckCircle2 className="h-5 w-5 text-emerald-700 shrink-0" /> : <FileUp className="h-5 w-5 text-stone-400 shrink-0" />}
      <span className="flex-1">
        <span className="block text-sm font-medium text-stone-900">
          {label} {required && <span className="text-rose-600">*</span>}
        </span>
        <span className="block text-xs text-stone-500">{filled ? "Ready" : "Tap to choose a file"}</span>
      </span>
      <input ref={ref} type="file" accept=".json,application/json" className="hidden" onChange={(e) => { const f = e.target.files?.[0]; if (f) read(f, key); }} />
    </button>
  );

  return (
    <div className="space-y-3">
      <p className="text-sm text-stone-600 leading-relaxed">
        Run these two commands in your project folder, then add the files they create:
        <span className="block mt-2 rounded-md bg-stone-900 text-stone-100 font-mono text-xs px-3 py-2 leading-relaxed">
          cie report . --json &gt; report.json<br />
          cie viz . --out viewspec.json
        </span>
      </p>
      {slot("1. The health report", true, got.report, reportRef, "report")}
      {slot("2. The picture/map file", false, got.viewspec, vizRef, "viewspec")}
      {slot("3. The 'what it does' list", false, got.concepts, conceptsRef, "concepts")}
      <div className="flex gap-2 pt-1">
        <Button onClick={apply} className="flex-1 bg-emerald-700 hover:bg-emerald-800 text-white">
          Show my project
        </Button>
        <Button variant="outline" onClick={() => { reset(); toast({ title: "Back to the sample project" }); }}>
          Use sample
        </Button>
      </div>
    </div>
  );
}

// ---------- the dialog ----------

export function UseYourOwnDialog() {
  const [mode, setMode] = useState<"connect" | "upload">("connect");

  return (
    <div className="space-y-4">
      {/* mode switch */}
      <div className="grid grid-cols-2 gap-1 rounded-xl bg-stone-100 p-1" role="tablist" aria-label="Ways to add your project">
        <button
          type="button"
          role="tab"
          aria-selected={mode === "connect"}
          onClick={() => setMode("connect")}
          className={`flex items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${mode === "connect" ? "bg-white text-stone-900 shadow-sm" : "text-stone-500 hover:text-stone-700"}`}
        >
          <GitBranch className="h-4 w-4" /> Connect a repo
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={mode === "upload"}
          onClick={() => setMode("upload")}
          className={`flex items-center justify-center gap-1.5 rounded-lg px-3 py-2 text-sm font-medium transition-colors ${mode === "upload" ? "bg-white text-stone-900 shadow-sm" : "text-stone-500 hover:text-stone-700"}`}
        >
          <FileUp className="h-4 w-4" /> Upload files
        </button>
      </div>

      {mode === "connect" ? <ConnectRepo /> : <UploadFiles />}
    </div>
  );
}