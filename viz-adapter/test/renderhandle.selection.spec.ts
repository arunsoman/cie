/**
 * RFC-0024 patch 1 — the loop-guarded selection hub
 * (renderhandle.selection.spec.ts). The patch's semantics hold EXACTLY
 * once, in selection.ts; every engine's RenderHandle embeds it.
 */

import { describe, expect, it } from 'vitest';
import { createSelectionHub } from '../src/selection.js';
import type { SelectionListener, SelectionOrigin } from '../src/index.js';

function collector(log: Array<[string[], SelectionOrigin]>): SelectionListener {
  return (ids, origin) => log.push([ids, origin]);
}

describe('selection hub — RFC-0024 patch 1 (RenderHandle.onSelection / setSelected)', () => {
  it('notifies subscribers with the ids and the origin', () => {
    const hub = createSelectionHub();
    const log: Array<[string[], SelectionOrigin]> = [];
    hub.subscribe(collector(log));
    hub.setSelected(['a', 'b'], 'user');
    expect(log).toEqual([[['a', 'b'], 'user']]);
    expect(hub.selected()).toEqual(['a', 'b']);
  });

  it('unsubscribe stops the callbacks', () => {
    const hub = createSelectionHub();
    const log: Array<[string[], SelectionOrigin]> = [];
    const off = hub.subscribe(collector(log));
    hub.setSelected(['a'], 'user');
    off();
    hub.setSelected(['b'], 'user');
    expect(log).toEqual([[['a'], 'user']]);
    expect(hub.listenerCount()).toBe(0);
  });

  it('loop guard: re-setting an IDENTICAL set never re-fires (echo suppression)', () => {
    const hub = createSelectionHub();
    const log: Array<[string[], SelectionOrigin]> = [];
    hub.subscribe(collector(log));
    hub.setSelected(['a', 'b'], 'user');
    hub.setSelected(['a', 'b'], 'user'); // the engine's own click-echo
    expect(log.length).toBe(1);
    expect(hub.suppressedEchoes()).toBe(1);
  });

  it('a DIFFERENT order is a different set and does fire', () => {
    const hub = createSelectionHub();
    const log: Array<[string[], SelectionOrigin]> = [];
    hub.subscribe(collector(log));
    hub.setSelected(['a', 'b'], 'user');
    hub.setSelected(['b', 'a'], 'user');
    expect(log.length).toBe(2);
  });

  it('loop guard: re-entrant setSelected from a listener is coalesced, not recursed', () => {
    const hub = createSelectionHub();
    const log: Array<[string[], SelectionOrigin]> = [];
    let reentered = false;
    hub.subscribe((ids, origin) => {
      log.push([ids, origin]);
      if (!reentered) {
        reentered = true;
        hub.setSelected(['c'], 'programmatic'); // would loop unguarded
      }
    });
    hub.setSelected(['a'], 'user');
    // first fire: ['a']; ONE deferred pass: ['c'] — never a second ['c'].
    expect(log.map(([ids]) => ids)).toEqual([['a'], ['c']]);
    expect(hub.droppedReentrant()).toBe(1);
    expect(hub.selected()).toEqual(['c']);
  });

  it('a re-entrant call that would NOT change the set is dropped silently', () => {
    const hub = createSelectionHub();
    const log: Array<[string[], SelectionOrigin]> = [];
    hub.subscribe((ids) => {
      log.push([ids, 'user']);
      hub.setSelected(ids, 'user'); // echo of what is being set
    });
    hub.setSelected(['a'], 'user');
    expect(log.length).toBe(1);
    expect(hub.droppedReentrant()).toBe(1);
  });

  it('origin passes through verbatim: user vs programmatic', () => {
    const hub = createSelectionHub();
    const log: Array<[string[], SelectionOrigin]> = [];
    hub.subscribe(collector(log));
    hub.setSelected(['chip-1'], 'programmatic');
    hub.setSelected(['chip-2'], 'user');
    expect(log.map(([, o]) => o)).toEqual(['programmatic', 'user']);
  });

  it('multiple listeners all fire; removing one keeps the other', () => {
    const hub = createSelectionHub();
    const logA: string[][] = [];
    const logB: string[][] = [];
    hub.subscribe((ids) => logA.push(ids));
    const offB = hub.subscribe((ids) => logB.push(ids));
    hub.setSelected(['x'], 'user');
    offB();
    hub.setSelected(['y'], 'user');
    expect(logA).toEqual([['x'], ['y']]);
    expect(logB).toEqual([['x']]);
  });
});
