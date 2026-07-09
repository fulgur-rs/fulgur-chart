# Vega-Lite `rect` mark 実装プラン

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Vega-Lite の `mark: "rect"` を受理し、`x`/`y`/`color` encoding でヒートマップを描画する。x/y は nominal/ordinal、color は quantitative(2 色補間)/nominal(パレット割当)を扱い、`aggregate: "mean"|"sum"` を支援する。

**Architecture:** 新 IR variant `ChartKind::VegaRect { x_labels, y_labels, cells: Vec<Vec<Option<Color>>> }` を追加し、scale 解決を frontend (`frontend/vegalite.rs`) で完結させる。layout は純粋な grid renderer (`layout/vega_rect.rs`、既存 `matrix.rs` を参考)。JSON Schema エクスポートには `VlRectSpec` を追加。ChartKind::Matrix と Chart.js matrix chart のパスは無変更。

**Tech Stack:** Rust, serde, schemars, insta (snapshot), cargo test / clippy / fmt。

**Design source of truth:** beads issue `fulgur-chart-05j`(`bd show fulgur-chart-05j`)。

---

## Baseline

- 作業 worktree: `/home/ubuntu/fulgur-chart/.worktrees/vl-rect-mark`
- ブランチ: `feat/vl-rect-mark` (base: main @ 8105c98)
- Pre-check: `cargo test -p fulgur-chart --tests --lib` が全 pass 状態
- 参考実装: `docs/plans/2026-07-09-vl-circle-mark.md` (circle mark 追加 PR #124)、`layout/matrix.rs`、`tests/render_matrix.rs`

---

## Task 1: IR に `ChartKind::VegaRect` variant を追加し、全 exhaustive match を通す(コンパイル土台)

**注意:** `ChartKind` は `#[non_exhaustive]` ではないので、既存の `_ =>` 無し match は全て新 variant で壊れる。実装前に:

```bash
grep -rn "match .*spec\.kind\|match kind\|fn chart_type_name" crates/fulgur-chart/src/
```

を実行して exhaustive match の全出現箇所を確認する。事前調査で以下 2 箇所が exhaustive と判明済み(残りは `_ =>` catch-all または `matches!`/`if let` 単発):
- `crates/fulgur-chart/src/layout/mod.rs:27-45` (`build_scene`)
- `crates/fulgur-chart/src/model.rs:241-267` (`chart_type_name`)

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs:194-290` (`ChartKind` enum)
- Modify: `crates/fulgur-chart/src/layout/mod.rs:22-45` (`build_scene` の match)
- Modify: `crates/fulgur-chart/src/model.rs:241-267` (`chart_type_name` の match)
- Create: `crates/fulgur-chart/src/layout/vega_rect.rs`

**Step 1: 失敗テストを追加**

`crates/fulgur-chart/tests/frontend_vegalite.rs` 末尾に追加:

```rust
#[test]
fn rect_ir_variant_exists() {
    use fulgur_chart::ir::{ChartKind, Color};
    let kind = ChartKind::VegaRect {
        x_labels: vec!["A".to_string(), "B".to_string()],
        y_labels: vec!["X".to_string()],
        cells: vec![vec![
            Some(Color { r: 10, g: 20, b: 30, a: 1.0 }),
            None,
        ]],
    };
    assert!(matches!(kind, ChartKind::VegaRect { .. }));
}
```

**Step 2: 失敗を確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/vl-rect-mark
cargo test -p fulgur-chart --test frontend_vegalite rect_ir_variant_exists 2>&1 | tail -20
```
Expected: `no variant or associated item named ``VegaRect`` found` でコンパイル失敗。

**Step 3: `ChartKind::VegaRect` variant を追加**

`crates/fulgur-chart/src/ir.rs:214`(`Matrix { ... }` の直後)に追加:

```rust
    /// Vega-Lite `mark: "rect"` (ヒートマップ)。
    /// scale 解決済み色を per-cell で持つ純粋 grid。`None` セルは描画スキップ(透過)。
    /// x_labels/y_labels は categories/series 経由ではなくここに直接持ち、layout 側は
    /// この variant の情報だけで描画する(既存 ChartKind::Matrix パスを触らないため)。
    VegaRect {
        /// 列ラベル(横軸カテゴリ)、first-seen 順。
        x_labels: Vec<String>,
        /// 行ラベル(縦軸カテゴリ)、first-seen 順。
        y_labels: Vec<String>,
        /// cells[row][col] = 解決済み Color または None(欠損/skip)。
        /// row: y_labels の index、col: x_labels の index。
        cells: Vec<Vec<Option<Color>>>,
    },
```

**Step 4: layout dispatch と stub モジュールを追加**

`crates/fulgur-chart/src/layout/mod.rs:9`(`pub mod matrix;` の次行)に追加:

```rust
pub mod vega_rect;
```

`crates/fulgur-chart/src/layout/mod.rs:36`(`ChartKind::Matrix { .. } => matrix::build(spec, m),` の直後)に追加:

```rust
        ChartKind::VegaRect { .. } => vega_rect::build(spec, m),
```

`crates/fulgur-chart/src/model.rs:254`(`ChartKind::Matrix { .. } => "matrix",` の直後)に追加:

```rust
        ChartKind::VegaRect { .. } => "vegaRect",
```

`crates/fulgur-chart/src/layout/vega_rect.rs` を新規作成(stub、後続タスクで実装):

```rust
//! Vega-Lite `mark: "rect"` (ヒートマップ) のレイアウト。
//! Task 5 でセル・軸ラベル・タイトルを描画する。現状は最小 Scene を返す stub。

use crate::ir::ChartSpec;
use crate::scene::Scene;
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    Scene {
        width: spec.width,
        height: spec.height,
        items: vec![],
    }
}
```

**Step 5: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_ir_variant_exists 2>&1 | tail -5
```
Expected: 1 test pass。

**Step 6: 既存テスト全体の回帰なしを確認**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail
```
Expected: 全 pass。

**Step 7: commit**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/layout/mod.rs crates/fulgur-chart/src/layout/vega_rect.rs crates/fulgur-chart/src/model.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
feat(ir): add ChartKind::VegaRect variant for Vega-Lite rect mark

Vega-Lite の mark: "rect" (ヒートマップ) を受ける新 variant を IR に追加する。
scale 解決済み Color を per-cell (Option<Color>) で持ち、layout は純粋な
grid renderer になる。既存の ChartKind::Matrix と Chart.js matrix chart
のパスは無変更。layout モジュールは stub。frontend と描画は後続タスクで
実装する。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 2: JSON Schema に `Rect` variant を追加(型のみ、frontend より先)

**Files:**
- Modify: `crates/fulgur-chart/src/schema/vegalite.rs`

**Step 1: `MarkRectName` / `MarkRectObject` / `MarkRect` を追加**

`crates/fulgur-chart/src/schema/vegalite.rs:156`(`MarkArc` の直後、Bar chart セクション区切りの前)に追加:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkRectName {
    Rect,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkRectObject {
    #[serde(rename = "type")]
    pub mark_type: MarkRectName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkRect {
    String(MarkRectName),
    Object(MarkRectObject),
}
```

**Step 2: `VegaLiteSpec::Rect(VlRectSpec)` variant を追加**

`crates/fulgur-chart/src/schema/vegalite.rs:15`(`Arc(VlArcSpec),` の次行)に追加:

```rust
    Rect(VlRectSpec),
```

**Step 3: `VlRectSpec` / `VlRectEncoding` を追加**

`crates/fulgur-chart/src/schema/vegalite.rs:301`(ファイル末尾、Arc セクションの後)に追加:

```rust
// ────────────────────────────────────────────────
// Rect / heatmap chart (mark: "rect")
//
// x/y はカテゴリ、color は quantitative(2色補間)または nominal(パレット割当)。
// encoding.color.aggregate として "mean" / "sum" を受理する(schema 上は文字列)。
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectSpec {
    pub mark: MarkRect,
    pub data: VlData,
    pub encoding: VlRectEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectEncoding {
    pub x: VlChannel,
    pub y: VlChannel,
    pub color: VlRectColorChannel,
}

/// rect の color チャネル。基本の `field`/`type` に加え、`aggregate` を許容する。
/// `aggregate` は "mean" | "sum" のみ frontend で受理される(他値は strict で Err)。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlRectColorChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aggregate: Option<String>,
}
```

**Step 4: ビルドとテスト回帰なしを確認**

```bash
cargo build -p fulgur-chart 2>&1 | tail -5
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail
```
Expected: build ok, 全 pass。

**Step 5: JSON Schema エクスポートに Rect が現れることを確認**

```bash
cargo run -p fulgur-chart-cli -- schema vegalite 2>&1 | grep -c '"MarkRect"\|"VlRectSpec"\|"VlRectEncoding"'
```
Expected: `3` 以上。

**Step 6: commit**

```bash
git add crates/fulgur-chart/src/schema/vegalite.rs
git commit -m "$(cat <<'EOF'
feat(schema): add Rect variant to Vega-Lite JSON Schema

VegaLiteSpec::Rect(VlRectSpec) を追加。encoding は x/y/color の 3 チャネル
を必須とし、color チャネルは aggregate("mean"|"sum") を許容する型
(VlRectColorChannel) を専用に持つ。frontend パースと描画は後続タスク。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 3: frontend パーサ — 最小 quantitative ケースを受理(TDD)

**目的:** 数値 color + string x/y + aggregate なし + 完全格子(欠損なし)の最小 spec を parse し `ChartKind::VegaRect` を返す。scale は quantitative の 2 色補間。パレットは既存 matrix 定数を再利用。

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs:191-213` (`parse_mark`)
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs` (helpers 追加)
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Step 1: 失敗テストを追加**

`crates/fulgur-chart/tests/frontend_vegalite.rs` 末尾に追加:

```rust
#[test]
fn rect_mark_quantitative_maps_to_vegarect() {
    // 2x2 grid, quantitative color.
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"day":"Mon","hour":"AM","v":1},
            {"day":"Tue","hour":"AM","v":3},
            {"day":"Mon","hour":"PM","v":5},
            {"day":"Tue","hour":"PM","v":7}
        ]},
        "encoding": {
            "x": {"field":"day","type":"nominal"},
            "y": {"field":"hour","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let (x_labels, y_labels, cells) = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { x_labels, y_labels, cells } => {
            (x_labels.clone(), y_labels.clone(), cells.clone())
        }
        _ => panic!("expected VegaRect, got {:?}", spec.kind),
    };
    // first-seen order
    assert_eq!(x_labels, vec!["Mon", "Tue"]);
    assert_eq!(y_labels, vec!["AM", "PM"]);
    // 2 rows x 2 cols
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].len(), 2);
    // min (v=1) at (Mon, AM) → color_lo (#ffffff white)
    let c00 = cells[0][0].expect("cell should not be None");
    assert_eq!((c00.r, c00.g, c00.b), (255, 255, 255), "min cell should be white");
    // max (v=7) at (Tue, PM) → color_hi (VL theme palette[0] = Tableau steel-blue #4c78a8 = (76, 120, 168))
    let c11 = cells[1][1].expect("cell should not be None");
    assert_eq!((c11.r, c11.g, c11.b), (76, 120, 168), "max cell should be Tableau steel-blue");
}

#[test]
fn rect_mark_object_form_accepted() {
    let json = r#"{
        "mark": {"type": "rect"},
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, fulgur_chart::ir::ChartKind::VegaRect { .. }));
}
```

**Step 2: 失敗を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_ 2>&1 | tail -20
```
Expected: 2 tests が `未対応の mark: rect` で fail。

**Step 3: `parse_mark` に rect 分岐を追加(sentinel を返す)**

現行 `parse_mark` は kind だけを返す設計だが、rect は encoding の解決結果まで含めて構築する必要がある。`parse_mark` は最小情報だけ返し、実際の cells は上位で組む方針で進める。まず sentinel の `ChartKind::VegaRect` を返す:

`crates/fulgur-chart/src/frontend/vegalite.rs:209`(`"circle" => Ok(ChartKind::Scatter),` の次行)に追加:

```rust
        "rect" => Ok(ChartKind::VegaRect {
            x_labels: Vec::new(),
            y_labels: Vec::new(),
            cells: Vec::new(),
        }),
```

**Step 4: `parse` 関数側で rect 用の cells 構築を組み込む**

`crates/fulgur-chart/src/frontend/vegalite.rs:45-83`(kind ごとの validation match)に rect 分岐を追加。`ChartKind::VegaRect { .. }` の場合は x/y をカテゴリ検証、color を(存在チェックのみ、この Step では型は問わない):

```rust
        ChartKind::VegaRect { .. } => {
            let xf = require_field(&x_field, "x")?;
            let yf = require_field(&y_field, "y")?;
            let cf = require_field(&color_field, "color")?;
            validate_category(&records, xf)?;
            validate_category(&records, yf)?;
            // color は数値または文字列のカテゴリ(quantitative/nominal を後段で判定)。
            // 存在だけ確認、型は build_rect_cells で扱う。
            for r in &records {
                if r.get(cf).is_none() {
                    return Err(format!("フィールド {cf} が見つかりません(typo?)"));
                }
            }
        }
```

**Step 5: rect 用の cells 構築ヘルパを追加**

`crates/fulgur-chart/src/frontend/vegalite.rs` の末尾(`check_object` の後)に追加:

```rust
/// rect ヒートマップ用の 2 色補間の endpoint。
/// HI は Vega-Lite テーマの palette[0] (Tableau10 steel-blue #4c78a8) と揃える。
/// nominal 経路も同じパレットを使うため、quantitative と nominal で色系統が一貫する。
/// (chart.js matrix の #36A2EB 定数は Chart.js DSL 経路 (`ChartKind::Matrix`) 側で
/// 独立に保持されており、Vega-Lite rect とは意図的に別テーマとする。)
const RECT_COLOR_LO: Color = Color { r: 255, g: 255, b: 255, a: 1.0 };
const RECT_COLOR_HI: Color = Color { r: 76, g: 120, b: 168, a: 1.0 };

fn lerp_rect_color(t: f64) -> Color {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    Color {
        r: (RECT_COLOR_LO.r as f64
            + (RECT_COLOR_HI.r as f64 - RECT_COLOR_LO.r as f64) * t)
            .round() as u8,
        g: (RECT_COLOR_LO.g as f64
            + (RECT_COLOR_HI.g as f64 - RECT_COLOR_LO.g as f64) * t)
            .round() as u8,
        b: (RECT_COLOR_LO.b as f64
            + (RECT_COLOR_HI.b as f64 - RECT_COLOR_LO.b as f64) * t)
            .round() as u8,
        a: RECT_COLOR_LO.a + (RECT_COLOR_HI.a - RECT_COLOR_LO.a) * t as f32,
    }
}

/// rect 用の cells / labels を構築する。
/// - x/y の distinct カテゴリを first-seen 順で採取
/// - 各セルの color 値 (この Step では quantitative のみ、集約なし)
/// - min/max の 2 色補間で Rgb 解決 → cells[row][col]
/// - 未出現の (x,y) は None(スキップ)
fn build_rect(
    records: &[Map<String, Value>],
    x_field: &str,
    y_field: &str,
    color_field: &str,
) -> (Vec<String>, Vec<String>, Vec<Vec<Option<Color>>>) {
    let x_labels = distinct_categories(records, Some(x_field));
    let y_labels = distinct_categories(records, Some(y_field));

    // 各セルの数値の生値を Vec に集める(この Step では最後の 1 件を採用)。
    // Task 5 で aggregate mean/sum を扱う。
    let mut cell_values: Vec<Vec<Option<f64>>> =
        vec![vec![None; x_labels.len()]; y_labels.len()];
    for r in records {
        let xk = field_category(r, Some(x_field));
        let yk = field_category(r, Some(y_field));
        let (Some(col), Some(row)) = (
            x_labels.iter().position(|l| l == &xk),
            y_labels.iter().position(|l| l == &yk),
        ) else {
            continue;
        };
        let v = r.get(color_field).and_then(Value::as_f64);
        cell_values[row][col] = v;
    }

    // min/max を有限値のみから算出。
    let (mut min_v, mut max_v) = (f64::INFINITY, f64::NEG_INFINITY);
    for row in &cell_values {
        for v in row.iter().flatten() {
            if v.is_finite() {
                if *v < min_v {
                    min_v = *v;
                }
                if *v > max_v {
                    max_v = *v;
                }
            }
        }
    }
    let range = if (max_v - min_v).abs() < f64::EPSILON {
        1.0
    } else {
        max_v - min_v
    };
    let degenerate = !min_v.is_finite() || (max_v - min_v).abs() < f64::EPSILON;

    let cells: Vec<Vec<Option<Color>>> = cell_values
        .iter()
        .map(|row| {
            row.iter()
                .map(|v| match v {
                    Some(v) if v.is_finite() => {
                        if degenerate {
                            Some(RECT_COLOR_HI)
                        } else {
                            Some(lerp_rect_color((*v - min_v) / range))
                        }
                    }
                    _ => None,
                })
                .collect()
        })
        .collect();

    (x_labels, y_labels, cells)
}
```

`Color` を frontend/vegalite.rs から使うため既存 `use crate::ir::*;` はそのままで OK。

**Step 6: `parse` 内で rect の場合に cells を組み立て、`kind` を実体で置き換える**

`crates/fulgur-chart/src/frontend/vegalite.rs:108-119`(series を組む match)の直前に、rect 用ブランチを分岐で先に処理する:

現行:
```rust
    let series = match &kind {
        ChartKind::Pie { .. } => build_pie(...),
        ChartKind::Scatter => build_scatter(...),
        _ => build_categorical(...),
    };
```

これを rect 対応版に差し替え。`kind` は mutable にする。`crates/fulgur-chart/src/frontend/vegalite.rs:31` を:

```rust
    let mut kind = parse_mark(top.get("mark"))?;
```

に変更(`let` → `let mut`)。

その後、series 組み立て match の直前(現行 108 行目付近)に追加:

```rust
    // rect ヒートマップの場合、kind に cells を差し替え、series/categories は空。
    // ここでは validation で確認済みだが、"パニックしない" invariant を守るため
    // require_field で Result 伝播する(実質 unreachable の Err)。
    if matches!(kind, ChartKind::VegaRect { .. }) {
        let xf = require_field(&x_field, "x")?;
        let yf = require_field(&y_field, "y")?;
        let cf = require_field(&color_field, "color")?;
        let (x_labels, y_labels, cells) = build_rect(&records, xf, yf, cf);
        kind = ChartKind::VegaRect { x_labels, y_labels, cells };
    }
```

そして `let series = match &kind {` の match 分岐に rect ケースを:

```rust
        ChartKind::VegaRect { .. } => vec![],
```

同様に `let categories = match &kind {` の分岐に:

```rust
        ChartKind::VegaRect { .. } => vec![],
```

も追加。`y_begin_at_zero` は rect も `false`(軸ゼロ起点は無関係):

```rust
    let y_begin_at_zero = !matches!(kind, ChartKind::Scatter | ChartKind::VegaRect { .. });
```

**Step 7: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_ 2>&1 | tail -10
```
Expected: 2 tests pass。

**Step 8: 既存テスト全体の回帰なしを確認**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail
```
Expected: 全 pass。

**Step 9: commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
feat(vegalite): parse mark: "rect" with quantitative color

x/y をカテゴリ、color を quantitative(2 色補間: 白 → chart.js blue)として
解決し、ChartKind::VegaRect に流し込む最小実装。first-seen 順の x_labels/
y_labels、未出現 (x,y) は None セル。aggregate 未指定(Task 5)、nominal
color 未対応(Task 4)、strict allow-list は Task 6 で追加。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 4: color 型 nominal (カテゴリパレット) 対応

**目的:** `encoding.color.type: "nominal"` または省略時に color 値が非数値なら nominal と判定し、パレットからラウンドロビン割当する。

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs` (`build_rect` を拡張)
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Step 1: 失敗テストを追加**

```rust
#[test]
fn rect_mark_nominal_color_uses_palette_roundrobin() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","c":"cat0"},
            {"x":"B","y":"X","c":"cat1"},
            {"x":"A","y":"Y","c":"cat0"}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"c","type":"nominal"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // Vega-Lite Tableau10 first color: #4c78a8 (76, 120, 168)
    // Vega-Lite Tableau10 second color: #f58518 (245, 133, 24)
    let cat0_color = cells[0][0].expect("cell (A,X) present"); // cat0 → palette[0]
    let cat1_color = cells[0][1].expect("cell (B,X) present"); // cat1 → palette[1]
    let cat0_color_again = cells[1][0].expect("cell (A,Y) present"); // cat0 → palette[0]
    assert_eq!((cat0_color.r, cat0_color.g, cat0_color.b), (76, 120, 168));
    assert_eq!((cat1_color.r, cat1_color.g, cat1_color.b), (245, 133, 24));
    assert_eq!(cat0_color, cat0_color_again, "same category → same color");
    // (B, Y) は未出現 → None
    assert!(cells[1][1].is_none(), "missing (B,Y) should be None");
}
```

**Step 2: 失敗を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_nominal_color 2>&1 | tail -10
```
Expected: fail(現在 quantitative 経路が数値でない値を 0 として扱い、cat0/cat1 が同じ色 lerp(0.0)=白 になる)。

**Step 3: `build_rect` を color 型判定付きに拡張**

`build_rect` シグネチャに `color_type: ColorType` を追加。ColorType は同ファイル内 enum:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
enum ColorType {
    Quantitative,
    Nominal,
}

/// encoding.color.type と実データから color 型を判定する。
/// - "quantitative" → Quantitative
/// - "nominal" / "ordinal" → Nominal
/// - 省略時 → 全レコードの color 値が数値なら Quantitative、それ以外は Nominal
fn infer_color_type(
    records: &[Map<String, Value>],
    color_field: &str,
    explicit: Option<&str>,
) -> ColorType {
    match explicit {
        Some("quantitative") => ColorType::Quantitative,
        Some("nominal" | "ordinal") => ColorType::Nominal,
        _ => {
            let all_numeric = records
                .iter()
                .all(|r| matches!(r.get(color_field), Some(Value::Number(_))));
            if all_numeric {
                ColorType::Quantitative
            } else {
                ColorType::Nominal
            }
        }
    }
}
```

`build_rect` を書き換え(quantitative は既存ロジック、nominal は新ロジック):

```rust
fn build_rect(
    records: &[Map<String, Value>],
    x_field: &str,
    y_field: &str,
    color_field: &str,
    color_type: ColorType,
    palette: &[Color],
) -> (Vec<String>, Vec<String>, Vec<Vec<Option<Color>>>) {
    let x_labels = distinct_categories(records, Some(x_field));
    let y_labels = distinct_categories(records, Some(y_field));

    match color_type {
        ColorType::Quantitative => {
            let mut cell_values: Vec<Vec<Option<f64>>> =
                vec![vec![None; x_labels.len()]; y_labels.len()];
            for r in records {
                let xk = field_category(r, Some(x_field));
                let yk = field_category(r, Some(y_field));
                let (Some(col), Some(row)) = (
                    x_labels.iter().position(|l| l == &xk),
                    y_labels.iter().position(|l| l == &yk),
                ) else {
                    continue;
                };
                let v = r.get(color_field).and_then(Value::as_f64);
                cell_values[row][col] = v;
            }
            let (mut min_v, mut max_v) = (f64::INFINITY, f64::NEG_INFINITY);
            for row in &cell_values {
                for v in row.iter().flatten() {
                    if v.is_finite() {
                        if *v < min_v { min_v = *v; }
                        if *v > max_v { max_v = *v; }
                    }
                }
            }
            let range = if (max_v - min_v).abs() < f64::EPSILON { 1.0 } else { max_v - min_v };
            let degenerate = !min_v.is_finite() || (max_v - min_v).abs() < f64::EPSILON;
            let cells = cell_values
                .iter()
                .map(|row| row.iter().map(|v| match v {
                    Some(v) if v.is_finite() => {
                        if degenerate { Some(RECT_COLOR_HI) } else { Some(lerp_rect_color((*v - min_v) / range)) }
                    }
                    _ => None,
                }).collect())
                .collect();
            (x_labels, y_labels, cells)
        }
        ColorType::Nominal => {
            // 色カテゴリの first-seen 順を採取。cat → palette index。
            let color_cats = distinct_categories(records, Some(color_field));
            let mut cells: Vec<Vec<Option<Color>>> =
                vec![vec![None; x_labels.len()]; y_labels.len()];
            for r in records {
                let xk = field_category(r, Some(x_field));
                let yk = field_category(r, Some(y_field));
                let ck = field_category(r, Some(color_field));
                let (Some(col), Some(row), Some(ci)) = (
                    x_labels.iter().position(|l| l == &xk),
                    y_labels.iter().position(|l| l == &yk),
                    color_cats.iter().position(|l| l == &ck),
                ) else {
                    continue;
                };
                cells[row][col] = Some(palette[ci % palette.len()]);
            }
            (x_labels, y_labels, cells)
        }
    }
}
```

**Step 4: 呼び出し側で color type を判定して渡す**

`crates/fulgur-chart/src/frontend/vegalite.rs` の parse 内 rect 分岐を書き換え:

```rust
    if matches!(kind, ChartKind::VegaRect { .. }) {
        let xf = require_field(&x_field, "x")?;
        let yf = require_field(&y_field, "y")?;
        let cf = require_field(&color_field, "color")?;
        // encoding.color.type を読み出す。VlChannel を経由せず生 JSON から取る。
        let color_type_hint = encoding
            .get("color")
            .and_then(Value::as_object)
            .and_then(|o| o.get("type"))
            .and_then(Value::as_str);
        let color_type = infer_color_type(&records, cf, color_type_hint);
        let (x_labels, y_labels, cells) = build_rect(&records, xf, yf, cf, color_type, &theme.palette);
        kind = ChartKind::VegaRect { x_labels, y_labels, cells };
    }
```

**Step 5: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark 2>&1 | tail -10
```
Expected: 3 tests pass(既存 quantitative 2 件 + 新 nominal 1 件)。

**Step 6: 回帰なしを確認**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail
```

**Step 7: commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
feat(vegalite): support nominal color for mark: "rect"

encoding.color.type が "nominal"/"ordinal"、または省略時に非数値なら
Vega-Lite テーマパレット(Tableau10)からラウンドロビン割当する。
quantitative は既存の 2 色補間。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 5: `aggregate: "mean"` / `"sum"` の対応

**目的:** 同一 (x, y) セルに複数レコードがある場合、`encoding.color.aggregate` を尊重して集約する。quantitative のみ対応、nominal + aggregate は Task 6 で strict Err にする(この Task では非 strict の許容範囲だけ定める)。

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Step 1: 失敗テストを追加**

```rust
#[test]
fn rect_mark_aggregate_mean() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":2},
            {"x":"A","y":"X","v":4},
            {"x":"B","y":"X","v":10}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"mean"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A, X) mean = (2 + 4) / 2 = 3.0 → min
    // (B, X) mean = 10 → max
    // range = 10 - 3 = 7, (A,X) t = 0.0 → white
    let ax = cells[0][0].expect("cell (A,X)");
    assert_eq!((ax.r, ax.g, ax.b), (255, 255, 255), "mean=3 → min → white");
    // (B, X) is at column index 1, row 0
    let bx = cells[0][1].expect("cell (B,X)");
    assert_eq!((bx.r, bx.g, bx.b), (76, 120, 168), "mean=10 → max → Tableau blue");
}

#[test]
fn rect_mark_aggregate_sum() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":2},
            {"x":"A","y":"X","v":4},
            {"x":"B","y":"X","v":10}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative","aggregate":"sum"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let cells = match &spec.kind {
        fulgur_chart::ir::ChartKind::VegaRect { cells, .. } => cells.clone(),
        _ => panic!("expected VegaRect"),
    };
    // (A, X) sum = 6, (B, X) sum = 10, range = 4
    // (A,X) t=0 → white, (B,X) t=1 → blue
    let ax = cells[0][0].expect("cell (A,X)");
    assert_eq!((ax.r, ax.g, ax.b), (255, 255, 255));
    let bx = cells[0][1].expect("cell (B,X)");
    assert_eq!((bx.r, bx.g, bx.b), (76, 120, 168));
}
```

**Step 2: 失敗を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_aggregate 2>&1 | tail -20
```
Expected: fail(現在は同一セルで「最後の 1 件を採用」→ mean=4, sum=4 が min として扱われる)。

**Step 3: `build_rect` の quantitative パスに aggregate 対応を実装**

Aggregate enum を追加:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
enum Aggregate {
    /// 集約なし。同一セル複数レコードは最後の 1 件で上書き(既存挙動)。
    None,
    Mean,
    Sum,
}
```

`build_rect` シグネチャに `aggregate: Aggregate` を追加。quantitative パスを:

```rust
        ColorType::Quantitative => {
            // 全レコードを (row, col) → Vec<f64> に集約用に蓄積
            let mut buckets: Vec<Vec<Vec<f64>>> =
                vec![vec![Vec::new(); x_labels.len()]; y_labels.len()];
            for r in records {
                let xk = field_category(r, Some(x_field));
                let yk = field_category(r, Some(y_field));
                let (Some(col), Some(row)) = (
                    x_labels.iter().position(|l| l == &xk),
                    y_labels.iter().position(|l| l == &yk),
                ) else { continue; };
                if let Some(v) = r.get(color_field).and_then(Value::as_f64) {
                    if v.is_finite() { buckets[row][col].push(v); }
                }
            }
            // aggregate 適用
            let cell_values: Vec<Vec<Option<f64>>> = buckets
                .iter()
                .map(|row| row.iter().map(|b| match (aggregate, b.as_slice()) {
                    (_, []) => None,
                    (Aggregate::Mean, vs) => Some(vs.iter().sum::<f64>() / vs.len() as f64),
                    (Aggregate::Sum, vs) => Some(vs.iter().sum()),
                    (Aggregate::None, vs) => Some(*vs.last().unwrap()),
                }).collect())
                .collect();
            // min/max + lerp(既存ロジック)
            let (mut min_v, mut max_v) = (f64::INFINITY, f64::NEG_INFINITY);
            for row in &cell_values {
                for v in row.iter().flatten() {
                    if v.is_finite() {
                        if *v < min_v { min_v = *v; }
                        if *v > max_v { max_v = *v; }
                    }
                }
            }
            let range = if (max_v - min_v).abs() < f64::EPSILON { 1.0 } else { max_v - min_v };
            let degenerate = !min_v.is_finite() || (max_v - min_v).abs() < f64::EPSILON;
            let cells = cell_values.iter().map(|row| row.iter().map(|v| match v {
                Some(v) if v.is_finite() => {
                    if degenerate { Some(RECT_COLOR_HI) } else { Some(lerp_rect_color((*v - min_v) / range)) }
                }
                _ => None,
            }).collect()).collect();
            (x_labels, y_labels, cells)
        }
```

**Step 4: 呼び出し側で aggregate を読み取って渡す**

`crates/fulgur-chart/src/frontend/vegalite.rs` parse 内 rect 分岐に:

```rust
        let aggregate_hint = encoding
            .get("color")
            .and_then(Value::as_object)
            .and_then(|o| o.get("aggregate"))
            .and_then(Value::as_str);
        let aggregate = match aggregate_hint {
            Some("mean") => Aggregate::Mean,
            Some("sum") => Aggregate::Sum,
            _ => Aggregate::None, // 非 strict では未対応値も無視(Aggregate::None 扱い)
        };
```

`build_rect(...)` 呼び出しに `aggregate` を渡す。

**Step 5: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark 2>&1 | tail -10
```
Expected: 5 tests pass。

**Step 6: 回帰なしを確認**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail
```

**Step 7: commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
feat(vegalite): support aggregate mean/sum for rect color channel

encoding.color.aggregate: "mean" | "sum" を quantitative color に適用する。
同一 (x,y) セルの複数レコードを集約してから scale 解決する。
"count"/"min"/"max"/"median" などは非 strict では無視(Aggregate::None)、
Task 6 で strict は Err にする。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 6: strict mode の allow-list に rect を追加 + Err ケースの pin テスト

**目的:** `check_unknown_keys` の mark 別 encoding allow-list に `"rect"` を追加。想定外の encoding や不正な `aggregate` / `quantitative x` などを strict で拒否する invariants をテストで固定する。

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs:486-522` (`check_unknown_keys`)
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Step 1: 失敗テストを追加**

```rust
#[test]
fn strict_rect_rejects_size_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "size": {"field":"v"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err(), "size should be rejected in strict");
    assert!(vegalite::parse(json, false).is_ok(), "size should be tolerated in non-strict");
}

#[test]
fn strict_rect_rejects_tooltip_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "tooltip": {"field":"v"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_x2_y2_encoding() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"}, "color": {"field":"v"},
            "x2": {"field":"x2"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err());
}

#[test]
fn strict_rect_rejects_quantitative_xy() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":1,"y":2,"v":3}]},
        "encoding": {
            "x": {"field":"x","type":"quantitative"},
            "y": {"field":"y"},
            "color": {"field":"v"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err(), "quantitative x should be rejected in strict");
    // 非 strict では文字列化して受理される(既存の緩さと同型)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_unsupported_aggregate() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","v":1}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"v","aggregate":"count"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err(), "aggregate=count should be rejected");
    // 非 strict では既存挙動(未対応値は無視)。
    assert!(vegalite::parse(json, false).is_ok());
}

#[test]
fn strict_rect_rejects_nominal_color_with_aggregate() {
    let json = r#"{
        "mark": "rect",
        "data": {"values": [{"x":"A","y":"X","c":"cat0"}]},
        "encoding": {
            "x": {"field":"x"}, "y": {"field":"y"},
            "color": {"field":"c","type":"nominal","aggregate":"sum"}
        }
    }"#;
    assert!(vegalite::parse(json, true).is_err(), "nominal + aggregate should be rejected in strict");
}
```

**Step 2: 失敗を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite strict_rect 2>&1 | tail -30
```
Expected: 全 fail(現在 strict allow-list に rect がないので non-strict と同じ挙動)。

**Step 3: `check_unknown_keys` の mark match に rect を追加**

`crates/fulgur-chart/src/frontend/vegalite.rs:506-510`:

```rust
        let allowed: &[&str] = match read_mark_name(top) {
            Some("bar" | "line" | "point" | "circle") => &["x", "y", "color"],
            Some("arc") => &["theta", "color", "x", "y"],
            _ => return Ok(()),
        };
```

を以下に置き換え:

```rust
        let allowed: &[&str] = match read_mark_name(top) {
            Some("bar" | "line" | "point" | "circle") => &["x", "y", "color"],
            Some("arc") => &["theta", "color", "x", "y"],
            Some("rect") => &["x", "y", "color"],
            _ => return Ok(()),
        };
```

さらに rect 限定の channel 検査を、その下の channel ループの後(`for channel in allowed {` の後、`Ok(())` の前)に追加:

```rust
        // rect 固有の strict チェック:
        // - x/y encoding の type: "quantitative" は binned ヒートマップ想定で MVP 外。
        // - color aggregate は "mean"/"sum" のみ受理。
        // - nominal color + aggregate は同時指定不可。
        if matches!(read_mark_name(top), Some("rect")) {
            for axis in ["x", "y"] {
                if let Some(ch) = encoding.get(axis).and_then(Value::as_object) {
                    if let Some(t) = ch.get("type").and_then(Value::as_str) {
                        if t == "quantitative" {
                            return Err(format!(
                                "rect の encoding.{axis}.type: \"quantitative\" は未対応です(binned ヒートマップは別 issue)"
                            ));
                        }
                    }
                }
            }
            if let Some(color) = encoding.get("color").and_then(Value::as_object) {
                let agg = color.get("aggregate").and_then(Value::as_str);
                if let Some(a) = agg {
                    if a != "mean" && a != "sum" {
                        return Err(format!(
                            "rect の encoding.color.aggregate: \"{a}\" は未対応です(mean/sum のみ)"
                        ));
                    }
                }
                let color_type = color.get("type").and_then(Value::as_str);
                if matches!(color_type, Some("nominal" | "ordinal")) && agg.is_some() {
                    return Err(
                        "rect の nominal color に aggregate は指定できません".to_string()
                    );
                }
            }
        }
```

さらに、rect の color チャネル allow-list に `aggregate` を含めるため、既存の channel 走査の直前で rect のときだけ allow を差し替える。`check_object(ch, &["field", "type"], ...)` の呼び出しを rect + color 時のみ `&["field", "type", "aggregate"]` にする:

```rust
        for channel in allowed {
            if let Some(ch) = encoding.get(*channel).and_then(Value::as_object) {
                let channel_allowed: &[&str] = if matches!(read_mark_name(top), Some("rect"))
                    && *channel == "color"
                {
                    &["field", "type", "aggregate"]
                } else {
                    &["field", "type"]
                };
                check_object(ch, channel_allowed, &format!("encoding.{channel}"))?;
            }
        }
```

**Step 4: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite strict_rect 2>&1 | tail -20
```
Expected: 6 tests pass。

**Step 5: 既存の strict テスト回帰なしを確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite 2>&1 | grep -E "test result" | tail
```
Expected: 全 pass。

**Step 6: commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
feat(vegalite): strict validation for mark: "rect"

strict allow-list に rect を追加し、rect 固有の禁止パターンを Err にする:
- x/y encoding の type: "quantitative" は binned 想定で MVP 外
- encoding.color.aggregate は mean/sum のみ
- nominal color + aggregate の同時指定
- size / tooltip / x2 / y2 encoding
非 strict は既存の緩い挙動を維持する。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 7: `layout/vega_rect.rs` — セル・軸ラベル・タイトルを描画

**目的:** stub だった layout モジュールを実装する。matrix.rs を参考にし、`None` セルは `Prim::Rect` を emit しない(透過)。

**Files:**
- Modify: `crates/fulgur-chart/src/layout/vega_rect.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Step 1: 失敗テストを追加(SVG smoke)**

```rust
#[test]
fn rect_mark_renders_svg_with_expected_rect_count() {
    // 2x2 grid, all cells present → 4 rects.
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},
            {"x":"B","y":"X","v":2},
            {"x":"A","y":"Y","v":3},
            {"x":"B","y":"Y","v":4}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    assert!(svg.starts_with("<svg"));
    // 背景の 1 枚 + セル 4 枚 = 5 枚(theme.background は白)
    // 単純に <rect ... の出現回数で確認。
    let rect_count = svg.matches("<rect").count();
    assert!(rect_count >= 4, "expected at least 4 cells, got {rect_count}: {svg}");
    // 軸ラベルが出る。
    assert!(svg.contains(">A<"));
    assert!(svg.contains(">B<"));
    assert!(svg.contains(">X<"));
    assert!(svg.contains(">Y<"));
}

#[test]
fn rect_mark_skips_missing_cells() {
    // (B, Y) missing → 3 セル、None セルは <rect> 非出力。
    let json = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},
            {"x":"B","y":"X","v":2},
            {"x":"A","y":"Y","v":3}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    let svg = fulgur_chart::render::render_chart(&spec);
    // 背景 1 + セル 3 = 4。>= 3 で十分(4 でないことは断定しない — background 有無で揺れる)。
    let rect_count = svg.matches("<rect").count();
    assert!(rect_count >= 3 && rect_count <= 4, "expected 3 cells + optional bg, got {rect_count}");
}
```

**Step 2: 失敗を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_renders 2>&1 | tail -20
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_skips 2>&1 | tail -20
```
Expected: fail(stub は空 scene を返す)。

**Step 3: `layout/vega_rect.rs` を実装**

`crates/fulgur-chart/src/layout/vega_rect.rs` を以下で置き換え:

```rust
//! Vega-Lite `mark: "rect"` (ヒートマップ) のレイアウト。
//! 純粋な grid renderer: cells[row][col] が Some のときのみ Prim::Rect を出す。
//! matrix.rs の構造を踏襲するが、scale 解決は frontend/vegalite.rs 側で完結している。

use super::common::{
    OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT, X_LABEL_BAND, X_LABEL_CENTER_RATIO,
};
use crate::ir::{ChartKind, ChartSpec};
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let (x_labels, y_labels, cells) = match &spec.kind {
        ChartKind::VegaRect { x_labels, y_labels, cells } => (x_labels, y_labels, cells),
        _ => unreachable!("vega_rect::build called on non-VegaRect kind"),
    };

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let n_rows = y_labels.len();
    let n_cols = x_labels.len();

    // y 軸ラベル最大幅
    let mut max_y_w = 0.0_f32;
    for l in y_labels {
        let w = m.width(l, label_font as f32);
        if w > max_y_w {
            max_y_w = w;
        }
    }
    let y_axis_w = max_y_w as f64 + 10.0;

    let title_band = if spec.title.is_some() { TITLE_BAND } else { 0.0 };

    let plot_left = OUTER_PAD + y_axis_w;
    let plot_right = spec.width - OUTER_PAD;
    let plot_top = OUTER_PAD + title_band;
    let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND;

    let plot_w = plot_right - plot_left;
    let plot_h = plot_bottom - plot_top;

    let cell_w = if n_cols > 0 { plot_w / n_cols as f64 } else { plot_w };
    let cell_h = if n_rows > 0 { plot_h / n_rows as f64 } else { plot_h };

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
            rotate_deg: None,
        });
    }

    // セル(None は skip)
    for (row, row_cells) in cells.iter().enumerate() {
        let cell_y = plot_top + row as f64 * cell_h;
        for (col, cell) in row_cells.iter().enumerate() {
            if let Some(fill) = cell {
                let cell_x = plot_left + col as f64 * cell_w;
                items.push(Prim::Rect {
                    x: cell_x,
                    y: cell_y,
                    w: cell_w,
                    h: cell_h,
                    fill: *fill,
                });
            }
        }
    }

    // x 軸ラベル(各列中央下)
    for (col, label) in x_labels.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left + col as f64 * cell_w + cell_w / 2.0,
            y: plot_bottom + X_LABEL_BAND * X_LABEL_CENTER_RATIO,
            size: label_font,
            anchor: Anchor::Middle,
            fill: ink,
            content: label.clone(),
            rotate_deg: None,
        });
    }

    // y 軸ラベル(各行中央左、右寄せ)
    for (row, label) in y_labels.iter().enumerate() {
        items.push(Prim::Text {
            x: plot_left - 6.0,
            y: plot_top + row as f64 * cell_h + cell_h / 2.0 + label_font * TEXT_BASELINE_RATIO,
            size: label_font,
            anchor: Anchor::End,
            fill: ink,
            content: label.clone(),
            rotate_deg: None,
        });
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
```

**Step 4: pass を確認**

```bash
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_renders 2>&1 | tail -10
cargo test -p fulgur-chart --test frontend_vegalite rect_mark_skips 2>&1 | tail -10
```
Expected: 2 tests pass。

**Step 5: 回帰なしを確認**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail
```

**Step 6: commit**

```bash
git add crates/fulgur-chart/src/layout/vega_rect.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "$(cat <<'EOF'
feat(layout): implement vega_rect grid renderer

layout/vega_rect.rs を stub から実装。matrix.rs のセル・軸ラベル・
タイトル配置を踏襲し、cells[row][col] が Some のときだけ Prim::Rect を
出す(None セルは透過スキップ)。scale 解決は frontend で完結しており、
layout は純粋な grid renderer に留まる。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 8: 決定的レンダリングの snapshot golden(insta)

**目的:** 3×3 quantitative fixture + 3×2 nominal color fixture の SVG を insta スナップショットで固定する。

**Files:**
- Create: `crates/fulgur-chart/tests/render_vega_rect.rs`
- Auto-create: `crates/fulgur-chart/tests/snapshots/render_vega_rect__*.snap`

**Step 1: テストファイルを作成**

`crates/fulgur-chart/tests/render_vega_rect.rs`:

```rust
use fulgur_chart::frontend::vegalite;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&vegalite::parse(json, false).unwrap())
}

#[test]
fn rect_quantitative_snapshot() {
    let svg = render(
        r#"{
            "mark": "rect",
            "data": {"values": [
                {"day":"Mon","hour":"AM","v":1},
                {"day":"Tue","hour":"AM","v":4},
                {"day":"Wed","hour":"AM","v":2},
                {"day":"Mon","hour":"PM","v":6},
                {"day":"Tue","hour":"PM","v":9},
                {"day":"Wed","hour":"PM","v":3},
                {"day":"Mon","hour":"EVE","v":2},
                {"day":"Tue","hour":"EVE","v":5},
                {"day":"Wed","hour":"EVE","v":7}
            ]},
            "encoding": {
                "x": {"field":"day","type":"nominal"},
                "y": {"field":"hour","type":"nominal"},
                "color": {"field":"v","type":"quantitative"}
            },
            "title": "Weekly Heatmap"
        }"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn rect_nominal_snapshot() {
    let svg = render(
        r#"{
            "mark": "rect",
            "data": {"values": [
                {"x":"A","y":"X","cat":"a"},
                {"x":"B","y":"X","cat":"b"},
                {"x":"C","y":"X","cat":"a"},
                {"x":"A","y":"Y","cat":"c"},
                {"x":"B","y":"Y","cat":"a"},
                {"x":"C","y":"Y","cat":"b"}
            ]},
            "encoding": {
                "x": {"field":"x","type":"nominal"},
                "y": {"field":"y","type":"nominal"},
                "color": {"field":"cat","type":"nominal"}
            }
        }"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn rect_missing_cells_snapshot() {
    // (B, Y) is absent, expect 3 rects for cells + optional background.
    let svg = render(
        r#"{
            "mark": "rect",
            "data": {"values": [
                {"x":"A","y":"X","v":1},
                {"x":"B","y":"X","v":2},
                {"x":"A","y":"Y","v":3}
            ]},
            "encoding": {
                "x": {"field":"x","type":"nominal"},
                "y": {"field":"y","type":"nominal"},
                "color": {"field":"v","type":"quantitative"}
            }
        }"#,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn rect_deterministic() {
    let j = r#"{
        "mark": "rect",
        "data": {"values": [
            {"x":"A","y":"X","v":1},{"x":"B","y":"X","v":3},{"x":"A","y":"Y","v":5}
        ]},
        "encoding": {
            "x": {"field":"x","type":"nominal"},
            "y": {"field":"y","type":"nominal"},
            "color": {"field":"v","type":"quantitative"}
        }
    }"#;
    assert_eq!(render(j), render(j));
}
```

**Step 2: スナップショットを生成して確定**

```bash
INSTA_UPDATE=always cargo test -p fulgur-chart --test render_vega_rect 2>&1 | tail -10
```
Expected: 4 tests pass。3 つの `.snap` が生成される。

**Step 3: スナップショットの検証(目視)**

```bash
ls crates/fulgur-chart/tests/snapshots/render_vega_rect__*.snap
# 生成された .snap を head で軽く確認(<svg で始まり </svg> で終わる)
head -3 crates/fulgur-chart/tests/snapshots/render_vega_rect__rect_quantitative_snapshot.snap
```
Expected: 3 個の .snap が生成され、SVG 文字列が入っている。

**Step 4: 再実行で pass(スナップショット確定)**

```bash
cargo test -p fulgur-chart --test render_vega_rect 2>&1 | tail -10
```
Expected: 4 tests pass。

**Step 5: commit**

```bash
git add crates/fulgur-chart/tests/render_vega_rect.rs crates/fulgur-chart/tests/snapshots/render_vega_rect__*.snap
git commit -m "$(cat <<'EOF'
test(vegalite): snapshot golden for mark: "rect" heatmap

3x3 quantitative、3x2 nominal、欠損セル込みの 3 fixture を insta スナップ
ショットで固定し、決定的レンダリングを保証する。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 9: example spec と CLI smoke

**目的:** `examples/specs/vegalite-rect-heatmap.json` を追加し、CLI 経由で SVG が生成できることを確認する。

**Files:**
- Create: `examples/specs/vegalite-rect-heatmap.json`

**Step 1: fixture spec を作成**

`examples/specs/vegalite-rect-heatmap.json`:

```json
{
  "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
  "mark": "rect",
  "title": "Weekly Activity Heatmap",
  "data": {
    "values": [
      {"day": "Mon", "hour": "AM", "v": 5},
      {"day": "Tue", "hour": "AM", "v": 8},
      {"day": "Wed", "hour": "AM", "v": 3},
      {"day": "Mon", "hour": "PM", "v": 9},
      {"day": "Tue", "hour": "PM", "v": 2},
      {"day": "Wed", "hour": "PM", "v": 7}
    ]
  },
  "encoding": {
    "x": {"field": "day", "type": "nominal"},
    "y": {"field": "hour", "type": "nominal"},
    "color": {"field": "v", "type": "quantitative"}
  }
}
```

**Step 2: CLI で SVG 生成確認**

```bash
cargo run -p fulgur-chart-cli -- render --dsl vegalite --format svg examples/specs/vegalite-rect-heatmap.json /tmp/rect_heatmap.svg 2>&1 | tail -5
head -2 /tmp/rect_heatmap.svg
```
Expected: 生成成功、`<svg` から始まる SVG。

(CLI の正確な引数は `cargo run -p fulgur-chart-cli -- --help` で確認、既存 vegalite の例と揃える。)

**Step 3: fixture のパースが strict でも通ることを確認**

```bash
cargo run -p fulgur-chart-cli -- render --dsl vegalite --strict --format svg examples/specs/vegalite-rect-heatmap.json /tmp/rect_heatmap_strict.svg 2>&1 | tail -5
```
Expected: 成功。

**Step 4: commit**

```bash
git add examples/specs/vegalite-rect-heatmap.json
git commit -m "$(cat <<'EOF'
docs(examples): add vegalite-rect-heatmap.json fixture

Vega-Lite mark: "rect" のサンプル spec。曜日 × 時間帯のアクティビティ
ヒートマップ。CLI から strict/非 strict の両モードで SVG 生成できる。

refs: fulgur-chart-05j
EOF
)"
```

---

## Task 10: 最終検証(全テスト・clippy・fmt)

**Step 1: 全テスト**

```bash
cargo test -p fulgur-chart --tests --lib 2>&1 | grep -E "test result" | tail
```
Expected: 全 pass、新規テスト分だけカウントが増えている。

**Step 2: workspace 全体テスト**

```bash
cargo test --workspace 2>&1 | grep -E "test result" | tail -20
```
Expected: 全 pass。

**Step 3: clippy**

```bash
cargo clippy -p fulgur-chart --tests -- -D warnings 2>&1 | tail -20
```
Expected: warnings 0。差分あれば修正して再 commit。

**Step 4: fmt**

```bash
cargo fmt --all --check 2>&1 | tail -5
```
Expected: 差分なし。差分あれば `cargo fmt --all` して commit。

**Step 5: 最終 commit(必要なら)**

fmt/clippy の追加修正があれば:

```bash
git add -A
git commit -m "chore: satisfy clippy and fmt for rect mark implementation"
```

---

## YAGNI / 非対応(この plan では扱わない)

- `size` / `tooltip` / `opacity` encoding(別 issue)
- `x2` / `y2` encoding(binned ヒートマップ)
- Vega-Lite color scheme 名(`viridis` 等)
- `aggregate` の `count` / `min` / `max` / `median`
- `scale.domain` / `scale.range` の詳細指定
- `sort` エンコーディングでカテゴリ順を変える(first-seen 固定)
- 数値カテゴリ x/y の binning(strict で quantitative を Err)

## 受け入れ基準(from beads issue fulgur-chart-05j)

- [x] `mark: "rect"` の Vega-Lite spec を parse し、決定的な SVG を描画できる → Task 3, 4, 5, 7, 8
- [x] x/y encoding の nominal/ordinal と color encoding の quantitative/nominal を扱える(aggregate mean/sum 含む) → Task 3, 4, 5
- [x] 未出現の (x,y) 組み合わせのセルは透過スキップされる(Prim::Rect emit なし) → Task 7
- [x] strict モードで size / tooltip / x2 / y2 encoding、quantitative x、quantitative y、mean/sum 以外の aggregate、nominal color + aggregate を Err にする → Task 6
- [x] `cargo test` (unit + snapshot golden) / `cargo clippy` / `cargo fmt` 通過 → Task 10
- [x] `examples/specs/vegalite-rect-heatmap.json` が動く → Task 9
- [x] 既存 ChartKind::Matrix パスと Chart.js matrix chart 出力に regression がない → 各タスクの回帰確認で保証
