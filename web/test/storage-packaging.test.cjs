const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { packageStandalone } = require('../scripts/package-storage-native.cjs');
for (const nested of [false,true]) test(`standalone packages storage and public assets (nested=${nested})`, () => {
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'cie-package-'));
  try {
    const server=nested?'.next/standalone/web/server.js':'.next/standalone/server.js';
    for (const [file,content] of [[server,'server'],['scripts/storage-worker.cjs','worker'],['scripts/storage-control.cjs','control'],['native/cie_storage_node.node','addon'],['.next/static/app.js','static'],['public/logo.svg','logo']]) {
      fs.mkdirSync(path.dirname(path.join(root,file)),{recursive:true});fs.writeFileSync(path.join(root,file),content);
    }
    const app=packageStandalone(root);
    assert.equal(fs.readFileSync(path.join(app,'native/cie_storage_node.node'),'utf8'),'addon');
    assert.equal(fs.readFileSync(path.join(app,'scripts/storage-control.cjs'),'utf8'),'control');
    assert.equal(fs.readFileSync(path.join(app,'.next/static/app.js'),'utf8'),'static');
    assert.equal(fs.readFileSync(path.join(app,'public/logo.svg'),'utf8'),'logo');
  } finally {fs.rmSync(root,{recursive:true,force:true});}
});
