// GET /api/analyze/result/extra?job=ID — the fields the main result route
// doesn't fetch (viewspec, concepts, stack, deps, cochange, polyglotSpecs,
// runtime, quality, stories, overlap, drift). Called after the project has
// already rendered on `report` alone (see showProject/restoreJobResult +
// cie-store.ts's mergeExtra); a failure here never blocks the project from
// opening, only from showing these heavier/less-urgent fields right away.

import { withRevision } from "@/lib/storage-native";
import { fetchArtifactField, EXTRA_FIELDS } from "@/lib/analyze-artifact";
import { NextRequest, NextResponse } from "next/server";
import { jobs, rehydratePersistedResult } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function GET(req: NextRequest) {
  const id = req.nextUrl.searchParams.get("job") ?? "";
  const job = jobs.get(id) ?? (await rehydratePersistedResult(id));
  if (!job || job.state !== "done" || !job.result) {
    return NextResponse.json({ error: "No finished job for this id." }, { status: 404 });
  }
  const root = job.result.storageRoot ?? job.result.sourceRoot;
  if (!root || !job.result.revision) {
    return NextResponse.json({ error: "This saved job has no captured storage reference." }, { status: 410 });
  }
  try {
    const document: Record<string, unknown> = await withRevision(root, async (context) => {
      const out: Record<string, unknown> = {};
      for (const kind of EXTRA_FIELDS) out[kind] = await fetchArtifactField(context, kind);
      return out;
    }, job.result.revision);
    return NextResponse.json(document);
  } catch (error) {
    return NextResponse.json({ error: error instanceof Error ? error.message : "The extra fields are unavailable." }, { status: 410 });
  }
}
