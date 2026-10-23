// POST /api/polyglot/symbols { root, query } — symbol search by qual_name
// substring, for picking a function to inspect in the IR viewer.

import { NextRequest, NextResponse } from "next/server";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { searchPolyglotSymbols } from "@/lib/polyglot-db";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { root?: string; revision?: string; query?: string };
  try {
    body = (await req.json()) as typeof body;
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const rootCheck = validateLocalPath(body.root ?? "");
  if (!rootCheck.ok) {
    return NextResponse.json({ error: rootCheck.plain }, { status: 400 });
  }

  const query = (body.query ?? "").trim();
  if (query.length < 2) {
    return NextResponse.json({ available: true as const, symbols: [] });
  }

  const symbols = await searchPolyglotSymbols(rootCheck.path, query, 30, body.revision);
  if (!symbols) {
    return NextResponse.json({ available: false as const });
  }
  return NextResponse.json({ available: true as const, symbols });
}
