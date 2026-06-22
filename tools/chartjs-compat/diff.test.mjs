import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diffModels } from './diff.mjs';

const base = () => ({
  meta: { type: 'bar', width: 800, height: 600 },
  axes: { x: { kind: 'category', labels: ['a','b'] },
          y: { kind: 'linear', min: 0, max: 100, step: 20, ticks: [0,20,40,60,80,100] } },
  series: [{ label: 's', fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'], values: [10,90] }],
  counts: { datasets: 1, legend_items: 1, x_ticks: 2 },
});

const geomBase = () => ({
  plot_area: { x: 0.08, y: 0.05, w: 0.9, h: 0.85 },
  elements: [
    { series: 0, index: 0, kind: 'bar', nx: 0.10, ny: 0.70, nw: 0.20, nh: 0.30 },
    { series: 0, index: 1, kind: 'bar', nx: 0.50, ny: 0.40, nw: 0.20, nh: 0.60 },
  ],
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

test('y_ticks の差分は counts 失敗を引き起こさない(両軸あり)', () => {
  const f = base(); const c = base();
  f.counts.y_ticks = 6;
  c.counts.y_ticks = 99; // わざと違う値
  const r = diffModels(f, c);
  assert.equal(r.dimensions.counts.pass, true, 'axes 比較済みなら counts は y_ticks を無視するべき');
  assert.equal(r.pass, true);
});

test('axes が skipped のとき y_ticks 差分は counts 失敗になる', () => {
  const f = base(); const c = base();
  f.counts.y_ticks = 6;
  c.counts.y_ticks = 0; // axes なし相当の値
  delete c.axes; // axes skipped を誘発
  const r = diffModels(f, c);
  assert.equal(r.dimensions.axes.skipped, true, 'axes は skipped のはず');
  assert.equal(r.dimensions.counts.pass, false, 'axes skipped 時は y_ticks を counts でチェックするべき');
});

test('geometry 一致は PASS', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, true);
});

test('geometry 座標ズレ(>0.02)は FAIL', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  c.geometry.elements[1].nx = 0.55; // 0.05 ズレ
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, false);
});

test('要素 kind 不一致は座標が合っていても FAIL', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  c.geometry.elements[0].kind = 'point'; // 座標は同一だが kind が違う
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, false);
  assert.ok(r.dimensions.geometry.diffs.some((d) => d.field === 'elem[0:0].kind'));
});

test('plot_area ズレは pass に影響せず info に記録される', () => {
  // 2 エンジンの余白差は要素正規化で吸収されるため pass を落とさない(診断のみ)。
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  c.geometry.plot_area.w = 0.80; // 内側領域の取り方が違う
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, true);
  assert.ok(r.dimensions.geometry.info.some((d) => d.field === 'plot_area.w'));
});

test('要素数不一致は FAIL', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = { ...base(), geometry: geomBase() };
  c.geometry.elements.pop(); // 要素数を 2→1 に
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.pass, false);
  assert.ok(r.dimensions.geometry.diffs.some((d) => d.field === 'element_count'));
});

test('片方に geometry が無ければ skip', () => {
  const f = { ...base(), geometry: geomBase() };
  const c = base(); // geometry なし
  const r = diffModels(f, c);
  assert.equal(r.dimensions.geometry.skipped, true);
  assert.equal(r.dimensions.geometry.pass, true);
});

test('paint-state: 片側 null スロットは colors 照合から除外され PASS', () => {
  // chart.js 側が未描画(null)、fulgur 側は色 → 照合対象外で PASS。
  const f = base(); const c = base();
  c.series[0].stroke = [null]; // borderWidth:0 で未描画
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, true);
});

test('paint-state: 混在 null/色は非 null スロットのみ照合', () => {
  const f = base(); const c = base();
  f.series[0].fill = ['rgba(1,1,1,1)', 'rgba(2,2,2,1)'];
  c.series[0].fill = [null, 'rgba(2,2,2,1)']; // 0番は未描画、1番は一致
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, true);
});

test('paint-state: 混在で非 null スロットが不一致なら FAIL', () => {
  const f = base(); const c = base();
  f.series[0].fill = ['rgba(1,1,1,1)', 'rgba(2,2,2,1)'];
  c.series[0].fill = [null, 'rgba(9,9,9,1)']; // 1番が不一致
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, false);
});

test('paint-state: null を含まない通常の不一致は従来どおり FAIL', () => {
  const f = base(); const c = base();
  c.series[0].fill = ['rgba(99,99,99,0.5)'];
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, false);
});

test('paint-state: 長さ1 [null] は長い色配列の全スロットへブロードキャストして skip', () => {
  // collapse で [null,null]→[null] に畳まれたケース: at() で全スロットへ展開され全 skip。
  const f = base(); const c = base();
  f.series[0].fill = ['rgba(1,1,1,1)', 'rgba(2,2,2,1)', 'rgba(3,3,3,1)'];
  c.series[0].fill = [null]; // 全スロット未描画
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, true);
});
