import { StorageClient, output } from '../storage-client';
import type { AtlasStorage, AtlasQuery, CapturedEvidence } from './atlas-types';
// No budget on a request falls back to the Rust side's own defaults
// (timeout_ms: 2000, max_response_bytes: 4MB) — far under the 30s/16MB
// ceiling the probe's own comment below assumes. Every Atlas call needs
// the raised ceiling explicitly; nothing here was setting it.
const BUDGET = { max_response_bytes: 16 * 1024 * 1024, timeout_ms: 30000, max_visited: 1000000 };
/** Transport only; selection/projection logic never reaches into physical storage. */
export function atlasStorage(root: string, revision: string): AtlasStorage {
    const client = new StorageClient(root, revision);
    return {
        async retrieve(query, outputs, signal) { return client.retrieve({ query, outputs, best_effort: true, budget: BUDGET }, signal); },
        async source(evidence, signal) {
            const range = evidence.startByte == null ? 'whole_file' : { bytes: { start: evidence.startByte, end: evidence.endByte } };
            const query: AtlasQuery = { select: { kind: 'file', filter: { field: { field: 'path', op: 'eq', value: evidence.file } } } };
            const reader = evidence.pin && evidence.pin.revision !== revision ? new StorageClient(root, evidence.pin.revision) : client;
            try {
                const result = await reader.retrieve({ query, outputs: [{ id: 'source', output: { detail: { source: { range } } } }], budget: BUDGET }, signal);
                if (!result.matches)
                    throw new Error('No captured source in this revision');
                const source = { ...output<CapturedEvidence & {
                        line_starts?: number[];
                    }>(result, 'source', 'source'), repository: result.repository };
                if (evidence.line != null && evidence.startByte == null) {
                    const offsets = source.line_starts ?? [];
                    const point = offsets[evidence.line - 1];
                    if (point == null || point < source.returned.start || point >= source.returned.end)
                        throw new Error('Requested evidence line was not returned by storage');
                    const start = Math.max(source.returned.start, offsets[Math.max(0, evidence.line - 4)] ?? point), end = Math.min(source.returned.end, offsets[evidence.line + 3] ?? source.returned.end);
                    source.text = new TextDecoder().decode(new TextEncoder().encode(source.text).slice(start - source.returned.start, end - source.returned.start));
                    source.returned = { start, end };
                }
                return source;
            }
            finally {
                if (reader !== client)
                    await reader.close();
            }
        },
        close: () => client.close(),
    };
}
