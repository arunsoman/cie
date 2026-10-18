// One-tap charts: pre-canned, deterministic visuals built straight from the
// loaded CIE data. The user picks a question from the Menu; the chart renders
// instantly — no typing, no AI round-trip. Every spec uses the same engine +
// layout disclosure (US-020) and the same renderer as Ask-CIE charts, so the
// fisheye lens (US-023), keyboard parity (US-140) and never-colour-alone
// labels (US-142) all carry over unchanged.

import type { CieReport, CieViewSpec, ConceptTree, Finding } from "./cie-types";
import { engineFor, layoutFor, type ChatChartKind, type ChatChartSpec } from "./chat-chart";
import {
  findingTitle,
  obligationTitle,
  authorityPlain,
  certaintyPlain,
  effectPlain,
  elementKindPlain,
  severityPlain,
} from "./plain-words";

export interface CannedChart {
  id: string;
  group: "Problems" | "Promises" | "The map" | "What the code does";
  question: string;
  hint: string; // one line shown under the question chip
  icon: "list" | "gauge" | "shapes" | "shield" | "map" | "activity";
  spec: ChatChartSpec | null; // null → honest empty state (not enough data)
}

function cut(s: string, n: number): string {
  return s.length > n ? s.slice(0, n - 1) + "…" : s;
}

function spec(kind: ChatChartKind, title: string, tell: string): ChatChartSpec {
  // engine + layout always disclosed (US-020), same as the AI-picked charts.
  return { kind, engine: engineFor(kind), layout: layoutFor(kind), title, tell };
}

function rows_max(m: Map<number, number>): number {
  return Math.max(1, ...m.values());
}

export function buildCannedCharts(
  report: CieReport | null,
  viewspec: CieViewSpec | null,
  concepts: ConceptTree | null
): CannedChart[] {
  if (!report) return [];
  const charts: CannedChart[] = [];

  // ---------- Problems ----------

  // 1. Which files have the most problems?
  {
    const byFile = new Map<number, number>();
    for (const f of report.findings) {
      const sym = report.symbols.find((s) => s.id === f.symbol);
      if (sym) byFile.set(sym.loc.file, (byFile.get(sym.loc.file) ?? 0) + 1);
    }
    const max = rows_max(byFile);
    const data = [...byFile.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 8)
      .map(([idx, v]) => {
        const path = report.files[idx] ?? `File ${idx}`;
        return {
          name: cut(path.split("/").pop() ?? path, 19),
          value: v,
          color: v === max ? "rose" : v >= max / 2 ? "amber" : "stone",
        };
      });
    charts.push({
      id: "files-most-problems",
      group: "Problems",
      question: "Which files have the most problems?",
      hint: "Bars sorted worst first — start at the top.",
      icon: "list",
      spec: data.length
        ? { ...spec("hbar", "Problems per file", "Each bar counts the things CIE flagged in that file."), data }
        : null,
    });
  }

  // 2. How urgent are the problems?
  {
    const order = ["Critical", "High", "Medium", "Low"] as const;
    const palette: Record<string, string> = { Critical: "rose", High: "amber", Medium: "stone", Low: "emerald" };
    const data = order
      .map((sev) => ({
        name: severityPlain(sev).label,
        value: report.findings.filter((f) => f.severity === sev).length,
        color: palette[sev],
      }))
      .filter((d) => d.value > 0);
    charts.push({
      id: "urgency-donut",
      group: "Problems",
      question: "How urgent are the problems?",
      hint: "Rose means soon; green can wait.",
      icon: "gauge",
      spec: data.length
        ? {
            ...spec("donut", "Problems by urgency", "The same findings, sorted by how soon they deserve a look."),
            data,
            center: "problems",
          }
        : null,
    });
  }

  // 3. What kinds of problems are there?
  {
    const byKind = new Map<string, number>();
    for (const f of report.findings) byKind.set(f.kind, (byKind.get(f.kind) ?? 0) + 1);
    const fake = (kind: string) =>
      ({ kind, id: "", severity: "Low", symbol: 0, symbol_name: "", message: "", evidence_lines: [], remediation: "" } as Finding);
    const max = Math.max(1, ...byKind.values());
    const data = [...byKind.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 8)
      .map(([kind, v]) => ({
        name: cut(findingTitle(fake(kind)), 19),
        value: v,
        color: v === max ? "rose" : "emerald",
      }));
    charts.push({
      id: "problem-kinds",
      group: "Problems",
      question: "What kinds of problems are there?",
      hint: "The same kind of trouble, counted.",
      icon: "shapes",
      spec: data.length
        ? { ...spec("hbar", "Kinds of trouble", "Every kind of problem CIE knows how to name, with counts."), data }
        : null,
    });
  }

  // 4. How much needs fixing right away?
  {
    const soon = report.findings.filter((f) => f.severity === "Critical" || f.severity === "High").length;
    charts.push({
      id: "fix-right-away",
      group: "Problems",
      question: "How much needs fixing right away?",
      hint: "The honest headline number.",
      icon: "shield",
      spec: soon + report.findings.length > 0
        ? {
            ...spec("stat", "Needs fixing soon", "Urgent problems out of everything CIE found."),
            data: [{ name: `out of ${report.findings.length} ${report.findings.length === 1 ? "thing" : "things"} CIE found in total`, value: soon }],
            unit: soon === 1 ? "thing" : "things",
          }
        : null,
    });
  }

  // 5. How much is completely clean?
  {
    const problemFiles = new Set<number>();
    for (const f of report.findings) {
      const sym = report.symbols.find((s) => s.id === f.symbol);
      if (sym) problemFiles.add(sym.loc.file);
    }
    const clean = report.files.filter((_, i) => !problemFiles.has(i)).length;
    charts.push({
      id: "clean-files",
      group: "Problems",
      question: "How much is completely clean?",
      hint: "Files with nothing flagged at all.",
      icon: "shield",
      spec:
        report.files.length > 0
          ? {
              ...spec("stat", "Spotless files", "Files where CIE found nothing to flag."),
              data: [
                {
                  name: `of ${report.files.length} ${report.files.length === 1 ? "file" : "files"} have no problems at all`,
                  value: clean,
                },
              ],
              unit: clean === 1 ? "file" : "files",
            }
          : null,
    });
  }

  // ---------- Promises ----------

  // 6. Who wrote the promises?
  {
    const written = report.contracts.filter((c) => c.authority === "Explicit").length;
    const guessed = report.contracts.length - written;
    const data = [
      { name: authorityPlain("Explicit").label, value: written, color: "emerald" },
      { name: authorityPlain("InferredCandidate").label, value: guessed, color: "amber" },
    ].filter((d) => d.value > 0);
    charts.push({
      id: "who-wrote-promises",
      group: "Promises",
      question: "Who wrote the promises?",
      hint: "Green ones come from your own code notes.",
      icon: "shield",
      spec: data.length
        ? {
            ...spec("donut", "Promises by author", "Green promises are written down by the developers; amber ones are CIE's guesses."),
            data,
            center: "promises",
          }
        : null,
    });
  }

  // 7. The promises, listed
  {
    const rows = report.contracts.slice(0, 8).map((c) => [
      cut(obligationTitle(c.obligation), 34),
      c.authority === "Explicit" ? "written by you" : "CIE's guess",
      `${c.symbols.length} ${c.symbols.length === 1 ? "part" : "parts"} must keep it`,
    ]);
    charts.push({
      id: "promise-list",
      group: "Promises",
      question: "What are the promises?",
      hint: "Each one must always hold true.",
      icon: "list",
      spec: rows.length
        ? {
            ...spec("table", "Every promise CIE found", "A promise is something the code must always do."),
            columns: ["The promise", "Who wrote it", "Who keeps it"],
            rows,
          }
        : null,
    });
  }

  // ---------- The map ----------

  // 8. How sure is the map?
  if (viewspec) {
    const tally = new Map<string, number>();
    for (const e of [...viewspec.elements, ...viewspec.edges]) tally.set(e.displayMode, (tally.get(e.displayMode) ?? 0) + 1);
    const palette: Record<string, string> = { Fact: "emerald", Inference: "amber", Hypothesis: "stone" };
    const data = (["Fact", "Inference", "Hypothesis"] as const)
      .map((m) => ({ name: certaintyPlain(m).label, value: tally.get(m) ?? 0, color: palette[m] }))
      .filter((d) => d.value > 0);
    const total = data.reduce((a, d) => a + d.value, 0);
    charts.push({
      id: "map-certainty",
      group: "The map",
      question: "How sure is CIE about the map?",
      hint: "Green is proven; amber is a good guess.",
      icon: "map",
      spec: total
        ? {
            ...spec("donut", "Certainty on the map", "Every bubble and connection, sorted by how CIE knows it."),
            data,
            center: "on the map",
          }
        : null,
    });
  }

  // 9. What kinds of things are on the map?
  if (viewspec) {
    const tally = new Map<string, number>();
    for (const e of viewspec.elements) tally.set(e.elementKind, (tally.get(e.elementKind) ?? 0) + 1);
    const data = [...tally.entries()]
      .sort((a, b) => b[1] - a[1])
      .map(([k, v]) => ({ name: cut(elementKindPlain(k), 26), value: v, color: k === "domain" ? "emerald" : k === "module" ? "amber" : "stone" }));
    const total = data.reduce((a, d) => a + d.value, 0);
    charts.push({
      id: "map-kinds",
      group: "The map",
      question: "What kinds of things are on the map?",
      hint: "Files, behaviours, rules — and the big picture.",
      icon: "map",
      spec: total
        ? { ...spec("donut", "What the map is made of", "Each slice is a kind of bubble on the Map page."), data, center: "bubbles" }
        : null,
    });
  }

  // 10. How much did CIE check?
  {
    const cov = report.coverage;
    const pct = cov.files_total ? Math.round((cov.files_analyzed / cov.files_total) * 100) : 0;
    charts.push({
      id: "coverage",
      group: "The map",
      question: "How much did CIE check?",
      hint: "The honest coverage number.",
      icon: "gauge",
      spec: cov.files_total
        ? {
            ...spec("stat", "Files checked", `CIE read ${cov.files_analyzed} of ${cov.files_total} files, and ${cov.symbols_analyzed} of ${cov.symbols_total} named things inside them.`),
            data: [{ name: cov.complete ? "everything got a full read — nothing was skipped" : "some parts were skipped — the Trust page names them", value: pct }],
            unit: "%",
          }
        : null,
    });
  }

  // ---------- What the code does ----------

  // 11. What does the code touch outside itself?
  {
    const byEffect = new Map<string, number>();
    for (const s of report.summaries) for (const e of s.effects ?? []) byEffect.set(e, (byEffect.get(e) ?? 0) + 1);
    const max = Math.max(1, ...byEffect.values());
    const data = [...byEffect.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 8)
      .map(([e, v]) => ({ name: cut(effectPlain(e), 26), value: v, color: e === "Crypto" || e === "DbWrite" ? "amber" : "emerald" }));
    charts.push({
      id: "outside-world",
      group: "What the code does",
      question: "What does the code touch outside itself?",
      hint: "Databases, printers, secrets — the real-world effects.",
      icon: "activity",
      spec: data.length
        ? { ...spec("hbar", "Touches to the outside world", "How many named things do each of these."), data }
        : null,
    });
  }

  // 12. Which files have the most named things?
  {
    const byFile = new Map<number, number>();
    for (const s of report.symbols) byFile.set(s.loc.file, (byFile.get(s.loc.file) ?? 0) + 1);
    const max = rows_max(byFile);
    const data = [...byFile.entries()]
      .sort((a, b) => b[1] - a[1])
      .slice(0, 8)
      .map(([idx, v]) => {
        const path = report.files[idx] ?? `File ${idx}`;
        return { name: cut(path.split("/").pop() ?? path, 19), value: v, color: v === max ? "emerald" : "stone" };
      });
    charts.push({
      id: "things-per-file",
      group: "What the code does",
      question: "Which files have the most named things?",
      hint: "Bigger files do more — and have more to check.",
      icon: "list",
      spec: data.length
        ? { ...spec("hbar", "Named things per file", "Functions, classes and modules with a name, counted per file."), data }
        : null,
    });
  }

  // 13. What does each file do?
  if (concepts) {
    const modules = concepts.nodes.filter((n) => n.kind === "Module").slice(0, 8);
    const rows = modules.map((m) => [
      cut(m.label, 22),
      cut(
        m.behaviors.map((b) => b.kind.replace(/_/g, " ").toLowerCase()).join(", ") || "nothing notable",
        48
      ),
    ]);
    charts.push({
      id: "file-does-what",
      group: "What the code does",
      question: "What does each file do?",
      hint: "Plain answers, straight from the Concepts page.",
      icon: "shapes",
      spec: rows.length
        ? {
            ...spec("table", "Each file, in plain words", "The behaviours CIE recognised in each file."),
            columns: ["File", "What it does"],
            rows,
          }
        : null,
    });
  }

  return charts;
}
