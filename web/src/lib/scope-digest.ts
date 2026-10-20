// RFC-0024 stage 5 — scopeDigest (web/src/lib/scope-digest.ts).
//
// R5's scoped answering (ux-spec §3-ASK + RFC-0023 §9-R5): a chips-scoped
// question answers ONLY from the selection's code space. The full digest
// is sliced to the chips' space — files ∪ named things ∪ recorded edges —
// and the result carries an honest header scope line ("drawn from N files
// over M named things") plus the chip manifest, so the model (and the G4
// footer) can say exactly what the answer drew from. Nothing here invents:
// a section with nothing in scope stays empty and SAYS so.

import type { ChatContextElement } from "./cie-types";

export interface ScopedDigest {
  scoped: Record<string, unknown>;
  files: string[];
  namedThings: string[];
  scopeLine: string;
}

/** Slice a built digest to the chips' code space. The digest's sections
 *  are plain arrays of objects; the known carriers of code-space identity
 *  are sliced per-field, everything else passes through with the scope
 *  banner attached (the model is told to treat the banner as binding). */
export function scopeDigest(
  digest: Record<string, unknown> | null,
  chips: ChatContextElement[]
): ScopedDigest | null {
  if (!digest) return null;
  if (!chips.length) return { scoped: digest, files: [], namedThings: [], scopeLine: "" };

  const files = new Set<string>();
  const things = new Set<string>();
  for (const c of chips) {
    if (c.loc?.file) files.add(c.loc.file);
    things.add(c.label);
    // a module chip names its file directly; a symbol chip may carry it
    const f = c.label.split("/").length > 1 ? c.label : null;
    if (f) files.add(f);
  }

  const inSpace = (text: unknown): boolean => {
    if (typeof text !== "string") return false;
    if (things.has(text)) return true;
    for (const f of files) if (text === f || text.includes(f)) return true;
    // "file.java line 12" / "file.java:12" carry the file name
    for (const t of things) if (text.includes(t)) return true;
    return false;
  };

  const sliceArray = (arr: unknown[]): unknown[] =>
    arr.filter((item) => {
      if (typeof item === "string") return inSpace(item);
      if (item && typeof item === "object") {
        const o = item as Record<string, unknown>;
        return (
          inSpace(o.where) ||
          inSpace(o.in_code_name) ||
          inSpace(o.file) ||
          inSpace(o.label) ||
          inSpace(o.name) ||
          (Array.isArray(o.must_be_kept_by) && o.must_be_kept_by.some(inSpace)) ||
          (Array.isArray(o.found_in) && o.found_in.some(inSpace))
        );
      }
      return false;
    });

  const out: Record<string, unknown> = { ...digest };
  for (const key of ["findings", "promises", "repeated_behavior", "cochange_facts", "overlap_facts", "drift_facts"]) {
    if (Array.isArray(digest[key])) out[key] = sliceArray(digest[key] as unknown[]);
  }

  const scope = {
    rule: "ONLY the selection's code space — the chips below are the entire world of this question",
    chips: chips.map((c) => ({ label: c.label, kind: c.kind, file: c.loc?.file ?? null })),
    files: [...files],
    named_things: [...things],
  };
  out.selection_scope = scope;

  const scopeLine = `drawn from ${files.size} file(s) over ${things.size} named thing(s) — the selection only`;
  return { scoped: out, files: [...files], namedThings: [...things], scopeLine };
}
