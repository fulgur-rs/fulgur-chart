//! 描画忠実性 cross-check: fulgur 意味モデルが主張する色が、実 fulgur SVG に
//! 実在するか(かつ意図しない alpha で描かれていないか)を検証する。
//!
//! fulgur SVG は色を `fill="#rrggbb" fill-opacity="0.5"`(stroke も同様、opacity は
//! alpha==1 のとき省略)で出力する。これを (rgb hex, alpha 文字列) ペアへ正規化し:
//!  1. alpha 整合性: モデルにも存在する rgb について、SVG が実際に塗った alpha は
//!     すべてモデルの主張集合の要素でなければならない(painted ⊆ claimed)。
//!     これにより alpha 乗算バグ(モデル {0.5,1} だが SVG が 0.25 を塗る)を検出。
//!     逆方向(モデルが主張するが SVG に無い)は失敗にしない — 非エリア線の fill は
//!     塗られないため(false positive 回避)。
//!  2. 系列単位の最低保証: 各系列の fill/stroke の rgb のうち少なくとも 1 つは
//!     SVG に塗られていなければならない(系列色丸ごと欠落/誤りを検出)。

function fmtAlpha(a) {
  if (a >= 1) return '1';
  if (a <= 0) return '0';
  return String(Math.round(a * 1000) / 1000);
}

function hexToRgbHex(hex) {
  return hex.toLowerCase();
}

// `rgba(r,g,b,a)` → { rgb: '#rrggbb', alpha: '<canonical>' }。
function parseRgba(s) {
  const m = s.match(/^rgba\((\d+),(\d+),(\d+),([\d.]+)\)$/);
  if (!m) return null;
  const r = +m[1];
  const g = +m[2];
  const b = +m[3];
  const rgb =
    '#' +
    [r, g, b].map((v) => v.toString(16).padStart(2, '0')).join('');
  return { rgb, alpha: fmtAlpha(parseFloat(m[4])) };
}

/// SVG 文字列から、各要素タグの fill/fill-opacity・stroke/stroke-opacity ペアを
/// 抽出し、RGB hex(小文字)→ 実際に塗られた canonical alpha 文字列の Set を返す。
export function svgByRgbMap(svg) {
  const byRgb = new Map();
  const tagRe = /<(rect|circle|path|line|polyline|polygon|text)\b[^>]*>/g;
  let m;
  while ((m = tagRe.exec(svg))) {
    const tag = m[0];
    for (const [kind, opName] of [
      ['fill', 'fill-opacity'],
      ['stroke', 'stroke-opacity'],
    ]) {
      const cm = tag.match(new RegExp(`\\b${kind}="(#[0-9a-fA-F]{6})"`));
      if (!cm) continue;
      const om = tag.match(new RegExp(`\\b${opName}="([0-9.]+)"`));
      const a = om ? parseFloat(om[1]) : 1;
      const rgb = hexToRgbHex(cm[1]);
      const alpha = fmtAlpha(a);
      if (!byRgb.has(rgb)) byRgb.set(rgb, new Set());
      byRgb.get(rgb).add(alpha);
    }
  }
  return byRgb;
}

/// 後方互換の参考ヘルパ: 塗られた色を canonical rgba 文字列の Set にする。
export function svgColorSet(svg) {
  const set = new Set();
  for (const [rgb, alphas] of svgByRgbMap(svg)) {
    const [r, g, b] = [
      parseInt(rgb.slice(1, 3), 16),
      parseInt(rgb.slice(3, 5), 16),
      parseInt(rgb.slice(5, 7), 16),
    ];
    for (const a of alphas) set.add(`rgba(${r},${g},${b},${a})`);
  }
  return set;
}

export function crosscheckColors(model, svg) {
  const svgByRgb = svgByRgbMap(svg);

  // モデルが主張する色集合(rgb → 主張 alpha の Set)。alpha 0(完全透明)は無視。
  const modelByRgb = new Map();
  for (const s of model.series) {
    for (const c of [...(s.fill || []), ...(s.stroke || [])]) {
      const p = parseRgba(c);
      if (!p || p.alpha === '0') continue;
      if (!modelByRgb.has(p.rgb)) modelByRgb.set(p.rgb, new Set());
      modelByRgb.get(p.rgb).add(p.alpha);
    }
  }

  // 1. alpha 整合性: モデルにもある rgb について painted ⊆ claimed。
  const divergences = [];
  for (const [rgb, claimedAlphas] of modelByRgb) {
    const paintedAlphas = svgByRgb.get(rgb);
    if (!paintedAlphas) continue; // 塗られていなければ(2)で系列単位に判定。
    for (const paintedAlpha of paintedAlphas) {
      if (!claimedAlphas.has(paintedAlpha)) {
        divergences.push({
          rgb,
          paintedAlpha,
          modelAlphas: [...claimedAlphas],
        });
      }
    }
  }

  // 2. 系列単位: 各系列の fill/stroke の rgb の少なくとも 1 つが SVG に塗られている。
  const unpainted = [];
  for (let i = 0; i < model.series.length; i++) {
    const s = model.series[i];
    const rgbs = [];
    for (const c of [...(s.fill || []), ...(s.stroke || [])]) {
      const p = parseRgba(c);
      if (!p || p.alpha === '0') continue;
      rgbs.push(p.rgb);
    }
    if (rgbs.length === 0) continue; // 主張色が無ければ判定対象外。
    const anyPainted = rgbs.some((rgb) => svgByRgb.has(rgb));
    if (!anyPainted) {
      unpainted.push({ series: i, rgbs: [...new Set(rgbs)] });
    }
  }

  const pass = divergences.length === 0 && unpainted.length === 0;
  return { pass, divergences, unpainted };
}
