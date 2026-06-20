import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { fmtAlpha } from './color-util.mjs';

// Rust fmt_alpha と JS fmtAlpha が byte-for-byte 一致することを保証する
// クロス言語フィクスチャ。同じ JSON を Rust 側テストも(インライン化して)読む。
const fixturePath = fileURLToPath(new URL('./rgba-fixture.json', import.meta.url));
const rows = JSON.parse(readFileSync(fixturePath, 'utf8'));

test('rgba fixture: JS fmtAlpha が全行で expected と一致', () => {
  for (const [r, g, b, a, expected] of rows) {
    const got = `rgba(${r},${g},${b},${fmtAlpha(a)})`;
    assert.equal(got, expected, `row r=${r} g=${g} b=${b} a=${a}`);
  }
});
