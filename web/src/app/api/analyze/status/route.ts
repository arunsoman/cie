// GET /api/analyze/status?job=ID — poll a job's progress (plain-words states).

import { NextRequest, NextResponse } from "next/server";
import { jobs, jobStatus } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function GET(req: NextRequest) {
  const id = req.nextUrl.searchParams.get("job") ?? "";
  const job = jobs.get(id);
  if (!job) {
    return NextResponse.json(
      { error: "We lost track of that job (it may be old). Please start again." },
      { status: 404 }
    );
  }
  return NextResponse.json({ status: jobStatus(job) });
}
