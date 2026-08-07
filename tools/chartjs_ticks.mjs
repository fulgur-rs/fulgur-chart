// chart.js v4 の LinearScale が実際に生成する目盛りを抽出する。
// canvas (node-canvas) で DOM なし環境でも Chart インスタンスを構築できる。

import { createCanvas } from 'canvas';
import { Chart } from 'chart.js/auto';

// グローバルフォント警告を抑制する。
Chart.defaults.font.size = 12;

async function getTicks(label, data, yOpts = {}) {
  const canvas = createCanvas(800, 400);
  const ctx = canvas.getContext('2d');

  const chart = new Chart(ctx, {
    type: 'bar',
    data: {
      labels: ['x'],
      datasets: [{ data }],
    },
    options: {
      animation: false,
      scales: { y: { ...yOpts } },
    },
  });

  const scale = chart.scales.y;
  if (!scale) {
    chart.destroy();
    throw new Error(`chart.scales.y が未定義です (label=${label})`);
  }
  const result = {
    label,
    data,
    yOpts,
    min: scale.min,
    max: scale.max,
    ticks: scale.ticks.map((t) => t.value),
    step: scale.ticks.length >= 2 ? scale.ticks[1].value - scale.ticks[0].value : null,
  };

  chart.destroy();
  return result;
}

const cases = [
  // beginAtZero: false (デフォルト)
  ['[0,100] default', [0, 100], {}],
  ['[0,173] default', [0, 173], {}],
  ['[-30,70] default', [-30, 70], {}],
  ['[0,1] default', [0, 1.0], {}],
  ['[100,10000] default', [100, 10000], {}],
  // beginAtZero: true
  ['[50,200] beginAtZero:true', [50, 200], { beginAtZero: true }],
  ['[-10,90] beginAtZero:true', [-10, 90], { beginAtZero: true }],
  // suggestedMin / suggestedMax
  ['[0,100] suggestedMin:-20', [0, 100], { suggestedMin: -20 }],
  ['[0,100] suggestedMax:150', [0, 100], { suggestedMax: 150 }],
];

const results = [];
for (const [label, data, opts] of cases) {
  results.push(await getTicks(label, data, opts));
}

console.log(JSON.stringify(results, null, 2));

// --- 対数軸 (logarithmic scale) ---
// chart.scales.y.ticks の各要素は major: boolean を持つ。
// 実際に描画されるラベルは tick.label (下記 getLogTicks 内のコメント参照) で
// 決まり、scale.getLabelForValue(value) は数値の書式化のみを行い可視性は
// 反映しない (実測で確認済み。docs/plans/2026-08-08-...md の
// 「Task 6 実測結果」参照)。記憶に頼らずここで実測する。

async function getLogTicks(label, data, yOpts = {}) {
  const canvas = createCanvas(800, 400);
  const ctx = canvas.getContext('2d');

  const chart = new Chart(ctx, {
    type: 'bar',
    data: { labels: data.map((_, i) => `x${i}`), datasets: [{ data }] },
    options: {
      animation: false,
      scales: { y: { type: 'logarithmic', ...yOpts } },
    },
  });

  const scale = chart.scales.y;
  const result = {
    label,
    data,
    yOpts,
    min: scale.min,
    max: scale.max,
    // NOTE: `scale.getLabelForValue(value)` only formats a value (thousands
    // separators, decimals) -- it does NOT decide whether a tick's label is
    // actually rendered. The real rendered text is `tick.label`, which is
    // set by `generateTickLabels()` (chart.js dist/chart.js:4028-4039) via
    // `options.ticks.callback`, whose default for the log scale is
    // `Ticks.formatters.logarithmic` (dist/chunks/helpers.dataset.js:901-917).
    // That formatter returns '' (empty, i.e. hidden) unless
    // `ticks[index].significand` is one of [1,2,3,5,10,15], OR
    // `index > 0.8 * ticks.length` (last ~20% of ticks are always labeled).
    // `significand` is a counter produced by `generateTicks()`
    // (dist/chart.js:10412-10448) and is NOT simply the mantissa digit for
    // ticks after the first decade -- see the doc section for the [3,7] case.
    // We capture both accessors here so the discrepancy is on record.
    ticks: scale.ticks.map((t, i) => ({
      value: t.value,
      major: !!t.major,
      significand: t.significand,
      label: t.label,
      getLabelForValue: scale.getLabelForValue(t.value),
    })),
  };
  chart.destroy();
  return result;
}

const logCases = [
  ['single decade [3,7]', [3, 7], {}],
  ['multi decade [30,4000]', [30, 4000], {}],
  ['sub-one [0.003,0.7]', [0.003, 0.7], {}],
  ['wide [1,1000000]', [1, 1_000_000], {}],
  ['exact powers [1,1000]', [1, 1000], {}],
];

const logResults = [];
for (const [label, data, opts] of logCases) {
  logResults.push(await getLogTicks(label, data, opts));
}
console.error('=== LOG TICKS ===');
console.error(JSON.stringify(logResults, null, 2));
