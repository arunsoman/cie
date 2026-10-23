const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
// With the workspace tracing root Next nests the app under standalone/web.
const standalone = path.join(root, '.next/standalone');
const app = fs.existsSync(path.join(standalone, 'web/server.js')) ? path.join(standalone, 'web') : standalone;
for (const dir of ['scripts', 'native']) fs.mkdirSync(path.join(app, dir), { recursive: true });
fs.copyFileSync(path.join(root, 'scripts/storage-worker.cjs'), path.join(app, 'scripts/storage-worker.cjs'));
fs.copyFileSync(path.join(root, 'native/cie_storage_node.node'), path.join(app, 'native/cie_storage_node.node'));
