const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../..');
const cargo = process.env.CARGO || 'cargo';
const result = spawnSync(cargo, ['build', '-p', 'cie-storage-node'], { cwd: root, stdio: 'inherit' });
if (result.status !== 0) process.exit(result.status || 1);
const file = process.platform === 'win32' ? 'cie_storage_node.dll' : process.platform === 'darwin' ? 'libcie_storage_node.dylib' : 'libcie_storage_node.so';
fs.copyFileSync(path.join(root, 'target/debug', file), path.join(root, 'target/debug/cie_storage_node.node'));

fs.mkdirSync(path.join(root, 'web/native'), { recursive: true });
fs.copyFileSync(path.join(root, 'target/debug/cie_storage_node.node'), path.join(root, 'web/native/cie_storage_node.node'));
