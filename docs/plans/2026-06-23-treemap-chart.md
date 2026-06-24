# Treemap Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement `type: "treemap"` chart rendering — QuickChart / chartjs-chart-treemap 互換の階層データを squarified アルゴリズムでネストした矩形に分割し、深さに応じた色で塗る。

**Architecture:** Add recursive `TreeNode` to IR + `Series.tree` + `ChartKind::Treemap` → 専用パス `parse_treemap` で `tree`/`key`/`groups` を**挿入順**でグルーピング・合算して `TreeNode` forest を構築 (matrix/gauge と同じ専用パスパターン) → `layout/treemap.rs` で squarify + 深さ色 + キャプション + ラベルを描画。レイアウトは完成 forest を受け取るだけ (boxplot の house pattern)。

**Tech Stack:** Rust, `Prim::{Rect, Text}` scene primitives, `layout/common.rs` の定数 (OUTER_PAD/TITLE_BAND/TITLE_FONT)、`num::fmt_num`、`TextMeasurer`。

**Design source:** beads issue `fulgur-chart-0zp` の design フィールド。

**確定済みの設計判断 (brainstorming):**
- groups 階層は**任意の深さ**に再帰対応
- 色は**トップ色相 + 深さで明度** (トップノードに `palette[i]`、子孫は親色相を継承し depth ごとに白へ寄せる)
- グループ矩形に**キャプション帯**を描画
- 収まらない/極小ラベルは**閾値以下非表示 + 切り詰め (…)**
- YAGNI: spacing/borderRadius/rtl/unsorted/sumKeys/hover/dividers/spanning/caption formatter 等の本家オプション面は実装しない

**Determinism (本プロジェクトのコア不変条件):**
- グルーピングは挿入順保持 (Vec + HashMap ルックアップ。`HashMap` の反復順に依存しない)
- squarify のソートは value 降順 + **元 index で安定 tie-break**

---

### Task 1: IR に TreeNode / Series.tree / ChartKind::Treemap を追加 (コンパイル green を維持)

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/model.rs` (`chart_type_name` の網羅 match)
- Create: `crates/fulgur-chart/src/layout/treemap.rs` (スタブ)
- Modify: `crates/fulgur-chart/src/layout/mod.rs` (module 登録 + dispatch)
- Modify (compiler-driven): 全 `Series { .. }` リテラルに `tree: vec![]` 追加

**Step 1: ir.rs に失敗するテストを書く**

`ir.rs` の `#[cfg(test)] mod tests` 末尾 (`outlabel_config_default_values` の後) に追加:

```rust
#[test]
fn tree_node_is_recursive() {
    let leaf = TreeNode { label: "a".into(), value: 3.0, children: vec![] };
    let group = TreeNode {
        label: "g".into(),
        value: 3.0,
        children: vec![leaf.clone()],
    };
    assert_eq!(group.children.len(), 1);
    assert_eq!(group.children[0].value, 3.0);
    assert!(leaf.children.is_empty());
}
```

**Step 2: 実行して失敗を確認**

```bash
cd /home/ubuntu/fulgur-chart/.worktrees/feat/treemap-chart
cargo test -p fulgur-chart tree_node_is_recursive 2>&1 | tail -5
```
Expected: `error[E0422]: cannot find struct ... TreeNode`

**Step 3: TreeNode 構造体を追加**

`ir.rs` の `BoxPoint` struct の後 (line ~29 の後) に追加:

```rust
/// treemap の階層ノード。リーフは children 空・value はリーフ値。
/// グループは value=子の合算・children=サブノード。任意の深さにネストできる。
#[derive(Clone, Debug, PartialEq)]
pub struct TreeNode {
    pub label: String,
    pub value: f64,
    pub children: Vec<TreeNode>,
}
```

**Step 4: Series に tree フィールドを追加**

`Series` struct の `box_points` フィールドの後に追加:

```rust
/// treemap の階層データ (トップレベルノードの forest)。treemap 種別のみ使用、他は空。
pub tree: Vec<TreeNode>,
```

**Step 5: ChartKind::Treemap を追加**

`ChartKind` enum の `OutlabeledPie { .. }` の後に追加:

```rust
/// QuickChart / chartjs-chart-treemap 互換の treemap。階層データを squarified で
/// ネストした矩形に分割し、深さに応じた色で塗る。データは series[0].tree に持つ。
Treemap,
```

**Step 6: 全 Series リテラルに `tree: vec![]` を追加**

コンパイラが各箇所を指摘する。`box_points: vec![]` の直後に `tree: vec![],` を加える。対象 (13 箇所):
- `crates/fulgur-chart/src/ir.rs` — テストヘルパ 3 箇所 (`fill_at_broadcasts_single_color` / `fill_at_indexes_per_point_colors` / `stroke_at_empty_is_black`)
- `crates/fulgur-chart/src/layout/scatter.rs:418`
- `crates/fulgur-chart/src/layout/common.rs:540`
- `crates/fulgur-chart/src/frontend/chartjs.rs:521`, `:1226`, `:1466`
- `crates/fulgur-chart/src/guard.rs:356`, `:390`, `:418`, `:445`
- `crates/fulgur-chart/src/frontend/vegalite.rs:367`, `:415`, `:461`

**Step 7: model.rs の網羅 match にアームを追加**

`chart_type_name` (line ~242 の match) の `ChartKind::OutlabeledPie { .. } => ...` の後に追加:

```rust
ChartKind::Treemap => "treemap",
```

> **Note:** `compute_geometry` (line ~115) と `compute_axes` (line ~344) はどちらも `_ => None` の catch-all を持つため変更不要。treemap は軸/直交ジオメトリを持たないので None で正しい。

**Step 8: layout/treemap.rs スタブを作成**

`crates/fulgur-chart/src/layout/treemap.rs` を新規作成:

```rust
//! Treemap チャートのレイアウト (squarified)。Task 3 で本実装に置き換える。

use crate::ir::ChartSpec;
use crate::scene::Scene;
use crate::text::TextMeasurer;

pub fn build(_spec: &ChartSpec, _m: &TextMeasurer) -> Scene {
    // 暫定スタブ。Task 3 で squarify + 描画を実装する。
    Scene {
        width: _spec.width,
        height: _spec.height,
        items: vec![],
    }
}
```

**Step 9: layout/mod.rs に登録 + dispatch**

`pub mod` リスト (アルファベット順、`pub mod sparkline;` の後) に追加:

```rust
pub mod treemap;
```

`build_scene` の match (`ChartKind::OutlabeledPie { .. } => ...` の後) に追加:

```rust
ChartKind::Treemap => treemap::build(spec, m),
```

**Step 10: テストとフルスイートを実行**

```bash
cargo test -p fulgur-chart tree_node_is_recursive 2>&1 | tail -5
cargo test 2>&1 | tail -10
```
Expected: 新テスト pass、全テスト pass (回帰なし)。

**Step 11: コミット**

```bash
git add crates/fulgur-chart/src/ir.rs \
        crates/fulgur-chart/src/model.rs \
        crates/fulgur-chart/src/layout/treemap.rs \
        crates/fulgur-chart/src/layout/mod.rs \
        crates/fulgur-chart/src/layout/scatter.rs \
        crates/fulgur-chart/src/layout/common.rs \
        crates/fulgur-chart/src/guard.rs \
        crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/src/frontend/vegalite.rs
git commit -m "feat(ir): add TreeNode, Series.tree, ChartKind::Treemap"
```

---

### Task 2: chartjs frontend で treemap をパース

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`

treemap は `tree`/`key`/`groups` という独自 dataset 形状のため、matrix/gauge と同じ**専用パス** (`parse_treemap`) で処理し、汎用 `check_unknown_keys` を回避する。

**Step 1: 失敗するパーステストを書く**

`chartjs.rs` の `#[cfg(test)]` ブロックに追加:

```rust
#[test]
fn parse_treemap_numeric_tree() {
    let json = r#"{
        "type": "treemap",
        "data": { "datasets": [{ "tree": [6, 4, 3, 2, 1] }] }
    }"#;
    let spec = parse(json, false).expect("parse error");
    assert!(matches!(spec.kind, crate::ir::ChartKind::Treemap));
    assert_eq!(spec.series.len(), 1);
    let t = &spec.series[0].tree;
    assert_eq!(t.len(), 5);
    assert_eq!(t[0].value, 6.0);
    assert!(t[0].children.is_empty());
}

#[test]
fn parse_treemap_grouped_sums_and_preserves_order() {
    let json = r#"{
        "type": "treemap",
        "data": { "datasets": [{
            "key": "value",
            "groups": ["cat", "sub"],
            "tree": [
                {"cat": "B", "sub": "x", "value": 2},
                {"cat": "A", "sub": "p", "value": 5},
                {"cat": "A", "sub": "p", "value": 1},
                {"cat": "A", "sub": "q", "value": 4},
                {"cat": "B", "sub": "x", "value": 3}
            ]
        }] }
    }"#;
    let spec = parse(json, false).expect("parse error");
    let t = &spec.series[0].tree;
    // トップは出現順 B, A (挿入順保持)
    assert_eq!(t.len(), 2);
    assert_eq!(t[0].label, "B");
    assert_eq!(t[1].label, "A");
    // B = sub x の合算 (2+3=5)
    assert_eq!(t[0].value, 5.0);
    assert_eq!(t[0].children.len(), 1);
    assert_eq!(t[0].children[0].label, "x");
    assert_eq!(t[0].children[0].value, 5.0);
    assert!(t[0].children[0].children.is_empty());
    // A = p(5+1=6) + q(4) = 10、子は出現順 p, q
    assert_eq!(t[1].value, 10.0);
    assert_eq!(t[1].children.len(), 2);
    assert_eq!(t[1].children[0].label, "p");
    assert_eq!(t[1].children[0].value, 6.0);
    assert_eq!(t[1].children[1].label, "q");
    assert_eq!(t[1].children[1].value, 4.0);
}
```

**Step 2: 実行して失敗を確認**

```bash
cargo test -p fulgur-chart parse_treemap 2>&1 | tail -10
```
Expected: FAIL with "未対応の type: treemap"

**Step 3: parse() の先頭で treemap を専用パスへルーティング**

`parse()` 内の matrix ルーティングブロック (line ~244 `if chart_type.as_deref() == Some("matrix")`) の直後に追加:

```rust
if chart_type.as_deref() == Some("treemap") {
    if strict {
        check_unknown_keys_treemap(json)?;
    }
    return parse_treemap(json);
}
```

**Step 4: parse_treemap と補助関数を実装**

`parse_matrix` 関数の直前 (line ~1114 の前) に追加:

```rust
/// treemap 専用パース。`tree`(数値配列 or オブジェクト配列) + `key` + `groups` を
/// 挿入順でグルーピング・合算して TreeNode forest を構築する。
fn parse_treemap(json: &str) -> Result<ChartSpec, String> {
    use crate::ir::TreeNode;

    #[derive(Deserialize)]
    struct TreemapWrapper {
        data: TreemapRawData,
        #[serde(default)]
        options: RawOptions,
    }
    #[derive(Deserialize)]
    struct TreemapRawData {
        datasets: Vec<TreemapRawDataset>,
    }
    #[derive(Deserialize)]
    struct TreemapRawDataset {
        #[allow(dead_code)]
        #[serde(default)]
        label: String,
        tree: TreeField,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        groups: Vec<String>,
        #[serde(rename = "backgroundColor", default)]
        background_color: Option<ScalarOrArray<String>>,
    }
    /// `tree`: 数値配列(フラット) または オブジェクト配列(groups でグルーピング)。
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TreeField {
        Nums(Vec<f64>),
        Objs(Vec<serde_json::Map<String, serde_json::Value>>),
    }

    let raw: TreemapWrapper = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if raw.data.datasets.len() != 1 {
        return Err("treemap チャートには dataset が 1 つ必要です".to_string());
    }
    let ds = raw.data.datasets.into_iter().next().unwrap();

    let forest: Vec<TreeNode> = match ds.tree {
        TreeField::Nums(nums) => nums
            .into_iter()
            .map(|v| TreeNode {
                label: String::new(),
                value: v,
                children: vec![],
            })
            .collect(),
        TreeField::Objs(objs) => {
            let key = ds
                .key
                .as_deref()
                .ok_or("treemap: オブジェクト tree には key が必要です")?;
            if ds.groups.is_empty() {
                // groups 無し: 各オブジェクトを単一リーフ (value=obj[key], label 無し)。
                objs.iter()
                    .map(|o| TreeNode {
                        label: String::new(),
                        value: obj_num(o, key),
                        children: vec![],
                    })
                    .collect()
            } else {
                build_tree_forest(&objs, &ds.groups, key)
            }
        }
    };

    let theme = build_theme(raw.options.theme);
    let no_axis = AxisSpec {
        title: None,
        min: None,
        max: None,
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: false,
        offset: false,
        grid: false,
    };

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
        tree: forest,
    }];

    // backgroundColor は v1 では未使用 (色は palette ベース)。受理のみ。
    let _ = ds.background_color;

    Ok(ChartSpec {
        kind: ChartKind::Treemap,
        series,
        categories: vec![],
        x_axis: no_axis.clone(),
        y_axis: no_axis,
        legend: crate::ir::LegendPos::None,
        title: raw
            .options
            .plugins
            .title
            .filter(|t| t.display)
            .map(|t| t.text),
        width: 800.0,
        height: 450.0,
        data_labels: false,
        theme,
    })
}

/// オブジェクトから数値プロパティを読む (欠落/非数値は 0.0)。
fn obj_num(o: &serde_json::Map<String, serde_json::Value>, key: &str) -> f64 {
    o.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

/// オブジェクトからグルーピングキーを文字列として読む。
/// 文字列はそのまま、数値は整形、欠落は空文字。
fn obj_group_key(o: &serde_json::Map<String, serde_json::Value>, field: &str) -> String {
    match o.get(field) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        Some(serde_json::Value::Bool(b)) => b.to_string(),
        _ => String::new(),
    }
}

/// オブジェクト群を groups[0] でグルーピング(挿入順保持)し、groups[1..] で再帰。
/// 各レベルの value は子の合算 (最深レベルは key の合算)。
fn build_tree_forest(
    objs: &[serde_json::Map<String, serde_json::Value>],
    groups: &[String],
    key: &str,
) -> Vec<crate::ir::TreeNode> {
    use crate::ir::TreeNode;
    let field = &groups[0];
    // 挿入順を保ちつつ O(1) ルックアップ (matrix の x/y カテゴリ収集と同じパターン)。
    let mut order: Vec<String> = Vec::new();
    let mut idx: HashMap<String, usize> = HashMap::new();
    let mut buckets: Vec<Vec<serde_json::Map<String, serde_json::Value>>> = Vec::new();
    for o in objs {
        let gk = obj_group_key(o, field);
        let bi = *idx.entry(gk.clone()).or_insert_with(|| {
            order.push(gk);
            buckets.push(Vec::new());
            buckets.len() - 1
        });
        buckets[bi].push(o.clone());
    }

    order
        .into_iter()
        .zip(buckets)
        .map(|(label, bucket)| {
            if groups.len() == 1 {
                // 最深レベル = リーフ。value = key の合算。
                let value: f64 = bucket.iter().map(|o| obj_num(o, key)).sum();
                TreeNode {
                    label,
                    value,
                    children: vec![],
                }
            } else {
                let children = build_tree_forest(&bucket, &groups[1..], key);
                let value: f64 = children.iter().map(|c| c.value).sum();
                TreeNode {
                    label,
                    value,
                    children,
                }
            }
        })
        .collect()
}

/// treemap の許可キーを検証する (strict モード)。
fn check_unknown_keys_treemap(json: &str) -> Result<(), String> {
    let value: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let Some(top) = value.as_object() else {
        return Ok(());
    };
    check_object(top, &["type", "data", "options"], "")?;
    if let Some(data) = top.get("data").and_then(|v| v.as_object()) {
        check_object(data, &["labels", "datasets"], "data")?;
        if let Some(datasets) = data.get("datasets").and_then(|v| v.as_array()) {
            for (i, ds) in datasets.iter().enumerate() {
                if let Some(ds) = ds.as_object() {
                    check_object(
                        ds,
                        &[
                            "label",
                            "tree",
                            "key",
                            "groups",
                            "backgroundColor",
                            "borderColor",
                            "borderWidth",
                        ],
                        &format!("data.datasets[{i}]"),
                    )?;
                }
            }
        }
    }
    if let Some(options) = top.get("options").and_then(|v| v.as_object()) {
        check_object(options, &["plugins", "theme"], "options")?;
        if let Some(plugins) = options.get("plugins").and_then(|v| v.as_object()) {
            check_object(plugins, &["title", "legend"], "options.plugins")?;
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

> **Note:** `RawOptions` / `build_theme` / `ScalarOrArray` / `check_object` / `legend_pos` は既存。`HashMap` は chartjs.rs 冒頭で既に `use` 済み (matrix が使用)。未 import ならファイル先頭の `use std::collections::HashMap;` を確認する。

**Step 5: パーステストを実行**

```bash
cargo test -p fulgur-chart parse_treemap 2>&1 | tail -10
```
Expected: 両テスト pass。

**Step 6: フルスイートを実行**

```bash
cargo test 2>&1 | tail -10
```
Expected: 全テスト pass。

**Step 7: strict モードのキーチェックを確認するテストを追加**

`chartjs.rs` テストブロックに追加:

```rust
#[test]
fn treemap_strict_rejects_unknown_dataset_key() {
    let json = r#"{
        "type": "treemap",
        "data": { "datasets": [{ "tree": [1,2], "bogus": true }] }
    }"#;
    assert!(parse(json, true).is_err());
}

#[test]
fn treemap_strict_accepts_known_keys() {
    let json = r#"{
        "type": "treemap",
        "data": { "datasets": [{ "key": "v", "groups": ["g"],
            "tree": [{"g": "a", "v": 1}] }] }
    }"#;
    assert!(parse(json, true).is_ok());
}
```

```bash
cargo test -p fulgur-chart treemap_strict 2>&1 | tail -5
```
Expected: 両テスト pass。

**Step 8: コミット**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs
git commit -m "feat(frontend): parse treemap type with tree/key/groups hierarchy"
```

---

### Task 3: squarified レイアウトと描画を実装

**Files:**
- Modify: `crates/fulgur-chart/src/layout/treemap.rs` (スタブを本実装に置換)

**Step 1: 失敗する幾何テストを書く**

`treemap.rs` を以下で**全置換** (スタブ削除)。まずテストだけ通る形にせず、関数シグネチャを定義してから実装する。最初に `treemap.rs` の末尾テストを書き、`squarify` 等が未定義でコンパイルエラーになることを確認する。

実装は Step 3 でまとめて入れるため、ここでは Step 3 の最終ファイルを書き、テストを後続で回す。

**Step 2: 本実装で treemap.rs を全置換**

`crates/fulgur-chart/src/layout/treemap.rs` の内容を以下に置き換える:

```rust
//! Treemap チャートのレイアウト。階層データを squarified アルゴリズム
//! (Bruls/Huizing/van Wijk) でネストした矩形に分割し、深さに応じた色で塗る。

use super::common::{OUTER_PAD, TEXT_BASELINE_RATIO, TITLE_BAND, TITLE_FONT};
use crate::ir::{ChartSpec, Color, TreeNode};
use crate::num::fmt_num;
use crate::scene::{Anchor, Prim, Scene};
use crate::text::TextMeasurer;

/// 隣接矩形間の隙間 (px)。各セルをこの分だけ内側へ縮める。
const SPACING: f64 = 2.0;
/// depth ごとに白へ寄せる比率 (上限あり)。
const DEPTH_LIGHTEN: f64 = 0.18;
const DEPTH_LIGHTEN_MAX: f64 = 0.6;
/// キャプション帯やラベルのパディング (px)。
const PAD: f64 = 3.0;

const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 1.0 };

/// レイアウト用の矩形 (左上原点)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TreemapRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn lerp_color(lo: Color, hi: Color, t: f64) -> Color {
    let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, 1.0) };
    Color {
        r: (lo.r as f64 + (hi.r as f64 - lo.r as f64) * t).round() as u8,
        g: (lo.g as f64 + (hi.g as f64 - lo.g as f64) * t).round() as u8,
        b: (lo.b as f64 + (hi.b as f64 - lo.b as f64) * t).round() as u8,
        a: lo.a + (hi.a - lo.a) * t as f32,
    }
}

/// depth に応じて base 色を白へ寄せる。depth 0 は base そのもの。
fn lighten(base: Color, depth: usize) -> Color {
    let t = (depth as f64 * DEPTH_LIGHTEN).min(DEPTH_LIGHTEN_MAX);
    lerp_color(base, WHITE, t)
}

/// 背景色の輝度からコントラストの取れる文字色 (濃灰 or 白) を選ぶ。
fn text_on(bg: Color) -> Color {
    let lum = 0.299 * bg.r as f64 + 0.587 * bg.g as f64 + 0.114 * bg.b as f64;
    if lum > 140.0 {
        Color { r: 60, g: 60, b: 60, a: 1.0 }
    } else {
        WHITE
    }
}

/// `s` を `max_w` 以内に収める。収まらなければ末尾を削り "…" を付す。
/// "…" 単体でも収まらなければ None (描画しない)。
fn truncate_to_width(s: &str, max_w: f64, font: f64, m: &TextMeasurer) -> Option<String> {
    if max_w <= 0.0 || s.is_empty() {
        return None;
    }
    if m.width(s, font as f32) as f64 <= max_w {
        return Some(s.to_string());
    }
    let ell = "…";
    let chars: Vec<char> = s.chars().collect();
    let mut end = chars.len();
    while end > 0 {
        end -= 1;
        let mut cand: String = chars[..end].iter().collect();
        cand.push_str(ell);
        if m.width(&cand, font as f32) as f64 <= max_w {
            return Some(cand);
        }
    }
    None
}

/// `worst`: 与えた area 行を length 辺に沿って並べたときの最悪アスペクト比。
/// Bruls et al. の定義。
fn worst(row: &[f64], length: f64) -> f64 {
    if row.is_empty() || length <= 0.0 {
        return f64::INFINITY;
    }
    let s: f64 = row.iter().sum();
    if s <= 0.0 {
        return f64::INFINITY;
    }
    let rmax = row.iter().cloned().fold(f64::MIN, f64::max);
    let rmin = row.iter().cloned().fold(f64::MAX, f64::min);
    let l2 = length * length;
    let s2 = s * s;
    (l2 * rmax / s2).max(s2 / (l2 * rmin.max(f64::EPSILON)))
}

/// squarified treemap: `areas` (各ノードの値) を `rect` 内へ充填し、入力順に対応する
/// 矩形列を返す。面積は値に比例し、矩形は rect を重なりなくタイルする。
pub(crate) fn squarify(areas: &[f64], rect: TreemapRect) -> Vec<TreemapRect> {
    let n = areas.len();
    let zero = TreemapRect { x: rect.x, y: rect.y, w: 0.0, h: 0.0 };
    let total: f64 = areas.iter().map(|a| a.max(0.0)).sum();
    if n == 0 || total <= 0.0 || rect.w <= 0.0 || rect.h <= 0.0 {
        return vec![zero; n];
    }
    let scale = (rect.w * rect.h) / total;
    let scaled: Vec<f64> = areas.iter().map(|a| a.max(0.0) * scale).collect();

    let mut result = vec![zero; n];
    let mut free = rect;
    let mut i = 0;
    while i < n {
        let shorter = free.w.min(free.h);
        // worst を悪化させない範囲で行を伸ばす。
        let mut row_end = i + 1;
        let mut best = worst(&scaled[i..row_end], shorter);
        while row_end < n {
            let cand = worst(&scaled[i..row_end + 1], shorter);
            if cand <= best {
                best = cand;
                row_end += 1;
            } else {
                break;
            }
        }
        let row = &scaled[i..row_end];
        let row_sum: f64 = row.iter().sum();
        if free.w >= free.h {
            // 左側に縦ストリップを敷く。幅 = row_sum / free.h。
            let strip_w = if free.h > 0.0 { row_sum / free.h } else { 0.0 };
            let mut y = free.y;
            for (j, &a) in row.iter().enumerate() {
                let cell_h = if strip_w > 0.0 { a / strip_w } else { 0.0 };
                result[i + j] = TreemapRect { x: free.x, y, w: strip_w, h: cell_h };
                y += cell_h;
            }
            free.x += strip_w;
            free.w -= strip_w;
        } else {
            // 上側に横ストリップを敷く。高さ = row_sum / free.w。
            let strip_h = if free.w > 0.0 { row_sum / free.w } else { 0.0 };
            let mut x = free.x;
            for (j, &a) in row.iter().enumerate() {
                let cell_w = if strip_h > 0.0 { a / strip_h } else { 0.0 };
                result[i + j] = TreemapRect { x, y: free.y, w: cell_w, h: strip_h };
                x += cell_w;
            }
            free.y += strip_h;
            free.h -= strip_h;
        }
        i = row_end;
    }
    result
}

fn inset(r: TreemapRect, by: f64) -> TreemapRect {
    TreemapRect {
        x: r.x + by / 2.0,
        y: r.y + by / 2.0,
        w: (r.w - by).max(0.0),
        h: (r.h - by).max(0.0),
    }
}

/// リーフ矩形の中央にラベル(+値)を描く。収まらなければ truncate、極小は省略。
fn draw_leaf_label(
    node: &TreeNode,
    r: TreemapRect,
    fill: Color,
    font: f64,
    m: &TextMeasurer,
    items: &mut Vec<Prim>,
) {
    let avail_w = r.w - 2.0 * PAD;
    let avail_h = r.h - 2.0 * PAD;
    if avail_w <= 0.0 || avail_h < font {
        return;
    }
    let color = text_on(fill);
    let cx = r.x + r.w / 2.0;
    let cy = r.y + r.h / 2.0;
    let value_str = fmt_num(node.value);
    let two_lines = !node.label.is_empty() && avail_h >= 2.0 * font + 2.0;
    if two_lines {
        if let Some(lbl) = truncate_to_width(&node.label, avail_w, font, m) {
            items.push(Prim::Text {
                x: cx,
                y: cy - font * 0.1,
                size: font,
                anchor: Anchor::Middle,
                fill: color,
                content: lbl,
            });
        }
        if let Some(v) = truncate_to_width(&value_str, avail_w, font, m) {
            items.push(Prim::Text {
                x: cx,
                y: cy + font * 0.95,
                size: font,
                anchor: Anchor::Middle,
                fill: color,
                content: v,
            });
        }
    } else {
        let single = if node.label.is_empty() {
            value_str
        } else {
            node.label.clone()
        };
        if let Some(t) = truncate_to_width(&single, avail_w, font, m) {
            items.push(Prim::Text {
                x: cx,
                y: cy + font * TEXT_BASELINE_RATIO,
                size: font,
                anchor: Anchor::Middle,
                fill: color,
                content: t,
            });
        }
    }
}

/// グループ矩形の上部にキャプション(グループ名)を描く。
fn draw_caption(
    label: &str,
    r: TreemapRect,
    fill: Color,
    font: f64,
    m: &TextMeasurer,
    items: &mut Vec<Prim>,
) {
    let avail_w = r.w - 2.0 * PAD;
    if let Some(t) = truncate_to_width(label, avail_w, font, m) {
        items.push(Prim::Text {
            x: r.x + PAD,
            y: r.y + font + 1.0,
            size: font,
            anchor: Anchor::Start,
            fill: text_on(fill),
            content: t,
        });
    }
}

/// ノード列を rect 内に squarify して再帰描画する。
/// base=None ならトップレベル (各ノードに palette[i])、Some なら親色を継承。
#[allow(clippy::too_many_arguments)]
fn draw_nodes(
    nodes: &[TreeNode],
    rect: TreemapRect,
    depth: usize,
    base: Option<Color>,
    palette: &[Color],
    font: f64,
    m: &TextMeasurer,
    items: &mut Vec<Prim>,
) {
    if nodes.is_empty() || palette.is_empty() {
        return;
    }
    // value 降順、同値は元 index で安定 tie-break (determinism)。
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by(|&a, &b| {
        nodes[b]
            .value
            .partial_cmp(&nodes[a].value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let areas: Vec<f64> = order.iter().map(|&i| nodes[i].value.max(0.0)).collect();
    let rects = squarify(&areas, rect);

    for (k, &i) in order.iter().enumerate() {
        let node = &nodes[i];
        let node_base = base.unwrap_or_else(|| palette[i % palette.len()]);
        let fill = lighten(node_base, depth);
        let cell = inset(rects[k], SPACING);
        if cell.w <= 0.0 || cell.h <= 0.0 {
            continue;
        }
        items.push(Prim::Rect {
            x: cell.x,
            y: cell.y,
            w: cell.w,
            h: cell.h,
            fill,
        });
        if node.children.is_empty() {
            draw_leaf_label(node, cell, fill, font, m, items);
        } else {
            draw_caption(&node.label, cell, fill, font, m, items);
            let cap_h = font + 6.0;
            let child_rect = TreemapRect {
                x: cell.x,
                y: cell.y + cap_h,
                w: cell.w,
                h: (cell.h - cap_h).max(0.0),
            };
            if child_rect.h > 0.0 {
                draw_nodes(
                    &node.children,
                    child_rect,
                    depth + 1,
                    Some(node_base),
                    palette,
                    font,
                    m,
                    items,
                );
            }
        }
    }
}

pub fn build(spec: &ChartSpec, m: &TextMeasurer) -> Scene {
    let font = spec.theme.font_size;
    let ink = spec.theme.text_color;
    let title_band = if spec.title.is_some() { TITLE_BAND } else { 0.0 };

    let plot = TreemapRect {
        x: OUTER_PAD,
        y: OUTER_PAD + title_band,
        w: (spec.width - 2.0 * OUTER_PAD).max(0.0),
        h: (spec.height - 2.0 * OUTER_PAD - title_band).max(0.0),
    };

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

    let forest: &[TreeNode] = spec.series.first().map(|s| s.tree.as_slice()).unwrap_or(&[]);
    draw_nodes(forest, plot, 0, None, &spec.theme.palette, font, m, &mut items);

    Scene {
        width: spec.width,
        height: spec.height,
        items,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;

    fn rects_overlap(a: &TreemapRect, b: &TreemapRect) -> bool {
        let eps = 1e-6;
        a.x + eps < b.x + b.w
            && b.x + eps < a.x + a.w
            && a.y + eps < b.y + b.h
            && b.y + eps < a.y + a.h
    }

    #[test]
    fn squarify_areas_proportional_to_values() {
        let rect = TreemapRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        let values = [6.0, 4.0, 3.0, 2.0, 1.0];
        let total: f64 = values.iter().sum();
        let rects = squarify(&values, rect);
        let container = rect.w * rect.h;
        for (k, &v) in values.iter().enumerate() {
            let area = rects[k].w * rects[k].h;
            let expected = v / total * container;
            assert!(
                (area - expected).abs() < 1e-3,
                "leaf {k}: area {area} != expected {expected}"
            );
        }
    }

    #[test]
    fn squarify_tiles_without_overlap_and_fills_container() {
        let rect = TreemapRect { x: 5.0, y: 7.0, w: 200.0, h: 120.0 };
        let values = [10.0, 7.0, 5.0, 3.0, 2.0, 1.0, 1.0];
        let rects = squarify(&values, rect);
        // 充填: 面積合計 ≈ コンテナ面積
        let sum: f64 = rects.iter().map(|r| r.w * r.h).sum();
        assert!((sum - rect.w * rect.h).abs() < 1e-3, "areas must fill container");
        // 重なりなし
        for a in 0..rects.len() {
            for b in (a + 1)..rects.len() {
                assert!(
                    !rects_overlap(&rects[a], &rects[b]),
                    "rects {a} and {b} overlap"
                );
            }
        }
        // 全矩形が rect 内に収まる
        for r in &rects {
            assert!(r.x >= rect.x - 1e-6 && r.y >= rect.y - 1e-6);
            assert!(r.x + r.w <= rect.x + rect.w + 1e-6);
            assert!(r.y + r.h <= rect.y + rect.h + 1e-6);
        }
    }

    fn treemap_spec(json: &str) -> ChartSpec {
        chartjs::parse(json, false).expect("parse error")
    }

    #[test]
    fn nested_treemap_has_rects_and_text() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{
                "key": "v", "groups": ["a", "b"],
                "tree": [
                    {"a":"X","b":"p","v":8},
                    {"a":"X","b":"q","v":4},
                    {"a":"Y","b":"r","v":6}
                ]
            }] }
        }"#;
        let spec = treemap_spec(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        let rects = scene.items.iter().filter(|p| matches!(p, Prim::Rect { .. })).count();
        let texts = scene.items.iter().filter(|p| matches!(p, Prim::Text { .. })).count();
        // グループ矩形 (X,Y) + リーフ矩形 (p,q,r) で 5 以上
        assert!(rects >= 5, "expected nested rects, got {rects}");
        // キャプション + ラベルのテキストが存在
        assert!(texts > 0, "expected labels/captions");
        // NaN を含まない
        assert!(!format!("{:?}", scene.items).contains("NaN"));
    }

    #[test]
    fn build_is_deterministic() {
        let json = r#"{
            "type": "treemap",
            "data": { "datasets": [{ "tree": [5, 5, 3, 3, 2] }] }
        }"#;
        let spec = treemap_spec(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let a = build(&spec, &m);
        let b = build(&spec, &m);
        assert_eq!(a, b, "same spec must produce identical scene");
    }

    #[test]
    fn scene_dims_match_spec() {
        let json = r#"{"type":"treemap","data":{"datasets":[{"tree":[1,2,3]}]}}"#;
        let spec = treemap_spec(json);
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        let scene = build(&spec, &m);
        assert_eq!(scene.width, spec.width);
        assert_eq!(scene.height, spec.height);
    }
}
```

**Step 3: レイアウトテストを実行**

```bash
cargo test -p fulgur-chart --lib treemap 2>&1 | tail -20
```
Expected: 全 treemap テスト pass。

**Step 4: フルスイートを実行**

```bash
cargo test 2>&1 | tail -10
```
Expected: 全テスト pass。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/layout/treemap.rs
git commit -m "feat(layout): implement squarified treemap with depth color, captions, labels"
```

---

### Task 4: エンドツーエンドのレンダリングテスト + スナップショット

**Files:**
- Create: `crates/fulgur-chart/tests/render_treemap.rs`

repo の慣習 (`tests/render_<kind>.rs` + insta スナップショット) に従う。`render_matrix.rs` を参考にする。

**Step 1: テストファイルを作成**

`crates/fulgur-chart/tests/render_treemap.rs`:

```rust
//! treemap チャートのエンドツーエンド描画テスト。

use fulgur_chart::frontend::chartjs;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    let spec = chartjs::parse(json, false).expect("parse error");
    render_chart(&spec)
}

const NESTED: &str = r#"{
    "type": "treemap",
    "options": { "plugins": { "title": { "display": true, "text": "Sales" } } },
    "data": { "datasets": [{
        "key": "value",
        "groups": ["region", "product"],
        "tree": [
            {"region": "EMEA", "product": "A", "value": 12},
            {"region": "EMEA", "product": "B", "value": 7},
            {"region": "APAC", "product": "A", "value": 9},
            {"region": "APAC", "product": "C", "value": 5},
            {"region": "AMER", "product": "A", "value": 14}
        ]
    }] }
}"#;

#[test]
fn treemap_renders_to_svg() {
    let svg = render(NESTED);
    assert!(svg.starts_with("<svg"), "should produce valid SVG");
    assert!(svg.contains("<rect"), "SVG should contain rect elements");
    assert!(svg.contains("<text"), "SVG should contain text labels/captions");
    assert!(!svg.contains("NaN"), "SVG must not contain NaN");
}

#[test]
fn treemap_numeric_tree_renders() {
    let svg = render(r#"{"type":"treemap","data":{"datasets":[{"tree":[6,4,3,2,1]}]}}"#);
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<rect"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn treemap_is_byte_deterministic() {
    assert_eq!(render(NESTED), render(NESTED));
}

#[test]
fn treemap_snapshot() {
    let svg = render(NESTED);
    insta::assert_snapshot!(svg);
}
```

> **Note:** `fulgur_chart::frontend::chartjs` / `fulgur_chart::render::render_chart` の公開パスは `tests/render_matrix.rs` で確認する。非公開なら同ファイルの `render` ヘルパの呼び出し方に合わせる。

**Step 2: テストを実行してスナップショットを生成**

```bash
cargo test --test render_treemap 2>&1 | tail -20
```
Expected: 構造テスト pass、スナップショットは初回 `treemap_snapshot` が pending。

**Step 3: スナップショットをレビューして承認**

```bash
cargo insta review
# または内容を確認のうえ:
cargo insta accept
```

生成された `.snap` の SVG を目視し、矩形がネストし重ならず、キャプション/ラベルが妥当か確認する。

**Step 4: フルスイートを実行**

```bash
cargo test 2>&1 | tail -10
```
Expected: 全テスト pass。

**Step 5: コミット**

```bash
git add crates/fulgur-chart/tests/render_treemap.rs \
        crates/fulgur-chart/tests/snapshots/render_treemap__treemap_snapshot.snap
git commit -m "test(render): add treemap end-to-end render and snapshot tests"
```

---

## Notes

- `Series.values` は treemap では空。全データは `Series.tree` に持つ (boxplot の `box_points` と同じ方針)。
- `model.rs` は treemap 用の意味モデル (要素ジオメトリ・軸) を構築しない。`compute_geometry`/`compute_axes` は `_ => None` のため treemap は軸/要素なしのモデルになる。スナップショットテスト用途では許容 (boxplot と同じ判断)。
- `backgroundColor` dataset プロパティは strict で受理するが v1 では未使用 (色は palette ベース)。
- vegalite フロントエンドは treemap を生成しないため変更不要。
- 受け入れ基準の「面積が値に比例・重なりなく充填」は `squarify` の専用ユニットテスト (Task 3 Step 2) で検証する。スナップショットは「描画されること」のみを保証する。
