/**
 * RFC-0024 stage 1 — the HOSTED render pipeline (renderHosted).
 * Same negotiation → gate → adapt → configure sequence as
 * `renderHeadless` (VIZ-09), then the host-injected render call. The
 * fake host here records the exact call it receives — proving the
 * adapter-prepared data/options and the gate still stand between the
 * spec and any pixels.
 */

import { describe, expect, it } from 'vitest';
import { renderHosted, defaultRegistry } from '../src/index.js';
import { createSelectionHub } from '../src/selection.js';
import type {
  HostRenderRuntime,
  LayoutOutcome,
  RenderHandle,
  ViewSpec,
} from '../src/index.js';
import architecture from '../fixtures/specs/architecture.json';

const spec = architecture as unknown as ViewSpec;

function fakeHost(log: string[], failGate = false): HostRenderRuntime {
  return {
    async render(engineId, container, s, options) {
      log.push(`render:${engineId}:${(container as HTMLElement).id}:${Object.keys(options as Record<string, unknown>).join('+')}`);
      if (failGate) throw new Error('host exploded');
      const hub = createSelectionHub();
      const handle: RenderHandle = {
        engineId,
        semanticHash: s.semanticHash,
        renderHash: 'fake',
        layout: { requested: undefined, actual: 'dagre', fidelity: 'Exact' } as LayoutOutcome,
        onSelection: (cb) => hub.subscribe(cb),
        setSelected: (ids, origin) => hub.setSelected(ids, origin),
        dispose: async () => {
          log.push('dispose');
        },
      };
      return handle;
    },
  };
}

describe('renderHosted — RFC-0024 stage 1', () => {
  it('negotiates → gates → adapts → configures → calls the host render', async () => {
    const log: string[] = [];
    const container = { id: 'host-1' } as unknown as HTMLElement;
    const result = await renderHosted(
      spec,
      { required: {} },
      defaultRegistry(),
      container,
      {},
      fakeHost(log),
    );
    expect(result.engineId).toBe('g6');
    expect(log[0]).toContain('render:g6:host-1:');
    // the host receives the ADAPTER-PREPARED options (data included)
    const opts = result.options as Record<string, unknown>;
    expect(opts).toHaveProperty('data');
    expect(opts).toHaveProperty('layout');
    // the handle comes straight from the host, disclosure intact
    expect(result.handle.engineId).toBe('g6');
    expect(result.disclosure.headline).toBe('Rendered: g6');
  });

  it('still refuses an ungated spec — the host is never the honesty bypass', async () => {
    const broken = JSON.parse(JSON.stringify(spec)) as ViewSpec;
    (broken.elements[0] as { evidence: unknown }).evidence = undefined;
    await expect(
      renderHosted(
        broken,
        { required: {} },
        defaultRegistry(),
        { id: 'x' } as unknown as HTMLElement,
        {},
        fakeHost([]),
      ),
    ).rejects.toThrowError(/ELEMENT_WITHOUT_EVIDENCE/);
  });

  it('handle selection surface works end-to-end (patch 1 on the hosted path)', async () => {
    const log: string[] = [];
    const result = await renderHosted(
      spec,
      { required: {} },
      defaultRegistry(),
      { id: 'host-2' } as unknown as HTMLElement,
      {},
      fakeHost(log),
    );
    const seen: string[][] = [];
    const off = result.handle.onSelection((ids) => seen.push(ids));
    result.handle.setSelected(['checkout'], 'user');
    result.handle.setSelected(['checkout'], 'user'); // echo suppressed
    off();
    result.handle.setSelected(['authorize'], 'programmatic');
    expect(seen).toEqual([['checkout']]);
    await result.handle.dispose();
    expect(log).toContain('dispose');
  });

  it('without a host runtime the adapter render stays a documented throw', async () => {
    const reg = defaultRegistry();
    const g6 = reg.get('g6')!;
    await expect(g6.render({} as HTMLElement, spec, {})).rejects.toThrowError(/host runtime/);
  });
});
