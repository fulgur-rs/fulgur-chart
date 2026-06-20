# chart.js 適合参照ツール Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 同一 chart.js v4 spec を実 chart.js(node-canvas)と fulgur の両方で評価し、構造・配色を数値/目視で照合する CI 外の参照ツールを整備する(beads: fulgur-chart-uob)。

**Architecture:** fulgur に意味モデルを出す `inspect` サブコマンドを追加し、JS オーケストレータが (1) fulgur 意味モデル ⟷ chart.js 意味モデルの数値照合、(2) fulgur 意味モデルの色が実 SVG に実在するかの描画忠実性 cross-check、を行い JSON/HTML レポートを出す。意味モデルは IR(解決済み色)と layout の `common::compute`(nice_ticks)から構築する。

**Tech Stack:** Rust (clap, serde, serde_json), Node.js (chart.js v4, node-canvas), insta スナップショット。

**重要な事前知識:**
- IR `ChartSpec`(`crates/fulgur-chart/src/ir.rs`)は既に解決済み色 `series[].fill: Vec<Color>` / `stroke: Vec<Color>` を持つ(`fill_at(i)`/`stroke_at(i)` で参照。len==1 はブロードキャスト)。
- 軸の min/max/step/ticks は layout の `layout::common::compute(spec, m) -> Frame` が返す `Frame.ticks: NiceTicks{min,max,step,ticks}`(bar/line/mixed の直交チャート用)。`compute` も `NiceTicks` フィールドも `pub`。`TextMeasurer::new(fulgur_chart::font::DEFAULT_FONT)` で測定器を作る。
- **SVG の色表現**: `crates/fulgur-chart/src/svg.rs` は色を `fill="#rrggbb" fill-opacity="0.5"`(hex + 別 opacity 属性、小文字 hex)で出力する。`rgba(...)` リテラルでは**ない**。cross-check はこのペアを rgba へ正規化して照合する。opacity は alpha<1.0 のときのみ出力され、値は `fmt_num` 整形。
- 対象 spec は `examples/specs/*.json`。chart.js コアで描画可能なのは bar, bar-horizontal, line, area, stacked-bar, pie, doughnut, scatter, bubble, radar。matrix/datalabels/theme/vegalite-* はプラグイン/別DSLのため Phase 1 対象外(ツールは未対応型を skip 表示)。

**正規化色フォーマット(Rust/JS/cross-check で完全一致させる規約):**
`rgba(R,G,B,A)` — R,G,B は整数 0–255、空白なし。A の整形:
- a >= 1.0 → `1`
- a <= 0.0 → `0`
- それ以外 → `round(a*1000)/1000` を 10進表記し末尾ゼロを除去(例 `0.5`, `0.25`, `0.333`)。
例: `rgba(54,162,235,0.5)`, `rgba(255,99,132,1)`。

---

## Phase 1 — Rust `inspect` + 色/軸/counts の意味照合 + JS diff/crosscheck

### Task 1: 意味モデル型と rgba フォーマッタ (`model.rs`)

**Files:**
- Create: `crates/fulgur-chart/src/model.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`(`pub mod model;` を `mod scene;` 付近のアルファベット順位置に追加)

**Step 1: 失敗するテストを書く**

`crates/fulgur-chart/src/model.rs` の末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Color;

    #[test]
    fn rgba_opaque_uses_1() {
        let c = Color { r: 54, g: 162, b: 235, a: 1.0 };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,1)");
    }

    #[test]
    fn rgba_half_alpha() {
        let c = Color { r: 54, g: 162, b: 235, a: 0.5 };
        assert_eq!(rgba_string(&c), "rgba(54,162,235,0.5)");
    }

    #[test]
    fn rgba_transparent_uses_0() {
        let c = Color { r: 0, g: 0, b: 0, a: 0.0 };
        assert_eq!(rgba_string(&c), "rgba(0,0,0,0)");
    }

    #[test]
    fn rgba_trims_trailing_zeros() {
        let c = Color { r: 1, g: 2, b: 3, a: 0.25 };
        assert_eq!(rgba_string(&c), "rgba(1,2,3,0.25)");
    }
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --lib model::tests 2>&1 | tail -20`
Expected: コンパイルエラー(`rgba_string` 未定義 / `model` モジュール未登録)。

**Step 3: 最小実装**

`crates/fulgur-chart/src/model.rs` 冒頭に:

```rust
//! チャート意味モデル: chart.js と数値照合するための、解決済み色・軸目盛り・
//! counts を持つシリアライズ可能な中間表現。描画はせず IR + layout から構築する。

use serde::Serialize;

use crate::ir::{ChartKind, ChartSpec, Color};

/// 解決済み色を正規化 rgba 文字列にする(plan の正規化規約に従う)。
pub fn rgba_string(c: &Color) -> String {
    format!("rgba({},{},{},{})", c.r, c.g, c.b, fmt_alpha(c.a))
}

/// alpha を正規化整形する(>=1→"1", <=0→"0", それ以外は 3 桁丸め・末尾ゼロ除去)。
fn fmt_alpha(a: f32) -> String {
    if a >= 1.0 {
        return "1".to_string();
    }
    if a <= 0.0 {
        return "0".to_string();
    }
    let r = (a as f64 * 1000.0).round() / 1000.0;
    let mut s = format!("{r}");
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

`lib.rs` に `pub mod model;` を追加。

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib model::tests 2>&1 | tail -20`
Expected: PASS(4 tests)。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/model.rs crates/fulgur-chart/src/lib.rs
git commit -m "feat(model): add chart model module with rgba normalization"
```

---

### Task 2: モデル型定義 + meta/series/counts ビルダー

**Files:**
- Modify: `crates/fulgur-chart/src/model.rs`

**Step 1: 失敗するテストを書く**(`model::tests` に追記)

```rust
    use crate::frontend::chartjs;

    #[test]
    fn builds_meta_series_counts_for_bar() {
        let json = r#"{"type":"bar","data":{"labels":["1月","2月","3月"],
          "datasets":[{"label":"売上","data":[120,200,150]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(model.meta.r#type, "bar");
        assert_eq!(model.series.len(), 1);
        assert_eq!(model.series[0].label, "売上");
        // 既定パレット先頭 #36A2EB、fill alpha=0.5 / stroke alpha=1.0(chart.js v4)
        assert_eq!(model.series[0].fill, vec!["rgba(54,162,235,0.5)".to_string()]);
        assert_eq!(model.series[0].stroke, vec!["rgba(54,162,235,1)".to_string()]);
        assert_eq!(model.series[0].values, vec![120.0, 200.0, 150.0]);
        assert_eq!(model.counts.datasets, 1);
        assert_eq!(model.counts.x_ticks, 3);
    }

    #[test]
    fn pie_emits_per_slice_fill() {
        let json = r##"{"type":"pie","data":{"labels":["a","b","c"],
          "datasets":[{"data":[1,2,3],
          "backgroundColor":["#ff0000","#00ff00","#0000ff"]}]}}"##;
        let spec = chartjs::parse(json, false).unwrap();
        let model = build_model_core(&spec);
        assert_eq!(model.series[0].fill.len(), 3);
        assert_eq!(model.series[0].fill[0], "rgba(255,0,0,1)");
    }
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --lib model::tests 2>&1 | tail -20`
Expected: `build_model_core` / 型未定義でコンパイルエラー。

**Step 3: 最小実装**(`model.rs` の型と `build_model_core`)

```rust
#[derive(Debug, Serialize, PartialEq)]
pub struct ChartModel {
    pub meta: Meta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub axes: Option<Axes>,
    pub series: Vec<SeriesModel>,
    pub counts: Counts,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Meta {
    pub r#type: String,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Axes {
    pub x: AxisModel,
    pub y: AxisModel,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AxisModel {
    pub kind: String, // "linear" | "category"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticks: Option<Vec<f64>>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct SeriesModel {
    pub label: String,
    pub fill: Vec<String>,
    pub stroke: Vec<String>,
    pub values: Vec<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Counts {
    pub datasets: usize,
    pub legend_items: usize,
    pub x_ticks: usize,
    pub y_ticks: usize,
}

/// 描画要素数(scatter/bubble は points、その他は values)。
fn element_count(s: &crate::ir::Series) -> usize {
    if s.points.is_empty() { s.values.len() } else { s.points.len() }
}

/// 色ベクタを「要素ごと rgba」に展開しつつ、全要素同色なら長さ1へ畳む。
fn colors_to_strings(colors: &[Color], n: usize) -> Vec<String> {
    use crate::ir::Color as C;
    let at = |i: usize| -> C {
        match colors.len() {
            0 => C { r: 0, g: 0, b: 0, a: 1.0 },
            1 => colors[0],
            _ => colors[i % colors.len()],
        }
    };
    let n = n.max(1);
    let all: Vec<String> = (0..n).map(|i| rgba_string(&at(i))).collect();
    if all.iter().all(|x| x == &all[0]) { vec![all[0].clone()] } else { all }
}

fn chart_type_name(kind: &ChartKind) -> &'static str {
    match kind {
        ChartKind::Bar { horizontal: true, .. } => "bar-horizontal",
        ChartKind::Bar { .. } => "bar",
        ChartKind::Line => "line",
        ChartKind::Pie { donut_ratio } if *donut_ratio > 0.0 => "doughnut",
        ChartKind::Pie { .. } => "pie",
        ChartKind::Scatter => "scatter",
        ChartKind::Bubble => "bubble",
        ChartKind::Radar => "radar",
        ChartKind::Mixed => "mixed",
        ChartKind::Matrix { .. } => "matrix",
    }
}

/// 軸抜き(meta/series/counts のみ)のコアモデル。Task 3 で軸を載せる。
pub fn build_model_core(spec: &ChartSpec) -> ChartModel {
    let series: Vec<SeriesModel> = spec
        .series
        .iter()
        .map(|s| {
            let n = element_count(s);
            SeriesModel {
                label: s.name.clone(),
                fill: colors_to_strings(&s.fill, n),
                stroke: colors_to_strings(&s.stroke, n),
                values: s.values.clone(),
            }
        })
        .collect();
    let legend_items = spec.series.iter().filter(|s| !s.name.is_empty()).count();
    ChartModel {
        meta: Meta {
            r#type: chart_type_name(&spec.kind).to_string(),
            width: spec.width,
            height: spec.height,
        },
        axes: None,
        series,
        counts: Counts {
            datasets: spec.series.len(),
            legend_items,
            x_ticks: spec.categories.len(),
            y_ticks: 0,
        },
    }
}
```

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib model::tests 2>&1 | tail -20`
Expected: PASS(全 model テスト)。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/model.rs
git commit -m "feat(model): build meta/series/counts from IR"
```

---

### Task 3: 直交チャートの軸を載せる + 完全ビルダー `build_model`

**Files:**
- Modify: `crates/fulgur-chart/src/model.rs`

**Step 1: 失敗するテストを書く**(`model::tests` に追記)

```rust
    use crate::font::DEFAULT_FONT;
    use crate::text::TextMeasurer;

    #[test]
    fn bar_has_linear_y_and_category_x() {
        let json = r#"{"type":"bar","data":{"labels":["1月","2月","3月"],
          "datasets":[{"data":[0,100,50]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        let axes = model.axes.expect("bar には軸があるべき");
        assert_eq!(axes.y.kind, "linear");
        assert_eq!(axes.y.min, Some(0.0));
        assert_eq!(axes.x.kind, "category");
        assert_eq!(axes.x.labels.as_deref(), Some(&["1月".to_string(),"2月".to_string(),"3月".to_string()][..]));
        // y_ticks は目盛り数に同期
        assert_eq!(model.counts.y_ticks, axes.y.ticks.unwrap().len());
    }

    #[test]
    fn pie_has_no_axes() {
        let json = r#"{"type":"pie","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let model = build_model(&spec, &m);
        assert!(model.axes.is_none());
    }
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --lib model::tests 2>&1 | tail -20`
Expected: `build_model` 未定義でコンパイルエラー。

**Step 3: 最小実装**(`model.rs`)

```rust
use crate::text::TextMeasurer;

/// 直交チャート(縦棒・線・mixed)か。これらは layout::common::compute が使える。
fn is_cartesian_vertical(kind: &ChartKind) -> bool {
    matches!(
        kind,
        ChartKind::Bar { horizontal: false, .. } | ChartKind::Line | ChartKind::Mixed
    )
}

/// IR + layout から完全な意味モデルを構築する。直交チャートのみ軸を載せる。
pub fn build_model(spec: &ChartSpec, m: &TextMeasurer) -> ChartModel {
    let mut model = build_model_core(spec);
    if is_cartesian_vertical(&spec.kind) {
        let frame = crate::layout::common::compute(spec, m);
        let t = &frame.ticks;
        let y = AxisModel {
            kind: "linear".to_string(),
            labels: None,
            min: Some(t.min),
            max: Some(t.max),
            step: Some(t.step),
            ticks: Some(t.ticks.clone()),
        };
        let x = AxisModel {
            kind: "category".to_string(),
            labels: Some(spec.categories.clone()),
            min: None,
            max: None,
            step: None,
            ticks: None,
        };
        model.counts.y_ticks = t.ticks.len();
        model.axes = Some(Axes { x, y });
    }
    model
}
```

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart --lib model 2>&1 | tail -20`
Expected: PASS。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/model.rs
git commit -m "feat(model): attach linear/category axes for cartesian charts"
```

---

### Task 4: `inspect` CLI サブコマンド

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs`
- Test: `crates/fulgur-chart-cli/tests/cli.rs`(末尾に追記)

**Step 1: 失敗するテストを書く**(`crates/fulgur-chart-cli/tests/cli.rs`)

既存テストの記法(assert_cmd / Command 等)に合わせること。まず冒頭の既存ヘルパを確認し、同じ流儀で:

```rust
#[test]
fn inspect_bar_emits_model_json() {
    // examples/specs/bar.json を inspect して JSON モデルを stdout に得る。
    let spec = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/specs/bar.json");
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_fulgur-chart"))
        .args(["inspect", spec, "-o", "-"])
        .output()
        .expect("run inspect");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["meta"]["type"], "bar");
    assert!(v["series"].as_array().unwrap().len() >= 1);
    assert!(v["axes"]["y"]["ticks"].is_array());
}
```

注: `CARGO_BIN_EXE_fulgur-chart` が使えない場合は既存テストと同じ起動方法(assert_cmd の `Command::cargo_bin` 等)に合わせる。既存 `cli.rs` 冒頭を読んで判断する。

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart-cli --test cli inspect 2>&1 | tail -20`
Expected: `inspect` サブコマンド未実装で失敗(exit 非ゼロ or unknown subcommand)。

**Step 3: 最小実装**(`main.rs`)

`enum Command` に追加:

```rust
    /// Emit fulgur's resolved semantic model (colors, axis ticks, counts) as JSON.
    Inspect(InspectArgs),
```

引数構造体:

```rust
#[derive(Parser)]
struct InspectArgs {
    /// Input spec file path. Use '-' to read from stdin.
    spec: String,
    /// Output path. Use '-' for stdout (default).
    #[arg(short, long, default_value = "-")]
    output: String,
    /// Input DSL (chartjs or vegalite). Auto-detected when omitted.
    #[arg(long)]
    dsl: Option<String>,
    /// Override chart width.
    #[arg(long)]
    width: Option<f64>,
    /// Override chart height.
    #[arg(long)]
    height: Option<f64>,
    /// Font file for text metrics. Defaults to the bundled font.
    #[arg(long)]
    font: Option<String>,
}
```

`main` の match に `Command::Inspect(args) => run_inspect(args),` を追加。実装:

```rust
fn run_inspect(args: InspectArgs) {
    let json = match read_spec(&args.spec) {
        Ok(s) => s,
        Err(e) => { eprintln!("error: failed to read input: {e}"); std::process::exit(1); }
    };
    let dsl: String = match &args.dsl {
        Some(d) => {
            if d != "chartjs" && d != "vegalite" {
                eprintln!("error: unsupported DSL '{d}' (supported: chartjs, vegalite)");
                std::process::exit(1);
            }
            d.clone()
        }
        None => match detect_dsl(&json) { Ok(d) => d.to_string(), Err(e) => { eprintln!("{e}"); std::process::exit(1); } }
    };
    let mut spec_ir = match parse_spec(&json, &dsl, false) {
        Ok(s) => s,
        Err(e) => { eprintln!("error: parse failed: {e}"); std::process::exit(1); }
    };
    if let Some(w) = args.width { spec_ir.width = w; }
    if let Some(h) = args.height { spec_ir.height = h; }
    if let Err(e) = fulgur_chart::guard::validate_spec(&spec_ir, &fulgur_chart::guard::InputLimits::default()) {
        eprintln!("error: {e}"); std::process::exit(1);
    }
    // フォント読込(測定器用)。--font 指定がなければバンドルフォント。
    let font_bytes: Vec<u8> = match &args.font {
        Some(path) => match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => { eprintln!("error: failed to read font '{path}': {e}"); std::process::exit(1); }
        },
        None => fulgur_chart::font::DEFAULT_FONT.to_vec(),
    };
    let measurer = match fulgur_chart::text::TextMeasurer::new(&font_bytes) {
        Ok(m) => m,
        Err(e) => { eprintln!("error: font load failed: {e}"); std::process::exit(1); }
    };
    let model = fulgur_chart::model::build_model(&spec_ir, &measurer);
    let out = serde_json::to_string_pretty(&model).expect("model serialization failed");
    if let Err(e) = write_output(&args.output, out.as_bytes()) {
        eprintln!("error: write failed: {e}"); std::process::exit(3);
    }
}
```

注意: `TextMeasurer::new` の正確なシグネチャ(`&[u8]` か `Vec<u8>` か、戻り値の Result/Option)は `crates/fulgur-chart/src/text.rs` を確認して合わせる。`font::DEFAULT_FONT` の型(`&[u8]`)も確認。

**Step 4: テストが通ることを確認**

Run: `cargo test -p fulgur-chart-cli --test cli inspect 2>&1 | tail -20`
Expected: PASS。続けて全体回帰: `cargo test 2>&1 | tail -15`(既存 30+ テストが緑のまま)。

**Step 5: コミット**

```bash
git add crates/fulgur-chart-cli/src/main.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(cli): add inspect subcommand emitting semantic model JSON"
```

---

### Task 5: `inspect` スナップショット(insta)で代表 spec を固定

**Files:**
- Create: `crates/fulgur-chart/tests/inspect_model.rs`

**Step 1: テストを書く**(insta スナップショット)

既存の insta 利用箇所(`grep -rn "insta::" crates/fulgur-chart/tests | head`)の流儀に合わせる。代表 spec を直接 IR 化してモデルを YAML スナップショットに固定する:

```rust
use fulgur_chart::font::DEFAULT_FONT;
use fulgur_chart::frontend::chartjs;
use fulgur_chart::model::build_model;
use fulgur_chart::text::TextMeasurer;

fn model_yaml(name: &str) -> fulgur_chart::model::ChartModel {
    let path = format!("{}/../../examples/specs/{}.json", env!("CARGO_MANIFEST_DIR"), name);
    let json = std::fs::read_to_string(path).unwrap();
    let spec = chartjs::parse(&json, false).unwrap();
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    build_model(&spec, &m)
}

#[test]
fn snapshot_bar_model() {
    insta::assert_yaml_snapshot!(model_yaml("bar"));
}

#[test]
fn snapshot_pie_model() {
    insta::assert_yaml_snapshot!(model_yaml("pie"));
}

#[test]
fn snapshot_line_model() {
    insta::assert_yaml_snapshot!(model_yaml("line"));
}
```

`ChartModel` に `serde::Serialize` は既にあるので insta YAML 可。

**Step 2: スナップショット生成**

Run: `INSTA_UPDATE=always cargo test -p fulgur-chart --test inspect_model 2>&1 | tail -15`
Expected: スナップショットが `crates/fulgur-chart/tests/snapshots/` に生成され PASS。生成された `.snap` を目視確認(色 rgba・y 軸 ticks が妥当か)。

**Step 3: 固定確認**

Run: `cargo test -p fulgur-chart --test inspect_model 2>&1 | tail -15`
Expected: PASS(差分なし)。

**Step 4: コミット**

```bash
git add crates/fulgur-chart/tests/inspect_model.rs crates/fulgur-chart/tests/snapshots/
git commit -m "test(model): pin inspect model snapshots for bar/pie/line"
```

---

### Task 6: JS — chart.js 抽出器 `extract.mjs`

**Files:**
- Create: `tools/chartjs-compat/extract.mjs`
- Modify: `tools/package.json`(scripts に `"compat"` を追加、後続タスクで使用)
- Test: `tools/chartjs-compat/extract.test.mjs`

**前提セットアップ(このタスク開始時に一度):**

```bash
cd tools && npm install
```
`tools/package.json` の dependencies に既に `chart.js` と `canvas` がある。無ければ `npm install chart.js@4 canvas` する。

**Step 1: 失敗するテストを書く**(`tools/chartjs-compat/extract.test.mjs`、node:test 使用)

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { extractChartjsModel } from './extract.mjs';

test('bar: 系列色と軸目盛りを共通スキーマで抽出', async () => {
  const spec = { type: 'bar', data: { labels: ['1月','2月','3月'],
    datasets: [{ label: '売上', data: [0,100,50] }] } };
  const model = await extractChartjsModel(spec, 800, 600);
  assert.equal(model.meta.type, 'bar');
  assert.equal(model.series.length, 1);
  // chart.js v4 既定: 解決済み色は rgba 正規化済み文字列
  assert.match(model.series[0].fill[0], /^rgba\(\d+,\d+,\d+,[\d.]+\)$/);
  assert.equal(model.axes.y.kind, 'linear');
  assert.ok(Array.isArray(model.axes.y.ticks));
  assert.equal(model.axes.x.labels.length, 3);
});
```

**Step 2: 失敗を確認**

Run: `cd tools && node --test chartjs-compat/extract.test.mjs 2>&1 | tail -20`
Expected: `extractChartjsModel` 未定義で失敗。

**Step 3: 最小実装**(`tools/chartjs-compat/extract.mjs`)

要点:
- node-canvas で `Chart` を構築、`animation:false`、`responsive:false`、明示 width/height。
- 色は**描画後の解決済み element options** から: `chart.update()` 後 `chart.getDatasetMeta(i).data[j].options.backgroundColor/borderColor`。生 dataset プロパティは使わない。
- 色正規化は node-canvas の `ctx.fillStyle` ラウンドトリップを使い、plan の正規化 rgba 規約に整形する `toRgba()` を実装。
- 軸: 線形スケール(`chart.scales` から `type==='linear'` のものを y、`category`/`x` を x)。`scale.min/max/ticks.map(t=>t.value)`、step は ticks 差分。

```js
import { createCanvas } from 'canvas';
import { Chart } from 'chart.js/auto';

Chart.defaults.font.size = 12;

// CSS 色文字列 → 正規化 rgba(R,G,B,A)。node-canvas の fillStyle 解釈を利用。
export function toRgba(css) {
  const c = createCanvas(1, 1);
  const ctx = c.getContext('2d');
  ctx.fillStyle = '#000';
  ctx.fillStyle = css;            // 無効なら黒のまま
  const v = ctx.fillStyle;        // '#rrggbb' か 'rgba(r, g, b, a)'
  let r, g, b, a = 1;
  if (v.startsWith('#')) {
    r = parseInt(v.slice(1, 3), 16); g = parseInt(v.slice(3, 5), 16); b = parseInt(v.slice(5, 7), 16);
  } else {
    const m = v.match(/rgba?\(([^)]+)\)/);
    const p = m[1].split(',').map(s => s.trim());
    r = +p[0]; g = +p[1]; b = +p[2]; a = p[3] === undefined ? 1 : +p[3];
  }
  return `rgba(${r},${g},${b},${fmtAlpha(a)})`;
}

function fmtAlpha(a) {
  if (a >= 1) return '1';
  if (a <= 0) return '0';
  let s = String(Math.round(a * 1000) / 1000);
  return s;
}

// 全要素同色なら長さ1へ畳む(fulgur 側 colors_to_strings と対称)。
function collapse(arr) {
  return arr.every(x => x === arr[0]) ? [arr[0]] : arr;
}

export async function extractChartjsModel(spec, width, height) {
  const canvas = createCanvas(width, height);
  const ctx = canvas.getContext('2d');
  const chart = new Chart(ctx, {
    type: spec.type,
    data: spec.data,
    options: { ...(spec.options || {}), animation: false, responsive: false },
  });
  chart.update();

  const series = spec.data.datasets.map((ds, i) => {
    const meta = chart.getDatasetMeta(i);
    const n = meta.data.length || (ds.data ? ds.data.length : 0);
    const fill = collapse(Array.from({ length: n }, (_, j) =>
      toRgba(meta.data[j]?.options?.backgroundColor ?? '#000')));
    const stroke = collapse(Array.from({ length: n }, (_, j) =>
      toRgba(meta.data[j]?.options?.borderColor ?? '#000')));
    const values = Array.isArray(ds.data)
      ? ds.data.map(d => (typeof d === 'object' && d !== null ? (d.y ?? d.v ?? null) : d))
      : [];
    return { label: ds.label ?? '', fill, stroke, values };
  });

  // 軸(線形スケールがあれば)。
  let axes;
  const scaleIds = Object.keys(chart.scales);
  const linId = scaleIds.find(id => chart.scales[id].type === 'linear');
  const catId = scaleIds.find(id => chart.scales[id].type === 'category');
  if (linId) {
    const s = chart.scales[linId];
    const ticks = s.ticks.map(t => t.value);
    const step = ticks.length >= 2 ? ticks[1] - ticks[0] : null;
    const yAxis = { kind: 'linear', min: s.min, max: s.max, step, ticks };
    const xAxis = catId
      ? { kind: 'category', labels: chart.scales[catId].getLabels() }
      : { kind: 'linear' };
    axes = { x: xAxis, y: yAxis };
  }

  const png = canvas.toBuffer('image/png');
  chart.destroy();

  return {
    meta: { type: spec.type, width, height },
    axes,
    series,
    counts: {
      datasets: spec.data.datasets.length,
      legend_items: spec.data.datasets.filter(d => d.label).length,
      x_ticks: (spec.data.labels || []).length,
      y_ticks: axes ? axes.y.ticks.length : 0,
    },
    png, // Buffer(レポート用)
  };
}
```

注: chart.js の `meta.data[j].options.backgroundColor` が `bar-horizontal`(`indexAxis:'y'`)で linear/category の軸割当が入れ替わる点に留意。`type==='linear'` を y とみなす単純規則で大半は足りるが、horizontal では x が linear になる。最小実装では linId を y に固定し、horizontal は Phase 2 の geometry 同様、差分エンジン側で軸キー名ではなく `kind` で突き合わせる(Task 7 参照)。

**Step 4: テストが通ることを確認**

Run: `cd tools && node --test chartjs-compat/extract.test.mjs 2>&1 | tail -20`
Expected: PASS。

**Step 5: コミット**

```bash
git add tools/chartjs-compat/extract.mjs tools/chartjs-compat/extract.test.mjs tools/package.json tools/package-lock.json
git commit -m "feat(compat): chart.js model extractor via node-canvas"
```

---

### Task 7: JS — 差分エンジン `diff.mjs`

**Files:**
- Create: `tools/chartjs-compat/diff.mjs`
- Test: `tools/chartjs-compat/diff.test.mjs`

**Step 1: 失敗するテストを書く**

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diffModels, TOLERANCES } from './diff.mjs';

const base = () => ({
  meta: { type: 'bar', width: 800, height: 600 },
  axes: { x: { kind: 'category', labels: ['a','b'] },
          y: { kind: 'linear', min: 0, max: 100, step: 20, ticks: [0,20,40,60,80,100] } },
  series: [{ label: 's', fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'], values: [10,90] }],
  counts: { datasets: 1, legend_items: 1, x_ticks: 2, y_ticks: 6 },
});

test('同一モデルは PASS', () => {
  const r = diffModels(base(), base());
  assert.equal(r.pass, true);
});

test('色不一致は FAIL', () => {
  const f = base(); const c = base();
  c.series[0].fill = ['rgba(54,162,235,0.25)']; // alpha 乗算バグ相当
  const r = diffModels(f, c);
  assert.equal(r.pass, false);
  assert.equal(r.dimensions.colors.pass, false);
});

test('目盛り不一致は FAIL', () => {
  const f = base(); const c = base();
  c.axes.y.ticks = [0,25,50,75,100];
  const r = diffModels(f, c);
  assert.equal(r.dimensions.axes.pass, false);
});
```

**Step 2: 失敗を確認**

Run: `cd tools && node --test chartjs-compat/diff.test.mjs 2>&1 | tail -20`
Expected: 未定義で失敗。

**Step 3: 最小実装**(`tools/chartjs-compat/diff.mjs`)

```js
export const TOLERANCES = { geometryNorm: 0.02 };

function num(a, b) { return a === b || (a != null && b != null && Math.abs(a - b) < 1e-9); }
function arrNum(a, b) { return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((x, i) => num(x, b[i])); }

// fulgur/chart.js の fill 表現(長さ1 畳み or 要素配列)を要素数 n に展開して比較。
function colorsEqual(a, b) {
  if (!Array.isArray(a) || !Array.isArray(b)) return false;
  const n = Math.max(a.length, b.length);
  const at = (arr, i) => arr.length === 1 ? arr[0] : arr[i % arr.length];
  for (let i = 0; i < n; i++) if (at(a, i) !== at(b, i)) return false;
  return true;
}

export function diffModels(fulgur, chartjs) {
  const dims = {};

  // colors
  const colorDiffs = [];
  const ns = Math.min(fulgur.series.length, chartjs.series.length);
  for (let i = 0; i < ns; i++) {
    if (!colorsEqual(fulgur.series[i].fill, chartjs.series[i].fill))
      colorDiffs.push({ series: i, field: 'fill', fulgur: fulgur.series[i].fill, chartjs: chartjs.series[i].fill });
    if (!colorsEqual(fulgur.series[i].stroke, chartjs.series[i].stroke))
      colorDiffs.push({ series: i, field: 'stroke', fulgur: fulgur.series[i].stroke, chartjs: chartjs.series[i].stroke });
  }
  dims.colors = { pass: colorDiffs.length === 0, diffs: colorDiffs };

  // axes(両方に linear y がある場合のみ厳密比較)
  const fy = fulgur.axes?.y, cy = chartjs.axes?.y;
  if (fy && cy) {
    const axDiffs = [];
    if (!num(fy.min, cy.min)) axDiffs.push({ field: 'y.min', fulgur: fy.min, chartjs: cy.min });
    if (!num(fy.max, cy.max)) axDiffs.push({ field: 'y.max', fulgur: fy.max, chartjs: cy.max });
    if (!num(fy.step, cy.step)) axDiffs.push({ field: 'y.step', fulgur: fy.step, chartjs: cy.step });
    if (!arrNum(fy.ticks, cy.ticks)) axDiffs.push({ field: 'y.ticks', fulgur: fy.ticks, chartjs: cy.ticks });
    dims.axes = { pass: axDiffs.length === 0, diffs: axDiffs };
  } else {
    dims.axes = { pass: true, skipped: true };
  }

  // counts
  const countDiffs = [];
  for (const k of ['datasets', 'legend_items', 'x_ticks', 'y_ticks']) {
    if (fulgur.counts[k] !== chartjs.counts[k])
      countDiffs.push({ field: k, fulgur: fulgur.counts[k], chartjs: chartjs.counts[k] });
  }
  dims.counts = { pass: countDiffs.length === 0, diffs: countDiffs };

  const pass = Object.values(dims).every(d => d.pass);
  return { pass, dimensions: dims };
}
```

**Step 4: テストが通ることを確認**

Run: `cd tools && node --test chartjs-compat/diff.test.mjs 2>&1 | tail -20`
Expected: PASS。

**Step 5: コミット**

```bash
git add tools/chartjs-compat/diff.mjs tools/chartjs-compat/diff.test.mjs
git commit -m "feat(compat): semantic diff engine with per-dimension verdicts"
```

---

### Task 8: JS — 描画忠実性 cross-check `crosscheck.mjs`

**Files:**
- Create: `tools/chartjs-compat/crosscheck.mjs`
- Test: `tools/chartjs-compat/crosscheck.test.mjs`

**目的:** fulgur 意味モデルの系列色が、実 fulgur SVG に実在し、かつ意図しない alpha で描かれていないか検証。SVG は `fill="#rrggbb" fill-opacity="0.5"`(stroke も同様)形式なので、データマークのタグから (RGB, alpha) を **fill/stroke の役割別**に抽出し、(1) alpha 整合性(painted ⊆ claimed)と (2) 系列単位の塗装存在を判定する。

> 実装メモ(計画から進化した点):
> - 返却は `{ pass, divergences, unpainted }`(当初案の `missing` 単一リストではない)。`divergences` は role/rgb/paintedAlpha/modelAlphas を持つ alpha 不整合、`unpainted` は系列丸ごと未塗装。
> - fill と stroke の alpha は**役割別**に追跡する(同一 RGB で fill 0.5 / stroke 1 の棒の典型ケースを取りこぼさない)。
> - 走査は**データマーク**(`rect`/`circle`/`path`/`polyline`/`polygon`)のみ。chrome である `line`(グリッド/軸)と `text`(ラベル)は除外し、系列色と同 RGB の chrome による偽の不整合を防ぐ。

**Step 1: テストを書く**

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { crosscheckColors } from './crosscheck.mjs';

// fill@0.5 と stroke@1 を両方描けば PASS。
test('MATCH: fill@0.5 + stroke@1 を SVG が両方描く → PASS', () => {
  const svg = `<svg><rect fill="#36a2eb" fill-opacity="0.5"/><path stroke="#36a2eb"/></svg>`;
  const model = { series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }] };
  assert.equal(crosscheckColors(model, svg).pass, true);
});

// fill が @0.25 で描かれる → fill 役割の主張 {0.5} に無い → divergence で FAIL。
test('ALPHA-MULTIPLIER BUG: fill@0.25 → fill 役割の divergence で FAIL', () => {
  const svg = `<svg><rect fill="#36a2eb" fill-opacity="0.25"/><path stroke="#36a2eb"/></svg>`;
  const model = { series: [{ fill: ['rgba(54,162,235,0.5)'], stroke: ['rgba(54,162,235,1)'] }] };
  const r = crosscheckColors(model, svg);
  assert.equal(r.pass, false);
  assert.equal(r.divergences[0].role, 'fill');
  assert.deepEqual(r.divergences[0].modelAlphas, ['0.5']);
});
```

**Step 2: 失敗を確認**

Run: `cd tools && node --test chartjs-compat/crosscheck.test.mjs 2>&1 | tail -20`
Expected: 未定義で失敗。

**Step 3: 実装**(`tools/chartjs-compat/crosscheck.mjs`)

```js
import { fmtAlpha } from './color-util.mjs';

const ROLES = [['fill', 'fill-opacity'], ['stroke', 'stroke-opacity']];
const normalizeHex = (hex) => hex.toLowerCase();

// 'rgba(r,g,b,a)' → { rgb: '#rrggbb', alpha: '<canonical>' }。
function parseRgba(s) {
  const m = s.match(/^rgba\((\d+),(\d+),(\d+),([\d.]+)\)$/);
  if (!m) return null;
  const rgb = '#' + [+m[1], +m[2], +m[3]].map((v) => v.toString(16).padStart(2, '0')).join('');
  return { rgb, alpha: fmtAlpha(parseFloat(m[4])) };
}

// データマークの fill/stroke を役割別に抽出: { fill: Map<rgb,Set>, stroke: Map<rgb,Set> }。
// chrome(<line> グリッド/軸・<text> ラベル)は走査しない。
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
  const modelByRole = { fill: new Map(), stroke: new Map() };
  for (const s of model.series) {
    for (const [role, list] of [['fill', s.fill], ['stroke', s.stroke]]) {
      for (const c of list || []) {
        const p = parseRgba(c);
        if (!p || p.alpha === '0') continue;
        if (!modelByRole[role].has(p.rgb)) modelByRole[role].set(p.rgb, new Set());
        modelByRole[role].get(p.rgb).add(p.alpha);
      }
    }
  }
  // 1. alpha 整合性(役割別): painted ⊆ claimed。
  const divergences = [];
  for (const role of ['fill', 'stroke']) {
    for (const [rgb, claimed] of modelByRole[role]) {
      const painted = svgByR[role].get(rgb);
      if (!painted) continue;
      for (const a of painted)
        if (!claimed.has(a)) divergences.push({ role, rgb, paintedAlpha: a, modelAlphas: [...claimed] });
    }
  }
  // 2. 系列単位: 各系列の rgb の少なくとも 1 つが(役割を問わず)塗られている。
  const unpainted = [];
  for (let i = 0; i < model.series.length; i++) {
    const rgbs = [...(model.series[i].fill || []), ...(model.series[i].stroke || [])]
      .map(parseRgba).filter((p) => p && p.alpha !== '0').map((p) => p.rgb);
    if (rgbs.length === 0) continue;
    if (!rgbs.some((rgb) => svgByR.fill.has(rgb) || svgByR.stroke.has(rgb)))
      unpainted.push({ series: i, rgbs: [...new Set(rgbs)] });
  }
  return { pass: divergences.length === 0 && unpainted.length === 0, divergences, unpainted };
}
```

**Step 4: テストが通ることを確認**

Run: `cd tools && node --test chartjs-compat/crosscheck.test.mjs 2>&1 | tail -20`
Expected: PASS。

**Step 5: コミット**

```bash
git add tools/chartjs-compat/crosscheck.mjs tools/chartjs-compat/crosscheck.test.mjs
git commit -m "feat(compat): render-fidelity color cross-check against fulgur SVG"
```

---

### Task 9: JS — オーケストレータ `compat.mjs` + レポート + npm script

**Files:**
- Create: `tools/chartjs-compat/report.mjs`
- Create: `tools/chartjs-compat/compat.mjs`
- Modify: `tools/package.json`(scripts: `"compat": "node chartjs-compat/compat.mjs"`)
- Modify: `tools/.gitignore`(`report/` を追加)

**Step 1: `report.mjs` 実装**(テストは軽く、生成物の存在のみ確認)

`report.mjs` は:
- `writeJsonReport(name, result)` → `tools/report/<name>.json`
- `writeHtmlReport(name, result, fulgurPngBuf, chartjsPngBuf)` → `tools/report/<name>.html`(両 PNG を base64 data URI で左右に並べ、上部に次元別 PASS/FAIL バッジと差分表)

**Step 2: `compat.mjs` 実装**

擬似フロー:

```js
// 1. 対象 spec 決定: 引数があればそれ、無ければ COMPAT_SPECS 既定リスト。
const COMPAT_SPECS = ['bar','bar-horizontal','line','area','stacked-bar','pie','doughnut','scatter','bubble'];
// 2. fulgur バイナリを一度ビルド: execSync('cargo build -p fulgur-chart-cli', {cwd: repoRoot})
//    バイナリパス: <repoRoot>/target/debug/fulgur-chart
// 3. spec ごと:
//    - fulgurModel = JSON.parse(execFileSync(bin, ['inspect', specPath, '-o', '-']))
//    - fulgurSvg   = execFileSync(bin, ['render', specPath, '-o', '-']).toString()  // SVG は stdout
//    - fulgurPng   = execFileSync(bin, ['render', specPath, '-o', '-', '--format', 'png'])
//    - spec JSON を読み chartjsModel = await extractChartjsModel(spec, w, h)
//      (w,h は fulgurModel.meta.width/height に合わせる)
//    - diff = diffModels(fulgurModel, chartjsModel)
//    - cross = crosscheckColors(fulgurModel, fulgurSvg)
//    - result = { name, diff, cross, pass: diff.pass && cross.pass }
//    - writeJsonReport / writeHtmlReport
//    - コンソールに PASS/FAIL を1行表示
// 4. 1つでも FAIL なら process.exitCode = 1。
```

注意点:
- `render -o -` は SVG を stdout に出す(既存仕様、main.rs の single mode)。PNG stdout は `--format png` を明示。
- chart.js が未対応の型(matrix 等)を引数指定された場合は skip 表示し、その spec は PASS/FAIL 対象外。
- `execFileSync` のバッファ上限に注意(PNG 用に `maxBuffer: 64*1024*1024`)。

**Step 3: 動作確認(代表 spec)**

```bash
cd tools && npm install
npm run compat -- bar
```
Expected: コンソールに `bar: PASS`(または既知の差分)。`tools/report/bar.json` と `tools/report/bar.html` が生成される。HTML をブラウザで開き fulgur と chart.js が左右に並ぶことを目視。

**Step 4: 全 spec 実行**

```bash
cd tools && npm run compat 2>&1 | tail -20
```
Expected: 各 spec の PASS/FAIL 行。既知の互換(直近修正済み palette/alpha)で色次元は PASS。

**Step 5: cross-check の有効性を手動確認(任意・記録のみ)**

`resolve_colors` か renderer を一時的に壊して色を変え、`npm run compat -- bar` が cross-check FAIL を出すことを確認 → 確認後 revert。これはコミットしない(ツールの有効性検証)。

**Step 6: コミット**

```bash
git add tools/chartjs-compat/report.mjs tools/chartjs-compat/compat.mjs tools/package.json tools/.gitignore
git commit -m "feat(compat): orchestrator with JSON+HTML reports and npm run compat"
```

---

## Phase 2 — geometry 正規化照合 + HTML 強化(任意・後続)

Phase 1 完了で issue の中核(色/目盛りの数値照合 + renderer 乖離検出 + 目視 HTML)を満たす。Phase 2 は上乗せで、必要に応じて別 issue 化してよい。

### Task 10: geometry をモデルに追加(fulgur 側)

- `model.rs` に `Geometry { plot_area: RectN, elements: Vec<ElemN> }` を追加。`ElemN { series, index, kind, nx, ny, nw, nh }`(プロット領域基準 [0,1] 正規化)。
- bar の矩形は `layout::common::compute` の frame(`plot_*`, `ys.map`)と `category_center`/`band_width` から算出し、`plot_*` で正規化(layout 本体と同じ式を使い乖離を避ける)。
- scatter/line の点も同様に正規化座標で。

### Task 11: geometry をモデルに追加(chart.js 側)+ 差分

- `extract.mjs` で `chart.getDatasetMeta(i).data[j]` の `{x, y, base, width}` を `chart.chartArea` 基準で正規化。
- `diff.mjs` に geometry 次元を追加: 正規化座標 `|Δ| <= TOLERANCES.geometryNorm` + 構造(要素数・左→右順序・bar 高さ単調性)。
- HTML レポートに geometry 差分の可視化を追加。

---

## 完了基準(Phase 1)

- `cargo test`(workspace 全体)が緑。`inspect` の insta スナップショットが妥当な値で固定。
- `cd tools && npm run compat` が Phase 1 対象 spec 群で動き、各 spec の次元別 PASS/FAIL と目視 HTML を生成。
- 色を意図的に壊すと cross-check が FAIL する(手動確認)。

## 既知の制約 / フォローアップ候補

- chart.js コア非対応型(matrix, datalabels, theme)と vegalite-* は対象外。matrix/datalabels はプラグイン導入で将来対応可。
- horizontal bar は linear/category 軸の割当が入れ替わる。`kind` ベース突合で吸収するが、要動作確認。
- geometry の厳密照合は Phase 2。
