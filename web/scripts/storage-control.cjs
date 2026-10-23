// This control entry only flips an atomic flag; SQL stays in the worker.
const path = require('node:path');
const fs = require('node:fs');
const nativeRequire = eval('require');
const packaged = path.resolve(__dirname, '../native/cie_storage_node.node');
let binding;
exports.cancel = id => {
  binding ||= nativeRequire(process.env.CIE_STORAGE_NATIVE || (fs.existsSync(packaged) ? packaged : path.resolve(__dirname, '../../target/debug/cie_storage_node.node')));
  binding.storageCancel(id);
};
