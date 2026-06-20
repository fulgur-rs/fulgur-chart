import { test } from 'node:test';
import assert from 'node:assert/strict';
import { extractChartjsModel, toRgba, fmtAlpha } from './extract.mjs';

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
