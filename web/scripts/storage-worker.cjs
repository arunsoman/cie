// Node's event loop never executes synchronous Rust/SQLite work.
const { parentPort } = require('node:worker_threads');
const path = require('node:path');
const binding = require(process.env.CIE_STORAGE_NATIVE || path.resolve(__dirname, '../../target/debug/cie_storage_node.node'));
parentPort.on('message', ({ id, request }) => {
  try { parentPort.postMessage({ id, response: JSON.parse(binding.storageDispatch(JSON.stringify(request))) }); }
  catch (error) { parentPort.postMessage({ id, response: { ok: false, error: { code: 'Io', message: String(error) } } }); }
});
