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
  // 既定パレット先頭 #36A2EB、fill alpha=0.5 / stroke alpha=1.0(chart.js v4)
  assert.deepEqual(model.series[0].fill, ['rgba(54,162,235,0.5)']);
  assert.deepEqual(model.series[0].stroke, ['rgba(54,162,235,1)']);
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

test('mixed bar+line: line データセットの要素は bar geometry に含めない', async () => {
  const spec = { type: 'bar', data: { labels: ['A','B','C'],
    datasets: [
      { type: 'bar', data: [10,20,30] },
      { type: 'line', data: [5,15,25] },
    ] } };
  const model = await extractChartjsModel(spec, 800, 600);
  // bar を含む混在チャートでは geometry を必ず持つ(undefined への退行を検出)。
  assert.ok(model.geometry, 'bar を含む混在チャートでは geometry を持つべき');
  // 全要素が有限な bar であること(NaN を含まない)。
  for (const e of model.geometry.elements) {
    assert.equal(e.kind, 'bar');
    assert.ok(Number.isFinite(e.nx) && Number.isFinite(e.nw)
      && Number.isFinite(e.ny) && Number.isFinite(e.nh), `NaN element: ${JSON.stringify(e)}`);
  }
  // bar データセット(series 0)の 3 要素のみ。
  assert.ok(model.geometry.elements.every((e) => e.series === 0));
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
