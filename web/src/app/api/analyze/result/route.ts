// GET /api/analyze/result?job=ID — fetch the three JSON exports of a finished job.

import { NextRequest, NextResponse } from "next/server";
import { jobs } from "@/lib/analyze-jobs";

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
  if (job.state !== "done" || !job.result) {
    return NextResponse.json({ error: "That job isn't finished yet." }, { status: 409 });
  }
  return NextResponse.json(job.result);
}
