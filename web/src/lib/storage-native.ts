// Server-only in-process worker bridge. No HTTP sidecar or retrieval subprocess.
import type { Worker } from "node:worker_threads";
import path from "node:path";
import { createRequire } from "node:module";
import { randomUUID } from "node:crypto";
// Turbopack specifically pattern-matches a directly-imported `Worker`
// identifier used in `new Worker(...)` to bundle it as a worker entrypoint,
// and its output-file-tracing step panics trying to trace that for this
// dynamic, runtime-resolved worker file (NftJsonAsset: cannot handle
// filepath node:worker_threads). Loading the constructor off the namespace
// import at runtime instead keeps it invisible to that static analysis.
import * as nodeWorkerThreads from "node:worker_threads";
const WorkerCtor = nodeWorkerThreads.Worker;
export class StorageFailure extends Error {
  constructor(public code: string, message: string) { super(message); }
}
type Pending = { resolve: (value: unknown) => void; reject: (error: Error) => void; timer: ReturnType<typeof setTimeout> };
type Bridge = { worker: Worker; pending: Map<number, Pending>; next: number; generation: string };
const globalStorage = globalThis as unknown as { __cieStorageBridge?: Bridge };
function bridge(): Bridge {
  if (globalStorage.__cieStorageBridge) return globalStorage.__cieStorageBridge;
  const worker = Reflect.construct(WorkerCtor, [path.resolve(process.cwd(), "scripts/storage-worker.cjs")]) as Worker;
  worker.unref();
  const value: Bridge = { worker, pending: new Map(), next: 1, generation: randomUUID() };
  const fail = (error: Error) => {
    for (const p of value.pending.values()) { clearTimeout(p.timer); p.reject(error); }
    value.pending.clear();
    if (globalStorage.__cieStorageBridge === value) delete globalStorage.__cieStorageBridge;
  };
  worker.on("message", ({ id, response }) => {
    const pending = value.pending.get(id); if (!pending) return;
    clearTimeout(pending.timer); value.pending.delete(id);
    if (response.ok) pending.resolve(response.result);
    else pending.reject(new StorageFailure(response.error.code, response.error.message));
  });
  worker.on("error", fail);
  worker.on("exit", () => fail(new StorageFailure("CursorExpired", "Storage worker closed; reopen the revision context")));
  globalStorage.__cieStorageBridge = value;
  return value;
}
function cancelRequest(id: string) {
  try {
    const nativeRequire = createRequire(path.join(process.cwd(), 'scripts/storage-worker.cjs'));
    const control = Reflect.apply(nativeRequire, undefined, ['./storage-control.cjs']) as { cancel(id: string): void };
    control.cancel(id);
  } catch {
    // The request still rejects and the native deadline remains active if
    // the control entry is unavailable during shutdown or a deployment change.
  }
}
export function storageCall<T = unknown>(request: Record<string, unknown>, signal?: AbortSignal): Promise<T> {
  if (signal?.aborted) return Promise.reject(new StorageFailure("Cancelled", "Storage request cancelled"));
  const b = bridge(); const id = b.next++;
  const cancellationId = `${b.generation}:${id}`;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', abort);
      b.pending.delete(id);
      reject(new StorageFailure("BudgetExceeded", "Storage worker deadline exceeded"));
      cancelRequest(cancellationId);
    }, 35_000);
    const abort = () => {
      if (!b.pending.delete(id)) return;
      clearTimeout(timer);
      cancelRequest(cancellationId);
      reject(new StorageFailure("Cancelled", "Storage request cancelled"));
    };
    const cleanup = () => signal?.removeEventListener('abort', abort);
    b.pending.set(id, { resolve: value => { cleanup(); resolve(value as T); }, reject: error => { cleanup(); reject(error); }, timer });
    signal?.addEventListener('abort', abort, { once: true });
    b.worker.postMessage({ id, request: {...request, cancellation_id: cancellationId} });
  });
}
export async function withRevision<T>(root: string, action: (context: string) => Promise<T>, revision?: string): Promise<T> {
  const opened = await storageCall<{ context: string }>({ op: "open", root, revision });
  try { return await action(opened.context); }
  finally { await storageCall({ op: "close", context: opened.context }).catch(() => undefined); }
}
