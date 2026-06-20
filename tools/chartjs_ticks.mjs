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
