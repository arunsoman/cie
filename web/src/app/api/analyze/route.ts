// POST /api/analyze — start a background "parse" job.
// Body: { repoUrl } for a public git repo, or { localPath } for a folder on
// this machine (read-only, analysed in place).

import { NextRequest, NextResponse } from "next/server";
import { startJob, validateRepoUrl, validateLocalPath, jobStatus } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { repoUrl?: unknown; localPath?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Could not read the request. Please try again." }, { status: 400 });
  }

  const localRaw = typeof body.localPath === "string" ? body.localPath.trim() : "";
  const urlRaw = typeof body.repoUrl === "string" ? body.repoUrl.trim() : "";

  if (localRaw) {
    const check = validateLocalPath(localRaw);
    if (!check.ok) return NextResponse.json({ error: check.plain }, { status: 400 });
    const job = await startJob({ source: "local", localPath: check.path, repoName: check.repoName });
    return NextResponse.json({ jobId: job.id, status: jobStatus(job) });
  }

  if (urlRaw) {
    const check = validateRepoUrl(urlRaw);
    if (!check.ok) return NextResponse.json({ error: check.plain }, { status: 400 });
    const job = await startJob({ source: "url", url: check.url, repoName: check.repoName });
    return NextResponse.json({ jobId: job.id, status: jobStatus(job) });
  }

  return NextResponse.json(
    { error: "Please give me a repo link or a folder on this computer." },
    { status: 400 }
  );
}
