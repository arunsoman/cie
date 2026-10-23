// Node's event loop never executes synchronous Rust/SQLite work.
const { parentPort } = require('node:worker_threads');
const path = require('node:path');
// Indirect require: bundlers (Turbopack/webpack) statically trace plain
// `require()` and try to resolve the native .node file at build time, which
// fails since it lives outside the bundle root. eval('require') is opaque to
// that static analysis, so the binding is only resolved at runtime.
const nativeRequire = eval('require');
const binding = nativeRequire(process.env.CIE_STORAGE_NATIVE || path.resolve(__dirname, '../../target/debug/cie_storage_node.node'));
parentPort.on('message', ({ id, request }) => {
  try { parentPort.postMessage({ id, response: JSON.parse(binding.storageDispatch(JSON.stringify(request))) }); }
  catch (error) { parentPort.postMessage({ id, response: { ok: false, error: { code: 'Io', message: String(error) } } }); }
});
