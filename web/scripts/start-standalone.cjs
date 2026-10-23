const path = require('node:path');
const fs = require('node:fs');
const standalone = path.resolve(__dirname, '../.next/standalone');
const app = fs.existsSync(path.join(standalone, 'web/server.js')) ? path.join(standalone, 'web') : standalone;
process.chdir(app);
require(path.join(app, 'server.js'));
