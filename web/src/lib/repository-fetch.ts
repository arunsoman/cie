// Keep legacy UI requests pinned to the revision loaded into the workspace.
import { useCie } from './cie-store';
export function repositoryFetch(url: string, init: RequestInit): Promise<Response> {
  if (typeof init.body === 'string') {
    const body = JSON.parse(init.body);
    const state = useCie.getState();
    if (state.report && state.sourceRoot && (body.root === state.sourceRoot || body.sourceRoot === state.sourceRoot)) {
      init = { ...init, body: JSON.stringify({ ...body, revision: String(state.report.rev) }) };
    }
  }
  return fetch(url, init);
}
