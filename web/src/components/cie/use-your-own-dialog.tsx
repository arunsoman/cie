"use client";

// "Use your own" dialog — three ways to bring your project in, one clean
// switch, every way supports picking the branch to read:
//   1. A Git repo link — public, on the web; any branch (remote heads are
//      listed live via /api/repo/branches).
//   2. This computer — the folder is read in place, exactly as it is; or, if
//      another branch is picked, as a temporary read-only git snapshot
//      (the folder and its .git are never written to).
//   3. Upload files — the three JSON files the `cie` CLI exports.
// All words are plain-English on purpose; every state says exactly what
// happened and what happens next.

import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { useToast } from "@/hooks/use-toast";
import { useCie } from "@/lib/cie-store";
import type { CieReport, CieViewSpec, ConceptTree, CieDeps, CieRuntime, CieCochange, CieOverlap, CieDrift } from "@/lib/cie-types";
import type { TechStack } from "@/lib/stack-types";
import { isTechStack } from "@/lib/stack-types";
import type { RepoScan } from "@/lib/language-profiles";
import { FolderBrowser, ScanCard } from "@/components/cie/repo-scan-card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Check,
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
  GraduationCap,
  PencilLine,
  RotateCcw,
  ShieldCheck,
  HelpCircle,
  ChevronDown,
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
  /** short display branch the job reads, when one was picked */
  branch?: string | null;
  /** true when a local job reads a read-only branch snapshot (not the disk copy) */
  viaSnapshot?: boolean;
  scan?: RepoScan | null;
  steps: StepView[];
  error?: { plain: string; technical: string };
  permission?: { plain: string; unreadable: { name: string; files: number }[] } | null;
}

// ---------- the branch list shape (matches /api/repo/branches) ----------

interface BranchList {
  source: "local" | "url";
  /** local: does this folder hold a git repo at all? */
  git?: boolean;
  /** local: branch checked out right now (null when detached). */
  current?: string | null;
  /** local: short hash checked out right now, when detached. */
  detachedSha?: string | null;
  /** local: this checkout's own branches ("main", "dev"). */
  local?: string[];
  /** local: remote-tracking branches ("origin/dev"). */
  remote?: string[];
  /** url: branch names on the remote repo. */
  heads?: string[];
  /** plain-words note when the list could not be fully read. */
  note?: string | null;
}

/** refs/remotes/origin/dev → "origin/dev"; refs/heads/dev → "dev". */
function shortBranch(ref: string): string {
  return ref.replace(/^refs\/(heads|tags)\//, "").replace(/^refs\/remotes\//, "");
}

// ---------- the branch picker ----------
//
// One dropdown for both sources. Empty value = the honest default:
//   web   → the repo's default branch (what the link opens with)
//   local → the folder exactly as it sits on disk (uncommitted edits included)
// A non-empty value is a plain branch name (web) or the exact ref the list
// offered (local: refs/heads/… or refs/remotes/…). A failed listing never
// blocks starting — a name can always be typed by hand.

function BranchPicker({
  kind,
  target,
  value,
  onChange,
  onList,
  disabled,
}: {
  kind: "web" | "local";
  target: string;
  value: string;
  onChange: (next: string) => void;
  /** tells the parent what the listing found (or null while resetting) */
  onList?: (list: BranchList | null) => void;
  disabled?: boolean;
}) {
  const [list, setList] = useState<BranchList | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [typing, setTyping] = useState(false);
  const [typed, setTyped] = useState("");
  const req = useRef(0);
  const lastTarget = useRef(target);

  const targetOk = kind === "web" ? /^https?:\/\/\S+/i.test(target) : target.length > 0;

  // A different source means the old pick no longer applies.
  useEffect(() => {
    if (lastTarget.current !== target) {
      lastTarget.current = target;
      if (value) onChange("");
    }
  }, [target, value, onChange]);

  // Read the branch list whenever the source settles (debounced).
  useEffect(() => {
    setList(null);
    setNote(null);
    setBusy(false);
    setTyping(false);
    setTyped("");
    onList?.(null);
    if (!targetOk) return;
    const mine = ++req.current;
    setBusy(true);
    const t = setTimeout(
      () => {
        void (async () => {
          try {
            const res = await fetch("/api/repo/branches", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify(kind === "web" ? { repoUrl: target } : { localPath: target }),
            });
            const data = (await res.json().catch(() => ({}))) as BranchList & { error?: string };
            if (mine !== req.current) return; // a newer source superseded this one
            if (!res.ok) {
              setList(null);
              setNote(data.error ?? "We couldn't read the branch list — you can still type a name by hand.");
              onList?.(null);
            } else {
              setList(data);
              setNote(data.note ?? null);
              onList?.(data);
            }
          } catch {
            if (mine === req.current) setNote("We couldn't reach the server to read the branch list.");
          } finally {
            if (mine === req.current) setBusy(false);
          }
        })();
      },
      kind === "web" ? 700 : 400
    );
    return () => clearTimeout(t);
  }, [target, targetOk, kind, onList]);

  const pick = (next: string) => {
    onChange(next);
    setTyping(false);
    setTyped("");
  };

  const triggerLabel = busy
    ? "Reading branches…"
    : value
      ? shortBranch(value)
      : kind === "web"
        ? "Default branch"
        : list?.current
          ? `On disk right now (${list.current})`
          : list?.detachedSha
            ? `On disk right now (detached @ ${list.detachedSha})`
            : "On disk right now";

  // What a non-default pick means, said plainly.
  const pickNote = value
    ? kind === "web"
      ? "Only that branch's newest commit is copied — read-only, and the copy is deleted when the check ends."
      : (() => {
          const localBranch = value.startsWith("refs/heads/") ? value.slice("refs/heads/".length) : value;
          if (list?.current && !value.startsWith("refs/remotes/") && localBranch === list.current)
            return "That branch is what's on disk right now — CIE reads the folder itself, uncommitted edits included.";
          return "CIE reads a temporary read-only copy of that branch (a git snapshot) — nothing in your folder is touched. The copy is deleted after the check, so the source panel can't open its files.";
        })()
    : kind === "local"
      ? "Reads the folder exactly as it is on disk — uncommitted edits included."
      : "";

  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2">
        <p className="text-sm font-medium text-stone-800 shrink-0 w-14">Branch</p>
        <DropdownMenu>
          <DropdownMenuTrigger asChild disabled={disabled}>
            <button
              type="button"
              aria-haspopup="listbox"
              aria-expanded="false"
              className={`min-w-0 flex-1 h-10 rounded-xl border bg-white px-3 inline-flex items-center gap-2 text-sm transition-colors ${
                disabled ? "opacity-60 cursor-not-allowed" : "hover:border-emerald-300"
              } ${value ? "border-emerald-300 text-emerald-900" : "border-stone-200 text-stone-700"}`}
            >
              <GitBranch className="h-4 w-4 text-stone-400 shrink-0" aria-hidden />
              <span className="truncate">{triggerLabel}</span>
              <ChevronDown className="ml-auto h-3.5 w-3.5 text-stone-400 shrink-0" aria-hidden />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" sideOffset={4} className="w-[300px] max-h-72 overflow-y-auto">
            {busy && (
              <p className="flex items-center gap-2 px-2 py-3 text-sm text-stone-500">
                <Loader2 className="h-3.5 w-3.5 animate-spin" /> Reading the branch list…
              </p>
            )}
            {!busy && (
              <>
                {note && (
                  <p className="px-2 py-2 text-xs text-amber-800 bg-amber-50 leading-relaxed rounded-md">
                    {note}
                  </p>
                )}
                {list?.source === "local" && list.git === false && (
                  <p className="px-2 py-2.5 text-xs text-stone-500 leading-relaxed">
                    This folder isn&apos;t a git repo, so there are no branches to pick — CIE reads these files
                    exactly as they are.
                  </p>
                )}
                {/* the honest default */}
                <DropdownMenuItem onClick={() => pick("")} className={value === "" ? "bg-emerald-50" : ""}>
                  <span className="min-w-0 flex-1 truncate">
                    {kind === "web"
                      ? "Default branch — what the repo opens with"
                      : `On disk right now${list?.current ? ` (${list.current})` : list?.detachedSha ? ` (detached @ ${list.detachedSha})` : ""}`}
                  </span>
                  {value === "" && <Check className="ml-auto h-3.5 w-3.5 text-emerald-700 shrink-0" />}
                </DropdownMenuItem>

                {/* web: branches on the remote repo */}
                {kind === "web" && list?.heads && list.heads.length > 0 && (
                  <>
                    <DropdownMenuSeparator />
                    <DropdownMenuLabel>Branches on that repo</DropdownMenuLabel>
                    {list.heads.map((b) => (
                      <DropdownMenuItem key={b} onClick={() => pick(b)} className={value === b ? "bg-emerald-50" : ""}>
                        <span className="min-w-0 flex-1 truncate font-mono text-xs">{b}</span>
                        {value === b && <Check className="ml-auto h-3.5 w-3.5 text-emerald-700 shrink-0" />}
                      </DropdownMenuItem>
                    ))}
                    {list.heads.length >= 200 && <p className="px-2 py-1 text-xs text-stone-400">Showing the first 200 — you can still type any branch by hand.</p>}
                  </>
                )}

                {/* local: this checkout's own branches + remote-tracking ones */}
                {kind === "local" && (list?.local?.length || list?.remote?.length ? true : false) && (
                  <>
                    {list?.local && list.local.length > 0 && (
                      <>
                        <DropdownMenuSeparator />
                        <DropdownMenuLabel>Branches in this copy</DropdownMenuLabel>
                        {list.local.map((b) => {
                          const v = `refs/heads/${b}`;
                          const inUse = list.current === b;
                          return (
                            <DropdownMenuItem key={v} onClick={() => pick(v)} className={value === v ? "bg-emerald-50" : ""}>
                              <span className="min-w-0 flex-1 truncate font-mono text-xs">
                                {b}
                                {inUse && <span className="font-sans text-stone-400"> — checked out</span>}
                              </span>
                              {value === v && <Check className="ml-auto h-3.5 w-3.5 text-emerald-700 shrink-0" />}
                            </DropdownMenuItem>
                          );
                        })}
                      </>
                    )}
                    {list?.remote && list.remote.length > 0 && (
                      <>
                        <DropdownMenuSeparator />
                        <DropdownMenuLabel>Remote-tracking branches</DropdownMenuLabel>
                        {list.remote.map((b) => {
                          const v = `refs/remotes/${b}`;
                          return (
                            <DropdownMenuItem key={v} onClick={() => pick(v)} className={value === v ? "bg-emerald-50" : ""}>
                              <span className="min-w-0 flex-1 truncate font-mono text-xs">{b}</span>
                              {value === v && <Check className="ml-auto h-3.5 w-3.5 text-emerald-700 shrink-0" />}
                            </DropdownMenuItem>
                          );
                        })}
                      </>
                    )}
                  </>
                )}

                {/* typing always possible — a listing must never block a start */}
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => setTyping(true)}>
                  <PencilLine className="h-3.5 w-3.5 text-stone-400" /> Type a branch name instead…
                </DropdownMenuItem>
              </>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {/* manual entry — for private repos, huge repos, or any listing failure */}
      {typing && (
        <div className="flex gap-1.5 sm:pl-16">
          <input
            autoFocus
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={kind === "web" ? "e.g. main, dev, v1.2" : "e.g. dev or origin/dev"}
            onKeyDown={(e) => {
              if (e.key === "Enter" && typed.trim()) pick(typed.trim());
              if (e.key === "Escape") setTyping(false);
            }}
            className="flex-1 h-9 rounded-lg border border-stone-200 bg-white px-3 text-sm focus:outline-none focus:ring-2 focus:ring-emerald-600/30 focus:border-emerald-300"
          />
          <Button
            type="button"
            size="sm"
            disabled={!typed.trim()}
            onClick={() => typed.trim() && pick(typed.trim())}
            className="h-9 bg-emerald-700 hover:bg-emerald-800 text-white px-3"
          >
            Use
          </Button>
          <Button type="button" size="sm" variant="ghost" onClick={() => setTyping(false)} className="h-9 text-stone-500">
            Cancel
          </Button>
        </div>
      )}

      {pickNote && <p className="text-xs text-stone-500 leading-relaxed sm:pl-16">{pickNote}</p>}
    </div>
  );
}

// ---------- mode 1: connect a repo (web or local) ----------

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

function ConnectRepo({ kind, onClosed }: { kind: "web" | "local"; onClosed?: (open: boolean) => void }) {
  const { setCustom, jobId, setJobId } = useCie();
  const { toast } = useToast();
  const [url, setUrl] = useState("");
  const [localPath, setLocalPath] = useState("");
  /** "" = the honest default; else a branch name (web) / ref (local) */
  const [branch, setBranch] = useState("");
  /** what the branch picker last found (local mode) — powers the
   *  preview-vs-branch caption under the scan card */
  const [branchList, setBranchList] = useState<BranchList | null>(null);
  const rememberBranchList = useCallback((l: BranchList | null) => setBranchList(l), []);
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

  // switching sources invalidates any picked branch
  useEffect(() => {
    setBranch("");
    setBranchList(null);
    setStartError("");
    setTechnicalOpen(false);
  }, [kind]);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  // Fetch the JSON exports and load them into the dashboard. `dest`
  // decides what happens after: default stays on the success card;
  // "master" closes the dialog straight into the journey view.
  const showProject = async (id: string, dest: "explore" | "master" = "explore") => {
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
        stack?: TechStack | null;
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
        stack: data.stack && isTechStack(data.stack) ? data.stack : null,
        sourceRoot: data.sourceRoot ?? null,
      });
      setJobId(null);
      setStatus(null);
      setUrl("");
      setBranch("");
      toast({ title: "Your project is ready", description: "Every page below now shows your code — and Ask CIE knows it too." });
      if (dest === "master") {
        onClosed?.(false);
        useCie.getState().setView("mastery");
      }
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
      const payload = kind === "web" ? { repoUrl: url, branch: branch || undefined } : { localPath, branch: branch || undefined };
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
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void start();
          }}
          className="space-y-3"
        >
          {/* the source row */}
          <label htmlFor="repo-source" className="text-sm font-medium text-stone-800 flex items-center gap-2">
            {kind === "web" ? (
              <><Link2 className="h-4 w-4 text-emerald-700" /> Paste a link to your public repo</>
            ) : (
              <><HardDrive className="h-4 w-4 text-emerald-700" /> Point CIE at your project&apos;s folder</>
            )}
          </label>
          <div className="flex flex-col sm:flex-row gap-2">
            <input
              id="repo-source"
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
          </div>

          {/* the branch that gets read — remote heads (web) or this repo's refs (local) */}
          <BranchPicker
            kind={kind}
            target={kind === "web" ? url : localPath}
            value={branch}
            onChange={setBranch}
            onList={rememberBranchList}
            disabled={starting}
          />

          <Button
            type="submit"
            disabled={starting || (kind === "web" ? !url.trim() : !localPath.trim())}
            className="w-full h-11 bg-emerald-700 hover:bg-emerald-800 text-white rounded-xl"
          >
            {starting ? <Loader2 className="h-4 w-4 animate-spin mr-1.5" /> : <GitBranch className="h-4 w-4 mr-1.5" />}
            {starting ? "Starting…" : branch ? `Check ${kind === "web" ? "that repo's" : "the folder's"} branch: ${shortBranch(branch)}` : "Check my project"}
          </Button>

          {startError && <p className="text-sm text-rose-700 bg-rose-50 border border-rose-200 rounded-lg px-3 py-2">{startError}</p>}
        </form>
      )}

      {/* folder browser (local mode only) */}
      {kind === "local" && !status && browsing && (
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
      {kind === "local" && !status && (scan || scanBusy || scanError) && (
        <>
          <ScanCard scan={scan} busy={scanBusy} error={scanError} />
          {/* the preview walks the working tree; a picked branch doesn't have to
              be it — say so, so "Python 1 here" never reads as a contradiction */}
          {scan && branch && branchList?.current && !branch.startsWith("refs/remotes/") && shortBranch(branch) !== branchList.current && (
            <p className="text-xs text-stone-500 leading-relaxed -mt-2">
              This preview is the folder as it sits on disk (checked out: <span className="font-medium">{branchList.current}</span>). The check itself reads branch <span className="font-medium">{shortBranch(branch)}</span>&apos;s own copy, and re-inventories it during the run.
            </p>
          )}
        </>
      )}

      {/* privacy lines — short, honest */}
      {!status && (
        <ul className="text-xs text-stone-500 space-y-1.5 border-t border-stone-100 pt-3">
          {kind === "web" ? (
            <>
              <li className="flex items-start gap-2">
                <ShieldCheck className="h-3.5 w-3.5 mt-0.5 shrink-0 text-emerald-700" />
                <span className="flex-1 text-left">Read-only: your repo is copied, checked, and the copy is deleted straight after — nothing of yours is changed.</span>
              </li>
              <li className="flex items-start gap-2">
                <FileUp className="h-3.5 w-3.5 mt-0.5 shrink-0" />
                <span className="flex-1 text-left">Private repo? The <span className="font-medium">Upload files</span> tab never sends a byte.</span>
              </li>
            </>
          ) : (
            <>
              <li className="flex items-start gap-2">
                <ShieldCheck className="h-3.5 w-3.5 mt-0.5 shrink-0 text-emerald-700" />
                <span className="flex-1 text-left">Nothing leaves this computer — CIE reads the folder where it lives, read-only.</span>
              </li>
              <li className="flex items-start gap-2">
                <FolderOpen className="h-3.5 w-3.5 mt-0.5 shrink-0" />
                <span className="flex-1 text-left">Tap <span className="font-medium">Browse folders…</span> or type the full path, e.g. <span className="font-mono">/home/you/my-project</span>.</span>
              </li>
            </>
          )}
        </ul>
      )}

      {/* live status panel */}
      {status && (
        <div className="space-y-3">
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <p className="text-sm font-semibold text-stone-900 flex items-center gap-2 min-w-0">
              <GitBranch className="h-4 w-4 text-emerald-700 shrink-0" />
              <span className="truncate">{status.repoName}</span>
              {status.branch && (
                <span className="inline-flex items-center rounded-full bg-stone-100 border border-stone-200 px-2 py-0.5 text-xs font-medium text-stone-700 shrink-0">
                  {status.branch}
                </span>
              )}
              {status.viaSnapshot && (
                <span className="inline-flex items-center rounded-full bg-amber-50 border border-amber-200 px-2 py-0.5 text-xs font-medium text-amber-900 shrink-0">
                  read-only copy
                </span>
              )}
            </p>
            {!active && (
              <Button variant="ghost" size="sm" onClick={resetJob} className="text-stone-500 h-8">
                <RotateCcw className="h-3.5 w-3.5 mr-1" /> Start over
              </Button>
            )}
          </div>
          {active && status.viaSnapshot && (
            <p className="text-xs text-stone-500">CIE is reading a read-only copy of branch {status.branch} — the folder on disk stays exactly as it is.</p>
          )}

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
              <p className="text-xs text-emerald-800">
                Branch: <span className="font-mono">{status.branch ?? (kind === "web" ? "default" : "as on disk")}</span>
                {status.viaSnapshot ? " (read-only copy — the source panel can't open these files)" : ""}
              </p>
              {loadingResult ? (
                <Button disabled className="bg-emerald-700 text-white h-10 w-full sm:w-auto">
                  <Loader2 className="h-4 w-4 animate-spin mr-1.5" /> Opening your project…
                </Button>
              ) : (
                <div className="flex flex-col sm:flex-row gap-2">
                  <Button onClick={() => void showProject(status.id)} className="bg-emerald-700 hover:bg-emerald-800 text-white h-10 w-full sm:w-auto">
                    Show my project
                  </Button>
                  <Button
                    onClick={() => void showProject(status.id, "master")}
                    variant="outline"
                    className="border-emerald-300 text-emerald-900 hover:bg-emerald-100 h-10 w-full sm:w-auto inline-flex items-center gap-1.5 justify-center"
                    title="Open the guided 5-station journey on this project"
                  >
                    <GraduationCap className="h-4 w-4" /> Master this repo (the journey)
                  </Button>
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---------- mode 2: upload files (the `cie` CLI exports) ----------

function UploadFiles({ onClosed }: { onClosed: (open: boolean) => void }) {
  const { setCustom, loadSpringDemo, loadPolyglotDemo, loadDemo } = useCie();
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
      className={`w-full text-left rounded-lg border p-3.5 flex items-center gap-3 transition-colors ${filled ? "border-emerald-300 bg-emerald-50" : "border-stone-200 bg-white hover:border-stone-300"}`}
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
        Run these commands inside the branch you want CIE to see, then add the files they create:
        <span className="block mt-2 rounded-md bg-stone-900 text-stone-100 font-mono text-xs px-3 py-2 leading-relaxed">
          cie report . --json &gt; report.json<br />
          cie viz . --out viewspec.json<br />
          cie concepts . --format json &gt; concepts.json
        </span>
      </p>
      {slot("1. The health report", true, got.report, reportRef, "report")}
      {slot("2. The picture/map file", false, got.viewspec, vizRef, "viewspec")}
      {slot("3. The &lsquo;what it does&rsquo; list", false, got.concepts, conceptsRef, "concepts")}
      <div className="flex gap-2 pt-1">
        <Button onClick={apply} className="flex-1 bg-emerald-700 hover:bg-emerald-800 text-white">
          Show my project
        </Button>
        <Button variant="outline" onClick={() => { void loadDemo("polyglot"); onClosed(false); toast({ title: "Loaded: the five-service polyglot sample (Java, Kotlin, Rust, TypeScript, Python)" }); }}>
          Use sample
        </Button>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" className="shrink-0">
              More samples <ChevronDown className="h-3.5 w-3.5 opacity-60 ml-0.5" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem
              onClick={() => {
                void loadSpringDemo();
                onClosed(false);
                toast({ title: "Loaded: the Spring demo (Java + Kotlin)" });
              }}
            >
              <span className="h-2 w-2 rounded-full bg-emerald-600 mr-1" aria-hidden />
              Spring Boot service — Java + Kotlin, the full four sections
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => {
                void loadPolyglotDemo();
                onClosed(false);
                toast({ title: "Loaded: the five-service polyglot sample (Java, Kotlin, Rust, TypeScript, Python)" });
              }}
            >
              <span className="h-2 w-2 rounded-full bg-sky-600 mr-1" aria-hidden />
              Five-service harbor system — the polyglot one (default)
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => {
                void loadDemo("banking");
                onClosed(false);
                toast({ title: "Loaded: the small banking sample (Python)" });
              }}
            >
              <span className="h-2 w-2 rounded-full bg-amber-600 mr-1" aria-hidden />
              Small banking app — the original honest-stub one
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}

// ---------- the dialog shell ----------
//
// One three-way switch at the top (a Git repo link / this computer / upload
// files) replaces the old nested tabs, and once a connect job is in flight
// the switch steps aside so the progress panel owns the dialog.

type SourceKind = "web" | "local" | "upload";

function SourceCards({ kind, setKind }: { kind: SourceKind; setKind: (k: SourceKind) => void }) {
  const cards: { id: SourceKind; icon: React.ReactNode; title: string; sub: string }[] = [
    { id: "web", icon: <Globe className="h-4 w-4" />, title: "A Git repo link", sub: "Public, on the web — any branch" },
    { id: "local", icon: <HardDrive className="h-4 w-4" />, title: "This computer", sub: "Read in place — any branch" },
    { id: "upload", icon: <FileUp className="h-4 w-4" />, title: "Upload files", sub: "From the cie command" },
  ];
  return (
    <div className="grid grid-cols-1 gap-2 sm:grid-cols-3" role="tablist" aria-label="Where is your project?">
      {cards.map((c) => (
        <button
          key={c.id}
          type="button"
          role="tab"
          aria-selected={kind === c.id}
          onClick={() => setKind(c.id)}
          className={`rounded-xl border p-3 text-left transition-colors ${
            kind === c.id
              ? "border-emerald-300 bg-emerald-50 ring-1 ring-emerald-200"
              : "border-stone-200 bg-white hover:border-stone-300 hover:bg-stone-50"
          }`}
        >
          <span className={`inline-flex h-7 w-7 items-center justify-center rounded-lg ${kind === c.id ? "bg-emerald-700 text-white" : "bg-stone-100 text-stone-500"}`}>
            {c.icon}
          </span>
          <span className="mt-2 block text-sm font-semibold text-stone-900">{c.title}</span>
          <span className="mt-0.5 block text-xs text-stone-500 leading-snug">{c.sub}</span>
        </button>
      ))}
    </div>
  );
}

export function UseYourOwnProjectDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const { jobId } = useCie();
  const [kind, setKind] = useState<SourceKind>("web");
  // A connect job in flight (or resuming) owns the dialog until it's
  // finished or the user starts over — the source switch steps aside.
  const inFlight = jobId !== null;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-xl max-h-[85vh] supports-[height:1dvh]:max-h-[85dvh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>See your own project here</DialogTitle>
          <DialogDescription>
            Point CIE at your code — a public repo link, a folder on this computer, or the files the{" "}
            <span className="font-mono text-xs bg-stone-100 rounded px-1">cie</span> command makes. Every view switches to
            your project — and the chat knows it too.
          </DialogDescription>
        </DialogHeader>
        {!inFlight && <SourceCards kind={kind} setKind={setKind} />}
        <div className="pt-1">
          {kind === "upload" && !inFlight ? <UploadFiles onClosed={onOpenChange} /> : <ConnectRepo kind={kind === "upload" ? "web" : kind} onClosed={onOpenChange} />}
        </div>
      </DialogContent>
    </Dialog>
  );
}