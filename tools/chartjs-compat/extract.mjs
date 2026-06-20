//! chart.js v4 を node-canvas で実評価し、fulgur と数値照合するための共通スキーマ
//! 意味モデル(解決済み色・軸目盛り・counts)へ抽出する。色は描画後の解決済み
//! element options(`getDatasetMeta(i).data[j].options`)から取り、canonical rgba
//! へ正規化する(Rust 側 model::rgba_string と byte-for-byte 一致)。

import { createCanvas } from 'canvas';
import { Chart } from 'chart.js/auto';
import { fmtAlpha } from './color-util.mjs';

Chart.defaults.font.size = 12;

/// CSS 色文字列 → canonical rgba(R,G,B,A)。node-canvas の fillStyle 解釈を利用し、
/// '#rrggbb' か 'rgba(r, g, b, a)' へ正規化したものを再整形する。
export function toRgba(css) {
  const c = createCanvas(1, 1);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#000';
  ctx.fillStyle = css; // 無効なら黒のまま
  const v = ctx.fillStyle; // '#rrggbb' か 'rgba(r, g, b, a)'
  // CanvasGradient/CanvasPattern が渡ると非文字列になるため透明にフォールバック。
  if (typeof v !== 'string') {
    return 'rgba(0,0,0,0)';
  }
  let r, g, b, a = 1;
  if (v.startsWith('#')) {
    r = parseInt(v.slice(1, 3), 16);
    g = parseInt(v.slice(3, 5), 16);
    b = parseInt(v.slice(5, 7), 16);
  } else {
    const m = v.match(/rgba?\(([^)]+)\)/);
    const p = m[1].split(',').map((s) => s.trim());
    r = +p[0];
    g = +p[1];
    b = +p[2];
    a = p[3] === undefined ? 1 : +p[3];
  }
  return `rgba(${r},${g},${b},${fmtAlpha(a)})`;
}

/// 全要素同色なら長さ1へ畳む(fulgur 側 colors_to_strings と対称)。
function collapse(arr) {
  return arr.length > 0 && arr.every((x) => x === arr[0]) ? [arr[0]] : arr;
}

export async function extractChartjsModel(spec, width, height) {
  const canvas = createCanvas(width, height);
  const ctx = canvas.getContext('2d');
  const chart = new Chart(ctx, {
    type: spec.type,
    data: spec.data,
    options: { ...(spec.options || {}), animation: false, responsive: false },
  });
  chart.update();

  const series = spec.data.datasets.map((ds, i) => {
    const meta = chart.getDatasetMeta(i);
    const n = meta.data.length || (ds.data ? ds.data.length : 0);
    // 描画後の解決済み element options を使う(生 dataset プロパティではない)。
    const fill = collapse(
      Array.from({ length: n }, (_, j) =>
        toRgba(meta.data[j]?.options?.backgroundColor ?? '#000'),
      ),
    );
    const stroke = collapse(
      Array.from({ length: n }, (_, j) =>
        toRgba(meta.data[j]?.options?.borderColor ?? '#000'),
      ),
    );
    const values = Array.isArray(ds.data)
      ? ds.data.map((d) =>
          typeof d === 'object' && d !== null ? (d.y ?? d.v ?? null) : d,
        )
      : [];
    return { label: ds.label ?? '', fill, stroke, values };
  });

  // 軸(線形スケールがあれば)。linear を y、category を x とみなす単純規則。
  let axes;
  const scaleIds = Object.keys(chart.scales);
  const linId = scaleIds.find((id) => chart.scales[id].type === 'linear');
  const catId = scaleIds.find((id) => chart.scales[id].type === 'category');
  if (linId) {
    const s = chart.scales[linId];
    const ticks = s.ticks.map((t) => t.value);
    const step = ticks.length >= 2 ? ticks[1] - ticks[0] : null;
    const yAxis = { kind: 'linear', min: s.min, max: s.max, step, ticks };
    const xAxis = catId
      ? { kind: 'category', labels: chart.scales[catId].getLabels() }
      : { kind: 'linear' };
    axes = { x: xAxis, y: yAxis };
  }

  const png = canvas.toBuffer('image/png');
  chart.destroy();

  return {
    meta: { type: spec.type, width, height },
    axes,
    series,
    counts: {
      datasets: spec.data.datasets.length,
      legend_items: spec.data.datasets.filter((d) => d.label).length,
      x_ticks: (spec.data.labels || []).length,
      y_ticks: axes ? axes.y.ticks.length : 0,
    },
    png, // Buffer(レポート用)
  };
}
