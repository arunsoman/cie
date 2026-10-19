// POST /api/analyze — start a background "parse" job.
// Body: { repoUrl, branch? } for a public git repo (branch optional — the
// repo's default when omitted), or { localPath, branchRef? } for a folder on
// this machine (read-only, analysed in place — or as a read-only branch
// snapshot when a branch other than what's on disk is asked for).

import { NextRequest, NextResponse } from "next/server";
import { startJob, validateRepoUrl, validateLocalPath, validateBranch, validateBranchRef, jobStatus } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { repoUrl?: unknown; localPath?: unknown; branch?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Could not read the request. Please try again." }, { status: 400 });
  }

  const localRaw = typeof body.localPath === "string" ? body.localPath.trim() : "";
  const urlRaw = typeof body.repoUrl === "string" ? body.repoUrl.trim() : "";
  const branchRaw = typeof body.branch === "string" ? body.branch.trim() : "";

  if (body.branch !== undefined && body.branch !== null && typeof body.branch !== "string") {
    return NextResponse.json({ error: "The branch must be a text name — for example main or dev." }, { status: 400 });
  }

  if (localRaw) {
    const check = validateLocalPath(localRaw);
    if (!check.ok) return NextResponse.json({ error: check.plain }, { status: 400 });
    let branchRef: string | undefined;
    if (branchRaw) {
      const br = validateBranchRef(branchRaw);
      if (!br.ok) return NextResponse.json({ error: br.plain }, { status: 400 });
      branchRef = br.ref;
    }
    const job = await startJob({ source: "local", localPath: check.path, repoName: check.repoName, branchRef });
    return NextResponse.json({ jobId: job.id, status: jobStatus(job) });
  }

  if (urlRaw) {
    const check = validateRepoUrl(urlRaw);
    if (!check.ok) return NextResponse.json({ error: check.plain }, { status: 400 });
    let branch: string | undefined;
    if (branchRaw) {
      const br = validateBranch(branchRaw);
      if (!br.ok) return NextResponse.json({ error: br.plain }, { status: 400 });
      branch = br.name;
    }
    const job = await startJob({ source: "url", url: check.url, repoName: check.repoName, branch });
    return NextResponse.json({ jobId: job.id, status: jobStatus(job) });
  }

  return NextResponse.json(
    { error: "Please give me a repo link or a folder on this computer." },
    { status: 400 }
  );
}