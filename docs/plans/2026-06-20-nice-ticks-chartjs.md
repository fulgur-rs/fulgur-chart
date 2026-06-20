# nice-ticks chart.js 一致 実装プラン

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `scale.rs` の `nice_ticks` を chart.js v4 の目盛り挙動（`maxTicksLimit=11`、`beginAtZero`、`suggestedMin`/`suggestedMax`）に精密一致させる。

**Architecture:** (1) Node.js スクリプトで chart.js 実出力を取得して差分を確定する。(2) `nice_ticks` の呼び出し側 `target_count` を 5→10 に修正する。(3) `AxisSpec` に `suggested_min`/`suggested_max` を追加して frontend と `value_domain` に接続する。(4) 確定した期待値で scale.rs にピンテストを追加する。

**Tech Stack:** Rust (`crate:fulgur-chart`)、Node.js v24 + `canvas` + `chart.js` ESM

---

### Task 1: 調査スクリプト `tools/chartjs_ticks.mjs` を作成して実行する

**Files:**
- Create: `tools/package.json`
- Create: `tools/chartjs_ticks.mjs`

**Step 1: `tools/` ディレクトリを初期化して依存パッケージをインストールする**

```bash
mkdir -p tools
cd tools
npm init -y
npm install canvas chart.js
```

Expected: `tools/node_modules/` が作成される。

**Step 2: `tools/chartjs_ticks.mjs` を書く**

```js
// chart.js v4 の LinearScale が実際に生成する目盛りを抽出する。
// canvas (node-canvas) で DOM なし環境でも Chart インスタンスを構築できる。

import { createCanvas } from 'canvas';
import { Chart } from 'chart.js/auto';

// グローバルフォント警告を抑制する。
Chart.defaults.font.size = 12;

async function getTicks(label, data, yOpts = {}) {
  const canvas = createCanvas(800, 400);
  const ctx = canvas.getContext('2d');

  const chart = new Chart(ctx, {
    type: 'bar',
    data: {
      labels: ['x'],
      datasets: [{ data }],
    },
    options: {
      animation: false,
      scales: { y: { ...yOpts } },
    },
  });

  const scale = chart.scales.y;
  const result = {
    label,
    data,
    yOpts,
    min: scale.min,
    max: scale.max,
    ticks: scale.ticks.map((t) => t.value),
    step: scale.ticks.length >= 2 ? scale.ticks[1].value - scale.ticks[0].value : null,
  };

  chart.destroy();
  return result;
}

const cases = [
  // beginAtZero: false (デフォルト)
  ['[0,100] default', [0, 100], {}],
  ['[0,173] default', [0, 173], {}],
  ['[-30,70] default', [-30, 70], {}],
  ['[0,1] default', [0, 1.0], {}],
  ['[100,10000] default', [100, 10000], {}],
  // beginAtZero: true
  ['[50,200] beginAtZero:true', [50, 200], { beginAtZero: true }],
  ['[-10,90] beginAtZero:true', [-10, 90], { beginAtZero: true }],
  // suggestedMin / suggestedMax
  ['[0,100] suggestedMin:-20', [0, 100], { suggestedMin: -20 }],
  ['[0,100] suggestedMax:150', [0, 100], { suggestedMax: 150 }],
];

const results = [];
for (const [label, data, opts] of cases) {
  results.push(await getTicks(label, data, opts));
}

console.log(JSON.stringify(results, null, 2));
```

**Step 3: スクリプトを実行して出力を確認する**

```bash
cd tools && node chartjs_ticks.mjs
```

Expected: 9 ケースの JSON が出力される。`ticks` 配列が `[0, 10, 20, ..., 100]` のように chart.js の挙動を反映していること。

**Step 4: 出力をファイルに保存する**

```bash
cd tools && node chartjs_ticks.mjs > chartjs_ticks_output.json
cat chartjs_ticks_output.json
```

`chartjs_ticks_output.json` は `tools/.gitignore` に追加してコミットしない（生成物）。

**Step 5: `.gitignore` を作成する**

`tools/.gitignore` を作成：
```
node_modules/
chartjs_ticks_output.json
```

**Step 6: Commit**

```bash
git add tools/package.json tools/chartjs_ticks.mjs tools/.gitignore
git commit -m "feat(tools): add chartjs_ticks.mjs investigation script"
```

---

### Task 2: `nice_ticks` の `target_count` を 5 から 10 に変更する

> **前提:** Task 1 の出力で chart.js がデフォルト 11 目盛り（10 インターバル）を生成することを確認済み。
> 現在のコードは `target_count=5`（5 インターバル）を渡しているため、目盛り数が約半分になっている。

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs:114`
- Modify: `crates/fulgur-chart/src/layout/bar.rs:150`
- Modify: `crates/fulgur-chart/src/layout/scatter.rs:83-84`
- Modify: `crates/fulgur-chart/src/layout/radar.rs:176`
- Modify: `crates/fulgur-chart/src/scale.rs` (既存テストの期待値更新)

**Step 1: 4 箇所の呼び出しを `target_count=10` に変更する**

`crates/fulgur-chart/src/layout/common.rs:114`:
```rust
let ticks = nice_ticks(domain_min, domain_max, 10);
```

`crates/fulgur-chart/src/layout/bar.rs:150`:
```rust
let ticks = nice_ticks(dmin, dmax, 10);
```

`crates/fulgur-chart/src/layout/scatter.rs:83-84`:
```rust
let x_ticks = nice_ticks(xmin, xmax, 10);
let y_ticks = nice_ticks(ymin, ymax, 10);
```

`crates/fulgur-chart/src/layout/radar.rs:176`:
```rust
let nice = nice_ticks(0.0, max_val, 10);
```

**Step 2: `scale.rs` の既存テストを新しい期待値に更新する**

`nice_ticks(0.0, 200.0, 5)` が `[0,50,100,150,200]` を生成していたが、
`nice_ticks(0.0, 200.0, 10)` は step=20 → `[0,20,40,...,200]`(11 ticks) になる。

```rust
#[test]
fn nice_ticks_round_numbers() {
    let t = nice_ticks(0.0, 200.0, 10);
    assert_eq!(t.step, 20.0);
    assert_eq!(t.min, 0.0);
    assert_eq!(t.max, 200.0);
    assert_eq!(
        t.ticks,
        vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0, 120.0, 140.0, 160.0, 180.0, 200.0]
    );
}

#[test]
fn nice_ticks_non_round_range() {
    let t = nice_ticks(0.0, 173.0, 10);
    // range=173, raw_step=17.3, magnitude=10, norm=1.73 → step=20
    assert_eq!(t.step, 20.0);
    assert_eq!(t.min, 0.0);
    assert_eq!(t.max, 180.0);
    assert_eq!(
        t.ticks,
        vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0, 120.0, 140.0, 160.0, 180.0]
    );
}

#[test]
fn nice_ticks_handles_negative_min() {
    let t = nice_ticks(-30.0, 70.0, 10);
    // range=100, raw_step=10, step=10
    assert_eq!(t.step, 10.0);
    assert_eq!(t.min, -30.0);
    assert_eq!(t.max, 70.0);
    assert_eq!(
        t.ticks,
        vec![-30.0, -20.0, -10.0, 0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0]
    );
}
```

> **Note:** 実際の期待値は Task 1 の `chartjs_ticks_output.json` の出力で確定すること。
> 上記は事前計算値だが、chart.js の出力と一致しない場合はそちらを優先する。

**Step 3: テストを実行して確認する**

```bash
cargo test --manifest-path crates/fulgur-chart/Cargo.toml 2>&1 | grep -E "(test result|FAILED|error)"
```

Expected: 全テスト通過。

**Step 4: Commit**

```bash
git add crates/fulgur-chart/src/layout/common.rs \
        crates/fulgur-chart/src/layout/bar.rs \
        crates/fulgur-chart/src/layout/scatter.rs \
        crates/fulgur-chart/src/layout/radar.rs \
        crates/fulgur-chart/src/scale.rs
git commit -m "fix(scale): align nice_ticks target_count with chart.js maxTicksLimit=11 (10 intervals)"
```

---

### Task 3: `AxisSpec` に `suggested_min`/`suggested_max` を追加して接続する

> chart.js の `suggestedMin`/`suggestedMax` はソフト制約。
> データが suggested 範囲を超えても問題ない（データが優先される）。
> `min`/`max` はハード制約（未実装のまま）— このタスクは suggestedMin/Max のみ。

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs:72-79` (AxisSpec に 2 フィールド追加)
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs` (3 箇所: parse + strict チェック + scatter パス)
- Modify: `crates/fulgur-chart/src/layout/common.rs` (`value_domain` を更新)

**Step 1: `ir.rs` の `AxisSpec` に 2 フィールドを追加する**

`crates/fulgur-chart/src/ir.rs:72-79` の `AxisSpec` を変更：

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct AxisSpec {
    pub title: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub suggested_min: Option<f64>,
    pub suggested_max: Option<f64>,
    pub begin_at_zero: bool,
    pub grid: bool,
}
```

**Step 2: frontend の `AxisSpec` 初期化を全箇所で更新する**

`crates/fulgur-chart/src/frontend/chartjs.rs` には `AxisSpec` を初期化する箇所が 4 か所ある（カテゴリ系 x/y、scatter/bubble x/y）。

スケールオプションは `options` フィールドが `serde_json::Value` として保持されている。
`options.scales.y.suggestedMin` を navigate する方法（stacked と同じパターン）：

```rust
// options は `Option<serde_json::Value>` のような型で保持されている
// カテゴリ系 y 軸の parse 箇所（行 405 付近）に追記する
let scales = options_val
    .as_ref()
    .and_then(|o| o.get("scales"));

let suggested_min_y = scales
    .and_then(|s| s.get("y"))
    .and_then(|a| a.get("suggestedMin"))
    .and_then(|v| v.as_f64());

let suggested_max_y = scales
    .and_then(|s| s.get("y"))
    .and_then(|a| a.get("suggestedMax"))
    .and_then(|v| v.as_f64());
```

次に `AxisSpec` 初期化（y 軸 405 行付近）：

```rust
y_axis: AxisSpec {
    title: None,
    min: None,
    max: None,
    suggested_min: suggested_min_y,
    suggested_max: suggested_max_y,
    begin_at_zero: y_begin_at_zero,
    grid: true,
},
```

x 軸の `suggested_min`/`suggested_max` は scatter 系に必要（カテゴリ系 x 軸は数値軸ではないため `None`）：

```rust
x_axis: AxisSpec {
    title: None,
    min: None,
    max: None,
    suggested_min: None,  // カテゴリ系 x 軸はソフト制約なし
    suggested_max: None,
    begin_at_zero: false,
    grid: true,
},
```

scatter パス（行 812-823 付近）は x/y 両軸に suggested を適用する。同様に `scales.x`/`scales.y` から取得する。

**Step 3: strict モードの許可キーに `suggestedMin`/`suggestedMax` を追加する**

`crates/fulgur-chart/src/frontend/chartjs.rs:609`:

```rust
&["stacked", "min", "max", "title", "grid", "beginAtZero", "suggestedMin", "suggestedMax"],
```

**Step 4: `value_domain` に suggestedMin/suggestedMax を適用する**

`crates/fulgur-chart/src/layout/common.rs` の `value_domain` 関数（98-107 行付近）を更新：

```rust
let (mut domain_min, mut domain_max) = if spec.y_axis.begin_at_zero {
    (data_min.min(0.0), data_max.max(0.0))
} else {
    (data_min, data_max)
};

// suggestedMin/suggestedMax: データが優先、suggested はドメインを広げるだけ。
if let Some(s) = spec.y_axis.suggested_min {
    if s < domain_min {
        domain_min = s;
    }
}
if let Some(s) = spec.y_axis.suggested_max {
    if s > domain_max {
        domain_max = s;
    }
}
```

**Step 5: コンパイルエラーがないか確認する**

```bash
cargo build --manifest-path crates/fulgur-chart/Cargo.toml 2>&1 | grep "^error"
```

Expected: エラーなし。

> `AxisSpec` を使う既存コードがすべて `suggested_min: None, suggested_max: None` を明示するか、
> または `..AxisSpec::default()` があれば問題ない。ない場合は各初期化箇所に 2 フィールドを追加する。

**Step 6: テストを実行する**

```bash
cargo test --manifest-path crates/fulgur-chart/Cargo.toml 2>&1 | grep -E "(test result|FAILED|error)"
```

Expected: 全テスト通過。

**Step 7: Commit**

```bash
git add crates/fulgur-chart/src/ir.rs \
        crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/src/layout/common.rs
git commit -m "feat(scale): add suggestedMin/suggestedMax to AxisSpec and wire to value_domain"
```

---

### Task 4: `scale.rs` に chart.js 出力との一致をピンするテストを追加する

> Task 1 の `chartjs_ticks_output.json` の値を使って期待値を確定する。
> このテストが regression guard になる。

**Files:**
- Modify: `crates/fulgur-chart/src/scale.rs` (テスト追加)

**Step 1: chartjs_ticks_output.json の値を読み取る**

```bash
cat tools/chartjs_ticks_output.json | python3 -c "import sys,json; d=json.load(sys.stdin); [print(x['label'], '->', x['step'], x['ticks'][:3], '...') for x in d]"
```

**Step 2: `scale.rs` に chart.js 一致テストを追加する**

以下は事前計算による雛形。**実際の期待値は Step 1 の出力で上書きすること。**

```rust
// chart.js v4 の maxTicksLimit=11(10インターバル) に一致させたピンテスト。
// 期待値は tools/chartjs_ticks.mjs の実行出力で確定。

#[test]
fn chartjs_compat_0_to_100() {
    // chart.js: [0,100] → step=10, ticks=[0,10,...,100] (11 ticks)
    let t = nice_ticks(0.0, 100.0, 10);
    assert_eq!(t.step, 10.0);
    assert_eq!(t.min, 0.0);
    assert_eq!(t.max, 100.0);
    assert_eq!(t.ticks.len(), 11);
    assert_eq!(t.ticks[0], 0.0);
    assert_eq!(t.ticks[10], 100.0);
}

#[test]
fn chartjs_compat_0_to_173() {
    // chart.js: [0,173] → step=20, ticks=[0,20,...,180] (10 ticks)
    let t = nice_ticks(0.0, 173.0, 10);
    assert_eq!(t.step, 20.0);
    assert_eq!(t.min, 0.0);
    assert_eq!(t.max, 180.0);
}

#[test]
fn chartjs_compat_neg30_to_70() {
    // chart.js: [-30,70] → step=10, ticks=[-30,-20,...,70] (11 ticks)
    let t = nice_ticks(-30.0, 70.0, 10);
    assert_eq!(t.step, 10.0);
    assert_eq!(t.min, -30.0);
    assert_eq!(t.max, 70.0);
    assert_eq!(t.ticks.len(), 11);
}

#[test]
fn chartjs_compat_0_to_1() {
    // chart.js: [0,1] → step=0.1, ticks=[0,0.1,...,1.0] (11 ticks)
    let t = nice_ticks(0.0, 1.0, 10);
    assert_eq!(t.step, 0.1);
    assert_eq!(t.min, 0.0);
    assert_eq!(t.max, 1.0);
    assert_eq!(t.ticks.len(), 11);
}
```

**Step 3: テストを実行する**

```bash
cargo test --manifest-path crates/fulgur-chart/Cargo.toml scale 2>&1 | grep -E "(test result|FAILED|ok|FAILED)"
```

Expected: 全 scale テスト通過。失敗した場合、Task 1 の出力値と比較して期待値を修正する。

**Step 4: Commit**

```bash
git add crates/fulgur-chart/src/scale.rs
git commit -m "test(scale): add chart.js v4 compatibility pin tests for nice_ticks"
```

---

### Task 5: 全テストを実行して完了確認する

**Step 1: workspace 全体でテストを実行する**

```bash
cargo test --manifest-path /home/ubuntu/fulgur-chart/.worktrees/feat/nice-ticks-chartjs/Cargo.toml 2>&1 | grep "test result"
```

Expected: すべての crate で `ok` が出ること。

**Step 2: bd issue を確認する**

```bash
bd show fulgur-chart-9oj
```

**Step 3: 完了を報告する**

全テスト通過を確認したら実装完了。
