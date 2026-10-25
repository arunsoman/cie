import { StorageClient, output, type EntityPage, storageTransport } from '../storage-client';
import type { AtlasPin } from './atlas-types';
import type { ChangeContext } from './atlas-workspace';
export async function atlasRevisions(root: string) { return storageTransport<{
    items: {
        revision: string;
        head: boolean;
    }[];
    total: number;
}>({ op: 'revisions', root, limit: 100 }); }
/** Compare captured hashes, not today's working tree. Missing hashes are Unknown. */
export async function compareAtlasRevisions(root: string, base: AtlasPin, head: AtlasPin, signal: AbortSignal, maxFiles = 20000): Promise<ChangeContext> {
    if (base.repository !== head.repository)
        throw new Error('Cannot compare different repositories');
    const read = async (pin: AtlasPin) => {
        const client = new StorageClient(root, pin.revision), files = new Map<string, string | null>();
        let continuation: string | null = null;
        try {
            do {
                if (signal.aborted)
                    throw new Error('Comparison cancelled');
                const response = await client.retrieve({ query: { select: { kind: 'file' } }, outputs: [{ id: 'rows', output: { entities: { page: { limit: 500, continuation } } } }] }, signal);
                if (response.repository !== pin.repository)
                    throw new Error('Comparison repository mismatch');
                const page = output<EntityPage>(response, 'rows', 'entities');
                for (const file of page.items) {
                    files.set(String(file.fields.path), typeof file.fields.content_hash === 'string' ? file.fields.content_hash : null);
                    if (files.size > maxFiles)
                        throw new Error('Comparison exceeds bounded file budget; request a storage-side comparison');
                }
                continuation = page.continuation;
            } while (continuation);
            return files;
        }
        finally {
            await client.close();
        }
    };
    const [a, b] = await Promise.all([read(base), read(head)]), changedFiles: string[] = [], deletedFiles: string[] = [], unknownFiles: string[] = [];
    for (const [file, hash] of b) {
        if (!a.has(file))
            changedFiles.push(file);
        else if (hash == null || a.get(file) == null)
            unknownFiles.push(file);
        else if (hash !== a.get(file))
            changedFiles.push(file);
    }
    for (const file of a.keys())
        if (!b.has(file))
            deletedFiles.push(file);
    return { base, head, changedFiles, deletedFiles, unknownFiles };
}
