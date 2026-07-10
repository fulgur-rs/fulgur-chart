# Sankey Extras Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** chartjs-chart-sankey で v1 に見送った 3 オプションを追加する: (A) `hoverColorFrom`/`hoverColorTo` を受理して no-op、(B) SankeyFlow の per-link `color`/`colorFrom`/`colorTo` 上書き、(C) `dataset.parsing` による `from`/`to`/`flow` キー再マップ。

**Architecture:** Phase を A→B→C の順に独立コミットで進める。
- Phase A は schema と strict 許可キーリストを拡張するだけで IR/レイアウトは触らない。
- Phase B は `SankeyLink` IR を拡張し、`layout::sankey::build` の per-link 描画箇所に override を注入する。既存 `Prim::GradientPath` が per-instance stop を持つため SVG def パスの拡張は不要。
- Phase C は `parse_sankey` を 2 段パースに書き換え、data 要素を先に `serde_json::Value` として受け、`parsing` で effective key を決定してから正規化 Value を組んで `SankeyFlow` 相当の struct に deserialize する。

**Tech Stack:** Rust / serde / schemars / cargo test。既存の sankey パーサ(`crates/fulgur-chart/src/frontend/chartjs.rs::parse_sankey`)、schema(`crates/fulgur-chart/src/schema/chartjs.rs`)、IR(`crates/fulgur-chart/src/ir.rs`)、レイアウト(`crates/fulgur-chart/src/layout/sankey.rs`)を修正する。

**Reference:** design は beads issue `fulgur-chart-40h` の design フィールド。設計上の判断根拠(採用しなかった案を含む)はそちらを参照。

**Determinism 要件:** 既存の sankey スナップショット / golden PNG は新機能未使用時に byte 不変。全 sort は安定 (`sort_by`)。新機能使用時も 2 回 render で byte 一致すること。

---

## Phase A: hoverColorFrom / hoverColorTo (accept + no-op)

Silent ignore ではなく **valid かどうかは検証する**。schema と strict 許可キーの両方に載せ、パーサでは `parse_color` を通して IR には格納しない。

### Task A.1: 受理テストと不正エラーテストを追加

**Files:**
- Test: `crates/fulgur-chart/tests/render_sankey.rs`

**Step 1: 失敗するテストを追加**

`tests/render_sankey.rs` の末尾に追加:

```rust
#[test]
fn sankey_accepts_hover_color_and_renders_identically() {
    // hoverColorFrom / hoverColorTo は静的レンダラでは描画されないため、
    // 指定した spec と指定しない spec の SVG が byte-identical になる。
    let with_hover = r#"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#36a2eb","colorTo":"#ff6384",
        "hoverColorFrom":"#000000","hoverColorTo":"#ffffff",
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"#;
    let without_hover = r#"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#36a2eb","colorTo":"#ff6384",
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"#;
    assert_eq!(render(with_hover), render(without_hover));
}

#[test]
fn sankey_rejects_invalid_hover_color() {
    let bad = r#"{"type":"sankey","data":{"datasets":[{
        "hoverColorFrom":"not-a-color",
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"#;
    let err = chartjs::parse(bad, false).unwrap_err();
    assert!(
        err.contains("hoverColorFrom"),
        "error must mention field: {err}"
    );
}
```

**Step 2: テストを実行して失敗を確認**

Run: `cargo test -p fulgur-chart --test render_sankey sankey_accepts_hover_color sankey_rejects_invalid_hover_color 2>&1 | tail -20`
Expected: 両テストが FAIL(strict 側で `hoverColorFrom` が未知キー扱い、または compile fail していない状態)。

**Step 3: コミットしない**(実装後の Task A.4 でまとめてコミット)

---

### Task A.2: schema に hover_color_from / hover_color_to を追加

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`(`SankeyDataset` 直下)

**Step 1: フィールド追加**

`SankeyDataset` 内の `color_mode` の直後(既存 `alpha` の直前)に追加:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_color_from: Option<ColorString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_color_to: Option<ColorString>,
```

既存の `#[serde(rename_all = "camelCase")]` により JSON キーは自動的に `hoverColorFrom` / `hoverColorTo` になる(SankeyDataset 冒頭で確認)。

**Step 2: cargo build で schema が壊れていないか確認**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: エラーなくビルドが通る。

**Step 3: schema drift チェック**(schemas/ が再生成対象なら)

Run: `find . -name 'chartjs*.json' -not -path '*/target/*' -not -path '*/node_modules/*' 2>/dev/null | head`
- 出力があれば schema drift を確認する `just` / `xtask` があるか確認。Task A.4 の一環で再生成する。

---

### Task A.3: 実装(strict 許可 + parser 検証)

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`check_unknown_keys_sankey`, `parse_sankey`)

**Step 1: strict 許可キーに追加**

`check_unknown_keys_sankey`(行 1082-1103 付近)の dataset キー配列に追加:

```rust
                        &[
                            "label",
                            "data",
                            "colorFrom",
                            "colorTo",
                            "colorMode",
                            "hoverColorFrom",
                            "hoverColorTo",
                            "alpha",
                            "borderColor",
                            "borderWidth",
                            "color",
                            "nodeWidth",
                            "nodePadding",
                            "modeX",
                            "size",
                            "labels",
                            "priority",
                            "column",
                        ],
```

**Step 2: parse_sankey 内 struct DS に受け口を追加**

`parse_sankey` の内部 `struct DS`(行 1817 付近)に追加:

```rust
        #[serde(rename = "hoverColorFrom", default)]
        hover_color_from: Option<String>,
        #[serde(rename = "hoverColorTo", default)]
        hover_color_to: Option<String>,
```

**Step 3: parse_sankey 本体で色検証(IR には流さない no-op)**

`parse_sankey` の色パース箇所(既存 `let color_from = ...` の近く、行 1897 付近)の**後**に追加:

```rust
    // hoverColorFrom / hoverColorTo は静的レンダラでは描画されないため IR に流さないが、
    // 指定時は色値としてパース可能かは検証する(silent ignore を防ぐ)。
    if let Some(s) = ds.hover_color_from.as_deref() {
        if parse_color(s).is_none() {
            return Err(format!("sankey hoverColorFrom is not a valid color: {s}"));
        }
    }
    if let Some(s) = ds.hover_color_to.as_deref() {
        if parse_color(s).is_none() {
            return Err(format!("sankey hoverColorTo is not a valid color: {s}"));
        }
    }
```

**Step 4: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: 成功。

---

### Task A.4: テスト再実行と Phase A コミット

**Step 1: Task A.1 のテストが通ることを確認**

Run: `cargo test -p fulgur-chart --test render_sankey sankey_accepts_hover_color sankey_rejects_invalid_hover_color 2>&1 | tail -10`
Expected: `test result: ok. 2 passed`

**Step 2: 既存 sankey スイート全体で regression 無しを確認**

Run: `cargo test -p fulgur-chart --test render_sankey 2>&1 | tail -20`
Expected: `test result: ok. 13 passed`(既存 11 + 追加 2)。

**Step 3: schema 生成物があれば再生成**

Run: `just gen-schema 2>/dev/null || cargo run -p fulgur-chart-cli -- schema > /tmp/schema.json 2>/dev/null; echo "check for schema tooling"`
- schemas/*.json が git 追跡対象なら再生成コマンドを流し、差分をコミット対象に含める。
- 存在しない/ツールが無ければスキップ。

**Step 4: git add & commit(Phase A)**

Run:
```bash
git add crates/fulgur-chart/src/schema/chartjs.rs \
        crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/tests/render_sankey.rs
# schemas/ に差分あれば追加
git status --short
git commit -m "feat(sankey): accept hoverColorFrom/hoverColorTo as no-op

Static renderer does not draw hover states. Accept the fields in both
schema and strict allowlist, validate the color strings so silent
ignore is impossible, and drop them on the floor at parse time.

Refs: fulgur-chart-40h"
```

Expected: 1 コミット作成、CI blockers なし。

---

## Phase B: per-link 色 (color / colorFrom / colorTo)

SankeyFlow に per-link 上書き色を追加。既存 `Prim::GradientPath` が per-instance stops を持つのでレンダラ変更は不要。IR と layout に override 経路を通す。

### Task B.1: SankeyLink IR に per-link 色フィールドを追加

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs`(`SankeyLink`)

**Step 1: フィールド追加**

`ir.rs` 行 51-57 の `SankeyLink` を書き換え:

```rust
/// sankey のリンク(フロー)。ノード間のフロー量を表す。from/to はノードID(文字列)。
/// per-link 色上書き: chartjs-chart-sankey の data 要素 `color`/`colorFrom`/`colorTo` に対応。
/// None なら dataset レベル(`ChartKind::Sankey.color_from` / `color_to`)にフォールバック。
/// - `color_from`: from 側 stop 上書き
/// - `color_to`: to 側 stop 上書き
/// - `color` は parse 時に解決(color_from/color_to が個別未指定なら両方に流し込む)ため IR には持たない。
#[derive(Clone, Debug, PartialEq)]
pub struct SankeyLink {
    pub from: String,
    pub to: String,
    pub flow: f64,
    pub color_from: Option<Color>,
    pub color_to: Option<Color>,
}
```

**Step 2: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: `SankeyLink { .. }` を構築している既存箇所(parser や tests)で `color_from`/`color_to` 未指定エラー。次タスクで埋める。

---

### Task B.2: SankeyFlow schema と strict 許可キーを拡張

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`(`SankeyFlow`)
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`check_unknown_keys_sankey`)

**Step 1: SankeyFlow に camelCase + per-link 色フィールドを追加**

`schema/chartjs.rs` の `SankeyFlow`(行 1156-1163 付近)を書き換え:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SankeyFlow {
    pub from: String,
    pub to: String,
    /// フロー量は非負(parser が flow < 0 を拒否するのに合わせる)。
    #[schemars(range(min = 0.0))]
    pub flow: f64,
    /// per-link 色上書き(shorthand): colorFrom/colorTo が個別に指定されない場合、
    /// この値が両端の stop 色として使われる。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorString>,
    /// per-link の from 側 stop 色上書き。指定なしは dataset の colorFrom を使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_from: Option<ColorString>,
    /// per-link の to 側 stop 色上書き。指定なしは dataset の colorTo を使用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_to: Option<ColorString>,
}
```

**Step 2: strict 許可キー(flow 要素)を拡張**

`frontend/chartjs.rs::check_unknown_keys_sankey`(行 1104-1113 付近)の point キー配列を書き換え:

```rust
                                check_object(
                                    pt,
                                    &["from", "to", "flow", "color", "colorFrom", "colorTo"],
                                    &format!("data.datasets[{i}].data[{j}]"),
                                )?;
```

**Step 3: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: schema/strict は通り、parser 側の `SankeyLink { from, to, flow }` 生成で未指定エラーが継続。次タスクで埋める。

---

### Task B.3: parse_sankey で per-link 色を読み取り precedence を実装

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`parse_sankey`)

**Step 1: 内部 struct `Flow` を拡張**

`parse_sankey` 内 `struct Flow`(行 1851-1856 付近)を書き換え:

```rust
    #[derive(Deserialize)]
    struct Flow {
        from: String,
        to: String,
        flow: f64,
        #[serde(default)]
        color: Option<String>,
        #[serde(rename = "colorFrom", default)]
        color_from: Option<String>,
        #[serde(rename = "colorTo", default)]
        color_to: Option<String>,
    }
```

**Step 2: リンク構築ループを precedence 込みで書き換え**

`parse_sankey` の links 構築ループ(行 1865-1875 付近)を書き換え:

```rust
    // リンク構築 + flow 有限性チェック。入力順を保持する。
    // per-link 色 precedence:
    //   effective_from = flow.color_from ?? flow.color ?? None  (None なら dataset フォールバック)
    //   effective_to   = flow.color_to   ?? flow.color ?? None  (None なら dataset フォールバック)
    // 不正な色文字列は明示エラー(silent default にしない)。
    let mut links = Vec::with_capacity(ds.data.len());
    for (i, f) in ds.data.into_iter().enumerate() {
        if !f.flow.is_finite() || f.flow < 0.0 {
            return Err("sankey flow must be a non-negative finite number".to_string());
        }
        let parse_flow_color = |name: &str, s: &str| -> Result<Color, String> {
            parse_color(s).ok_or_else(|| {
                format!(
                    "sankey data[{i}].{name} is not a valid color: {s}"
                )
            })
        };
        let shared = match f.color.as_deref() {
            Some(s) => Some(parse_flow_color("color", s)?),
            None => None,
        };
        let cf = match f.color_from.as_deref() {
            Some(s) => Some(parse_flow_color("colorFrom", s)?),
            None => shared,
        };
        let ct = match f.color_to.as_deref() {
            Some(s) => Some(parse_flow_color("colorTo", s)?),
            None => shared,
        };
        links.push(SankeyLink {
            from: f.from,
            to: f.to,
            flow: f.flow,
            color_from: cf,
            color_to: ct,
        });
    }
```

**Step 3: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: 成功(未使用の変数警告があれば無視)。

---

### Task B.4: layout/sankey.rs で per-link override を反映

**Files:**
- Modify: `crates/fulgur-chart/src/layout/sankey.rs`(`build` 内のリボン描画)

**Step 1: リボン描画ループを override 対応に書き換え**

`sankey::build` のリボンループ(行 977-1013)の match 部分を書き換え:

```rust
    // リボン(ノードより背面)。データ順。
    for (i, link) in data.iter().enumerate() {
        let from_idx = key_to_idx[&link.from];
        let to_idx = key_to_idx[&link.to];
        let from_x = nodes[from_idx].x.unwrap_or(0) as f64;
        let to_x = nodes[to_idx].x.unwrap_or(0) as f64;
        let from_y_val = nodes[from_idx].y.unwrap_or(0.0) + to_add_y[i];
        let to_y_val = nodes[to_idx].y.unwrap_or(0.0) + from_add_y[i];

        let x = px(from_x) + node_width + border_space;
        let x2 = px(to_x) - border_space;
        let y = py(from_y_val);
        let y2 = py(to_y_val);
        let height = (py(from_y_val + link.flow) - y).abs();

        // per-link 上書きを反映 (None なら dataset レベル)。alpha は dataset のみ。
        let eff_from = link.color_from.unwrap_or(color_from);
        let eff_to = link.color_to.unwrap_or(color_to);

        let d = ribbon_path(x, y, x2, y2, height);
        match color_mode {
            SankeyColorMode::From => items.push(Prim::Path {
                d,
                fill: Some(with_alpha(eff_from, alpha)),
                stroke: None,
                stroke_width: 0.0,
            }),
            SankeyColorMode::To => items.push(Prim::Path {
                d,
                fill: Some(with_alpha(eff_to, alpha)),
                stroke: None,
                stroke_width: 0.0,
            }),
            SankeyColorMode::Gradient => items.push(Prim::GradientPath {
                d,
                x0: x,
                x1: x2,
                stop0: with_alpha(eff_from, alpha),
                stop1: with_alpha(eff_to, alpha),
            }),
        }
    }
```

**Step 2: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: 成功。

---

### Task B.5: per-link 色テスト追加

**Files:**
- Test: `crates/fulgur-chart/tests/render_sankey.rs`

**Step 1: 失敗するテストを追加**

`tests/render_sankey.rs` の末尾に追加:

```rust
#[test]
fn sankey_per_link_color_short_form_overrides_both_stops() {
    // per-link `color` は from/to 両 stop の shorthand。
    let with_override = r#"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#36a2eb","colorTo":"#ff6384",
        "data":[{"from":"A","to":"B","flow":1,"color":"#00ff00"}]
    }]}}"#;
    let svg = render(with_override);
    // gradient stops はいずれも指定色由来 (00ff00) を含み、dataset 色 (36a2eb / ff6384) は含まない。
    assert!(svg.contains("<linearGradient"), "gradient mode default");
    assert!(svg.contains("#00ff00") || svg.contains("00ff00"), "override color present: {svg}");
    assert!(
        !svg.contains("36a2eb") && !svg.contains("ff6384"),
        "dataset colors overridden: {svg}"
    );
}

#[test]
fn sankey_per_link_color_from_overrides_only_from_stop() {
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "colorFrom":"#111111","colorTo":"#222222",
        "data":[{"from":"A","to":"B","flow":1,"colorFrom":"#abcdef"}]
    }]}}"#;
    let svg = render(json);
    assert!(svg.contains("abcdef"), "from override present");
    assert!(svg.contains("222222"), "to keeps dataset value");
    assert!(!svg.contains("111111"), "from dataset value replaced");
}

#[test]
fn sankey_per_link_color_from_wins_over_color_shorthand() {
    // color と colorFrom を併用: colorFrom が勝つ (from 側)。to 側は color の値。
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":1,"color":"#aa0000","colorFrom":"#00aa00"}]
    }]}}"#;
    let svg = render(json);
    assert!(svg.contains("00aa00"), "from uses explicit colorFrom");
    assert!(svg.contains("aa0000"), "to uses shorthand color");
}

#[test]
fn sankey_per_link_color_deterministic() {
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":1,"color":"#123456"}]
    }]}}"#;
    assert_eq!(render(json), render(json));
}

#[test]
fn sankey_per_link_color_from_invalid_rejected() {
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":1,"colorFrom":"not-a-color"}]
    }]}}"#;
    let err = chartjs::parse(json, false).unwrap_err();
    assert!(err.contains("colorFrom"), "error must mention colorFrom: {err}");
}

#[test]
fn sankey_per_link_color_works_with_from_mode() {
    // colorMode=from + per-link color: リンクごとの effective_from が単色塗りに使われる。
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "colorMode":"from",
        "data":[{"from":"A","to":"B","flow":1,"colorFrom":"#abcdef"}]
    }]}}"#;
    let svg = render(json);
    assert!(!svg.contains("<linearGradient"), "from mode → solid");
    assert!(svg.contains("abcdef"), "per-link colorFrom fills path");
}
```

**Step 2: テストを実行**

Run: `cargo test -p fulgur-chart --test render_sankey sankey_per_link 2>&1 | tail -20`
Expected: 全 6 テストが PASS。

**Step 3: 既存 sankey スイート全体を回して byte 互換確認**

Run: `cargo test -p fulgur-chart --test render_sankey 2>&1 | tail -20`
Expected: すべて green(既存 13 + 追加 6 = 19)。特に `sankey_snapshot` / `sankey_is_byte_deterministic` に regression がないこと。

---

### Task B.6: Phase B コミット

**Step 1: git status で対象確認**

Run: `git status --short`
Expected: `ir.rs`, `schema/chartjs.rs`, `frontend/chartjs.rs`, `layout/sankey.rs`, `tests/render_sankey.rs` が変更対象。

**Step 2: schema 生成物があれば再生成**

Task A.4 と同じ手順。

**Step 3: コミット**

Run:
```bash
git add crates/fulgur-chart/src/ir.rs \
        crates/fulgur-chart/src/schema/chartjs.rs \
        crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/src/layout/sankey.rs \
        crates/fulgur-chart/tests/render_sankey.rs
git commit -m "feat(sankey): per-link color / colorFrom / colorTo overrides

SankeyFlow accepts optional color (shorthand for both stops), colorFrom,
and colorTo. Precedence: flow.colorFrom || flow.color || dataset.colorFrom
(same for to). colorMode stays dataset-level per chartjs-chart-sankey.
Existing Prim::GradientPath already carries per-instance stops, so the SVG
def path needs no change.

Refs: fulgur-chart-40h"
```

Expected: 1 コミット作成。

---

## Phase C: dataset.parsing キー再マップ

`parse_sankey` を 2 段パースに書き換える。data 要素を `serde_json::Value` として受け、effective key で拾い直して正規化 Value を組み、`SankeyFlow` の内部 struct に deserialize する。

### Task C.1: SankeyParsing schema 追加と strict 許可キー拡張

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`check_unknown_keys_sankey`)

**Step 1: SankeyParsing 構造体を新規追加**

`schema/chartjs.rs` の SankeyOptions 定義の近く(SankeyDataset の**手前**)に追加:

```rust
/// dataset.parsing による from/to/flow キー再マップ。
///
/// 指定したキーがある場合、入力 JSON の flow 要素はそのキーから値を読む。
/// 例: `parsing: { flow: "value" }` を与えると `{ from, to, value }` の形式で受理する。
/// 指定なしの場合は default キー名 (`from`/`to`/`flow`) を使う。
///
/// 注意: parsing 指定時は入力 JSON が本 schema の `SankeyFlow` と乖離する(schema 上は
/// 常に `from`/`to`/`flow` を要求している)。schema-driven なクライアントで parsing を
/// 使う場合、data 部分は事前検証を無効化する必要がある。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SankeyParsing {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
}
```

**Step 2: SankeyDataset に parsing フィールドを追加**

`SankeyDataset` の(Task A.2 で編集した位置に隣接する)`column` の直後あたりに追加:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsing: Option<SankeyParsing>,
```

**Step 3: strict 許可キー(dataset)に "parsing" を追加**

`check_unknown_keys_sankey` の dataset キー配列(Task A.3 で編集済み)に `"parsing"` を追加。位置は `"column"` の後で OK。

**Step 4: strict 許可キー(flow 要素) — Task C.3 で dynamic 対応**

flow 要素の allowlist は parsing 未指定時のみ機能させる(現状)。Task C.3 で parsing 指定時に allowlist を条件分岐する。ここでは触らない。

**Step 5: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: 成功(まだ parser は parsing を読まないので runtime は現状のまま)。

---

### Task C.2: parse_sankey を 2 段パースに書き換え

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`parse_sankey` 内)

**Step 1: 内部 struct を書き換え**

`parse_sankey` 内の `struct DS`(Task A.3/B.3 で拡張済み)の `data` フィールドを `Vec<serde_json::Value>` に変更し、`Parsing` 構造体を追加:

```rust
    #[derive(Deserialize)]
    struct DS {
        #[serde(default)]
        label: String,
        data: Vec<serde_json::Value>,  // ← 変更: 一旦 raw で受ける
        #[serde(rename = "colorFrom", default)]
        color_from: Option<String>,
        // ... 他フィールドは Task A.3/B.3 の状態を維持 ...
        #[serde(default)]
        parsing: Option<Parsing>,  // ← 追加
    }

    #[derive(Deserialize)]
    struct Parsing {
        #[serde(default)]
        from: Option<String>,
        #[serde(default)]
        to: Option<String>,
        #[serde(default)]
        flow: Option<String>,
    }
```

`struct Flow` は残す(Task B.3 で拡張した per-link 色フィールドを維持)。

**Step 2: リンク構築ループの手前で effective key を決定**

`parse_sankey` の `if raw.data.datasets.len() != 1` 直後、`ds` 取り出しの後に:

```rust
    // parsing の effective key。未指定なら default 名 (from/to/flow) を使う。
    // 指定時は入力 JSON からそのキー名で値を取り出す(chartjs 挙動)。
    let parsing = ds.parsing.as_ref();
    let key_from = parsing
        .and_then(|p| p.from.as_deref())
        .unwrap_or("from");
    let key_to = parsing
        .and_then(|p| p.to.as_deref())
        .unwrap_or("to");
    let key_flow = parsing
        .and_then(|p| p.flow.as_deref())
        .unwrap_or("flow");
```

**Step 3: リンク構築ループを 2 段パースに書き換え**

Task B.3 のリンク構築ループを書き換え:

```rust
    // リンク構築: 各要素を Value としてパースし、parsing で指定された effective key で
    // from/to/flow を拾ってから正規化 Value を組み、Flow struct に deserialize する。
    // parsing 未指定なら key_* は default 名 なので、既存 spec に対する挙動は変わらない。
    // per-link color/colorFrom/colorTo は parsing で remap しない(chartjs 互換)。
    let mut links = Vec::with_capacity(ds.data.len());
    for (i, raw_entry) in ds.data.into_iter().enumerate() {
        let obj = raw_entry.as_object().ok_or_else(|| {
            format!("sankey data[{i}] must be an object")
        })?;
        let take_str = |k: &str| -> Result<String, String> {
            let v = obj.get(k).ok_or_else(|| {
                format!(
                    "sankey data[{i}] missing key '{k}' (mapped via dataset.parsing)"
                )
            })?;
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("sankey data[{i}].{k} must be a string"))
        };
        let take_num = |k: &str| -> Result<f64, String> {
            let v = obj.get(k).ok_or_else(|| {
                format!(
                    "sankey data[{i}] missing key '{k}' (mapped via dataset.parsing)"
                )
            })?;
            v.as_f64()
                .ok_or_else(|| format!("sankey data[{i}].{k} must be a number"))
        };
        let from = take_str(key_from)?;
        let to = take_str(key_to)?;
        let flow = take_num(key_flow)?;
        if !flow.is_finite() || flow < 0.0 {
            return Err("sankey flow must be a non-negative finite number".to_string());
        }
        // per-link color は常に固定キー ("color"/"colorFrom"/"colorTo") で読む。
        let color = obj.get("color").and_then(|v| v.as_str()).map(str::to_owned);
        let color_from_str = obj.get("colorFrom").and_then(|v| v.as_str()).map(str::to_owned);
        let color_to_str = obj.get("colorTo").and_then(|v| v.as_str()).map(str::to_owned);
        // color 検証 & precedence(Task B.3 と同じロジック)。
        let parse_flow_color = |name: &str, s: &str| -> Result<Color, String> {
            parse_color(s).ok_or_else(|| {
                format!("sankey data[{i}].{name} is not a valid color: {s}")
            })
        };
        let shared = match color.as_deref() {
            Some(s) => Some(parse_flow_color("color", s)?),
            None => None,
        };
        let cf = match color_from_str.as_deref() {
            Some(s) => Some(parse_flow_color("colorFrom", s)?),
            None => shared,
        };
        let ct = match color_to_str.as_deref() {
            Some(s) => Some(parse_flow_color("colorTo", s)?),
            None => shared,
        };
        links.push(SankeyLink {
            from,
            to,
            flow,
            color_from: cf,
            color_to: ct,
        });
    }
```

(Task B.3 で追加した内部 `struct Flow` は不要になるので削除する。同時に `use serde` の `Deserialize` 経由での flow パースも不要になる。)

**Step 4: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: 成功。unused struct 警告が出たら削除。

---

### Task C.3: strict 許可キー(flow 要素)を parsing 対応に

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`(`check_unknown_keys_sankey`)

**Step 1: parsing を先読みして許可キーを決定**

`check_unknown_keys_sankey` の flow 要素ループ(現在は `check_object(pt, &["from", "to", "flow", "color", "colorFrom", "colorTo"], ...)`)を書き換える:

```rust
                    // parsing 指定時は from/to/flow の代わりに parsing で指定された
                    // キー名を許可する(残る "color"/"colorFrom"/"colorTo" は固定)。
                    let (key_from, key_to, key_flow) = {
                        let p = ds.get("parsing").and_then(|v| v.as_object());
                        let s = |k: &str, dflt: &'static str| -> String {
                            p.and_then(|o| o.get(k))
                                .and_then(|v| v.as_str())
                                .map(str::to_owned)
                                .unwrap_or_else(|| dflt.to_string())
                        };
                        (s("from", "from"), s("to", "to"), s("flow", "flow"))
                    };
                    if let Some(points) = ds.get("data").and_then(|v| v.as_array()) {
                        for (j, pt) in points.iter().enumerate() {
                            if let Some(pt) = pt.as_object() {
                                check_object(
                                    pt,
                                    &[
                                        key_from.as_str(),
                                        key_to.as_str(),
                                        key_flow.as_str(),
                                        "color",
                                        "colorFrom",
                                        "colorTo",
                                    ],
                                    &format!("data.datasets[{i}].data[{j}]"),
                                )?;
                            }
                        }
                    }
```

(現在の `if let Some(points) = ...` ブロックをこの新しい構造で置き換える。既存の for ループはこの中に取り込む。)

**Step 2: check_object のシグネチャ確認**

Run: `grep -n "fn check_object" crates/fulgur-chart/src/frontend/chartjs.rs`
Expected: `fn check_object(obj: &Map<String, Value>, allowed: &[&str], path: &str) -> Result<(), String>` またはそれに近い。`&[&str]` を要求するなら slice を組み立てる。

- `check_object` が `&[&str]` を取る場合、`&[key_from.as_str(), key_to.as_str(), key_flow.as_str(), "color", "colorFrom", "colorTo"]` は借用の生存期間で通る(同スコープ内で `key_*: String` を保持しているため)。

**Step 3: cargo build**

Run: `cargo build -p fulgur-chart 2>&1 | tail -10`
Expected: 成功。

---

### Task C.4: parsing テスト追加

**Files:**
- Test: `crates/fulgur-chart/tests/render_sankey.rs`

**Step 1: 失敗するテストを追加**

`tests/render_sankey.rs` の末尾に追加:

```rust
#[test]
fn sankey_parsing_flow_only() {
    // parsing.flow="value" だけを指定: from/to は default キー。
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "parsing":{"flow":"value"},
        "data":[{"from":"A","to":"B","value":3}]
    }]}}"#;
    let svg = render(json);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn sankey_parsing_all_three_keys() {
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "parsing":{"from":"src","to":"dst","flow":"value"},
        "data":[{"src":"A","dst":"B","value":3},{"src":"B","dst":"C","value":2}]
    }]}}"#;
    let svg = render(json);
    assert!(svg.starts_with("<svg"));
    assert!(!svg.contains("NaN"));
}

#[test]
fn sankey_parsing_regression_no_parsing_matches_baseline() {
    // parsing なし: 既存 spec の描画が完全一致(regression 検証)。
    let baseline = r#"{"type":"sankey","data":{"datasets":[{
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"#;
    let with_empty_parsing = r#"{"type":"sankey","data":{"datasets":[{
        "parsing":{},
        "data":[{"from":"A","to":"B","flow":1}]
    }]}}"#;
    assert_eq!(render(baseline), render(with_empty_parsing));
}

#[test]
fn sankey_parsing_missing_key_reports_error() {
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "parsing":{"flow":"value"},
        "data":[{"from":"A","to":"B","flow":3}]
    }]}}"#;
    let err = chartjs::parse(json, false).unwrap_err();
    assert!(err.contains("value"), "error mentions missing mapped key: {err}");
}

#[test]
fn sankey_parsing_prefers_mapped_key_over_default() {
    // parsing.from="src" を指定した場合、入力に "from" と "src" の両方があっても
    // "src" のみが使われる。ここでは "from" だけ違う値にして、"src" 値のノードが
    // 描画されることを間接的に確認する(labels 経由)。
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "parsing":{"from":"src"},
        "labels":{"MAPPED":"MAPPED"},
        "data":[{"src":"MAPPED","from":"IGNORED","to":"B","flow":1}]
    }]}}"#;
    let svg = render(json);
    assert!(svg.contains("MAPPED"), "mapped key value used as node id");
    assert!(!svg.contains("IGNORED"), "default 'from' ignored when parsing.from set");
}

#[test]
fn sankey_parsing_with_per_link_color() {
    // parsing は from/to/flow のみを remap し、color 関連キーは固定名のまま読まれる。
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "parsing":{"flow":"value"},
        "data":[{"from":"A","to":"B","value":1,"color":"#abcdef"}]
    }]}}"#;
    let svg = render(json);
    assert!(svg.contains("abcdef"), "per-link color still applies with parsing");
}

#[test]
fn sankey_parsing_deterministic() {
    let json = r#"{"type":"sankey","data":{"datasets":[{
        "parsing":{"from":"src","to":"dst","flow":"v"},
        "data":[{"src":"A","dst":"B","v":1}]
    }]}}"#;
    assert_eq!(render(json), render(json));
}
```

**Step 2: テストを実行**

Run: `cargo test -p fulgur-chart --test render_sankey sankey_parsing 2>&1 | tail -20`
Expected: 全 7 テストが PASS。

**Step 3: sankey スイート全体で regression 確認**

Run: `cargo test -p fulgur-chart --test render_sankey 2>&1 | tail -25`
Expected: 全 (13 + 6 + 7 = 26) テストが green。既存スナップショットに変化なし。

---

### Task C.5: Phase C コミット

**Step 1: git status で対象確認**

Run: `git status --short`
Expected: `schema/chartjs.rs`, `frontend/chartjs.rs`, `tests/render_sankey.rs` が変更対象。

**Step 2: schema 生成物あれば再生成**

Task A.4 と同じ手順。

**Step 3: コミット**

Run:
```bash
git add crates/fulgur-chart/src/schema/chartjs.rs \
        crates/fulgur-chart/src/frontend/chartjs.rs \
        crates/fulgur-chart/tests/render_sankey.rs
git commit -m "feat(sankey): dataset.parsing for from/to/flow key remap

Two-pass parse: read each flow entry as a JSON Value, resolve effective
key names via dataset.parsing (defaulting to from/to/flow), then normalize
into the internal Flow shape. color / colorFrom / colorTo are never
remapped (chartjs-chart-sankey behavior). Strict allowlist accepts the
mapped key names alongside the fixed color keys.

Refs: fulgur-chart-40h"
```

---

## Phase D: 統合検証

### Task D.1: 全ワークスペーステスト

**Step 1: workspace 全体でのテスト**

Run: `cargo test --workspace 2>&1 | tail -30`
Expected: 全 crate green。

**Step 2: golden PNG のバイト一致確認**

Run: `find . -name 'sankey*.png' -not -path '*/target/*' -not -path '*/node_modules/*'`
- 出力があれば、`cargo test` 内で golden 比較が回っているはず。green ならバイト一致 OK。

**Step 3: schema drift 確認**

Run: `git diff --stat schemas/ 2>/dev/null; git status --short schemas/ 2>/dev/null`
- 更新済みでコミット済みなら OK。未追跡差分があれば再生成して Phase A/B/C の対応コミットに含めるか、統合 fixup コミットで扱う。

### Task D.2: verification-before-completion

`superpowers:verification-before-completion` スキルを起動し、Acceptance Criteria (fulgur-chart-40h の acceptance) を逐項目 verify する。

### Task D.3: finishing-a-development-branch

`superpowers:finishing-a-development-branch` スキルを起動し、PR 作成か main への merge を選択する。

### Task D.4: beads issue close

CI/レビュー通過後:

```bash
bd close fulgur-chart-40h
```

---

## 実装順の妥当性メモ

- Phase A は独立で、既存 code path を全く触らない。既存 snapshot は byte 不変。
- Phase B は SankeyLink IR に変更を入れるが、パーサでは `None`(dataset フォールバック)にして既存 spec を通す。既存 snapshot は byte 不変。
- Phase C は Phase B で追加した per-link color フィールドを踏まえて 2 段パースに置き換える。B→C の順で書くと C の per-link color テスト (Task C.4 の parsing_with_per_link_color) が既に動く前提で書ける。

## 検討し採用しなかった案

- **SankeyFlow に `#[serde(flatten)] extra: HashMap<String, Value>` で動的キー**: `deny_unknown_fields` を捨てることになり、strict モードの契約が緩む。2 段パースを採用。
- **parsing.color の追加**: chartjs-chart-sankey には `parsing.color` は存在しない。導入しない。
- **hoverColor 未指定時の色値検証もスキップ**: silent ignore になり、typo が握りつぶされる。明示検証を採用。
