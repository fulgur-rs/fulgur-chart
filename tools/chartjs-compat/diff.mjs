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
// paint-state: どちらかのスロットが null(= そのエンジンで未描画)なら、そのスロットは
// 「可視描画差」ではないため照合対象外にする(false-positive 抑止)。
function colorsEqual(a, b) {
  if (!Array.isArray(a) || !Array.isArray(b)) return false;
  // 片方だけ空配列(= 描画マーク 0 個)は構造的な差異。null skip が「全要素 skip」へ
  // 退化して空 vs 全 null を取り違えないよう、ここで即 false にする。
  if ((a.length === 0) !== (b.length === 0)) return false;
  const n = Math.max(a.length, b.length);
  const at = (arr, i) => (arr.length === 1 ? arr[0] : arr[i]);
  for (let i = 0; i < n; i++) {
    const av = at(a, i);
    const bv = at(b, i);
    if (av === null || bv === null) continue; // 未描画スロットは照合しない
    if (av !== bv) return false;
  }
  return true;
}

// 両モデルの geometry を照合する。pass は要素数(構造)と要素ごとの正規化座標
// (nx/ny/nw/nh、プロット領域基準)で判定する。後者は各 bar の位置(nx)・幅(nw)・
// 上端(ny)・高さ(nh)を tolerance 内で検証するため、「左→右順序」「系列ごと高さの
// 増減傾向」を包含する。以前あった order/monotonicity の構造チェックは、この要素ごと
// 数値照合と冗長なうえ、tolerance 未満のノイズで誤検出しやすい(例: 高さの符号バケットが
// 微差でブレる/スタックで nx 同値のタイ順がエンジン間で揺れる)ため削除した。
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
  // キーは構造上一意(各 bar = 1 系列 × 1 カテゴリ index)なので last-wins のペアリングで安全。
  const cmap = new Map(cg.elements.map((e) => [key(e), e]));

  // 数値: (series,index) で対応付けて nx/ny/nw/nh を比較。
  for (const fe of fg.elements) {
    const ce = cmap.get(key(fe));
    if (!ce) {
      diffs.push({ field: `elem[${key(fe)}]`, fulgur: 'present', chartjs: 'missing' });
      continue;
    }
    // kind は要素の契約値(現状は両側 'bar')。不一致は座標が偶然合っても FAIL。
    if (fe.kind !== ce.kind)
      diffs.push({ field: `elem[${key(fe)}].kind`, fulgur: fe.kind, chartjs: ce.kind });
    for (const k of ['nx', 'ny', 'nw', 'nh']) {
      const d = Math.abs(fe[k] - ce[k]);
      // NaN/undefined 座標は Math.abs が NaN になり `NaN > tol` は false なので
      // 差分として検出されない。Number.isFinite で異常値も FAIL 扱いにする。
      if (!Number.isFinite(d) || d > tol)
        diffs.push({ field: `elem[${key(fe)}].${k}`, fulgur: fe[k], chartjs: ce[k] });
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
