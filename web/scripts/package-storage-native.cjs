const fs = require('node:fs');
const path = require('node:path');
function packageStandalone(root) {
  const standalone = path.join(root, '.next/standalone');
  const app = fs.existsSync(path.join(standalone, 'web/server.js')) ? path.join(standalone, 'web') : standalone;
  if (!fs.existsSync(path.join(app, 'server.js'))) throw new Error('Next standalone server is missing');
  for (const dir of ['scripts', 'native']) fs.mkdirSync(path.join(app, dir), { recursive: true });
  for (const script of ['storage-worker.cjs', 'storage-control.cjs'])
    fs.copyFileSync(path.join(root, 'scripts', script), path.join(app, 'scripts', script));
  fs.copyFileSync(path.join(root, 'native/cie_storage_node.node'), path.join(app, 'native/cie_storage_node.node'));
  fs.cpSync(path.join(root, '.next/static'), path.join(app, '.next/static'), { recursive: true });
  if (fs.existsSync(path.join(root, 'public'))) fs.cpSync(path.join(root, 'public'), path.join(app, 'public'), { recursive: true });
  return app;
}
module.exports = { packageStandalone };
if (require.main === module) packageStandalone(path.resolve(__dirname, '..'));
