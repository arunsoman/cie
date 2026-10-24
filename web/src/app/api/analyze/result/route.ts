// GET /api/analyze/result?job=ID — retrieve the captured workspace artifact.

import { withRevision, storageCall } from "@/lib/storage-native";
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
    const captured=await withRevision<{outputs:{schema_version:number;data:{type:string;artifact_kind:string;document:Record<string,unknown>}}[]}>(root,
      context=>storageCall({op:'retrieve',context,request:{query:{select:{kind:'file'}},outputs:[{id:'bootstrap',output:{artifact:{kind:'ui_bootstrap_v1'}}}],budget:{max_response_bytes:16*1024*1024}}}),job.result.revision);
    const output=captured.outputs[0];
    if(output.schema_version!==2||output.data.type!=='artifact'||output.data.artifact_kind!=='ui_bootstrap_v1')throw new Error('Unsupported workspace artifact schema');
    return NextResponse.json({...output.data.document,label:job.result.label,scan:job.result.scan,sourceRoot:job.result.sourceRoot??null,storageRoot:root});
  } catch(error) {return NextResponse.json({error:error instanceof Error?error.message:'The captured workspace is unavailable; reconnect the repository.'},{status:410});}

}
