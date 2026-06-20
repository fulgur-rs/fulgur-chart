//! fulgur 意味モデル ⟷ chart.js 意味モデルの次元別(色 / 軸 / counts)数値照合。
//! 各次元ごとに pass と差分リストを返し、全次元 pass で総合 pass。

export const TOLERANCES = { geometryNorm: 0.02 }; // Phase 2: geometry 次元用(現状未使用)

function num(a, b) {
  return a === b || (a != null && b != null && Math.abs(a - b) < 1e-9);
}
function arrNum(a, b) {
  return (
    Array.isArray(a) &&
    Array.isArray(b) &&
    a.length === b.length &&
    a.every((x, i) => num(x, b[i]))
  );
}

// fulgur/chart.js の fill 表現(長さ1 畳み or 要素配列)を要素数 n に展開して比較。
function colorsEqual(a, b) {
  if (!Array.isArray(a) || !Array.isArray(b)) return false;
  const n = Math.max(a.length, b.length);
  const at = (arr, i) => (arr.length === 1 ? arr[0] : arr[i]);
  for (let i = 0; i < n; i++) if (at(a, i) !== at(b, i)) return false;
  return true;
}

export function diffModels(fulgur, chartjs) {
  const dims = {};

  // colors
  const colorDiffs = [];
  const ns = Math.min(fulgur.series.length, chartjs.series.length);
  for (let i = 0; i < ns; i++) {
    if (!colorsEqual(fulgur.series[i].fill, chartjs.series[i].fill))
      colorDiffs.push({
        series: i,
        field: 'fill',
        fulgur: fulgur.series[i].fill,
        chartjs: chartjs.series[i].fill,
      });
    if (!colorsEqual(fulgur.series[i].stroke, chartjs.series[i].stroke))
      colorDiffs.push({
        series: i,
        field: 'stroke',
        fulgur: fulgur.series[i].stroke,
        chartjs: chartjs.series[i].stroke,
      });
  }
  dims.colors = { pass: colorDiffs.length === 0, diffs: colorDiffs };

  // axes(両方に linear y がある場合のみ厳密比較)
  const fy = fulgur.axes?.y;
  const cy = chartjs.axes?.y;
  if (fy && cy) {
    const axDiffs = [];
    if (!num(fy.min, cy.min))
      axDiffs.push({ field: 'y.min', fulgur: fy.min, chartjs: cy.min });
    if (!num(fy.max, cy.max))
      axDiffs.push({ field: 'y.max', fulgur: fy.max, chartjs: cy.max });
    if (!num(fy.step, cy.step))
      axDiffs.push({ field: 'y.step', fulgur: fy.step, chartjs: cy.step });
    if (!arrNum(fy.ticks, cy.ticks))
      axDiffs.push({ field: 'y.ticks', fulgur: fy.ticks, chartjs: cy.ticks });
    dims.axes = { pass: axDiffs.length === 0, diffs: axDiffs };
  } else {
    dims.axes = { pass: true, skipped: true };
  }

  // counts
  const countDiffs = [];
  for (const k of ['datasets', 'legend_items', 'x_ticks', 'y_ticks']) {
    if (fulgur.counts[k] !== chartjs.counts[k])
      countDiffs.push({ field: k, fulgur: fulgur.counts[k], chartjs: chartjs.counts[k] });
  }
  dims.counts = { pass: countDiffs.length === 0, diffs: countDiffs };

  const pass = Object.values(dims).every((d) => d.pass);
  return { pass, dimensions: dims };
}
