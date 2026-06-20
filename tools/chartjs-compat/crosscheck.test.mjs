import { test } from 'node:test';
import assert from 'node:assert/strict';
import { svgColorSet, crosscheckColors } from './crosscheck.mjs';

// fill+opacity を rgba 集合へ正規化(参考: svgColorSet)。
test('svgColorSet: fill+opacity と stroke(opacity 無し→a=1)を rgba 集合へ', () => {
  const svg = `<svg><rect x="1" y="2" width="3" height="4" fill="#36a2eb" fill-opacity="0.5"/>
    <line x1="0" y1="0" x2="1" y2="1" stroke="#36a2eb" stroke-width="2"/></svg>`;
  const set = svgColorSet(svg);
  assert.ok(set.has('rgba(54,162,235,0.5)')); // fill+opacity
  assert.ok(set.has('rgba(54,162,235,1)')); // stroke(opacity 無し → a=1)
});

// MATCH: model fill@0.5 + stroke@1, SVG が両方を描く → PASS。
test('MATCH: モデルの fill@0.5 と stroke@1 を SVG が両方描く → PASS', () => {
  const svg = `<svg><rect fill="#36a2eb" fill-opacity="0.5"/>
    <path stroke="#36a2eb"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, true);
  assert.deepEqual(r.divergences, []);
  assert.deepEqual(r.unpainted, []);
});

// ALPHA-MULTIPLIER BUG: SVG が #36a2eb@0.25 を描く → 0.25 ∉ {0.5,1} → divergence で FAIL。
test('ALPHA-MULTIPLIER BUG: SVG が @0.25 を描く → divergence で FAIL', () => {
  const svg = `<svg><rect fill="#36a2eb" fill-opacity="0.25"/>
    <path stroke="#36a2eb"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, false);
  assert.equal(r.divergences.length, 1);
  const d = r.divergences[0];
  assert.equal(d.rgb, '#36a2eb');
  assert.equal(d.paintedAlpha, '0.25');
  assert.ok(d.modelAlphas.includes('0.5'));
  assert.ok(d.modelAlphas.includes('1'));
});

// LINE NON-AREA: SVG は stroke@1 のみ描き fill@0.5 は描かない → PASS(false positive 無し)。
test('LINE NON-AREA: SVG は stroke@1 のみ → PASS(fill@0.5 不在は divergence でない)', () => {
  const svg = `<svg><path stroke="#36a2eb" fill="none"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, true);
  assert.deepEqual(r.divergences, []);
  assert.deepEqual(r.unpainted, []);
});

// WHOLLY MISSING SERIES COLOR: model 色は #36a2eb のみ、SVG は #ff0000 のみ
// → 当該系列の色がどれも描かれない → unpainted で FAIL。
test('WHOLLY MISSING SERIES COLOR: 系列色がどれも描かれない → unpainted で FAIL', () => {
  const svg = `<svg><rect fill="#ff0000" fill-opacity="0.5"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, false);
  assert.equal(r.unpainted.length, 1);
});

// 完全透明(alpha 0)のモデル色は無視される。
test('alpha 0 のモデル色は無視される', () => {
  const svg = `<svg><rect fill="#36a2eb" fill-opacity="0.5"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(0,0,0,0)'], stroke: ['rgba(54,162,235,0.5)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, true);
});
