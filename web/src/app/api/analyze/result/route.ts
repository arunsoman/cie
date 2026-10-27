// GET /api/analyze/result?job=ID — retrieve the captured workspace's core
// artifact (report only). `report` is the one thing the page actually
// shown after a project loads needs (sections.tsx's StartView); everything
// else (viewspec, concepts, deps, cochange, overlap, drift, stack,
// polyglotSpecs, runtime, quality, stories) is fetched lazily by
// GET /api/analyze/result/extra once the page has already rendered, so a
// large repo's heavier fields never block — or risk — getting here at all.

import { withRevision } from "@/lib/storage-native";
import { fetchArtifactField, CORE_FIELDS } from "@/lib/analyze-artifact";
import { NextRequest, NextResponse } from "next/server";
import { jobs, rehydratePersistedResult } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function GET(req: NextRequest) {
  const id = req.nextUrl.searchParams.get("job") ?? "";
  // The in-memory map first; after a server restart the persisted result
  // (result.json — "launch with the last opened workspace", the disk half)
  // rehydrates the storage reference and retrieves its pinned artifact.
  const job = jobs.get(id) ?? (await rehydratePersistedResult(id));
  if (!job) {
    return NextResponse.json(
      { error: "We lost track of that job (it may be old). Please start again." },
      { status: 404 }
    );
  }
  if (job.state !== "done" || !job.result) {
    return NextResponse.json({ error: "That job isn't finished yet." }, { status: 409 });
  }
  const root=job.result.storageRoot??job.result.sourceRoot;
  if (!root || !job.result.revision) return NextResponse.json({error:"This saved job has no captured storage reference. Reconnect the repository."},{status:410});
  try {
    const document: Record<string, unknown> = await withRevision(root, async (context) => {
      const out: Record<string, unknown> = {};
      for (const kind of CORE_FIELDS) out[kind] = await fetchArtifactField(context, kind);
      return out;
    }, job.result.revision);
    return NextResponse.json({...document,label:job.result.label,scan:job.result.scan,sourceRoot:job.result.sourceRoot??null,storageRoot:root});
  } catch(error) {return NextResponse.json({error:error instanceof Error?error.message:'The captured workspace is unavailable; reconnect the repository.'},{status:410});}

}
