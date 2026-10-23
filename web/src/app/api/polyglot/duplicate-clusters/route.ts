// POST /api/polyglot/duplicate-clusters { root } — exact-IR-hash duplicate
// families from the cie-polyglot store (see src/lib/polyglot-db.ts), one
// representative "hub" per family with every other exact-duplicate member
// attached to it, biggest family first.

import { NextRequest, NextResponse } from "next/server";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { getDuplicateClusters } from "@/lib/polyglot-db";

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

  const result = await getDuplicateClusters(rootCheck.path, { revision: body.revision });
  if (!result) {
    return NextResponse.json({ available: false as const });
  }
  return NextResponse.json({ available: true as const, ...result });
}
