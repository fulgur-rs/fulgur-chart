import { test } from 'node:test';
import assert from 'node:assert/strict';
import { crosscheckColors } from './crosscheck.mjs';

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

// ALPHA-MULTIPLIER BUG: SVG が #36a2eb@0.25 を fill で描く → fill 役割の主張 {0.5} に
// 0.25 ∉ → divergence で FAIL。役割別追跡なので modelAlphas は fill の {0.5} のみ。
test('ALPHA-MULTIPLIER BUG: fill@0.25 → fill 役割の divergence で FAIL', () => {
  const svg = `<svg><rect fill="#36a2eb" fill-opacity="0.25"/>
    <path stroke="#36a2eb"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, false);
  assert.equal(r.divergences.length, 1);
  const d = r.divergences[0];
  assert.equal(d.role, 'fill');
  assert.equal(d.rgb, '#36a2eb');
  assert.equal(d.paintedAlpha, '0.25');
  assert.deepEqual(d.modelAlphas, ['0.5']);
});

// 役割別追跡: fill と stroke が同一 RGB・異なる alpha のとき、fill が誤って stroke の
// alpha(1)で塗られたら検出する。RGB だけで束ねると 1 ∈ {0.5,1} で取りこぼす。
test('役割別: fill が stroke の alpha(1)で塗られる → divergence で FAIL', () => {
  const svg = `<svg><rect fill="#36a2eb"/>
    <path stroke="#36a2eb"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, false);
  assert.equal(r.divergences.length, 1);
  const d = r.divergences[0];
  assert.equal(d.role, 'fill');
  assert.equal(d.paintedAlpha, '1');
  assert.deepEqual(d.modelAlphas, ['0.5']);
});

// chrome 除外: 系列色と同じ RGB を持つ text/line(ラベル・グリッド)は走査対象外。
// 例: 系列 rgba(102,102,102,0.5)、テキスト #666666@1 → 偽の divergence を出さない。
test('chrome 除外: 系列色と同 RGB の <text>/<line> は divergence を出さない', () => {
  const svg = `<svg><rect fill="#666666" fill-opacity="0.5"/>
    <text fill="#666666">label</text>
    <line stroke="#666666"/></svg>`;
  const model = { series: [{ fill: ['rgba(102,102,102,0.5)'], stroke: [] }] };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, true);
  assert.deepEqual(r.divergences, []);
  assert.deepEqual(r.unpainted, []);
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

// COMBINED PATH: fulgur が area/pie で実際に出力する単一 <path>(fill, stroke,
// stroke-width, fill-opacity, stroke-opacity を同居)を正しくパースできる。
// \bfill="..." が fill-opacity="..." と、\bstroke="..." が stroke-width="..." と
// 衝突しないことを pin する(crates/fulgur-chart/src/svg.rs の path 出力に対応)。
test('COMBINED PATH: fill/stroke/opacity 同居 path を両色とも正しく抽出 → PASS', () => {
  const svg = `<svg><path d="M0 0 L1 1" fill="#010203" stroke="#040506" stroke-width="2" fill-opacity="0.5" stroke-opacity="0.25"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(1,2,3,0.5)'], stroke: ['rgba(4,5,6,0.25)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, true);
  assert.deepEqual(r.divergences, []);
  assert.deepEqual(r.unpainted, []);
});

// COMBINED PATH alpha-multiplier: 同居 path だが fill-opacity が誤って 0.25 →
// fill rgb #010203 の painted alpha 0.25 ∉ {0.5} → divergence で FAIL。
test('COMBINED PATH: fill-opacity 乗算バグ(0.25)→ #010203 で divergence', () => {
  const svg = `<svg><path d="M0 0 L1 1" fill="#010203" stroke="#040506" stroke-width="2" fill-opacity="0.25" stroke-opacity="0.25"/></svg>`;
  const model = {
    series: [{ fill: ['rgba(1,2,3,0.5)'], stroke: ['rgba(4,5,6,0.25)'] }],
  };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, false);
  assert.equal(r.divergences.length, 1);
  const d = r.divergences[0];
  assert.equal(d.rgb, '#010203');
  assert.equal(d.paintedAlpha, '0.25');
  assert.deepEqual(d.modelAlphas, ['0.5']);
  assert.deepEqual(r.unpainted, []);
});
