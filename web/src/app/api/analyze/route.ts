// POST /api/analyze — start a background "clone + parse" job for a public git repo.

import { NextRequest, NextResponse } from "next/server";
import { startJob, validateRepoUrl, jobStatus } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { repoUrl?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Could not read the request. Please try again." }, { status: 400 });
  }

  const check = validateRepoUrl(String(body.repoUrl ?? ""));
  if (!check.ok) {
    return NextResponse.json({ error: check.plain }, { status: 400 });
  }

  const job = await startJob(check.url, check.repoName);
  return NextResponse.json({ jobId: job.id, status: jobStatus(job) });
}
