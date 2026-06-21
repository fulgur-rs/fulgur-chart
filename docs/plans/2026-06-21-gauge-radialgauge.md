# gauge / radialGauge Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chart.js v4 互換 JSON の `type:"radialGauge"`(全円・塗りつぶし弧+中央値+トラック) と `type:"gauge"`(半円・色帯ゾーン+針+値ラベル) を QuickChart 忠実なモデルで実装し、決定的な SVG/PNG として描画する。

**Architecture:** `ChartKind` に 2 変種 `RadialGauge {..}` / `Gauge {..}` を追加(描画モデルが別物のため分離)。config 形状が標準の `datasets[].data` 数値配列に収まらない(gauge は `value`/`minValue`、options に `domain`/`needle`/`valueLabel` 等)ため、matrix と同じく `frontend/chartjs.rs::parse()` 冒頭(`check_unknown_keys` の前)に**専用パス** `parse_gauge()` を分岐させる。閾値/値は `Series.values`、ゾーン色/塗り色は `Series.fill` に載せ既存の色解決経路を再利用、針値・domain・トラック色・内径比・rounded 等のスカラは `ChartKind` 側に持たせる。レイアウトは軸なしの `layout/gauge.rs` を新設し、弧パスは `fmt_num` 整形 + standalone な空白区切り M/L/A/Z トークンで決定的に生成する(`raster_direct::parse_path_data` 不変条件)。

**Tech Stack:** Rust(workspace: `crates/fulgur-chart` コア + `crates/fulgur-chart-cli`)、serde / schemars、insta(スナップショット)、resvg/tiny-skia(PNG)。

**ビルド/テスト/lint コマンド(worktree 内で実行):**
- テスト: `cargo test -p fulgur-chart`
- 全テスト: `cargo test --workspace`
- フォーマット: `cargo fmt --all`
- lint: `cargo clippy --workspace --all-targets -- -D warnings`

**確定事項(参照: beads issue fulgur-chart-c19 の design / プラグインソース照合済み):**

QuickChart 既定値(ソース照合済み):
- radialGauge(pandameister/chartjs-chart-radial-gauge): `domain`=[0,100]、`trackColor`=rgb(204,221,238)、`centerPercentage`=80(→内径比 0.8)、`roundedCorners`=true、`centerArea.displayText`=true、開始 -90°(12時)から時計回り。
- gauge(haiiaaa/chartjs-gauge): `cutoutPercentage`=50(→内径比 0.5)、半円(circumference 180°)、`minValue`=0(max=`data` 末尾)、needle{lengthPercentage 80, widthPercentage 3.2, radiusPercentage 2, color 黒}、valueLabel{display true, color 白, backgroundColor 黒, borderRadius 5, padding 5}。

設計確定:
- JS の `valueLabel.formatter` / `centerArea.text` は実行せず `fmt_num(value.round())` で代替。
- 形状(roundedCorners 含む)は QuickChart に忠実に再現。
- gauge/radialGauge プラグインは互換ツール(tools/)に未インストールで chart.js golden 参照を生成できないため、テストは progressBar と同じ **snapshot + 不変条件**方式。
- title(options.plugins.title)と theme は他チャート同様サポート。legend は描画しない(LegendPos::None 相当)。

座標規約(SVG, y 下向き): 点 = (cx + R·cosθ, cy + R·sinθ)。θ=-90°(=−π/2) が 12 時、+方向は時計回り(画面上)。

---

## Task 1: ChartKind 2 変種追加 + 専用パス分岐 + レイアウト雛形(end-to-end で SVG が通る)

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`(`ChartKind` に `RadialGauge`/`Gauge` 追加)
- Modify: `crates/fulgur-chart/src/layout/mod.rs`(`pub mod gauge;` と dispatch 2 arm)
- Create: `crates/fulgur-chart/src/layout/gauge.rs`(最小 `build`: タイトルのみ)
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`parse()` 冒頭に gauge/radialGauge 分岐 + `parse_gauge`)
- Modify: `crates/fulgur-chart/src/model.rs`(`chart_type_name` に 2 arm)
- Test: `crates/fulgur-chart/tests/render_gauge.rs`(新規)

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/render_gauge.rs` を新規作成:

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&chartjs::parse(json, false).unwrap())
}

#[test]
fn radial_gauge_renders_svg() {
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[70]}]}}"#);
    assert!(
        svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"),
        "{svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}

#[test]
fn gauge_renders_svg() {
    let svg = render(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["green","yellow","red"]}]}}"#,
    );
    assert!(
        svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"),
        "{svg}"
    );
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: FAIL(`chartjs::parse` が `Err("未対応の type: radialGauge")` で `.unwrap()` panic)

**Step 3: Write minimal implementation**

(a) `ir.rs` の `ChartKind` enum、`Progress` の後に 2 変種を追加:

```rust
    /// QuickChart 互換の progress バー。軸なし水平バー。
    /// series[0].values=各バーの値、series.get(1).values=per-bar max(省略時100)。
    Progress,
    /// QuickChart radialGauge: 全円。値まで塗りつぶす弧 + トラック + 中央値テキスト。
    /// series[0].values[0]=値、series[0].fill[0]=塗り色。スカラ構造値はここに持つ。
    RadialGauge {
        min: f64,
        max: f64,
        track: Color,
        inner_ratio: f64, // centerPercentage/100
        rounded: bool,
        display_text: bool,
    },
    /// QuickChart gauge: 半円。color zone(series[0].values=累積閾値, series[0].fill=ゾーン色)
    /// + 針 + 値ラベル。value=針値、min=下端(max は閾値末尾)。
    Gauge {
        value: f64,
        min: f64,
        needle: Color,
        label: bool,         // valueLabel.display
        label_color: Color,  // valueLabel.color
        label_bg: Color,     // valueLabel.backgroundColor
    },
```

(b) `layout/gauge.rs` を新規作成(雛形):

```rust
//! gauge / radialGauge チャートのレイアウト: ChartSpec → Scene。
//! 軸なし。決定的に組み立て、NaN/Inf/panic を出さない。
//! すべての弧は standalone な空白区切り M/L/A/Z トークンで生成する
//! (raster_direct::parse_path_data 不変条件。pie.rs / progress.rs と同様)。

use super::common::{OUTER_PAD, TITLE_FONT};
use crate::ir::ChartSpec;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let mut items: Vec<Prim> = Vec::new();

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

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
```

(c) `layout/mod.rs` に module 宣言と dispatch を追加:

```rust
pub mod bar;
pub mod common;
pub mod gauge;   // ← 追加
pub mod line;
```

`build_scene` の match に arm を追加(`ChartKind::Progress` の後):

```rust
        ChartKind::Progress => progress::build(spec, m),
        ChartKind::RadialGauge { .. } | ChartKind::Gauge { .. } => gauge::build(spec, m),
```

(d) `model.rs::chart_type_name` の match に arm を追加(`ChartKind::Progress => "progress"` の後):

```rust
        ChartKind::Progress => "progress",
        ChartKind::RadialGauge { .. } => "radialGauge",
        ChartKind::Gauge { .. } => "gauge",
```

(e) `frontend/chartjs.rs`: `parse()` 冒頭の matrix 分岐ブロックの直後(matrix の `if chart_type.as_deref() == Some("matrix")` ブロックの後)に gauge 分岐を追加:

```rust
        if matches!(chart_type.as_deref(), Some("gauge") | Some("radialGauge")) {
            return parse_gauge(json, chart_type.as_deref() == Some("radialGauge"));
        }
```

> 注: matrix 分岐は内側スコープ `{ let chart_type = ...; if ... }` 内にある。同じスコープ内、matrix の `if` の直後にこの `if` を置く(`chart_type` 変数を再利用)。strict の未知キー検査は本タスクでは行わず、後続タスクで `check_unknown_keys_gauge` を足す(まずは end-to-end を通す)。

(f) `frontend/chartjs.rs` の末尾付近(`parse_matrix` の後)に `parse_gauge` を新規追加。本タスクでは最小実装(値・閾値・色・スカラを読み、ChartKind を構築):

```rust
fn parse_gauge(json: &str, radial: bool) -> Result<ChartSpec, String> {
    use crate::ir::ChartKind;

    #[derive(Deserialize)]
    struct GaugeWrapper {
        data: GaugeRawData,
        #[serde(default)]
        options: serde_json::Value,
    }
    #[derive(Deserialize)]
    struct GaugeRawData {
        datasets: Vec<GaugeRawDataset>,
    }
    #[derive(Deserialize)]
    struct GaugeRawDataset {
        #[serde(default)]
        value: Option<f64>,
        #[serde(rename = "minValue", default)]
        min_value: Option<f64>,
        #[serde(default)]
        data: Vec<f64>,
        #[serde(rename = "backgroundColor", default)]
        background_color: Option<ScalarOrArray<String>>,
    }

    let raw: GaugeWrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if raw.data.datasets.is_empty() {
        return Err("gauge チャートには dataset が 1 つ必要です".to_string());
    }
    let ds = raw.data.datasets.into_iter().next().unwrap();
    let opt = &raw.options;
    let theme = build_theme(None); // theme は Task 8 で options.theme 接続。まずは既定。

    // タイトル(options.plugins.title.display/text)。
    let title = opt
        .get("plugins")
        .and_then(|p| p.get("title"))
        .filter(|t| t.get("display").and_then(|d| d.as_bool()).unwrap_or(false))
        .and_then(|t| t.get("text").and_then(|s| s.as_str()).map(|s| s.to_string()));

    // 色解決ヘルパ(背景色配列を Color に)。
    let colors: Vec<crate::ir::Color> = ds
        .background_color
        .map(|c| c.into_vec())
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, s)| parse_color(s).unwrap_or_else(|| theme.palette[i % theme.palette.len()]))
        .collect();

    let (kind, values, fill) = if radial {
        // radialGauge: data[0]=値、color[0]=塗り色、domain/track/centerPercentage/...
        let value = ds.data.first().copied().unwrap_or(0.0);
        let domain = opt.get("domain").and_then(|d| d.as_array());
        let min = domain
            .and_then(|a| a.first())
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let max = domain
            .and_then(|a| a.get(1))
            .and_then(|v| v.as_f64())
            .unwrap_or(100.0);
        let track = opt
            .get("trackColor")
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color { r: 204, g: 221, b: 238, a: 1.0 });
        let center_pct = opt
            .get("centerPercentage")
            .and_then(|v| v.as_f64())
            .filter(|p| p.is_finite() && *p >= 0.0 && *p < 100.0)
            .unwrap_or(80.0);
        let rounded = opt
            .get("roundedCorners")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let display_text = opt
            .get("centerArea")
            .and_then(|c| c.get("displayText"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let fill = if colors.is_empty() {
            vec![theme.palette[0]]
        } else {
            vec![colors[0]]
        };
        (
            ChartKind::RadialGauge {
                min,
                max,
                track,
                inner_ratio: center_pct / 100.0,
                rounded,
                display_text,
            },
            vec![value],
            fill,
        )
    } else {
        // gauge: data=累積閾値、value=針、min=minValue、backgroundColor=ゾーン色。
        let value = ds.value.unwrap_or(0.0);
        let min = ds.min_value.unwrap_or(0.0);
        let needle = opt
            .get("needle")
            .and_then(|n| n.get("color"))
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color { r: 0, g: 0, b: 0, a: 1.0 });
        let vl = opt.get("valueLabel");
        let label = vl
            .and_then(|v| v.get("display"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let label_color = vl
            .and_then(|v| v.get("color"))
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color { r: 255, g: 255, b: 255, a: 1.0 });
        let label_bg = vl
            .and_then(|v| v.get("backgroundColor"))
            .and_then(|v| v.as_str())
            .and_then(parse_color)
            .unwrap_or(crate::ir::Color { r: 0, g: 0, b: 0, a: 1.0 });
        // ゾーン色が閾値数に満たなければパレットで補完。
        let n = ds.data.len();
        let fill: Vec<crate::ir::Color> = (0..n)
            .map(|i| {
                colors
                    .get(i)
                    .copied()
                    .unwrap_or(theme.palette[i % theme.palette.len()])
            })
            .collect();
        (
            ChartKind::Gauge {
                value,
                min,
                needle,
                label,
                label_color,
                label_bg,
            },
            ds.data.clone(),
            fill,
        )
    };

    let series = vec![Series {
        name: String::new(),
        values,
        points: vec![],
        fill,
        stroke: vec![],
        stroke_width: 0.0,
        area: false,
        tension: 0.0,
        series_type: SeriesType::Bar,
        point_radius: None,
    }];

    Ok(ChartSpec {
        kind,
        series,
        categories: vec![],
        x_axis: zero_axis(),
        y_axis: zero_axis(),
        legend: LegendPos::None,
        title,
        width: 800.0,
        height: 450.0,
        data_labels: false,
        theme,
    })
}

/// gauge 用の最小 AxisSpec(軸を使わないチャート向け)。
fn zero_axis() -> AxisSpec {
    AxisSpec {
        title: None,
        min: None,
        max: None,
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: false,
        grid: false,
    }
}
```

> 注: `zero_axis` は matrix が AxisSpec を直書きしているのと同じ値。重複を避けるためヘルパ化する(matrix もこのヘルパに差し替えてよいが、本タスクでは gauge 用にのみ追加し、リファクタは任意)。`ScalarOrArray`/`parse_color`/`build_theme`/`Series`/`SeriesType`/`LegendPos`/`AxisSpec` は同ファイル内で既に use 済み。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: PASS(`radial_gauge_renders_svg`, `gauge_renders_svg`)

`cargo build` で網羅 match のコンパイルエラーが出たら、`layout/mod.rs` と `model.rs::chart_type_name` の 2 箇所にのみ arm を追加すれば解消する(他は `_`/`matches!` で吸収)。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/layout/mod.rs \
        crates/fulgur-chart/src/layout/gauge.rs crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/src/model.rs crates/fulgur-chart/tests/render_gauge.rs \
        docs/plans/2026-06-21-gauge-radialgauge.md
git commit -m "feat(gauge): add RadialGauge/Gauge ChartKind, dedicated parse path, layout scaffold"
```

---

## Task 2: 弧パスヘルパ `ring_segment_path`(pure 関数・単体テスト)

**Files:**
- Modify: `crates/fulgur-chart/src/layout/gauge.rs`(`ring_segment_path` + `#[cfg(test)]`)

リング(内外半径ありの円弧帯)の塗りパス。pie.rs の doughnut スライス path を範に、gauge ゾーン・radialGauge トラック/値弧で共有する。

**Step 1: Write the failing test**

`gauge.rs` 末尾に追加:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    #[test]
    fn ring_segment_path_is_closed_and_clean() {
        let d = ring_segment_path(100.0, 100.0, 80.0, 40.0, -PI / 2.0, 0.0);
        assert!(d.starts_with('M'), "must start with moveto: {d}");
        assert!(d.ends_with('Z'), "must close: {d}");
        assert!(!d.contains("NaN") && !d.contains("inf"), "{d}");
    }

    #[test]
    fn ring_segment_path_uses_standalone_command_tokens() {
        // PNG 用 raster_direct::parse_path_data は split_ascii_whitespace で
        // トークン化し、スタンドアロンの M/L/A/Z しか解釈しない。
        let d = ring_segment_path(100.0, 100.0, 80.0, 40.0, -PI / 2.0, PI / 2.0);
        let tokens: Vec<&str> = d.split_ascii_whitespace().collect();
        assert!(tokens.iter().any(|t| *t == "M"), "{d}");
        assert_eq!(tokens.iter().filter(|t| **t == "A").count(), 2, "{d}");
        assert_eq!(tokens.iter().filter(|t| **t == "Z").count(), 1, "{d}");
    }

    #[test]
    fn ring_segment_path_deterministic() {
        let a = ring_segment_path(1.0, 2.0, 50.0, 25.0, 0.0, 1.0);
        let b = ring_segment_path(1.0, 2.0, 50.0, 25.0, 0.0, 1.0);
        assert_eq!(a, b);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart ring_segment_path`
Expected: FAIL(`ring_segment_path` 未定義のコンパイルエラー)

**Step 3: Write minimal implementation**

`gauge.rs` の `build` の下に追加。`use` に `fmt_num` と `PI`、`Color` を足す:

```rust
use crate::num::fmt_num;
use std::f64::consts::PI;
```

```rust
/// 内外半径ありの円弧帯(リングセグメント)の SVG path data。
/// a0→a1 を外弧(sweep 1)、a1→a0 を内弧(sweep 0)で閉じる。pie の doughnut と同形。
/// `a1 > a0` かつ `a1-a0 <= 2π` を前提(呼び出し側で保証)。
/// すべて fmt_num 整形 + 空白区切り(raster_direct 不変条件)。
fn ring_segment_path(cx: f64, cy: f64, r_outer: f64, r_inner: f64, a0: f64, a1: f64) -> String {
    let laf = if (a1 - a0) > PI { 1 } else { 0 };
    let o0 = (cx + r_outer * a0.cos(), cy + r_outer * a0.sin());
    let o1 = (cx + r_outer * a1.cos(), cy + r_outer * a1.sin());
    let i0 = (cx + r_inner * a0.cos(), cy + r_inner * a0.sin());
    let i1 = (cx + r_inner * a1.cos(), cy + r_inner * a1.sin());
    format!(
        "M {} {} A {} {} 0 {} 1 {} {} L {} {} A {} {} 0 {} 0 {} {} Z",
        fmt_num(o0.0),
        fmt_num(o0.1),
        fmt_num(r_outer),
        fmt_num(r_outer),
        laf,
        fmt_num(o1.0),
        fmt_num(o1.1),
        fmt_num(i1.0),
        fmt_num(i1.1),
        fmt_num(r_inner),
        fmt_num(r_inner),
        laf,
        fmt_num(i0.0),
        fmt_num(i0.1),
    )
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart ring_segment_path`
Expected: PASS(3 tests)

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/gauge.rs
git commit -m "feat(gauge): add deterministic ring_segment_path helper"
```

---

## Task 3: radialGauge 本体(トラックリング + 値弧 + クランプ)

**Files:**
- Modify: `crates/fulgur-chart/src/layout/gauge.rs`(`build` を radialGauge 分岐で実装)
- Test: `crates/fulgur-chart/tests/render_gauge.rs`

**Step 1: Write the failing test**

`render_gauge.rs` に追加:

```rust
fn count(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

#[test]
fn radial_gauge_has_track_and_value_arc() {
    // トラックリング(全周) + 値弧 = path 2 以上。色も両方出る。
    let svg = render(
        r##"{"type":"radialGauge","data":{"datasets":[{"data":[70],"backgroundColor":"#ff0000"}]}}"##,
    );
    assert!(count(&svg, "<path") >= 2, "track + value arc: {svg}");
    assert!(svg.contains("#ff0000") || svg.contains("rgb"), "value color: {svg}");
}

#[test]
fn radial_gauge_zero_value_track_only() {
    // value=min(0) → 値弧 sweep 0、トラックのみ。NaN/inf なし。
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[0]}]}}"#);
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
    assert!(count(&svg, "<path") >= 1, "{svg}");
}

#[test]
fn radial_gauge_clamps_over_domain() {
    // domain 既定 [0,100]、value=150 → クランプして panic/NaN なし。
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[150]}]}}"#);
    assert!(!svg.contains("NaN") && !svg.contains("inf"), "{svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_gauge radial_gauge_has_track`
Expected: FAIL(雛形は path を描かないため `>= 2` を満たさない)

**Step 3: Write minimal implementation**

`gauge.rs` の `build` を、ChartKind で分岐する構成に変更。radialGauge 分岐を実装。`use` に `ChartKind`, `Color` を追加:

```rust
use crate::ir::{ChartKind, ChartSpec, Color};
```

`build` を以下に置き換え(タイトル描画は共通で先頭、その後 kind 分岐):

```rust
pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ink = spec.theme.text_color;
    let mut items: Vec<Prim> = Vec::new();

    let title_band = if spec.title.is_some() {
        super::common::TITLE_BAND
    } else {
        0.0
    };
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

    match &spec.kind {
        ChartKind::RadialGauge {
            min,
            max,
            track,
            inner_ratio,
            rounded,
            display_text,
        } => build_radial(
            &mut items, spec, title_band, *min, *max, *track, *inner_ratio, *rounded,
            *display_text,
        ),
        ChartKind::Gauge {
            value,
            min,
            needle,
            label,
            label_color,
            label_bg,
        } => build_semi(
            &mut items, spec, m, title_band, *value, *min, *needle, *label, *label_color,
            *label_bg,
        ),
        _ => {}
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

/// プロット領域の中心と半径(タイトル帯を除いた領域に内接)。
fn area_geom(spec: &ChartSpec, title_band: f64) -> (f64, f64, f64) {
    let area_top = OUTER_PAD + title_band;
    let area_bottom = spec.height - OUTER_PAD;
    let area_left = OUTER_PAD;
    let area_right = spec.width - OUTER_PAD;
    let cx = (area_left + area_right) / 2.0;
    let cy = (area_top + area_bottom) / 2.0;
    let r = ((area_right - area_left).min(area_bottom - area_top) / 2.0 * 0.9).max(0.0);
    (cx, cy, r)
}

#[allow(clippy::too_many_arguments)]
fn build_radial(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    title_band: f64,
    min: f64,
    max: f64,
    track: Color,
    inner_ratio: f64,
    _rounded: bool,    // Task 4 で使用
    _display_text: bool, // Task 4 で使用
) {
    let (cx, cy, r_outer) = area_geom(spec, title_band);
    let r_inner = (r_outer * inner_ratio).clamp(0.0, r_outer);
    if r_outer <= 0.0 {
        return;
    }
    let fill = spec.series.first().map(|s| s.fill_at(0)).unwrap_or(track);
    let value = spec
        .series
        .first()
        .and_then(|s| s.values.first().copied())
        .unwrap_or(min);

    // 値の割合(domain でスケール・クランプ)。range が 0 のとき 0。
    let frac = if (max - min).abs() > f64::EPSILON {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let start = -PI / 2.0; // 12 時。
    // トラック: 全周リング。全周は単一 A で描けないため中点で 2 分割。
    let mid = start + PI;
    items.push(Prim::Path {
        d: ring_segment_path(cx, cy, r_outer, r_inner, start, mid),
        fill: Some(track),
        stroke: None,
        stroke_width: 0.0,
    });
    items.push(Prim::Path {
        d: ring_segment_path(cx, cy, r_outer, r_inner, mid, start + 2.0 * PI),
        fill: Some(track),
        stroke: None,
        stroke_width: 0.0,
    });

    // 値弧: start から時計回りに frac×360°。frac>0 のみ。
    if frac > 0.0 {
        let end = start + frac * 2.0 * PI;
        // 半周超は単一 A で描けるが large-arc-flag は ring_segment_path が処理する。
        // 全周(frac==1)は 2 分割。
        if frac >= 1.0 - 1e-9 {
            let amid = start + PI;
            items.push(Prim::Path {
                d: ring_segment_path(cx, cy, r_outer, r_inner, start, amid),
                fill: Some(fill),
                stroke: None,
                stroke_width: 0.0,
            });
            items.push(Prim::Path {
                d: ring_segment_path(cx, cy, r_outer, r_inner, amid, start + 2.0 * PI),
                fill: Some(fill),
                stroke: None,
                stroke_width: 0.0,
            });
        } else {
            items.push(Prim::Path {
                d: ring_segment_path(cx, cy, r_outer, r_inner, start, end),
                fill: Some(fill),
                stroke: None,
                stroke_width: 0.0,
            });
        }
    }
}
```

gauge 分岐は Task 5 で実装するため、本タスクでは `build_semi` の最小スタブを置く:

```rust
#[allow(clippy::too_many_arguments)]
fn build_semi(
    _items: &mut [Prim],
    _spec: &ChartSpec,
    _m: &TextMeasurer,
    _title_band: f64,
    _value: f64,
    _min: f64,
    _needle: Color,
    _label: bool,
    _label_color: Color,
    _label_bg: Color,
) {
    // Task 5/6 で実装。
}
```

> 注: `build_semi` は本タスクではスタブ。`_items: &mut [Prim]` でなく `&mut Vec<Prim>` にしておくと Task 5 で push できる。引数型は `&mut Vec<Prim>` に統一すること。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: PASS(radialGauge の 3 テスト + Task 1 の 2 テスト)

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/gauge.rs crates/fulgur-chart/tests/render_gauge.rs
git commit -m "feat(gauge): radialGauge track ring + clamped value arc"
```

---

## Task 4: radialGauge の roundedCorners + 中央値テキスト

**Files:**
- Modify: `crates/fulgur-chart/src/layout/gauge.rs`(`build_radial` に rounded キャップ + 中央テキスト)
- Test: `crates/fulgur-chart/tests/render_gauge.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn radial_gauge_shows_center_value_by_default() {
    // displayText 既定 true → 中央に丸めた値テキスト。
    let svg = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[72]}]}}"#);
    assert!(svg.contains(">72<"), "center value missing: {svg}");
}

#[test]
fn radial_gauge_center_text_hidden_when_disabled() {
    let svg = render(
        r#"{"type":"radialGauge","data":{"datasets":[{"data":[72]}],
        "options":{"centerArea":{"displayText":false}}}"#,
    );
    assert!(!svg.contains(">72<"), "center value should be hidden: {svg}");
}

#[test]
fn radial_gauge_rounded_default_adds_caps() {
    // roundedCorners 既定 true → 値弧の両端に半円キャップ(<circle>)が出る。
    // flat(false)指定時はキャップなし(radialGauge は針なし=他に circle 無し)。
    // 値が中間(両端が露出)で比較。キャップは Prim::Circle → <circle> 要素。
    let rounded = render(r#"{"type":"radialGauge","data":{"datasets":[{"data":[50]}]}}"#);
    let flat = render(
        r#"{"type":"radialGauge","data":{"datasets":[{"data":[50]}],
        "options":{"roundedCorners":false}}}"#,
    );
    assert!(
        rounded.matches("<circle").count() > flat.matches("<circle").count(),
        "rounded should add cap circles: rounded={} flat={}",
        rounded.matches("<circle").count(),
        flat.matches("<circle").count()
    );
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_gauge radial_gauge_shows_center`
Expected: FAIL(中央テキスト未描画 / rounded 未実装)

**Step 3: Write minimal implementation**

`build_radial` の `_rounded`/`_display_text` を有効化。値弧描画を、rounded のとき端に半円キャップを足す版に置き換え、末尾に中央テキストを追加。

(a) キャップ付き値弧ヘルパを追加:

```rust
/// 値弧を描く。rounded のとき両端に半円キャップ(Circle 代用に小さなリング無しの
/// 半円 path)を足す。簡易には端点に直径=帯幅の円を描く。
fn push_value_arc(
    items: &mut Vec<Prim>,
    cx: f64,
    cy: f64,
    r_outer: f64,
    r_inner: f64,
    a0: f64,
    a1: f64,
    fill: Color,
    rounded: bool,
) {
    items.push(Prim::Path {
        d: ring_segment_path(cx, cy, r_outer, r_inner, a0, a1),
        fill: Some(fill),
        stroke: None,
        stroke_width: 0.0,
    });
    if rounded {
        let cap_r = (r_outer - r_inner) / 2.0;
        let mid_r = (r_outer + r_inner) / 2.0;
        for a in [a0, a1] {
            items.push(Prim::Circle {
                cx: cx + mid_r * a.cos(),
                cy: cy + mid_r * a.sin(),
                r: cap_r.max(0.0),
                fill,
            });
        }
    }
}
```

> 注: `Prim::Circle` は SVG `<circle>` + PNG 直接ラスタライザ双方で描画される(scatter で使用実績あり)。rounded キャップを円で近似することで弧端が丸く見える。クランプで cap_r>=0 を保証。

(b) `build_radial` 内の値弧描画ブロックを `push_value_arc` 呼び出しに差し替え:

```rust
    if frac > 0.0 {
        let end = start + frac * 2.0 * PI;
        if frac >= 1.0 - 1e-9 {
            let amid = start + PI;
            push_value_arc(items, cx, cy, r_outer, r_inner, start, amid, fill, false);
            push_value_arc(items, cx, cy, r_outer, r_inner, amid, start + 2.0 * PI, fill, false);
        } else {
            push_value_arc(items, cx, cy, r_outer, r_inner, start, end, fill, _rounded);
        }
    }
```

> 注: 全周(frac==1)はキャップ不要(始終点が一致)なので rounded=false で呼ぶ。

(c) `build_radial` 末尾に中央テキスト:

```rust
    if _display_text {
        items.push(Prim::Text {
            x: cx,
            y: cy + spec.theme.font_size * super::common::TEXT_BASELINE_RATIO,
            size: spec.theme.font_size * 1.6, // 中央値は大きめ。
            anchor: Anchor::Middle,
            fill: spec.theme.text_color,
            content: fmt_num(value.round()),
        });
    }
```

`_rounded`/`_display_text` の先頭アンダースコアを外し `rounded`/`display_text` にリネーム(引数名も)。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/gauge.rs crates/fulgur-chart/tests/render_gauge.rs
git commit -m "feat(gauge): radialGauge rounded caps + center value text"
```

---

## Task 5: gauge 半円ゾーン + 針

**Files:**
- Modify: `crates/fulgur-chart/src/layout/gauge.rs`(`build_semi` 実装)
- Test: `crates/fulgur-chart/tests/render_gauge.rs`

半円: t∈[0,1] に対し θ(t) = π + t·π(左 9 時→上→右 3 時)。内径比 0.5(cutout 50%)。針支点は (cx, cy)。

**Step 1: Write the failing test**

```rust
#[test]
fn gauge_renders_one_path_per_zone() {
    // data=[2,4,6] → 3 ゾーン。各ゾーン 1 path + 針。ゾーン path は 3 つ以上。
    let svg = render(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"#,
    );
    assert!(count(&svg, "<path") >= 3, "3 zones: {svg}");
    assert!(svg.contains("#00ff00") && svg.contains("#ff0000"), "zone colors: {svg}");
}

#[test]
fn gauge_needle_present() {
    // 針(三角形 path or polygon)が描かれる。針色 黒(既定)。
    let svg = render(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"#,
    );
    // 針は polygon/path。最低限 path 数がゾーン数より多い(針を足した分)。
    assert!(count(&svg, "<path") >= 4, "needle adds a path: {svg}");
}

#[test]
fn gauge_no_panic_on_empty_zones() {
    let svg = render(r#"{"type":"gauge","data":{"datasets":[{"value":0,"data":[]}]}}"#);
    assert!(svg.starts_with("<svg") && !svg.contains("NaN"), "{svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_gauge gauge_renders_one_path`
Expected: FAIL(`build_semi` スタブで何も描かない)

**Step 3: Write minimal implementation**

`build_semi` を実装。引数型を `items: &mut Vec<Prim>` に統一済みであること。

```rust
#[allow(clippy::too_many_arguments)]
fn build_semi(
    items: &mut Vec<Prim>,
    spec: &ChartSpec,
    _m: &TextMeasurer,
    title_band: f64,
    value: f64,
    min: f64,
    needle: Color,
    label: bool,
    label_color: Color,
    label_bg: Color,
) {
    // 半円は縦に半分しか使わないため、領域の下端を支点に取り直す。
    let area_top = OUTER_PAD + title_band;
    let area_bottom = spec.height - OUTER_PAD;
    let area_left = OUTER_PAD;
    let area_right = spec.width - OUTER_PAD;
    // 半円の幅基準半径(横幅の半分)と高さ基準で小さい方。
    let r_outer = (((area_right - area_left) / 2.0).min(area_bottom - area_top) * 0.9).max(0.0);
    let cx = (area_left + area_right) / 2.0;
    // 支点 cy は半円の底辺。中央やや下に置く。
    let cy = (area_top + area_bottom) / 2.0 + r_outer / 2.0;
    let r_inner = r_outer * 0.5; // cutout 50%。
    if r_outer <= 0.0 {
        return;
    }

    let series = spec.series.first();
    let thresholds: &[f64] = series.map(|s| s.values.as_slice()).unwrap_or(&[]);
    if thresholds.is_empty() {
        return;
    }
    // max = 閾値末尾(有限)。min との縮退は range=1 で防御。
    let max = thresholds
        .iter()
        .rev()
        .find(|v| v.is_finite())
        .copied()
        .unwrap_or(min + 1.0);
    let range = if (max - min).abs() > f64::EPSILON {
        max - min
    } else {
        1.0
    };
    let angle = |frac: f64| PI + frac.clamp(0.0, 1.0) * PI;

    // ゾーン: 各閾値境界を角度に変換し帯を塗る。
    let mut lo = min;
    for (i, &thr) in thresholds.iter().enumerate() {
        if !thr.is_finite() {
            continue;
        }
        let hi = thr;
        let a0 = angle((lo - min) / range);
        let a1 = angle((hi - min) / range);
        if a1 > a0 {
            let fill = series.map(|s| s.fill_at(i)).unwrap_or(needle);
            items.push(Prim::Path {
                d: ring_segment_path(cx, cy, r_outer, r_inner, a0, a1),
                fill: Some(fill),
                stroke: None,
                stroke_width: 0.0,
            });
        }
        lo = hi;
    }

    // 針: 支点から value 角へ向かう三角形 + 支点の小円。
    let va = angle((value - min) / range);
    let needle_len = r_outer * 0.8;
    let tip = (cx + needle_len * va.cos(), cy + needle_len * va.sin());
    // 支点で針幅を取るための直交方向。
    let half_w = (r_outer * 0.032).max(1.5); // widthPercentage 3.2。
    let perp = va + PI / 2.0;
    let base1 = (cx + half_w * perp.cos(), cy + half_w * perp.sin());
    let base2 = (cx - half_w * perp.cos(), cy - half_w * perp.sin());
    items.push(Prim::Path {
        d: format!(
            "M {} {} L {} {} L {} {} Z",
            fmt_num(tip.0),
            fmt_num(tip.1),
            fmt_num(base1.0),
            fmt_num(base1.1),
            fmt_num(base2.0),
            fmt_num(base2.1),
        ),
        fill: Some(needle),
        stroke: None,
        stroke_width: 0.0,
    });
    items.push(Prim::Circle {
        cx,
        cy,
        r: (r_outer * 0.04).max(2.0), // radiusPercentage 2。
        fill: needle,
    });

    // 値ラベルは Task 6。
    let _ = (label, label_color, label_bg);
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/gauge.rs crates/fulgur-chart/tests/render_gauge.rs
git commit -m "feat(gauge): semicircle color zones + needle"
```

---

## Task 6: gauge 値ラベル(角丸背景 + 丸めた値)

**Files:**
- Modify: `crates/fulgur-chart/src/layout/gauge.rs`(値ラベル描画 + `rounded_rect_path` 共有)
- Modify: `crates/fulgur-chart/src/layout/progress.rs`(`rounded_rect_path` を `pub(crate)` に)
- Test: `crates/fulgur-chart/tests/render_gauge.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn gauge_shows_value_label_by_default() {
    let svg = render(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"#,
    );
    assert!(svg.contains(">3<"), "value label missing: {svg}");
}

#[test]
fn gauge_value_label_hidden_when_disabled() {
    let svg = render(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6]}],
        "options":{"valueLabel":{"display":false}}}"#,
    );
    assert!(!svg.contains(">3<"), "value label should be hidden: {svg}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_gauge gauge_shows_value_label`
Expected: FAIL(ラベル未描画)

**Step 3: Write minimal implementation**

(a) `progress.rs` の `rounded_rect_path` を共有するため可視性を上げる:

```rust
// 変更前: fn rounded_rect_path(...)
pub(crate) fn rounded_rect_path(x: f64, y: f64, w: f64, h: f64, r: f64) -> String {
```

(b) `gauge.rs` の `build_semi` 末尾、`let _ = (label, label_color, label_bg);` を置き換え:

```rust
    if label {
        let text = fmt_num(value.round());
        let font = spec.theme.font_size;
        // 概算ラベル幅(等幅近似)。TextMeasurer を使わず決定的に: 1 文字 ≈ font*0.6。
        let text_w = text.chars().count() as f64 * font * 0.6;
        let pad = 5.0;
        let box_w = text_w + pad * 2.0;
        let box_h = font + pad * 2.0;
        // 支点直下に配置。
        let box_x = cx - box_w / 2.0;
        let box_y = cy + r_outer * 0.12;
        items.push(Prim::Path {
            d: crate::layout::progress::rounded_rect_path(box_x, box_y, box_w, box_h, 5.0),
            fill: Some(label_bg),
            stroke: None,
            stroke_width: 0.0,
        });
        items.push(Prim::Text {
            x: cx,
            y: box_y + box_h / 2.0 + font * super::common::TEXT_BASELINE_RATIO,
            size: font,
            anchor: Anchor::Middle,
            fill: label_color,
            content: text,
        });
    }
```

> 注: `_m` 引数(TextMeasurer)を使ってラベル幅を正確に測る選択肢もあるが、本実装は決定的近似(font*0.6)で十分。`_m` はそのまま未使用で可(`_` プレフィクス維持)。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/gauge.rs crates/fulgur-chart/src/layout/progress.rs \
        crates/fulgur-chart/tests/render_gauge.rs
git commit -m "feat(gauge): value label with rounded background"
```

---

## Task 7: エッジケース + 決定性 + スナップショット + PNG 回帰

**Files:**
- Test: `crates/fulgur-chart/tests/render_gauge.rs`
- Create(テスト初回実行で生成): `crates/fulgur-chart/tests/snapshots/render_gauge__*.snap`

**Step 1: Write the failing test**

```rust
#[test]
fn gauge_deterministic() {
    let j = r##"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4,6],
        "backgroundColor":["#00ff00","#ffff00","#ff0000"]}]}}"##;
    assert_eq!(render(j), render(j));
}

#[test]
fn radial_gauge_deterministic() {
    let j = r##"{"type":"radialGauge","data":{"datasets":[{"data":[63],"backgroundColor":"#36a2eb"}]}}"##;
    assert_eq!(render(j), render(j));
}

#[test]
fn radial_gauge_snapshot() {
    let svg = render(
        r##"{"type":"radialGauge","data":{"datasets":[{"data":[63],"backgroundColor":"#36a2eb"}]},
        "options":{"plugins":{"title":{"display":true,"text":"CPU"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn gauge_snapshot() {
    let svg = render(
        r##"{"type":"gauge","data":{"datasets":[{"value":58,"minValue":0,"data":[33,66,100],
        "backgroundColor":["#4caf50","#ffc107","#f44336"]}]},
        "options":{"plugins":{"title":{"display":true,"text":"Load"}}}}"##,
    );
    insta::assert_snapshot!(svg);
}

#[test]
fn radial_gauge_rasterizes_value_color_in_png() {
    // 回帰: 弧パスが PNG 直接ラスタライザで描画される。前景色ピクセルが PNG に現れる。
    use resvg::tiny_skia::Pixmap;
    let spec = chartjs::parse(
        r##"{"type":"radialGauge","data":{"datasets":[{"data":[100],"backgroundColor":"#36a2eb"}]},
        "options":{"roundedCorners":false}}"##,
        false,
    )
    .unwrap();
    let png = fulgur_chart::raster_direct::render_chart_to_png(
        &spec,
        1.0,
        fulgur_chart::font::DEFAULT_FONT,
    )
    .unwrap();
    let pixmap = Pixmap::decode_png(&png).unwrap();
    let found = pixmap
        .pixels()
        .iter()
        .any(|p| p.red() == 54 && p.green() == 162 && p.blue() == 235);
    assert!(found, "radialGauge value arc (#36a2eb) must be rasterized into the PNG");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: 決定性/PNG は PASS のはず。`*_snapshot` は新規スナップショットで FAIL(保留中)。

**Step 3: Accept snapshots**

```bash
INSTA_UPDATE=always cargo test -p fulgur-chart --test render_gauge radial_gauge_snapshot gauge_snapshot
```

生成された `.snap` に `NaN`/`inf` が無いこと、radialGauge は track 2 + 値弧 + 中央テキスト、gauge は 3 ゾーン + 針 + 値ラベルが含まれることを目視確認する。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_gauge`
Expected: PASS(全 gauge テスト)

**Step 5: Commit**

```bash
git add crates/fulgur-chart/tests/render_gauge.rs crates/fulgur-chart/tests/snapshots/
git commit -m "test(gauge): edge cases, determinism, snapshots, PNG regression"
```

---

## Task 8: strict 未知キー検査 + options.theme 接続

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`check_unknown_keys_gauge` + theme 接続 + strict 分岐)
- Test: `crates/fulgur-chart-cli/tests/cli.rs` または `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/render_gauge.rs` に追加(strict は parse 経由で確認):

```rust
#[test]
fn gauge_strict_rejects_unknown_key() {
    let err = chartjs::parse(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4],"bogus":1}]}}"#,
        true,
    );
    assert!(err.is_err(), "strict should reject unknown dataset key");
}

#[test]
fn gauge_strict_accepts_known_keys() {
    let ok = chartjs::parse(
        r#"{"type":"gauge","data":{"datasets":[{"value":3,"data":[2,4],
        "backgroundColor":["#0f0","#f00"]}]},
        "options":{"needle":{"color":"#000"},"valueLabel":{"display":true}}}"#,
        true,
    );
    assert!(ok.is_ok(), "known keys should pass strict: {ok:?}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test render_gauge gauge_strict`
Expected: `gauge_strict_rejects_unknown_key` は FAIL(現状 strict 検査をスキップしているため未知キーが通る)。

**Step 3: Write minimal implementation**

(a) Task 1 で追加した分岐に strict 検査を足す:

```rust
        if matches!(chart_type.as_deref(), Some("gauge") | Some("radialGauge")) {
            let radial = chart_type.as_deref() == Some("radialGauge");
            if strict {
                check_unknown_keys_gauge(json, radial)?;
            }
            return parse_gauge(json, radial);
        }
```

(b) `check_unknown_keys_gauge` を `check_unknown_keys_matrix` を範に追加。許可キー:
- top: `type`, `data`, `options`
- data: `datasets`
- dataset: `label`, `value`, `minValue`, `data`, `backgroundColor`, `borderColor`, `borderWidth`
- options: `domain`, `trackColor`, `centerPercentage`, `roundedCorners`, `centerArea`, `needle`, `valueLabel`, `plugins`, `theme`
- options.plugins: `title`, `legend`
- options.centerArea: `displayText`, `fontSize`, `fontColor`, `text`, `subText`, `padding`
- options.needle: `color`, `radiusPercentage`, `widthPercentage`, `lengthPercentage`
- options.valueLabel: `display`, `formatter`, `color`, `backgroundColor`, `borderRadius`, `padding`, `bottomMarginPercentage`, `fontSize`
- options.theme: `palette`, `gridColor`, `textColor`, `backgroundColor`, `fontSize`

```rust
fn check_unknown_keys_gauge(json: &str, _radial: bool) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(
                        ds,
                        &[
                            "label", "value", "minValue", "data",
                            "backgroundColor", "borderColor", "borderWidth",
                        ],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(
            options,
            &[
                "domain", "trackColor", "centerPercentage", "roundedCorners",
                "centerArea", "needle", "valueLabel", "plugins", "theme",
            ],
            "options",
        )?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(plugins, &["title", "legend"], "options.plugins")?;
        }
        if let Some(ca) = options.get("centerArea").and_then(|v| v.as_object()) {
            check_object(
                ca,
                &["displayText", "fontSize", "fontColor", "text", "subText", "padding"],
                "options.centerArea",
            )?;
        }
        if let Some(nd) = options.get("needle").and_then(|v| v.as_object()) {
            check_object(
                nd,
                &["color", "radiusPercentage", "widthPercentage", "lengthPercentage"],
                "options.needle",
            )?;
        }
        if let Some(vl) = options.get("valueLabel").and_then(|v| v.as_object()) {
            check_object(
                vl,
                &[
                    "display", "formatter", "color", "backgroundColor",
                    "borderRadius", "padding", "bottomMarginPercentage", "fontSize",
                ],
                "options.valueLabel",
            )?;
        }
        if let Some(theme) = options.get("theme").and_then(|v| v.as_object()) {
            check_object(
                theme,
                &["palette", "gridColor", "textColor", "backgroundColor", "fontSize"],
                "options.theme",
            )?;
        }
    }
    Ok(())
}
```

(c) `parse_gauge` の `build_theme(None)` を `options.theme` 接続に変更。`options` は `serde_json::Value` なので、`RawTheme` に逐次デシリアライズして渡す:

```rust
    let raw_theme: Option<RawTheme> = opt
        .get("theme")
        .and_then(|t| serde_json::from_value(t.clone()).ok());
    let theme = build_theme(raw_theme);
```

> `RawTheme` は同ファイルで定義済み(`#[derive(Deserialize)]`)。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test render_gauge gauge_strict`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/tests/render_gauge.rs
git commit -m "feat(gauge): strict unknown-key validation + options.theme support"
```

---

## Task 9: JSON Schema に gauge / radialGauge 変種追加

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`(`ChartJsSpec` enum + 型)
- Test: `crates/fulgur-chart-cli/tests/cli.rs`

**Step 1: Write the failing test**

`crates/fulgur-chart-cli/tests/cli.rs` に追加(既存 `schema_chartjs_includes_progress` を範に):

```rust
#[test]
fn schema_chartjs_includes_gauge() {
    let out = Command::cargo_bin("fulgur-chart")
        .unwrap()
        .args(["schema"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains("gauge"), "schema should mention gauge");
    assert!(s.contains("radialGauge"), "schema should mention radialGauge");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart-cli --test cli schema_chartjs_includes_gauge`
Expected: FAIL(schema に gauge/radialGauge 無し)

**Step 3: Write minimal implementation**

(a) `ChartJsSpec` enum に変種追加(`Progress` の後)。radialGauge は `rename_all="lowercase"` だと "radialgauge" になるため明示 rename:

```rust
    #[serde(alias = "progressBar")]
    Progress(ProgressSpec),
    Gauge(GaugeSpec),
    #[serde(rename = "radialGauge")]
    RadialGauge(RadialGaugeSpec),
}
```

(b) Progress 型の後に gauge/radialGauge 型を追加:

```rust
// ────────────────────────────────────────────────
// Gauge chart (QuickChart chartjs-gauge: semicircle, zones + needle)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GaugeSpec {
    pub data: GaugeData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<GaugeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GaugeData {
    pub datasets: Vec<GaugeDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GaugeDataset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Needle value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// Domain minimum (default 0; max = last threshold).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f64>,
    /// Cumulative zone thresholds.
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GaugeOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needle: Option<NeedleOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_label: Option<ValueLabelOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NeedleOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub length_percentage: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValueLabelOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ColorString>,
}

// ────────────────────────────────────────────────
// Radial gauge chart (QuickChart radial-gauge: full circle, fill-to-value)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadialGaugeSpec {
    pub data: RadialGaugeData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<RadialGaugeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadialGaugeData {
    pub datasets: Vec<RadialGaugeDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadialGaugeDataset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Single value [value].
    pub data: Vec<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<ScalarOrArray<ColorString>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadialGaugeOptions {
    /// [min, max] domain (default [0, 100]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Vec<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rounded_corners: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub center_area: Option<CenterAreaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CenterAreaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
}
```

> 注: `CommonPlugins`/`ThemeOptions`/`ColorString`/`ScalarOrArray` は `super::common` から既に import 済み(必要なら use に追記)。schema は `schema` サブコマンドの JSON Schema 出力専用で、実パースは `frontend/chartjs.rs` が担う(両者のキー名を一致させる)。

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart-cli --test cli schema_chartjs`
Expected: PASS(`schema_chartjs_is_valid_json` と `schema_chartjs_includes_gauge` 両方)

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs crates/fulgur-chart-cli/tests/cli.rs
git commit -m "feat(gauge): add Gauge/RadialGauge variants to JSON schema"
```

---

## Task 10: example spec + README + CHANGELOG + 最終品質ゲート

**Files:**
- Create: `examples/specs/gauge.json`, `examples/specs/radial-gauge.json`
- Modify: `README.md`、`CHANGELOG.md`

**Step 1: example spec を作成**

`examples/specs/radial-gauge.json`:

```json
{
  "type": "radialGauge",
  "data": { "datasets": [{ "data": [63], "backgroundColor": "#36a2eb" }] },
  "options": {
    "domain": [0, 100],
    "centerArea": { "displayText": true },
    "plugins": { "title": { "display": true, "text": "CPU Usage" } }
  }
}
```

`examples/specs/gauge.json`:

```json
{
  "type": "gauge",
  "data": {
    "datasets": [{
      "value": 58,
      "minValue": 0,
      "data": [33, 66, 100],
      "backgroundColor": ["#4caf50", "#ffc107", "#f44336"]
    }]
  },
  "options": {
    "needle": { "color": "#333" },
    "valueLabel": { "display": true },
    "plugins": { "title": { "display": true, "text": "System Load" } }
  }
}
```

CLI で描画して目視確認:

```bash
cargo run -p fulgur-chart-cli -- render examples/specs/radial-gauge.json -o /tmp/radial.svg
cargo run -p fulgur-chart-cli -- render examples/specs/gauge.json -o /tmp/gauge.svg
head -c 120 /tmp/radial.svg; echo; head -c 120 /tmp/gauge.svg
```

Expected: 両者 `<svg ...>` で始まる。

**Step 1.5: 視覚検証(QuickChart 忠実度の最終確認 — 必須)**

snapshot は「出力した内容」を凍結するだけで、上下逆さ・ゾーン反転・針外れ等を検出しない。
「QuickChart に倣う」の本体はここで検証する。SVG 構造チェックだけで done を主張しない。

(a) 両 example を PNG に描画し、**画像として目視**する(Read ツールは PNG を表示する):

```bash
cargo run -p fulgur-chart-cli -- render examples/specs/radial-gauge.json -o /tmp/radial.png
cargo run -p fulgur-chart-cli -- render examples/specs/gauge.json -o /tmp/gauge.png
```

→ Read `/tmp/radial.png` と `/tmp/gauge.png` で実際の見た目を確認する。

(b) QuickChart.io 実物(本物のプラグイン出力)と比較する。同一 config を URL エンコードして:

```
https://quickchart.io/chart?c=<同じJSON>
```

を WebFetch/取得し、向き・ゾーン配置・針/中央値の位置を突き合わせる。

確認ポイント:
- radialGauge: 値弧が 12 時から時計回り、中央に値、トラックが残部リング。
- gauge: 半円が**上半分**(平らな底辺が下)、ゾーンが左→右、針が value を指す、値ラベルが中央下。
- **特に半円の向き**: chartjs-gauge の rotation:-π/circumference:π は解釈が曖昧。実物と逆さ/左右反転していれば `angle(frac)` の式(θ=π+frac·π)と cy オフセット符号を修正する。

ずれがあれば該当 Task に戻って幾何を直し、snapshot を取り直す。

**Step 2: README 更新**

「Supported chart types」リストに追加:

```markdown
- Gauge chart (QuickChart-style; semicircle with colored zones, needle, value label)
- Radial gauge chart (QuickChart-style; full circle fill-to-value with center value text)
```

「Supported chart.js subset」の type 行に `gauge` / `radialGauge` を追記:

```markdown
- `type` — `bar` / `line` / `pie` / `doughnut` / `scatter` / `bubble` / `radar` / `progress` / `gauge` / `radialGauge`
```

（必要なら 1〜2 行説明: gauge は `datasets[0].data`=累積閾値・`value`=針値・`backgroundColor`=ゾーン色、`options.domain`/`needle`/`valueLabel`。radialGauge は `data`=単一値・`options.domain`/`trackColor`/`centerPercentage`/`roundedCorners`。JS formatter は丸めた数値で代替。）

**Step 3: CHANGELOG 更新**

最新節(Unreleased 等)に追加:

```markdown
- Add `gauge` chart type (QuickChart chartjs-gauge): semicircle colored zones from
  cumulative thresholds, needle pointing at the value, and a value label.
- Add `radialGauge` chart type (QuickChart radial-gauge): full-circle fill-to-value
  arc on a track ring with optional rounded caps and center value text.
```

**Step 4: 最終品質ゲート(全部グリーンであること)**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: fmt 差分なし、clippy 警告ゼロ、全テスト PASS。

**Step 5: Commit**

```bash
git add examples/specs/gauge.json examples/specs/radial-gauge.json README.md CHANGELOG.md
git commit -m "docs(gauge): add example specs, README and CHANGELOG entries"
```

---

## 完了基準(acceptance, issue fulgur-chart-c19 より)

- `type:"radialGauge"` が全円の塗りつぶし弧 + トラック + 中央値テキストで SVG/PNG 描画される。domain でスケールし value をクランプ。roundedCorners で弧端が丸い。
- `type:"gauge"` が半円の色帯ゾーン(data 累積閾値 + backgroundColor 配列)+ 針(value を指す)+ 値ラベルで描画される。
- JS の valueLabel.formatter / centerArea.text は実行せず丸めた数値にフォールバックする。
- エッジケース(domain 0幅 / value 範囲外 / 空 data / 色配列不足)で panic せず、出力に NaN/inf を含めない。
- 同一入力で byte 一致(決定性)。
- schema に gauge/radialGauge 変種を追加、examples/specs に例、README・CHANGELOG 更新済み。
- cargo test --workspace / cargo fmt / cargo clippy -D warnings がすべて緑。
- **視覚検証済み(必須)**: 両 example を PNG で描画して画像目視し、QuickChart.io 実物と向き・ゾーン配置・針/中央値位置が一致する(Task 10 Step 1.5)。snapshot 緑だけで done を主張しない。

## 留意点
- `ChartKind` への 2 変種追加で網羅 match は `layout/mod.rs::build_scene` と `model.rs::chart_type_name` の 2 箇所のみ(他は `_`/`matches!` で吸収)。コンパイルエラーが出たら arm を追加。
- 全弧パスは standalone な空白区切り M/L/A/Z トークン(`raster_direct::parse_path_data` 不変条件)。連結すると PNG で消える(progress.rs の回帰参照)。
- `Prim::Circle`(rounded キャップ/針支点)は `raster_direct.rs`(L193)・`svg.rs`(L129)双方でサポート確認済み(scatter/line/radar/mixed で使用実績)。PNG でも描画される。
- gauge/radialGauge プラグインは互換ツール未インストールで chart.js golden 照合は不能。テストは snapshot + 不変条件(progressBar の前例)。実物比較は QuickChart.io ライブ API で行う(Task 10 Step 1.5)。
- 数値既定(centerPercentage 80・cutout 50・needle 各 %)はプラグインソース照合値。golden が無いため厳密ピン留め不要だが、QuickChart の見た目に寄せる基準として使う。
- 半円 gauge の向き(rotation:-π/circumference:π)は解釈が曖昧。Task 5 実装後、Task 10 Step 1.5 の実物比較で上下/左右が合っているか必ず確認し、ずれれば `angle()` と cy オフセットを修正する。
- guard.rs はゾーン数が `data.len()`(既存 total_points 上限で被覆)のため原則変更不要。snapshot で要素数が想定通りか確認する。
