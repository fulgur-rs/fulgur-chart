# Matrix (Heatmap) Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `type: "matrix"` の JSON 入力を受け取り、2D グリッド各セルを値に応じた色（白→指定色のグラデーション）で塗るヒートマップ SVG を生成する。

**Architecture:** `ChartKind::Matrix { color_lo, color_hi }` を IR に追加。既存 `ChartSpec` の `categories`（x列ラベル）と `series[i]`（y行 i、name=y ラベル、values=行の値）を流用してデータを格納。`layout/matrix.rs` で各セルを `Prim::Rect` に変換する。

**Tech Stack:** Rust, serde/serde_json, insta (スナップショットテスト)

---

### Task 1: IR に `ChartKind::Matrix` を追加し stub レイアウトを登録

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`
- Create: `crates/fulgur-chart/src/layout/matrix.rs`

**Step 1: `ir.rs` の `ChartKind` enum に Matrix バリアントを追加**

`crates/fulgur-chart/src/ir.rs` の `ChartKind` enum（行 91–99）に追記:

```rust
    Mixed, // 共有カテゴリ x・線形 y に bar+line を重ねる。種別は Series.series_type
    Matrix {
        color_lo: Color, // min 値のセル色（白固定）
        color_hi: Color, // max 値のセル色（backgroundColor 由来）
    },
```

**Step 2: stub レイアウトファイルを作成**

`crates/fulgur-chart/src/layout/matrix.rs` を新規作成:

```rust
use crate::ir::{ChartKind, ChartSpec, Color};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use super::common::{OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT, X_LABEL_BAND, X_LABEL_CENTER_RATIO};

const NAN_COLOR: Color = Color { r: 224, g: 224, b: 224, a: 1.0 };

fn lerp_color(lo: Color, hi: Color, t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: (lo.r as f64 + (hi.r as f64 - lo.r as f64) * t).round() as u8,
        g: (lo.g as f64 + (hi.g as f64 - lo.g as f64) * t).round() as u8,
        b: (lo.b as f64 + (hi.b as f64 - lo.b as f64) * t).round() as u8,
        a: lo.a + (hi.a - lo.a) * t,
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let (color_lo, color_hi) = match spec.kind {
        ChartKind::Matrix { color_lo, color_hi } => (color_lo, color_hi),
        _ => unreachable!(),
    };

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let n_rows = spec.series.len();
    let n_cols = spec.categories.len();

    // y 軸ラベル最大幅
    let mut max_y_w = 0.0_f32;
    for s in &spec.series {
        let w = m.width(&s.name, label_font as f32);
        if w > max_y_w { max_y_w = w; }
    }
    let y_axis_w = max_y_w as f64 + 10.0;

    let title_band = if spec.title.is_some() { TITLE_BAND } else { 0.0 };

    let plot_left   = OUTER_PAD + y_axis_w;
    let plot_right  = spec.width - OUTER_PAD;
    let plot_top    = OUTER_PAD + title_band;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND;

    let plot_w = plot_right - plot_left;
    let plot_h = plot_bottom - plot_top;

    let cell_w = if n_cols > 0 { plot_w / n_cols as f64 } else { plot_w };
    let cell_h = if n_rows > 0 { plot_h / n_rows as f64 } else { plot_h };

    // min/max 収集（NaN スキップ）
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for s in &spec.series {
        for &v in &s.values {
            if v.is_finite() {
                if v < min_v { min_v = v; }
                if v > max_v { max_v = v; }
            }
        }
    }
    let range = if (max_v - min_v).abs() < f64::EPSILON { 1.0 } else { max_v - min_v };

    let mut items: Vec<Prim> = Vec::new();

    // タイトル
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: OUTER_PAD + TITLE_FONT,
            size: TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
        });
    }

    // セル
    for (row, s) in spec.series.iter().enumerate() {
        let cell_y = plot_top + row as f64 * cell_h;
        for (col, &v) in s.values.iter().enumerate() {
            let cell_x = plot_left + col as f64 * cell_w;
            let fill = if v.is_finite() {
                lerp_color(color_lo, color_hi, (v - min_v) / range)
            } else {
                NAN_COLOR
            };
            items.push(Prim::Rect { x: cell_x, y: cell_y, w: cell_w, h: cell_h, fill });
        }
    }

    // x 軸ラベル（各列中央下）
    for (col, label) in spec.categories.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left + col as f64 * cell_w + cell_w / 2.0,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: label_font,
            anchor: Anchor::Middle,
            fill: ink,
            content: label.clone(),
        });
    }

    // y 軸ラベル（各行中央左、右寄せ）
    for (row, s) in spec.series.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left - 6.0,
            y: plot_top + row as f64 * cell_h + cell_h / 2.0 + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: s.name.clone(),
        });
    }

    Scene { width: spec.width, height: spec.height, items }
}
```

**Step 3: `layout/mod.rs` に `matrix` モジュールと Matrix アームを追加**

`crates/fulgur-chart/src/layout/mod.rs`:
- `pub mod matrix;` を追加（`pub mod radar;` の次の行）
- `build_scene` の `match` に `ChartKind::Matrix { .. } => matrix::build(spec, m),` を追加

```rust
pub mod bar;
pub mod common;
pub mod line;
pub mod matrix;   // ← 追加
pub mod mixed;
pub mod pie;
pub mod radar;
pub mod scatter;
```

```rust
    let mut scene = match spec.kind {
        ChartKind::Bar { .. } => bar::build(spec, m),
        ChartKind::Line => line::build(spec, m),
        ChartKind::Pie { .. } => pie::build(spec, m),
        ChartKind::Scatter | ChartKind::Bubble => scatter::build(spec, m),
        ChartKind::Radar => radar::build(spec, m),
        ChartKind::Mixed => mixed::build(spec, m),
        ChartKind::Matrix { .. } => matrix::build(spec, m),  // ← 追加
    };
```

**Step 4: ビルドが通ることを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo build 2>&1
```

Expected: コンパイルエラーなし

**Step 5: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/layout/mod.rs crates/fulgur-chart/src/layout/matrix.rs
git commit -m "feat(ir): add ChartKind::Matrix and layout/matrix stub"
```

---

### Task 2: スキーマ型を追加（`schema/chartjs.rs`）

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`

**Step 1: 失敗するスキーマテストを書く**

`crates/fulgur-chart/tests/frontend_chartjs.rs` の末尾に追加:

```rust
#[test]
fn matrix_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r#"{
        "type": "matrix",
        "data": {
            "datasets": [{
                "label": "Heat",
                "data": [{"x": "Mon", "y": "AM", "v": 5.0}],
                "backgroundColor": "#36a2eb"
            }]
        }
    }"#;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::Matrix(_)));
}
```

**Step 2: テストが失敗することを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test matrix_schema_roundtrip 2>&1
```

Expected: FAIL（`ChartJsSpec::Matrix` が存在しない）

**Step 3: スキーマ型を追加**

`crates/fulgur-chart/src/schema/chartjs.rs` の `ChartJsSpec` enum の末尾（`Radar(RadarSpec)` の後）に追加:

```rust
    Matrix(MatrixSpec),
```

同ファイルの末尾に以下の型を追加:

```rust
// ────────────────────────────────────────────────
// Matrix chart
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixSpec {
    pub data: MatrixData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<MatrixOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixData {
    pub datasets: Vec<MatrixDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MatrixDataset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<MatrixPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixPoint {
    pub x: String,
    pub y: String,
    pub v: f64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MatrixOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}
```

`CommonPlugins` は既存の `BarPlugins` / `LinePlugins` と重複する。`schema/common.rs` に `CommonPlugins` が既存かどうか確認し、なければ `schema/chartjs.rs` 内の既存 `BarPlugins` を参照して同等のものを追加するか、`BarPlugins` を `MatrixOptions.plugins` の型として再利用する。実際のファイルを読んで判断すること。

**Step 4: テストが通ることを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test matrix_schema_roundtrip 2>&1
```

Expected: PASS

**Step 5: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
git add crates/fulgur-chart/src/schema/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat(schema): add MatrixSpec types to chartjs schema"
```

---

### Task 3: `frontend/chartjs.rs` に matrix パーサを追加

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
- Modify: `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Step 1: 失敗するパーステストを書く**

`crates/fulgur-chart/tests/frontend_chartjs.rs` の末尾に追加:

```rust
#[test]
fn matrix_parses_categories_and_series() {
    let json = r#"{
        "type": "matrix",
        "data": {"datasets": [{"label": "h", "data": [
            {"x": "Mon", "y": "Morning", "v": 5.0},
            {"x": "Tue", "y": "Morning", "v": 8.0},
            {"x": "Mon", "y": "Evening", "v": 3.0},
            {"x": "Tue", "y": "Evening", "v": 9.0}
        ], "backgroundColor": "rgba(54,162,235,1.0)"}]}
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Matrix { .. }));
    // x カテゴリ: 出現順
    assert_eq!(spec.categories, vec!["Mon", "Tue"]);
    // y 系列: 出現順 (Morning が先)
    assert_eq!(spec.series.len(), 2);
    assert_eq!(spec.series[0].name, "Morning");
    assert_eq!(spec.series[0].values, vec![5.0, 8.0]);
    assert_eq!(spec.series[1].name, "Evening");
    assert_eq!(spec.series[1].values, vec![3.0, 9.0]);
}

#[test]
fn matrix_multiple_datasets_is_error() {
    let json = r#"{"type":"matrix","data":{"datasets":[
        {"data":[{"x":"A","y":"X","v":1}]},
        {"data":[{"x":"A","y":"X","v":2}]}
    ]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn matrix_missing_cell_becomes_nan() {
    // 2x2 グリッドで (Tue, Evening) が欠損
    let json = r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"Mon","y":"Morning","v":1.0},
        {"x":"Tue","y":"Morning","v":2.0},
        {"x":"Mon","y":"Evening","v":3.0}
    ]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(spec.series[1].values[1].is_nan()); // (Evening, Tue) は NaN
}
```

**Step 2: テストが失敗することを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test matrix_parses 2>&1 | tail -20
```

Expected: FAIL

**Step 3: `DataField` に `MatrixPoints` バリアントを追加**

`crates/fulgur-chart/src/frontend/chartjs.rs` の `DataField` enum に追加（`Points` の後）:

```rust
#[derive(Deserialize)]
#[serde(untagged)]
enum DataField {
    Nums(Vec<f64>),
    Points(Vec<RawPoint>),
    MatrixPoints(Vec<RawMatrixPoint>), // ← 追加
}
```

`RawPoint` の定義の後に `RawMatrixPoint` を追加:

```rust
#[derive(Deserialize, Clone)]
struct RawMatrixPoint {
    x: String,
    y: String,
    v: f64,
}
```

**Step 4: `parse` 関数に matrix 早期分岐を追加**

`crates/fulgur-chart/src/frontend/chartjs.rs` の `pub fn parse` 関数の先頭（`if strict { ... }` の直後、`let raw: RawSpec = ...` の前）に追加:

```rust
    // matrix は専用パスで処理する（data 形式が {x,y,v} で他と異なるため）
    {
        let peeked: Result<serde_json::Value, _> = serde_json::from_str(json);
        if peeked.as_ref().ok().and_then(|v| v.get("type")).and_then(|t| t.as_str()) == Some("matrix") {
            return parse_matrix(json);
        }
    }
```

※ `strict` モードのキーチェックは matrix には適用しない（`deny_unknown_fields` はスキーマ層で担保）。

**Step 5: `parse_matrix` 関数を追加**

`crates/fulgur-chart/src/frontend/chartjs.rs` の末尾（`fn check_unknown_keys` の前）に追加:

```rust
#[derive(Deserialize)]
struct RawMatrixSpec {
    data: RawMatrixData,
    #[serde(default)]
    options: RawOptions,
}

#[derive(Deserialize)]
struct RawMatrixData {
    datasets: Vec<RawMatrixDataset>,
}

#[derive(Deserialize)]
struct RawMatrixDataset {
    #[serde(default)]
    label: String,
    data: Vec<RawMatrixCell>,
    #[serde(rename = "backgroundColor", default)]
    background_color: Option<String>,
    #[serde(rename = "borderWidth", default)]
    border_width: Option<f64>,
}

#[derive(Deserialize)]
struct RawMatrixCell {
    x: String,
    y: String,
    v: f64,
}

fn parse_matrix(json: &str) -> Result<ChartSpec, String> {
    #[derive(Deserialize)]
    struct Wrapper {
        data: RawMatrixData,
        #[serde(default)]
        options: RawOptions,
    }
    let raw: Wrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;

    if raw.data.datasets.len() > 1 {
        return Err("matrix チャートは dataset が 1 つのみサポートされます".to_string());
    }
    if raw.data.datasets.is_empty() {
        return Err("matrix チャートには dataset が 1 つ必要です".to_string());
    }

    let ds = raw.data.datasets.into_iter().next().unwrap();

    // x/y カテゴリを出現順に収集（重複除去）
    let mut x_cats: Vec<String> = vec![];
    let mut y_cats: Vec<String> = vec![];
    for cell in &ds.data {
        if !x_cats.contains(&cell.x) { x_cats.push(cell.x.clone()); }
        if !y_cats.contains(&cell.y) { y_cats.push(cell.y.clone()); }
    }

    let n_cols = x_cats.len();
    let n_rows = y_cats.len();

    // NaN で初期化したグリッドを構築
    let mut grid: Vec<Vec<f64>> = vec![vec![f64::NAN; n_cols]; n_rows];
    for cell in &ds.data {
        let xi = x_cats.iter().position(|c| c == &cell.x).unwrap();
        let yi = y_cats.iter().position(|r| r == &cell.y).unwrap();
        grid[yi][xi] = cell.v;
    }

    let theme = build_theme(raw.options.theme);

    // max 色: backgroundColor → パレット[0]
    let color_hi = ds
        .background_color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(theme.palette[0]);
    let color_lo = Color { r: 255, g: 255, b: 255, a: 1.0 };

    // 系列: rows（y カテゴリ）
    let series: Vec<Series> = y_cats
        .iter()
        .enumerate()
        .map(|(i, name)| Series {
            name: name.clone(),
            values: grid[i].clone(),
            points: vec![],
            fill: vec![color_hi],
            stroke: vec![],
            stroke_width: ds.border_width.unwrap_or(0.0),
            area: false,
            tension: 0.0,
            series_type: SeriesType::Bar,
            point_radius: None,
        })
        .collect();

    Ok(ChartSpec {
        kind: ChartKind::Matrix { color_lo, color_hi },
        series,
        categories: x_cats,
        x_axis: AxisSpec { title: None, min: None, max: None, begin_at_zero: false, grid: false },
        y_axis: AxisSpec { title: None, min: None, max: None, begin_at_zero: false, grid: false },
        legend: legend_pos(&raw.options.plugins.legend),
        title: raw.options.plugins.title.filter(|t| t.display).map(|t| t.text),
        width: 800.0,
        height: 450.0,
        data_labels: false,
        theme,
    })
}
```

**Step 6: テストが通ることを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test matrix_ 2>&1
```

Expected: matrix_parses_categories_and_series, matrix_multiple_datasets_is_error, matrix_missing_cell_becomes_nan がすべて PASS

**Step 7: 全テストが通ることを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test 2>&1 | tail -10
```

Expected: 0 failed

**Step 8: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
git add crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat(frontend): parse matrix chart type from chartjs JSON"
```

---

### Task 4: レンダーテストを書き、レイアウトを検証

**Files:**
- Create: `crates/fulgur-chart/tests/render_matrix.rs`

**Step 1: テストファイルを作成**

`crates/fulgur-chart/tests/render_matrix.rs`:

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn matrix_renders_correct_rect_count() {
    // 2 列 × 2 行 = 4 セル
    let svg = render(r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1},{"x":"B","y":"X","v":2},
        {"x":"A","y":"Y","v":3},{"x":"B","y":"Y","v":4}
    ]}]}}"#);
    let rect_count = svg.matches("<rect").count();
    assert_eq!(rect_count, 4, "2x2 matrix should have 4 rects, got: {rect_count}\n{svg}");
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}

#[test]
fn matrix_nan_cell_uses_nan_color() {
    // (B, Y) が欠損 → NaN セルは #e0e0e0
    let svg = render(r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1},{"x":"B","y":"X","v":2},
        {"x":"A","y":"Y","v":3}
    ]}]}}"#);
    assert_eq!(svg.matches("<rect").count(), 4, "4 rects including NaN cell: {svg}");
    assert!(svg.contains("#e0e0e0"), "NaN cell should use #e0e0e0: {svg}");
}

#[test]
fn matrix_min_cell_is_white() {
    // min 値(0.0)のセルは白(#ffffff)
    let svg = render(r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":0},{"x":"B","y":"X","v":10}
    ],"backgroundColor":"#0000ff"}]}}"#);
    assert!(svg.contains("#ffffff"), "min cell should be white: {svg}");
}

#[test]
fn matrix_max_cell_matches_background_color() {
    // max 値のセルは backgroundColor (#0000ff)
    let svg = render(r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":0},{"x":"B","y":"X","v":10}
    ],"backgroundColor":"#0000ff"}]}}"#);
    assert!(svg.contains("#0000ff"), "max cell should match backgroundColor: {svg}");
}

#[test]
fn matrix_renders_axis_labels() {
    let svg = render(r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"Mon","y":"Morning","v":5},{"x":"Tue","y":"Morning","v":8}
    ]}]}}"#);
    assert!(svg.contains(">Mon<"), "x label Mon missing: {svg}");
    assert!(svg.contains(">Tue<"), "x label Tue missing: {svg}");
    assert!(svg.contains(">Morning<"), "y label Morning missing: {svg}");
}

#[test]
fn matrix_renders_title() {
    let svg = render(r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"A","y":"X","v":1}
    ]}]},"options":{"plugins":{"title":{"display":true,"text":"Weekly Heatmap"}}}}"#);
    assert!(svg.contains("Weekly Heatmap"), "title missing: {svg}");
}

#[test]
fn matrix_deterministic() {
    let j = r#"{"type":"matrix","data":{"datasets":[{"data":[
        {"x":"Mon","y":"Morning","v":5},{"x":"Tue","y":"Morning","v":8},
        {"x":"Mon","y":"Evening","v":3},{"x":"Tue","y":"Evening","v":9}
    ],"backgroundColor":"rgba(54,162,235,1.0)"}]}}"#;
    assert_eq!(render(j), render(j));
}

#[test]
fn matrix_snapshot() {
    let svg = render(r#"{"type":"matrix","data":{"datasets":[{"label":"Sales","data":[
        {"x":"Mon","y":"Morning","v":5},{"x":"Tue","y":"Morning","v":8},{"x":"Wed","y":"Morning","v":3},
        {"x":"Mon","y":"Evening","v":9},{"x":"Tue","y":"Evening","v":2},{"x":"Wed","y":"Evening","v":7}
    ],"backgroundColor":"rgba(54,162,235,1.0)"}]},"options":{"plugins":{"title":{"display":true,"text":"Weekly Heatmap"}}}}"#);
    insta::assert_snapshot!(svg);
}
```

**Step 2: テストを実行してスナップショットを生成**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test -p fulgur-chart --test render_matrix 2>&1
```

Expected: `matrix_snapshot` が `snapshot not found` で失敗、他は PASS  
スナップショット未生成エラーが出たら:

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo insta accept 2>&1 || INSTA_UPDATE=always cargo test -p fulgur-chart --test render_matrix matrix_snapshot 2>&1
```

**Step 3: 全テストが通ることを確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test 2>&1 | tail -10
```

Expected: 0 failed

**Step 4: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
git add crates/fulgur-chart/tests/render_matrix.rs crates/fulgur-chart/src/layout/matrix.rs
git add crates/fulgur-chart/src/snapshots/ 2>/dev/null || true
git add -A "*.snap" 2>/dev/null || git add crates/fulgur-chart/tests/snapshots/ 2>/dev/null || true
git commit -m "test(matrix): add render tests and snapshot"
```

---

### Task 5: サンプル JSON を追加してギャラリーに登録

**Files:**
- Create: `examples/specs/matrix.json`

**Step 1: サンプル JSON を作成**

`examples/specs/matrix.json`:

```json
{
  "type": "matrix",
  "data": {
    "datasets": [{
      "label": "Weekly Activity",
      "data": [
        {"x": "Mon", "y": "Morning",   "v": 5},
        {"x": "Tue", "y": "Morning",   "v": 8},
        {"x": "Wed", "y": "Morning",   "v": 3},
        {"x": "Thu", "y": "Morning",   "v": 7},
        {"x": "Fri", "y": "Morning",   "v": 4},
        {"x": "Mon", "y": "Afternoon", "v": 9},
        {"x": "Tue", "y": "Afternoon", "v": 2},
        {"x": "Wed", "y": "Afternoon", "v": 6},
        {"x": "Thu", "y": "Afternoon", "v": 8},
        {"x": "Fri", "y": "Afternoon", "v": 5},
        {"x": "Mon", "y": "Evening",   "v": 3},
        {"x": "Tue", "y": "Evening",   "v": 7},
        {"x": "Wed", "y": "Evening",   "v": 9},
        {"x": "Thu", "y": "Evening",   "v": 4},
        {"x": "Fri", "y": "Evening",   "v": 6}
      ],
      "backgroundColor": "rgba(54, 162, 235, 1.0)"
    }]
  },
  "options": {
    "plugins": {
      "title": {
        "display": true,
        "text": "Weekly Activity Heatmap"
      }
    }
  }
}
```

**Step 2: CLI でレンダリングして目視確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo run -p fulgur-chart-cli -- --input examples/specs/matrix.json --output /tmp/matrix.svg 2>&1
```

Expected: `/tmp/matrix.svg` が生成される

**Step 3: 全テスト最終確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
cargo test 2>&1 | tail -15
```

Expected: 0 failed

**Step 4: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat-matrix
git add examples/specs/matrix.json
git commit -m "chore: add matrix heatmap example to gallery"
```
