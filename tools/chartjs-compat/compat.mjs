#!/usr/bin/env node
//! chart.js v4 適合参照ツールのオーケストレータ。
//!
//! 各 spec について:
//!   1. fulgur CLI (`inspect`/`render`) で意味モデル・SVG・PNG を得る
//!   2. 同じ spec を実 chart.js (node-canvas) で評価し意味モデル+PNG を得る
//!   3. diffModels で数値照合(色/軸/counts)、crosscheckColors で描画忠実性照合
//!   4. JSON + HTML レポートを <root>/tools/report/ に書く
//!   5. 1 行サマリを表示
//!
//! 使い方:  node chartjs-compat/compat.mjs [specNames...]
//! 既定の spec リストは下記。引数があればそれらに絞る。
//! chart.js コアが未対応の型は SKIP(クラッシュさせない)。

import { execSync, execFileSync } from 'node:child_process';
import { readFileSync, existsSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

import { extractChartjsModel } from './extract.mjs';
import { diffModels } from './diff.mjs';
import { crosscheckColors } from './crosscheck.mjs';
import { writeJsonReport, writeHtmlReport } from './report.mjs';

// compat.mjs は <root>/tools/chartjs-compat/compat.mjs。root はそこから '../../'。
const fileDir = dirname(fileURLToPath(import.meta.url));
const root = join(fileDir, '../../');

if (!existsSync(join(root, 'examples/specs'))) {
  console.error(
    `error: could not locate repo root (no examples/specs under ${root})`,
  );
  process.exit(2);
}

const DEFAULT_SPECS = [
  'bar',
  'bar-horizontal',
  'line',
  'area',
  'stacked-bar',
  'pie',
  'doughnut',
  'scatter',
  'bubble',
];

const args = process.argv.slice(2);
const specs = args.length > 0 ? args : DEFAULT_SPECS;

const bin = join(root, 'target/debug/fulgur-chart');
const outDir = join(root, 'tools/report');
mkdirSync(outDir, { recursive: true });

// fulgur バイナリを一度だけビルド。
console.log('Building fulgur-chart-cli...');
execSync('cargo build -p fulgur-chart-cli', { cwd: root, stdio: 'inherit' });

const MAX_BUFFER = 64 * 1024 * 1024;

let passed = 0;
let failed = 0;
let skipped = 0;
let errored = 0;

for (const name of specs) {
  const specPath = join(root, 'examples/specs', `${name}.json`);

  if (!existsSync(specPath)) {
    console.log(`ERROR ${name}: spec file not found (${specPath})`);
    errored++;
    continue;
  }

  // --- fulgur 側(失敗は ERROR、SKIP ではない) ---
  let fulgurModel, fulgurSvg, fulgurPng, specObj;
  try {
    specObj = JSON.parse(readFileSync(specPath, 'utf8'));
    fulgurModel = JSON.parse(
      execFileSync(bin, ['inspect', specPath, '-o', '-'], {
        maxBuffer: MAX_BUFFER,
      }).toString('utf8'),
    );
    fulgurSvg = execFileSync(bin, ['render', specPath, '-o', '-'], {
      maxBuffer: MAX_BUFFER,
    }).toString('utf8');
    fulgurPng = execFileSync(
      bin,
      ['render', specPath, '-o', '-', '--format', 'png'],
      { maxBuffer: MAX_BUFFER },
    );
  } catch (e) {
    const stderr = e.stderr ? `\nStderr: ${e.stderr.toString()}` : '';
    console.log(`ERROR ${name}: fulgur pipeline failed: ${e.message}${stderr}`);
    errored++;
    continue;
  }

  const width = Math.round(fulgurModel.meta.width);
  const height = Math.round(fulgurModel.meta.height);

  // --- chart.js 側(未対応型は SKIP) ---
  let chartjs;
  try {
    chartjs = await extractChartjsModel(specObj, width, height);
  } catch (e) {
    console.log(`SKIP ${name}: chart.js extract failed: ${e.message}`);
    skipped++;
    continue;
  }

  // --- 照合 ---
  const diff = diffModels(fulgurModel, chartjs);
  const cross = crosscheckColors(fulgurModel, fulgurSvg);
  const result = {
    name,
    pass: diff.pass && cross.pass,
    diff,
    cross,
  };

  // --- レポート ---
  writeJsonReport(name, result, outDir);
  writeHtmlReport(name, result, fulgurPng, chartjs.png, outDir);

  // --- サマリ行 ---
  if (result.pass) {
    console.log(`PASS ${name}`);
    passed++;
  } else {
    const failedDims = [];
    for (const [dim, d] of Object.entries(diff.dimensions)) {
      if (!d.pass) failedDims.push(dim);
    }
    if (!cross.pass) failedDims.push('crosscheck');
    console.log(`FAIL ${name} [${failedDims.join(', ')}]`);
    failed++;
  }
}

console.log(
  `\nSummary: ${passed} passed / ${failed} failed / ${skipped} skipped` +
    (errored > 0 ? ` / ${errored} errored` : ''),
);

if (failed > 0 || errored > 0) {
  process.exitCode = 1;
}
