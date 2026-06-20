//! 照合結果の永続化レポータ。compat.mjs が組み立てた result
//! ({ name, pass, diff, cross }) を:
//!  - writeJsonReport: 整形 JSON(全次元の差分・cross-check の divergences/unpainted・総合 pass)
//!  - writeHtmlReport: 自己完結 HTML(次元別 PASS/FAIL バッジ + 両 PNG を base64 で左右 + 差分表)
//! として書き出す。HTML は依存なし・インライン CSS。spec/label テキストはエスケープする。

import { writeFileSync, mkdirSync } from 'node:fs';
import { join } from 'node:path';

/// HTML へ挿入するテキストの最小エスケープ(& < > " ')。
function esc(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/// 任意の値(配列・数値・文字列・null)を読みやすい文字列にして HTML エスケープ。
function fmtVal(v) {
  if (v === null || v === undefined) return esc(String(v));
  if (Array.isArray(v) || typeof v === 'object') return esc(JSON.stringify(v));
  return esc(String(v));
}

export function writeJsonReport(name, result, outDir) {
  mkdirSync(outDir, { recursive: true });
  const path = join(outDir, `${name}.json`);
  writeFileSync(path, JSON.stringify(result, null, 2) + '\n');
  return path;
}

/// PASS/FAIL バッジ HTML。skipped=true なら中立の SKIP バッジ。
function badge(label, pass, skipped) {
  if (skipped) {
    return `<span class="badge skip">${esc(label)}: SKIP</span>`;
  }
  const cls = pass ? 'pass' : 'fail';
  const txt = pass ? 'PASS' : 'FAIL';
  return `<span class="badge ${cls}">${esc(label)}: ${txt}</span>`;
}

/// 色/軸/counts の差分行をテーブル行へ。各 diff は { field, fulgur, chartjs, series? }。
function diffRows(dimName, dim) {
  const diffs = dim && dim.diffs ? dim.diffs : [];
  if (diffs.length === 0) return '';
  return diffs
    .map((d) => {
      const field =
        d.series !== undefined ? `series[${d.series}].${d.field}` : d.field;
      return `<tr><td>${esc(dimName)}</td><td>${esc(field)}</td><td class="f">${fmtVal(
        d.fulgur,
      )}</td><td class="c">${fmtVal(d.chartjs)}</td></tr>`;
    })
    .join('\n');
}

/// cross-check の divergences / unpainted を別テーブルへ。
function crossRows(cross) {
  const rows = [];
  for (const dv of cross.divergences || []) {
    rows.push(
      `<tr><td>divergence</td><td>${esc(dv.rgb)}</td><td class="f">painted alpha ${fmtVal(
        dv.paintedAlpha,
      )}</td><td class="c">model alphas ${fmtVal(dv.modelAlphas)}</td></tr>`,
    );
  }
  for (const up of cross.unpainted || []) {
    rows.push(
      `<tr><td>unpainted</td><td>series[${esc(up.series)}]</td><td class="f">${fmtVal(
        up.rgbs,
      )}</td><td class="c">(none painted in SVG)</td></tr>`,
    );
  }
  return rows.join('\n');
}

export function writeHtmlReport(
  name,
  result,
  fulgurPngBuf,
  chartjsPngBuf,
  outDir,
) {
  mkdirSync(outDir, { recursive: true });
  const path = join(outDir, `${name}.html`);

  const dims = result.diff.dimensions;
  const cross = result.cross;

  const badges = [
    badge('colors', dims.colors.pass, dims.colors.skipped),
    badge('axes', dims.axes.pass, dims.axes.skipped),
    badge('counts', dims.counts.pass, dims.counts.skipped),
    badge('crosscheck', cross.pass, false),
  ].join(' ');

  const overall = result.pass
    ? '<span class="badge pass big">OVERALL: PASS</span>'
    : '<span class="badge fail big">OVERALL: FAIL</span>';

  const fulgurB64 = Buffer.isBuffer(fulgurPngBuf)
    ? fulgurPngBuf.toString('base64')
    : Buffer.from(fulgurPngBuf).toString('base64');
  const chartjsB64 = Buffer.isBuffer(chartjsPngBuf)
    ? chartjsPngBuf.toString('base64')
    : Buffer.from(chartjsPngBuf).toString('base64');

  const diffTableBody = [
    diffRows('colors', dims.colors),
    diffRows('axes', dims.axes),
    diffRows('counts', dims.counts),
  ]
    .filter(Boolean)
    .join('\n');

  const crossTableBody = crossRows(cross);

  const diffSection =
    diffTableBody.length > 0
      ? `<h2>Semantic diffs (fulgur vs chart.js)</h2>
<table>
  <thead><tr><th>dimension</th><th>field</th><th>fulgur</th><th>chart.js</th></tr></thead>
  <tbody>
${diffTableBody}
  </tbody>
</table>`
      : '<h2>Semantic diffs</h2><p class="ok">No differences.</p>';

  const crossSection =
    crossTableBody.length > 0
      ? `<h2>Cross-check divergences / unpainted</h2>
<table>
  <thead><tr><th>kind</th><th>key</th><th>painted / claimed</th><th>note</th></tr></thead>
  <tbody>
${crossTableBody}
  </tbody>
</table>`
      : '<h2>Cross-check</h2><p class="ok">All claimed colors painted with consistent alpha.</p>';

  const html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>compat: ${esc(name)}</title>
<style>
  body { font-family: system-ui, -apple-system, sans-serif; margin: 24px; color: #222; }
  h1 { font-size: 20px; }
  h2 { font-size: 16px; margin-top: 28px; }
  .badge { display: inline-block; padding: 4px 10px; border-radius: 4px; font-size: 13px; font-weight: 600; margin: 2px 4px 2px 0; color: #fff; }
  .badge.pass { background: #2e7d32; }
  .badge.fail { background: #c62828; }
  .badge.skip { background: #757575; }
  .badge.big { font-size: 15px; padding: 6px 14px; }
  .images { display: flex; gap: 16px; flex-wrap: wrap; margin-top: 16px; }
  .images figure { margin: 0; border: 1px solid #ddd; padding: 8px; border-radius: 6px; background: #fafafa; }
  .images figcaption { font-weight: 600; margin-bottom: 6px; }
  .images img { display: block; max-width: 480px; height: auto; background: #fff; }
  table { border-collapse: collapse; margin-top: 8px; font-size: 13px; }
  th, td { border: 1px solid #ccc; padding: 4px 8px; text-align: left; vertical-align: top; }
  th { background: #f0f0f0; }
  td.f { color: #1565c0; }
  td.c { color: #6a1b9a; }
  p.ok { color: #2e7d32; }
</style>
</head>
<body>
<h1>chart.js compat report: ${esc(name)}</h1>
<p>${badges} &nbsp; ${overall}</p>
<div class="images">
  <figure>
    <figcaption>fulgur</figcaption>
    <img alt="fulgur ${esc(name)}" src="data:image/png;base64,${fulgurB64}">
  </figure>
  <figure>
    <figcaption>chart.js</figcaption>
    <img alt="chart.js ${esc(name)}" src="data:image/png;base64,${chartjsB64}">
  </figure>
</div>
${diffSection}
${crossSection}
</body>
</html>
`;

  writeFileSync(path, html);
  return path;
}
