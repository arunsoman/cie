// POST /api/polyglot/table-rows { root, table, page? } — a paginated raw
// dump of one whitelisted cie-polyglot table (see POLYGLOT_TABLES in
// src/lib/polyglot-db.ts). `table` is checked against that whitelist before
// it ever reaches a SQL string.

import { NextRequest, NextResponse } from "next/server";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { getPolyglotTableRows } from "@/lib/polyglot-db";

export const runtime = "nodejs";

const PAGE_SIZE = 50;

export async function POST(req: NextRequest) {
  let body: { root?: string; revision?: string; table?: string; page?: number };
  try {
    body = (await req.json()) as typeof body;
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const rootCheck = validateLocalPath(body.root ?? "");
  if (!rootCheck.ok) {
    return NextResponse.json({ error: rootCheck.plain }, { status: 400 });
  }
  if (!body.table) {
    return NextResponse.json({ error: "Missing table name." }, { status: 400 });
  }

  const page = Math.max(body.page ?? 0, 0);
  const result = await getPolyglotTableRows(rootCheck.path, body.table, {
    revision: body.revision,
    limit: PAGE_SIZE,
    offset: page * PAGE_SIZE,
  });
  if (!result) {
    return NextResponse.json({ available: false as const });
  }
  return NextResponse.json({ available: true as const, pageSize: PAGE_SIZE, page, ...result });
}
