// Builds the compact, plain-words digest of the loaded CIE data that the
// chat backend uses as its single source of truth. Everything is truncated
// and capped so the payload stays small, and every technical term is
// translated with the same plain-words policy as the rest of the UI.

import type { CieReport, CieViewSpec, ConceptTree } from "./cie-types";
import { findingTitle, obligationTitle, effectPlain, certaintyPlain } from "./plain-words";

function cut(s: string, n: number): string {
  const t = String(s ?? "").replace(/\s+/g, " ").trim();
  return t.length > n ? t.slice(0, n - 1) + "…" : t;
}

export function buildDigest(
  report: CieReport | null,
  viewspec: CieViewSpec | null,
  concepts: ConceptTree | null,
  label: string
): Record<string, unknown> | null {
  if (!report) return null;

  const fileName = (idx: number) => report.files?.[idx] ?? `file #${idx}`;
  const sym = (id: number) => report.symbols.find((s) => s.id === id);

  // Findings — plain titles, capped at 40.
  const findings = report.findings.slice(0, 40).map((f) => {
    const s = sym(f.symbol);
    return {
      urgency: f.severity,
      problem: findingTitle(f),
      technical_kind: f.kind,
      where: s ? `${fileName(s.loc.file)} line ${f.evidence_lines?.[0] ?? s.loc.start}` : "unknown location",
      in_code_name: s?.qual_name ?? f.symbol_name,
      says: cut(f.message, 260),
      how_to_fix: cut(f.remediation, 200),
    };
  });

  // Promises (contracts) — all of them, usually a handful.
  const promises = report.contracts.map((c) => ({
    promise: obligationTitle(c.obligation),
    exact_words: cut(c.expr, 140),
    who_wrote_it: c.authority === "Explicit" ? "written by the developers" : "CIE's guess (worth checking)",
    must_be_kept_by: c.symbols
      .map((id) => sym(id)?.qual_name)
      .filter(Boolean)
      .slice(0, 8),
  }));

  // Coverage + trust, verbatim from the report (already human-readable).
  const trust = {
    checked_files: `${report.coverage.files_analyzed} of ${report.coverage.files_total}`,
    checked_named_things: `${report.coverage.symbols_analyzed} of ${report.coverage.symbols_total}`,
    fully_checked: report.coverage.complete,
    notes: report.coverage.notes?.map((n) => cut(n, 200)).slice(0, 10),
    assumptions: report.evidence.assumptions?.map((a) => cut(a, 200)).slice(0, 10) ?? [],
    names_cie_could_not_match: (report.evidence.unresolved ?? []).map((u) => cut(u, 80)).slice(0, 25),
    warnings: (report.warnings ?? []).map((w) => cut(w, 200)).slice(0, 10),
    engine_provenance: cut(report.evidence.provenance, 120),
  };

  // Map summary — counts only; the digest stays small.
  let map = null;
  if (viewspec) {
    const tally = (items: { displayMode: string }[]) => {
      const t = { proven: 0, good_guess: 0, not_sure: 0 };
      for (const it of items) {
        if (it.displayMode === "Fact") t.proven++;
        else if (it.displayMode === "Inference") t.good_guess++;
        else t.not_sure++;
      }
      return t;
    };
    map = {
      what_the_map_shows: "bubbles = parts of your code; connections = how they relate",
      bubbles_by_kind: viewspec.elements.reduce<Record<string, number>>((acc, e) => {
        acc[e.elementKind] = (acc[e.elementKind] ?? 0) + 1;
        return acc;
      }, {}),
      bubbles_by_certainty: tally(viewspec.elements),
      connections_by_certainty: tally(viewspec.edges),
      things_cie_hid: Array.isArray(viewspec.hidden) ? viewspec.hidden.length : 0,
      gaps_cie_admits: Array.isArray(viewspec.gaps) ? viewspec.gaps.length : 0,
      caption: viewspec.footer?.caption ? cut(viewspec.footer.caption, 160) : undefined,
    };
  }

  // Concepts — what each code file does, in plain words.
  let whatItDoes = null;
  if (concepts) {
    const byKind = (kind: string) => concepts.nodes.filter((n) => n.kind === kind);
    whatItDoes = {
      whole_project: byKind("Domain").map((n) => ({
        does: n.behaviors.map((b) => cut(b.kind, 60)).slice(0, 8),
        covers_named_things: n.symbols.length,
      })),
      code_files: byKind("Module").map((n) => ({
        file: n.label,
        does: n.behaviors.map((b) => cut(b.kind, 60)).slice(0, 6),
        rules: (n.rules ?? []).map((r) => cut(String(r.kind ?? "").replace(/_/g, " "), 60)).slice(0, 6),
      })),
    };
  }

  // Effects — what the code touches in the outside world.
  const effectTally: Record<string, number> = {};
  for (const s of report.summaries) {
    for (const e of s.effects ?? []) {
      const k = effectPlain(e);
      effectTally[k] = (effectTally[k] ?? 0) + 1;
    }
  }

  // Certainty vocabulary, so the model uses the same words as the UI.
  const words = {
    Fact: certaintyPlain("Fact").label,
    Inference: certaintyPlain("Inference").label,
    Hypothesis: certaintyPlain("Hypothesis").label,
    Critical: "fix soon",
    High: "please look",
    Medium: "worth checking",
    Low: "good to know",
  };

  return {
    about: `Plain-words health report for: ${label}. Produced by the CIE engine from a direct read of the project (revision ${report.rev}).`,
    certainty_words: words,
    project: {
      files: report.files?.map((f) => f.split("/").pop() ?? f) ?? [],
      named_things_total: report.symbols.length,
      languages: [...new Set(report.symbols.map((s) => s.lang))],
      touches_outside_world: effectTally,
    },
    things_to_look_at: {
      total: report.findings.length,
      fix_soon_first: true,
      list: findings,
    },
    promises,
    coverage_and_trust: trust,
    map,
    what_it_does: whatItDoes,
  };
}
