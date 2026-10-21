const fs = require('node:fs'), path = require('node:path'), ts = require('typescript');
module.exports = function loader(mocks = {}) {
  const cache = new Map();
  function load(file) {
    file = path.resolve(file);
    if (cache.has(file)) return cache.get(file);
    const out = {}; cache.set(file, out);
    const code = ts.transpileModule(fs.readFileSync(file, 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, jsx: ts.JsxEmit.ReactJSX, esModuleInterop: true }}).outputText;
    const localRequire = id => {
      if (id in mocks) return mocks[id];
      if (id === 'viz-adapter') return load(path.resolve(__dirname, '../../../viz-adapter/src/index.ts'));
      if (id.startsWith('@/')) return resolve(path.resolve(__dirname, '../../src', id.slice(2)));
      if (id.startsWith('.')) return resolve(path.resolve(path.dirname(file), id));
      return require(id);
    };
    function resolve(base) {
      for (const name of [base, base + '.ts', base + '.tsx', base.replace(/\.js$/, '.ts')]) {
        if (fs.existsSync(name) && fs.statSync(name).isFile()) return load(name);
      }
      throw new Error('Missing module ' + base);
    }
    new Function('require', 'exports', code)(localRequire, out);
    return out;
  }
  return load;
};
