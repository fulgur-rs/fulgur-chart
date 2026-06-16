# fulgur-chart v1 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chart.js v4 互換の JSON スペックから決定的な静的 SVG/PNG を生成する CLI ツール `fulgur-chart` を、bar/line/area/pie・doughnut の4種について作る。

**Architecture:** フロントエンド(chart.js DSL) → IR(`ChartSpec`) → Layout/Scale → Scene(描画プリミティブ) → SVG文字列 という層構造。PNG は SVG を `resvg`/`tiny-skia` でラスタライズして得る。各層は純関数で、乱数・時刻・グローバル状態を持たず byte-identical な出力を保証する。文字は `<text>` 要素 + 自前幅計測（Noto Sans JP 同梱）。

**Tech Stack:** Rust 2024 / Cargo workspace、`serde`/`serde_json`(spec)、`clap`(CLI)、`resvg`+`usvg`+`tiny-skia` 0.45 系(SVGパース・ラスタライズ)、`ttf-parser`(文字幅計測)、`insta`(スナップショットテスト)。

**設計の根拠:** `docs/plans/2026-06-17-fulgur-chart-design.md` を参照。

---

## 前提・規約

- **TDD 厳守**: 各機能は「失敗するテストを書く → 失敗を確認 → 最小実装 → 成功を確認 → コミット」。
  REQUIRED SUB-SKILL: superpowers:test-driven-development
- **決定性が最優先**: 浮動小数は必ず固定精度でフォーマット（後述 `fmt_num`）。`HashMap` の反復順に依存しない。
  - 注意: 同一プロセス内で `render_svg` を2回呼んで一致を見るテストは、`HashMap` 反復順のリグレッションを
    **検出できない**（プロセス内ではシードが同じため）。確実な担保は「描画経路に `HashMap` を一切持ち込まない」
    こと。順序が要る箇所は `Vec` か `BTreeMap` を使う。これを規約として守る。
- **依存のバージョン乖離回避**: `tiny-skia` と `usvg` は `resvg` の再エクスポート（`resvg::tiny_skia`, `resvg::usvg`）を使い、別 crate として重複させない。`fontdb` は `usvg::fontdb` を使う。
- **コミットは頻繁に**。各タスク末尾でコミットする。
- 作業ディレクトリ: `/home/ubuntu/fulgur-chart`（既に git init 済み・設計ドキュメントが commit 済み）。

---

## Task 1: Cargo ワークスペースと2クレートの雛形

**Files:**
- Create: `Cargo.toml`（ワークスペース）
- Create: `crates/fulgur-chart/Cargo.toml`
- Create: `crates/fulgur-chart/src/lib.rs`
- Create: `crates/fulgur-chart-cli/Cargo.toml`
- Create: `crates/fulgur-chart-cli/src/main.rs`

**Step 1: ワークスペース Cargo.toml を作成**

```toml
[workspace]
resolver = "2"
members = ["crates/fulgur-chart", "crates/fulgur-chart-cli"]

[workspace.package]
edition = "2024"
rust-version = "1.85.0"
license = "MIT OR Apache-2.0"
repository = "https://github.com/fulgur-rs/fulgur-chart"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
clap = { version = "4", features = ["derive"] }
resvg = "0.45"
usvg = "0.45"
ttf-parser = "0.25"
insta = { version = "1", features = ["yaml"] }
```

**Step 2: コアライブラリクレート `crates/fulgur-chart/Cargo.toml`**

```toml
[package]
name = "fulgur-chart"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
resvg = { workspace = true }
usvg = { workspace = true }
ttf-parser = { workspace = true }

[dev-dependencies]
insta = { workspace = true }
```

**Step 3: ライブラリのプレースホルダ `crates/fulgur-chart/src/lib.rs`**

```rust
//! fulgur-chart: chart.js v4 互換 JSON から決定的な静的 SVG/PNG を生成するライブラリ。

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
```

**Step 4: CLI クレート `crates/fulgur-chart-cli/Cargo.toml`**

```toml
[package]
name = "fulgur-chart-cli"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "fulgur-chart"
path = "src/main.rs"

[dependencies]
fulgur-chart = { path = "../fulgur-chart" }
clap = { workspace = true }
```

**Step 5: CLI プレースホルダ `crates/fulgur-chart-cli/src/main.rs`**

```rust
fn main() {
    println!("fulgur-chart {}", fulgur_chart::version());
}
```

**Step 6: ビルド確認**

Run: `cargo build`
Expected: 両クレートがコンパイル成功。

**Step 7: コミット**

```bash
git add Cargo.toml crates/
git commit -m "chore: Cargo ワークスペースと fulgur-chart / -cli の雛形を追加"
```

---

## Task 2: Noto Sans JP フォントの同梱

**Files:**
- Create: `assets/fonts/NotoSansJP-Regular.ttf`（ダウンロード）
- Create: `assets/fonts/LICENSE-NotoSansJP.txt`（SIL OFL）
- Modify: `crates/fulgur-chart/src/lib.rs`

**Step 1: フォントを取得**

Noto Sans JP Regular（**static**、可変フォントではない）と OFL ライセンスを `assets/fonts/` に置く。

> **要確認 (advisor 指摘):** 現在の Google Fonts 配布は Noto Sans JP が**可変フォント**。static 版は
> notofonts / noto-cjk リリースから取得する。形式は **OTF/CFF アウトライン**のことが多く、拡張子が
> `.otf` になる場合がある（`ttf-parser` は CFF も読めるので機能上は問題ないが、ファイル名・
> `include_bytes!` のパス・数 MB の埋め込みサイズを実物で確認すること）。ビルド環境からネットワーク
> 取得できるかも先に確認する。取得できない場合は利用者にダウンロードを依頼する。

取得後にサイズと形式を確認:

Run: `ls -la assets/fonts/`
Expected: `NotoSansJP-Regular.{ttf,otf}` が存在（数 MB）。`font.rs` の `include_bytes!` のパスを実ファイル名に合わせる。

**Step 2: フォントバイトを埋め込むモジュールを追加**

`crates/fulgur-chart/src/font.rs` を作成:

```rust
//! 同梱フォントの提供。計測と描画で同一バイト列を使い、三者一致を保証する。

/// バイナリに埋め込んだ既定フォント（Noto Sans JP Regular）。
pub static DEFAULT_FONT: &[u8] =
    include_bytes!("../../../assets/fonts/NotoSansJP-Regular.ttf");
```

`lib.rs` に `pub mod font;` を追加。

**Step 3: フォントがパースできることのテスト**

`crates/fulgur-chart/src/font.rs` の末尾:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_font_parses() {
        let face = ttf_parser::Face::parse(DEFAULT_FONT, 0).unwrap();
        assert!(face.number_of_glyphs() > 0);
    }
}
```

**Step 4: テスト実行**

Run: `cargo test -p fulgur-chart font::tests::default_font_parses`
Expected: PASS。

**Step 5: コミット**

```bash
git add assets/ crates/fulgur-chart/src/font.rs crates/fulgur-chart/src/lib.rs
git commit -m "feat: 既定フォント Noto Sans JP を同梱し埋め込む"
```

---

## Task 3: 決定的な数値フォーマット `fmt_num`

座標・寸法の文字列化を一箇所に集約し、ロケール非依存・固定精度で決定性を担保する。

**Files:**
- Create: `crates/fulgur-chart/src/num.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`（`pub mod num;`）

**Step 1: 失敗するテストを書く**

`crates/fulgur-chart/src/num.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_two_decimals() {
        assert_eq!(fmt_num(1.0), "1");
        assert_eq!(fmt_num(1.005), "1");      // 偶数丸めの確認用に後述
        assert_eq!(fmt_num(1.5), "1.5");
        assert_eq!(fmt_num(1.25), "1.25");
        assert_eq!(fmt_num(1.234), "1.23");
        assert_eq!(fmt_num(-0.0), "0");        // 負ゼロを正規化
        assert_eq!(fmt_num(100.0), "100");
    }
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart num::`
Expected: FAIL（`fmt_num` 未定義）。

**Step 3: 最小実装**

```rust
//! 決定的な数値フォーマット。SVG 座標・寸法はすべてこれを通す。

/// 小数2桁に丸め、末尾の不要な 0 と小数点を除去する。
/// 負ゼロは "0" に正規化。ロケール非依存。
pub fn fmt_num(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let rounded = if rounded == 0.0 { 0.0 } else { rounded }; // -0.0 → 0.0
    let mut s = format!("{rounded:.2}");
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

> 注: `1.005` は f64 表現上 `1.00499…` となり "1" に丸まる。テストはこの実挙動に合わせている。

**Step 4: 成功を確認**

Run: `cargo test -p fulgur-chart num::`
Expected: PASS。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/num.rs crates/fulgur-chart/src/lib.rs
git commit -m "feat: 決定的な数値フォーマッタ fmt_num を追加"
```

---

## Task 4: 文字幅計測 `TextMeasurer`

同梱フォントの `hmtx` から advance width を読み、指定 px サイズでの文字列幅を返す。
レイアウト（右寄せ・中央寄せ・凡例幅）に使う。

**Files:**
- Create: `crates/fulgur-chart/src/text.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`（`pub mod text;`）

**Step 1: 失敗するテストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;

    #[test]
    fn empty_string_is_zero_width() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        assert_eq!(m.width("", 12.0), 0.0);
    }

    #[test]
    fn wider_text_is_wider() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let a = m.width("W", 12.0);
        let b = m.width("WWW", 12.0);
        assert!(b > a && b > 0.0);
    }

    #[test]
    fn scales_with_font_size() {
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let small = m.width("売上", 10.0);
        let large = m.width("売上", 20.0);
        assert!((large / small - 2.0).abs() < 1e-6);
    }
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart text::`
Expected: FAIL（`TextMeasurer` 未定義）。

**Step 3: 最小実装**

```rust
//! 同梱フォントの advance width に基づく決定的な文字列幅計測。

use ttf_parser::Face;

pub struct TextMeasurer<'a> {
    face: Face<'a>,
    units_per_em: f32,
}

impl<'a> TextMeasurer<'a> {
    pub fn new(font_bytes: &'a [u8]) -> Result<Self, String> {
        let face = Face::parse(font_bytes, 0).map_err(|e| e.to_string())?;
        let upem = face.units_per_em() as f32;
        Ok(Self { face, units_per_em: upem })
    }

    /// `text` を `size_px` で描いたときの advance 幅合計（px）。
    /// 字形シェーピング・カーニングは行わない（v1）。未収録文字は 0 幅扱い。
    pub fn width(&self, text: &str, size_px: f32) -> f32 {
        let scale = size_px / self.units_per_em;
        let mut total = 0.0_f32;
        for ch in text.chars() {
            if let Some(gid) = self.face.glyph_index(ch) {
                if let Some(adv) = self.face.glyph_hor_advance(gid) {
                    total += adv as f32 * scale;
                }
            }
        }
        total
    }
}
```

**Step 4: 成功を確認**

Run: `cargo test -p fulgur-chart text::`
Expected: PASS。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/text.rs crates/fulgur-chart/src/lib.rs
git commit -m "feat: 同梱フォントによる文字列幅計測 TextMeasurer を追加"
```

---

## Task 5: IR（中間表現）型の定義

DSL 非依存の正規化モデル。フロントエンドが補完済みの「決まった値」だけを持つ。

**Files:**
- Create: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`（`pub mod ir;`）

**Step 1: 型を定義（テスト不要の純データ定義。最小限から）**

```rust
//! IR: フロントエンド(DSL) と描画コアの安定境界。

/// 解決済みの色（不透明 RGB + アルファ）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32, // 0.0–1.0
}

/// 色は**データ点ごと**に持てる（pie のスライス別色が標準形のため）。
/// 長さ 1 のときは全点へブロードキャストする。`fill_at`/`stroke_at` で安全に参照する。
#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
    pub fill: Vec<Color>,    // len==1 でブロードキャスト、または点ごと
    pub stroke: Vec<Color>,  // 同上
    pub stroke_width: f64,
    pub area: bool,    // line のとき塗りつぶすか
    pub tension: f64,  // 0.0 = 直線
}

impl Series {
    /// i 番目のデータ点の塗り色。空なら黒、len==1 ならブロードキャスト。
    pub fn fill_at(&self, i: usize) -> Color { color_at(&self.fill, i) }
    pub fn stroke_at(&self, i: usize) -> Color { color_at(&self.stroke, i) }
}

fn color_at(colors: &[Color], i: usize) -> Color {
    match colors.len() {
        0 => Color { r: 0, g: 0, b: 0, a: 1.0 },
        1 => colors[0],
        _ => colors[i % colors.len()],
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisSpec {
    pub title: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub begin_at_zero: bool,
    pub grid: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LegendPos { Top, Bottom, Left, Right, None }

#[derive(Clone, Debug, PartialEq)]
pub enum ChartKind {
    Bar { horizontal: bool },
    Line,                 // area/tension は Series 側
    Pie { donut_ratio: f64 }, // 0.0 = pie, >0 = doughnut
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub series: Vec<Series>,
    pub categories: Vec<String>,
    pub x_axis: AxisSpec,
    pub y_axis: AxisSpec,
    pub legend: LegendPos,
    pub title: Option<String>,
    pub width: f64,
    pub height: f64,
}
```

**Step 2: ビルド確認**

Run: `cargo build -p fulgur-chart`
Expected: 成功。

**Step 3: コミット**

```bash
git add crates/fulgur-chart/src/ir.rs crates/fulgur-chart/src/lib.rs
git commit -m "feat: IR(ChartSpec ほか) 型を定義"
```

---

## Task 6: chart.js v4 デフォルト配色パレット

単色指定が無いときに使う循環パレット。chart.js v4 の既定色に準拠（実装前に実 v4 で検証）。

**Files:**
- Create: `crates/fulgur-chart/src/palette.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`

**Step 1: 失敗するテストを書く**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_through_palette() {
        let n = PALETTE.len();
        assert!(n >= 6);
        // 循環すること
        assert_eq!(palette_color(0), palette_color(n));
        assert_eq!(palette_color(1), palette_color(n + 1));
    }

    #[test]
    fn first_color_is_chartjs_blue() {
        let c = palette_color(0);
        assert_eq!((c.r, c.g, c.b), (54, 162, 235)); // #36A2EB
    }
}
```

**Step 2: 失敗を確認 → Step 3: 実装**

```rust
//! chart.js v4 既定カラーパレット。

use crate::ir::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color { Color { r, g, b, a: 1.0 } }

/// chart.js v4 の既定色循環（要実 v4 検証）。
pub static PALETTE: &[Color] = &[
    rgb(54, 162, 235),  // #36A2EB blue
    rgb(255, 99, 132),  // #FF6384 red
    rgb(255, 159, 64),  // #FF9F40 orange
    rgb(255, 205, 86),  // #FFCD56 yellow
    rgb(75, 192, 192),  // #4BC0C0 green
    rgb(153, 102, 255), // #9966FF purple
    rgb(201, 203, 207), // #C9CBCF grey
];

pub fn palette_color(i: usize) -> Color {
    PALETTE[i % PALETTE.len()]
}
```

**Step 4: 成功を確認 → Step 5: コミット**

```bash
git add crates/fulgur-chart/src/palette.rs crates/fulgur-chart/src/lib.rs
git commit -m "feat: chart.js v4 既定カラーパレットを追加"
```

> **要検証 (実装時):** chart.js v4 の正確なデフォルト色・自動配色挙動（透明度の扱い含む）を実 v4 で確認し、必要なら値を補正する。

---

## Task 7: 色文字列パーサ

chart.js spec の `backgroundColor` 等（`#RGB`/`#RRGGBB`/`rgba(...)`/CSS 名）を `Color` に変換。

**Files:**
- Create: `crates/fulgur-chart/src/color.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`

**Step 1: 失敗するテスト**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex6() {
        let c = parse_color("#36A2EB").unwrap();
        assert_eq!((c.r, c.g, c.b), (54, 162, 235));
        assert_eq!(c.a, 1.0);
    }
    #[test]
    fn parses_hex3() {
        let c = parse_color("#abc").unwrap();
        assert_eq!((c.r, c.g, c.b), (0xaa, 0xbb, 0xcc));
    }
    #[test]
    fn parses_rgba() {
        let c = parse_color("rgba(255, 99, 132, 0.5)").unwrap();
        assert_eq!((c.r, c.g, c.b), (255, 99, 132));
        assert!((c.a - 0.5).abs() < 1e-6);
    }
    #[test]
    fn rejects_garbage() {
        assert!(parse_color("not-a-color").is_none());
    }
}
```

**Step 2–4: 実装して通す**（`#RGB`/`#RRGGBB`/`rgb()`/`rgba()` を実装。CSS 名は最小セット or 省略可。失敗時 `None`）。

**Step 5: コミット**

```bash
git commit -am "feat: 色文字列パーサ parse_color を追加"
```

---

## Task 8: chart.js フロントエンド（spec → IR）

serde で chart.js v4 spec を受け、デフォルト補完・色解決して `ChartSpec` を返す。

**Files:**
- Create: `crates/fulgur-chart/src/frontend/mod.rs`
- Create: `crates/fulgur-chart/src/frontend/chartjs.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`（`pub mod frontend;`）

**Step 1: 失敗する統合テストを書く**

`crates/fulgur-chart/tests/frontend_chartjs.rs`:

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::ir::ChartKind;

#[test]
fn parses_minimal_bar_spec() {
    let json = r#"{
      "type": "bar",
      "data": {
        "labels": ["1月", "2月", "3月"],
        "datasets": [{ "label": "売上", "data": [120, 200, 150] }]
      }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { horizontal: false }));
    assert_eq!(spec.categories, vec!["1月", "2月", "3月"]);
    assert_eq!(spec.series.len(), 1);
    assert_eq!(spec.series[0].name, "売上");
    assert_eq!(spec.series[0].values, vec![120.0, 200.0, 150.0]);
    // 色未指定 → パレット先頭(#36A2EB) を全点へブロードキャスト(len==1)
    let c = spec.series[0].fill_at(0);
    assert_eq!((c.r, c.g, c.b), (54, 162, 235));
    assert_eq!(spec.series[0].fill.len(), 1); // bar は系列1色
}

#[test]
fn pie_with_per_slice_colors() {
    // pie の標準形: backgroundColor がスライス別の配列
    let json = r#"{ "type":"pie","data":{"labels":["a","b","c"],
      "datasets":[{"data":[1,2,3],"backgroundColor":["#ff0000","#00ff00","#0000ff"]}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].fill.len(), 3);
    assert_eq!(
        (spec.series[0].fill_at(2).r, spec.series[0].fill_at(2).g, spec.series[0].fill_at(2).b),
        (0, 0, 255)
    );
}

#[test]
fn pie_without_colors_uses_palette_per_slice() {
    let json = r#"{ "type":"pie","data":{"labels":["a","b"],
      "datasets":[{"data":[1,2]}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.series[0].fill.len(), 2); // pie はスライス別パレット
    assert_ne!(spec.series[0].fill_at(0), spec.series[0].fill_at(1));
}

#[test]
fn area_fill_string_mode_is_filled() {
    let json = r#"{ "type":"line","data":{"labels":["a"],
      "datasets":[{"data":[1],"fill":"origin"}]} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(spec.series[0].area);
}

#[test]
fn horizontal_bar_via_index_axis_y() {
    let json = r#"{ "type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
      "options":{"indexAxis":"y"} }"#;
    let spec = chartjs::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Bar { horizontal: true }));
}

#[test]
fn strict_rejects_unknown_top_level_key() {
    let json = r#"{ "type":"bar","data":{"labels":[],"datasets":[]},"wat":1 }"#;
    assert!(chartjs::parse(json, true).is_err());
    assert!(chartjs::parse(json, false).is_ok()); // 非strictは無視
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test frontend_chartjs`
Expected: FAIL（`chartjs::parse` 未定義）。

**Step 3: 実装**

`chartjs.rs` の骨子（serde 構造体 → IR 変換）:

```rust
//! chart.js v4 spec のデータ専用・静的サブセットを IR へ変換する。

use serde::Deserialize;
use crate::color::parse_color;
use crate::ir::*;
use crate::palette::palette_color;

#[derive(Deserialize)]
#[cfg_attr(test, derive(Debug))]
struct RawSpec {
    #[serde(rename = "type")]
    chart_type: String,
    data: RawData,
    #[serde(default)]
    options: RawOptions,
    // strict 検出用: 未知キーは deny_unknown_fields で拾う方式にしない
    // （非strictで無視するため）。strict は serde_json::Value で別途検査する。
}

#[derive(Deserialize, Default)]
struct RawOptions {
    #[serde(rename = "indexAxis")]
    index_axis: Option<String>,
    // plugins.title / plugins.legend / scales は段階的に追加
    #[serde(default)]
    plugins: RawPlugins,
    #[serde(default)]
    scales: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct RawPlugins {
    title: Option<RawTitle>,
    legend: Option<RawLegend>,
}

#[derive(Deserialize)]
struct RawTitle { #[serde(default)] display: bool, #[serde(default)] text: String }

#[derive(Deserialize)]
struct RawLegend { #[serde(default = "default_true")] display: bool, position: Option<String> }

fn default_true() -> bool { true }

#[derive(Deserialize)]
struct RawData { #[serde(default)] labels: Vec<String>, datasets: Vec<RawDataset> }

#[derive(Deserialize)]
struct RawDataset {
    #[serde(default)] label: String,
    data: Vec<f64>,
    // chart.js v4 は単一値・配列のどちらも取り得る（pie はスライス別配列が標準）。
    #[serde(rename = "backgroundColor")] background_color: Option<ScalarOrArray<String>>,
    #[serde(rename = "borderColor")] border_color: Option<ScalarOrArray<String>>,
    #[serde(rename = "borderWidth")] border_width: Option<f64>,
    // area の `fill` は bool だけでなく "origin"/"start"/"end" 等の文字列も取る。
    #[serde(default)] fill: FillSpec,
    #[serde(default)] tension: f64,
}

/// chart.js の「スカラ or 配列」を許容する untagged ヘルパ。
#[derive(Deserialize)]
#[serde(untagged)]
enum ScalarOrArray<T> { One(T), Many(Vec<T>) }

impl<T: Clone> ScalarOrArray<T> {
    fn into_vec(self) -> Vec<T> {
        match self { ScalarOrArray::One(v) => vec![v], ScalarOrArray::Many(v) => v }
    }
}

/// `fill`: bool / 文字列("origin"等) を受ける。v1 は「塗るか否か」だけ解釈。
#[derive(Deserialize, Default)]
#[serde(untagged)]
enum FillSpec { Bool(bool), Mode(String), #[default] Absent }

impl FillSpec {
    fn is_filled(&self) -> bool {
        match self { FillSpec::Bool(b) => *b, FillSpec::Mode(_) => true, FillSpec::Absent => false }
    }
}

pub fn parse(json: &str, strict: bool) -> Result<ChartSpec, String> {
    if strict {
        check_unknown_keys(json)?; // serde_json::Value を歩いて既知キー以外を検出
    }
    let raw: RawSpec = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let kind = match raw.chart_type.as_str() {
        "bar" => ChartKind::Bar {
            horizontal: raw.options.index_axis.as_deref() == Some("y"),
        },
        "line" => ChartKind::Line,
        "pie" => ChartKind::Pie { donut_ratio: 0.0 },
        "doughnut" => ChartKind::Pie { donut_ratio: 0.5 },
        other => return Err(format!("未対応の type: {other}")),
    };

    let is_pie = matches!(kind, ChartKind::Pie { .. });
    let series = raw.data.datasets.into_iter().enumerate().map(|(i, ds)| {
        let n = ds.data.len();
        // 色解決: 指定があれば点ごと（配列はそのまま、スカラは全点へ）。
        // 未指定なら pie はスライス別パレット、それ以外は系列1色。
        let fill = resolve_colors(ds.background_color, is_pie, i, n);
        let stroke = resolve_colors(ds.border_color, is_pie, i, n);
        Series {
            name: ds.label,
            values: ds.data,
            fill,
            stroke,
            stroke_width: ds.border_width.unwrap_or(default_border_width(&kind)),
            area: ds.fill.is_filled(),
            tension: ds.tension,
        }
    }).collect();

    // title / legend / axes のデフォルト補完（省略部分は実装時に埋める）
    Ok(ChartSpec {
        kind,
        series,
        categories: raw.data.labels,
        x_axis: AxisSpec { title: None, min: None, max: None, begin_at_zero: false, grid: true },
        y_axis: AxisSpec { title: None, min: None, max: None, begin_at_zero: true, grid: true },
        legend: legend_pos(&raw.options.plugins.legend),
        title: raw.options.plugins.title.as_ref()
            .filter(|t| t.display).map(|t| t.text.clone()),
        width: 800.0,
        height: 450.0,
    })
}

fn default_border_width(kind: &ChartKind) -> f64 {
    match kind { ChartKind::Line => 3.0, _ => 1.0 }
}

/// 指定色(スカラ/配列)を点ごとの Vec<Color> に解決する。
/// 未指定: pie はスライス別パレット(n色)、それ以外は系列インデックスの 1 色。
fn resolve_colors(
    spec: Option<ScalarOrArray<String>>,
    is_pie: bool,
    series_index: usize,
    n: usize,
) -> Vec<Color> {
    match spec {
        Some(s) => s.into_vec().iter()
            .map(|c| parse_color(c).unwrap_or(palette_color(series_index)))
            .collect(),
        None if is_pie => (0..n).map(palette_color).collect(),
        None => vec![palette_color(series_index)],
    }
}

fn legend_pos(l: &Option<RawLegend>) -> LegendPos {
    match l {
        Some(l) if !l.display => LegendPos::None,
        Some(l) => match l.position.as_deref() {
            Some("bottom") => LegendPos::Bottom,
            Some("left") => LegendPos::Left,
            Some("right") => LegendPos::Right,
            _ => LegendPos::Top,
        },
        None => LegendPos::Top,
    }
}

// strict 用: 既知キーのホワイトリストで Value を再帰検査する関数。
fn check_unknown_keys(json: &str) -> Result<(), String> {
    // 実装時に既知キー集合を定義して検査。未知キーがあれば Err(キー名)。
    let _ = json;
    Ok(())
}
```

**Step 4: 成功を確認**

Run: `cargo test -p fulgur-chart --test frontend_chartjs`
Expected: PASS（`strict_rejects_unknown_top_level_key` を通すため `check_unknown_keys` を実装すること）。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/frontend/ crates/fulgur-chart/src/lib.rs crates/fulgur-chart/tests/
git commit -m "feat: chart.js v4 フロントエンド(spec→IR)を実装"
```

---

## Task 9: スケールと nice ticks

数値軸の範囲決定（begin_at_zero / min / max）と、見やすい目盛り値の生成。

**Files:**
- Create: `crates/fulgur-chart/src/scale.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`

**Step 1: 失敗するテスト**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ticks_round_numbers() {
        let t = nice_ticks(0.0, 200.0, 5);
        assert_eq!(t.ticks, vec![0.0, 50.0, 100.0, 150.0, 200.0]);
    }

    #[test]
    fn maps_value_to_pixel() {
        // [0,200] を [0,400px] に。y は上下反転（後段で扱う）ためここは線形のみ。
        let s = LinearScale::new(0.0, 200.0, 0.0, 400.0);
        assert!((s.map(0.0) - 0.0).abs() < 1e-6);
        assert!((s.map(100.0) - 200.0).abs() < 1e-6);
        assert!((s.map(200.0) - 400.0).abs() < 1e-6);
    }
}
```

**Step 2–4:** `LinearScale { map() }` と `nice_ticks(min, max, target_count) -> NiceTicks { min, max, step, ticks }`（拡張版 Wilkinson / d3 風の 1-2-5 ステップ）を実装。

**Step 5: コミット**

```bash
git commit -am "feat: 線形スケールと nice ticks を追加"
```

---

## Task 10: Scene（描画プリミティブ）と SVG シリアライザ

`Scene` は幾何 + スタイルだけを持つ中間表現。SVG 文字列化はここに集約し、すべて `fmt_num` を通す。

**Files:**
- Create: `crates/fulgur-chart/src/scene.rs`
- Create: `crates/fulgur-chart/src/svg.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`

**Step 1: 失敗するテスト（SVG出力の決定性）**

```rust
// crates/fulgur-chart/src/svg.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::*;
    use crate::ir::Color;

    #[test]
    fn renders_rect_deterministically() {
        let scene = Scene {
            width: 100.0, height: 50.0,
            items: vec![Prim::Rect {
                x: 1.005, y: 2.0, w: 10.0, h: 20.0,
                fill: Color { r: 54, g: 162, b: 235, a: 1.0 },
            }],
        };
        let svg = render_svg(&scene);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains(r#"width="100" height="50""#));
        assert!(svg.contains(r#"<rect x="1" y="2" width="10" height="20" fill="#36a2eb"/>"#));
        assert!(svg.trim_end().ends_with("</svg>"));
        // 二度呼んでも完全一致（決定性）
        assert_eq!(svg, render_svg(&scene));
    }

    #[test]
    fn renders_text_with_font_family() {
        let scene = Scene {
            width: 100.0, height: 50.0,
            items: vec![Prim::Text {
                x: 5.0, y: 10.0, size: 12.0, anchor: Anchor::Middle,
                fill: Color { r: 0, g: 0, b: 0, a: 1.0 },
                content: "売上".into(),
            }],
        };
        let svg = render_svg(&scene);
        assert!(svg.contains(r#"font-family="Noto Sans JP, sans-serif""#));
        assert!(svg.contains(r#"text-anchor="middle""#));
        assert!(svg.contains(">売上</text>"));
    }

    #[test]
    fn escapes_xml_special_chars() {
        let scene = Scene {
            width: 10.0, height: 10.0,
            items: vec![Prim::Text {
                x: 0.0, y: 0.0, size: 10.0, anchor: Anchor::Start,
                fill: Color { r: 0, g: 0, b: 0, a: 1.0 },
                content: "a<b & c>d".into(),
            }],
        };
        let svg = render_svg(&scene);
        assert!(svg.contains("a&lt;b &amp; c&gt;d"));
    }
}
```

**Step 2: 失敗を確認**

**Step 3: 実装**

`scene.rs`: `Prim` 列挙（`Rect`/`Line`/`Polyline`/`Path`/`Circle`/`Arc`/`Text`）、`Anchor`（Start/Middle/End）、`Scene { width, height, items }`。

`svg.rs`:
```rust
//! Scene → 決定的な SVG 文字列。座標はすべて fmt_num を通す。

use crate::ir::Color;
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use std::fmt::Write;

pub fn render_svg(scene: &Scene) -> String {
    let mut s = String::new();
    write!(
        s,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" \
         viewBox=\"0 0 {} {}\">",
        fmt_num(scene.width), fmt_num(scene.height),
        fmt_num(scene.width), fmt_num(scene.height),
    ).unwrap();
    for item in &scene.items {
        write_prim(&mut s, item);
    }
    s.push_str("</svg>\n");
    s
}

fn color_hex(c: &Color) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn write_prim(s: &mut String, p: &Prim) {
    match p {
        Prim::Rect { x, y, w, h, fill } => {
            write!(s, r#"<rect x="{}" y="{}" width="{}" height="{}" fill="{}"/>"#,
                fmt_num(*x), fmt_num(*y), fmt_num(*w), fmt_num(*h), color_hex(fill)).unwrap();
        }
        Prim::Text { x, y, size, anchor, fill, content } => {
            let a = match anchor { Anchor::Start => "start", Anchor::Middle => "middle", Anchor::End => "end" };
            write!(s,
                r#"<text x="{}" y="{}" font-family="Noto Sans JP, sans-serif" font-size="{}" text-anchor="{}" fill="{}">{}</text>"#,
                fmt_num(*x), fmt_num(*y), fmt_num(*size), a, color_hex(fill), xml_escape(content)).unwrap();
        }
        // Line / Polyline / Path / Circle / Arc も同様に fmt_num を通して実装
        _ => { /* 実装時に埋める */ }
    }
}
```

> 注: alpha < 1.0 のときは `fill-opacity` を併記する（実装時に追加）。

**Step 4: 成功を確認 → Step 5: コミット**

```bash
git commit -am "feat: Scene と決定的 SVG シリアライザを追加"
```

---

## Task 11: bar チャートのレイアウト（縦スライス完成）

IR → Scene を bar について実装し、end-to-end でスナップショットテスト。

**Files:**
- Create: `crates/fulgur-chart/src/layout/mod.rs`
- Create: `crates/fulgur-chart/src/layout/bar.rs`
- Create: `crates/fulgur-chart/src/render.rs`（`render_chart(spec) -> String` 公開API）
- Modify: `crates/fulgur-chart/src/lib.rs`
- Test: `crates/fulgur-chart/tests/snapshot_bar.rs`

**Step 1: 失敗するスナップショットテスト**

```rust
use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

#[test]
fn bar_chart_snapshot() {
    let json = r#"{
      "type": "bar",
      "data": {
        "labels": ["1月", "2月", "3月"],
        "datasets": [{ "label": "売上", "data": [120, 200, 150] }]
      },
      "options": { "plugins": { "title": { "display": true, "text": "四半期売上" } } }
    }"#;
    let spec = chartjs::parse(json, false).unwrap();
    let svg = render_chart(&spec);
    insta::assert_snapshot!(svg);
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test snapshot_bar`
Expected: FAIL（`render_chart`/`layout::bar` 未実装）。

**Step 3: 実装**

- `layout::bar::build(spec, measurer) -> Scene`: プロット領域算出（タイトル・凡例・軸ラベル幅を `TextMeasurer` で確保）、`nice_ticks` で y 目盛り、各カテゴリ×系列の棒矩形、軸線・グリッド・ラベル `<text>`、凡例を `Prim` で積む。横棒は x/y を入れ替え。
- `render::render_chart(spec)`: `TextMeasurer::new(DEFAULT_FONT)` → `layout::dispatch(spec)` で kind 別レイアウト → `svg::render_svg(scene)`。

**Step 4: スナップショット受理**

Run: `cargo test -p fulgur-chart --test snapshot_bar`
→ `cargo insta review`（または `INSTA_UPDATE=always cargo test`）で出力 SVG を目視確認して受理。
Expected: 受理後 PASS。SVG を実際にブラウザ等で開いて見た目を確認する。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/layout/ crates/fulgur-chart/src/render.rs \
        crates/fulgur-chart/src/lib.rs crates/fulgur-chart/tests/
git commit -m "feat: bar チャートのレイアウトと end-to-end SVG 生成を実装"
```

---

## Task 12: CLI `render`（縦スライスを CLI から叩く）

**Files:**
- Modify: `crates/fulgur-chart-cli/src/main.rs`
- Create: `crates/fulgur-chart-cli/tests/cli.rs`
- Add dev-dep: `assert_cmd`, `predicates` を CLI クレートに

**Step 1: 失敗する CLI 統合テスト**

```rust
// assert_cmd を使用
#[test]
fn renders_bar_spec_to_svg_stdout() {
    let spec = r#"{"type":"bar","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]}}"#;
    let mut cmd = assert_cmd::Command::cargo_bin("fulgur-chart").unwrap();
    let out = cmd.args(["render", "-", "-o", "-"]).write_stdin(spec).assert().success();
    let svg = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    assert!(svg.starts_with("<svg"));
}

#[test]
fn invalid_json_exits_1() {
    let mut cmd = assert_cmd::Command::cargo_bin("fulgur-chart").unwrap();
    cmd.args(["render", "-", "-o", "-"]).write_stdin("{ not json")
        .assert().failure().code(1);
}
```

**Step 2: 失敗を確認**

**Step 3: 実装**

clap derive で `Cli { command: Render }`。`Render { spec: String, output: String, format: Option<Format>, width, height, scale, font, strict, dsl }`。
`-` を stdin/stdout として扱う。format は出力拡張子 or `--format`、stdout 既定 svg。
exit code: 入力エラー=1 / strict 違反=2 / 描画・IO=3。`std::process::exit` で制御。

**Step 4: 成功を確認 → Step 5: コミット**

```bash
git commit -am "feat: CLI render コマンド(SVG, stdin/stdout, 終了コード)を実装"
```

---

## Task 13: line + area チャート

**Files:** `crates/fulgur-chart/src/layout/line.rs`、`tests/snapshot_line.rs`

bar と同じ TDD サイクル（スナップショット）。実装ポイント:
- カテゴリを等間隔の x、値を y スケールでマップし `Polyline`（`tension>0` なら Catmull-Rom→ベジエの `Path`）。
- `area`(Series.area=true) は下端まで閉じた `Path` を半透明 fill。
- マーカー（`pointRadius>0`）は `Circle`。
- 軸・グリッド・凡例は bar のレイアウト補助を共通化（`layout::common` に抽出＝DRY）。

各種スナップショットを受理してコミット:
```bash
git commit -am "feat: line / area チャートを実装"
```

---

## Task 14: pie / doughnut チャート

**Files:** `crates/fulgur-chart/src/layout/pie.rs`、`tests/snapshot_pie.rs`

実装ポイント:
- 単一データセットの各値→角度（合計に対する比）。開始角は 12 時方向、時計回り（chart.js 準拠を実装時確認）。
- 各扇形は `Arc`（SVG `<path>` の `A` コマンド）。`donut_ratio>0` は内側半径を空ける。
- 軸なし。凡例はカテゴリ単位。
- 0 値・合計 0 のエッジケースをテスト。

```bash
git commit -am "feat: pie / doughnut チャートを実装"
```

---

## Task 15: PNG ラスタライズ（SVG → resvg/tiny-skia）

**Files:** `crates/fulgur-chart/src/raster.rs`、`tests/png_smoke.rs`、CLI 側で `--format png` を有効化

**Step 1: 失敗するテスト（PNG が妥当なサイズで生成される）**

```rust
#[test]
fn rasterizes_to_png_bytes() {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"10\"><rect x=\"0\" y=\"0\" width=\"20\" height=\"10\" fill=\"#36a2eb\"/></svg>";
    let png = fulgur_chart::raster::svg_to_png(svg, 1.0).unwrap();
    assert_eq!(&png[1..4], b"PNG");           // PNG シグネチャ
}
```

**Step 3: 実装**

```rust
//! SVG → PNG。resvg/tiny-skia を使い、fontdb に同梱フォントをロードして三者一致を保つ。

use resvg::{tiny_skia, usvg};
use crate::font::DEFAULT_FONT;

pub fn svg_to_png(svg: &str, scale: f32) -> Result<Vec<u8>, String> {
    let mut fontdb = usvg::fontdb::Database::new();
    fontdb.load_font_data(DEFAULT_FONT.to_vec());
    let opt = usvg::Options { fontdb: fontdb.into(), ..Default::default() };
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let size = tree.size();
    let w = (size.width() * scale).round() as u32;
    let h = (size.height() * scale).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("invalid size")?;
    let ts = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, ts, &mut pixmap.as_mut());
    pixmap.encode_png().map_err(|e| e.to_string())
}
```

> **要確認 (advisor 指摘・高リスク行):** 上の構造体リテラル `usvg::Options { fontdb: fontdb.into(), .. }`
> は usvg 0.45 では**コンパイルしない可能性が高い**。0.45 の `Options.fontdb` は `Arc<fontdb::Database>` で、
> `options.fontdb_mut()`（内部で `Arc::make_mut`）経由でフォントをロードする形が定石。インストール済み
> バージョンの実 API（`Options::default()` → `opt.fontdb_mut().load_font_data(...)`、`Tree::from_str` の
> シグネチャ）を `cargo doc -p usvg --open` 等で確認し、スニペットを鵜呑みにしない。

CLI: `--format png`/`--scale` 時に `svg_to_png` を呼んでバイナリ出力。

**Step 4–5:** テスト通過 → コミット。
```bash
git commit -am "feat: SVG→PNG ラスタライズと CLI --format png を実装"
```

---

## Task 16: 仕上げ（README・examples・data labels・legend 位置）

- `--strict` の `check_unknown_keys` を全既知キーで完成させる。
- `options.plugins.datalabels` 相当の最小データラベル（棒・点の値表示）。
- `legend` の left/right 配置のレイアウト対応。
- `examples/` に bar/line/area/pie の spec と期待 SVG、Fulgur 連携の HTML サンプル。
- `README.md`（インストール・使い方・Fulgur 連携・フォント同梱の注意・決定性の保証）。
- `CHANGELOG.md`、`LICENSE-MIT`/`LICENSE-APACHE`。
- CI（`cargo test` + `cargo fmt --check` + `cargo clippy`）を `.github/workflows/` に。

各項目を小さくコミットする。

```bash
git commit -m "docs: README / examples / CHANGELOG を追加"
```

---

## 完了の定義（Definition of Done）

- `cargo test` 全通過（単体 + スナップショット + CLI 統合 + PNG smoke）。
- `fulgur-chart render <spec> -o out.svg` が bar/line/area/pie で妥当な SVG を生成。
- 同一入力で SVG が byte-identical（決定性テストが保証）。
- 生成 SVG を Fulgur で PDF 化し、チャートがベクターで埋め込まれることを手動確認。
- README に使い方と Fulgur 連携手順が記載されている。

## 実装時に要検証の事項（設計ドキュメントより再掲）

- chart.js v4 の自動配色アルゴリズムと既定色の正確な再現。
- nice-ticks を chart.js の目盛り挙動にどこまで寄せるか。
- usvg 0.45 の `Options.fontdb` / `Tree::from_str` の正確な API シグネチャ。
- pie の開始角・回転方向の chart.js 準拠。
