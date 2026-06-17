# 最小データラベル (datalabels) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chart.js spec の `options.plugins.datalabels` を最小サポートし、bar(縦/横)・line/area・pie/doughnut の各データ点に値テキストを決定的に描画する。

**Architecture:** フロントエンド(chartjs)で「datalabels キーが存在し display!=false」を解決し、IR `ChartSpec` の `data_labels: bool` に落とす。各レイアウト(bar/line/pie)は既存ループ内で `spec.data_labels` が真のときのみ `Prim::Text` を追加する。座標はすべて `fmt_num` 経由で byte-identical を維持。

**Tech Stack:** Rust 2024 / serde / insta(スナップショット)。対応 issue: `fulgur-chart-5jb`。

**規約:** TDD 厳守(失敗テスト→失敗確認→最小実装→成功確認→コミット)。決定性最優先。色は文脈依存(bar/line=INK #666、pie=白)。オプションは `display` のみ解釈(YAGNI)。

---

## Task 1: IR フラグ + frontend パース + strict 検査

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`（`ChartSpec` に `data_labels`）
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`（`RawDataLabels` パース・有効判定・strict）
- Test: `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Step 1: 失敗するテストを追加**

`tests/frontend_chartjs.rs` の末尾に追記:

```rust
#[test]
fn datalabels_key_present_enables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{}}} }"#;
    assert!(chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_display_true_enables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":true}}} }"#;
    assert!(chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_display_false_disables() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":false}}} }"#;
    assert!(!chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn datalabels_absent_is_false() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]} }"#;
    assert!(!chartjs::parse(json, false).unwrap().data_labels);
}
#[test]
fn strict_accepts_known_datalabels_keys() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"display":true}}} }"#;
    assert!(chartjs::parse(json, true).is_ok());
}
#[test]
fn strict_rejects_unknown_datalabels_key() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"plugins":{"datalabels":{"foo":1}}} }"#;
    assert!(chartjs::parse(json, true).is_err());
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test frontend_chartjs`
Expected: コンパイルエラー（`data_labels` フィールド未定義）。

**Step 3: 実装**

`ir.rs` の `ChartSpec` にフィールドを追加（`height` の後）:

```rust
    pub width: f64,
    pub height: f64,
    /// データラベルを描画するか(frontend で解決済み)。
    pub data_labels: bool,
}
```

`chartjs.rs` の `RawPlugins` に field を追加し、構造体を定義:

```rust
#[derive(Deserialize, Default)]
struct RawPlugins {
    title: Option<RawTitle>,
    legend: Option<RawLegend>,
    datalabels: Option<RawDataLabels>,
}

#[derive(Deserialize)]
struct RawDataLabels {
    #[serde(default)]
    display: Option<bool>,
}
```

`parse()` 内、`let kind = ...` の後あたりで有効判定を計算:

```rust
    // datalabels: キーが存在し display!=false なら有効。
    let data_labels = match &raw.options.plugins.datalabels {
        Some(dl) => dl.display != Some(false),
        None => false,
    };
```

`Ok(ChartSpec { ... })` の末尾フィールドに `data_labels,` を追加（`height: 450.0,` の後）。

`check_unknown_keys()` の plugins ブロックに内側キー検査を追加:

```rust
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(plugins, &["title", "legend", "datalabels"], "options.plugins")?;
            if let Some(dl) = plugins.get("datalabels").and_then(|v| v.as_object()) {
                check_object(dl, &["display"], "options.plugins.datalabels")?;
            }
        }
```

> 注: `ChartSpec` を構築するのは frontend の `parse()` のみ（render/CLI は `&ChartSpec` を受けるだけ）。念のため `grep -rn "ChartSpec {" crates/` で他の構築箇所が無いことを確認する。

**Step 4: 成功を確認**

Run: `cargo test -p fulgur-chart --test frontend_chartjs`
Expected: PASS（既存テストも全通過）。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat: datalabels の IR フラグと chart.js フロントエンド解決を追加"
```

---

## Task 2: 縦棒のデータラベル

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs`（`build_vertical`）
- Test: `crates/fulgur-chart/tests/render_datalabels.rs`（新規）

**Step 1: 失敗するテストを追加**

`tests/render_datalabels.rs` を新規作成:

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn vertical_bar_datalabels_render_values() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["1月","2月","3月"],"datasets":[{"data":[123,87,151]}]},
      "options":{"plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    // 123/87 は奇数・非5倍数。nice_ticks の目盛り(丸い値)にもカテゴリ名にも
    // 一致しないため、この部分文字列はデータラベル由来とのみ判定できる。
    assert!(svg.contains(">123</text>"), "datalabel 123 が描画されること");
    assert!(svg.contains(">87</text>"));
}

#[test]
fn vertical_bar_without_datalabels_has_no_value_text() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["1月"],"datasets":[{"data":[123]}]}
    }"#;
    // 123 はどの nice_ticks 目盛りにも出ない値なので、無効時は SVG に現れない。
    assert!(!render(json).contains(">123</text>"), "無効時は値ラベルを描かない");
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test render_datalabels`
Expected: `vertical_bar_datalabels_render_values` が FAIL（ラベル未描画）。

**Step 3: 実装**

`bar.rs` 冒頭の use にラベル描画用を追加:

```rust
use crate::num::fmt_num;
use crate::scene::Anchor;
use super::common::{INK, LABEL_FONT};
```

`build_vertical` の内側ループ、`items.push(Prim::Rect { ... });` の直後に追加:

```rust
            if spec.data_labels && v.is_finite() {
                let cx = bx + (bar_w * BAR_FILL_RATIO) / 2.0;
                // 正(上向き)は棒上端の上、負は棒下端の下。
                let label_y = if v >= base_v {
                    y_top - 4.0
                } else {
                    y_top + h + LABEL_FONT
                };
                items.push(Prim::Text {
                    x: cx,
                    y: label_y,
                    size: LABEL_FONT,
                    anchor: Anchor::Middle,
                    fill: INK,
                    content: fmt_num(v),
                });
            }
```

（`v` / `bx` / `bar_w` / `y_top` / `h` / `base_v` は同ループ内で既出。）

**Step 4: 成功を確認 + スナップショット受理**

Run: `cargo test -p fulgur-chart --test render_datalabels`
Expected: PASS。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/layout/bar.rs crates/fulgur-chart/tests/render_datalabels.rs
git commit -m "feat: 縦棒のデータラベル描画を追加"
```

---

## Task 3: 横棒のデータラベル

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs`（`build_horizontal`）
- Test: `crates/fulgur-chart/tests/render_datalabels.rs`（追記）

**Step 1: 失敗するテストを追記**

```rust
#[test]
fn horizontal_bar_datalabels_render_values() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["a","b"],"datasets":[{"data":[123,87]}]},
      "options":{"indexAxis":"y","plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    assert!(svg.contains(">123</text>"));
    assert!(svg.contains(">87</text>"));
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test render_datalabels horizontal_bar`
Expected: FAIL。

**Step 3: 実装**

`build_horizontal` 内（`use crate::layout::common::*;` 済みで `value_label`/`LABEL_GAP`/`LABEL_FONT`/`INK`/`TEXT_BASELINE_RATIO` 利用可、`use crate::scene::Anchor;` も利用可）。共通ヘルパ `common::value_label`（Task 2 で導入済み）を使う。
横棒の `items.push(Prim::Rect { ... });` の直後に追加:

```rust
            if spec.data_labels && v.is_finite() {
                let cy = by + (bar_h * BAR_FILL_RATIO) / 2.0 + LABEL_FONT * TEXT_BASELINE_RATIO;
                // 正は棒右端の右(Start)、負は左端の左(End)に LABEL_GAP 分離す。
                let (lx, anchor) = if v >= base_v {
                    (vx + LABEL_GAP, Anchor::Start)
                } else {
                    (vx - LABEL_GAP, Anchor::End)
                };
                items.push(value_label(lx, cy, anchor, INK, v));
            }
```

（`v` / `by` / `bar_h` / `vx` / `base_v` は同ループ内で既出。`fmt_num` は目盛りラベルで引き続き使うため import はそのまま。）

**Step 4: 成功を確認**

Run: `cargo test -p fulgur-chart --test render_datalabels`
Expected: PASS。

**Step 5: コミット**

```bash
git commit -am "feat: 横棒のデータラベル描画を追加"
```

---

## Task 4: line / area のデータラベル

**Files:**
- Modify: `crates/fulgur-chart/src/layout/line.rs`
- Test: `crates/fulgur-chart/tests/render_datalabels.rs`（追記）

**Step 1: 失敗するテストを追記**

```rust
#[test]
fn line_datalabels_render_values() {
    let json = r#"{
      "type":"line",
      "data":{"labels":["a","b","c"],"datasets":[{"data":[123,87,151]}]},
      "options":{"plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    assert!(svg.contains(">123</text>"));
    assert!(svg.contains(">87</text>"));
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test render_datalabels line_datalabels`
Expected: FAIL。

**Step 3: 実装**

`line.rs` の use に `Anchor` を追加（既存 `use crate::scene::{Prim, Scene};` を変更）:

```rust
use crate::scene::{Anchor, Prim, Scene};
```

`build` 内、各 `ser` ループのマーカー描画ブロックの後（点ごとの値ラベル）に追加。共通ヘルパ `common::value_label` を使う:

```rust
        // データラベル(点の上、マーカー半径ぶん+余白だけ上)。
        if spec.data_labels {
            for (i, (x, y)) in pts.iter().enumerate() {
                if let Some(&v) = ser.values.get(i) {
                    if v.is_finite() {
                        items.push(common::value_label(
                            *x,
                            *y - MARKER_R - common::LABEL_GAP,
                            Anchor::Middle,
                            common::INK,
                            v,
                        ));
                    }
                }
            }
        }
```

（`MARKER_R`(3.0) は line.rs の既存 const。`MARKER_R + LABEL_GAP` = 7px 上に置き、マーカーと重ならないようにする。`fmt_num` は area/曲線パスで引き続き使うため import はそのまま。）

**Step 4: 成功を確認**

Run: `cargo test -p fulgur-chart --test render_datalabels`
Expected: PASS。

**Step 5: コミット**

```bash
git commit -am "feat: line/area のデータラベル描画を追加"
```

---

## Task 5: pie / doughnut のデータラベル

**Files:**
- Modify: `crates/fulgur-chart/src/layout/pie.rs`
- Test: `crates/fulgur-chart/tests/render_datalabels.rs`（追記）

**Step 1: 失敗するテストを追記**

```rust
#[test]
fn pie_datalabels_render_values() {
    let json = r#"{
      "type":"pie",
      "data":{"labels":["a","b","c"],"datasets":[{"data":[30,50,20]}]},
      "options":{"plugins":{"datalabels":{"display":true}}}
    }"#;
    let svg = render(json);
    assert!(svg.contains(">30</text>"));
    assert!(svg.contains(">50</text>"));
    assert!(svg.contains(">20</text>"));
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test render_datalabels pie_datalabels`
Expected: FAIL。

**Step 3: 実装**

`pie.rs` のラベル白色用に定数を追加（`SLICE_STROKE` の近く）:

```rust
/// データラベルの文字色(スライス上で読めるよう白)。
const LABEL_COLOR: Color = Color { r: 255, g: 255, b: 255, a: 1.0 };
```

ラベル蓄積用 Vec を用意する。**`if total > 0.0 && radius > 0.0 { ... }` ブロックの外側（関数本体スコープ、ブロックの直前）で宣言する**こと（ブロック後の `items.extend(labels)` から見える必要があるため）:

```rust
    let mut labels: Vec<Prim> = Vec::new();
```

スライスループ内、`a0 = a1;` の直前（スライス Prim を push した後）に追加。共通ヘルパ `common::value_label` を使う:

```rust
            if spec.data_labels {
                let amid = (a0 + a1) / 2.0;
                let label_r = if inner > 0.0 {
                    (inner + radius) / 2.0
                } else {
                    radius * 0.6
                };
                labels.push(common::value_label(
                    cx + label_r * amid.cos(),
                    cy + label_r * amid.sin() + common::LABEL_FONT * common::TEXT_BASELINE_RATIO,
                    Anchor::Middle,
                    LABEL_COLOR,
                    v,
                ));
            }
```

スライスループ終了後（`Scene { ... }` を返す直前）にラベルを最前面へ:

```rust
    items.extend(labels);
```

> 全周単一スライス(2分割)の場合は `a0..amid` と `amid..a1` の 2 つの slice を push しているが、ラベルはループ1反復につき1つ（その値の中点角）でよい。上記は反復単位で push するため 1 スライス=1 ラベルになる。

**Step 4: 成功を確認**

Run: `cargo test -p fulgur-chart --test render_datalabels`
Expected: PASS。

**Step 5: コミット**

```bash
git commit -am "feat: pie/doughnut のデータラベル描画を追加"
```

---

## Task 6: 全体検証 + README 反映

**Files:**
- Modify: `README.md`

**Step 1: 全テスト + lint**

```bash
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```
Expected: すべて通過（既存スナップショットは無変化）。

**Step 2: 決定性スポット確認**

同一 datalabels spec を2回レンダリングして byte-identical を確認（既存の決定性テストでも担保されるが、念のため手動 1 回）:

```bash
echo '{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[120]}]},"options":{"plugins":{"datalabels":{"display":true}}}}' \
  | cargo run -q -p fulgur-chart-cli -- render - -o - > /tmp/d1.svg
echo '{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[120]}]},"options":{"plugins":{"datalabels":{"display":true}}}}' \
  | cargo run -q -p fulgur-chart-cli -- render - -o - > /tmp/d2.svg
diff /tmp/d1.svg /tmp/d2.svg && echo "byte-identical OK"
```

**Step 3: README 更新**

- 「対応する chart.js サブセット」の `options.plugins.title / legend` の行に `datalabels（display）` を追記。
- 「将来対応（v1 では未対応）」の箇条書きから「データラベル（datalabels）」を削除。

**Step 4: コミット**

```bash
git add README.md
git commit -m "docs: README にデータラベル対応を反映"
```

---

## 完了の定義 (Definition of Done)

- `options.plugins.datalabels`（display!=false）で bar(縦/横)・line/area・pie/doughnut に値ラベルが描画される。
- キー無し / `display:false` では描画されず、既存スナップショットは無変化。
- 出力は byte-identical（決定性テスト通過）。
- `cargo test` 全通過、`cargo fmt --check` / `cargo clippy -D warnings` 通過。
- README から datalabels が「将来対応」を抜け、サブセット表に載っている。
