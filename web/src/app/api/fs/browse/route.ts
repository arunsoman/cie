// GET /api/fs/browse?path=/some/dir — list the folders inside a directory so
// the "On this computer" flow can offer a real Browse button instead of making
// people type paths by hand.
//
// The server and the browser share this machine (the app runs locally), so
// "local" here genuinely means the user's own computer. Only folder names are
// ever returned — never file contents. Protected system areas are refused
// with the same policy as the analyzer.

import { NextRequest, NextResponse } from "next/server";
import { promises as fs } from "fs";
import os from "os";
import path from "path";
import { isBlockedLocalPath } from "@/lib/analyze-jobs";

export const runtime = "nodejs";

const MAX_ENTRIES = 500;

// Friendly starting points, shown as one-tap chips.
const ROOT_CANDIDATES = ["/home", "/mnt", "/media", "/opt", "/srv", "/Users", "/workspace", "/projects"];

export async function GET(req: NextRequest) {
  const raw = (req.nextUrl.searchParams.get("path") ?? "").trim();
  const home = os.homedir();

  let target: string;
  if (!raw) {
    target = home;
  } else {
    if (!raw.startsWith("/")) {
      return NextResponse.json(
        { error: "Please use a full path starting with / — for example /home/you/my-project" },
        { status: 400 }
      );
    }
    target = path.resolve(raw);
    if (isBlockedLocalPath(target)) {
      return NextResponse.json(
        { error: "That folder is off-limits (system files). Pick your project folder instead." },
        { status: 403 }
      );
    }
  }

  let st;
  try {
    st = await fs.stat(target);
  } catch {
    return NextResponse.json(
      { error: "We couldn't find that folder. Double-check the path (and spelling)." },
      { status: 404 }
    );
  }
  if (!st.isDirectory()) {
    return NextResponse.json({ error: "That path points at a file, not a folder." }, { status: 400 });
  }

  let entries;
  try {
    entries = await fs.readdir(target, { withFileTypes: true });
  } catch {
    return NextResponse.json(
      { error: "That folder can't be opened (permissions). Pick another one." },
      { status: 403 }
    );
  }

  const dirs = entries
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort((a, b) => {
      const aDot = a.startsWith(".");
      const bDot = b.startsWith(".");
      if (aDot !== bDot) return aDot ? 1 : -1;
      return a.localeCompare(b);
    });

  const roots: string[] = [];
  for (const r of [home, ...ROOT_CANDIDATES]) {
    try {
      if ((await fs.stat(r)).isDirectory() && !roots.includes(r)) roots.push(r);
    } catch {
      /* candidate doesn't exist — skip */
    }
  }

  const parent = target === "/" ? null : path.dirname(target);
  return NextResponse.json({
    path: target,
    parent,
    name: target.split("/").filter(Boolean).pop() ?? "/",
    home,
    roots,
    entries: dirs.slice(0, MAX_ENTRIES).map((name) => ({ name })),
    truncated: dirs.length > MAX_ENTRIES,
  });
}
