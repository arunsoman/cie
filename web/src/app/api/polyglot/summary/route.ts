// POST /api/polyglot/summary { root } — reads the cie-polyglot SQLite
// scratch store already produced by `cie polyglot <root>` (see
// src/lib/polyglot-db.ts). Never runs the CLI itself — a real run on a
// large repo can take a long time — so {available:false} just means no
// store exists yet for this project, not an error.

import { NextRequest, NextResponse } from "next/server";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { getPolyglotSummary } from "@/lib/polyglot-db";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { root?: string; revision?: string };
  try {
    body = (await req.json()) as typeof body;
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const rootCheck = validateLocalPath(body.root ?? "");
  if (!rootCheck.ok) {
    return NextResponse.json({ error: rootCheck.plain }, { status: 400 });
  }

  const summary = await getPolyglotSummary(rootCheck.path, body.revision);
  if (!summary) {
    return NextResponse.json({ available: false as const });
  }
  return NextResponse.json({ available: true as const, summary });
}
