// POST /api/fs/file { root, path, start?, end? } — read ONE file from the
// folder the user themselves pointed at ("Use your own → a folder on this
// computer"), so a tapped chart element can show the code it came from.
//
// Read-only and bounded, exactly like /api/fs/browse and /api/repo/scan:
//  - the root must pass validateLocalPath (absolute, not a system area);
//  - the requested path must stay INSIDE the root (no .., no symlink escape);
//  - only regular files, size-capped;
//  - the response carries a bounded window of lines (400 max), never the
//    whole file when it is large.
// Nothing is written, nothing is copied, nothing leaves this machine.

import { NextRequest, NextResponse } from "next/server";
import { promises as fs } from "fs";
import path from "path";
import { validateLocalPath } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

const MAX_FILE_BYTES = 512 * 1024; // 512 kB — source files, not data dumps
const MAX_WINDOW = 400; // lines per response
const DEFAULT_WINDOW = 200; // when no target line was given

export async function POST(req: NextRequest) {
  let body: { root?: string; path?: string; start?: number; end?: number; all?: boolean };
  try {
    body = (await req.json()) as typeof body;
  } catch {
    return NextResponse.json({ error: "We couldn't read that request. Please try again." }, { status: 400 });
  }

  const rootCheck = validateLocalPath(body.root ?? "");
  if (!rootCheck.ok) {
    return NextResponse.json({ error: rootCheck.plain }, { status: 400 });
  }

  const rel = String(body.path ?? "").trim();
  if (!rel || rel.length > 1024 || rel.includes("\0")) {
    return NextResponse.json({ error: "That file path doesn't look right." }, { status: 400 });
  }

  const rootAbs = rootCheck.path;
  const fileAbs = path.resolve(rootAbs, rel);
  // containment: the resolved path must stay inside the consented root
  if (fileAbs !== rootAbs && !fileAbs.startsWith(rootAbs + path.sep)) {
    return NextResponse.json({ error: "That file is outside the project folder you connected." }, { status: 403 });
  }

  let realRoot: string;
  let realFile: string;
  try {
    realRoot = await fs.realpath(rootAbs);
    realFile = await fs.realpath(fileAbs); // also rejects missing files
    if (realFile !== realRoot && !realFile.startsWith(realRoot + path.sep)) {
      // a symlink inside the project pointing outside it — refuse, quietly
      return NextResponse.json({ error: "That file is outside the project folder you connected." }, { status: 403 });
    }
  } catch {
    return NextResponse.json({ error: "We couldn't find that file in your project. It may have moved or been renamed." }, { status: 404 });
  }

  try {
    const st = await fs.stat(realFile);
    if (!st.isFile()) {
      return NextResponse.json({ error: "That path points at a folder, not a file." }, { status: 400 });
    }
    if (st.size > MAX_FILE_BYTES) {
      return NextResponse.json(
        { error: "That file is too big to show here (over 512 kB). CIE still indexed it — it just won't render it as code." },
        { status: 413 }
      );
    }

    const raw = await fs.readFile(realFile, "utf8");
    const allLines = raw.split("\n");
    const totalLines = allLines.length;

    // bounded window: around the requested target line, or the head of the
    // file — unless `all` was asked (byte-offset → line resolution needs the
    // whole file; the size cap above still bounds it)
    const wantAll = body.all === true;
    const hasTarget = Number.isFinite(body.start) && (body.start as number) >= 1;
    const reqStart = hasTarget ? Math.max(1, Math.floor(body.start as number)) : 1;
    const reqEnd = Number.isFinite(body.end) && (body.end as number) >= reqStart ? Math.floor(body.end as number) : reqStart + DEFAULT_WINDOW - 1;
    let start = reqStart;
    let end = Math.min(totalLines, Math.max(reqEnd, reqStart), reqStart + MAX_WINDOW - 1);
    if (!hasTarget && totalLines > DEFAULT_WINDOW) {
      start = 1;
      end = DEFAULT_WINDOW;
    }
    if (wantAll) {
      start = 1;
      end = totalLines;
    }
    const content = allLines.slice(start - 1, end).join("\n");

    return NextResponse.json({
      path: rel,
      start,
      end,
      totalLines,
      truncated: end < totalLines || (hasTarget && reqEnd > end),
      content,
    });
  } catch {
    return NextResponse.json({ error: "We could reach the folder but couldn't read that file. Please try again." }, { status: 500 });
  }
}
