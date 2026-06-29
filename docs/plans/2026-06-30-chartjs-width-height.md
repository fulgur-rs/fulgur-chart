# chartjs frontend top-level width/height Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (or subagent-driven-development) to implement this plan task-by-task.

**Goal:** `chart.js` spec の top-level `width`/`height` を全チャート種別で `ChartSpec.width/height` に反映し、ハードコードの 800x450 を置き換える(fulgur-chart-tgb)。

**Architecture:** チャート種別ごとに 3 層を 1 ユニットとして移動する: (1) parser の Raw ラッパーに `width/height: Option<f64>` を追加し ChartSpec 構築を `raw.width.unwrap_or(現行default)` に変更, (2) strict の `check_unknown_keys*` の top-level 許可キーに `"width","height"` を追加, (3) `schema/chartjs.rs` の各 `*Spec` 構造体に `width/height: Option<f64>` フィールドを追加。`guard.rs::validate_spec` は既に width/height を範囲検証するので変更不要。各 layout は spec.width/height を実消費する。

**Tech Stack:** Rust, serde, schemars, tiny-skia(下流)。テストは `crates/fulgur-chart/tests/frontend_chartjs.rs`。

**実行方針(advisor 指針):** チャート種別ごとに分割して別エージェントに振らない。単一の一貫したパスで実装する(N エージェントが微妙に異なる width/height 処理を発明する不整合を防ぐ)。wordCloud(`WcWrapper` / `check_unknown_keys_wordcloud` / `WordCloudSpec`)が完成済みの参照パターン。

**現行 default(全て維持):** 主要パス/matrix/treemap/sankey/gauge = 800.0/450.0、wordCloud = 500.0/300.0(変更しない)。

**対象ファイル:**
- `crates/fulgur-chart/src/frontend/chartjs.rs`(parser + strict check)
- `crates/fulgur-chart/src/schema/chartjs.rs`(公開スキーマ)
- `crates/fulgur-chart/tests/frontend_chartjs.rs`(テスト)
- `crates/chart-server/src/handlers/chart.rs`(Task 4: `?w=&h=` 合成経路の検証テスト)

---

## Task 1: 主要 parse パス(parser + strict check)

主要 `parse` は Bar/Line/Pie/Doughnut/Scatter/Bubble/Radar/Progress/Boxplot/Sparkline/PolarArea/OutlabeledPie/OutlabeledDoughnut を backing する。

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
  - `struct RawSpec`(:20-26): `width/height: Option<f64>` 追加
  - `parse` の `Ok(ChartSpec{..})`(:667-668): `width: raw.width.unwrap_or(800.0)` / `height: raw.height.unwrap_or(450.0)`
  - `check_unknown_keys`(:835): `check_object(top, &["type","data","options","width","height"], "")`
  - `check_unknown_keys_progress`(:1192): 同上(progress も主要 parse 経由・専用 strict check)

**Step 1: 失敗するテストを書く**

`tests/frontend_chartjs.rs` に追加:
```rust
#[test]
fn parses_top_level_width_height() {
    let json = r##"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1.0]}]},"width":640,"height":360}"##;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.width, 640.0);
    assert_eq!(spec.height, 360.0);
}

#[test]
fn defaults_width_height_when_absent() {
    let json = r##"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1.0]}]}}"##;
    let spec = chartjs::parse(json, false).unwrap();
    assert_eq!(spec.width, 800.0);
    assert_eq!(spec.height, 450.0);
}

#[test]
fn strict_allows_top_level_width_height() {
    let json = r##"{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1.0]}]},"width":640,"height":360}"##;
    assert!(chartjs::parse(json, true).is_ok());
}
```

**Step 2: 失敗を確認**

Run: `cargo test -p fulgur-chart --test frontend_chartjs parses_top_level_width_height`
Expected: FAIL(width が 800 のまま / または strict で未知キーエラー)

**Step 3: 実装**

`RawSpec` に追加:
```rust
    #[serde(default)]
    options: RawOptions,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    height: Option<f64>,
}
```
`parse` の ChartSpec 構築:
```rust
        width: raw.width.unwrap_or(800.0),
        height: raw.height.unwrap_or(450.0),
```
`check_unknown_keys`(:835)と `check_unknown_keys_progress`(:1192)の top-level `check_object` に `"width","height"` を追加。

**Step 4: テスト通過を確認**

Run: `cargo test -p fulgur-chart --test frontend_chartjs`
Expected: PASS(既存 76 + 新規)

**Step 5: コミット**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "fix(chartjs): honor top-level width/height in main parse path"
```

---

## Task 2: 特殊 parse パス(matrix / treemap / sankey / gauge)

各 wrapper に width/height を追加し ChartSpec 構築と strict check を更新する。

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
  - `TreemapWrapper`(:1238)+ ChartSpec(:1366-1367)+ `check_unknown_keys_treemap`(:1462)
  - `MatrixWrapper`(:1561)+ ChartSpec(:1719-1720)+ `check_unknown_keys_matrix`(:952)
  - sankey `struct W`(:1731)+ ChartSpec(:1927-1928)+ `check_unknown_keys_sankey`(:1014)
  - `GaugeWrapper`(:1938)+ ChartSpec(:2145-2146)+ `check_unknown_keys_gauge`(:1093)(gauge と radialGauge 両方が parse_gauge 経由)

各 wrapper に `#[serde(default)] width: Option<f64>` / `height: Option<f64>` を追加し、ChartSpec を `raw.width.unwrap_or(800.0)` / `raw.height.unwrap_or(450.0)` に変更、各 `check_object(top, ...)` に `"width","height"` を追加。

**Step 1: 失敗するテストを書く**

各特殊パスで非デフォルトサイズが反映されることを検証(代表 4 種 + strict):
```rust
#[test]
fn special_paths_honor_width_height() {
    let cases = [
        r##"{"type":"matrix","data":{"datasets":[{"data":[{"x":"a","y":"b","v":1.0}]}]},"width":640,"height":360}"##,
        r##"{"type":"treemap","data":{"datasets":[{"tree":[{"name":"a","value":1.0}],"key":"value"}]},"width":640,"height":360}"##,
        r##"{"type":"sankey","data":{"datasets":[{"data":[{"from":"a","to":"b","flow":1.0}]}]},"width":640,"height":360}"##,
        r##"{"type":"gauge","data":{"datasets":[{"value":50.0}]},"options":{"min":0,"max":100},"width":640,"height":360}"##,
    ];
    for json in cases {
        let spec = chartjs::parse(json, false).unwrap();
        assert_eq!(spec.width, 640.0, "spec={json}");
        assert_eq!(spec.height, 360.0, "spec={json}");
        assert!(chartjs::parse(json, true).is_ok(), "strict failed for {json}");
    }
}
```
※ 各種別の最小有効 JSON は既存テスト(matrix/treemap/sankey/gauge の roundtrip・parse テスト)から正確な形を借りること。上記が無効なら既存サンプルに `"width"/"height"` を足す形に変える。

**Step 2-4:** 失敗確認 → 実装 → 全テスト通過(`cargo test -p fulgur-chart --test frontend_chartjs`)

**Step 5: コミット**

```bash
git commit -am "fix(chartjs): honor top-level width/height in matrix/treemap/sankey/gauge paths"
```

---

## Task 3: 公開スキーマ(schema/chartjs.rs)

`deny_unknown_fields` の各 `*Spec` に width/height を追加。WordCloudSpec(:843-852)が参照パターン。

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`
  - 追加対象(15構造体): `BarSpec` `LineSpec` `PieSpec`(Pie/Doughnut/PolarArea 共有)`ScatterSpec` `BubbleSpec` `RadarSpec` `MatrixSpec` `TreemapSpec` `ProgressSpec` `BoxplotSpec` `SparklineSpec` `GaugeSpec` `RadialGaugeSpec` `OutlabeledPieSpec`(2 variant 共有)`SankeySpec`
  - `WordCloudSpec` は対応済(変更不要)

各構造体に追加(WordCloudSpec と同一):
```rust
    /// Canvas width in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    /// Canvas height in px. Defaults to fulgur's built-in size when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
```
※ ユーザー向けスキーマ doc コメントは英語(メンテナ方針)。

**Step 1: 失敗するテストを書く**

`tests/frontend_chartjs.rs`:
```rust
#[test]
fn schema_accepts_top_level_width_height() {
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let cases = [
        r##"{"type":"bar","data":{"datasets":[{"data":[1.0]}]},"width":640,"height":360}"##,
        r##"{"type":"line","data":{"datasets":[{"data":[1.0]}]},"width":640,"height":360}"##,
        r##"{"type":"pie","data":{"datasets":[{"data":[1.0]}]},"width":640,"height":360}"##,
        r##"{"type":"gauge","data":{"datasets":[{"value":50.0}]},"width":640,"height":360}"##,
        // 各 *Spec を最低 1 つカバーするよう代表種別を追加
    ];
    for json in cases {
        let r: Result<ChartJsSpec, _> = serde_json::from_str(json);
        assert!(r.is_ok(), "schema rejected {json}: {:?}", r.err());
    }
}
```
※ 各 `*Spec` の最小有効 JSON は既存の `<kind>_schema_roundtrip` テストから借り、`"width"/"height"` を付与する。全 15 構造体をカバーすること(patch coverage 100%)。

**Step 2-4:** 失敗確認 → 実装 → 通過(`cargo test -p fulgur-chart`)

**Step 5: コミット**

```bash
git commit -am "feat(schema): accept top-level width/height for all chart kinds"
```

---

## Task 4: 統合検証 + chart-server 経路

**Step 1:** chart-server の `?w=&h=` が反映されることをユニットで確認(`apply_overrides_value` は既存テストあり。parse 後 spec.width に届くことを 1 件追加してもよい)。

**Step 2: 全体検証**

```bash
cargo test -p fulgur-chart
cargo test -p chart-server
cargo clippy -p fulgur-chart -p chart-server --all-targets -- -D warnings
cargo fmt --check
```

**Step 3:** patch coverage 100% 確認(新規/変更行が全てテストで実行されること)。

**Step 4: コミット(必要なら)**

```bash
git commit -am "test(chartjs): integration coverage for width/height plumbing"
```

---

## 非対象 / フォローアップ
- vegalite フロントエンドの width/height は本 issue 対象外(別途確認 → 必要なら follow-up issue)。
- README/OpenAPI の width/height 記述は既に対応済(確認のみ)。
