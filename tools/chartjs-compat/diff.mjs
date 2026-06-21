//! fulgur 意味モデル ⟷ chart.js 意味モデルの次元別(色 / 軸 / counts)数値照合。
//! 各次元ごとに pass と差分リストを返し、全次元 pass で総合 pass。

export const TOLERANCES = { geometryNorm: 0.02 }; // Phase 2: geometry 次元の正規化座標許容差

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

const gsgn = (d) => (Math.abs(d) < TOLERANCES.geometryNorm ? 0 : Math.sign(d));

// 両モデルの geometry を構造+数値照合する。pass は要素座標(プロット領域基準)と
// 構造(要素数・左→右順序・系列ごと bar 高さ単調性)のみで判定する。
// plot_area(キャンバス基準)の差は info として記録するが pass には含めない:
// 2 つのレイアウトエンジンが余白(OUTER_PAD/軸幅/タイトル帯)を画素一致させる
// 保証はなく、各要素は「自分のプロット領域」で正規化済みのため plot_area 差に
// 頑健だから。要素が tolerance 内で一致する = 分母が意味的に整合していた、の証左。
function diffGeometry(fg, cg) {
  const tol = TOLERANCES.geometryNorm;
  const diffs = [];
  const info = [];

  // plot_area: 診断情報のみ(pass 不参加)。内側領域の取り方の目安。
  for (const k of ['x', 'y', 'w', 'h']) {
    if (Math.abs(fg.plot_area[k] - cg.plot_area[k]) > tol)
      info.push({ field: `plot_area.${k}`, fulgur: fg.plot_area[k], chartjs: cg.plot_area[k] });
  }

  // 構造: 要素数。
  if (fg.elements.length !== cg.elements.length) {
    diffs.push({ field: 'element_count', fulgur: fg.elements.length, chartjs: cg.elements.length });
    return { pass: false, diffs, info }; // ペアリング不能なので以降は省略。
  }

  const key = (e) => `${e.series}:${e.index}`;
  const cmap = new Map(cg.elements.map((e) => [key(e), e]));

  // 数値: (series,index) で対応付けて nx/ny/nw/nh を比較。
  for (const fe of fg.elements) {
    const ce = cmap.get(key(fe));
    if (!ce) {
      diffs.push({ field: `elem[${key(fe)}]`, fulgur: 'present', chartjs: 'missing' });
      continue;
    }
    for (const k of ['nx', 'ny', 'nw', 'nh']) {
      if (Math.abs(fe[k] - ce[k]) > tol)
        diffs.push({ field: `elem[${key(fe)}].${k}`, fulgur: fe[k], chartjs: ce[k] });
    }
  }

  // 構造: 左→右順序(nx 昇順の (series,index) 列が一致)。
  const order = (els) => [...els].sort((a, b) => a.nx - b.nx).map(key).join(',');
  if (order(fg.elements) !== order(cg.elements))
    diffs.push({ field: 'order', fulgur: order(fg.elements), chartjs: order(cg.elements) });

  // 構造: 系列ごとの bar 高さ(nh)単調性。連続する index の nh 増減符号が一致。
  const bySeries = (els) => {
    const m = new Map();
    for (const e of [...els].sort((a, b) => a.index - b.index)) {
      if (!m.has(e.series)) m.set(e.series, []);
      m.get(e.series).push(e.nh);
    }
    return m;
  };
  const fh = bySeries(fg.elements);
  const ch = bySeries(cg.elements);
  for (const [s, hs] of fh) {
    const cs = ch.get(s);
    if (!cs || cs.length !== hs.length) continue;
    for (let i = 1; i < hs.length; i++) {
      if (gsgn(hs[i] - hs[i - 1]) !== gsgn(cs[i] - cs[i - 1])) {
        diffs.push({ field: `monotonicity.series[${s}]`, fulgur: hs, chartjs: cs });
        break;
      }
    }
  }

  return { pass: diffs.length === 0, diffs, info };
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
  // y_ticks は両 axes.y が比較された場合は axes 次元に委ねる(冗長ノイズ回避)。
  // axes が skipped(片方のみ axes.y あり)の場合はフォールバックとして y_ticks を比較する。
  const axesActuallyCompared = !!(fulgur.axes?.y && chartjs.axes?.y);
  const countKeys = axesActuallyCompared
    ? ['datasets', 'legend_items', 'x_ticks']
    : ['datasets', 'legend_items', 'x_ticks', 'y_ticks'];
  const countDiffs = [];
  for (const k of countKeys) {
    if (fulgur.counts[k] !== chartjs.counts[k])
      countDiffs.push({ field: k, fulgur: fulgur.counts[k], chartjs: chartjs.counts[k] });
  }
  dims.counts = { pass: countDiffs.length === 0, diffs: countDiffs };

  // geometry(両方にある場合のみ照合)。
  if (fulgur.geometry && chartjs.geometry) {
    dims.geometry = diffGeometry(fulgur.geometry, chartjs.geometry);
  } else {
    dims.geometry = { pass: true, skipped: true };
  }

  const pass = Object.values(dims).every((d) => d.pass);
  return { pass, dimensions: dims };
}
