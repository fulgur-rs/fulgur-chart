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

/// 縦棒の BarElement を chartArea 基準 [0,1] へ正規化。横棒(indexAxis:'y')、
/// 非 bar、混在(bar+line 等)は undefined。fulgur 側 compute_geometry は
/// `ChartKind::Bar { horizontal: false }` のみ Some を返し Mixed は None なので、
/// 混在で chart.js だけ bar geometry を出すと diff が片側 skip=pass で緑になり
/// 実際に描く棒を一切照合しなくなる。両側 None に揃えて「両者とも未対応」を顕在化する。
function barGeometry(chart, spec, width, height) {
  const indexAxis = (spec.options && spec.options.indexAxis) || 'x';
  if (spec.type !== 'bar' || indexAxis === 'y') return undefined;
  // データセット単位 type を解決し(未指定はトップレベル type を継承)、bar 以外を
  // 1 つでも含めば混在チャート → fulgur のスコープ外なので geometry を出さない。
  const isMixed = spec.data.datasets.some(
    (ds) => (ds.type ?? spec.type) !== 'bar',
  );
  if (isMixed) return undefined;
  const a = chart.chartArea;
  const caw = a.right - a.left;
  const cah = a.bottom - a.top;
  if (!(caw > 0) || !(cah > 0)) return undefined;
  const elements = [];
  for (let s = 0; s < spec.data.datasets.length; s++) {
    const meta = chart.getDatasetMeta(s);
    for (let i = 0; i < meta.data.length; i++) {
      const { x, y, base, width: bw } = meta.data[i].getProps(
        ['x', 'y', 'base', 'width'],
        true,
      );
      /// 純 bar チャートでも防御的に非 bar 要素(width/base 無し)は除外する。
      if (bw === undefined) continue;
      const left = x - bw / 2;
      const top = Math.min(y, base);
      const h = Math.abs(base - y);
      elements.push({
        series: s,
        index: i,
        kind: 'bar',
        nx: (left - a.left) / caw,
        ny: (top - a.top) / cah,
        nw: bw / caw,
        nh: h / cah,
      });
    }
  }
  return {
    plot_area: { x: a.left / width, y: a.top / height, w: caw / width, h: cah / height },
    elements,
  };
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

  // 軸(線形スケールがあれば)。値(線形)軸→y、カテゴリ→x の正規化規約。
  // fulgur 側 model.rs の compute_axes も同じ規約で値軸を y に載せるため
  // apples-to-apples 照合が成立する。
  // scatter/bubble は x・y とも linear なので axis==='y' を優先して y-linear を選ぶ。
  // 横棒(indexAxis:'y')は chart.js の linear scale が x 軸に付くため
  // axis==='y' では見つからず、fallback で x-linear を axes.y に載せる。
  // counts.y_ticks は diff.mjs では比較されない(axes 次元が担当するため)。
  let axes;
  const scaleIds = Object.keys(chart.scales);
  const linId =
    scaleIds.find(
      (id) =>
        chart.scales[id].type === 'linear' && chart.scales[id].axis === 'y',
    ) ?? scaleIds.find((id) => chart.scales[id].type === 'linear');
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

  const geometry = barGeometry(chart, spec, width, height);
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
    geometry,
    png, // Buffer(レポート用)
  };
}
