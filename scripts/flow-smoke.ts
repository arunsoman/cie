// Smoke test: buildFlowSpec at all four selection levels against the real
// demo report (public/data/report.json). Verifies: every level returns a
// spec with nodes, ids resolve, no dangling link endpoints, honest empties.
import { readFileSync } from "fs";
import { buildFlowSpec, ATLAS_RESET, type AtlasSelection } from "/home/z/my-project/web/src/lib/atlas";
import type { CieReport } from "/home/z/my-project/web/src/lib/cie-types";

const report = JSON.parse(readFileSync("/home/z/my-project/web/public/data/report.json", "utf8")) as CieReport;
console.log("files:", report.files.length, "symbols:", report.symbols.length, "summaries:", report.summaries.length);

function check(label: string, sel: AtlasSelection) {
  const b = buildFlowSpec(report, sel);
  if (!b.spec) {
    console.log(`[${label}] level=${b.level} EMPTY — honest, ok`);
    return;
  }
  const nodes = b.spec.nodes ?? [];
  const links = b.spec.links ?? [];
  const ids = new Set(nodes.map((n) => n.id));
  const dangling = links.filter((l) => !ids.has(l.from) || !ids.has(l.to));
  console.log(`[${label}] level=${b.level} nodes=${nodes.length} links=${links.length} dangling=${dangling.length} title="${b.spec.title}"`);
  if (dangling.length) console.log("   DANGLING:", dangling.map((d) => `${d.from}->${d.to}`).join(", "));
}

// 1. system
check("system", ATLAS_RESET);

// 2. container: first real directory (first segment of a file that has one)
const folders = [...new Set(report.files.filter((f) => f.includes("/")).map((f) => f.split("/")[0]))];
const container = folders.length ? folders[0] : "(project root)";
check(`container:${container}`, { primary: { kind: "container", id: container, label: container }, scope: "container" });

// 3. module: a file with several symbols
const fileCounts = new Map<string, number>();
for (const s of report.symbols) fileCounts.set(report.files[s.loc.file] ?? "", (fileCounts.get(report.files[s.loc.file] ?? "") ?? 0) + 1);
const module = [...fileCounts.entries()].sort((a, b) => b[1] - a[1])[0][0];
check(`module:${module}`, { primary: { kind: "module", id: module, label: module }, scope: "module" });

// 4. symbol: the symbol with the most recorded callees
const withCallees = report.summaries.filter((s) => (s.callees ?? []).length > 0).sort((a, b) => b.callees.length - a.callees.length)[0];
const sym = report.symbols.find((s) => Number(s.id) === Number(withCallees.sym))!;
const qual = sym.qual_name || sym.name;
check(`symbol:${qual}`, { primary: { kind: "symbol", id: qual, label: qual }, scope: "symbol" });
console.log("   that symbol's callees recorded:", withCallees.callees.length, "loops:", withCallees.has_loops, "failure:", withCallees.has_failure_path);

// 5. store fallback honesty
check("store:db", { primary: { kind: "store", id: "store:db", label: "db" }, scope: "module" });

// 6. a symbol with NO summary (honest start→end)
const noSum = report.symbols.find((s) => s.kind !== "Module" && !report.summaries.some((u) => Number(u.sym) === Number(s.id)));
if (noSum) check(`symbol-no-summary:${noSum.qual_name || noSum.name}`, { primary: { kind: "symbol", id: noSum.qual_name || noSum.name, label: noSum.name }, scope: "symbol" });
