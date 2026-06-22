import { test } from 'node:test';
import assert from 'node:assert/strict';
import { extractChartjsModel, toRgba } from './extract.mjs';
import { fmtAlpha } from './color-util.mjs';

test('bar: 系列色と軸目盛りを共通スキーマで抽出', async () => {
  const spec = { type: 'bar', data: { labels: ['1月','2月','3月'],
    datasets: [{ label: '売上', data: [0,100,50] }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.equal(model.meta.type, 'bar');
  assert.equal(model.series.length, 1);
  // chart.js v4 既定: 解決済み色は rgba 正規化済み文字列
  assert.match(model.series[0].fill[0], /^rgba\(\d+,\d+,\d+,[\d.]+\)$/);
  assert.equal(model.axes.y.kind, 'linear');
  assert.ok(Array.isArray(model.axes.y.ticks));
  assert.equal(model.axes.x.labels.length, 3);
});

test('bar: 既定パレット色は canonical rgba に正規化される', async () => {
  const spec = { type: 'bar', data: { labels: ['a','b','c'],
    datasets: [{ label: 's', data: [0,100,50] }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  // 既定パレット先頭 #36A2EB、fill alpha=0.5(塗りは常に描画)。
  assert.deepEqual(model.series[0].fill, ['rgba(54,162,235,0.5)']);
  // 既定 bar は borderWidth:0(枠線未描画)→ paint-state で stroke は null。
  assert.deepEqual(model.series[0].stroke, [null]);
  // PNG バッファも返る
  assert.ok(Buffer.isBuffer(model.png));
});

test('bar: chartArea 基準の正規化 geometry を出力', async () => {
  const spec = { type: 'bar', data: { labels: ['A','B','C'],
    datasets: [{ data: [10,20,30] }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.ok(model.geometry, 'geometry を持つべき');
  assert.equal(model.geometry.elements.length, 3);
  const { plot_area, elements } = model.geometry;
  // plot_area はキャンバス [0,1]。
  assert.ok(plot_area.x > 0 && plot_area.x < 1);
  assert.ok(plot_area.w > 0 && plot_area.w <= 1);
  for (const e of elements) {
    assert.equal(e.kind, 'bar');
    assert.ok(e.nx >= 0 && e.nx <= 1, `nx=${e.nx}`);
    assert.ok(e.nw > 0 && e.nw <= 1, `nw=${e.nw}`);
  }
  // 左→右にカテゴリ、値が大きいほど高い。
  assert.ok(elements[0].nx < elements[1].nx);
  assert.ok(elements[2].nh > elements[0].nh);
});

test('horizontal bar は geometry を出さない(スコープ外)', async () => {
  const spec = { type: 'bar', data: { labels: ['A','B'], datasets: [{ data: [10,90] }] },
    options: { indexAxis: 'y' } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.equal(model.geometry, undefined);
});

test('mixed bar+line: geometry を出さない(fulgur Mixed=None に揃える)', async () => {
  const spec = { type: 'bar', data: { labels: ['A','B','C'],
    datasets: [
      { type: 'bar', data: [10,20,30] },
      { type: 'line', data: [5,15,25] },
    ] } };
  const model = await extractChartjsModel(spec, 800, 600);
  // 混在チャートは fulgur 側 compute_geometry が None。chart.js だけ bar geometry を
  // 出すと diff が片側 skip=pass で緑になり実際の棒を照合しないため undefined に揃える。
  assert.equal(model.geometry, undefined);
});

test('base line + 全 dataset bar override: fulgur は Bar 扱い → geometry を出す', async () => {
  // fulgur frontend は解決後種別が全 bar なら基本 type=line でも ChartKind::Bar。
  // トップレベル type だけ見て undefined にすると片側 skip=pass の見せかけ緑になる。
  const spec = { type: 'line', data: { labels: ['A','B','C'],
    datasets: [
      { type: 'bar', data: [10,20,30] },
      { type: 'bar', data: [5,15,25] },
    ] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.ok(model.geometry, '全 bar override は縦棒なので geometry を持つべき');
  assert.equal(model.geometry.elements.length, 6); // 2 系列 × 3 カテゴリ
  for (const e of model.geometry.elements) {
    assert.equal(e.kind, 'bar');
    assert.ok(Number.isFinite(e.nx) && Number.isFinite(e.nw)
      && Number.isFinite(e.ny) && Number.isFinite(e.nh));
  }
});

test('toRgba: 空白付き rgba を canonical 形へ正規化', () => {
  assert.equal(toRgba('rgba(54, 162, 235, 0.50)'), 'rgba(54,162,235,0.5)');
  assert.equal(toRgba('rgb(54, 162, 235)'), 'rgba(54,162,235,1)');
  assert.equal(toRgba('#36a2eb'), 'rgba(54,162,235,1)');
});

test('fmtAlpha: 正規化規約', () => {
  assert.equal(fmtAlpha(1), '1');
  assert.equal(fmtAlpha(1.5), '1');
  assert.equal(fmtAlpha(0), '0');
  assert.equal(fmtAlpha(-0.1), '0');
  assert.equal(fmtAlpha(0.5), '0.5');
  assert.equal(fmtAlpha(0.25), '0.25');
  assert.equal(fmtAlpha(0.3333333), '0.333');
});

test('paint-state: 既定 bar の stroke は未描画(borderWidth:0)で null', async () => {
  const spec = { type: 'bar', data: { labels: ['a','b'],
    datasets: [{ data: [10,90] }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.deepEqual(model.series[0].stroke, [null]);
  // 塗りは描画されるので色を保持。
  assert.notEqual(model.series[0].fill[0], null);
});

test('paint-state: line(fill:false)の fill は未描画(area 未塗り)で null', async () => {
  const spec = { type: 'line', data: { labels: ['a','b','c'],
    datasets: [{ data: [1,2,3], borderColor: '#ff6384', fill: false }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.deepEqual(model.series[0].fill, [null]);
  // 線は描画されるので stroke は色を保持。
  assert.notEqual(model.series[0].stroke[0], null);
});

test('paint-state: area(fill:true)の fill は描画されるので色を保持(回帰防止)', async () => {
  const spec = { type: 'line', data: { labels: ['a','b','c'],
    datasets: [{ data: [1,2,3], borderColor: '#4bc0c0',
      backgroundColor: '#4bc0c0', fill: true }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.notEqual(model.series[0].fill[0], null);
  assert.notEqual(model.series[0].stroke[0], null);
});

test('paint-state: 既定 line(fill キー無し)も area 未塗りで fill は null', async () => {
  // line.json fixture は fill キーを持たず、chart.js v4 が既定で fill:false に解決する。
  // 既定 line の area も未塗りなので fill スロットは null になる(回帰防止)。
  const spec = { type: 'line', data: { labels: ['a','b','c'],
    datasets: [{ data: [1,2,3], borderColor: '#ff6384' }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.deepEqual(model.series[0].fill, [null]);
  // 線は描画されるので stroke は色を保持。
  assert.notEqual(model.series[0].stroke[0], null);
});

test('scatter: chartArea 基準の正規化 geometry を出力', async () => {
  const spec = {
    type: 'scatter',
    data: { datasets: [{ data: [{ x: 1, y: 2 }, { x: 3, y: 4 }, { x: 5, y: 6 }] }] },
  };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.ok(model.geometry, 'scatter は geometry を持つべき');
  assert.equal(model.geometry.elements.length, 3);
  for (const e of model.geometry.elements) {
    assert.equal(e.kind, 'scatter');
    assert.equal(e.nw, 0);
    assert.equal(e.nh, 0);
    assert.ok(e.nx >= 0 && e.nx <= 1, `nx=${e.nx}`);
    assert.ok(e.ny >= 0 && e.ny <= 1, `ny=${e.ny}`);
  }
  assert.ok(model.geometry.elements[0].nx < model.geometry.elements[1].nx,
    'x 増加 → nx 増加');
});

test('bubble: chartArea 基準 geometry (nw=正規化半径)', async () => {
  const spec = {
    type: 'bubble',
    data: { datasets: [{ data: [{ x: 1, y: 2, r: 10 }, { x: 3, y: 4, r: 20 }] }] },
  };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.ok(model.geometry, 'bubble は geometry を持つべき');
  assert.equal(model.geometry.elements.length, 2);
  for (const e of model.geometry.elements) {
    assert.equal(e.kind, 'bubble');
    assert.ok(e.nw > 0, `bubble の nw は正: ${e.nw}`);
    assert.equal(e.nh, 0);
  }
  assert.ok(model.geometry.elements[1].nw > model.geometry.elements[0].nw,
    '大きい半径ほど大きい nw');
});

test('line: chartArea 基準の正規化 geometry を出力', async () => {
  const spec = {
    type: 'line',
    data: { labels: ['a', 'b', 'c'], datasets: [{ data: [10, 20, 30] }] },
  };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.ok(model.geometry, 'line は geometry を持つべき');
  assert.equal(model.geometry.elements.length, 3);
  for (const e of model.geometry.elements) {
    assert.equal(e.kind, 'line');
    assert.equal(e.nw, 0);
    assert.equal(e.nh, 0);
  }
  assert.ok(model.geometry.elements[0].nx < model.geometry.elements[1].nx,
    'カテゴリ順に nx 増加');
});
