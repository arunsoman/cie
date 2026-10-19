// POST /api/repo/branches — the branch names behind the branch picker in the
// "Use your own" dialog, BEFORE any analysis starts.
//
// Body { repoUrl }   → one small `git ls-remote --heads` network read:
//                      { source: "url", heads: ["main", "dev", …] }
// Body { localPath } → read-only walks of the folder's own refs:
//                      { source: "local", git: true, current, local: […],
//                        remote: ["origin/dev", …] }
//                      or { source: "local", git: false } when the folder
//                      isn't a git repo (the picker is simply hidden).
//
// A failed listing is a plain-words note, never a fatal error: the analysis
// itself can still be started with a hand-typed branch name.

import { NextRequest, NextResponse } from "next/server";
import { promises as fs } from "fs";
import { validateRepoUrl, validateLocalPath, listBranches } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

export async function POST(req: NextRequest) {
  let body: { repoUrl?: unknown; localPath?: unknown };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Could not read the request. Please try again." }, { status: 400 });
  }

  const localRaw = typeof body.localPath === "string" ? body.localPath.trim() : "";
  const urlRaw = typeof body.repoUrl === "string" ? body.repoUrl.trim() : "";

  if (localRaw) {
    const check = validateLocalPath(localRaw);
    if (!check.ok) return NextResponse.json({ error: check.plain }, { status: 400 });
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
      const list = await listBranches({ source: "local", localPath: check.path });
      return NextResponse.json(list);
    } catch {
      return NextResponse.json(
        { error: "We could open the folder but couldn't read its branch list — you can still type a branch name by hand." },
        { status: 500 }
      );
    }
  }

  if (urlRaw) {
    const check = validateRepoUrl(urlRaw);
    if (!check.ok) return NextResponse.json({ error: check.plain }, { status: 400 });
    try {
      const list = await listBranches({ source: "url", url: check.url });
      return NextResponse.json(list);
    } catch {
      return NextResponse.json(
        { error: "We couldn't read the branch list from that address — you can still type a branch name by hand." },
        { status: 500 }
      );
    }
  }

  return NextResponse.json({ error: "Please give me a repo link or a folder on this computer." }, { status: 400 });
}