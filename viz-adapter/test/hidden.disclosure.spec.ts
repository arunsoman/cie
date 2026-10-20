/**
 * RFC-0024 patch 2 + G1 — the hidden-ledger disclosure rows
 * (hidden.disclosure.spec.ts). The grouped-out box (`…and N more`) is
 * never a scope: it opens a popup listing rows built from `hidden[]`,
 * each row acting as its own element.
 */

import { describe, expect, it } from 'vitest';
import { hiddenRows } from '../src/disclosure.js';
import type { HiddenCandidate } from '../src/index.js';

describe('hiddenRows — RFC-0024 patch 2 (G1 disclosure popup rows)', () => {
  it('uses label/kind when the producer recorded them (patch 2)', () => {
    const rows = hiddenRows([
      { id: 'svc-notif', reason: 'below salience cut', salienceWeight: 0.2, label: 'notification-service', kind: 'container' },
    ]);
    expect(rows[0]).toMatchObject({
      id: 'svc-notif',
      label: 'notification-service',
      kind: 'container',
      reason: 'below salience cut',
      fallback: false,
    });
  });

  it('falls back to id/unknown when label/kind are absent — and discloses it', () => {
    const rows = hiddenRows([{ id: 'legacy-billing', reason: 'no evidence ref', salienceWeight: 0.1 }]);
    expect(rows[0]).toMatchObject({
      id: 'legacy-billing',
      label: 'legacy-billing',
      kind: 'unknown',
      fallback: true,
    });
  });

  it('rows keep the ledger order and the reason (the G1 popup never reorders)', () => {
    const hidden: HiddenCandidate[] = [
      { id: 'a', reason: 'r-a', salienceWeight: 0.9, label: 'A', kind: 'module' },
      { id: 'b', reason: 'r-b', salienceWeight: 0.5, label: 'B', kind: 'symbol' },
      { id: 'c', reason: 'r-c', salienceWeight: 0.1, label: 'C', kind: 'edge' },
    ];
    expect(hiddenRows(hidden).map((r) => r.id)).toEqual(['a', 'b', 'c']);
    expect(hiddenRows(hidden).map((r) => r.reason)).toEqual(['r-a', 'r-b', 'r-c']);
  });

  it('an empty ledger yields an empty row list (the popup then says so, honestly)', () => {
    expect(hiddenRows([])).toEqual([]);
  });

  it('mixed ledger marks only the rows that actually fell back', () => {
    const rows = hiddenRows([
      { id: 'x', reason: 'rx', salienceWeight: 0.4, label: 'X', kind: 'container' },
      { id: 'y', reason: 'ry', salienceWeight: 0.3 }, // no patch-2 fields
    ]);
    expect(rows.map((r) => r.fallback)).toEqual([false, true]);
  });
});
