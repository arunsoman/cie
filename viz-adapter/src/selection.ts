/**
 * RFC-0024 patch 1 — the loop-guarded selection hub (ux-spec §2.3, §13.1).
 *
 * Every engine's RenderHandle embeds one of these so the patch-1 semantics
 * are implemented EXACTLY once:
 *   - `setSelected` with an identical set never re-fires listeners (echo
 *     suppression — the engine's own click-echo cannot loop);
 *   - a listener that calls `setSelected` re-entrantly is COALESCED, not
 *     recursed (depth is capped at one deferred pass; a re-entrant call
 *     that would not change the set is dropped silently);
 *   - every callback carries its `origin` ('user' | 'programmatic') so the
 *     conductor can distinguish a human click from a host-driven move
 *     (chips, plan, cross-filter).
 *
 * Headless-pure: no DOM, no engine import — vitest-suite-testable.
 */

import type { SelectionListener, SelectionOrigin } from "./types";

export interface SelectionHub {
  /** Subscribe; returns the unsubscribe function. */
  subscribe(cb: SelectionListener): () => void;
  /** Move the selection (loop-guarded; see module doc). */
  setSelected(ids: string[], origin: SelectionOrigin): void;
  /** Current selection in pick order. */
  selected(): string[];
  /** Diagnostics for the G4 footer disclosure. */
  listenerCount(): number;
  /** Re-entrant calls dropped by the guard (disclosed, never silent). */
  droppedReentrant(): number;
  /** Echo-suppressed identical sets (disclosed, never silent). */
  suppressedEchoes(): number;
}

/** Ordered-set equality: same members, same order ⇒ no-op. */
function sameSet(a: string[], b: string[]): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}

export function createSelectionHub(): SelectionHub {
  let listeners: SelectionListener[] = [];
  let current: string[] = [];
  let notifying = false;
  let pending: { ids: string[]; origin: SelectionOrigin } | null = null;
  let dropped = 0;
  let suppressed = 0;

  const hub: SelectionHub = {
    subscribe(cb) {
      listeners.push(cb);
      return () => {
        listeners = listeners.filter((l) => l !== cb);
      };
    },

    setSelected(ids, origin) {
      // Loop guard 1: re-entrant call from inside a listener — coalesce
      // into ONE deferred pass instead of recursing (depth cap = 1).
      // Checked BEFORE the echo guard: a re-entrant echo is still
      // re-entrancy, and dropping it keeps the loop impossible.
      if (notifying) {
        dropped += 1;
        pending = { ids: [...ids], origin };
        return;
      }
      // Loop guard 2: an identical set is an echo — never re-fire.
      if (sameSet(ids, current)) {
        suppressed += 1;
        return;
      }

      const notify = (value: string[], source: SelectionOrigin) => {
        current = [...value];
        notifying = true;
        try {
          for (const l of [...listeners]) l([...current], source);
        } finally {
          notifying = false;
        }
      };
      notify(ids, origin);
      // Bound conflicting listeners to ONE deferred notification.
      const next = pending;
      pending = null;
      if (next && !sameSet(next.ids, current)) notify(next.ids, next.origin);
      pending = null;
    },

    selected() {
      return [...current];
    },

    listenerCount() {
      return listeners.length;
    },

    droppedReentrant() {
      return dropped;
    },

    suppressedEchoes() {
      return suppressed;
    },
  };

  return hub;
}
