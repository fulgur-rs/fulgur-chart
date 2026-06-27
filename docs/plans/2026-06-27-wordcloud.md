# WordCloud Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** QuickChart 互換の `wordCloud` チャート種別を追加する。単語の重要度をフォントサイズで表現し、アルキメデス螺旋配置で重ならず配置する。

**Architecture:** `ChartKind::WordCloud { entries, ... }` を IR に追加し、`layout/wordcloud.rs` でアルキメデス螺旋 + AABB 衝突検出による配置を実装。回転テキスト（0°/−90°）は `Prim::Text.rotate_deg` フィールドを追加して SVG `transform="rotate(...)"` で実現する。

**Tech Stack:** Rust, serde_json, schemars, insta (snapshot テスト)

---

### Task 1: `Prim::Text` に `rotate_deg` フィールドを追加し SVG レンダリングを対応させる

**Files:**
- Modify: `crates/fulgur-chart/src/scene.rs`
- Modify: `crates/fulgur-chart/src/svg.rs`
- Modify (一括): 以下 14 ファイルの `Prim::Text { ... }` 構築を全て更新（`rotate_deg: None,` を追加）

**Step 1: scene.rs に `rotate_deg` フィールドを追加**

`Prim::Text` variant（`scene.rs`）に最後のフィールドとして追加する:

```rust
Prim::Text {
    x: f64,
    y: f64,
    size: f64,
    anchor: Anchor,
    fill: Color,
    content: String,
    rotate_deg: Option<f64>, // Some(deg) → SVG transform="rotate(deg,x,y)"
},
```

**Step 2: コンパイルエラーを確認して既存構築を更新**

```bash
cd crates/fulgur-chart && cargo build 2>&1 | grep "error\[E0063\]"
```

Expected: `missing field \`rotate_deg\`` が各 `Prim::Text { ... }` 構築箇所に出る。

以下 14 ファイルの全 `Prim::Text { ... }` ブロックに `rotate_deg: None,` を追加する:
- `src/layout/bar.rs`, `src/layout/pie.rs`, `src/layout/polar_area.rs`
- `src/layout/matrix.rs`, `src/layout/progress.rs`, `src/layout/scatter.rs`
- `src/layout/line.rs`, `src/layout/gauge.rs`, `src/layout/radar.rs`
- `src/layout/treemap.rs`, `src/layout/common.rs`, `src/layout/outlabeled_pie.rs`
- `src/raster_direct.rs`, `src/svg.rs`

各ファイルで `content: <expr>,` の直後の行に追加するパターン:
```rust
content: some_expr,
rotate_deg: None,   // ← 追加
```

**Step 3: svg.rs の Text 描画に transform を追加**

`write_prim` 関数の `Prim::Text { ... }` アームを更新:

```rust
Prim::Text {
    x,
    y,
    size,
    anchor,
    fill,
    content,
    rotate_deg,
} => {
    let xv = fmt_num(*x);
    let yv = fmt_num(*y);
    let size = fmt_num(*size);
    let anchor = match anchor {
        Anchor::Start => "start",
        Anchor::Middle => "middle",
        Anchor::End => "end",
    };
    let hex = color_hex(fill);
    let op = opacity_attr("fill-opacity", fill.a);
    let escaped = xml_escape(content);
    let fam = xml_escape_attr(font_family);
    let transform = rotate_deg
        .map(|d| format!(r#" transform="rotate({},{},{})"#, fmt_num(d), xv, yv))
        .unwrap_or_default();
    write!(
        s,
        r#"<text x="{xv}" y="{yv}"{transform} font-family="{fam}" font-size="{size}" text-anchor="{anchor}" fill="{hex}"{op}>{escaped}</text>"#
    )
    .unwrap();
}
```

**Step 4: svg.rs のデストラクチャパターン（`match` の他の箇所）を確認**

`src/svg.rs` 内の `Prim::Text {` パターンマッチを全て `rotate_deg,` を含む形に更新する（match 腕が複数ある場合）。

**Step 5: ビルドが通ることを確認**

```bash
cd crates/fulgur-chart && cargo build 2>&1 | grep "^error"
```

Expected: エラーなし

**Step 6: テスト通過を確認**

```bash
cd crates/fulgur-chart && cargo test --lib -q 2>&1 | tail -5
```

Expected: `196 passed; 0 failed`

**Step 7: コミット**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat/wordcloud
git add crates/fulgur-chart/src/scene.rs crates/fulgur-chart/src/svg.rs \
    crates/fulgur-chart/src/layout/ crates/fulgur-chart/src/raster_direct.rs
git commit -m "feat(scene): add rotate_deg to Prim::Text for SVG transform support"
```

---

### Task 2: IR に `WordEntry` と `ChartKind::WordCloud` を追加

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`

**Step 1: `WordEntry` 構造体を追加**

`ir.rs` の `BoxPoint` 定義の後に追加:

```rust
/// ワードクラウドの 1 単語エントリ。
#[derive(Clone, Debug, PartialEq)]
pub struct WordEntry {
    /// 表示テキスト。
    pub text: String,
    /// フォントサイズ (px)。入力 data[] の値をそのまま使う。
    pub size: f64,
    /// 塗り色。None のときはパレット巡回。
    pub color: Option<Color>,
}
```

**Step 2: `ChartKind::WordCloud` を追加**

`ChartKind` enum の `Treemap` variant の後に追加:

```rust
/// QuickChart / chartjs-chart-wordcloud 互換のワードクラウド。
/// 単語の重要度をフォントサイズで表現し、アルキメデス螺旋で非重複配置する。
WordCloud {
    entries: Vec<WordEntry>,
    /// 最小回転角度 (度)。デフォルト: -90.0
    min_rotation: f64,
    /// 最大回転角度 (度)。デフォルト: 0.0
    max_rotation: f64,
    /// 離散回転ステップ数。デフォルト: 2
    rotation_steps: u32,
    /// 各単語の周囲パディング (px)。デフォルト: 2.0
    padding: f64,
},
```

**Step 3: ビルドが通ることを確認**

```bash
cd crates/fulgur-chart && cargo build 2>&1 | grep "^error"
```

Expected: `model.rs` などで exhaustive match エラーが出ることを確認（次タスクで修正）。もし出なければ model.rs を確認する。

**Step 4: コミット**

```bash
git add crates/fulgur-chart/src/ir.rs
git commit -m "feat(ir): add WordEntry and ChartKind::WordCloud"
```

---

### Task 3: `schema/chartjs.rs` に `WordCloudSpec` を追加 + schema roundtrip テスト

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`
- Modify: `crates/fulgur-chart/tests/frontend_chartjs.rs`

**Step 1: `ChartJsSpec` enum に `WordCloud` variant を追加**

`ChartJsSpec` の `OutlabeledDoughnut` の後に追加:

```rust
#[serde(rename = "wordCloud")]
WordCloud(WordCloudSpec),
```

**Step 2: `WordCloudSpec` 構造体群を追加**

`schema/chartjs.rs` の末尾に追加（Treemap セクションの後）:

```rust
// ────────────────────────────────────────────────
// WordCloud chart (QuickChart / chartjs-chart-wordcloud)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudSpec {
    pub data: WordCloudData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<WordCloudOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudData {
    pub labels: Vec<String>,
    pub datasets: Vec<WordCloudDataset>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordCloudDataset {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Word sizes in pixels (same length as labels).
    pub data: Vec<f64>,
    /// Optional color(s) for words. Scalar = all same, array = per-word.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ScalarOrArray<ColorString>>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elements: Option<WordCloudElements>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<WordCloudPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WordCloudElements {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word: Option<WordElementOptions>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WordElementOptions {
    /// Minimum rotation angle in degrees. Default: -90.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_rotation: Option<f64>,
    /// Maximum rotation angle in degrees. Default: 0.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rotation: Option<f64>,
    /// Number of discrete rotation steps. Default: 2
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_steps: Option<u32>,
    /// Padding around each word in pixels. Default: 2.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct WordCloudPlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<TitlePlugin>,
}
```

**Step 3: ビルドを確認**

```bash
cd crates/fulgur-chart && cargo build 2>&1 | grep "^error"
```

**Step 4: schema roundtrip テストを `tests/frontend_chartjs.rs` に追加**

ファイル末尾に追加:

```rust
#[test]
fn wordcloud_schema_roundtrip() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;

    let json = r##"{
        "type": "wordCloud",
        "data": {
            "labels": ["Rust", "SVG", "Chart"],
            "datasets": [{"data": [90, 60, 45], "color": ["#e63946", "#457b9d", "#2a9d8f"]}]
        },
        "options": {
            "elements": {"word": {"minRotation": -90, "maxRotation": 0, "rotationSteps": 2, "padding": 2}}
        }
    }"##;
    let spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(spec, ChartJsSpec::WordCloud(_)));
    // strict parser も受理することを確認
    assert!(
        chartjs::parse(json, true).is_ok(),
        "strict parser should accept wordCloud"
    );

    // scalar color も受理
    let scalar = r##"{"type":"wordCloud","data":{"labels":["Hi"],"datasets":[{"data":[40],"color":"#ff0000"}]}}"##;
    let s: ChartJsSpec = serde_json::from_str(scalar).unwrap();
    assert!(matches!(s, ChartJsSpec::WordCloud(_)));
}
```

**Step 5: テスト実行**

```bash
cd crates/fulgur-chart && cargo test wordcloud_schema_roundtrip -- --nocapture 2>&1
```

Expected: `test wordcloud_schema_roundtrip ... ok`（パーサー未実装なので `chartjs::parse` は失敗する可能性あり — その場合は `chartjs::parse` のアサートだけ後回しにしてよい）

**Step 6: コミット**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat(schema): add WordCloudSpec to ChartJsSpec"
```

---

### Task 4: `model.rs` に `chart_type_name` アームを追加

**Files:**
- Modify: `crates/fulgur-chart/src/model.rs`

**Step 1: `chart_type_name` match に `WordCloud` アームを追加**

`ChartKind::Treemap => "treemap",` の後に追加:

```rust
ChartKind::WordCloud { .. } => "wordCloud",
```

**Step 2: ビルドとテストを確認**

```bash
cd crates/fulgur-chart && cargo build 2>&1 | grep "^error"
```

Expected: エラーなし（model.rs の exhaustive match が解決される）

**Step 3: コミット**

```bash
git add crates/fulgur-chart/src/model.rs
git commit -m "feat(model): add WordCloud arm to chart_type_name"
```

---

### Task 5: `guard.rs` に wordcloud バリデーションを追加

**Files:**
- Modify: `crates/fulgur-chart/src/guard.rs`

**Step 1: 定数を追加**

`guard.rs` の既存定数（例: `MAX_TREEMAP_INPUT_ROWS` のそば）に追加:

```rust
/// wordcloud の単語数上限 (DoS 対策)。
const MAX_WORDCLOUD_WORDS: usize = 500;
/// wordcloud の 1 語あたりバイト長上限 (SVG サイズ攻撃対策)。
const MAX_WORDCLOUD_WORD_BYTES: usize = 200;
```

**Step 2: `validate_spec` に wordcloud ブロックを追加**

`validate_spec` 関数内の末尾（`Ok(())` の直前）に追加:

```rust
// --- wordcloud 単語数・ラベル長 ---
if let ChartKind::WordCloud { entries, .. } = &spec.kind {
    if entries.len() > MAX_WORDCLOUD_WORDS {
        return Err(format!(
            "wordcloud の単語数 {} が上限 {} を超えています",
            entries.len(),
            MAX_WORDCLOUD_WORDS,
        ));
    }
    for e in entries {
        if e.text.len() > MAX_WORDCLOUD_WORD_BYTES {
            return Err(format!(
                "wordcloud: 単語 {:?}... の長さ {} バイトが上限 {} を超えています",
                &e.text[..e.text.len().min(20)],
                e.text.len(),
                MAX_WORDCLOUD_WORD_BYTES,
            ));
        }
    }
}
```

**Step 3: guard のユニットテストを追加**

`guard.rs` のテストモジュール末尾に追加:

```rust
#[test]
fn wordcloud_too_many_words() {
    use crate::ir::{ChartKind, WordEntry};
    let entries: Vec<WordEntry> = (0..=500)
        .map(|i| WordEntry {
            text: format!("word{i}"),
            size: 12.0,
            color: None,
        })
        .collect();
    let s = ChartSpec {
        kind: ChartKind::WordCloud {
            entries,
            min_rotation: -90.0,
            max_rotation: 0.0,
            rotation_steps: 2,
            padding: 2.0,
        },
        series: vec![],
        categories: vec![],
        ..base_spec()
    };
    assert!(validate_spec(&s, &default_limits()).is_err());
}

#[test]
fn wordcloud_label_too_long() {
    use crate::ir::{ChartKind, WordEntry};
    let s = ChartSpec {
        kind: ChartKind::WordCloud {
            entries: vec![WordEntry {
                text: "a".repeat(201),
                size: 12.0,
                color: None,
            }],
            min_rotation: -90.0,
            max_rotation: 0.0,
            rotation_steps: 2,
            padding: 2.0,
        },
        series: vec![],
        categories: vec![],
        ..base_spec()
    };
    assert!(validate_spec(&s, &default_limits()).is_err());
}

#[test]
fn wordcloud_valid_passes_guard() {
    use crate::ir::{ChartKind, WordEntry};
    let s = ChartSpec {
        kind: ChartKind::WordCloud {
            entries: vec![
                WordEntry { text: "Rust".to_string(), size: 80.0, color: None },
                WordEntry { text: "SVG".to_string(), size: 60.0, color: None },
            ],
            min_rotation: -90.0,
            max_rotation: 0.0,
            rotation_steps: 2,
            padding: 2.0,
        },
        series: vec![],
        categories: vec![],
        ..base_spec()
    };
    assert!(validate_spec(&s, &default_limits()).is_ok());
}
```

**Step 4: guard テスト実行**

```bash
cd crates/fulgur-chart && cargo test guard -- --nocapture 2>&1 | tail -10
```

Expected: 3 つの新テストが全て pass

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/guard.rs
git commit -m "feat(guard): add wordcloud word count and label byte validation"
```

---

### Task 6: `frontend/chartjs.rs` に wordCloud パーサーを追加

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`

**Step 1: `parse` 関数の type 分岐に wordCloud を追加**

`parse_treemap` 分岐の後に追加:

```rust
if matches!(chart_type.as_deref(), Some("wordCloud") | Some("word")) {
    if strict {
        check_unknown_keys_wordcloud(json)?;
    }
    return parse_wordcloud(json);
}
```

**Step 2: `check_unknown_keys_wordcloud` を追加**

既存の `check_unknown_keys_treemap` の近くに追加:

```rust
fn check_unknown_keys_wordcloud(json: &str) -> Result<(), String> {
    use crate::schema::chartjs::{ChartJsSpec, WordCloudSpec};
    let _: WordCloudSpec = serde_json::from_str(
        &serde_json::from_str::<serde_json::Value>(json)
            .map_err(|e| e.to_string())?
            .get("data")
            .and_then(|v| {
                serde_json::to_string(&serde_json::json!({
                    "data": v,
                    "options": serde_json::from_str::<serde_json::Value>(json)
                        .ok()
                        .and_then(|v| v.get("options").cloned())
                        .unwrap_or(serde_json::Value::Null)
                }))
                .ok()
            })
            .unwrap_or_default(),
    )
    .map_err(|e| format!("wordCloud strict: {e}"))?;
    Ok(())
}
```

Note: `check_unknown_keys_*` の実装パターンは既存の `check_unknown_keys_treemap` などを参照すること。シンプルに `ChartJsSpec` での deserialize で `deny_unknown_fields` を利用できる。以下の実装で OK:

```rust
fn check_unknown_keys_wordcloud(json: &str) -> Result<(), String> {
    serde_json::from_str::<crate::schema::chartjs::ChartJsSpec>(json)
        .map(|_| ())
        .map_err(|e| format!("wordCloud (strict): {e}"))
}
```

**Step 3: `parse_wordcloud` 関数を追加**

ファイル末尾（`parse_treemap` の近く）に追加:

```rust
/// wordCloud 専用パース。labels + datasets[0].data を WordEntry に変換する。
fn parse_wordcloud(json: &str) -> Result<ChartSpec, String> {
    #[derive(serde::Deserialize)]
    struct WcWrapper {
        data: WcData,
        #[serde(default)]
        options: Option<WcOptions>,
        #[serde(default)]
        width: Option<f64>,
        #[serde(default)]
        height: Option<f64>,
    }
    #[derive(serde::Deserialize)]
    struct WcData {
        labels: Vec<String>,
        datasets: Vec<WcDataset>,
    }
    #[derive(serde::Deserialize)]
    struct WcDataset {
        data: Vec<f64>,
        #[serde(default)]
        color: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize, Default)]
    struct WcOptions {
        #[serde(default)]
        elements: Option<WcElements>,
        #[serde(default)]
        plugins: Option<serde_json::Value>,
        #[serde(default)]
        theme: Option<serde_json::Value>,
    }
    #[derive(serde::Deserialize, Default)]
    struct WcElements {
        #[serde(default)]
        word: Option<WcWordOpts>,
    }
    #[derive(serde::Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct WcWordOpts {
        #[serde(default)]
        min_rotation: Option<f64>,
        #[serde(default)]
        max_rotation: Option<f64>,
        #[serde(default)]
        rotation_steps: Option<u32>,
        #[serde(default)]
        padding: Option<f64>,
    }

    let raw: WcWrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if raw.data.datasets.is_empty() {
        return Err("wordCloud チャートには dataset が 1 つ必要です".to_string());
    }
    let ds = &raw.data.datasets[0];
    if ds.data.len() != raw.data.labels.len() {
        return Err(format!(
            "wordCloud: labels ({}) と data ({}) の長さが一致しません",
            raw.data.labels.len(),
            ds.data.len(),
        ));
    }

    // color の解析（スカラー or 配列）
    let colors: Vec<Option<crate::ir::Color>> = match &ds.color {
        None => vec![None; ds.data.len()],
        Some(serde_json::Value::String(s)) => {
            let c = parse_color(s).ok();
            vec![c; ds.data.len()]
        }
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|v| v.as_str().and_then(|s| parse_color(s).ok()))
            .collect(),
        _ => vec![None; ds.data.len()],
    };

    let entries: Vec<crate::ir::WordEntry> = raw
        .data
        .labels
        .iter()
        .zip(ds.data.iter())
        .zip(colors.iter())
        .map(|((text, &size), color)| crate::ir::WordEntry {
            text: text.clone(),
            size,
            color: *color,
        })
        .collect();

    let word_opts = raw
        .options
        .as_ref()
        .and_then(|o| o.elements.as_ref())
        .and_then(|e| e.word.as_ref());

    let min_rotation = word_opts.and_then(|w| w.min_rotation).unwrap_or(-90.0);
    let max_rotation = word_opts.and_then(|w| w.max_rotation).unwrap_or(0.0);
    let rotation_steps = word_opts.and_then(|w| w.rotation_steps).unwrap_or(2).max(1);
    let padding = word_opts.and_then(|w| w.padding).unwrap_or(2.0);

    // plugins から title を取得（共通ヘルパーを使う）
    let title = raw
        .options
        .as_ref()
        .and_then(|o| o.plugins.as_ref())
        .and_then(|p| p.get("title"))
        .and_then(|t| {
            if t.get("display")?.as_bool()? {
                t.get("text")?.as_str().map(|s| s.to_string())
            } else {
                None
            }
        });

    // theme
    let theme = raw
        .options
        .as_ref()
        .and_then(|o| o.theme.as_ref())
        .map(|t| parse_theme(t))
        .unwrap_or_default();

    Ok(ChartSpec {
        kind: crate::ir::ChartKind::WordCloud {
            entries,
            min_rotation,
            max_rotation,
            rotation_steps,
            padding,
        },
        series: vec![],
        categories: vec![],
        title,
        width: raw.width.unwrap_or(500.0),
        height: raw.height.unwrap_or(300.0),
        theme,
        strict: false,
    })
}
```

注意: `parse_color`, `parse_theme` は既存の内部ヘルパーを使うこと。関数名が異なる場合はファイル内を検索して確認する。

**Step 4: テスト実行**

```bash
cd crates/fulgur-chart && cargo test wordcloud_schema_roundtrip -- --nocapture 2>&1
```

Expected: `ok`

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs
git commit -m "feat(frontend): add wordCloud parser"
```

---

### Task 7: `layout/wordcloud.rs` を実装する（アルキメデス螺旋配置）

**Files:**
- Create: `crates/fulgur-chart/src/layout/wordcloud.rs`

**Step 1: ファイルを作成**

```rust
//! WordCloud チャートのレイアウト。
//! アルキメデス螺旋 + AABB 衝突検出で単語を非重複配置する。

use super::common::{render_title, OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT};
use crate::ir::{ChartKind, ChartSpec, Color, WordEntry};
use crate::palette;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// 螺旋パラメータ: r = SPIRAL_A * θ (px/rad)
const SPIRAL_A: f64 = 3.0;
/// θ のステップ量 (rad)
const DELTA_THETA: f64 = 0.08;
/// フォントサイズに対する行高さ比率
const LINE_HEIGHT: f64 = 1.2;

/// 軸揃え境界ボックス (中心座標 + 半幅/半高)
#[derive(Clone, Copy)]
struct Aabb {
    cx: f64,
    cy: f64,
    half_w: f64,
    half_h: f64,
}

impl Aabb {
    fn intersects(&self, other: &Aabb) -> bool {
        (self.cx - other.cx).abs() < self.half_w + other.half_w
            && (self.cy - other.cy).abs() < self.half_h + other.half_h
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let ChartKind::WordCloud {
        entries,
        min_rotation,
        max_rotation,
        rotation_steps,
        padding,
    } = &spec.kind
    else {
        unreachable!()
    };

    let title_band = if spec.title.is_some() { TITLE_BAND } else { 0.0 };
    let plot_top = OUTER_PAD + title_band;
    let plot_h = spec.height - plot_top - OUTER_PAD;
    let center_x = spec.width / 2.0;
    let center_y = plot_top + plot_h / 2.0;
    let max_r = ((spec.width / 2.0).hypot(plot_h / 2.0)) * 1.1;

    // size 降順・同値は text 昇順でソート (決定的)
    let mut sorted: Vec<(usize, &WordEntry)> = entries.iter().enumerate().collect();
    sorted.sort_by(|(_, a), (_, b)| {
        b.size.partial_cmp(&a.size).unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.text.cmp(&b.text))
    });

    let mut placed: Vec<Aabb> = Vec::with_capacity(sorted.len());
    let mut items: Vec<Prim> = Vec::new();
    let palette = &spec.theme.palette;

    for (orig_idx, entry) in &sorted {
        let text_w = m.width(&entry.text, entry.size as f32) as f64;
        let text_h = entry.size * LINE_HEIGHT;

        // 回転角度の決定
        let rotate_deg = if *rotation_steps <= 1 {
            *max_rotation
        } else {
            let step_idx = orig_idx % (*rotation_steps as usize);
            let t = step_idx as f64 / (*rotation_steps as f64 - 1.0);
            min_rotation + t * (max_rotation - min_rotation)
        };
        // 0° か −90° の 2 択のみサポート (AABB が axis-aligned になる)
        let is_vertical = rotate_deg.abs() == 90.0;

        // AABB (padding 込み)
        let (hw, hh) = if is_vertical {
            (text_h / 2.0 + padding, text_w / 2.0 + padding)
        } else {
            (text_w / 2.0 + padding, text_h / 2.0 + padding)
        };

        // 螺旋探索
        let mut theta: f64 = 0.0;
        let mut placed_pos: Option<(f64, f64)> = None;

        loop {
            let r = SPIRAL_A * theta;
            let cx = center_x + r * theta.cos();
            let cy = center_y + r * theta.sin();

            let candidate = Aabb { cx, cy, half_w: hw, half_h: hh };
            // キャンバス境界チェック
            let in_bounds = cx - hw >= 0.0
                && cx + hw <= spec.width
                && cy - hh >= plot_top
                && cy + hh <= spec.height - OUTER_PAD;

            if in_bounds && !placed.iter().any(|p| p.intersects(&candidate)) {
                placed.push(candidate);
                placed_pos = Some((cx, cy));
                break;
            }

            theta += DELTA_THETA;
            if r > max_r {
                break; // 収まらない単語はスキップ
            }
        }

        let Some((cx, cy)) = placed_pos else { continue };

        // 色の決定
        let color = entry.color.unwrap_or_else(|| {
            palette[orig_idx % palette.len()]
        });

        // SVG テキスト Prim
        // y は baseline 位置: cy + size * TEXT_BASELINE_RATIO
        let y_baseline = cy + entry.size * TEXT_BASELINE_RATIO;
        let rotate = if rotate_deg.abs() < 1e-9 {
            None
        } else {
            // rotate_deg での回転は text anchor (cx, y_baseline) を中心にする
            Some(rotate_deg)
        };

        // 縦文字の場合、x/y は回転中心に設定
        let (tx, ty) = if is_vertical {
            (cx, cy)
        } else {
            (cx, y_baseline)
        };

        items.push(Prim::Text {
            x: tx,
            y: ty,
            size: entry.size,
            anchor: Anchor::Middle,
            fill: color,
            content: entry.text.clone(),
            rotate_deg: rotate,
        });
    }

    // タイトル
    if let Some(title) = &spec.title {
        let title_prims = render_title(title, spec.width, OUTER_PAD, TITLE_FONT);
        items.splice(0..0, title_prims);
    }

    // 背景
    if let Some(bg) = spec.theme.background {
        items.insert(
            0,
            Prim::Rect {
                x: 0.0,
                y: 0.0,
                w: spec.width,
                h: spec.height,
                fill: bg,
            },
        );
    }

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}
```

**Step 2: ビルドを確認**

```bash
cd crates/fulgur-chart && cargo build 2>&1 | grep "^error"
```

Expected: エラーなし。もし `render_title` のシグネチャが異なれば `common.rs` を確認して合わせる。

**Step 3: コミット**

```bash
git add crates/fulgur-chart/src/layout/wordcloud.rs
git commit -m "feat(layout): implement wordcloud archimedean spiral placement"
```

---

### Task 8: `layout/mod.rs` にディスパッチを追加

**Files:**
- Modify: `crates/fulgur-chart/src/layout/mod.rs`

**Step 1: `pub mod wordcloud;` を追加**

`pub mod treemap;` の後に追加:

```rust
pub mod wordcloud;
```

**Step 2: `build` 関数の match に `WordCloud` アームを追加**

`ChartKind::Treemap => treemap::build(spec, m),` の後に追加:

```rust
ChartKind::WordCloud { .. } => wordcloud::build(spec, m),
```

**Step 3: ビルドとテストを確認**

```bash
cd crates/fulgur-chart && cargo test --lib -q 2>&1 | tail -5
```

Expected: 全テスト pass（既存 196 + guard の新 3 = 199 以上）

**Step 4: コミット**

```bash
git add crates/fulgur-chart/src/layout/mod.rs
git commit -m "feat(layout): dispatch ChartKind::WordCloud to wordcloud::build"
```

---

### Task 9: example spec と render テストを追加

**Files:**
- Create: `examples/specs/wordcloud.json`
- Create: `crates/fulgur-chart/tests/render_wordcloud.rs`

**Step 1: example spec を作成**

```json
{
  "type": "wordCloud",
  "width": 500,
  "height": 300,
  "data": {
    "labels": ["Rust", "SVG", "Chart", "Fast", "Safe", "Concurrent", "Memory", "Zero-Cost", "Abstraction", "Performance"],
    "datasets": [{
      "data": [90, 70, 60, 55, 50, 45, 40, 35, 30, 25],
      "color": ["#e63946", "#457b9d", "#2a9d8f", "#e9c46a", "#264653", "#f4a261", "#e76f51", "#023e8a", "#80b918", "#7b2d8b"]
    }]
  },
  "options": {
    "elements": {"word": {"minRotation": -90, "maxRotation": 0, "rotationSteps": 2, "padding": 3}},
    "plugins": {"title": {"display": true, "text": "Rust WordCloud"}}
  }
}
```

**Step 2: render テストを作成**

```rust
//! WordCloud チャートのエンドツーエンド描画テスト。

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).expect("parse error");
    render_chart(&spec)
}

const BASIC: &str = r#"{
    "type": "wordCloud",
    "width": 500,
    "height": 300,
    "data": {
        "labels": ["Rust", "SVG", "Chart", "Fast", "Safe"],
        "datasets": [{"data": [80, 60, 50, 40, 30]}]
    }
}"#;

#[test]
fn wordcloud_renders_to_svg() {
    let svg = render(BASIC);
    assert!(svg.starts_with("<svg"), "should produce valid SVG");
    assert!(svg.contains("<text"), "SVG should contain text elements");
    assert!(!svg.contains("NaN"), "SVG must not contain NaN");
}

#[test]
fn wordcloud_is_byte_deterministic() {
    assert_eq!(render(BASIC), render(BASIC));
}

#[test]
fn wordcloud_with_rotation() {
    let json = r#"{
        "type": "wordCloud",
        "width": 400,
        "height": 300,
        "data": {
            "labels": ["Alpha", "Beta"],
            "datasets": [{"data": [60, 40]}]
        },
        "options": {"elements": {"word": {"minRotation": -90, "maxRotation": 0, "rotationSteps": 2}}}
    }"#;
    let svg = render(json);
    // "Alpha" (index 0) → horizontal, "Beta" (index 1) → vertical (-90°)
    assert!(svg.contains("rotate"), "vertical word should have rotate transform");
    assert!(!svg.contains("NaN"));
}

#[test]
fn wordcloud_with_colors() {
    let json = r#"{
        "type": "wordCloud",
        "width": 500,
        "height": 300,
        "data": {
            "labels": ["Red", "Blue"],
            "datasets": [{"data": [60, 40], "color": ["#ff0000", "#0000ff"]}]
        }
    }"#;
    let svg = render(json);
    assert!(svg.contains("#ff0000"), "red color should appear in SVG");
    assert!(svg.contains("#0000ff"), "blue color should appear in SVG");
}

#[test]
fn wordcloud_snapshot() {
    let svg = render(BASIC);
    insta::assert_snapshot!(svg);
}
```

**Step 3: テスト実行**

```bash
cd crates/fulgur-chart && cargo test render_wordcloud -- --nocapture 2>&1
```

Expected: 全テスト pass（snapshot は初回自動生成）

**Step 4: コミット**

```bash
git add examples/specs/wordcloud.json crates/fulgur-chart/tests/render_wordcloud.rs \
    crates/fulgur-chart/tests/snapshots/
git commit -m "test(wordcloud): add render tests and example spec"
```

---

### Task 10: 全テスト実行・最終確認

**Step 1: 全テスト実行**

```bash
cd crates/fulgur-chart && cargo test -q 2>&1 | tail -10
```

Expected: 全テスト pass、失敗 0

**Step 2: clippy**

```bash
cd crates/fulgur-chart && cargo clippy -- -D warnings 2>&1 | grep "^error"
```

Expected: エラーなし

**Step 3: README の type 列挙を更新（任意）**

`README.md` の対応チャート種別一覧に `wordCloud` を追加する（存在する場合）。

**Step 4: 最終コミット（README 更新がある場合のみ）**

```bash
git add README.md
git commit -m "docs: add wordCloud to supported chart types"
```
