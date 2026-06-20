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
//!
//! 走査対象(M2): データマークのタグ(rect/circle/path/polyline/polygon)のみを
//!   見る。グリッド線・軸ベースライン(`<line>`)とラベル・タイトル(`<text>`)は
//!   chrome であり系列塗装ではないため除外する。これにより、系列色が chrome の色
//!   (例: テキスト #666666 と系列 rgba(102,102,102,…))と衝突しても、chrome の
//!   alpha が系列の主張集合と誤照合されることを防ぐ。
//! 役割別追跡(M2b): fill と stroke の alpha を役割ごとに分けて検証する。fill と
//!   stroke が同一 RGB・異なる alpha(棒の典型: fill 0.5 / stroke 1)を持つ場合に、
//!   一方の役割の主張で他方の塗装を取りこぼさないようにする。
//! caveat (M3): fulgur は opacity を fmt_num(小数 2 桁)で出力する一方、alpha は
//!   canonical 3 桁で正規化する。パレット alpha {0.25,0.5,0.75,1} では両者は一致する
//!   が、0.01 未満の alpha では乖離しうる(現状は認識のみ)。

import { fmtAlpha } from './color-util.mjs';

const ROLES = [
  ['fill', 'fill-opacity'],
  ['stroke', 'stroke-opacity'],
];

function normalizeHex(hex) {
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

/// SVG 文字列から、データマークの fill/stroke を役割別に抽出する。
/// 返り値: { fill: Map<rgb, Set<alpha>>, stroke: Map<rgb, Set<alpha>> }。
/// chrome(`<line>` グリッド/軸・`<text>` ラベル)は走査対象外(M2 参照)。
export function svgByRole(svg) {
  const byRole = { fill: new Map(), stroke: new Map() };
  const tagRe = /<(rect|circle|path|polyline|polygon)\b[^>]*>/g;
  let m;
  while ((m = tagRe.exec(svg))) {
    const tag = m[0];
    for (const [role, opName] of ROLES) {
      const cm = tag.match(new RegExp(`\\b${role}="(#[0-9a-fA-F]{6})"`));
      if (!cm) continue;
      const om = tag.match(new RegExp(`\\b${opName}="([0-9.]+)"`));
      const a = om ? parseFloat(om[1]) : 1;
      const rgb = normalizeHex(cm[1]);
      const map = byRole[role];
      if (!map.has(rgb)) map.set(rgb, new Set());
      map.get(rgb).add(fmtAlpha(a));
    }
  }
  return byRole;
}

export function crosscheckColors(model, svg) {
  const svgByR = svgByRole(svg);

  // モデルが主張する色を役割別に集計(rgb → 主張 alpha の Set)。alpha 0 は無視。
  const modelByRole = { fill: new Map(), stroke: new Map() };
  for (const s of model.series) {
    for (const [role, list] of [
      ['fill', s.fill],
      ['stroke', s.stroke],
    ]) {
      for (const c of list || []) {
        const p = parseRgba(c);
        if (!p || p.alpha === '0') continue;
        const map = modelByRole[role];
        if (!map.has(p.rgb)) map.set(p.rgb, new Set());
        map.get(p.rgb).add(p.alpha);
      }
    }
  }

  // 1. alpha 整合性(役割別): モデルにもある (role,rgb) について painted ⊆ claimed。
  //    塗られていなければ(2)で系列単位に判定。
  const divergences = [];
  for (const role of ['fill', 'stroke']) {
    for (const [rgb, claimedAlphas] of modelByRole[role]) {
      const paintedAlphas = svgByR[role].get(rgb);
      if (!paintedAlphas) continue;
      for (const paintedAlpha of paintedAlphas) {
        if (!claimedAlphas.has(paintedAlpha)) {
          divergences.push({
            role,
            rgb,
            paintedAlpha,
            modelAlphas: [...claimedAlphas],
          });
        }
      }
    }
  }

  // 2. 系列単位: 各系列の rgb のうち少なくとも 1 つが(役割を問わず)塗られている。
  //    丸ごと欠落/誤りのみを捉えるため、ここでは fill/stroke を区別せず存在判定する。
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
    const anyPainted = rgbs.some(
      (rgb) => svgByR.fill.has(rgb) || svgByR.stroke.has(rgb),
    );
    if (!anyPainted) {
      unpainted.push({ series: i, rgbs: [...new Set(rgbs)] });
    }
  }

  const pass = divergences.length === 0 && unpainted.length === 0;
  return { pass, divergences, unpainted };
}
