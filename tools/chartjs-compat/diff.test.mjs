import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diffModels } from './diff.mjs';

const base = () => ({
  meta: { type: 'bar', width: 800, height: 600 },
  axes: { x: { kind: 'category', labels: ['a','b'] },
          y: { kind: 'linear', min: 0, max: 100, step: 20, ticks: [0,20,40,60,80,100] } },
  series: [{ label: 's', fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'], values: [10,90] }],
  counts: { datasets: 1, legend_items: 1, x_ticks: 2, y_ticks: 6 },
});

test('同一モデルは PASS', () => {
  const r = diffModels(base(), base());
  assert.equal(r.pass, true);
});

test('色不一致は FAIL', () => {
  const f = base(); const c = base();
  c.series[0].fill = ['rgba(54,162,235,0.25)']; // alpha 乗算バグ相当
  const r = diffModels(f, c);
  assert.equal(r.pass, false);
  assert.equal(r.dimensions.colors.pass, false);
});

test('目盛り不一致は FAIL', () => {
  const f = base(); const c = base();
  c.axes.y.ticks = [0,25,50,75,100];
  const r = diffModels(f, c);
  assert.equal(r.dimensions.axes.pass, false);
});

test('色配列ブロードキャスト: [c] と [c,c] は一致扱い', () => {
  const f = base(); const c = base();
  c.series[0].fill = ['rgba(54,162,235,0.5)', 'rgba(54,162,235,0.5)'];
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, true);
});

test('counts 不一致は FAIL', () => {
  const f = base(); const c = base();
  c.counts.datasets = 2;
  const r = diffModels(f, c);
  assert.equal(r.dimensions.counts.pass, false);
  assert.equal(r.pass, false);
});

test('軸が片方欠ける場合は axes を skip', () => {
  const f = base(); const c = base();
  delete c.axes;
  const r = diffModels(f, c);
  assert.equal(r.dimensions.axes.skipped, true);
  assert.equal(r.dimensions.axes.pass, true);
});
