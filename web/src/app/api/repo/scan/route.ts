// POST /api/repo/scan { localPath } — peek inside a local folder BEFORE
// starting the analysis: which languages are used, which of them CIE can
// read, which it can't yet, and which frameworks the manifests show.
//
// Read-only and bounded. The engine remains the source of truth for what is
// actually parsed — this is the honest preview.

import { NextRequest, NextResponse } from "next/server";
import { promises as fs } from "fs";
import { validateLocalPath } from "@/lib/analyze-jobs";
import { scanRepoLanguages } from "@/lib/repo-scan";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { localPath?: string };
  try {
    body = (await req.json()) as { localPath?: string };
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const check = validateLocalPath(body.localPath ?? "");
  if (!check.ok) {
    return NextResponse.json({ error: check.plain }, { status: 400 });
  }

  try {
    const st = await fs.stat(check.path);
    if (!st.isDirectory()) {
      return NextResponse.json(
        { error: "That path points at a file, not a folder. Please give me the project folder." },
        { status: 400 }
      );
    }
  } catch {
    return NextResponse.json(
      { error: "We couldn't find that folder on this computer. Double-check the path (and spelling)." },
      { status: 404 }
    );
  }

  try {
    const scan = await scanRepoLanguages(check.path, check.repoName);
    return NextResponse.json({ scan });
  } catch {
    return NextResponse.json(
      { error: "We could open the folder but couldn't look inside it. Please try again." },
      { status: 500 }
    );
  }
}
