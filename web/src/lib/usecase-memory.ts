// The Usecases section's last story pick, per workspace — a preference, not
// a fact. Restoring it only ever changes WHICH story the section opens on;
// it never touches what the engine recorded, and a pick that no longer
// exists (re-analysis renumbered the concepts) is ignored by the caller's
// validity check, which falls back to the deterministic default.
//
// Everything here fails soft on purpose: a private-mode browser with no
// localStorage, a corrupted record, or an empty workspace key all degrade
// to "no remembered pick" — the section's default story is fine.

const KEY = "cie.usecase.last";

export function rememberUsecasePick(wsKey: string, conceptId: number): void {
  if (!wsKey) return;
  try {
    const all = readAll();
    all[wsKey] = conceptId;
    window.localStorage.setItem(KEY, JSON.stringify(all));
  } catch {
    /* private-mode quota — the deterministic default is fine */
  }
}

export function readUsecasePick(wsKey: string): number | null {
  if (!wsKey) return null;
  const v = readAll()[wsKey];
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

function readAll(): Record<string, number> {
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return {};
    const v: unknown = JSON.parse(raw);
    if (!v || typeof v !== "object" || Array.isArray(v)) return {};
    const out: Record<string, number> = {};
    for (const [k, n] of Object.entries(v as Record<string, unknown>)) {
      if (k && typeof n === "number" && Number.isFinite(n)) out[k] = n;
    }
    return out;
  } catch {
    return {};
  }
}
