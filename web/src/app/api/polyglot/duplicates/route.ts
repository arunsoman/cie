// POST /api/polyglot/duplicates { root, tier?, detection?, epistemic?, page? }
// — a flat, filterable, paginated slice of the `duplicates` table (every
// tier, unlike /duplicate-clusters which is canonical-only and graph-shaped)
// from the cie-polyglot store (see src/lib/polyglot-db.ts), ids resolved to
// symbol name/kind/path.

import { NextRequest, NextResponse } from "next/server";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { getDuplicates } from "@/lib/polyglot-db";

export const runtime = "nodejs";

const PAGE_SIZE = 50;

export async function POST(req: NextRequest) {
  let body: { root?: string; revision?: string; tier?: string; detection?: string; epistemic?: string; page?: number };
  try {
    body = (await req.json()) as typeof body;
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const rootCheck = validateLocalPath(body.root ?? "");
  if (!rootCheck.ok) {
    return NextResponse.json({ error: rootCheck.plain }, { status: 400 });
  }

  const page = Math.max(body.page ?? 0, 0);
  const result = await getDuplicates(rootCheck.path, {
    revision: body.revision,
    tier: body.tier || undefined,
    detection: body.detection || undefined,
    epistemic: body.epistemic || undefined,
    limit: PAGE_SIZE,
    offset: page * PAGE_SIZE,
  });
  if (!result) {
    return NextResponse.json({ available: false as const });
  }
  return NextResponse.json({ available: true as const, pageSize: PAGE_SIZE, page, ...result });
}
