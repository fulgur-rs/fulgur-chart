# chartjs logarithmic scale (fulgur-chart-smw) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `options.scales.{x,y}.type: "logarithmic"` を縦棒・横棒・折れ線・エリアの数値軸で受理し、Chart.js 互換の主目盛(10^n)/minor目盛(2..9×10^n)と対数座標写像で描画する。

**Architecture:** IR (`AxisSpec`) に `scale_kind: ScaleKind` を追加し、値→ピクセル写像を `LinearScale` から `ValueScale`(内部で log10 変換を吸収するラッパー)に差し替える。`Frame.ys`/横棒の `xs` を `ValueScale` に統一することで、既存の37箇所の `.map(v)` 呼び出しはコンパイラが強制する「構築箇所」だけ直せば済み、線形パスは無変更(バイト同一)を維持する。

**Tech Stack:** Rust (fulgur-chart crate)、Node.js + chart.js + node-canvas (tools/chartjs_ticks.mjs、実測値ピン留め用)。

**Beads issue:** `fulgur-chart-smw`(design/acceptance フィールドに背景・スコープ決定の経緯あり。`bd show fulgur-chart-smw` で参照)。follow-up: `fulgur-chart-rwe`(scatter/bubble、本issue範囲外)。

**Scope（ユーザー承認済み）:** 縦棒・横棒・折れ線・エリア(= `ChartKind::Bar{..}` と `ChartKind::Line`)の「値軸」のみ。カテゴリ軸への `type:"logarithmic"` 指定、および Mixed/BoxPlot/Scatter/Bubble/Radar/Pie 等の他 kind への指定は **黙って無視して Linear のまま**にする(ユーザーの承認済みスコープ外。エラーにはしない)。

---

## 前提: このリポジトリの規約

- テストは `cargo test -p fulgur-chart`。作業はworktree `/home/ubuntu/fulgur-chart/.worktrees/fulgur-chart-smw-logarithmic-scale` 内で行う。
- golden PNG回帰: `crates/fulgur-chart/tests/golden_png.rs`。再生成は `UPDATE_GOLDEN=1 cargo test -p fulgur-chart --test golden_png`。
- Chart.js実測値ピン留めの慣習: `tools/chartjs_ticks.mjs` (Node+chart.js+node-canvas) で実際のブラウザ挙動を取得し、`scale.rs` の `chartjs_compat_*` テストにハードコードする。今回も同じ手法を log scale に拡張する。
- 各タスック末尾でコミットする(頻繁なコミット)。

---

### Task 0: `tools/` の依存関係をインストール(worktree では node_modules が未インストール)

**Files:** なし(セットアップのみ)

**Step 1:** 実行:
```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fulgur-chart-smw-logarithmic-scale/tools
npm install
```
**Step 2:** 確認: `node -e "require('chart.js')"` がエラーなく終了すること。

---

### Task 1: `ScaleKind` を IR に追加(振る舞い変更なし、コンパイルが通ることのみ確認)

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs:230-244`(`AxisSpec` 定義の直前・内部)
- Modify: 以下の `AxisSpec { ... }` リテラルすべて(`cargo build` のコンパイルエラーで機械的に発見できる。現時点で判明している箇所を列挙するが、リストが漏れていてもコンパイラが教えてくれる):
  - `crates/fulgur-chart/src/frontend/chartjs.rs:962,973,1764,2148,2162,2772`
  - `crates/fulgur-chart/src/frontend/vegalite.rs:331,354`
  - `crates/fulgur-chart/src/layout/scatter.rs:568,579`
  - `crates/fulgur-chart/src/layout/wordcloud.rs:214,225`
  - `crates/fulgur-chart/src/ir.rs:698,709`
  - `crates/fulgur-chart/src/layout/common.rs:1158,1169`

**Step 1: `ir.rs` に enum を追加(`AxisSpec` の直前、230行目あたり)**

```rust
/// cartesian 軸のスケール種別。カテゴリ軸(chart種別で暗黙決定)には適用しない。
/// 数値軸(value axis)のみが Linear/Logarithmic を切り替える。
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ScaleKind {
    #[default]
    Linear,
    Logarithmic,
}
```

**Step 2: `AxisSpec` に1フィールド追加**

```rust
pub struct AxisSpec {
    pub title: Option<AxisTitle>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub suggested_min: Option<f64>,
    pub suggested_max: Option<f64>,
    pub begin_at_zero: bool,
    pub offset: bool,
    pub grid: AxisGrid,
    pub border: AxisBorder,
    /// 数値軸の目盛スケール種別。カテゴリ軸(XPositions::Category が支配する軸)では
    /// 意味を持たないが、AxisSpec は x/y 共通型のため常に存在する。
    pub scale_kind: ScaleKind,
}
```

**Step 3: `cargo build -p fulgur-chart` を実行し、出力されたコンパイルエラー(missing field `scale_kind`)の箇所すべてに `scale_kind: ScaleKind::Linear,` を追加する。**

エラーが出なくなるまで繰り返す。

**Step 4: 確認**
```bash
cargo build -p fulgur-chart 2>&1 | tail -20
cargo test -p fulgur-chart 2>&1 | tail -15
```
Expected: ビルド成功・既存テスト全通過(振る舞いは一切変えていないので regression ゼロのはず)。

**Step 5: Commit**
```bash
git add crates/fulgur-chart/src
git commit -m "feat(ir): add ScaleKind::{Linear,Logarithmic} to AxisSpec (no behavior change)"
```

---

### Task 2: `ValueScale` を導入(線形パスをラップするだけ、ピクセル出力はバイト同一)

これは advisor 推奨の「安全なリファクタ第一歩」: 全て `Linear` のまま `ValueScale` に包み、golden PNG と wasm_runtime テストが無変化であることを確認してからログ機能に進む。

**Files:**
- Modify: `crates/fulgur-chart/src/scale.rs`(末尾、`bounded_ticks` の後、テストモジュールの前)
- Modify: `crates/fulgur-chart/src/layout/common.rs:200-210`(`Frame` 構造体)、`:600`(`ys` 構築)
- Modify: `crates/fulgur-chart/src/layout/bar.rs:326`(`xs` 構築、`build_horizontal` 内)
- Modify: `crates/fulgur-chart/src/model.rs` は変更不要(このタスクでは)

**Step 1: `scale.rs` に `ValueScale` を追加**

```rust
/// 値→ピクセル写像。線形はそのまま `LinearScale` に委譲し、対数は内部で
/// `log10` 変換してから同じ `LinearScale` に委譲する。呼び出し側は
/// `ValueScale::map(v)` だけを見ればよく、線形/対数の分岐を意識しない。
#[derive(Debug, Clone)]
pub enum ValueScale {
    Linear(LinearScale),
    Log {
        /// ログ空間(log10(d0)..log10(d1))を写す内部スケール。
        inner: LinearScale,
        /// この値以下は floor にクランプしてから log10 する(0 や丸め誤差での
        /// 負値が -inf/NaN を作らないための総関数化)。呼び出し側で「floor 未満は
        /// 描画しない」判断が必要な場合(負値スキップ)は、ここに来る前に
        /// 呼び出し側がフィルタ済みである前提(このタスクでは Linear 経路のみ使うため
        /// floor 分岐は Task 8 まで到達しない)。
        floor: f64,
    },
}

impl ValueScale {
    pub fn map(&self, v: f64) -> f64 {
        match self {
            ValueScale::Linear(s) => s.map(v),
            ValueScale::Log { inner, floor } => inner.map(v.max(*floor).log10()),
        }
    }
}
```

**Step 2: `layout/common.rs` の `Frame` を更新**

```rust
pub struct Frame {
    pub scene_width: f64,
    pub scene_height: f64,
    pub plot_left: f64,
    pub plot_right: f64,
    pub plot_top: f64,
    pub plot_bottom: f64,
    pub ticks: NiceTicks,
    pub ys: ValueScale,
    pub temporal_ticks: Vec<TemporalTick>,
}
```

`compute()` 内 (`common.rs:600`):
```rust
let ys = ValueScale::Linear(LinearScale::new(ticks.min, ticks.max, plot_bottom, plot_top));
```

**Step 3: `layout/bar.rs` の `build_horizontal`(326行目)**

```rust
let xs = ValueScale::Linear(LinearScale::new(ticks.min, ticks.max, plot_left, plot_right));
```
`use crate::scale::{LinearScale, nice_ticks};` の import に `ValueScale` を追加。

**Step 4: `cargo build -p fulgur-chart` を実行し、型不一致エラーが出た箇所を確認する。**

`.ys.map(v)` / `xs.map(v)` の呼び出し自体はシグネチャ互換(`fn map(&self, v: f64) -> f64`)なのでコンパイルは通るはず。エラーが出るのはテストコードで `LinearScale::new(...)` を直接 `Frame.ys`/`xs` に代入している箇所や、`ValueScale` を `PartialEq`/`Clone` で比較しているテストがあれば追加 derive が必要になる場合のみ。

**Step 5: 確認**
```bash
cargo test -p fulgur-chart 2>&1 | tail -20
```
Expected: 全テスト通過。

**Step 6: golden/wasm の無変化を明示的に確認(このタスクの核心)**
```bash
cargo test -p fulgur-chart --test golden_png 2>&1 | tail -10
cargo test -p fulgur-chart --test wasm_runtime 2>&1 | tail -10
```
Expected: 両方とも既存 golden と完全一致(diff なし)で PASS。**ここで diff が出たら Task 2 の実装に誤りがある(線形パスの算術が変わってしまっている)ので、ログ機能に進む前に必ず原因を特定して直す。**

**Step 7: Commit**
```bash
git add crates/fulgur-chart/src
git commit -m "refactor(scale): wrap LinearScale in ValueScale (no-op for linear path)"
```

---

### Task 3: `AxisOptions` に `type` フィールドを追加(スキーマ層)

**重要な設計上の注意:** `AxisOptions` の既存コメント(`schema/common.rs:196-204`)が明言している通り、`type` は Chart.js では `"category"`/`"time"`/`"linear"` など多数の値を取りうる。**厳格な enum(`#[serde(rename_all="lowercase")] enum ScaleType { Linear, Logarithmic }`)にすると、既存の Chart.js JSON が明示的に `"type":"category"` を書いているだけで(非常によくある)、strict/非strict 問わず deserialize エラーになり既存互換性を壊す。** 必ず `Option<String>` として受理し、「"logarithmic" 以外は全部無視」を frontend 側(Task 4)でハンドリングすること。

**Files:**
- Modify: `crates/fulgur-chart/src/schema/common.rs:207-232`(`AxisOptions`)

**Step 1: フィールド追加**

```rust
pub struct AxisOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stacked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// Chart.js scale type. `"logarithmic"` のみが振る舞いを変える。他の値
    /// (`"category"`/`"time"`/`"linear"` やタイポ)は frontend 側で黙って
    /// 既定(Linear)として扱う。厳格な enum にしない理由: struct 冒頭のコメント参照。
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    pub title: Option<AxisTitleOptions>,
    // ... (既存フィールドはそのまま)
}
```

**Step 2: 確認**
```bash
cargo build -p fulgur-chart
```

**Step 3: Commit**
```bash
git add crates/fulgur-chart/src/schema/common.rs
git commit -m "feat(schema): accept options.scales.{x,y}.type as an opaque string"
```

---

### Task 4: strict モード許可リストに `type` を追加 + IR への橋渡し + 負値マスキング

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs:1289-1300`(strict allow-list)
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs` `parse()` 内(`x_opts`/`y_opts` の hoist、`series` 構築、`AxisSpec` 構築)

**Step 1: allow-list に `"type"` を追加(1289-1300行目)**

```rust
let allowed_axis_keys: &[&str] = if allow_radial_scale {
    &["min", "max", "suggestedMin", "suggestedMax", "beginAtZero"]
} else {
    &[
        "stacked",
        "min",
        "max",
        "type",
        "title",
        "grid",
        "border",
        "beginAtZero",
        "suggestedMin",
        "suggestedMax",
        "offset",
    ]
};
```

**Step 2: `x_opts`/`y_opts` を `series` 構築より前に hoist する**

現在 `let x_opts = ...; let y_opts = ...;` は896-897行目付近(`series` 構築(775行目)より後)にある。この2行を `series` 構築より前(878行目付近、`is_horizontal`/`is_line` 計算の直後)に移動する。897行目以降で同じ変数名を再度 `let` しないよう、元の位置の宣言は削除する(以降の行はそのまま同じ変数を参照できる)。

**Step 3: スケール種別判定と負値マスキング用のヘルパーを追加**

`x_opts`/`y_opts` hoist の直後、`series` 構築の前に:

```rust
// v1 スコープ: 縦棒・横棒・折れ線の「値軸」のみ log を許可する。
// カテゴリ軸や他 kind への type:"logarithmic" 指定は黙って無視(Linear のまま)。
fn is_logarithmic(opts: Option<&AxisOptions>) -> bool {
    opts.and_then(|a| a.r#type.as_deref()) == Some("logarithmic")
}
let x_axis_is_log =
    matches!(kind, ChartKind::Bar { horizontal: true, .. }) && is_logarithmic(x_opts);
let y_axis_is_log = matches!(kind, ChartKind::Bar { horizontal: false, .. } | ChartKind::Line)
    && is_logarithmic(y_opts);
let value_axis_is_log = x_axis_is_log || y_axis_is_log;
```

(`fn is_logarithmic` はモジュールの他の `fn axis_title_from` 等と並べてトップレベルに置いてもよい。上記はインライン例。)

**Step 4: 負値を NaN 化(対数軸のときのみ)**

`series` 構築ブロック内、`(ds.data.into_values(), vec![], vec![])` の箇所(787行目付近)を:

```rust
} else {
    let mut values = ds.data.into_values();
    if value_axis_is_log {
        // 対数軸では負値は描画不能(chart.js 互換)。既存の null→NaN センチネル経路に
        // 乗せることで、bar.rs/line.rs の既存の `!v.is_finite() { continue }` が
        // そのままギャップとして扱ってくれる(新しいスキップ機構を作らない)。
        // 0 はここでは変えない(0 → decade 下限への置換は value_domain 側の責務)。
        for v in &mut values {
            if v.is_finite() && *v < 0.0 {
                *v = f64::NAN;
            }
        }
    }
    (values, vec![], vec![])
}
```

**Step 5: `AxisSpec` 構築(962-983行目)に `scale_kind` を配線**

```rust
x_axis: AxisSpec {
    // ...(既存フィールドはそのまま)
    scale_kind: if x_axis_is_log {
        ScaleKind::Logarithmic
    } else {
        ScaleKind::Linear
    },
},
y_axis: AxisSpec {
    // ...(既存フィールドはそのまま)
    scale_kind: if y_axis_is_log {
        ScaleKind::Logarithmic
    } else {
        ScaleKind::Linear
    },
},
```

(Task 1 で仮置きした `scale_kind: ScaleKind::Linear,` をこの2箇所だけ本物の判定に差し替える。他の8箇所は `ScaleKind::Linear` のままでよい。)

**Step 6: 確認**
```bash
cargo build -p fulgur-chart
cargo test -p fulgur-chart 2>&1 | tail -20
```
Expected: 既存テスト全通過(まだ `layout` 側が `scale_kind` を見ていないので、`type:"logarithmic"` を指定してもレイアウトは従来通り Linear として描画されるはずだが、それは Task 7-9 まで想定内)。

**Step 7: Commit**
```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs
git commit -m "feat(frontend): parse scales.{x,y}.type=logarithmic into ScaleKind, mask negatives"
```

---

### Task 5: `scale.rs` に `log_ticks` を実装(構造のみ、chart.js ピン留めは Task 6-7)

**Files:**
- Modify: `crates/fulgur-chart/src/scale.rs`

**Step 1: 失敗するテストを書く(`scale.rs` のテストモジュール末尾に追加)**

```rust
#[test]
fn log_ticks_single_decade() {
    let t = log_ticks(3.0, 7.0);
    assert_eq!(t.min, 1.0);
    assert_eq!(t.max, 10.0);
    assert_eq!(t.major, vec![1.0, 10.0]);
    assert_eq!(t.minor, vec![2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]);
}

#[test]
fn log_ticks_multi_decade() {
    let t = log_ticks(30.0, 4000.0);
    assert_eq!(t.min, 10.0);
    assert_eq!(t.max, 10000.0);
    assert_eq!(t.major, vec![10.0, 100.0, 1000.0, 10000.0]);
    assert!(t.minor.contains(&20.0));
    assert!(t.minor.contains(&9000.0));
}

#[test]
fn log_ticks_rejects_non_positive_domain_does_not_panic() {
    // 呼び出し側(value_domain)は常に正のドメインを渡す契約だが、境界値で
    // panic しないことだけ保証する(NaN/inf を作らない)。
    let t = log_ticks(f64::MIN_POSITIVE, f64::MIN_POSITIVE * 10.0);
    assert!(t.min.is_finite() && t.min > 0.0);
    assert!(t.max.is_finite() && t.max > t.min);
}
```

**Step 2: テストが失敗することを確認**
```bash
cargo test -p fulgur-chart log_ticks 2>&1 | tail -20
```
Expected: `log_ticks` が存在せずコンパイルエラー。

**Step 3: 実装(`nice_ticks` の直後に追加)**

```rust
/// 対数スケールの目盛りセット。`major` は 10^n(ラベル表示対象)、`minor` は
/// 各 decade の mantissa 2..9 倍(ラベルなしグリッド用)。両方とも値空間(データ空間)の
/// 実値であり、log10 変換は `ValueScale::Log` が写像時に行う。
#[derive(Clone, Debug, PartialEq)]
pub struct LogTicks {
    pub min: f64,
    pub max: f64,
    pub major: Vec<f64>,
    pub minor: Vec<f64>,
}

/// nice_ticks の MAX_TICK_INTERVALS と同じ趣旨: 極端なドメイン(例 1..1e300)で
/// decade 数が爆発しないよう上限を設ける。
const MAX_LOG_DECADES: i32 = 308;

/// `data_min`..`data_max`(共に正の有限値)を 10^n の decade 境界に丸め、
/// 主目盛(10^n)と minor目盛(mantissa 2..9)を生成する。
/// 呼び出し側契約: `data_min > 0.0 && data_max >= data_min && 両方有限`。
pub fn log_ticks(data_min: f64, data_max: f64) -> LogTicks {
    let data_min = if data_min.is_finite() && data_min > 0.0 {
        data_min
    } else {
        f64::MIN_POSITIVE
    };
    let data_max = if data_max.is_finite() && data_max >= data_min {
        data_max
    } else {
        data_min * 10.0
    };

    let lo_exp = data_min.log10().floor() as i32;
    let hi_exp_raw = data_max.log10().ceil() as i32;
    let hi_exp = hi_exp_raw
        .max(lo_exp + 1)
        .min(lo_exp.saturating_add(MAX_LOG_DECADES));

    let mut major = Vec::new();
    let mut minor = Vec::new();
    for exp in lo_exp..=hi_exp {
        let decade = 10f64.powi(exp);
        major.push(decade);
        if exp < hi_exp {
            for mantissa in 2..=9 {
                minor.push(mantissa as f64 * decade);
            }
        }
    }

    LogTicks {
        min: 10f64.powi(lo_exp),
        max: 10f64.powi(hi_exp),
        major,
        minor,
    }
}
```

**Step 4: テストを通す**
```bash
cargo test -p fulgur-chart log_ticks 2>&1 | tail -20
```
Expected: PASS。

**Step 5: Commit**
```bash
git add crates/fulgur-chart/src/scale.rs
git commit -m "feat(scale): add log_ticks (major/minor decade tick generation)"
```

---

### Task 6: `tools/chartjs_ticks.mjs` を対数軸対応に拡張し、実測値を確定する

**目的:** 「どの目盛にラベルを付けるか」(design で保留にした論点)を記憶ではなく実測で確定する。Chart.js の `chart.scales.y.ticks` は各 tick に `major: boolean` を持つほか、実際の描画ラベルは `chart.scales.y._resolvedTickFontOptions` 等の内部APIではなく `chart.scales.y.options.ticks.callback`(既定の `Ticks.formatters.logarithmic` 相当)経由で決まる。既定コールバックを呼び出すのが最も確実。

**Step 1: `tools/chartjs_ticks.mjs` に log 用のケースと出力を追加**

既存の `getTicks` 関数はそのまま流用しつつ、log 専用の関数を追加する(`type: 'bar'` の `y` スケールに `type: 'logarithmic'` を渡すだけで動く):

```js
async function getLogTicks(label, data, yOpts = {}) {
  const canvas = createCanvas(800, 400);
  const ctx = canvas.getContext('2d');

  const chart = new Chart(ctx, {
    type: 'bar',
    data: { labels: data.map((_, i) => `x${i}`), datasets: [{ data }] },
    options: {
      animation: false,
      scales: { y: { type: 'logarithmic', ...yOpts } },
    },
  });

  const scale = chart.scales.y;
  const result = {
    label,
    data,
    yOpts,
    min: scale.min,
    max: scale.max,
    ticks: scale.ticks.map((t, i) => ({
      value: t.value,
      major: !!t.major,
      // 既定の tick フォーマッタを直接呼び、実際に描画されるラベル文字列を取る。
      label: scale.getLabelForValue(t.value),
    })),
  };
  chart.destroy();
  return result;
}

const logCases = [
  ['single decade [3,7]', [3, 7], {}],
  ['multi decade [30,4000]', [30, 4000], {}],
  ['sub-one [0.003,0.7]', [0.003, 0.7], {}],
  ['wide [1,1000000]', [1, 1_000_000], {}],
  ['exact powers [1,1000]', [1, 1000], {}],
];

const logResults = [];
for (const [label, data, opts] of logCases) {
  logResults.push(await getLogTicks(label, data, opts));
}
console.error('=== LOG TICKS ===');
console.error(JSON.stringify(logResults, null, 2));
```

(`console.error` にしているのは既存の `console.log(JSON.stringify(results...))`(線形ケース)の出力と混ざらないようにするため。)

**Step 2: 実行して実測値を取得**
```bash
cd /home/ubuntu/fulgur-chart/.worktrees/fulgur-chart-smw-logarithmic-scale/tools
node chartjs_ticks.mjs > /tmp/chartjs_ticks_linear.json 2> /tmp/chartjs_ticks_log.json
cat /tmp/chartjs_ticks_log.json
```

**Step 3: 出力を確認し、以下を判定する:**
1. `major: true` になっているのは mantissa=1(10^n ちょうど)の tick だけか?
2. `label` が空文字列/非表示になっている tick はあるか? あるなら、どの mantissa(1/2/5 など)のときか?
3. 一番端(min/max)の tick は major でなくてもラベルが付くか?

> **【実測により判明した重要な訂正】** 上記 Step 1 のコメント「既定の tick フォーマッタを直接呼び、実際に描画されるラベル文字列を取る」は誤りだった。`scale.getLabelForValue(value)` は数値フォーマット(桁区切り・小数桁)のみを行い、ラベルの可視性(空文字列になるかどうか)は一切反映しない。実際に描画されるラベルは `tick.label`(`generateTickLabels()` が `options.ticks.callback`、既定は `Ticks.formatters.logarithmic`、を通して設定する値)であり、両者は全く別物だった。実装した `tools/chartjs_ticks.mjs` は `label: t.label` と `getLabelForValue: scale.getLabelForValue(t.value)` を両方出力するよう修正済み。詳細と根拠は文末の「Task 6 実測結果」節を参照。

**Step 4: この実測結果を `docs/plans/2026-08-08-fulgur-chart-smw-logarithmic-scale.md` の本セクションの下に追記する(次タスクの入力になるため記録を残す)。**

**Step 5: Commit**
```bash
git add tools/chartjs_ticks.mjs docs/plans/2026-08-08-fulgur-chart-smw-logarithmic-scale.md
git commit -m "test(tools): extend chartjs_ticks.mjs with logarithmic scale reference cases"
```

---

### Task 7: Task 6 の実測値で `log_ticks` のラベル可視性ルールを確定・ピン留め

**Files:**
- Modify: `crates/fulgur-chart/src/scale.rs`

**Step 1:** Task 6 の実測結果を見て、以下のいずれかを選ぶ(実測前に断定しない):

- **(A) major(10^n)のみラベル表示** だった場合: `LogTicks.major`(ラベルあり)/`minor`(ラベルなし)の現状の2分割で十分。Task 5 の実装のまま。
- **(B) mantissa 1/2/5 もラベル表示** だった場合: `LogTicks` に3値目のラベル可視性が必要。`minor: Vec<f64>` を `minor_labeled: Vec<f64>` と `minor_unlabeled: Vec<f64>` に分けるか、`pub struct LogTick { value: f64, major: bool, labeled: bool }` の `Vec<LogTick>` に構造を変える。**この場合は Task 5 のコードと本ドキュメントの `LogTicks` 定義を実測に合わせて改訂すること(TDD: 実測 → 失敗するテスト → 実装修正)。**

**Step 2: 実測値をハードコードした `chartjs_compat_log_*` テストを追加(`nice_ticks` の `chartjs_compat_*` と同じパターン)**

```rust
// chart.js v4 logarithmic scale の実出力に対するピンテスト。
// 期待値は tools/chartjs_ticks.mjs (log ケース)の実行結果で確定。
// 再生成: cd tools && node chartjs_ticks.mjs 2> /tmp/log.json

#[test]
fn chartjs_compat_log_single_decade() {
    // chart.js: data=[3,7] → (Task 6 の実測値で埋める)
    let t = log_ticks(3.0, 7.0);
    // assert_eq!(t.min, ...); assert_eq!(t.max, ...); ...
}
```
(具体的な assert は Task 6 の実測結果が出るまでプレースホルダ。実測後に埋める。)

**Step 3: テスト実行**
```bash
cargo test -p fulgur-chart chartjs_compat_log 2>&1 | tail -30
```
Expected: PASS(実装が実測と食い違えば `log_ticks` を修正して再度合わせる)。

**Step 4: Commit**
```bash
git add crates/fulgur-chart/src/scale.rs
git commit -m "test(scale): pin log_ticks label-visibility rule to chart.js observed output"
```

---

### Task 8: `layout/common.rs::value_domain` に対数ドメイン計算を追加

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs:239-345`(`value_domain`)

**Step 1: 失敗するテストを書く(`layout/common.rs` のテストモジュールに追加、`make_bar_spec` ヘルパーの近く)**

```rust
#[test]
fn log_value_domain_uses_min_positive_and_max() {
    let mut spec = make_bar_spec(1, 600.0);
    spec.y_axis.scale_kind = ScaleKind::Logarithmic;
    spec.series[0].values = vec![5.0, 50.0, 500.0];
    let (min, max) = value_domain(&spec, &spec.y_axis);
    assert_eq!(min, 5.0);
    assert_eq!(max, 500.0);
}

#[test]
fn log_value_domain_substitutes_zero_with_decade_below_min_positive() {
    let mut spec = make_bar_spec(1, 600.0);
    spec.y_axis.scale_kind = ScaleKind::Logarithmic;
    spec.series[0].values = vec![0.0, 30.0];
    let (min, max) = value_domain(&spec, &spec.y_axis);
    assert_eq!(min, 3.0); // 30 の1桁下
    assert_eq!(max, 30.0);
}

#[test]
fn log_value_domain_ignores_begin_at_zero() {
    let mut spec = make_bar_spec(1, 600.0);
    spec.y_axis.scale_kind = ScaleKind::Logarithmic;
    spec.y_axis.begin_at_zero = true;
    spec.series[0].values = vec![40.0, 80.0];
    let (min, _max) = value_domain(&spec, &spec.y_axis);
    assert_eq!(min, 40.0); // begin_at_zero=true でも 0 を含めない
}

#[test]
fn log_value_domain_ignores_non_positive_suggested_bounds() {
    let mut spec = make_bar_spec(1, 600.0);
    spec.y_axis.scale_kind = ScaleKind::Logarithmic;
    spec.y_axis.suggested_min = Some(-10.0);
    spec.y_axis.suggested_max = Some(0.0);
    spec.series[0].values = vec![10.0, 20.0];
    let (min, max) = value_domain(&spec, &spec.y_axis);
    assert_eq!(min, 10.0);
    assert_eq!(max, 20.0);
}

#[test]
fn log_value_domain_falls_back_when_all_non_positive() {
    let mut spec = make_bar_spec(1, 600.0);
    spec.y_axis.scale_kind = ScaleKind::Logarithmic;
    spec.series[0].values = vec![0.0, f64::NAN]; // NaN は既にネガティブマスク済み想定
    let (min, max) = value_domain(&spec, &spec.y_axis);
    assert_eq!((min, max), (1.0, 10.0));
}
```

**Step 2: 失敗を確認**
```bash
cargo test -p fulgur-chart log_value_domain 2>&1 | tail -20
```

**Step 3: 実装。`value_domain` の先頭に早期分岐を追加し、対数専用ロジックを別関数に切り出す(既存の線形ロジックには一切触れない)。**

```rust
pub fn value_domain(spec: &ChartSpec, axis: &AxisSpec) -> (f64, f64) {
    if axis.scale_kind == ScaleKind::Logarithmic {
        return log_value_domain(spec, axis);
    }
    // ... 既存の線形実装はそのまま ...
}

/// 対数軸専用のドメイン計算。線形版と異なり: begin_at_zero は無視、0 は
/// 最小正値の1桁下に置換してドメインへ含め、負値(既に NaN 化済みのはず)は
/// 通常の有限値フィルタで自然に除外される。suggested_min/max は正の値のみ尊重。
fn log_value_domain(spec: &ChartSpec, axis: &AxisSpec) -> (f64, f64) {
    let mut min_positive = f64::INFINITY;
    let mut max_positive = f64::NEG_INFINITY;
    let mut has_zero = false;
    for s in &spec.series {
        for &v in &s.values {
            if !v.is_finite() {
                continue;
            }
            if v == 0.0 {
                has_zero = true;
                continue;
            }
            if v > 0.0 {
                if v < min_positive {
                    min_positive = v;
                }
                if v > max_positive {
                    max_positive = v;
                }
            }
            // v < 0.0 はここに来ないはず(parse 時に NaN 化済み)が、念のため無視する。
        }
    }

    if !min_positive.is_finite() || !max_positive.is_finite() {
        // 正データが1つもない(空 / 0 のみ / 負のみ)。既定の 1..10 にフォールバック。
        return (1.0, 10.0);
    }

    let mut domain_min = if has_zero {
        min_positive / 10.0
    } else {
        min_positive
    };
    let mut domain_max = max_positive;

    if let Some(s) = axis.suggested_min
        && s.is_finite()
        && s > 0.0
        && s < domain_min
    {
        domain_min = s;
    }
    if let Some(s) = axis.suggested_max
        && s.is_finite()
        && s > 0.0
        && s > domain_max
    {
        domain_max = s;
    }
    if domain_max <= domain_min {
        domain_max = domain_min * 10.0;
    }
    (domain_min, domain_max)
}
```

`AxisSpec`/`ScaleKind` を `layout/common.rs` の import に追加する必要があれば追加する(`crate::ir::ScaleKind` 等)。

**Step 4: テストを通す**
```bash
cargo test -p fulgur-chart log_value_domain 2>&1 | tail -20
cargo test -p fulgur-chart 2>&1 | tail -10
```
Expected: 新規テスト PASS、既存テストも regression なし(線形パスは早期リターンの外なので無変更)。

**Step 5: Commit**
```bash
git add crates/fulgur-chart/src/layout/common.rs
git commit -m "feat(layout): compute logarithmic axis domain (zero substitution, negative exclusion)"
```

---

### Task 9: `layout/common.rs::compute()` を対数分岐に対応させる(縦軸)

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs:199-210`(`Frame` に `minor_ticks` 追加)
- Modify: `crates/fulgur-chart/src/layout/common.rs:363-373`(`compute()` の tick/scale 構築)
- Modify: `crates/fulgur-chart/src/layout/common.rs:733-758`(グリッド + ラベル描画ループ)

**Step 1: `Frame` に `minor_ticks` を追加**

```rust
pub struct Frame {
    // ... 既存フィールド ...
    pub ticks: NiceTicks,
    pub ys: ValueScale,
    /// 対数軸のラベルなし minor 目盛(mantissa 2..9)。線形軸では常に空。
    pub minor_ticks: Vec<f64>,
    pub temporal_ticks: Vec<TemporalTick>,
}
```

**Step 2: `compute()` 内、`ticks`/`ys` 構築を分岐(363-373行目 と 600行目)**

```rust
let (domain_min, domain_max) = value_domain(spec, &spec.y_axis);
let (ticks, minor_ticks) = if spec.y_axis.scale_kind == ScaleKind::Logarithmic {
    let log = crate::scale::log_ticks(domain_min, domain_max);
    (
        NiceTicks {
            min: log.min,
            max: log.max,
            // 対数軸では decade 間隔が一定でない(1,10,100,...)ため "step" は
            // 意味を持たない。0.0 は「非対数の step とは値域が異なる」ことを示す
            // 番兵(nice_ticks は常に step>0 を返すため 0.0 は log 専用の合図になる)。
            step: 0.0,
            ticks: log.major,
        },
        log.minor,
    )
} else if matches!(spec.size_mode, SizeMode::PlotArea)
    && matches!(spec.kind, ChartKind::Line)
    && matches!(spec.x_positions, XPositions::Temporal { .. })
{
    (vega_nice_ticks(domain_min, domain_max, spec.height), Vec::new())
} else {
    (nice_ticks(domain_min, domain_max, 10), Vec::new())
};
```

(既存の `let ticks = if ... vega_nice_ticks ... else nice_ticks(...)` の3行を上記に置き換え、以後の `ticks` 参照はそのまま使う。)

600行目付近の `ys` 構築:
```rust
let ys = if spec.y_axis.scale_kind == ScaleKind::Logarithmic {
    ValueScale::Log {
        inner: LinearScale::new(ticks.min.log10(), ticks.max.log10(), plot_bottom, plot_top),
        floor: ticks.min,
    }
} else {
    ValueScale::Linear(LinearScale::new(ticks.min, ticks.max, plot_bottom, plot_top))
};
```

`Frame { ... }` の返り値に `minor_ticks,` を追加する。

**Step 3: ラベルフォーマッタの分岐と minor グリッド描画(733-758行目)**

まず label フォーマッタ(Task 10 で実装する `fmt_num_log` を先取り利用。Task 10 を先に実装してから本タスクに進んでも良い ── 依存関係としては Task 10 → Task 9 の順でも成立する。ここでは Task 9 のコードに `fmt_num_log` 呼び出しを書き、Task 10 で関数自体を追加する形で進める):

```rust
// 2. 横グリッド + y 軸ラベル(主目盛)。
let grid_cfg = &spec.y_axis.grid;
let grid_color = grid_cfg.color.unwrap_or(spec.theme.grid_color);
let is_log = spec.y_axis.scale_kind == ScaleKind::Logarithmic;
for &t in &frame.ticks.ticks {
    let y = frame.ys.map(t);
    if grid_cfg.display {
        items.push(Prim::Line {
            x1: frame.plot_left,
            y1: y,
            x2: frame.plot_right,
            y2: y,
            stroke: grid_color,
            stroke_width: grid_cfg.line_width,
            dash: Vec::new(),
        });
    }
    items.push(Prim::Text {
        x: frame.plot_left - 6.0,
        y: y + label_font * TEXT_BASELINE_RATIO,
        size: label_font,
        anchor: Anchor::End,
        fill: ink,
        content: if is_log { crate::num::fmt_num_log(t) } else { fmt_num(t) },
        rotate_deg: None,
    });
}
// 2b. 対数軸の minor グリッド(ラベルなし)。線形軸では frame.minor_ticks が空なので no-op。
if grid_cfg.display {
    for &t in &frame.minor_ticks {
        let y = frame.ys.map(t);
        items.push(Prim::Line {
            x1: frame.plot_left,
            y1: y,
            x2: frame.plot_right,
            y2: y,
            stroke: grid_color,
            stroke_width: grid_cfg.line_width,
            dash: Vec::new(),
        });
    }
}
```

**Step 4: y軸ラベル幅計算(`compute()` 内、377-383行目)も対数フォーマッタを使うよう更新**

```rust
let mut max_w = 0.0_f32;
for &t in &ticks.ticks {
    let s = if spec.y_axis.scale_kind == ScaleKind::Logarithmic {
        crate::num::fmt_num_log(t)
    } else {
        fmt_num(t)
    };
    let w = m.width(&s, spec.theme.font_size as f32);
    if w > max_w {
        max_w = w;
    }
}
```

**Step 5: ビルドエラーを解消(`fmt_num_log` は未定義なので一旦 `fmt_num` のスタブとして定義するか、Task 10 を先に完了させる)。Task 10 を先に終わらせてからこのタスクの Step 3-4 を書くことを推奨。**

**Step 6: 確認**
```bash
cargo build -p fulgur-chart 2>&1 | tail -30
cargo test -p fulgur-chart 2>&1 | tail -15
cargo test -p fulgur-chart --test golden_png 2>&1 | tail -10
```
Expected: 全通過、golden PNG は無変化(まだ log spec の golden を追加していないので既存 golden は影響を受けないはず)。

**Step 7: Commit**
```bash
git add crates/fulgur-chart/src/layout/common.rs
git commit -m "feat(layout): render logarithmic y-axis (major/minor grid, log-aware labels)"
```

---

### Task 10: `num.rs` に対数ラベル用フォーマッタを追加

**Files:**
- Modify: `crates/fulgur-chart/src/num.rs`

**注意:** `fmt_num` は202箇所で使われる「全SVG座標の最終出口」であり、絶対に変更しない(座標のbyte一致が壊れる)。新関数を追加するだけ。

**Step 1: 失敗するテストを書く**

```rust
#[test]
fn fmt_num_log_preserves_small_magnitudes() {
    assert_eq!(fmt_num_log(0.001), "0.001");
    assert_eq!(fmt_num_log(1.0), "1");
    assert_eq!(fmt_num_log(1000.0), "1000");
    assert_eq!(fmt_num_log(0.0001), "0.0001");
}

#[test]
fn fmt_num_log_non_finite_falls_back_to_zero() {
    assert_eq!(fmt_num_log(f64::NAN), "0");
    assert_eq!(fmt_num_log(f64::INFINITY), "0");
}
```

**Step 2: 失敗を確認**
```bash
cargo test -p fulgur-chart fmt_num_log 2>&1 | tail -15
```

**Step 3: 実装(`fmt_num` の後に追加)**

```rust
/// 対数軸の目盛ラベル用。`fmt_num` と違い小数点以下を2桁に丸めない
/// (log軸は 0.0001 のような広いレンジの値を扱うため)。
/// 有効数字ベースで十分な桁を残しつつ、末尾の不要な 0 を除去する。
/// ticks.format(fulgur-chart-pof、別issue)が実装されたら、明示指定時はそちらを優先し、
/// 未指定時のデフォルトとしてこの関数を使い続ける想定。
pub fn fmt_num_log(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_string();
    }
    if v == 0.0 {
        return "0".to_string();
    }
    // 対数軸の目盛は log_ticks が生成する 10^n × {1..9} のみなので、
    // 15桁程度の精度で十分表現でき、末尾ゼロ除去で "0.001" のような形になる。
    let mut s = format!("{v:.15}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}
```

**Step 4: テストを通す**
```bash
cargo test -p fulgur-chart fmt_num_log 2>&1 | tail -15
```
Expected: PASS。(`{v:.15}` で丸め誤差が出ないか要確認。例えば `0.1_f64` を `.15` フォーマットすると `0.100000000000000` のような表示になり、末尾ゼロ除去で "0.1" になるはずだが、`log_ticks` が生成する値は常に `mantissa as f64 * 10f64.powi(exp)` の形なので、丸め誤差で末尾に `...0001` のようなゴミが出ないか実際にテストで確認すること。出る場合は有効数字ベースの丸め(例: `format!("{:e}")` を解析して指数部と仮数部を分離)に切り替える。)

**Step 5: Commit**
```bash
git add crates/fulgur-chart/src/num.rs
git commit -m "feat(num): add fmt_num_log for wide-magnitude logarithmic tick labels"
```

---

### Task 11: 横棒(`build_horizontal`)を対数分岐に対応させる

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs:246-330`(`build_horizontal` 冒頭)
- Modify: `crates/fulgur-chart/src/layout/bar.rs:343-360`(縦グリッド+値ラベル)
- Modify: `crates/fulgur-chart/src/layout/bar.rs:400-430`(tick マーク・baseline 付近、必要なら)

**Step 1: `build_horizontal` の domain/tick/scale 構築(258-259行目、326行目)を Task 9 と同じパターンで分岐**

```rust
let (dmin, dmax) = value_domain(spec, &spec.x_axis);
let is_log = spec.x_axis.scale_kind == ScaleKind::Logarithmic;
let (ticks, minor_ticks) = if is_log {
    let log = crate::scale::log_ticks(dmin, dmax);
    (
        NiceTicks { min: log.min, max: log.max, step: 0.0, ticks: log.major },
        log.minor,
    )
} else {
    (nice_ticks(dmin, dmax, 10), Vec::new())
};
```

326行目:
```rust
let xs = if is_log {
    ValueScale::Log {
        inner: LinearScale::new(ticks.min.log10(), ticks.max.log10(), plot_left, plot_right),
        floor: ticks.min,
    }
} else {
    ValueScale::Linear(LinearScale::new(ticks.min, ticks.max, plot_left, plot_right))
};
```

**Step 2: 値ラベル(347行目付近、`fmt_num(t)` 呼び出し箇所)を `is_log` 分岐に**

```rust
for &t in &ticks.ticks {
    let x = xs.map(t);
    if x_grid_cfg.display {
        items.push(Prim::Line { /* ... 既存 ... */ });
    }
    items.push(Prim::Text {
        // ...
        content: if is_log { crate::num::fmt_num_log(t) } else { fmt_num(t) },
        // ...
    });
}
if x_grid_cfg.display {
    for &t in &minor_ticks {
        let x = xs.map(t);
        items.push(Prim::Line {
            x1: x, y1: plot_top, x2: x, y2: plot_bottom,
            stroke: x_grid_color, stroke_width: x_grid_cfg.line_width, dash: Vec::new(),
        });
    }
}
```

**Step 3: `base_v`(427行目)は変更不要であることを確認する**

`let base_v = 0.0_f64.clamp(ticks.min, ticks.max);` は対数軸でも `ticks.min` が常に正(decade境界)なので、`0.0.clamp(ticks.min, ticks.max) == ticks.min` となり、既存コードのまま「軸の下端から棒を描く」という chart.js 互換の挙動になる。**このタスクではこの行を触らない。触っていないことをコードレビューで確認する。**

**Step 4: 確認**
```bash
cargo build -p fulgur-chart 2>&1 | tail -30
cargo test -p fulgur-chart 2>&1 | tail -15
```

**Step 5: Commit**
```bash
git add crates/fulgur-chart/src/layout/bar.rs
git commit -m "feat(layout): render logarithmic x-axis for horizontal bar"
```

---

### Task 12: `model.rs` の introspection API を壊さないようにする

**背景:** `compute_axes()` は `Frame.ticks`(`NiceTicks`)をそのまま `linear_axis()` に渡し `kind: "linear"` として `AxisModel` を作る。対数軸でもこの経路は動くが、(a) `kind` が実際には "logarithmic" なのに "linear" と誤報告される、(b) Task 9 で `step` に `0.0` を仮置きしたためたまたま crash はしないが意味的に誤り。**この API は本issueのacceptance criteriaには含まれないが、誤った情報を返す/将来 NaN で crash するリスクがあるため最小限直す。**

**Files:**
- Modify: `crates/fulgur-chart/src/model.rs:354-364`(`linear_axis`)、`:403-461`(`compute_axes`)

**Step 1: `logarithmic_axis` を追加**

```rust
/// LogTicks 由来の major ticks (NiceTicks 形状で受け取る) を対数軸モデルへ変換する。
/// step は decade 間隔が一定でないため意味を持たず、常に None にする。
fn logarithmic_axis(t: &crate::scale::NiceTicks) -> AxisModel {
    AxisModel {
        kind: "logarithmic".to_string(),
        labels: None,
        min: Some(t.min),
        max: Some(t.max),
        step: None,
        ticks: Some(t.ticks.clone()),
    }
}
```

**Step 2: `compute_axes()` の3箇所を分岐**

- 405-413行目(temporal line の y軸):
  ```rust
  if let (ChartKind::Line, XPositions::Temporal { unix_millis }) = (&spec.kind, &spec.x_positions) {
      let frame = crate::layout::common::compute(spec, m);
      let y_model = if spec.y_axis.scale_kind == ScaleKind::Logarithmic {
          logarithmic_axis(&frame.ticks)
      } else {
          linear_axis(&frame.ticks)
      };
      return Some((temporal_axis(unix_millis, &frame.temporal_ticks), y_model, frame.ticks.ticks.len()));
  }
  ```
- 417-428行目(縦棒/Line/Mixed の y軸):
  ```rust
  ChartKind::Bar { horizontal: false, .. } | ChartKind::Line | ChartKind::Mixed => {
      let t = crate::layout::common::compute(spec, m).ticks;
      let y_model = if spec.y_axis.scale_kind == ScaleKind::Logarithmic {
          logarithmic_axis(&t)
      } else {
          linear_axis(&t)
      };
      Some((category_axis(&spec.categories), y_model, t.ticks.len()))
  }
  ```
- 429-440行目(横棒の x軸):
  ```rust
  ChartKind::Bar { horizontal: true, .. } => {
      let (lo, hi) = crate::layout::common::value_domain(spec, &spec.x_axis);
      let (t, x_model) = if spec.x_axis.scale_kind == ScaleKind::Logarithmic {
          let log = crate::scale::log_ticks(lo, hi);
          let nt = crate::scale::NiceTicks { min: log.min, max: log.max, step: 0.0, ticks: log.major };
          let model = logarithmic_axis(&nt);
          (nt, model)
      } else {
          let nt = nice_ticks(lo, hi, 10);
          let model = linear_axis(&nt);
          (nt, model)
      };
      Some((category_axis(&spec.categories), x_model, t.ticks.len()))
  }
  ```

**Step 3: 確認**
```bash
cargo build -p fulgur-chart 2>&1 | tail -30
cargo test -p fulgur-chart model 2>&1 | tail -20
```

**Step 4: Commit**
```bash
git add crates/fulgur-chart/src/model.rs
git commit -m "fix(model): report logarithmic axes correctly in ChartModel introspection"
```

---

### Task 13: 統合テスト(`tests/frontend_chartjs.rs`)

**Files:**
- Modify: `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Step 1: テストを追加(既存の `strict_accepts_scales_stacked` 等の近くに)**

```rust
#[test]
fn strict_accepts_scales_type_logarithmic_on_bar_y_axis() {
    let json = r#"{ "type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,10]}]},
      "options":{"scales":{"y":{"type":"logarithmic"}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}

#[test]
fn logarithmic_type_sets_ir_scale_kind() {
    let json = r#"{ "type":"line","data":{"labels":["a","b"],"datasets":[{"data":[1,10]}]},
      "options":{"scales":{"y":{"type":"logarithmic"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.y_axis.scale_kind, ScaleKind::Logarithmic);
    assert_eq!(spec.x_axis.scale_kind, ScaleKind::Linear);
}

#[test]
fn logarithmic_type_on_category_axis_is_ignored() {
    // 縦棒の x はカテゴリ軸。type:"logarithmic" を指定しても Linear のまま(v1スコープ外、無視)。
    let json = r#"{ "type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,10]}]},
      "options":{"scales":{"x":{"type":"logarithmic"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.x_axis.scale_kind, ScaleKind::Linear);
}

#[test]
fn logarithmic_type_on_unsupported_kind_is_ignored() {
    // scatter は v1 スコープ外。type:"logarithmic" があっても無視して Linear。
    let json = r#"{ "type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2}]}]},
      "options":{"scales":{"y":{"type":"logarithmic"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.y_axis.scale_kind, ScaleKind::Linear);
}

#[test]
fn unknown_scale_type_value_defaults_to_linear_without_error() {
    // "category"/"time" や typo は非strict/strict 問わずエラーにせず Linear 扱い
    // (AxisOptions.type は Option<String> として寛容に受理する設計、Task 3 参照)。
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"type":"category"}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.y_axis.scale_kind, ScaleKind::Linear);
}

#[test]
fn logarithmic_axis_masks_negative_values_to_gap() {
    let json = r#"{ "type":"line","data":{"labels":["a","b","c"],"datasets":[{"data":[1,-5,10]}]},
      "options":{"scales":{"y":{"type":"logarithmic"}}} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(spec.series[0].values[1].is_nan());
}
```

**Step 2: 確認**
```bash
cargo test -p fulgur-chart --test frontend_chartjs logarithmic 2>&1 | tail -30
cargo test -p fulgur-chart --test frontend_chartjs scale_type 2>&1 | tail -30
```

**Step 3: Commit**
```bash
git add crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "test(frontend): cover logarithmic scale parsing, scoping, and negative masking"
```

---

### Task 14: golden PNG の追加

**Files:**
- Create: `examples/specs/bar_logarithmic.json`
- Modify: `crates/fulgur-chart/tests/golden_png.rs:17-27`(`NAMES`)

**Step 1: spec を作成**

```json
{
  "type": "bar",
  "data": {
    "labels": ["A", "B", "C", "D", "E"],
    "datasets": [
      {
        "label": "Requests",
        "data": [5, 50, 500, 5000, 50000],
        "backgroundColor": "#36a2eb"
      }
    ]
  },
  "options": {
    "scales": { "y": { "type": "logarithmic" } },
    "plugins": {
      "title": { "display": true, "text": "Requests (log scale)" }
    }
  }
}
```

**Step 2: `NAMES` に追加**
```rust
const NAMES: &[&str] = &[
    "bar",
    "line",
    "area",
    "pie",
    "line_decimated",
    "line_decimated_lttb",
    "line_with_null",
    "bar_with_null",
    "boxplot_with_null",
    "bar_logarithmic",
];
```

**Step 3: golden を生成**
```bash
UPDATE_GOLDEN=1 cargo test -p fulgur-chart --test golden_png 2>&1 | tail -10
```

**Step 4: 生成された PNG を目視確認(できれば)**

`crates/fulgur-chart/tests/golden/bar_logarithmic.png` を確認し、5本の棒がおおよそ等間隔(log軸なので 5,50,500,5000,50000 は等比間隔)で描かれていること、主目盛(10,100,1000,...)にラベルが付いていること、minorグリッドが薄く/同色で入っていることを確認する。

**Step 5: 再実行して安定していることを確認**
```bash
cargo test -p fulgur-chart --test golden_png 2>&1 | tail -10
```
Expected: PASS(直前に生成した golden と一致)。

**Step 6: Commit**
```bash
git add examples/specs/bar_logarithmic.json crates/fulgur-chart/tests/golden_png.rs crates/fulgur-chart/tests/golden/bar_logarithmic.png
git commit -m "test(golden): add bar_logarithmic golden PNG regression case"
```

---

### Task 15: 全体検証

**Step 1: フルテスト**
```bash
cargo test -p fulgur-chart 2>&1 | tail -60
```
Expected: 全 PASS。

**Step 2: `wasm_runtime` の byte-identical チェック(ログ座標計算で `log10` を導入したため必須)**
```bash
cargo test -p fulgur-chart --test wasm_runtime 2>&1 | tail -20
```
Expected: PASS。

**Step 3: clippy / fmt**
```bash
cargo clippy -p fulgur-chart --all-targets -- -D warnings 2>&1 | tail -40
cargo fmt --check 2>&1 | tail -20
```
Expected: warning/diff なし(あれば修正して追加コミット)。

**Step 4: ワークスペース全体のビルド確認(CLI/chart-server もこの crate に依存)**
```bash
cargo build 2>&1 | tail -20
```

**Step 5: 既知の未対応事項を bd issue にファイルする(セッション終了前に必須)**

以下は本issueのacceptance criteria外と判断した既知のギャップ。それぞれ bd issue 化するか、既存の `fulgur-chart-rwe`(scatter/bubble)に注記を追加する:
- Mixed chart の対数軸対応(現状: type指定があっても常に Linear に強制)
- BoxPlot chart の対数軸対応(同上)
- 対数軸の `ticks.format`(`fulgur-chart-pof` 実装後に統合)

---

## Task 実行順序の依存関係

```
Task 0 (npm install)
Task 1 (ScaleKind追加) → Task 2 (ValueScale導入・golden無変化確認)
Task 3 (schema type追加)
Task 4 (frontend橋渡し・負値マスク) ← Task 1, Task 3
Task 5 (log_ticks骨格)
Task 6 (chartjs_ticks.mjs拡張・実測)
Task 7 (log_ticksをピン留め) ← Task 5, Task 6
Task 8 (value_domain対数分岐) ← Task 1
Task 10 (fmt_num_log)
Task 9 (compute()対数分岐・縦軸描画) ← Task 2, Task 7, Task 8, Task 10
Task 11 (build_horizontal対数分岐) ← Task 9 と同じ依存
Task 12 (model.rs修正) ← Task 9, Task 11
Task 13 (統合テスト) ← Task 4, Task 9, Task 11
Task 14 (golden PNG) ← Task 9, Task 11
Task 15 (全体検証)
```

---

## Task 6 実測結果

**実行環境:** chart.js `4.5.1`(`tools/node_modules/chart.js/package.json`)、node.js `v24.2.0`、`tools/chartjs_ticks.mjs` を `node chartjs_ticks.mjs > /tmp/chartjs_ticks_linear.json 2> /tmp/chartjs_ticks_log.json` で実行(2026-08-08 実施)。

### 最重要の発見: Step 1 の設計コミュニケーション(`getLabelForValue`)は実際のラベル可視性を反映しない

タスク説明・当初のプラン記述はどちらも「`scale.getLabelForValue(value)` を呼ぶのが最も確実」としていたが、これは**誤り**であることが `chart.js` のソース(`tools/node_modules/chart.js/dist/chart.js`, `tools/node_modules/chart.js/dist/chunks/helpers.dataset.js`)を読んで確認し、かつ実測でも再現した。

- `LogarithmicScale.getLabelForValue(value)`(`dist/chart.js:10534`)の実装は:
  ```js
  getLabelForValue(value) {
      return value === undefined ? '0' : formatNumber(value, this.chart.options.locale, this.options.ticks.format);
  }
  ```
  これは**常に**数値をロケール書式(桁区切り・小数)でフォーマットするだけで、可視/非表示の判定は一切行わない。全 tick に対して非空文字列を返す。
- 実際に canvas に描画される文字列は `tick.label` プロパティであり、`Scale.update()` 内の `_convertTicksToLabels()`(`dist/chart.js:3936`, `4190-4202`)→ `generateTickLabels()`(`dist/chart.js:4028-4039`)が `options.ticks.callback` を呼んで設定する。`LogarithmicScale` の既定 `ticks.callback` は `Ticks.formatters.logarithmic`(`dist/chart.js:10452-10459` の `static defaults`)。
- その `Ticks.formatters.logarithmic` の実体(`dist/chunks/helpers.dataset.js:901-917`):
  ```js
  logarithmic (tickValue, index, ticks) {
      if (tickValue === 0) {
          return '0';
      }
      const remain = ticks[index].significand || tickValue / Math.pow(10, Math.floor(log10(tickValue)));
      if ([1, 2, 3, 5, 10, 15].includes(remain) || index > 0.8 * ticks.length) {
          return formatters.numeric.call(this, tickValue, index, ticks);
      }
      return '';
  }
  ```
  つまり `getLabelForValue` は書式化(`formatters.numeric` 相当)だけを担当し、可視性は `Ticks.formatters.logarithmic` が担う別レイヤ。両者は完全に別物であり、`getLabelForValue` だけを見ると「全 tick にラベルが付く」という誤った結論になる(実際、修正前の実測で確認済み: 全ケースで空文字列が一つも出なかった)。

この発見を受け、`tools/chartjs_ticks.mjs` の `getLogTicks()` は最終的に次の3フィールドを出力するよう修正した:
- `significand`: tick 生成時に `generateTicks()`(`dist/chart.js:10412-10448`)が計算する内部カウンタ(後述、mantissa と等しいとは限らない)
- `label`: 実際に描画される文字列(空文字列 = 非表示)。**これが正解データ。**
- `getLabelForValue`: 数値書式のみ(ラベル可視性の判定には使えない)

### 実測データ(生 JSON、`/tmp/chartjs_ticks_log.json` の内容)

```json
[
  {
    "label": "single decade [3,7]",
    "data": [3, 7],
    "yOpts": {},
    "min": 1,
    "max": 7,
    "ticks": [
      { "value": 1,   "major": true,  "significand": 0,  "label": "1.00", "getLabelForValue": "1" },
      { "value": 1.1, "major": false, "significand": 1,  "label": "1.10", "getLabelForValue": "1.1" },
      { "value": 1.2, "major": false, "significand": 2,  "label": "1.20", "getLabelForValue": "1.2" },
      { "value": 1.3, "major": false, "significand": 3,  "label": "1.30", "getLabelForValue": "1.3" },
      { "value": 1.4, "major": false, "significand": 4,  "label": "",     "getLabelForValue": "1.4" },
      { "value": 1.5, "major": false, "significand": 5,  "label": "1.50", "getLabelForValue": "1.5" },
      { "value": 1.6, "major": false, "significand": 6,  "label": "",     "getLabelForValue": "1.6" },
      { "value": 1.7, "major": false, "significand": 7,  "label": "",     "getLabelForValue": "1.7" },
      { "value": 1.8, "major": false, "significand": 8,  "label": "",     "getLabelForValue": "1.8" },
      { "value": 1.9, "major": false, "significand": 9,  "label": "",     "getLabelForValue": "1.9" },
      { "value": 2,   "major": false, "significand": 10, "label": "2.00", "getLabelForValue": "2" },
      { "value": 2.5, "major": false, "significand": 15, "label": "2.50", "getLabelForValue": "2.5" },
      { "value": 3,   "major": false, "significand": 2,  "label": "3.00", "getLabelForValue": "3" },
      { "value": 4,   "major": false, "significand": 3,  "label": "4.00", "getLabelForValue": "4" },
      { "value": 5,   "major": false, "significand": 4,  "label": "5.00", "getLabelForValue": "5" },
      { "value": 6,   "major": false, "significand": 5,  "label": "6.00", "getLabelForValue": "6" },
      { "value": 7,   "major": false, "significand": 6,  "label": "7.00", "getLabelForValue": "7" }
    ]
  },
  {
    "label": "multi decade [30,4000]",
    "data": [30, 4000],
    "yOpts": {},
    "min": 10,
    "max": 4000,
    "ticks": [
      { "value": 10,   "major": true,  "significand": 1,  "label": "10",    "getLabelForValue": "10" },
      { "value": 30,   "major": false, "significand": 3,  "label": "30",    "getLabelForValue": "30" },
      { "value": 60,   "major": false, "significand": 6,  "label": "",      "getLabelForValue": "60" },
      { "value": 80,   "major": false, "significand": 8,  "label": "",      "getLabelForValue": "80" },
      { "value": 100,  "major": true,  "significand": 10, "label": "100",   "getLabelForValue": "100" },
      { "value": 200,  "major": false, "significand": 2,  "label": "200",   "getLabelForValue": "200" },
      { "value": 400,  "major": false, "significand": 4,  "label": "",      "getLabelForValue": "400" },
      { "value": 600,  "major": false, "significand": 6,  "label": "",      "getLabelForValue": "600" },
      { "value": 800,  "major": false, "significand": 8,  "label": "",      "getLabelForValue": "800" },
      { "value": 1000, "major": true,  "significand": 10, "label": "1,000", "getLabelForValue": "1,000" },
      { "value": 2000, "major": false, "significand": 2,  "label": "2,000", "getLabelForValue": "2,000" },
      { "value": 4000, "major": false, "significand": 4,  "label": "4,000", "getLabelForValue": "4,000" }
    ]
  },
  {
    "label": "sub-one [0.003,0.7]",
    "data": [0.003, 0.7],
    "yOpts": {},
    "min": 0.001,
    "max": 0.7,
    "ticks": [
      { "value": 0.001, "major": true,  "significand": 1,  "label": "0.001", "getLabelForValue": "0.001" },
      { "value": 0.003, "major": false, "significand": 3,  "label": "0.003", "getLabelForValue": "0.003" },
      { "value": 0.006, "major": false, "significand": 6,  "label": "",      "getLabelForValue": "0.006" },
      { "value": 0.008, "major": false, "significand": 8,  "label": "",      "getLabelForValue": "0.008" },
      { "value": 0.01,  "major": true,  "significand": 10, "label": "0.010", "getLabelForValue": "0.01" },
      { "value": 0.02,  "major": false, "significand": 2,  "label": "0.020", "getLabelForValue": "0.02" },
      { "value": 0.04,  "major": false, "significand": 4,  "label": "",      "getLabelForValue": "0.04" },
      { "value": 0.06,  "major": false, "significand": 6,  "label": "",      "getLabelForValue": "0.06" },
      { "value": 0.08,  "major": false, "significand": 8,  "label": "",      "getLabelForValue": "0.08" },
      { "value": 0.1,   "major": true,  "significand": 10, "label": "0.100", "getLabelForValue": "0.1" },
      { "value": 0.2,   "major": false, "significand": 2,  "label": "0.200", "getLabelForValue": "0.2" },
      { "value": 0.4,   "major": false, "significand": 4,  "label": "0.400", "getLabelForValue": "0.4" },
      { "value": 0.6,   "major": false, "significand": 6,  "label": "0.600", "getLabelForValue": "0.6" }
    ]
  },
  {
    "label": "wide [1,1000000]",
    "data": [1, 1000000],
    "yOpts": {},
    "min": 0.1,
    "max": 1000000,
    "ticks": [
      { "value": 0.1,     "major": true,  "significand": 1,  "label": "0.1",         "getLabelForValue": "0.1" },
      { "value": 0.6,     "major": false, "significand": 6,  "label": "",            "getLabelForValue": "0.6" },
      { "value": 1,       "major": true,  "significand": 10, "label": "1.0",         "getLabelForValue": "1" },
      { "value": 5,       "major": false, "significand": 5,  "label": "5.0",         "getLabelForValue": "5" },
      { "value": 10,      "major": true,  "significand": 10, "label": "10.0",        "getLabelForValue": "10" },
      { "value": 50,      "major": false, "significand": 5,  "label": "50.0",        "getLabelForValue": "50" },
      { "value": 100,     "major": true,  "significand": 10, "label": "100.0",       "getLabelForValue": "100" },
      { "value": 500,     "major": false, "significand": 5,  "label": "500.0",       "getLabelForValue": "500" },
      { "value": 1000,    "major": true,  "significand": 10, "label": "1,000.0",     "getLabelForValue": "1,000" },
      { "value": 5000,    "major": false, "significand": 5,  "label": "5,000.0",     "getLabelForValue": "5,000" },
      { "value": 10000,   "major": true,  "significand": 10, "label": "10,000.0",    "getLabelForValue": "10,000" },
      { "value": 50000,   "major": false, "significand": 5,  "label": "50,000.0",    "getLabelForValue": "50,000" },
      { "value": 100000,  "major": true,  "significand": 10, "label": "100,000.0",   "getLabelForValue": "100,000" },
      { "value": 500000,  "major": false, "significand": 5,  "label": "500,000.0",   "getLabelForValue": "500,000" },
      { "value": 1000000, "major": true,  "significand": 10, "label": "1,000,000.0", "getLabelForValue": "1,000,000" }
    ]
  },
  {
    "label": "exact powers [1,1000]",
    "data": [1, 1000],
    "yOpts": {},
    "min": 0.1,
    "max": 1000,
    "ticks": [
      { "value": 0.1,  "major": true,  "significand": 1,  "label": "0.1",     "getLabelForValue": "0.1" },
      { "value": 0.3,  "major": false, "significand": 3,  "label": "0.3",     "getLabelForValue": "0.3" },
      { "value": 0.6,  "major": false, "significand": 6,  "label": "",        "getLabelForValue": "0.6" },
      { "value": 0.8,  "major": false, "significand": 8,  "label": "",        "getLabelForValue": "0.8" },
      { "value": 1,    "major": true,  "significand": 10, "label": "1.0",     "getLabelForValue": "1" },
      { "value": 2,    "major": false, "significand": 2,  "label": "2.0",     "getLabelForValue": "2" },
      { "value": 4,    "major": false, "significand": 4,  "label": "",        "getLabelForValue": "4" },
      { "value": 6,    "major": false, "significand": 6,  "label": "",        "getLabelForValue": "6" },
      { "value": 8,    "major": false, "significand": 8,  "label": "",        "getLabelForValue": "8" },
      { "value": 10,   "major": true,  "significand": 10, "label": "10.0",    "getLabelForValue": "10" },
      { "value": 20,   "major": false, "significand": 2,  "label": "20.0",    "getLabelForValue": "20" },
      { "value": 40,   "major": false, "significand": 4,  "label": "",        "getLabelForValue": "40" },
      { "value": 60,   "major": false, "significand": 6,  "label": "",        "getLabelForValue": "60" },
      { "value": 80,   "major": false, "significand": 8,  "label": "",        "getLabelForValue": "80" },
      { "value": 100,  "major": true,  "significand": 10, "label": "100.0",   "getLabelForValue": "100" },
      { "value": 200,  "major": false, "significand": 2,  "label": "200.0",   "getLabelForValue": "200" },
      { "value": 400,  "major": false, "significand": 4,  "label": "400.0",   "getLabelForValue": "400" },
      { "value": 600,  "major": false, "significand": 6,  "label": "600.0",   "getLabelForValue": "600" },
      { "value": 800,  "major": false, "significand": 8,  "label": "800.0",   "getLabelForValue": "800" },
      { "value": 1000, "major": true,  "significand": 10, "label": "1,000.0", "getLabelForValue": "1,000" }
    ]
  }
]
```

(完全な生出力は `tools/chartjs_ticks.mjs` を再実行すればいつでも再生成できる: `cd tools && node chartjs_ticks.mjs 2>&1 >/dev/null` で確認可能。)

### 3つの問いへの回答

**Q1. `major: true` になっているのは mantissa=1(10^n ちょうど)の tick だけか?**

**Yes。** 全5ケースを通して `major: true` の tick は必ず value が 10 のべき乗(0.001, 0.01, 0.1, 1, 10, 100, 1000, 10000, 100000, 1000000)のときだけであり、それ以外は例外なく `major: false`。これはソース上も `isMajor(tickVal)`(`dist/chart.js:10391-10394`)が `tickVal / Math.pow(10, log10Floor(tickVal)) === 1` を判定しているだけで確認できる。**`major` は value から直接計算されるフラグであり、`significand`(下記参照)とは独立している。**

**Q2. `label` が空文字列/非表示になっている tick はあるか? あるならどの mantissa のときか?**

**Yes、ある。** 例(`multi decade [30,4000]`より): `{ value: 60, significand: 6, label: "" }`、`{ value: 80, significand: 8, label: "" }`、`{ value: 400, significand: 4, label: "" }`、`{ value: 600, significand: 6, label: "" }`、`{ value: 800, significand: 8, label: "" }`。

非表示になる条件を実測とソース(`Ticks.formatters.logarithmic`, 前掲)から確定すると:

```
表示 ⟺ [1, 2, 3, 5, 10, 15].includes(tick.significand) OR tickIndex > 0.8 * ticks.length
```

つまり「mantissa 1/2/5 のみ表示」という設計時の仮説は**不正確**だった。正しくは **mantissa 1, 2, 3, 5 が表示され、4, 6, 7, 8, 9 が非表示**(1桁 decade 内の `significand` は 10進の mantissa 数字と一致する場合)。**3 が表示され 5 だけでなく含まれる点、および 4 が非表示である点**が設計時仮説との差分。

ただし2つの重要な注意点がある:

1. **`significand` は常に mantissa 数字そのものとは限らない。** `generateTicks()`(`dist/chart.js:10412-10448`)は「ドメインの最初の decade」ではより細かい刻み(1.1, 1.2, …, 1.9, 2.0, 2.5)を生成でき、そこでの `significand` は 1〜9, 10, 15 という「歩数カウンタ」であり 10進 mantissa 桁と一致しない(例: `single decade [3,7]` ケースの value=2.5 は mantissa 的には "2.5" だが `significand=15`、value=5 は mantissa "5" だが `significand=4`)。2巡目以降の decade(例: 30, 40, …, 90 のような整数 mantissa の decade)では `significand` は mantissa 数字(2〜9)と一致する。
2. **tick 配列の末尾 ~20%(`index > 0.8 * ticks.length`)は mantissa に関係なく常に表示される。** 例: `multi decade [30,4000]` の value=4000(significand=4、本来は非表示のはず)が表示されているのは末尾条件による(ticks.length=12, index=11 > 9.6)。同様に `sub-one [0.003,0.7]` の value=0.6(significand=6)も末尾条件で表示。
3. 加えて、**tick index 0 で `significand` が 0(falsy)になるケースがある**(`single decade [3,7]` の value=1 で `significand: 0`)。この場合 `ticks[index].significand || tickValue / Math.pow(10, floor(log10(tickValue)))` の `||` フォールバックが発火し、実際の mantissa(=1)で判定される(結果的に表示)。`significand` を "既に計算済みのフラグ" として素朴に転記するとこの 0 のケースを見誤る。

**Q3. 一番端(min/max)の tick は major でなくてもラベルが付くか?**

**Yes、付く。** 具体例: `multi decade [30,4000]` の max tick `{ value: 4000, major: false, significand: 4, label: "4,000" }`(mantissa 4 は本来非表示対象だが、末尾 tick のため表示)。`sub-one [0.003,0.7]` の max tick `{ value: 0.6, major: false, significand: 6, label: "0.600" }` も同様(significand 6 は非表示対象だが末尾のため表示)。これは前述の `index > 0.8 * ticks.length` ルールの直接的な帰結であり、「chart.js は目盛の端(データがちょうど収まる境界)を常に読めるようにする」という UX 上の意図と整合する。

### Task 7 への申し送り事項(実装判断はしない、事実のみ記録)

- **ラベル可視性の正しい判定式:** `mantissa ∈ {1,2,3,5} ⟹ 表示`, `mantissa ∈ {4,6,7,8,9} ⟹ 非表示(ただし末尾 tick を除く)`。Task 5 のプランに書かれていた「(A) major のみ」「(B) mantissa 1/2/5」の 2 択のどちらとも一致しない。3 も表示対象に含める必要がある。「末尾 tick は常に表示」という第3の例外ルールも要考慮(または許容してテストケースから除外)。
- **`single decade [3,7]` ケースは Task 5 の `log_ticks(3.0, 7.0)` の出力とそのまま突き合わせられない。** `crates/fulgur-chart/src/scale.rs` の既存(dead code)実装は `log_ticks(3.0, 7.0)` に対し `min=1.0, max=10.0, major=[1,10], minor=[2,3,4,5,6,7,8,9]` を返す(decade 境界に丸めるのみで、chart.js のような「最初の decade だけ細かく刻む」ズーム挙動は実装していない)。一方 chart.js 実測は `min=1, max=7` で tick 値そのものが `1, 1.1, 1.2, …, 1.9, 2, 2.5, 3, 4, 5, 6, 7` と全く異なる集合になる。**ラベル可視性ルールを Task 7 で `log_ticks` にピンする際は、tick 値の集合が chart.js と一致するケース(例えば `multi decade [30,4000]` や `exact powers [1,1000]` のような、対象 decade 内で mantissa が整数 2〜9 で埋まるケース)を使うか、あるいは可視性ルールのみを抽出して mantissa ベースで再実装するかを Task 7 側で判断する必要がある。**
- `_convertTicksToLabels`(`dist/chart.js:4190-4202`)は `label` が `null`/`undefined` の tick だけを配列から削除し(`isNullOrUndef` 判定)、空文字列 `''` の tick は削除せず「ラベルなしの目盛線(グリッドのみ)」として残す。Rust 側で「ラベル非表示」を表現する際は「tick 自体を消す」のではなく「ラベル文字列だけを空にする」設計にする方が chart.js の挙動と一致する。
- `getLabelForValue` は書式化だけを担うため、**数値の書式(桁区切り "1,000" や小数桁 "1.10"/"0.001" など)を再現する目的では引き続き参考になる**(Task 10 の `fmt_num_log` 向け)。ただし桁数は `formatters.numeric` 内部で ticks 間の delta から動的に決まる(`calculateDelta`)ため、それ自体も単純な固定桁数ではない点に注意(例: `single decade [3,7]` は "1.10" のように小数2桁、`multi decade [30,4000]` は "1,000" のように整数)。この点は Task 10 の実測対象になる。
