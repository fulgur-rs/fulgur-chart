# Compat colors 次元 paint-state 意味化 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chart.js 適合参照ツールの colors 次元を「宣言された色スロット」から「実際に描画される色(paint-state)」へ意味化し、`borderWidth:0` / `fill:false` の未描画スロット由来の false-positive を除去する(beads: fulgur-chart-4lf)。

**Architecture:** `extract.mjs`(chart.js 側抽出)で未描画スロット(`borderWidth===0` の stroke、`dataset.fill===false` の fill)を `null` 化し、`diff.mjs` の `colorsEqual` で「どちらかが null なら照合 skip」にする。fulgur 側(Rust / model.rs)は無変更。

**Tech Stack:** Node.js (ESM, `node:test`), chart.js v4 + node-canvas(抽出)、対象は `tools/chartjs-compat/`。

**作業ディレクトリ:** `.worktrees/feat/compat-colors-paint-state/`(branch `feat/compat-colors-paint-state`)。テストは `cd tools && npm test`(= `node --test "chartjs-compat/*.test.mjs"`)。`tools/node_modules` は main からの symlink(canvas/chart.js 利用可)。

**実測根拠(ground truth):**
- 既定パレット bar / 明示色 bar: 解決済み `borderWidth=0`(枠線未描画)だが `borderColor` は解決済み既定色を持つ → 現状 stroke にノイズ。
- line(`fill:false`): fulgur モデル `fill` は area 塗り色(@0.5)だが未描画。chart.js extract は point 背景 `rgba(0,0,0,0.1)` を読む → 現状 fill にノイズ。
- area(`fill:true`)/ pie / doughnut / scatter / bubble: `borderWidth>0` かつ fill 描画あり → 変更後も PASS 維持。

---

## Task 1: diff.mjs — colorsEqual の null スキップ

純関数の変更。canvas 不要で高速。先に実装すると Task 3 の統合が通る土台になる。

**Files:**
- Modify: `tools/chartjs-compat/diff.mjs`(`colorsEqual`、18-25 行付近)
- Test: `tools/chartjs-compat/diff.test.mjs`(末尾に追加)

**Step 1: 失敗するテストを書く**

`tools/chartjs-compat/diff.test.mjs` の末尾に追加:

```javascript
test('paint-state: 片側 null スロットは colors 照合から除外され PASS', () => {
  // chart.js 側が未描画(null)、fulgur 側は色 → 照合対象外で PASS。
  const f = base(); const c = base();
  c.series[0].stroke = [null]; // borderWidth:0 で未描画
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, true);
});

test('paint-state: 混在 null/色は非 null スロットのみ照合', () => {
  const f = base(); const c = base();
  f.series[0].fill = ['rgba(1,1,1,1)', 'rgba(2,2,2,1)'];
  c.series[0].fill = [null, 'rgba(2,2,2,1)']; // 0番は未描画、1番は一致
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, true);
});

test('paint-state: 混在で非 null スロットが不一致なら FAIL', () => {
  const f = base(); const c = base();
  f.series[0].fill = ['rgba(1,1,1,1)', 'rgba(2,2,2,1)'];
  c.series[0].fill = [null, 'rgba(9,9,9,1)']; // 1番が不一致
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, false);
});

test('paint-state: null を含まない通常の不一致は従来どおり FAIL', () => {
  const f = base(); const c = base();
  c.series[0].fill = ['rgba(99,99,99,0.5)'];
  const r = diffModels(f, c);
  assert.equal(r.dimensions.colors.pass, false);
});
```

**Step 2: テストが失敗することを確認**

Run: `cd tools && node --test "chartjs-compat/diff.test.mjs"`
Expected: 「片側 null スロット」「混在 null/色」のテストが FAIL(現状 `colorsEqual` は `null !== 'rgba(...)'` で不一致 → colors.pass=false)。「非 null 不一致は FAIL」系は PASS のまま。

**Step 3: 最小実装**

`tools/chartjs-compat/diff.mjs` の `colorsEqual` を次へ置換(コメントも更新):

```javascript
// fulgur/chart.js の fill 表現(長さ1 畳み or 要素配列)を要素数 n に展開して比較。
// paint-state: どちらかのスロットが null(= そのエンジンで未描画)なら、そのスロットは
// 「可視描画差」ではないため照合対象外にする(false-positive 抑止)。
function colorsEqual(a, b) {
  if (!Array.isArray(a) || !Array.isArray(b)) return false;
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
```

**Step 4: テストが通ることを確認**

Run: `cd tools && node --test "chartjs-compat/diff.test.mjs"`
Expected: PASS(追加4件含む全件)。

**Step 5: コミット**

```bash
git add tools/chartjs-compat/diff.mjs tools/chartjs-compat/diff.test.mjs
git commit -m "feat(compat): colorsEqual で paint-state null スロットを照合除外"
```

---

## Task 2: extract.mjs — paint-state 捕捉(未描画スロットの null 化)

**Files:**
- Modify: `tools/chartjs-compat/extract.mjs`(`extractChartjsModel` の series 構築、119-139 行付近)
- Test: `tools/chartjs-compat/extract.test.mjs`(既存1件を更新 + 新規追加)

**Step 1: 失敗するテストを書く(既存更新 + 新規追加)**

(a) 既存テスト「bar: 既定パレット色は canonical rgba に正規化される」(19-28 行)の `stroke` 期待値を新セマンティクスへ更新する。既定パレット bar は解決済み `borderWidth=0` のため stroke は未描画 = `[null]`。`fill` は bar が常に塗るため不変。該当 assert を次へ:

```javascript
  // 既定パレット先頭 #36A2EB、fill alpha=0.5(塗りは常に描画)。
  assert.deepEqual(model.series[0].fill, ['rgba(54,162,235,0.5)']);
  // 既定 bar は borderWidth:0(枠線未描画)→ paint-state で stroke は null。
  assert.deepEqual(model.series[0].stroke, [null]);
```

(b) `extract.test.mjs` の末尾に新規追加:

```javascript
test('paint-state: 既定 bar の stroke は未描画(borderWidth:0)で null', async () => {
  const spec = { type: 'bar', data: { labels: ['a','b'],
    datasets: [{ data: [10,90] }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.deepEqual(model.series[0].stroke, [null]);
  // 塗りは描画されるので色を保持。
  assert.notEqual(model.series[0].fill[0], null);
});

test('paint-state: line(fill:false)の fill は未描画(area 未塗り)で null', async () => {
  const spec = { type: 'line', data: { labels: ['a','b','c'],
    datasets: [{ data: [1,2,3], borderColor: '#ff6384', fill: false }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.deepEqual(model.series[0].fill, [null]);
  // 線は描画されるので stroke は色を保持。
  assert.notEqual(model.series[0].stroke[0], null);
});

test('paint-state: area(fill:true)の fill は描画されるので色を保持(回帰防止)', async () => {
  const spec = { type: 'line', data: { labels: ['a','b','c'],
    datasets: [{ data: [1,2,3], borderColor: '#4bc0c0',
      backgroundColor: '#4bc0c0', fill: true }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.notEqual(model.series[0].fill[0], null);
  assert.notEqual(model.series[0].stroke[0], null);
});
```

**Step 2: テストが失敗することを確認**

Run: `cd tools && node --test "chartjs-compat/extract.test.mjs"`
Expected: 更新した既存テストと新規 paint-state テスト(bar stroke null / line fill null)が FAIL(現状 extract は未描画でも解決済み色を出す)。area テストは PASS のまま。

**Step 3: 最小実装**

`tools/chartjs-compat/extract.mjs` の `extractChartjsModel` 内 series 構築(現状 119-139 行)を次へ置換。`fill`/`stroke` を per-slot で paint-state 判定する:

```javascript
  const series = spec.data.datasets.map((ds, i) => {
    const meta = chart.getDatasetMeta(i);
    const n = meta.data.length || (ds.data ? ds.data.length : 0);
    // dataset の area 塗りが無効(line の fill:false 等)なら fill は未描画。
    // bar/scatter 等は dataset 要素を持たず undefined のため塗り扱い(null にしない)。
    const fillUnpainted = meta.dataset?.options?.fill === false;
    // 描画後の解決済み element options を使う(生 dataset プロパティではない)。
    // paint-state: 未描画スロットは解決済み既定色ではなく null を出し、diff で照合除外する。
    const fill = collapse(
      Array.from({ length: n }, (_, j) =>
        fillUnpainted
          ? null
          : toRgba(meta.data[j]?.options?.backgroundColor ?? '#000'),
      ),
    );
    const stroke = collapse(
      Array.from({ length: n }, (_, j) => {
        // borderWidth:0 は枠線未描画(bar の既定等)→ stroke は null。
        if (meta.data[j]?.options?.borderWidth === 0) return null;
        return toRgba(meta.data[j]?.options?.borderColor ?? '#000');
      }),
    );
    const values = Array.isArray(ds.data)
      ? ds.data.map((d) =>
          typeof d === 'object' && d !== null ? (d.y ?? d.v ?? null) : d,
        )
      : [];
    return { label: ds.label ?? '', fill, stroke, values };
  });
```

注: `collapse([null,null]) === [null]`(`null===null`)。混在 `[null,'X']` は展開維持され diff 側で per-slot 判定される。`collapse` は変更不要。

**Step 4: テストが通ることを確認**

Run: `cd tools && node --test "chartjs-compat/extract.test.mjs"`
Expected: PASS(更新済み既存 + 新規3件含む全件)。

**Step 5: コミット**

```bash
git add tools/chartjs-compat/extract.mjs tools/chartjs-compat/extract.test.mjs
git commit -m "feat(compat): extract で未描画スロット(borderWidth0/fill偽)を paint-state null 化"
```

---

## Task 3: 統合検証(compat.mjs で全 spec の colors 次元を確認)

コードは Task 1/2 で完了。ここは受け入れ基準の実機検証(fulgur を cargo build するため時間がかかる)。

**Files:**
- 変更なし(検証のみ)。差分が出れば原因を切り分けて Task 1/2 に戻る。

**Step 1: 全ツールテストがグリーンか確認**

Run: `cd tools && npm test`
Expected: 全 `.test.mjs` PASS(diff/extract/crosscheck/rgba-fixture)。

**Step 2: compat ハーネスを実行**

Run: `cd tools && node chartjs-compat/compat.mjs`
(初回は `cargo build -p fulgur-chart-cli` が走る。)

**Step 3: colors 次元の結果を検証**

各 spec のレポート JSON から colors 次元を確認:

Run:
```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat/compat-colors-paint-state
for f in tools/report/*.json; do node -e "const r=require('./$f'); const c=r.diff?.dimensions?.colors; console.log(r.name.padEnd(16), c?.pass?'PASS':'FAIL', JSON.stringify(c?.diffs||[]));"; done
```
Expected:
- bar / bar-horizontal / line / stacked-bar の colors が **PASS**(以前は FAIL)。
- area / pie / doughnut / scatter / bubble の colors が **PASS 維持**。
- いずれの colors diffs も空配列。

注: 他次元(geometry/axes 等)や全体 pass は本 issue のスコープ外。colors 次元のみを判定する。万一 colors に残差分が出たら、その spec の paint-state 判定漏れを切り分けて Task 2 を修正。

**Step 4: 受け入れ基準の最終確認**

- [ ] bar / bar-horizontal / line / stacked-bar の colors 次元が PASS
- [ ] area / pie / doughnut / scatter / bubble の colors 次元が PASS 維持
- [ ] `npm test` 全件グリーン
- [ ] fulgur 側(Rust / model.rs)は無変更(`git diff --stat` で crates/ に変更が無いこと)

**Step 5: コミット(検証ログを残す場合のみ)**

通常コード変更は無し。レポート生成物(`tools/report/`)は gitignore 対象か確認し、追跡対象なら別途判断。検証のみで差分が無ければコミット不要。
