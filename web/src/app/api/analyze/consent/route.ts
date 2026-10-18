// POST /api/analyze/consent — answer the ask-before-skipping question.
// Body: { job: id, proceed: true|false }.
// true  → the check continues; files CIE can't read stay honestly skipped.
// false → the check stops right there. Nothing was read beyond the
//         inventory, and nothing will be.

import { NextRequest, NextResponse } from "next/server";
import { resumeJob, jobStatus } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { job?: unknown; proceed?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Could not read the request. Please try again." }, { status: 400 });
  }
  const id = typeof body.job === "string" ? body.job : "";
  const proceed = body.proceed === true;
  if (!id) {
    return NextResponse.json({ error: "Which check is this about? Please try again." }, { status: 400 });
  }
  const job = resumeJob(id, proceed);
  if (!job) {
    return NextResponse.json(
      { error: "That check isn't waiting for an answer right now — it may have finished already." },
      { status: 409 }
    );
  }
  return NextResponse.json({ status: jobStatus(job) });
}
