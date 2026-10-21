import type { RenderHandle } from './types';
/** One sheet, many handles. Only user events change application scope. */
export class SelectionConductor {
  private handles = new Map<RenderHandle, Set<string>>();
  attach(handle: RenderHandle, ids: Iterable<string>, onUser: (ids: string[]) => void): () => void {
    this.handles.set(handle, new Set(ids));
    const off = handle.onSelection((selection, origin) => {
      if (origin !== 'user') return;
      const allowed = this.handles.get(handle)!;
      const selected = selection.filter(id => allowed.has(id));
      this.select(selected);
      onUser(selected);
    });
    return () => { off(); this.handles.delete(handle); };
  }
  select(ids: string[]): void {
    for (const [handle, allowed] of this.handles) handle.setSelected(ids.filter(id => allowed.has(id)), 'programmatic');
  }
}
