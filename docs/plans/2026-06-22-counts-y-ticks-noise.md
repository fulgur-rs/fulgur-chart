# counts.y_ticks ノイズ除去 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `diff.mjs` の counts 比較ゲートから `y_ticks` を除外し、tick算法の差異によって発生する冗長な counts 失敗を取り除く。

**Architecture:** `counts.y_ticks` は常に `axes.y.ticks.length` と同義なので、axes 次元が失敗すれば必ず counts 次元も失敗する二重カウントになっている。diff.mjs のループから `y_ticks` を除外するだけで、axes 比較が通れば counts は通るという正しい状態になる。モデルスキーマ（Rust・extract.mjs）は変更せず、比較ロジックのみ修正する。

**Tech Stack:** JavaScript (ES modules), Node.js test runner (`node --test`), Rust (fulgur-chart), cargo test

---

### Task 1: y_ticks を counts ゲートから除外する

**Files:**
- Modify: `tools/chartjs-compat/diff.mjs:71`

**Step 1: 現状の失敗を確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise/tools
npm test 2>&1 | tail -10
```

Expected: 20 pass, 0 fail

**Step 2: 失敗テストを追加する（TDD: 先に書く）**

`tools/chartjs-compat/diff.test.mjs` の末尾に追加:

```js
test('y_ticks の差分は counts 失敗を引き起こさない', () => {
  const f = base(); const c = base();
  c.counts.y_ticks = 99; // わざと違う値
  const r = diffModels(f, c);
  assert.equal(r.dimensions.counts.pass, true, 'counts は y_ticks を無視するべき');
  assert.equal(r.pass, true);
});
```

**Step 3: テストを実行して FAIL を確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise/tools
npm test 2>&1 | grep -E "pass|fail|y_ticks"
```

Expected: 20 pass, 1 fail (`y_ticks の差分は counts 失敗を引き起こさない` が失敗)

**Step 4: diff.mjs を修正**

`tools/chartjs-compat/diff.mjs` の71行目:

```js
// Before
for (const k of ['datasets', 'legend_items', 'x_ticks', 'y_ticks']) {

// After
for (const k of ['datasets', 'legend_items', 'x_ticks']) {
```

**Step 5: テストを実行して PASS を確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise/tools
npm test 2>&1 | tail -10
```

Expected: 21 pass, 0 fail

**Step 6: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise
git add tools/chartjs-compat/diff.mjs tools/chartjs-compat/diff.test.mjs
git commit -m "fix: remove y_ticks from counts comparison gate

counts.y_ticks は axes.y.ticks.length と同義であり、tick 算法の差異が
あると counts と axes の両次元が同時に失敗するノイズになっていた。
counts ゲートから y_ticks を除外し、軸比較が主担当となるよう整理する。"
```

---

### Task 2: extract.mjs のコメントを補足する（任意）

**Files:**
- Modify: `tools/chartjs-compat/extract.mjs:77-97`

**Step 1: コメントを更新**

`extract.mjs` の77行目付近のコメントブロックを以下に置き換える:

```js
  // 軸(線形スケールがあれば)。値(線形)軸→y、カテゴリ→x の正規化規約。
  // fulgur 側 model.rs の compute_axes も同じ規約で値軸を y に載せるため
  // apples-to-apples 照合が成立する。
  // scatter/bubble は x・y とも linear なので axis==='y' を優先して y-linear を選ぶ。
  // 横棒(indexAxis:'y')は chart.js の linear scale が x 軸に付くため
  // axis==='y' では見つからず、fallback で x-linear を axes.y に載せる。
  // counts.y_ticks は diff.mjs では比較されない(axes 次元が担当するため)。
```

**Step 2: テストが引き続き通ることを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise/tools
npm test 2>&1 | tail -5
```

Expected: 21 pass, 0 fail

**Step 3: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise
git add tools/chartjs-compat/extract.mjs
git commit -m "docs: clarify axis normalization convention in extract.mjs"
```

---

### Task 3: compat tool で結果確認

**Step 1: Rustバイナリをビルド**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise
cargo build -p fulgur-chart-cli 2>&1 | tail -3
```

**Step 2: compat tool を実行**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fix/counts-y-ticks-noise
node tools/chartjs-compat/compat.mjs 2>&1 | grep -E "PASS|FAIL|SKIP|ERROR|Summary"
```

Expected (修正後):
```
FAIL bar [colors]
FAIL bar-horizontal [colors]
FAIL line [colors, axes, crosscheck]    ← counts が消えた
FAIL stacked-bar [colors]
PASS pie
PASS doughnut
FAIL area [axes]                        ← counts が消えた
PASS scatter
PASS bubble
Summary: 4 passed / 5 failed / 0 skipped
```

`line` から `counts` が消え、`area` から `counts` が消えていれば成功。

**No commit needed for Task 3** (確認のみ)
