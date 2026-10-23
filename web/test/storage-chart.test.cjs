const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { storageChart } = require('./helpers/load-ts.cjs')()(path.resolve(__dirname, '../src/lib/storage-chart.ts'));
const view = { renderer:'cie-cube', kind:'hbar', revision:'7', cells:{dimensions:[{id:'language'}],measures:{count:{}},items:[{members:{language:'Rust'},measures:{count:12}}]} };
test('storage projection maps numeric facts and revision into a renderable chart', () => {
  const chart = storageChart(view);
  assert.deepEqual(chart.data, [{name:'Rust',value:12}]);
  assert.deepEqual(chart.rows, [['Rust',12]]);
  assert.match(chart.source, /revision 7/);
});
test('storage charts reject invalid partitions and unknown numeric measures', () => {
  assert.throws(() => storageChart({...view,kind:'donut'}), /partition/);
  assert.throws(() => storageChart({...view,cells:{...view.cells,items:[{members:{},measures:{count:null}}]}}), /numeric/);
});
