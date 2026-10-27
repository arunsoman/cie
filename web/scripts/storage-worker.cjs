// Node's event loop never executes synchronous Rust/SQLite work.
const { parentPort } = require('node:worker_threads');
const path = require('node:path');
const fs = require('node:fs');
const packaged = path.resolve(__dirname, '../native/cie_storage_node.node');
// Indirect require: bundlers (Turbopack/webpack) statically trace plain
// `require()` and try to resolve the native .node file at build time, which
// fails since it lives outside the bundle root. eval('require') is opaque to
// that static analysis, so the binding is only resolved at runtime.
const nativeRequire = eval('require');
const binding = nativeRequire(process.env.CIE_STORAGE_NATIVE || (fs.existsSync(packaged) ? packaged : path.resolve(__dirname, '../../target/debug/cie_storage_node.node')));
// This handler blocks the whole worker for the duration of storageDispatch,
// so queued requests only actually start once every earlier one has fully
// returned. The "started" ack marks that real start so the caller's own
// deadline can be measured from it, not from the (possibly much earlier)
// moment the request was enqueued.
parentPort.on('message', ({ id, request }) => {
  parentPort.postMessage({ id, started: true });
  try { parentPort.postMessage({ id, response: JSON.parse(binding.storageDispatch(JSON.stringify(request))) }); }
  catch (error) { parentPort.postMessage({ id, response: { ok: false, error: { code: 'Io', message: String(error) } } }); }
});
