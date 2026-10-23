// PERF-L1 artifact-identity comparison — noise-aware protocol:
//   baseline1 vs baseline2 (serial vs serial) → the NOISE path set (paths that
//   differ between any two runs of the SAME code: wall stamps, time-derived
//   ids, commit dates).
//   baseline1 vs parallel → must differ ONLY on paths inside the noise set
//   (same path shape, different timestamp values), and be byte-identical on
//   every other path. Any path outside the noise set is a REAL regression.
//
//   node compare.cjs <baseline1.json> <baseline2.json> <parallel.json>

const fs = require("node:fs");

const [p1, p2, p3] = process.argv.slice(2);
if (!p1 || !p2 || !p3) {
  console.error("usage: compare.cjs <baseline1.json> <baseline2.json> <parallel.json>");
  process.exit(2);
}
const b1 = JSON.parse(fs.readFileSync(p1, "utf8"));
const b2 = JSON.parse(fs.readFileSync(p2, "utf8"));
const par = JSON.parse(fs.readFileSync(p3, "utf8"));

const ARTIFACTS = ["report", "viewspec", "concepts", "deps", "stack", "polyglotSpecs", "cochange", "overlap", "drift"];

// Deep-collect the paths whose values differ, descending only when both
// sides are objects (arrays compared whole — length or order changes show
// up as the array path itself).
function diffPaths(x, y, prefix, out) {
  if (x === null || y === null || x === undefined || y === undefined || typeof x !== "object" || typeof y !== "object" || Array.isArray(x) || Array.isArray(y)) {
    if (JSON.stringify(x) !== JSON.stringify(y)) out.add(prefix || "<root>");
    return;
  }
  const keys = new Set([...Object.keys(x), ...Object.keys(y)]);
  for (const k of keys) diffPaths(x[k], y[k], prefix ? `${prefix}.${k}` : k, out);
}

let real = 0;
for (const key of ARTIFACTS) {
  const noise = new Set();
  diffPaths(b1.result?.[key] ?? null, b2.result?.[key] ?? null, "", noise);
  const changed = new Set();
  diffPaths(b1.result?.[key] ?? null, par.result?.[key] ?? null, "", changed);
  const outside = [...changed].filter((p) => !noise.has(p));
  const shape = (v) => (v === null || v === undefined ? "null" : Array.isArray(v) ? `array(${v.length})` : typeof v === "object" ? `object(${Object.keys(v).length} keys)` : JSON.stringify(v)?.slice(0, 40));
  if (outside.length === 0) {
    console.log(`IDENTICAL-modulo-noise  ${key}  (noise paths: ${noise.size}, parallel-changed paths: ${changed.size})`);
  } else {
    real++;
    console.log(`REAL DIFFERENCES        ${key}  (${outside.length} paths outside the serial noise set):`);
    for (const p of outside.slice(0, 8)) {
      const [aSec, bSec, cSec] = [b1.result?.[key], par.result?.[key], b2.result?.[key]];
      const get = (o, path) => path.split(".").reduce((acc, k) => (acc == null ? acc : acc[k]), o);
      console.log(`    ${p}: baseline=${JSON.stringify(get(aSec, p))?.slice(0, 60)} parallel=${JSON.stringify(get(bSec, p))?.slice(0, 60)} baseline2=${JSON.stringify(get(cSec, p))?.slice(0, 60)}`);
    }
  }
}

console.log("\n== timings ==");
const fmt = (ms) => (ms == null ? "-" : `${(ms / 1000).toFixed(2)}s`);
for (const [tag, d] of [["baseline1", b1], ["baseline2", b2], ["parallel", par]]) {
  const steps = Object.fromEntries(d.steps.map((s) => [s.key, s.ms]));
  console.log(`  ${tag.padEnd(9)} wall=${(d.wallMs / 1000).toFixed(2)}s  read=${fmt(steps.read)}  map=${fmt(steps.map)}  concepts=${fmt(steps.concepts)}  deps=${fmt(steps.deps)}  stack=${fmt(steps.stack)}  polyglot=${fmt(steps.polyglot)}  cochange=${fmt(steps.cochange)}  overlap=${fmt(steps.overlap)}  drift=${fmt(steps.drift)}  maxConcurrent=${d.maxConcurrentRunning}`);
}

console.log(`\n${real === 0 ? "PASS — parallel artifacts identical to serial modulo the serial noise set" : `FAIL — ${real} artifact(s) with real differences`}`);
process.exit(real === 0 ? 0 : 1);
