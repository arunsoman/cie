// POST /api/polyglot/tables { root } — row-count inventory of every table
// `ensure_all_schemas` creates in the cie-polyglot store (see
// src/lib/polyglot-db.ts), whether or not this run populated it.

import { NextRequest, NextResponse } from "next/server";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { getPolyglotTableInventory } from "@/lib/polyglot-db";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { root?: string };
  try {
    body = (await req.json()) as typeof body;
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const rootCheck = validateLocalPath(body.root ?? "");
  if (!rootCheck.ok) {
    return NextResponse.json({ error: rootCheck.plain }, { status: 400 });
  }

  const tables = getPolyglotTableInventory(rootCheck.path);
  if (!tables) {
    return NextResponse.json({ available: false as const });
  }
  return NextResponse.json({ available: true as const, tables });
}
