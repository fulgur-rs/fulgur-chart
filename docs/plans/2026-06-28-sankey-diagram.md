# Sankey Diagram Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a QuickChart / chartjs-chart-sankey 互換の sankey チャート種別 (`ChartKind::Sankey`) を fulgur-chart コアに実装する。ノード間フロー量を幅で表す帯状グラフを、決定的 SVG/PNG として出力する。

**Architecture:** chartjs-chart-sankey の `lib/{core,layout}.ts` のアルゴリズムを Rust に忠実移植する(列割り当て + y 位置決めは決定的な単一パス)。リボン描画は水平 3 次ベジェ帯で、`colorMode='gradient'`(デフォルト)を真に再現するため描画バックエンド(scene/svg/raster)に水平リニアグラデーション塗りを先行追加する。

**Tech Stack:** Rust, serde / serde_json (frontend), schemars (schema), tiny-skia (PNG raster), insta (snapshot test)。

**ビルド/テスト基点:** worktree `/home/ubuntu/fulgur-chart/.worktrees/feat/sankey`、crate は `crates/fulgur-chart`。コマンドはすべて crate ディレクトリ(`crates/fulgur-chart`)から実行する。
- ビルド: `cargo build -p fulgur-chart`
- テスト: `cargo test -p fulgur-chart`
- 単一テスト: `cargo test -p fulgur-chart <test_name>`

**ベースライン:** 465 tests passing, 0 failures(着手前に確認済み)。

---

## 設計の要点(全タスク共通の不変条件)

これらは determinism のため**絶対に**守ること(advisor レビュー反映):

1. **ノードコンテナは挿入順**(初出データ順)。`Vec<String>`(キー順序) + `HashMap<String, usize>`(キー→index) で表現する(既存 `parse_matrix` の `x_cats`/`x_idx` と同じ流儀。`indexmap` は直接依存でないため使わない)。chartjs は `Map` を挿入順に走査し、これが **出力そのもの** (`findStartNode` / `processRest` / `addPadding` / ノード描画順) に影響する。`labels`/`priority`/`column` は参照専用なので `HashMap` で良い。
2. **全ソートは安定**: `slice::sort_by`(安定)を使い、`sort_unstable*` は**禁止**。tie-break を chartjs から忠実移植する:
   - `flowSort`: `b.flow - a.flow`、同値なら `a.index - b.index`(降順 flow、index 昇順)。
   - `flowByNodeCount(prop)`: `nodeCount(a) - nodeCount(b)`、同値なら `a.node[prop].len() - b.node[prop].len()`。
   - `sortFlows` の from/to 比較子(下記 Task 3.5 に明記)。
   - priority ソート `(a.priority ?? 0) - (b.priority ?? 0)` は明示 tie-break が無く JS の安定ソートに依存 → Rust でも `sort_by`(安定)で同順を得る。
3. グローバル可変カウンタ `getCountId`/`_visited`(chartjs 自身がバグ予備軍とコメント)は**移植しない**。`nodeCount` / `getAllKeysForward` は走査ごとの `HashSet<String>`(visited)で再帰する。
4. 浮動小数の比較ソートは `f64`。NaN は入力検証で弾く(flow は有限数)。`sort_by` に渡す比較は `partial_cmp(...).unwrap_or(Equal)` で安全に(NaN は事前排除済みのため実際には発生しない)。
5. **「高互換」の定義**: chartjs-chart-sankey と同一アルゴリズム + 同一オプション面 + 視覚的に忠実 + **自前の決定的 golden**。QuickChart の PNG とのバイト一致は目指さない(レンダラが別物のため不可能)。

### chartjs デフォルト値(controller.ts / flow.ts より)
- `modeX='edge'`, `nodeWidth=10`, `nodePadding=10`, `borderColor=black`, `borderWidth=1`, ラベル色 `color=black`, `size='max'`。
- Flow: `colorFrom='red'`, `colorTo='green'`, `colorMode='gradient'`, `alpha=0.5`。
- layout padding `{top:3, left:3, right:13, bottom:3}`、凡例なし。

---

## Phase 0 — グラデーション描画バックエンド拡張(独立コミット、最初に着手)

`Prim::Path` は単色塗りのみ。sankey の `colorMode='gradient'`(デフォルト)を再現するため、水平リニアグラデーションで塗る新プリミティブ `Prim::GradientPath` を追加する。既存 `Prim::Path` は不変。

### Task 0.1: `Prim::GradientPath` をシーン IR に追加

**Files:**
- Modify: `crates/fulgur-chart/src/scene.rs`

**Step 1: シーンにバリアントを追加**

`src/scene.rs` の `enum Prim` に追加(`Prim::Path` の直後):

```rust
    /// 水平リニアグラデーションで塗る任意パス。sankey のリボンに使う。
    /// グラデーションは userSpace の x0→x1 で stop0→stop1 に補間する(y 方向は一定)。
    /// d は `Prim::Path` と同じく fmt_num 整形済みトークンのみを含むこと。
    GradientPath {
        d: String,
        /// グラデーション開始 x(stop0 の位置、ユーザ座標)。
        x0: f64,
        /// グラデーション終了 x(stop1 の位置、ユーザ座標)。
        x1: f64,
        stop0: Color,
        stop1: Color,
    },
```

**Step 2: ビルドして網羅 match の漏れを検出**

Run: `cargo build -p fulgur-chart 2>&1 | grep -E "error|GradientPath" | head`
Expected: `src/svg.rs` と `src/raster_direct.rs` の `match prim` が non-exhaustive エラー(= 次タスクで埋める)。`scene.rs` 自体はコンパイル可。

**Step 3: コミット(Task 0.2/0.3 とまとめてでも可)** — 次タスクで描画を埋めてからコミットする。

---

### Task 0.2: SVG グラデーション描画(`<defs>` + `<linearGradient>` + `url()`)

**Files:**
- Modify: `crates/fulgur-chart/src/svg.rs`
- Test: `crates/fulgur-chart/src/svg.rs`(`#[cfg(test)]` 内に追加)

**Step 1: 失敗するテストを書く**

`src/svg.rs` のテストモジュールに追加:

```rust
    #[test]
    fn gradient_path_emits_defs_and_url_ref() {
        let scene = Scene {
            width: 100.0,
            height: 100.0,
            items: vec![Prim::GradientPath {
                d: "M0 0L10 0L10 10Z".to_string(),
                x0: 0.0,
                x1: 10.0,
                stop0: Color { r: 255, g: 0, b: 0, a: 0.5 },
                stop1: Color { r: 0, g: 128, b: 0, a: 0.5 },
            }],
        };
        let svg = render_svg(&scene, "sans-serif");
        assert!(svg.contains("<defs>"), "must emit defs");
        assert!(svg.contains("<linearGradient id=\"grad0\""), "deterministic id");
        assert!(svg.contains("gradientUnits=\"userSpaceOnUse\""));
        assert!(svg.contains("stop-color=\"#ff0000\""));
        assert!(svg.contains("stop-color=\"#008000\""));
        assert!(svg.contains(r##"fill="url(#grad0)""##), "path must ref gradient");
    }

    #[test]
    fn gradient_path_is_byte_deterministic() {
        let scene = Scene {
            width: 50.0, height: 50.0,
            items: vec![Prim::GradientPath {
                d: "M0 0L5 0L5 5Z".into(), x0: 0.0, x1: 5.0,
                stop0: Color { r: 1, g: 2, b: 3, a: 1.0 },
                stop1: Color { r: 4, g: 5, b: 6, a: 1.0 },
            }],
        };
        assert_eq!(render_svg(&scene, "s"), render_svg(&scene, "s"));
    }
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart gradient_path_emits_defs -- --nocapture`
Expected: コンパイルエラー(GradientPath の match 未実装)。

**Step 3: 実装する**

`src/svg.rs` の `render_svg` を以下のように変更する。`<svg ...>` 開始タグ直後に `<defs>` を出力し、`GradientPath` を出現順で `grad{n}` と採番する。描画ループでは同じ採番規則で `url(#grad{n})` を参照する(2 パスとも GradientPath を同順で数えるため id 一致)。

```rust
pub fn render_svg(scene: &Scene, font_family: &str) -> String {
    let mut s = String::new();
    let w = fmt_num(scene.width);
    let h = fmt_num(scene.height);
    write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
    )
    .unwrap();

    // defs: GradientPath を出現順に grad{n} で採番(userSpaceOnUse・水平)。
    let mut grad_defs = String::new();
    let mut gi = 0usize;
    for item in &scene.items {
        if let Prim::GradientPath { x0, x1, stop0, stop1, .. } = item {
            write_linear_gradient(&mut grad_defs, gi, *x0, *x1, stop0, stop1);
            gi += 1;
        }
    }
    if gi > 0 {
        s.push_str("<defs>");
        s.push_str(&grad_defs);
        s.push_str("</defs>");
    }

    let mut gi = 0usize;
    for item in &scene.items {
        if let Prim::GradientPath { d, .. } = item {
            write!(s, r#"<path d="{d}" fill="url(#grad{gi})" stroke="none"/>"#).unwrap();
            gi += 1;
        } else {
            write_prim(&mut s, item, font_family);
        }
    }
    s.push_str("</svg>\n");
    s
}

fn write_linear_gradient(s: &mut String, idx: usize, x0: f64, x1: f64, stop0: &Color, stop1: &Color) {
    let x0f = fmt_num(x0);
    let x1f = fmt_num(x1);
    let c0 = color_hex(stop0);
    let c1 = color_hex(stop1);
    let o0 = stop_opacity(stop0.a);
    let o1 = stop_opacity(stop1.a);
    write!(
        s,
        r#"<linearGradient id="grad{idx}" gradientUnits="userSpaceOnUse" x1="{x0f}" y1="0" x2="{x1f}" y2="0"><stop offset="0" stop-color="{c0}"{o0}/><stop offset="1" stop-color="{c1}"{o1}/></linearGradient>"#
    )
    .unwrap();
}

fn stop_opacity(a: f32) -> String {
    if a < 1.0 {
        format!(r#" stop-opacity="{}""#, fmt_num(a as f64))
    } else {
        String::new()
    }
}
```

注: 既存の `write_prim` の `match prim` には `Prim::GradientPath { .. } => {}`(no-op、`render_svg` 側で処理済み)を追加してコンパイルを通す。あるいは `write_prim` を呼ばない上記分岐で網羅されるが、`match` は全バリアントを要求するため no-op アームを足すこと。

**Step 4: パスしたことを確認**

Run: `cargo test -p fulgur-chart gradient_path -- --nocapture`
Expected: PASS(2 tests)。

**Step 5: コミットは Task 0.3 と一緒に。**

---

### Task 0.3: ラスタ(PNG)グラデーション描画(tiny-skia LinearGradient)

**Files:**
- Modify: `crates/fulgur-chart/src/raster_direct.rs`
- Test: `crates/fulgur-chart/tests/golden_png.rs`(または近接の PNG テスト)に簡易バイト安定テストを追加

**Step 1: 失敗するテストを書く**(scene→png のバイト安定。`scene_to_png` は pub。)

`crates/fulgur-chart/tests/` に新規 `render_gradient.rs` を作成:

```rust
//! GradientPath のラスタ描画 byte 安定テスト。
use fulgur_chart::scene::{Prim, Scene};
use fulgur_chart::ir::Color;
use fulgur_chart::raster_direct::scene_to_png;

const FONT: &[u8] = include_bytes!("../assets/fonts/NotoSansJP-Regular.otf");

fn scene() -> Scene {
    Scene {
        width: 40.0, height: 20.0,
        items: vec![Prim::GradientPath {
            d: "M0 0L40 0L40 20L0 20Z".into(),
            x0: 0.0, x1: 40.0,
            stop0: Color { r: 255, g: 0, b: 0, a: 0.5 },
            stop1: Color { r: 0, g: 128, b: 0, a: 0.5 },
        }],
    }
}

#[test]
fn gradient_png_is_byte_deterministic() {
    let a = scene_to_png(&scene(), 1.0, FONT).unwrap();
    let b = scene_to_png(&scene(), 1.0, FONT).unwrap();
    assert_eq!(a, b);
    assert!(!a.is_empty());
}
```

> 確認済み: `scene`/`raster_direct`/`ir` は `pub mod`(lib.rs)、`scene_to_png(scene: &Scene, scale: f32, font_bytes: &[u8]) -> Result<Vec<u8>, String>` は pub(`src/raster_direct.rs:62`)。フォントは `assets/fonts/NotoSansJP-Regular.otf`。`parse_color(s: &str) -> Option<Color>`(`src/color.rs:14`)。

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart gradient_png -- --nocapture`
Expected: コンパイルエラー(GradientPath の raster match 未実装)。

**Step 3: 実装する**

`src/raster_direct.rs` の `render_prim` の `match prim` に追加(`Prim::Path` アームの直後)。tiny-skia の `LinearGradient` を使う:

```rust
        Prim::GradientPath { d, x0, x1, stop0, stop1 } => {
            let Some(path) = parse_path_data(d) else { return; };
            use tiny_skia::{GradientStop, LinearGradient, Point, SpreadMode, Shader};
            let to_ts = |c: &Color| tiny_skia::Color::from_rgba8(
                c.r, c.g, c.b, (c.a * 255.0).round() as u8);
            let shader = LinearGradient::new(
                Point::from_xy(*x0 as f32, 0.0),
                Point::from_xy(*x1 as f32, 0.0),
                vec![
                    GradientStop::new(0.0, to_ts(stop0)),
                    GradientStop::new(1.0, to_ts(stop1)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            );
            let mut paint = Paint::default();
            // LinearGradient::new は縮退時(x0==x1)に None を返すため solid にフォールバック。
            paint.shader = shader.unwrap_or_else(|| Shader::SolidColor(to_ts(stop0)));
            pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
        }
```

> 重要: グラデーションの `Transform` はシェーダ座標系に効く。`fill_path` の `transform`(スケール)はパスとシェーダ双方に適用されるので、シェーダ側 Transform は `identity()` でよい(x0/x1 はパスと同じユーザ座標)。

**Step 4: パスしたことを確認**

Run: `cargo test -p fulgur-chart gradient_png -- --nocapture`
Expected: PASS。

**Step 5: 全テスト + コミット**

Run: `cargo test -p fulgur-chart 2>&1 | tail -5`
Expected: 既存 465 + 新規 gradient テストが全て PASS。

```bash
git add crates/fulgur-chart/src/scene.rs crates/fulgur-chart/src/svg.rs crates/fulgur-chart/src/raster_direct.rs crates/fulgur-chart/tests/render_gradient.rs
git commit -m "feat(render): add horizontal linear gradient fill (Prim::GradientPath) for SVG and PNG"
```

---

## Phase 1 — IR 足場(スタブでコンパイルを通す)

### Task 1.1: `SankeyLink` 構造体と `Series.links` フィールド追加

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify(全 `Series { .. }` リテラル): grep で洗い出す。

**Step 1: ir.rs に型追加**

`src/ir.rs` の `TreeNode` の近くに追加:

```rust
/// sankey のリンク(フロー)。ノード間のフロー量を表す。from/to はノードID(文字列)。
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLink {
    pub from: String,
    pub to: String,
    pub flow: f64,
}
```

`struct Series` に新フィールドを追加(`tree` の直後):

```rust
    /// sankey のリンク(フロー)配列。sankey 種別のみ使用、他は空。
    pub links: Vec<SankeyLink>,
```

**Step 2: 全 Series リテラルを更新**

Run: `grep -rn "tree: vec!\[\]" crates/fulgur-chart/src crates/fulgur-chart/tests`
各 `Series { .. tree: vec![], }` リテラルに `links: vec![],` を追加する(`ir.rs` のテスト内 4 箇所、`frontend/chartjs.rs`、`layout/*.rs` 等)。`tree: <expr>,` を持つ全 `Series` リテラルが対象。

> ヒント: `tree: vec![],` の直後行に `links: vec![],` を機械的に挿入。`tree` が非空(treemap)のリテラルも `links: vec![]` を足す。

**Step 3: ビルド**

Run: `cargo build -p fulgur-chart 2>&1 | grep -E "missing field .links|error\[" | head`
Expected: `Series` リテラルの欠落が無くなるまで繰り返す。最終的に `error: missing field links` が 0 件。
(この時点では `ChartKind::Sankey` 未追加なのでレイアウト dispatch エラーは出ない。)

**Step 4: テスト + コミット**

Run: `cargo test -p fulgur-chart 2>&1 | tail -3`(既存テストが通ること)
```bash
git add -A && git commit -m "feat(ir): add SankeyLink type and Series.links field"
```

---

### Task 1.2: sankey 用 enum と `ChartKind::Sankey` バリアント追加

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`

**Step 1: enum と ChartKind バリアントを追加**

`src/ir.rs` に追加(`ChartKind` の上あたり):

```rust
/// sankey リンクの配色モード。chartjs-chart-sankey の colorMode に対応。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SankeyColorMode {
    From,
    To,
    Gradient,
}

/// sankey の x 方向レイアウトモード。chartjs の modeX に対応。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SankeyModeX {
    Edge,
    Even,
}

/// sankey のノードサイズ算出方式。chartjs の size に対応(max=既定)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SankeySize {
    Min,
    Max,
}
```

`enum ChartKind` に追加(`WordCloud` の後):

```rust
    /// QuickChart / chartjs-chart-sankey 互換の sankey。ノード間フロー量を帯幅で表す。
    /// データは series[0].links に持つ。設定値は kind に保持(Gauge 同様)。
    Sankey {
        color_from: Color,
        color_to: Color,
        color_mode: SankeyColorMode,
        /// リンク塗りの不透明度(0.0–1.0)。chartjs default 0.5。
        alpha: f32,
        node_width: f64,
        node_padding: f64,
        mode_x: SankeyModeX,
        size: SankeySize,
        border: Color,
        border_width: f64,
        label_color: Color,
        /// ノードID→表示ラベル上書き。未登録は ID をそのまま表示。
        labels: std::collections::HashMap<String, String>,
        /// ノードID→priority(列内ソートキー)。空なら priority レイアウト無効。
        priority: std::collections::HashMap<String, f64>,
        /// ノードID→列番号(手動 x 指定)。
        columns: std::collections::HashMap<String, usize>,
    },
```

**Step 2: ビルドして網羅 match の漏れを検出**

Run: `cargo build -p fulgur-chart 2>&1 | grep -E "non-exhaustive|not covered|error" | head`
Expected: `src/layout/mod.rs`(build_scene)と `src/model.rs`(chart_type_name)で non-exhaustive。

**Step 3: スタブ arm を追加**

`src/layout/mod.rs` の `build_scene` の `match spec.kind` に追加(本実装は Phase 3):

```rust
        ChartKind::Sankey { .. } => sankey::build(spec, m),
```
かつ `pub mod sankey;` を `mod.rs` 冒頭のモジュール宣言に追加。
そして **スタブ** `src/layout/sankey.rs` を作成:

```rust
//! sankey レイアウト(Phase 3 で本実装)。
use crate::ir::ChartSpec;
use crate::scene::Scene;
use crate::text::TextMeasurer;

pub fn build(spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    Scene { width: spec.width, height: spec.height, items: vec![] }
}
```

`src/model.rs` の `chart_type_name` に追加:

```rust
        ChartKind::Sankey { .. } => "sankey",
```
(`compute_geometry` / `compute_axes` は `_ => None` のため変更不要。)

**Step 4: ビルド + テスト + コミット**

Run: `cargo build -p fulgur-chart` → Expected: 成功。
Run: `cargo test -p fulgur-chart 2>&1 | tail -3` → Expected: 既存テスト全通過。
```bash
git add -A && git commit -m "feat(ir): add ChartKind::Sankey variant and layout stub"
```

---

## Phase 2 — フロントエンドパース + スキーマ

### Task 2.1: JSON Schema に `Sankey` バリアントを追加

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`
- Test: `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Step 1: 失敗するテストを書く**

`tests/frontend_chartjs.rs` に追加(`matrix_schema_roundtrip` の流儀):

```rust
#[test]
fn sankey_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "sankey",
        "data": { "datasets": [{
            "label": "Energy",
            "data": [
                {"from": "A", "to": "B", "flow": 10},
                {"from": "A", "to": "C", "flow": 5},
                {"from": "B", "to": "C", "flow": 10}
            ],
            "colorFrom": "#36a2eb",
            "colorTo": "#ff6384",
            "colorMode": "gradient",
            "labels": {"A": "Alpha"},
            "priority": {"A": 0},
            "column": {"A": 0}
        }],
        "labels": []
        },
        "options": { "plugins": { "title": {"display": true, "text": "T"} } }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::Sankey(_)));
    // 同じ文書を strict パーサも受理すること(parser↔schema パリティ)。
    assert!(chartjs::parse(json, true).is_ok(), "strict parser should accept sankey");
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart sankey_schema_roundtrip`
Expected: コンパイルエラー(`ChartJsSpec::Sankey` 未定義)。

**Step 3: スキーマを実装する**

`src/schema/chartjs.rs` の `enum ChartJsSpec` に追加(`WordCloud` の後):

```rust
    Sankey(SankeySpec),
```

そして `MatrixSpec` ブロックの流儀で構造体を追加:

```rust
// ────────────────────────────────────────────────
// Sankey chart (QuickChart / chartjs-chart-sankey)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeySpec {
    pub data: SankeyData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<MatrixOptions>, // plugins(title/legend) + theme を共用
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeyData {
    pub datasets: Vec<SankeyDataset>,
    /// chart.js 互換のため受理するが sankey では未使用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SankeyDataset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub data: Vec<SankeyFlow>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_from: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_to: Option<ColorString>,
    /// "from" | "to" | "gradient"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f64>,
    /// ノードラベル色
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_padding: Option<f64>,
    /// "edge" | "even"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_x: Option<String>,
    /// "min" | "max"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<std::collections::HashMap<String, f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<std::collections::HashMap<String, u32>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SankeyFlow {
    pub from: String,
    pub to: String,
    pub flow: f64,
}
```

> `ColorString` / `ScalarOrArray` / `MatrixOptions` は同ファイル内で既に定義済み。`use` 済みのものを使う。

**Step 4: ビルドのみ確認(parser はまだ無いので strict 部分は次タスクまで失敗する)**

Run: `cargo build -p fulgur-chart` → Expected: 成功。
> 注: `sankey_schema_roundtrip` の `chartjs::parse(json, true)` は Task 2.2 完了まで失敗する。Step 5 のコミットは Task 2.2 と一緒に行う(TDD: 先にスキーマ部 `matches!(spec, ChartJsSpec::Sankey(_))` だけ通す中間テストにしてもよい)。

---

### Task 2.2: フロントエンドパーサ `parse_sankey` + dispatch + strict キー検証

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
- Test: `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Step 1: 失敗するテストを書く**

`tests/frontend_chartjs.rs` に追加:

```rust
#[test]
fn sankey_basic_parse() {
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[
        {"from":"A","to":"B","flow":10},
        {"from":"A","to":"C","flow":5},
        {"from":"B","to":"C","flow":10},
        {"from":"C","to":"D","flow":15}
    ]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, fulgur_chart::ir::ChartKind::Sankey { .. }));
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].links.len(), 4);
    assert_eq!(spec.series[0].links[0].from, "A");
    assert_eq!(spec.series[0].links[0].flow, 10.0);
}

#[test]
fn sankey_defaults_match_chartjs() {
    use fulgur_chart::ir::{ChartKind, SankeyColorMode, SankeyModeX, SankeySize, Color};
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]}}"#;
    let spec = chartjs::parse(json, false).unwrap();
    let ChartKind::Sankey { color_from, color_to, color_mode, alpha, node_width, node_padding, mode_x, size, border_width, .. } = spec.kind else { panic!() };
    assert_eq!(color_from, Color { r: 255, g: 0, b: 0, a: 1.0 });   // 'red'
    assert_eq!(color_to, Color { r: 0, g: 128, b: 0, a: 1.0 });     // 'green'
    assert_eq!(color_mode, SankeyColorMode::Gradient);
    assert!((alpha - 0.5).abs() < 1e-9);
    assert_eq!(node_width, 10.0);
    assert_eq!(node_padding, 10.0);
    assert_eq!(mode_x, SankeyModeX::Edge);
    assert_eq!(size, SankeySize::Max);
    assert_eq!(border_width, 1.0);
}

#[test]
fn sankey_rejects_non_finite_flow() {
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":"x"}]}]}}"#;
    assert!(chartjs::parse(json, false).is_err());
}

#[test]
fn sankey_strict_rejects_unknown_key() {
    let json = r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}],"bogus":1}]}}"#;
    assert!(chartjs::parse(json, true).is_err());
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart sankey_basic_parse`
Expected: コンパイルエラー or パース失敗(parse_sankey 未実装)。

**Step 3: dispatch に分岐を追加**

`src/frontend/chartjs.rs` の `parse()` 内、matrix/treemap 分岐の並びに追加(`treemap` の後):

```rust
        if chart_type.as_deref() == Some("sankey") {
            if strict {
                check_unknown_keys_sankey(json)?;
            }
            return parse_sankey(json);
        }
```

**Step 4: `parse_sankey` を実装**

`parse_matrix` の隣に追加。`'red'`/`'green'` 等の名前付き色は既存 `parse_color` が CSS 名を解釈できるか確認(できなければ定数で `Color{255,0,0,1.0}` / `Color{0,128,0,1.0}` をデフォルトに使う)。

```rust
fn parse_sankey(json: &str) -> Result<ChartSpec, String> {
    use crate::ir::{ChartKind, SankeyColorMode, SankeyModeX, SankeySize, SankeyLink};
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct W { data: D, #[serde(default)] options: RawOptions }
    #[derive(Deserialize)]
    struct D { datasets: Vec<DS> }
    #[derive(Deserialize)]
    struct DS {
        #[allow(dead_code)] #[serde(default)] label: String,
        data: Vec<Flow>,
        #[serde(rename = "colorFrom", default)] color_from: Option<String>,
        #[serde(rename = "colorTo", default)] color_to: Option<String>,
        #[serde(rename = "colorMode", default)] color_mode: Option<String>,
        #[serde(default)] alpha: Option<f64>,
        #[serde(rename = "borderColor", default)] border_color: Option<String>,
        #[serde(rename = "borderWidth", default)] border_width: Option<f64>,
        #[serde(default)] color: Option<String>,
        #[serde(rename = "nodeWidth", default)] node_width: Option<f64>,
        #[serde(rename = "nodePadding", default)] node_padding: Option<f64>,
        #[serde(rename = "modeX", default)] mode_x: Option<String>,
        #[serde(default)] size: Option<String>,
        #[serde(default)] labels: Option<HashMap<String, String>>,
        #[serde(default)] priority: Option<HashMap<String, f64>>,
        #[serde(default)] column: Option<HashMap<String, u32>>,
    }
    #[derive(Deserialize)]
    struct Flow { from: String, to: String, flow: f64 }

    let raw: W = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if raw.data.datasets.len() != 1 {
        return Err("sankey チャートは dataset が 1 つのみサポートされます".to_string());
    }
    let ds = raw.data.datasets.into_iter().next().unwrap();

    // リンク構築 + flow 有限性チェック。
    let mut links = Vec::with_capacity(ds.data.len());
    for f in ds.data {
        if !f.flow.is_finite() || f.flow < 0.0 {
            return Err("sankey の flow は非負の有限数である必要があります".to_string());
        }
        links.push(SankeyLink { from: f.from, to: f.to, flow: f.flow });
    }

    let theme = build_theme(raw.options.theme);
    let red = Color { r: 255, g: 0, b: 0, a: 1.0 };
    let green = Color { r: 0, g: 128, b: 0, a: 1.0 };
    let black = Color { r: 0, g: 0, b: 0, a: 1.0 };

    let color_from = ds.color_from.as_deref().and_then(parse_color).unwrap_or(red);
    let color_to = ds.color_to.as_deref().and_then(parse_color).unwrap_or(green);
    let color_mode = match ds.color_mode.as_deref() {
        Some("from") => SankeyColorMode::From,
        Some("to") => SankeyColorMode::To,
        _ => SankeyColorMode::Gradient, // 既定 + "gradient"
    };
    let alpha = ds.alpha.map(|a| a as f32).unwrap_or(0.5).clamp(0.0, 1.0);
    let border = ds.border_color.as_deref().and_then(parse_color).unwrap_or(black);
    let border_width = ds.border_width.unwrap_or(1.0);
    let label_color = ds.color.as_deref().and_then(parse_color).unwrap_or(black);
    let node_width = ds.node_width.unwrap_or(10.0);
    let node_padding = ds.node_padding.unwrap_or(10.0);
    let mode_x = match ds.mode_x.as_deref() {
        Some("even") => SankeyModeX::Even,
        _ => SankeyModeX::Edge,
    };
    let size = match ds.size.as_deref() {
        Some("min") => SankeySize::Min,
        _ => SankeySize::Max,
    };
    let labels = ds.labels.unwrap_or_default();
    let priority = ds.priority.unwrap_or_default();
    let columns = ds.column.unwrap_or_default()
        .into_iter().map(|(k, v)| (k, v as usize)).collect();

    let series = vec![Series {
        name: String::new(),
        values: vec![],
        points: vec![],
        fill: vec![],
        stroke: vec![],
        stroke_width: 0.0,
        area: false,
        tension: 0.0,
        series_type: SeriesType::Bar,
        point_radius: None,
        box_points: vec![],
        tree: vec![],
        links,
    }];

    Ok(ChartSpec {
        kind: ChartKind::Sankey {
            color_from, color_to, color_mode, alpha, node_width, node_padding,
            mode_x, size, border, border_width, label_color, labels, priority, columns,
        },
        series,
        categories: vec![],
        x_axis: empty_axis(), // matrix と同じ no-op AxisSpec(下記参照)
        y_axis: empty_axis(),
        legend: crate::ir::LegendPos::None,
        title: raw.options.plugins.title.filter(|t| t.display).map(|t| t.text),
        width: 800.0,
        height: 450.0,
        data_labels: false,
        theme,
    })
}
```

> `empty_axis()` は `parse_matrix` の `AxisSpec { title: None, min: None, ... grid: false }` をそのままインライン展開してよい(ヘルパーが無ければインラインで書く)。`parse_color` が `&str` を取るか `&String` かはシグネチャを確認(`src/frontend/chartjs.rs:1553` 付近)。CSS 名 'red'/'green' を解さない場合でも上記は定数フォールバックで正しいデフォルトになる。

**Step 5: `check_unknown_keys_sankey` を実装**

`check_unknown_keys_matrix` の流儀で:

```rust
fn check_unknown_keys_sankey(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v, Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else { return Ok(()); };
    check_object(top, &["type", "data", "options"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["datasets", "labels"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(ds, &[
                        "label", "data", "colorFrom", "colorTo", "colorMode", "alpha",
                        "borderColor", "borderWidth", "color", "nodeWidth", "nodePadding",
                        "modeX", "size", "labels", "priority", "column",
                    ], &format!("data.datasets[{i}]"))?;
                    if let Some(points) = ds.get("data").and_then(|v| v.as_array()) {
                        for (j, pt) in points.iter().enumerate() {
                            if let Some(pt) = pt.as_object() {
                                check_object(pt, &["from", "to", "flow"],
                                    &format!("data.datasets[{i}].data[{j}]"))?;
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["plugins", "theme"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(plugins, &["title", "legend"], "options.plugins")?;
        }
    }
    Ok(())
}
```

**Step 6: パス + コミット**

Run: `cargo test -p fulgur-chart sankey -- --nocapture`
Expected: `sankey_schema_roundtrip` / `sankey_basic_parse` / `sankey_defaults_match_chartjs` / `sankey_rejects_non_finite_flow` / `sankey_strict_rejects_unknown_key` 全 PASS。
Run: `cargo test -p fulgur-chart 2>&1 | tail -3` → 既存全通過。
```bash
git add -A && git commit -m "feat(frontend,schema): parse sankey spec and add schema variant"
```

---

## Phase 3 — レイアウトアルゴリズム移植(`layout/sankey.rs`)

chartjs-chart-sankey `lib/{core,layout}.ts` の移植。下記の TS 参照を Rust に翻訳する。**Phase 0 の不変条件(挿入順・安定ソート・visited set)を厳守**。

内部データ構造(`layout/sankey.rs` のモジュール内 struct):

```rust
struct Node {
    key: String,
    in_flow: f64,    // chartjs `in`
    out_flow: f64,   // chartjs `out`
    size: f64,
    x: Option<usize>,  // 列。未割当=None(chartjs の defined(node.x))
    y: Option<f64>,    // 縦位置。未割当=None
    priority: Option<f64>,
    has_manual_column: bool, // chartjs `node.column`(手動 x 指定フラグ)
    from: Vec<Edge>, // 入力リンク
    to: Vec<Edge>,   // 出力リンク
}
struct Edge { flow: f64, index: usize, key: String, node: usize, add_y: f64 } // node = ターゲット Node の index
```

> chartjs はノード参照をポインタで持つが、Rust では `Vec<Node>` + index で表す。`Edge.node` は相手ノードの index。ノードの index は **初出データ順**で固定(`Vec<String> keys` + `HashMap<String,usize>`)。

### Task 3.1: ノード構築(buildNodesFromData 相当)

**TS 参照(core.ts):** `buildNodesFromData` / `setSizes` / `setPriorities` / `setColumns` / `flowSort`。

実装ポイント:
- データ順に走査し、`from`/`to` を初出順でノード登録。各リンク i について `fromNode.out += flow`、`fromNode.to.push(Edge{flow,index:i,key:to,node:to_idx,add_y:0})`、`toNode.in += flow`、`toNode.from.push(Edge{...})`。
- `setSizes`: 各ノードの `from`/`to` を `flowSort`(`b.flow-a.flow`、同値で `a.index-b.index`)で**安定**ソート。`size = match size_method { Max => max(in||out, out||in), Min => min(...) }`。`||` は「0 なら相手」のフォールバック(`if in_flow != 0.0 { in_flow } else { out_flow }` 等)。
- `setPriorities`: `priority` マップにキーがあれば `node.priority = Some(v)`。
- `setColumns`: `columns` マップにキーがあれば `node.has_manual_column = true; node.x = Some(col)`。

**Step 1〜4(TDD):** ノード数・in/out 合計・size・from/to ソート順を検証する単体テスト(`#[cfg(test)]` in `layout/sankey.rs`)を先に書き、実装して通す。例:

```rust
#[test]
fn builds_nodes_in_data_order_with_sizes() {
    // A->B 10, A->C 5, B->C 10, C->D 15
    let nodes = build_nodes(&links(&[("A","B",10.0),("A","C",5.0),("B","C",10.0),("C","D",15.0)]),
                            SankeySize::Max, &hm(), &hm_u());
    // 初出順 A,B,C,D
    assert_eq!(nodes.keys, ["A","B","C","D"]);
    // A: out=15 in=0 size=15 ; C: in=15 out=15 size=15 ; D: in=15 size=15
    // B: in=10 out=10 size=10
}
```

### Task 3.2: 列(x)割り当て(calculateX 相当)

**TS 参照(layout.ts):** `getAllKeysForward` / `startColumn` / `nextColumn` / `calculateX`。

実装ポイント:
- `getAllKeysForward(start_nodes, visited)`: visited は `HashSet<usize>`(ノード index)。再帰で `to` を辿る。
- `startColumn`: `from.is_empty()` のノードを列 0。循環時は data 順で `referenced` 集合を使って起点を補う(TS 通り)。
- `nextColumn`: 残キーのうち「残キーから to されていない」ものを次列に。無ければ残キー先頭 1 つ(循環打破)。
- `calculateX`: `x=0` は `startColumn`、以降 `nextColumn`。`node.x` が未割当(`None`)のときのみ `Some(x)` を設定(手動列ノードは保持)。空列は `panic!` ではなく `Err`/ロギングではなく、TS 同様「ここに来たらバグ」なので `debug_assert` + 残キー先頭で前進(無限ループ防止)。`maxX = max(node.x)`。`modeX==Edge` のとき、出力が無く手動列でないノードを `maxX` へ。

**Step 1〜4(TDD):** 直線連鎖 A→B→C→D で x=0,1,2,3、分岐 A→B,A→C で B,C が同列、edge モードで出力なしノードが右端、手動 column 指定が効くこと、単純な循環 A→B→A で panic しないことを検証。

### Task 3.3: y 割り当て(デフォルト calculateY 相当)

**TS 参照(layout.ts):** `findStartNode` / `processFrom` / `processTo` / `setOrGetY` / `processRest` / `fixTop` / `calculateY`。`SMALL_VALUE=1e-6`。

実装ポイント:
- 再帰(`processFrom`/`processTo`)は **visited 不要だが** `node.y` 未割当チェックで自然終了する。ただし長い連鎖でのスタック深さに注意 → guard で node 数を制限(Phase 4)。深い連鎖でも安全にするため、再帰実装で問題なければ可(guard 上限内)。
- `findStartNode`: 最大 size のノード。複数なら x 昇順で、左端(x==0)優先、無ければ右端(x==maxX)、無ければ中央。**安定**ソートで TS と同順。
- `processRest` / `fixTop` を TS 通り移植。
- `calculateY` は `maxY`(= `fixTop` の戻り)を返す。

**Step 1〜4(TDD):** 小さな DAG で各ノードの y が TS 期待値(手計算 or 既知例)になること、`fixTop` 後に同列ノードが重ならない(y 昇順で `y_i + size_i <= y_{i+1}`)ことを検証。

### Task 3.4: priority 指定時の y(calculateYUsingPriority 相当)

**TS 参照:** `calculateYUsingPriority`。列ごとに priority 昇順(安定)で並べ、`y += max(out, in)` で縦積み。`priority` マップが非空のとき(`!options.priority` 相当)に使う。

**Step 1〜4(TDD):** priority マップを与えたとき列内順序が priority 昇順になることを検証。

### Task 3.5: padding と flow オフセット(addPadding / sortFlows 相当)

**TS 参照:** `nodeByXYSize` / `addPadding` / `sortFlows`。`padding = (maxY / heightPx) * nodePadding`(heightPx = `spec.height`)。

- `addPadding`: `nodeByXYSize`(x→y→size)で**安定**ソートしてグリッド走査。TS のロジックを忠実移植(各ノードに paddings*padding を加算、maxY 更新)。`columnXs`/`grid` は `HashMap<usize, usize>` + `Vec<Vec<f64>>`。
- `sortFlows`: 各ノードで `from` を `a.node.y + a.node.out/2` 昇順(**安定**)、`to` を `a.node.y + a.node.in/2` 昇順で並べ、`add_y` を計算(overlap 分岐含む)。`add_y` は後段のリボン端点 y オフセット。

**Step 1〜4(TDD):** padding 適用後に同列ノード間隔が空くこと、`sortFlows` 後に各ノードの from/to の add_y が単調(非 overlap 時は累積 flow)になることを検証。

### Task 3.6: シーン生成(ノード矩形・リボン・ラベル)

**TS 参照:** `controller.ts` の `updateElements` / `_drawNodes` / `_drawLabels`、`flow.ts` の `controlPoints` / `draw`。

座標写像(描画域 `[plot_left, plot_right] × [plot_top, plot_bottom]`):
- 描画域は `matrix::build` 流に算出。上は `OUTER_PAD + title_band`、下は `spec.height - OUTER_PAD`。左右はラベル幅を考慮(ノードは左半分なら右側にラベル、右半分なら左側 → 両端にラベル用マージンを確保。最大ラベル幅 `m.width(label, font)` を左右マージンに足す)。
- x 写像: `px(x) = plot_left + (x / maxX) * plot_w`(maxX==0 のとき plot_left)。
- y 写像: `py(y) = plot_top + (y / maxY) * plot_h`(maxY==0 のとき plot_top)。chartjs は y 反転(reverse)で 0 が上 → この写像で 0=plot_top=上。OK。
- ノード矩形: `x=px(node.x)`, `y=py(node.y)`, `w=node_width`, `h=py(node.y + node.size) - py(node.y)`。`Prim::Rect{fill: ノード色}` + 境界線(`border_width>0` のとき矩形 4 辺を `Prim::Path` or `Prim::Line` で stroke。`Prim::Rect` に stroke が無いため、境界は `Prim::Path` の矩形 d で `fill:None, stroke:Some(border)`)。
  - ノード色: chartjs は from=colorFrom / to=colorTo(last-flow-wins)。簡潔には「出力を持つ(out>0)なら colorFrom、出力が無い(終端)なら colorTo」で近似 → ただし TS 厳密は「各リンクで from.color=colorFrom, to.color=colorTo を順に上書き、最後が残る」。**厳密移植**: リンクを index 順に走査し各ノードの色を colorFrom(from 側)/colorTo(to 側)で上書きし最後の値を使う。alpha はノードには適用しない(chartjs はノード塗りに colorFrom/colorTo をそのまま使う; ノードは alpha 無し)。
- リボン(各リンク i、parsed の流儀):
  - `from_node`, `to_node`。`from_y = py(from.y + add_y(from.to, link))`、`to_y = py(to.y + add_y(to.from, link))`。`height = py(from.y + flow) - py(from.y)`(リンク幅 = flow に比例; xScale/yScale 線形なので flow→ピクセル高は `(flow/maxY)*plot_h`)。
  - 端点 x: `x = px(from.x) + node_width + border_space`、`x2 = px(to.x) - border_space`(`border_space = if border_width>0 {border_width/2 + 0.5} else {0}`)。
  - 制御点(`controlPoints`、x<x2 の通常枝): `cp1=(x + (x2-x)/3*2, y)`, `cp2=(x + (x2-x)/3, y2)`。x>=x2 の枝も TS 通り(後方/循環リンク)。
  - パス d(閉じた帯): `M x y C cp1x cp1y cp2x cp2y x2 y2 L x2 (y2+height) C (cp2x) (cp2y+height) (cp1x) (cp1y+height) x (y+height) Z`。全数値 `fmt_num`。
  - 塗り: `colorMode==From` → `Prim::Path{ fill:Some(color_from.with_alpha(alpha)) }`、`To` → `color_to.with_alpha(alpha)`、`Gradient` → `Prim::GradientPath{ d, x0:x, x1:x2, stop0:color_from.with_alpha(alpha), stop1:color_to.with_alpha(alpha) }`。
    - `with_alpha`: `Color{ a: alpha, ..c }`。
- ラベル: ノード中央 y(`y + h/2`)に `Prim::Text`。`px(node.x) < 描画域中央` なら右側(`anchor:Start`, `x = px+node_width+border+4`)、そうでなければ左側(`anchor:End`, `x = px - border - 4`)。色 `label_color`。表示文字列は `labels.get(key).unwrap_or(key)`。

**Step 1〜4(TDD):** `render_chart(&spec)` レベルのテスト(新規 `tests/render_sankey.rs`):

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;
fn render(j: &str) -> String { render_chart(&chartjs::parse(j, false).unwrap()) }

const ENERGY: &str = r#"{"type":"sankey","data":{"datasets":[{"data":[
  {"from":"Coal","to":"Electricity","flow":25},
  {"from":"Gas","to":"Electricity","flow":15},
  {"from":"Electricity","to":"Residential","flow":20},
  {"from":"Electricity","to":"Industrial","flow":20}
],"colorFrom":"#36a2eb","colorTo":"#ff6384"}]}}"#;

#[test] fn sankey_renders_svg() {
    let svg = render(ENERGY);
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<rect"));   // nodes
    assert!(svg.contains("<path"));   // ribbons
    assert!(svg.contains("<text"));   // labels
    assert!(!svg.contains("NaN"));
}
#[test] fn sankey_is_byte_deterministic() { assert_eq!(render(ENERGY), render(ENERGY)); }
#[test] fn sankey_gradient_default_emits_defs() {
    let svg = render(r#"{"type":"sankey","data":{"datasets":[{"data":[{"from":"A","to":"B","flow":1}]}]}}"#);
    assert!(svg.contains("<linearGradient"), "gradient mode default should emit defs");
}
#[test] fn sankey_snapshot() { insta::assert_snapshot!(render(ENERGY)); }
```

スタブ `layout/sankey.rs::build` を本実装で置き換える。

**Step 5: 全テスト + スナップショット承認 + コミット**

Run: `cargo test -p fulgur-chart sankey 2>&1 | tail`
Expected: 全 PASS。スナップショット新規は `cargo insta review`(または `INSTA_UPDATE=always`)で承認後コミット。
```bash
git add -A && git commit -m "feat(layout): implement sankey layout and rendering (chartjs-chart-sankey port)"
```

---

## Phase 4 — 入力ガード(guard.rs)

### Task 4.1: `validate_spec` に sankey ガードを追加

**Files:**
- Modify: `crates/fulgur-chart/src/guard.rs`
- Test: `crates/fulgur-chart/src/guard.rs`(`#[cfg(test)]`)

**Step 1: 失敗するテストを書く**

```rust
#[test]
fn sankey_link_count_over_limit_rejected() {
    let spec = sankey_spec_with_links(MAX_SANKEY_LINKS + 1);
    assert!(validate_spec(&spec, &InputLimits::default()).is_err());
}
#[test]
fn sankey_within_limits_ok() {
    let spec = sankey_spec_with_links(10);
    assert!(validate_spec(&spec, &InputLimits::default()).is_ok());
}
#[test]
fn sankey_node_label_too_long_rejected() {
    // from に max_label_bytes+1 のキー
    ...
    assert!(validate_spec(&spec, &InputLimits::default()).is_err());
}
```

**Step 2: 失敗を確認** → `MAX_SANKEY_LINKS` 未定義でコンパイルエラー。

**Step 3: 実装**

`guard.rs` 冒頭に定数:

```rust
/// sankey のリンク数上限。DoS 対策。
pub const MAX_SANKEY_LINKS: usize = 100_000;
/// sankey のノード数上限。再帰深さ(processFrom/To)に直結するため抑えめ。
pub const MAX_SANKEY_NODES: usize = 10_000;
```

`validate_spec` に sankey 分岐を追加(progress/wordcloud 分岐の並び):

```rust
if let crate::ir::ChartKind::Sankey { .. } = &spec.kind {
    let links = spec.series.first().map(|s| s.links.len()).unwrap_or(0);
    if links > MAX_SANKEY_LINKS {
        return Err(format!("sankey のリンク数 {} が上限 {} を超えています", links, MAX_SANKEY_LINKS));
    }
    // 初出順ノード集合とラベルバイト検証。
    let mut seen = std::collections::HashSet::new();
    let mut node_count = 0usize;
    if let Some(s) = spec.series.first() {
        for l in &s.links {
            for key in [&l.from, &l.to] {
                if seen.insert(key.clone()) {
                    node_count += 1;
                    if key.len() > limits.max_label_bytes {
                        return Err(format!(
                            "sankey ノードラベルの長さ {} バイトが上限 {} を超えています",
                            key.len(), limits.max_label_bytes));
                    }
                }
            }
        }
    }
    if node_count > MAX_SANKEY_NODES {
        return Err(format!("sankey のノード数 {} が上限 {} を超えています", node_count, MAX_SANKEY_NODES));
    }
}
```

そして `total_points` 集計(`guard.rs:308` 付近)に links を算入する:

```rust
let link_points: usize = spec.series.iter().map(|s| s.links.len()).sum();
// total_points = ... + tree_points + link_points;
```

**Step 4: パス + 全テスト + コミット**

Run: `cargo test -p fulgur-chart 2>&1 | tail -3`
```bash
git add -A && git commit -m "feat(guard): validate sankey node/link counts, recursion-safe limits, label bytes"
```

---

## Phase 5 — examples / README / CLI スモーク / golden PNG

### Task 5.1: example spec + golden PNG

**Files:**
- Create: `examples/specs/sankey.json`
- (任意)golden PNG を既存 `tests/golden_png.rs` の流儀で追加

**Step 1:** `examples/specs/sankey.json` を作成:

```json
{
  "type": "sankey",
  "data": {
    "datasets": [{
      "label": "Energy flow",
      "colorFrom": "#36a2eb",
      "colorTo": "#ff6384",
      "data": [
        {"from": "Coal",        "to": "Electricity", "flow": 25},
        {"from": "Gas",         "to": "Electricity", "flow": 15},
        {"from": "Solar",       "to": "Electricity", "flow": 10},
        {"from": "Electricity", "to": "Residential", "flow": 22},
        {"from": "Electricity", "to": "Industrial",  "flow": 18},
        {"from": "Electricity", "to": "Commercial",  "flow": 10}
      ]
    }]
  },
  "options": { "plugins": { "title": { "display": true, "text": "Energy Sankey" } } }
}
```

**Step 2: CLI スモーク**(crate ルート `crates/fulgur-chart-cli` から、または既存 CLI バイナリで)

Run(リポジトリルートから):
```bash
cargo run -p fulgur-chart-cli -- --input examples/specs/sankey.json --format svg > /tmp/sankey.svg && head -c 80 /tmp/sankey.svg
```
> 実際の CLI フラグは `crates/fulgur-chart-cli/src` または README を確認して合わせる。
Expected: `<svg ...` で始まる SVG が出力される。PNG も同様に確認。

**Step 3: example レンダリングテスト**(任意、`tests/render_sankey.rs` に追加)

```rust
#[test]
fn sankey_example_spec_renders() {
    let json = include_str!("../../../examples/specs/sankey.json");
    let svg = render(json);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}
```

**Step 4: コミット**
```bash
git add -A && git commit -m "docs(examples): add sankey example spec"
```

### Task 5.2: README 更新

**Files:** Modify: `README.md`

**Step 1:** README の対応チャート type 列挙(`treemap`/`wordCloud` が並ぶ箇所)に `sankey` を追加。サポートオプション(colorFrom/colorTo/colorMode/modeX/size/labels/priority/column/nodeWidth/nodePadding)を一行で記載。

**Step 2: コミット**
```bash
git add README.md && git commit -m "docs(readme): document sankey chart type"
```

---

## Phase 6 — 最終検証(verification-before-completion)

**Step 1: 全テスト**

Run: `cargo test -p fulgur-chart 2>&1 | grep -E "test result:" `
Expected: 全 suite で `0 failed`。新規テスト数 = ベースライン 465 + 追加分。

**Step 2: ワークスペース全体ビルド + clippy(設定があれば)**

Run: `cargo build` / `cargo clippy -p fulgur-chart --all-targets 2>&1 | tail`(警告ゼロ目標)

**Step 3: 決定性の最終確認**

Run: `cargo test -p fulgur-chart sankey_is_byte_deterministic gradient_path_is_byte_deterministic gradient_png_is_byte_deterministic`
Expected: PASS。

**Step 4: schema↔parser パリティ**

Run: `cargo test -p fulgur-chart sankey_schema_roundtrip`
Expected: PASS(schema 受理 + strict parser 受理)。

**Step 5: acceptance 照合**(beads issue fulgur-chart-5qx の acceptance を 1 つずつ確認)。

---

## 完了後

- `superpowers:finishing-a-development-branch` でブランチ完了処理(PR or merge)。
- beads issue を close(`bd close fulgur-chart-5qx`)。
- フォローアップ issue: `parsing` キー再マップ、hover 色、per-link 色、`bd create` で起票。
