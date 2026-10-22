// POST /api/polyglot/snippet { root, path, startByte, endByte } — reads
// ONE bounded byte range from a file inside the project folder the user
// connected, so the duplicate-cluster view can show the actual code behind
// a cie-polyglot symbol (its `symbols.start_byte`/`end_byte` span).
//
// Same containment discipline as /api/fs/file: root must pass
// validateLocalPath, the resolved file must stay inside it (no .., no
// symlink escape), and the response is size-capped.

import { NextRequest, NextResponse } from "next/server";
import { promises as fs } from "fs";
import path from "path";
import { validateLocalPath } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

const MAX_FILE_BYTES = 2 * 1024 * 1024; // files bigger than this aren't opened at all
const MAX_SNIPPET_BYTES = 20_000; // a span wider than this is truncated, not rejected

export async function POST(req: NextRequest) {
  let body: { root?: string; path?: string; startByte?: number; endByte?: number };
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
  const startByte = Number(body.startByte);
  const endByte = Number(body.endByte);
  if (!Number.isFinite(startByte) || !Number.isFinite(endByte) || startByte < 0 || endByte < startByte) {
    return NextResponse.json({ error: "That byte range doesn't look right." }, { status: 400 });
  }

  const rootAbs = rootCheck.path;
  const fileAbs = path.resolve(rootAbs, rel);
  if (fileAbs !== rootAbs && !fileAbs.startsWith(rootAbs + path.sep)) {
    return NextResponse.json({ error: "That file is outside the project folder you connected." }, { status: 403 });
  }

  let realRoot: string;
  let realFile: string;
  try {
    realRoot = await fs.realpath(rootAbs);
    realFile = await fs.realpath(fileAbs);
    if (realFile !== realRoot && !realFile.startsWith(realRoot + path.sep)) {
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
      return NextResponse.json({ error: "That file is too big to show a snippet from here." }, { status: 413 });
    }

    const cappedEnd = Math.min(endByte, startByte + MAX_SNIPPET_BYTES, st.size);
    const fd = await fs.open(realFile, "r");
    let text: string;
    try {
      const len = Math.max(0, cappedEnd - startByte);
      const buf = Buffer.alloc(len);
      await fd.read(buf, 0, len, startByte);
      text = buf.toString("utf8");
    } finally {
      await fd.close();
    }

    return NextResponse.json({
      path: rel,
      startByte,
      endByte: cappedEnd,
      truncated: cappedEnd < endByte,
      text,
    });
  } catch {
    return NextResponse.json({ error: "We could reach the folder but couldn't read that file. Please try again." }, { status: 500 });
  }
}
