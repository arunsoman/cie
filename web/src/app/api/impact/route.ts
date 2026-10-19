// POST /api/impact — on-demand blast-radius query (`cie impact --symbol`).
//
// Unlike report/viz/concepts/deps/cochange/overlap/drift, this is never
// pre-computed at ingest: it needs a specific symbol as a target, so one
// call per question would mean one `cie impact` run per symbol in the
// project. It also needs the project's files still readable on disk —
// true for a local folder (read in place, sourceRoot kept), never true for
// a URL clone (deleted right after ingest) or uploaded files (never
// touched disk). Both limits are stated honestly to the caller rather than
// silently failing.

import { NextRequest, NextResponse } from "next/server";
import { promises as fs } from "fs";
import { isBlockedLocalPath, resolveCieBin, run } from "@/lib/analyze-jobs";
import type { CieImpact } from "@/lib/cie-types";

export const runtime = "nodejs";

const TIMEOUT_MS = 120_000;

export async function POST(req: NextRequest) {
  let body: { sourceRoot?: string; symbol?: string };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Could not read the request. Please try again." }, { status: 400 });
  }

  const sourceRoot = String(body.sourceRoot ?? "").trim();
  const symbol = String(body.symbol ?? "").trim();

  if (!symbol || symbol.length > 300) {
    return NextResponse.json({ error: "Please name a real symbol to check the impact of." }, { status: 400 });
  }
  if (!sourceRoot) {
    return NextResponse.json(
      {
        error:
          "Impact analysis needs the project's files still on disk, and this project's aren't — it was loaded from a cloned URL (deleted right after reading) or uploaded files (never saved to disk). Point CIE at a local folder on this computer to ask this.",
      },
      { status: 409 }
    );
  }
  if (!sourceRoot.startsWith("/") || isBlockedLocalPath(sourceRoot)) {
    return NextResponse.json({ error: "That project location isn't one CIE can read from." }, { status: 400 });
  }
  try {
    const st = await fs.stat(sourceRoot);
    if (!st.isDirectory()) throw new Error("not a directory");
  } catch {
    return NextResponse.json(
      { error: "That project folder isn't there any more — it may have moved since CIE read it. Reconnect it and try again." },
      { status: 404 }
    );
  }

  const bin = await resolveCieBin();
  if (!bin) {
    return NextResponse.json({ error: "CIE's engine isn't available on this server right now." }, { status: 503 });
  }

  const res = await run(bin, ["impact", sourceRoot, "--symbol", symbol, "--json"], { timeoutMs: TIMEOUT_MS });
  if (res.timedOut) {
    return NextResponse.json({ error: "That took too long to work out — please try a smaller project or a more specific symbol." }, { status: 504 });
  }
  if (res.code !== 0) {
    if (/symbol '.*' not found/.test(res.stderr)) {
      return NextResponse.json({ error: `CIE couldn't find a symbol named '${symbol}' in this project.` }, { status: 404 });
    }
    return NextResponse.json({ error: "CIE couldn't work out the impact just now.", technical: res.stderr.slice(0, 400) }, { status: 502 });
  }
  let impact: CieImpact;
  try {
    impact = JSON.parse(res.stdout);
  } catch {
    return NextResponse.json({ error: "CIE's answer came back in a shape we didn't expect." }, { status: 502 });
  }
  return NextResponse.json(impact);
}
