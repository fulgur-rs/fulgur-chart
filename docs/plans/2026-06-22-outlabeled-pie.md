# outlabeledPie / outlabeledDoughnut Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** QuickChart 互換の `outlabeledPie` / `outlabeledDoughnut` チャートを追加する。各スライスから円の外側へ引き出し線を描き、ラベル（カテゴリ名 + パーセント）を外側に配置する。

**Architecture:** 新しい `ChartKind::OutlabeledPie { donut_ratio, outlabel }` variant を追加し、設定を variant に内包することで `ChartSpec` 構造体リテラルへの影響を最小化する。専用の `layout/outlabeled_pie.rs` モジュールが描画を担当し、既存の `pie.rs` から `Geom`/`make_slice`/`slice_path` を再利用する。

**Tech Stack:** Rust, SVG path, `crates/fulgur-chart/src/`

---

### Task 1: `ir.rs` に `OutlabelConfig` と `ChartKind::OutlabeledPie` を追加

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`

**Step 1: テストを書く**

`ir.rs` の `#[cfg(test)]` ブロック末尾に追加:

```rust
#[test]
fn outlabeled_pie_kind_is_distinct_from_pie() {
    let pie = ChartKind::Pie { donut_ratio: 0.0 };
    let out = ChartKind::OutlabeledPie {
        donut_ratio: 0.0,
        outlabel: OutlabelConfig::default(),
    };
    assert_ne!(
        std::mem::discriminant(&pie),
        std::mem::discriminant(&out)
    );
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test outlabeled_pie_kind_is_distinct_from_pie 2>&1 | tail -5
```
Expected: コンパイルエラー（`OutlabeledPie` / `OutlabelConfig` 未定義）

**Step 3: `OutlabelConfig` 構造体を追加**

`ir.rs` の `ChartKind` enum 定義の**直前**（`#[derive(Clone, Debug, PartialEq)]` のある行の前）に追加:

```rust
/// outlabeledPie / outlabeledDoughnut の引き出しラベル設定。
#[derive(Clone, Debug, PartialEq)]
pub struct OutlabelConfig {
    /// ラベルテキストテンプレート。%l=カテゴリ名, %v=値, %p=パーセント。
    pub text: String,
    /// ラベル文字色。
    pub color: Color,
    /// ラベル背景色。None = スライス色を使用。
    pub background: Option<Color>,
    /// 引き出し線の長さ(px)。外周からこの距離だけ外側へ伸びる。
    pub stretch: f64,
}

impl Default for OutlabelConfig {
    fn default() -> Self {
        OutlabelConfig {
            text: "%l\n%p%".to_string(),
            color: Color { r: 255, g: 255, b: 255, a: 1.0 },
            background: None,
            stretch: 40.0,
        }
    }
}
```

**Step 4: `ChartKind::OutlabeledPie` variant を追加**

`ChartKind` enum の末尾（`Gauge { ... }` の後）に追加:

```rust
    /// QuickChart 互換の outlabeledPie / outlabeledDoughnut。
    /// 各スライスから円外側へ引き出し線を描き、ラベルを外に配置する。
    OutlabeledPie {
        donut_ratio: f64,
        outlabel: OutlabelConfig,
    },
```

**Step 5: テストが通ることを確認**

```bash
cargo test outlabeled_pie_kind_is_distinct_from_pie 2>&1 | tail -5
```
Expected: `test result: ok. 1 passed`

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/ir.rs
git commit -m "feat(ir): add OutlabelConfig and ChartKind::OutlabeledPie"
```

---

### Task 2: `schema/chartjs.rs` にスキーマ型を追加

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`

JSON Schema エクスポート用の型を追加する（`fulgur-chart schema --dsl chartjs` の出力に含まれる）。

**Step 1: `ChartJsSpec` に 2 variant を追加**

`ChartJsSpec` の `PolarArea` の後に追加:

```rust
    #[serde(rename = "outlabeledPie")]
    OutlabeledPie(OutlabeledPieSpec),
    #[serde(rename = "outlabeledDoughnut")]
    OutlabeledDoughnut(OutlabeledPieSpec),
```

**Step 2: `OutlabeledPieSpec` / `OutlabeledPieOptions` / `OutlabelsPlugin` を追加**

ファイル末尾（`RadialGaugeSpec` の定義の後）に追加:

```rust
// ────────────────────────────────────────────────
// outlabeledPie / outlabeledDoughnut
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlabeledPieSpec {
    pub data: PieData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<OutlabeledPieOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutlabeledPieOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<OutlabeledPiePlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct OutlabeledPiePlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legend: Option<LegendPlugin>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlabels: Option<OutlabelsPlugin>,
}

/// `chartjs-plugin-piechart-outlabels` 互換の引き出しラベル設定。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlabelsPlugin {
    /// ラベルテキストのテンプレート。%l=カテゴリ名, %v=値, %p=パーセント。
    /// デフォルト: "%l\n%p%"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// ラベル文字色。デフォルト: "white"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    /// ラベル背景色。省略時はスライス色を使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ColorString>,
    /// 引き出し線の長さ(px)。デフォルト: 40
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stretch: Option<f64>,
}
```

**Step 3: ビルドが通ることを確認**

```bash
cargo build 2>&1 | tail -5
```
Expected: `Finished`

**Step 4: コミット**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs
git commit -m "feat(schema): add OutlabeledPieSpec and OutlabelsPlugin for schema export"
```

---

### Task 3: `frontend/chartjs.rs` でパース処理を追加

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`

**Step 1: テストを書く**

既存テストブロック末尾に追加:

```rust
#[test]
fn parse_outlabeled_pie_kind() {
    let json = r#"{"type":"outlabeledPie","data":{"labels":["A","B","C"],"datasets":[{"data":[10,20,30]}]}}"#;
    let spec = parse(json, false).expect("parse error");
    assert!(matches!(
        spec.kind,
        crate::ir::ChartKind::OutlabeledPie { donut_ratio, .. } if (donut_ratio - 0.0).abs() < 1e-9
    ));
}

#[test]
fn parse_outlabeled_doughnut_kind() {
    let json = r#"{"type":"outlabeledDoughnut","data":{"labels":["A","B"],"datasets":[{"data":[40,60]}]}}"#;
    let spec = parse(json, false).expect("parse error");
    assert!(matches!(
        spec.kind,
        crate::ir::ChartKind::OutlabeledPie { donut_ratio, .. } if (donut_ratio - 0.5).abs() < 1e-9
    ));
}

#[test]
fn parse_outlabeled_pie_outlabels_plugin() {
    let json = r#"{
        "type": "outlabeledPie",
        "data": {"labels": ["X"], "datasets": [{"data": [100]}]},
        "options": {"plugins": {"outlabels": {"stretch": 60.0, "color": "black"}}}
    }"#;
    let spec = parse(json, false).expect("parse error");
    if let crate::ir::ChartKind::OutlabeledPie { outlabel, .. } = &spec.kind {
        assert!((outlabel.stretch - 60.0).abs() < 1e-9, "stretch mismatch");
        assert_eq!(outlabel.color.r, 0, "color should be black");
    } else {
        panic!("wrong kind");
    }
}

#[test]
fn outlabeled_pie_fill_alpha_is_one() {
    // outlabeledPie も pie 同様に fill alpha = 1.0 であるべき。
    let json = r#"{"type":"outlabeledPie","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}"#;
    let spec = parse(json, false).expect("parse error");
    assert!((spec.series[0].fill[0].a - 1.0).abs() < 1e-6, "fill alpha must be 1.0");
}
```

**Step 2: テストが失敗することを確認**

```bash
cargo test parse_outlabeled 2>&1 | tail -10
```
Expected: コンパイルエラーまたはパース失敗

**Step 3: `RawPlugins` に `outlabels` フィールドを追加**

`RawPlugins` 構造体（`struct RawPlugins` の定義箇所）に追加:

```rust
#[derive(Deserialize, Default)]
struct RawPlugins {
    title: Option<RawTitle>,
    legend: Option<RawLegend>,
    datalabels: Option<RawDataLabels>,
    outlabels: Option<RawOutlabels>,
}

#[derive(Deserialize)]
struct RawOutlabels {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    color: Option<String>,
    #[serde(rename = "backgroundColor", default)]
    background_color: Option<String>,
    #[serde(default)]
    stretch: Option<f64>,
}
```

**Step 4: type 文字列のマッチに追加**

`match raw.chart_type.as_str()` の `other => return Err(...)` の直前に追加:

```rust
            "outlabeledPie" => ChartKind::OutlabeledPie {
                donut_ratio: 0.0,
                outlabel: build_outlabel_config(&raw.options.plugins.outlabels),
            },
            "outlabeledDoughnut" => ChartKind::OutlabeledPie {
                donut_ratio: 0.5,
                outlabel: build_outlabel_config(&raw.options.plugins.outlabels),
            },
```

**Step 5: `build_outlabel_config` 関数を追加**

`build_theme` 関数の直後に追加:

```rust
fn build_outlabel_config(raw: &Option<RawOutlabels>) -> crate::ir::OutlabelConfig {
    use crate::ir::OutlabelConfig;
    let mut cfg = OutlabelConfig::default();
    let Some(raw) = raw else { return cfg };
    if let Some(t) = &raw.text {
        cfg.text = t.clone();
    }
    if let Some(c) = raw.color.as_deref().and_then(parse_color) {
        cfg.color = c;
    }
    if let Some(c) = raw.background_color.as_deref().and_then(parse_color) {
        cfg.background = Some(c);
    }
    if let Some(s) = raw.stretch {
        if s.is_finite() && s >= 0.0 {
            cfg.stretch = s;
        }
    }
    cfg
}
```

**Step 6: `is_pie` の `matches!` に `OutlabeledPie` を追加**

```rust
let is_pie = matches!(kind, ChartKind::Pie { .. } | ChartKind::PolarArea | ChartKind::OutlabeledPie { .. });
```

**Step 7: テストが通ることを確認**

```bash
cargo test parse_outlabeled outlabeled_pie_fill_alpha 2>&1 | tail -10
```
Expected: `4 passed`

**Step 8: コミット**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs
git commit -m "feat(frontend): parse outlabeledPie/outlabeledDoughnut types"
```

---

### Task 4: `layout/outlabeled_pie.rs` を新規作成

**Files:**
- Create: `crates/fulgur-chart/src/layout/outlabeled_pie.rs`

**Step 1: テストを書く（ファイル末尾の `#[cfg(test)]` ブロックとして）**

新規ファイルの末尾に記述:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::render::render_chart_with_font;

    fn make_spec(type_str: &str) -> crate::ir::ChartSpec {
        let json = format!(
            r#"{{"type":"{}","data":{{"labels":["A","B","C"],"datasets":[{{"data":[10,20,30]}}]}}}}"#,
            type_str
        );
        chartjs::parse(&json, false).expect("parse error")
    }

    #[test]
    fn outlabeled_pie_renders_to_svg() {
        let spec = make_spec("outlabeledPie");
        let svg = render_chart_with_font(&spec, DEFAULT_FONT).unwrap();
        assert!(svg.starts_with("<svg"), "should produce valid SVG");
    }

    #[test]
    fn outlabeled_doughnut_renders_to_svg() {
        let spec = make_spec("outlabeledDoughnut");
        let svg = render_chart_with_font(&spec, DEFAULT_FONT).unwrap();
        assert!(svg.starts_with("<svg"), "should produce valid SVG");
    }

    #[test]
    fn outlabeled_pie_has_text_primitives() {
        let spec = make_spec("outlabeledPie");
        let scene = build(&spec, &crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap());
        let has_text = scene.items.iter().any(|p| matches!(p, crate::scene::Prim::Text { .. }));
        assert!(has_text, "scene must contain Text primitives for labels");
    }

    #[test]
    fn outlabeled_pie_single_slice_renders() {
        // 単一スライス(100%)は2分割パスになるが、クラッシュしないことを確認。
        let json = r#"{"type":"outlabeledPie","data":{"labels":["Only"],"datasets":[{"data":[100]}]}}"#;
        let spec = chartjs::parse(json, false).expect("parse error");
        let svg = render_chart_with_font(&spec, DEFAULT_FONT).unwrap();
        assert!(svg.starts_with("<svg"));
    }

    #[test]
    fn outlabeled_doughnut_inner_radius_nonzero() {
        // doughnut は donut_ratio=0.5 なので inner > 0 のパスが生成されるはず。
        let spec = make_spec("outlabeledDoughnut");
        let scene = build(&spec, &crate::text::TextMeasurer::new(DEFAULT_FONT).unwrap());
        let has_doughnut_path = scene.items.iter().any(|p| {
            if let crate::scene::Prim::Path { d, .. } = p {
                // doughnut の path には inner arc (sweep=0) が含まれる。
                d.contains("0 0 ")
            } else {
                false
            }
        });
        assert!(has_doughnut_path, "doughnut must have inner arc paths");
    }
}
```

**Step 2: テストが失敗することを確認（ファイルが存在しないため）**

```bash
cargo test 2>&1 | grep "outlabeled" | head -5
```
Expected: コンパイルエラー（モジュールが見つからない）

**Step 3: `outlabeled_pie.rs` の本体を実装**

以下の内容でファイルを作成する。ロジックは `pie.rs` の `build()` をベースに、スライスを描いた後で引き出し線とラベルを追加する:

```rust
//! outlabeledPie / outlabeledDoughnut。スライス外側に引き出し線+ラベルを描く。

use super::common;
use super::pie::{Geom, SLICE_STROKE, make_slice};
use crate::ir::{ChartKind, ChartSpec, Color, LegendPos, OutlabelConfig};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;
use std::f64::consts::PI;

/// キャンバスに対するラベル領域の余白比。この分だけ円を小さくする。
const LABEL_MARGIN: f64 = 0.45; // radius = available * (1 - LABEL_MARGIN) ≒ 0.55

/// ラベルボックスの水平シェルフ長(px)。
const SHELF_LEN: f64 = 20.0;

/// ラベルボックスの内側パディング(px)。
const LABEL_PAD: f64 = 3.0;

/// 行間隔(px)。
const LINE_GAP: f64 = 2.0;

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let mut items: Vec<Prim> = Vec::new();

    let ink = spec.theme.text_color;
    let label_font = spec.theme.font_size;

    let (donut_ratio, outlabel) = match &spec.kind {
        ChartKind::OutlabeledPie { donut_ratio, outlabel } => (*donut_ratio, outlabel.clone()),
        _ => return Scene { width: spec.width, height: spec.height, items },
    };

    let series = spec.series.first();
    let empty: Vec<f64> = Vec::new();
    let values = series.map(|s| &s.values).unwrap_or(&empty);

    // 1. タイトル。
    let title_band = if spec.title.is_some() { common::TITLE_BAND } else { 0.0 };
    if let Some(title) = &spec.title {
        items.push(Prim::Text {
            x: spec.width / 2.0,
            y: common::OUTER_PAD + common::TITLE_FONT,
            size: common::TITLE_FONT,
            anchor: Anchor::Middle,
            fill: ink,
            content: title.clone(),
        });
    }

    // 2. 凡例。outlabeledPie でも Legend は描画可能とする。
    let has_legend = matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && spec.categories.iter().any(|c| !c.is_empty());
    let legend_top = if has_legend && spec.legend == LegendPos::Top { common::LEGEND_BAND } else { 0.0 };
    let legend_bottom = if has_legend && spec.legend == LegendPos::Bottom { common::LEGEND_BAND } else { 0.0 };

    if has_legend && matches!(spec.legend, LegendPos::Top | LegendPos::Bottom) {
        let mut total = 0.0_f64;
        let n = spec.categories.len();
        for (k, cat) in spec.categories.iter().enumerate() {
            total += common::legend_entry_width(m, cat, label_font);
            if k == n - 1 { total -= 16.0; }
        }
        let start_x = (spec.width - total) / 2.0;
        let legend_cy = if spec.legend == LegendPos::Top {
            common::OUTER_PAD + title_band + common::LEGEND_BAND / 2.0
        } else {
            spec.height - common::OUTER_PAD - common::LEGEND_BAND / 2.0
        };
        let mut cursor = start_x;
        for (i, cat) in spec.categories.iter().enumerate() {
            let swatch = series.map(|s| s.fill_at(i)).unwrap_or(ink);
            items.push(Prim::Rect { x: cursor, y: legend_cy - 6.0, w: 12.0, h: 12.0, fill: swatch });
            items.push(Prim::Text {
                x: cursor + 16.0,
                y: legend_cy + label_font * common::TEXT_BASELINE_RATIO,
                size: label_font, anchor: Anchor::Start, fill: ink, content: cat.clone(),
            });
            cursor += common::legend_entry_width(m, cat, label_font);
        }
    }

    // 3. 円の領域（通常より半径を小さくして外側にラベルスペースを確保）。
    let area_top = common::OUTER_PAD + title_band + legend_top;
    let area_bottom = spec.height - common::OUTER_PAD - legend_bottom;
    let cx = spec.width / 2.0;
    let cy = (area_top + area_bottom) / 2.0;
    let available = ((spec.width).min(area_bottom - area_top)) / 2.0;
    let radius = (available * (1.0 - LABEL_MARGIN)).max(0.0);
    let inner = radius * donut_ratio;

    // 4. スライスと引き出し線+ラベル。
    let total: f64 = values.iter().filter(|v| v.is_finite() && **v > 0.0).sum();
    let mut label_prims: Vec<Prim> = Vec::new();

    if total > 0.0 && radius > 0.0 {
        let mut a0 = -PI / 2.0;

        for (i, &v) in values.iter().enumerate() {
            if !(v.is_finite() && v > 0.0) { continue; }

            let frac = v / total;
            let a1 = a0 + frac * 2.0 * PI;
            let fill = series.map(|s| s.fill_at(i)).unwrap_or(ink);

            let g = Geom { cx, cy, r_outer: radius, r_inner: inner };

            // 全周スライス(単一要素100%)は SVG A の制約で2分割。
            if a1 - a0 >= 2.0 * PI - 1e-9 {
                let amid = a0 + (a1 - a0) / 2.0;
                items.push(make_slice(&g, a0, amid, fill));
                items.push(make_slice(&g, amid, a1, fill));
            } else {
                items.push(make_slice(&g, a0, a1, fill));
            }

            // 引き出し線 + ラベル。
            let amid = (a0 + a1) / 2.0;
            draw_outlabel(
                &mut label_prims,
                cx, cy, radius,
                amid, fill,
                i, v, frac,
                &spec.categories,
                &outlabel,
                label_font,
            );

            a0 = a1;
        }
    }

    // ラベルはスライスの上に描く。
    items.extend(label_prims);

    Scene { width: spec.width, height: spec.height, items }
}

/// 1スライス分の引き出し線とラベルボックスを `out` に追加する。
fn draw_outlabel(
    out: &mut Vec<Prim>,
    cx: f64, cy: f64, radius: f64,
    amid: f64,
    slice_fill: Color,
    idx: usize,
    value: f64,
    frac: f64,
    categories: &[String],
    cfg: &OutlabelConfig,
    font_size: f64,
) {
    let stretch = cfg.stretch;

    // P0: 外周上。
    let p0 = (cx + radius * amid.cos(), cy + radius * amid.sin());
    // P1: stretch 分だけ外側。
    let stretch_r = radius + stretch;
    let p1 = (cx + stretch_r * amid.cos(), cy + stretch_r * amid.sin());
    // P2: 水平シェルフの端点（右側+、左側-）。
    let on_right = amid.cos() >= 0.0;
    let p2 = if on_right {
        (p1.0 + SHELF_LEN, p1.1)
    } else {
        (p1.0 - SHELF_LEN, p1.1)
    };

    // 引き出し線（P0 → P1 → P2）。
    out.push(Prim::Polyline {
        points: vec![p0, p1, p2],
        stroke: slice_fill,
        stroke_width: 1.5,
    });

    // ラベルテキストの生成。%l=カテゴリ名, %v=値, %p=パーセント(整数)。
    let label_str = categories.get(idx).map(|s| s.as_str()).unwrap_or("");
    let pct = (frac * 100.0).round() as i64;
    let line1_raw = cfg.text.split('\n').next().unwrap_or("%l");
    let line2_raw = cfg.text.split('\n').nth(1).unwrap_or("%p%");
    let line1 = expand_template(line1_raw, label_str, value, pct);
    let line2 = expand_template(line2_raw, label_str, value, pct);

    // テキスト配置。
    let (anchor, text_x) = if on_right {
        (Anchor::Start, p2.0 + LABEL_PAD)
    } else {
        (Anchor::End, p2.0 - LABEL_PAD)
    };
    let line_h = font_size + LINE_GAP;
    let text_y_top = p2.1 - line_h / 2.0;

    // ラベル背景ボックス。
    let bg_color = cfg.background.unwrap_or(slice_fill);
    let w1 = estimate_text_width(&line1, font_size);
    let w2 = estimate_text_width(&line2, font_size);
    let box_w = w1.max(w2) + LABEL_PAD * 2.0;
    let box_h = line_h * 2.0 + LABEL_PAD * 2.0;
    let box_x = if on_right { p2.0 } else { p2.0 - box_w };
    let box_y = text_y_top - LABEL_PAD;
    out.push(Prim::Rect { x: box_x, y: box_y, w: box_w, h: box_h, fill: bg_color });

    // 1行目テキスト。
    out.push(Prim::Text {
        x: text_x,
        y: text_y_top + font_size * common::TEXT_BASELINE_RATIO,
        size: font_size,
        anchor,
        fill: cfg.color,
        content: line1,
    });
    // 2行目テキスト。
    out.push(Prim::Text {
        x: text_x,
        y: text_y_top + line_h + font_size * common::TEXT_BASELINE_RATIO,
        size: font_size,
        anchor,
        fill: cfg.color,
        content: line2,
    });
}

/// `%l`, `%v`, `%p` を実際の値に展開する。
fn expand_template(tmpl: &str, label: &str, value: f64, pct: i64) -> String {
    tmpl.replace("%l", label)
        .replace("%v", &fmt_num(value))
        .replace("%p", &pct.to_string())
}

/// テキスト幅の粗い見積もり（実際の TextMeasurer なしで近似）。
/// 0.6 * font_size * 文字数 程度で見積もる。
fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.chars().count() as f64 * font_size * 0.6
}
```

**Step 4: `layout/mod.rs` に `outlabeled_pie` モジュールと分岐を追加**

`mod.rs` の `pub mod` リストに追加:

```rust
pub mod outlabeled_pie;
```

`build_scene` の `match spec.kind` に追加:

```rust
        ChartKind::OutlabeledPie { .. } => outlabeled_pie::build(spec, m),
```

**Step 5: テストが通ることを確認**

```bash
cargo test outlabeled 2>&1 | tail -10
```
Expected: 全テスト pass

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/layout/outlabeled_pie.rs crates/fulgur-chart/src/layout/mod.rs
git commit -m "feat(layout): add outlabeled_pie rendering with leader lines and labels"
```

---

### Task 5: examples を追加してエンドツーエンド確認

**Files:**
- Create: `examples/specs/outlabeled_pie.json`
- Create: `examples/specs/outlabeled_doughnut.json`

**Step 1: `outlabeled_pie.json` を作成**

```json
{
  "type": "outlabeledPie",
  "data": {
    "labels": ["Desktop", "Mobile", "Tablet", "Other"],
    "datasets": [
      { "data": [55, 30, 12, 3] }
    ]
  },
  "options": {
    "plugins": {
      "title": { "display": true, "text": "Access by Device (outlabeled)" }
    }
  }
}
```

**Step 2: `outlabeled_doughnut.json` を作成**

```json
{
  "type": "outlabeledDoughnut",
  "data": {
    "labels": ["Chrome", "Safari", "Edge", "Firefox", "Other"],
    "datasets": [
      { "data": [63, 19, 8, 6, 4] }
    ]
  },
  "options": {
    "plugins": {
      "title": { "display": true, "text": "Browser Share (outlabeled doughnut)" },
      "outlabels": { "stretch": 50.0 }
    }
  }
}
```

**Step 3: CLI で SVG 出力を確認**

```bash
cargo run --bin fulgur-chart -- examples/specs/outlabeled_pie.json /tmp/outlabeled_pie.svg
cargo run --bin fulgur-chart -- examples/specs/outlabeled_doughnut.json /tmp/outlabeled_doughnut.svg
echo "exit: $?"
```
Expected: `exit: 0`

**Step 4: 全テストスイートが通ることを確認**

```bash
cargo test 2>&1 | tail -10
```
Expected: 全テスト pass（ベースライン 33 + 新規テスト）

**Step 5: コミット**

```bash
git add examples/specs/outlabeled_pie.json examples/specs/outlabeled_doughnut.json
git commit -m "feat(examples): add outlabeled_pie and outlabeled_doughnut example specs"
```

---

## 完了チェックリスト

- [ ] `ir.rs`: `OutlabelConfig` + `ChartKind::OutlabeledPie` 追加済み
- [ ] `schema/chartjs.rs`: `OutlabeledPieSpec` / `OutlabelsPlugin` 追加済み
- [ ] `frontend/chartjs.rs`: type 文字列マッチ + `is_pie` 拡張 + `build_outlabel_config` 追加済み
- [ ] `layout/outlabeled_pie.rs`: 新規作成済み
- [ ] `layout/mod.rs`: `ChartKind::OutlabeledPie` 分岐追加済み
- [ ] `cargo test` 全テスト pass
- [ ] examples/specs に 2 ファイル追加済み
