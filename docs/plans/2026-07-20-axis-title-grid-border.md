# Axis title / grid / border → IR 伝搬 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Chart.js の `options.scales.{x,y}.{title,grid,border}` を型付き Schema にし、IR と SVG レンダリングまで一貫して反映させる (beads: fulgur-chart-s7o)。

**Architecture:** Schema 側で `serde_json::Value` の受け皿を Chart.js と 1:1 対応する typed struct に差し替え、IR `AxisSpec` を sub-struct(`AxisTitle` / `AxisGrid` / `AxisBorder`) 化。`frontend/chartjs.rs` の変換ブリッジで Schema→IR を埋め、`layout/common.rs::draw_frame` で軸タイトル/グリッド/ベースライン/tick 刻みを新 IR から描画する。

**Tech Stack:** Rust workspace (`crates/fulgur-chart`) / serde / schemars / snapshot 系の integration fixture。

**Related:**
- 現状: `crates/fulgur-chart/src/schema/common.rs:95` の `AxisOptions.title` / `grid` は `Option<serde_json::Value>`
- 影響レイヤ: `schema/common.rs`, `ir.rs`, `frontend/chartjs.rs`, `frontend/vegalite.rs`, `layout/common.rs`, `layout/bar.rs`, `layout/scatter.rs`, `layout/boxplot.rs`, `scene.rs`, `svg.rs`, `raster_direct.rs`
- スコープ外(受理のみ or 後続 issue): `title.padding`, `font.family/weight/style`, `grid.tickLength`, `grid.offset`, per-tick 配色

---

## Task 1: Schema 新型 (FontSpec / AxisTitleAlign / AxisTitleOptions / GridLineOptions / AxisBorderOptions)

**Files:**
- Modify: `crates/fulgur-chart/src/schema/common.rs`
- Test: 同ファイル `#[cfg(test)] mod tests`

**Step 1: 失敗するテストを書く**

`crates/fulgur-chart/src/schema/common.rs` 末尾に:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_title_options_accepts_full_shape() {
        let v: AxisTitleOptions = serde_json::from_str(
            r#"{"display":true,"text":"Y (円)","color":"#333","font":{"size":14},"align":"center"}"#,
        )
        .unwrap();
        assert_eq!(v.text.as_deref(), Some("Y (円)"));
        assert!(matches!(v.align, Some(AxisTitleAlign::Center)));
    }

    #[test]
    fn grid_line_options_rejects_unknown_key() {
        let e = serde_json::from_str::<GridLineOptions>(r#"{"colorx":"#eee"}"#);
        assert!(e.is_err(), "unknown key must be rejected");
    }

    #[test]
    fn axis_border_options_accepts_dash_array() {
        let v: AxisBorderOptions =
            serde_json::from_str(r#"{"color":"#000","width":2,"dash":[4,4]}"#).unwrap();
        assert_eq!(v.dash.as_deref(), Some(&[4.0, 4.0][..]));
    }
}
```

**Step 2: テストが失敗することを確認**

`cargo test -p fulgur-chart schema::common::tests` → 型未定義でコンパイルエラー。

**Step 3: 最小実装**

`crates/fulgur-chart/src/schema/common.rs` の既存 `AxisOptions` 定義の**前**に以下を追加:

```rust
/// options.scales.<axis>.title (Chart.js 準拠, camelCase)。
/// padding/font.family などは受理のみで v1 未描画。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AxisTitleOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontSpec>,
    /// v1 では未使用(受理のみ)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<AxisTitleAlign>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum AxisTitleAlign {
    Start,
    Center,
    End,
}

/// options.scales.<axis>.grid (Chart.js 準拠)。
/// tick_length/offset/color per-tick 配列などは v1 未描画(受理のみ)。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GridLineOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ScalarOrArray<ColorString>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_width: Option<ScalarOrArray<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_on_chart_area: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draw_ticks: Option<bool>,
    /// v1 では未使用(受理のみ)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_length: Option<f64>,
    /// v1 では未使用(受理のみ)。chart.js は band 中心/端で grid を描く挙動の切替。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<bool>,
}

/// options.scales.<axis>.border (Chart.js 準拠)。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AxisBorderOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<Vec<f64>>,
}

/// Chart.js の共通 font オブジェクト。v1 では size のみ描画に反映される。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FontSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// number | "bold" 等。v1 では受理のみ。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}
```

**Step 4: テストが通ることを確認**

`cargo test -p fulgur-chart schema::common::tests` → 全 pass。

**Step 5: commit**

```bash
git add crates/fulgur-chart/src/schema/common.rs
git commit -m "feat(schema): add typed AxisTitle/GridLine/AxisBorder options"
```

---

## Task 2: AxisOptions フィールド差し替え

**Files:**
- Modify: `crates/fulgur-chart/src/schema/common.rs:95` の `AxisOptions`
- 影響: `crates/fulgur-chart/src/frontend/chartjs.rs` の scales 読み取り箇所 (`.get("title")` などの直接 JSON 参照が生き残っていれば要調整)

**Step 1: 失敗するテストを書く**

`schema::common::tests` に追加:

```rust
#[test]
fn axis_options_accepts_typed_title_grid_border() {
    let v: AxisOptions = serde_json::from_str(
        r#"{"title":{"text":"X"},"grid":{"color":"#eee"},"border":{"width":2}}"#,
    )
    .unwrap();
    assert!(v.title.is_some());
    assert!(v.grid.is_some());
    assert!(v.border.is_some());
}

#[test]
fn axis_options_rejects_unknown_border_field() {
    let e = serde_json::from_str::<AxisOptions>(r#"{"border":{"colorr":"#000"}}"#);
    assert!(e.is_err());
}
```

**Step 2: 失敗を確認**

`cargo test -p fulgur-chart schema::common::tests` → `border` 未定義でコンパイルエラー。

**Step 3: 実装**

`AxisOptions` の該当箇所を差し替え:

```rust
// 差し替え前:
//   pub title: Option<serde_json::Value>,
//   pub grid: Option<serde_json::Value>,
// 差し替え後:
#[serde(default, skip_serializing_if = "Option::is_none")]
pub title: Option<AxisTitleOptions>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub grid: Option<GridLineOptions>,
#[serde(default, skip_serializing_if = "Option::is_none")]
pub border: Option<AxisBorderOptions>,
```

**Step 4: 全体ビルドで型不整合を潰す**

`cargo build --workspace 2>&1 | tail -30`。`frontend/chartjs.rs` などで `axis.title.as_ref().and_then(|v| v.get(...))` のような JSON アクセスが残っていれば、そこは Task 6 で正しく直すので、必要なら一旦 `let _ = axis;` などで警告だけ潰し、コンパイルを通す。

**Step 5: schema JSON snapshot テストを走らせて壊れていないか確認**

`cargo test -p fulgur-chart-cli schema_chartjs` → schemars 生成 JSON が更新されるはずなので、fixture がある場合は再生成手順に従う。

**Step 6: commit**

```bash
git add crates/fulgur-chart/src/schema/common.rs
git commit -m "feat(schema): replace AxisOptions title/grid untyped Value with typed structs, add border"
```

---

## Task 3: IR 新型 (AxisTitle / AxisGrid / AxisBorder)

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`

**Step 1: 失敗するテストを書く**

`ir.rs` 末尾に `#[cfg(test)] mod tests` を追加(既存なければ)し:

```rust
#[test]
fn axis_grid_default_is_chartjs_shape() {
    let g = AxisGrid::default();
    assert_eq!(g.display, true);
    assert_eq!(g.line_width, 1.0);
    assert_eq!(g.draw_ticks, true);
    assert!(g.color.is_none());
}

#[test]
fn axis_border_default_is_chartjs_shape() {
    let b = AxisBorder::default();
    assert_eq!(b.display, true);
    assert_eq!(b.width, 1.0);
    assert!(b.color.is_none());
    assert!(b.dash.is_empty());
}
```

**Step 2: 失敗を確認**

`cargo test -p fulgur-chart ir::tests` → 型未定義でコンパイルエラー。

**Step 3: 実装**

`AxisSpec` 定義の**前**に追加:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct AxisTitle {
    /// 空なら表示しない(schema 側で display=false or text 空を弾く)。
    pub text: String,
    pub color: Option<Color>,
    pub font_size: Option<f64>,
    pub align: AxisTitleAlign,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum AxisTitleAlign {
    Start,
    #[default]
    Center,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisGrid {
    pub display: bool,
    pub color: Option<Color>,
    pub line_width: f64,
    pub draw_ticks: bool,
}

impl Default for AxisGrid {
    fn default() -> Self {
        Self {
            display: true,
            color: None,
            line_width: 1.0,
            draw_ticks: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisBorder {
    pub display: bool,
    pub color: Option<Color>,
    pub width: f64,
    pub dash: Vec<f64>,
}

impl Default for AxisBorder {
    fn default() -> Self {
        Self {
            display: true,
            color: None,
            width: 1.0,
            dash: Vec::new(),
        }
    }
}
```

**Step 4: テスト成功**

`cargo test -p fulgur-chart ir::tests`

**Step 5: commit**

```bash
git add crates/fulgur-chart/src/ir.rs
git commit -m "feat(ir): add AxisTitle/AxisGrid/AxisBorder sub-structs"
```

---

## Task 4: AxisSpec フィールド差し替え + 全 callsite 更新

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs` (`AxisSpec`)
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs` (`AxisSpec { ... }` の 4 箇所 + `zero_axis()`)
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs` (2 箇所)
- Modify: `crates/fulgur-chart/src/layout/wordcloud.rs` (2 箇所)
- Modify: `crates/fulgur-chart/src/layout/common.rs` テストヘルパ `make_bar_spec`
- Modify: `crates/fulgur-chart/src/layout/scatter.rs` テストヘルパ

**Step 1: AxisSpec を書き換え**

```rust
pub struct AxisSpec {
    pub title: Option<AxisTitle>,   // 元: Option<String>
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub suggested_min: Option<f64>,
    pub suggested_max: Option<f64>,
    pub begin_at_zero: bool,
    pub offset: bool,
    pub grid: AxisGrid,             // 元: bool
    pub border: AxisBorder,         // 新規
}
```

**Step 2: 全 callsite を機械的に更新**

`cargo build --workspace 2>&1 | tail -60` でエラー行を列挙し、それぞれ:
- `title: None,` → そのまま
- `grid: true,` → `grid: AxisGrid::default(),`
- `grid: false,` → `grid: AxisGrid { display: false, ..Default::default() },`
- `AxisSpec { ... }` の末尾に `border: AxisBorder::default(),` 追加

`zero_axis()`(`frontend/chartjs.rs:2396`) は「軸を描かない」用途なので:
```rust
grid: AxisGrid { display: false, draw_ticks: false, ..Default::default() },
border: AxisBorder { display: false, ..Default::default() },
```

**Step 3: ビルドがクリーンに通ることを確認**

`cargo build --workspace` → warning のみ (未使用の title など)。

**Step 4: 既存テスト全走で回帰なしを確認**

`cargo test --workspace --no-fail-fast 2>&1 | grep "test result"` → すべて `ok`。

**Step 5: commit**

```bash
git add -u
git commit -m "refactor(ir): replace AxisSpec title/grid with typed sub-structs, add border"
```

---

## Task 5: Prim::Line に dash 対応を追加

**Files:**
- Modify: `crates/fulgur-chart/src/scene.rs` (`Prim::Line`)
- Modify: `crates/fulgur-chart/src/svg.rs` (`Prim::Line` レンダリング)
- Modify: `crates/fulgur-chart/src/raster_direct.rs` (`Prim::Line` レンダリング)
- Modify: `Prim::Line { ... }` を書いている全 callsite (`layout/common.rs`, `layout/bar.rs`, `layout/scatter.rs`, `layout/boxplot.rs`, `layout/radar.rs`, scene.rs テスト等)

**Step 1: 失敗するテストを書く**

`crates/fulgur-chart/src/svg.rs` の既存 SVG レンダリングテストがある領域に:

```rust
#[test]
fn line_with_dash_renders_dasharray() {
    let scene = Scene {
        width: 10.0,
        height: 10.0,
        items: vec![Prim::Line {
            x1: 0.0, y1: 0.0, x2: 10.0, y2: 0.0,
            stroke: Color { r: 0, g: 0, b: 0, a: 1.0 },
            stroke_width: 1.0,
            dash: vec![4.0, 4.0],
        }],
    };
    let svg = render_svg(&scene);
    assert!(svg.contains("stroke-dasharray=\"4 4\""), "svg={svg}");
}
```

**Step 2: 失敗確認**

`cargo test -p fulgur-chart svg::` → `dash` フィールド未定義でコンパイルエラー。

**Step 3: 実装**

- `scene.rs` の `Prim::Line` に `dash: Vec<f64>` を追加(空=実線)
- `svg.rs` の `Prim::Line` レンダリングで `if !dash.is_empty()` のとき `stroke-dasharray="{}"` を出力(値は空白区切り、`fmt_num` で整形)
- `raster_direct.rs` の `Prim::Line` レンダリングで dash を反映(tiny-skia の `Stroke::dash` を使う。既存の `Prim::Path` に dash があれば pattern を流用)
- 全 callsite に `dash: Vec::new(),` を追加(空 = 既存挙動不変)

**Step 4: 全体テスト走らせて回帰なしを確認**

`cargo test --workspace --no-fail-fast`

**Step 5: commit**

```bash
git add -u
git commit -m "feat(scene): add dash pattern support to Prim::Line"
```

---

## Task 6: Schema→IR 変換ヘルパ (axis_*_from) + unit test

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`

**Step 1: 失敗するテストを書く**

`frontend/chartjs.rs` のテストモジュールに:

```rust
#[test]
fn axis_title_from_returns_none_when_display_false() {
    let opts = AxisTitleOptions {
        display: Some(false),
        text: Some("Y".into()),
        ..Default::default()
    };
    assert!(axis_title_from(Some(&opts)).is_none());
}

#[test]
fn axis_title_from_maps_text_and_align() {
    let opts = AxisTitleOptions {
        display: Some(true),
        text: Some("Y (円)".into()),
        align: Some(crate::schema::common::AxisTitleAlign::End),
        ..Default::default()
    };
    let t = axis_title_from(Some(&opts)).unwrap();
    assert_eq!(t.text, "Y (円)");
    assert!(matches!(t.align, crate::ir::AxisTitleAlign::End));
}

#[test]
fn axis_grid_from_defaults_when_none() {
    let g = axis_grid_from(None);
    assert!(g.display);
    assert_eq!(g.line_width, 1.0);
}

#[test]
fn axis_grid_from_display_false_kills_grid() {
    let opts = GridLineOptions { display: Some(false), ..Default::default() };
    assert!(!axis_grid_from(Some(&opts)).display);
}

#[test]
fn axis_grid_from_draw_on_chart_area_false_kills_grid_in_v1() {
    let opts = GridLineOptions {
        display: Some(true),
        draw_on_chart_area: Some(false),
        ..Default::default()
    };
    assert!(!axis_grid_from(Some(&opts)).display, "v1: drawOnChartArea=false は display=false と同義");
}

#[test]
fn axis_border_from_dash_flows_through() {
    let opts = AxisBorderOptions {
        dash: Some(vec![4.0, 4.0]),
        width: Some(2.0),
        ..Default::default()
    };
    let b = axis_border_from(Some(&opts));
    assert_eq!(b.dash, vec![4.0, 4.0]);
    assert_eq!(b.width, 2.0);
}
```

**Step 2: 失敗確認**

`cargo test -p fulgur-chart frontend::chartjs` → 未定義関数エラー。

**Step 3: 実装**

`frontend/chartjs.rs` に fn を追加(既存 `parse_color` などのユーティリティ近傍):

```rust
fn axis_title_from(opts: Option<&AxisTitleOptions>) -> Option<AxisTitle> {
    let t = opts?;
    if t.display == Some(false) { return None; }
    let text = t.text.as_deref().unwrap_or("");
    if text.is_empty() { return None; }
    Some(AxisTitle {
        text: text.to_string(),
        color: t.color.as_deref().and_then(parse_color),
        font_size: t.font.as_ref().and_then(|f| f.size),
        align: match t.align {
            Some(schema_common::AxisTitleAlign::Start) => ir::AxisTitleAlign::Start,
            Some(schema_common::AxisTitleAlign::End) => ir::AxisTitleAlign::End,
            _ => ir::AxisTitleAlign::Center,
        },
    })
}

fn axis_grid_from(opts: Option<&GridLineOptions>) -> AxisGrid {
    let Some(g) = opts else { return AxisGrid::default(); };
    let display = g.display.unwrap_or(true)
        && g.draw_on_chart_area.unwrap_or(true);   // v1: 両者は同義
    let color = match &g.color {
        Some(ScalarOrArray::One(s)) => parse_color(s),
        Some(ScalarOrArray::Many(v)) => v.first().and_then(|s| parse_color(s)),
        None => None,
    };
    let line_width = match &g.line_width {
        Some(ScalarOrArray::One(w)) => *w,
        Some(ScalarOrArray::Many(v)) => v.first().copied().unwrap_or(1.0),
        None => 1.0,
    };
    AxisGrid {
        display,
        color,
        line_width,
        draw_ticks: g.draw_ticks.unwrap_or(true),
    }
}

fn axis_border_from(opts: Option<&AxisBorderOptions>) -> AxisBorder {
    let Some(b) = opts else { return AxisBorder::default(); };
    AxisBorder {
        display: b.display.unwrap_or(true),
        color: b.color.as_deref().and_then(parse_color),
        width: b.width.unwrap_or(1.0),
        dash: b.dash.clone().unwrap_or_default(),
    }
}
```

**Step 4: テスト通す**

`cargo test -p fulgur-chart frontend::chartjs::` の該当テストで pass。

**Step 5: commit**

```bash
git add -u
git commit -m "feat(chartjs): add axis_title_from/axis_grid_from/axis_border_from helpers"
```

---

## Task 7: Bridge を ChartSpec 構築へ配線

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs:679-698` (`x_axis: AxisSpec {..}`, `y_axis: AxisSpec {..}`)
- Modify: 同ファイル `:1795` `:1424` (mixed / boxplot 系の AxisSpec 構築箇所)

**Step 1: 失敗する統合テストを書く**

`frontend/chartjs.rs` のテストモジュールに(既存の end-to-end テスト形式に倣う):

```rust
#[test]
fn scales_x_title_flows_into_spec_x_axis() {
    let json = r#"{
      "type":"bar",
      "data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
      "options":{"scales":{"x":{"title":{"display":true,"text":"時刻"}}}}
    }"#;
    let spec = super::build_spec_from_json(json).unwrap(); // 既存の from-JSON helper 名に合わせる
    let t = spec.x_axis.title.as_ref().expect("x title");
    assert_eq!(t.text, "時刻");
}

#[test]
fn scales_y_border_dash_flows_into_spec_y_axis() {
    let json = r#"{
      "type":"line",
      "data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"scales":{"y":{"border":{"dash":[4,4],"width":2}}}}
    }"#;
    let spec = super::build_spec_from_json(json).unwrap();
    assert_eq!(spec.y_axis.border.dash, vec![4.0, 4.0]);
    assert_eq!(spec.y_axis.border.width, 2.0);
}
```

**Step 2: 失敗確認**

`cargo test -p fulgur-chart frontend::chartjs::scales_` → 既存の `title: None` ハードコードで期待値と不一致。

**Step 3: 実装**

`x_axis: AxisSpec { title: None, ..., grid: true, ... }` を:

```rust
let x_scale = scales_val.and_then(|s| s.get("x"));
let y_scale = scales_val.and_then(|s| s.get("y"));
// x_scale / y_scale は今 serde_json::Value 経由で読んでいるが、raw.options.scales も
// 通っているはずなので、そちらの typed 参照を使う。無ければ raw.options.scales を辿る:
let x_typed = raw.options.scales.as_ref().and_then(|s| s.x.as_ref());
let y_typed = raw.options.scales.as_ref().and_then(|s| s.y.as_ref());
// ...
x_axis: AxisSpec {
    title: axis_title_from(x_typed.and_then(|a| a.title.as_ref())),
    min: None, max: None,
    suggested_min: suggested_min_x,
    suggested_max: suggested_max_x,
    begin_at_zero: x_begin_at_zero,
    offset: x_offset,
    grid: axis_grid_from(x_typed.and_then(|a| a.grid.as_ref())),
    border: axis_border_from(x_typed.and_then(|a| a.border.as_ref())),
},
```

同様に y_axis, および同ファイル内の他の AxisSpec 構築箇所も更新。

**Step 4: テスト通す**

`cargo test -p fulgur-chart frontend::chartjs` → 全 pass。回帰なし。

**Step 5: commit**

```bash
git add -u
git commit -m "feat(chartjs): wire scales.{x,y}.{title,grid,border} into IR"
```

---

## Task 8: Grid レンダリングを AxisGrid 化

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs::draw_frame` の「2. 横グリッド + y 軸ラベル」ブロック

**Step 1: 失敗するテストを書く**

`layout/common.rs` のテストモジュールに:

```rust
#[test]
fn grid_display_false_produces_no_grid_lines() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.y_axis.grid.display = false;
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);
    let n_horizontal_lines = items.iter().filter(|p| matches!(p,
        Prim::Line { y1, y2, .. } if (y1 - y2).abs() < 0.01
    )).count();
    // baseline (border) は 1 本残る。gridline は 0。合計 1 本のはず。
    assert_eq!(n_horizontal_lines, 1, "grid display=false → gridline 0, baseline 1");
}

#[test]
fn grid_color_override_reaches_prim() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.y_axis.grid.color = Some(Color { r: 255, g: 0, b: 0, a: 1.0 });
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);
    let has_red = items.iter().any(|p| matches!(p,
        Prim::Line { stroke: Color { r: 255, g: 0, b: 0, .. }, .. }
    ));
    assert!(has_red, "grid.color=red は grid line stroke に反映されるべき");
}
```

**Step 2: 失敗確認**

`cargo test -p fulgur-chart layout::common::tests::grid_` → 期待値と不一致。

**Step 3: 実装**

`draw_frame` の horizontal grid loop を:

```rust
// 2. 横グリッド + y 軸ラベル。
let grid_cfg = &spec.y_axis.grid;
let grid_color = grid_cfg.color.unwrap_or(spec.theme.grid_color);
for &t in &frame.ticks.ticks {
    let y = frame.ys.map(t);
    if grid_cfg.display {
        items.push(Prim::Line {
            x1: frame.plot_left, y1: y, x2: frame.plot_right, y2: y,
            stroke: grid_color,
            stroke_width: grid_cfg.line_width,
            dash: Vec::new(),
        });
    }
    items.push(Prim::Text {
        // ... 既存の y ラベルはそのまま
    });
}
```

**Step 4: テスト通す**

`cargo test -p fulgur-chart layout::common`

**Step 5: commit**

```bash
git add -u
git commit -m "feat(layout): honor AxisGrid display/color/line_width in draw_frame"
```

---

## Task 9: Border レンダリング + tick 刻み描画

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs::draw_frame` の「3. x ベースライン」ブロック

**Step 1: 失敗するテストを書く**

```rust
#[test]
fn border_display_false_produces_no_baseline() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.x_axis.border.display = false;
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);
    // gridlines は残るが baseline (y=plot_bottom の水平線) は消える
    let n_baseline = items.iter().filter(|p| matches!(p,
        Prim::Line { y1, y2, .. } if (y1 - y2).abs() < 0.01
            && (*y1 - frame.plot_bottom).abs() < 0.01
    )).count();
    assert_eq!(n_baseline, 0, "border.display=false → baseline 描画なし");
}

#[test]
fn border_dash_reaches_prim() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.x_axis.border.dash = vec![4.0, 4.0];
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);
    let has_dash = items.iter().any(|p| matches!(p,
        Prim::Line { y1, y2, dash, .. }
            if (y1 - y2).abs() < 0.01
                && (*y1 - frame.plot_bottom).abs() < 0.01
                && dash == &vec![4.0, 4.0]
    ));
    assert!(has_dash, "border.dash が baseline に伝わっていない");
}

#[test]
fn grid_draw_ticks_false_skips_tick_marks() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.y_axis.grid.draw_ticks = false;
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);
    // tick marks は baseline に平行な短い線分。draw_ticks=false なら 0 本。
    // 具体的には plot_left から外側(左)に 4px 伸びる短線を数える。
    let n_ticks = items.iter().filter(|p| matches!(p,
        Prim::Line { x1, x2, .. } if (x2 - x1).abs() > 0.0 && (x2 - x1).abs() <= 5.0
    )).count();
    assert_eq!(n_ticks, 0, "draw_ticks=false は tick 刻みを描かない");
}
```

**Step 2: 失敗確認**

**Step 3: 実装**

`draw_frame` の baseline ブロックを:

```rust
// 3. x ベースライン。
let border = &spec.x_axis.border;
if border.display {
    let border_color = border.color.unwrap_or(ink);
    items.push(Prim::Line {
        x1: frame.plot_left, y1: frame.plot_bottom,
        x2: frame.plot_right, y2: frame.plot_bottom,
        stroke: border_color,
        stroke_width: border.width,
        dash: border.dash.clone(),
    });
}

// 3b. y 軸目盛(tick 刻み)。border と grid の間に描く。
let ticks_cfg = &spec.y_axis.grid;
if ticks_cfg.draw_ticks {
    let tick_color = ticks_cfg.color.unwrap_or(ink);
    const TICK_LEN: f64 = 4.0;
    for &t in &frame.ticks.ticks {
        let y = frame.ys.map(t);
        items.push(Prim::Line {
            x1: frame.plot_left - TICK_LEN, y1: y,
            x2: frame.plot_left, y2: y,
            stroke: tick_color,
            stroke_width: ticks_cfg.line_width,
            dash: Vec::new(),
        });
    }
}
```

**Step 4: テスト通す**

**Step 5: commit**

```bash
git add -u
git commit -m "feat(layout): honor AxisBorder display/color/width/dash and grid.draw_ticks"
```

---

## Task 10: Y 軸タイトル描画 + compute() 左余白拡張

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs`:
  - `compute()` の `y_axis_w` に Y タイトル幅を加算
  - `draw_frame` に Y タイトル描画を追加(全処理の**最後**、他要素と重ならないよう)

**Step 1: 失敗するテストを書く**

```rust
#[test]
fn y_axis_title_shifts_plot_left_right() {
    let mut spec_no_title = make_bar_spec(3, 400.0);
    let mut spec_with_title = make_bar_spec(3, 400.0);
    spec_with_title.y_axis.title = Some(AxisTitle {
        text: "売上 (円)".into(),
        color: None, font_size: None,
        align: AxisTitleAlign::Center,
    });
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let f_no = compute(&spec_no_title, &m);
    let f_ti = compute(&spec_with_title, &m);
    assert!(f_ti.plot_left > f_no.plot_left,
        "Y 軸タイトル分だけ plot_left が右にシフトすべき: no={} ti={}",
        f_no.plot_left, f_ti.plot_left);
}

#[test]
fn y_axis_title_renders_rotated_text() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.y_axis.title = Some(AxisTitle {
        text: "売上".into(), color: None, font_size: None,
        align: AxisTitleAlign::Center,
    });
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);
    let has_rotated_title = items.iter().any(|p| matches!(p,
        Prim::Text { content, rotate_deg: Some(deg), .. }
            if content == "売上" && (deg.abs() - 90.0).abs() < 0.1
    ));
    assert!(has_rotated_title, "Y 軸タイトルは -90deg 回転で描画されるべき");
}
```

**Step 2: 失敗確認**

**Step 3: 実装**

`compute()`:
```rust
// 既存: let y_axis_w = max_w as f64 + 10.0;
let y_title_w = spec.y_axis.title.as_ref()
    .map(|t| t.font_size.unwrap_or(spec.theme.font_size * 1.1) + 6.0) // 回転後のバンド幅は 1行分ちょい
    .unwrap_or(0.0);
let y_axis_w = max_w as f64 + 10.0 + y_title_w;
```

`draw_frame` の末尾(凡例の後):
```rust
if let Some(title) = &spec.y_axis.title {
    let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
    let color = title.color.unwrap_or(ink);
    let cy_center = (frame.plot_top + frame.plot_bottom) / 2.0;
    let (cy, anchor) = match title.align {
        AxisTitleAlign::Start => (frame.plot_top, Anchor::Start),
        AxisTitleAlign::End => (frame.plot_bottom, Anchor::End),
        AxisTitleAlign::Center => (cy_center, Anchor::Middle),
    };
    let x = OUTER_PAD + font / 2.0;
    items.push(Prim::Text {
        x, y: cy, size: font, anchor, fill: color,
        content: title.text.clone(),
        rotate_deg: Some(-90.0),
    });
}
```

**Step 4: テスト通す + 既存 fixture が Y タイトルなしで regression していないことを確認**

`cargo test --workspace 2>&1 | grep "test result"`

**Step 5: commit**

```bash
git add -u
git commit -m "feat(layout): render y-axis title (rotated) and expand plot_left"
```

---

## Task 11: X 軸タイトル描画 + compute() 下余白拡張

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs`

**Step 1: 失敗するテストを書く**

```rust
#[test]
fn x_axis_title_shifts_plot_bottom_up() {
    let mut a = make_bar_spec(3, 400.0);
    let mut b = make_bar_spec(3, 400.0);
    b.x_axis.title = Some(AxisTitle { text: "時刻".into(), color: None, font_size: None, align: AxisTitleAlign::Center });
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let fa = compute(&a, &m);
    let fb = compute(&b, &m);
    assert!(fb.plot_bottom < fa.plot_bottom,
        "X タイトルぶん plot_bottom が上にシフトすべき: fa={} fb={}", fa.plot_bottom, fb.plot_bottom);
}

#[test]
fn x_axis_title_renders_horizontal_text() {
    let mut spec = make_bar_spec(3, 400.0);
    spec.x_axis.title = Some(AxisTitle { text: "時刻".into(), color: None, font_size: None, align: AxisTitleAlign::Center });
    let m = TextMeasurer::new(crate::font::DEFAULT_FONT).unwrap();
    let frame = compute(&spec, &m);
    let mut items = Vec::new();
    draw_frame(&mut items, &spec, &frame, &m);
    let has_x_title = items.iter().any(|p| matches!(p,
        Prim::Text { content, rotate_deg: None, .. } if content == "時刻"
    ));
    assert!(has_x_title);
}
```

**Step 2: 失敗確認**

**Step 3: 実装**

`compute()`:
```rust
const AXIS_TITLE_BAND: f64 = 20.0;
let x_title_h = if spec.x_axis.title.is_some() { AXIS_TITLE_BAND } else { 0.0 };
let plot_bottom = spec.height - OUTER_PAD - X_LABEL_BAND - legend_bottom - x_title_h;
```

`draw_frame` の末尾に Y タイトルと並べて:
```rust
if let Some(title) = &spec.x_axis.title {
    let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
    let color = title.color.unwrap_or(ink);
    let (cx, anchor) = match title.align {
        AxisTitleAlign::Start => (frame.plot_left, Anchor::Start),
        AxisTitleAlign::End => (frame.plot_right, Anchor::End),
        AxisTitleAlign::Center => ((frame.plot_left + frame.plot_right) / 2.0, Anchor::Middle),
    };
    let y = frame.plot_bottom + X_LABEL_BAND + font * 0.9;
    items.push(Prim::Text {
        x: cx, y, size: font, anchor, fill: color,
        content: title.text.clone(),
        rotate_deg: None,
    });
}
```

**Step 4: テスト通す**

**Step 5: commit**

```bash
git add -u
git commit -m "feat(layout): render x-axis title and expand plot bottom"
```

---

## Task 12: bar/scatter/boxplot の同等パッチ

**Files:**
- Modify: `crates/fulgur-chart/src/layout/bar.rs::build_horizontal` (横棒: 値軸=x, カテゴリ軸=y)
- Modify: `crates/fulgur-chart/src/layout/scatter.rs` の compute 相当
- Modify: `crates/fulgur-chart/src/layout/boxplot.rs`

**Step 1: 失敗するテストを書く(scatter で 1 本、bar 横棒で 1 本)**

例: scatter で `y_axis.grid.display = false` のとき水平グリッド線が 0 本。

**Step 2〜4: 各 layout の draw_frame 相当箇所を Task 8〜11 と同じロジックで更新**

- scatter は `layout::scatter::build` 内で独自に軸を描いているのでそれぞれ AxisGrid/AxisBorder/AxisTitle を尊重
- bar 横棒 (`build_horizontal`) は縦棒と同じ `draw_frame` を通っているか要確認。もし通っていれば追加作業なし
- boxplot も同じ

**Step 5: commit**

```bash
git add -u
git commit -m "feat(layout): apply axis styling to bar-horizontal/scatter/boxplot"
```

---

## Task 13: Fixture 追加 + snapshot テスト

**Files:**
- Create: `tests/fixtures/chartjs/axis-title-basic.json` (bar + x/y title)
- Create: `tests/fixtures/chartjs/axis-grid-color.json` (line + grid.color/lineWidth)
- Create: `tests/fixtures/chartjs/axis-border-dashed.json` (line + border.color/width/dash)
- 場所は既存 fixture の慣習に従う (`bd remember` で位置を確認 or 既存 dir を grep)

**Step 1: 既存 fixture の場所を確認**

```bash
find /home/ubuntu/fulgur-chart/.worktrees/s7o-axis-styling -type d -name "fixtures" | head -5
ls /home/ubuntu/fulgur-chart/.worktrees/s7o-axis-styling/crates/fulgur-chart-cli/tests/fixtures 2>/dev/null | head
```

**Step 2: 3 つの fixture json を新規追加**

内容は minimal に(chart type + 対象オプションだけを厚めに書く)。

**Step 3: 既存 snapshot テストの再実行手順に従い、期待 SVG を生成/確認**

```bash
UPDATE_SNAPSHOT=1 cargo test -p fulgur-chart-cli renders_ || cargo test -p fulgur-chart-cli snapshot_
```

出力を目視 (`less .snapshots/...`) して「軸タイトルが描かれている」「グリッドが期待色」「baseline が破線」が確認できたら fixture を確定。

**Step 4: 全体テストで regression なしを確認**

```bash
cargo test --workspace --no-fail-fast 2>&1 | grep "test result"
```

**Step 5: commit**

```bash
git add -A tests/ crates/
git commit -m "test(chartjs): add axis title/grid/border fixture snapshots"
```

---

## Task 14: JSON Schema 更新 + lint / typecheck 走らせて仕上げる

**Files:** (auto-gen)
- schemars 経由で生成される JSON Schema がリポジトリに commit されていれば更新
- `cargo fmt`, `cargo clippy`, プロジェクトの `pre-commit` hook 実行

**Step 1: 生成 JSON schema の再エクスポート**

```bash
cargo run -p fulgur-chart-cli -- schema --dsl chartjs > docs/schema/chartjs.json  # 実際のコマンドは既存規約に従う
```

もし schema 出力コマンドがなければ、schema インテグレーションテストが JSON をチェックしていることを確認。

**Step 2: `cargo fmt --all` / `cargo clippy --workspace --no-deps -- -D warnings` を通す**

**Step 3: 全体テスト最終走**

```bash
cargo test --workspace --no-fail-fast 2>&1 | tail -30
```

**Step 4: commit (必要ならまとめ commit)**

```bash
git add -A
git commit -m "chore: regenerate schema and format"
```

---

## Verification Checklist (before finishing)

- [ ] `cargo test --workspace` 全 pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `cargo fmt --all --check` clean
- [ ] beads acceptance フィールドの各項目に対応する fixture/テストが存在
- [ ] タイポ (`grrid`, `borderr` 等) が deny_unknown_fields で拒否される unit test あり
- [ ] Y/X 軸タイトルなしの既存 fixture が unchanged (backward compat)
