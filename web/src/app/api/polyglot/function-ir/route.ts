// POST /api/polyglot/function-ir { root, symbolId } — one symbol's lowered
// CFG (blocks/instructions/edges) plus its outgoing calls, name-resolved.

import { NextRequest, NextResponse } from "next/server";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { getFunctionIr } from "@/lib/polyglot-db";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { root?: string; symbolId?: number };
  try {
    body = (await req.json()) as typeof body;
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const rootCheck = validateLocalPath(body.root ?? "");
  if (!rootCheck.ok) {
    return NextResponse.json({ error: rootCheck.plain }, { status: 400 });
  }
  if (typeof body.symbolId !== "number") {
    return NextResponse.json({ error: "Missing symbolId." }, { status: 400 });
  }

  const ir = getFunctionIr(rootCheck.path, body.symbolId);
  if (!ir) {
    return NextResponse.json({ available: false as const });
  }
  return NextResponse.json({ available: true as const, ...ir });
}
