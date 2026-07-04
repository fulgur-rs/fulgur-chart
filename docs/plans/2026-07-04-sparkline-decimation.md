# Sparkline Decimation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 巨大データの sparkline を Chart.js 互換 decimation で自動間引きし、SVG/PNG を高速化＋線をクリーン化する（fulgur-chart-uir）。

**Architecture:** 既存 `spec.decimation`（全 ChartKind 共通 IR）と `layout::decimate::{resolve, decimate_one}` を再利用する。sparkline はマーカー無し・gap 分割無しなので、系列の全点を**単一セグメント**として間引くだけ。line の per-segment / マーカー抑制ロジックは不要。schema `SparklineOptions` に `plugins.decimation` を追加し公開スキーマの parity gap を閉じる。

**Tech Stack:** Rust, cargo, insta（snapshot）, schemars（JSON Schema）, serde_json。

**発動条件（line 完全同一継承）:** `enabled` 既定 true、threshold = 論理plot幅px×4、samples 既定 = 論理plot幅px。既定800px幅 → 約3,136点、200px幅 → 約736点で自動発動。

**重要な不変条件:**
- **受け入れ#1（no-fire バイト不変）**: `resolve()==None`（threshold 未満 or `enabled:false`）のとき、変更前と完全に同一の float 列・描画順・SVG バイトを保つ。tuple 化リファクタで float がずれないこと。
- **non-finite フィルタは足さない**: line は事前フィルタするが sparkline に足すと no-fire 経路のバイトが変わる。現状維持。

**対象ファイル:**
- Modify: `crates/fulgur-chart/src/layout/sparkline.rs`（decimation 配線 + 単体テスト）
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs:743-750`（`SparklineOptions` に plugins 追加）
- Modify: `crates/fulgur-chart/tests/frontend_chartjs.rs`（parity テスト追加）
- Modify: `crates/fulgur-chart/benches/cases.rs`（任意: bench ケース）
- Modify: `CHANGELOG.md`
- Reference（変更しない）: `crates/fulgur-chart/src/layout/decimate.rs`, `crates/fulgur-chart/src/layout/line.rs`（実装パターン）, `crates/fulgur-chart/src/schema/common.rs:73`（`DecimationPlugin`）

---

## Task 1: Schema parity — `SparklineOptions.plugins.decimation`

runtime（loose/strict）は既に sparkline の `options.plugins.decimation` を受理し `spec.decimation` に解決する（確認済み）。だが公開 JSON Schema の `SparklineOptions` は `scales`/`theme` のみで decimation を広告していない。schema にも追加し parity を取る。

**Files:**
- Test: `crates/fulgur-chart/tests/frontend_chartjs.rs`（`schema_strict_parity_decimation_matrix` at:554 を mirror）
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs:743-750`

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/frontend_chartjs.rs` の `schema_strict_parity_decimation_matrix`（:570 直後）に追記:

```rust
#[test]
fn schema_strict_parity_decimation_sparkline() {
    // sparkline はマーカー無し。line と同じ decimation を受理する。runtime strict と
    // 公開 schema の両方が options.plugins.decimation を受理し、危険方向のパリティ破れ
    // (schema OK / strict NG、またはその逆) を作らないこと。
    use fulgur_chart::schema::chartjs::ChartJsSpec;
    let json = r##"{
        "type": "sparkline",
        "data": { "datasets": [{ "data": [1.0, 2.0, 3.0] }] },
        "options": { "plugins": { "decimation": { "enabled": false, "algorithm": "lttb" } } }
    }"##;
    // strict 側: 厳格パーサが受理し、enabled=false が spec に届く。
    let spec = chartjs::parse(json, true).unwrap();
    assert!(!spec.decimation.enabled);
    // schema 側: ChartJsSpec でも受理される。
    let schema_spec: ChartJsSpec = serde_json::from_str(json).unwrap();
    assert!(matches!(schema_spec, ChartJsSpec::Sparkline(_)));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test frontend_chartjs schema_strict_parity_decimation_sparkline`
Expected: FAIL — schema 側 `serde_json::from_str::<ChartJsSpec>` が `unknown field 'plugins'`（`SparklineOptions` は `deny_unknown_fields` で plugins 未定義）で panic。

**Step 3: Write minimal implementation**

`crates/fulgur-chart/src/schema/chartjs.rs` の `SparklineOptions`（:743-750）を書き換え、直前に `SparklinePlugins` を新設:

```rust
/// sparkline が受け付ける plugins。sparkline は title/legend/datalabels を描画しないため
/// decimation のみ公開する（正直な最小 schema）。line と同じ巨大データ間引きを許可する。
#[derive(Serialize, Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
pub struct SparklinePlugins {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decimation: Option<DecimationPlugin>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SparklineOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<SparklinePlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<BarScales>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
}
```

（`DecimationPlugin` は同ファイル内 `CommonPlugins`/`BarPlugins` で既に import 済み。追加 use 不要。）

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --test frontend_chartjs schema_strict_parity_decimation_sparkline`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat(chart): advertise decimation in sparkline JSON schema (parity)"
```

---

## Task 2: 単一セグメント decimation を sparkline layout に配線

sparkline は `spec.series` ごとに `Vec<(f64,f64)>` を作り area/polyline/spline を描く。これを `(x, y, index)` タプルに変え、`resolve` が発動したら `decimate_one` で単一セグメント間引きする。no-fire 時は現状と完全同一出力を保つ。

**Files:**
- Modify: `crates/fulgur-chart/src/layout/sparkline.rs`
- Test: `crates/fulgur-chart/src/layout/sparkline.rs`（末尾に `#[cfg(test)] mod tests`）

**Step 1: Write the failing tests**

`sparkline.rs` 末尾（`catmull_rom_path` の後）に追記:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::DEFAULT_FONT;
    use crate::frontend::chartjs;
    use crate::scene::Prim;

    fn build_spec(json: &str) -> Scene {
        let spec = chartjs::parse(json, false).unwrap();
        let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
        build(&spec, &m)
    }

    fn polyline_len(scene: &Scene) -> usize {
        scene
            .items
            .iter()
            .find_map(|p| match p {
                Prim::Polyline { points, .. } => Some(points.len()),
                _ => None,
            })
            .expect("sparkline should have a polyline")
    }

    fn huge_sparkline_json(extra_opts: &str) -> String {
        // 5000点 > threshold(既定800px幅 → 784*4 ≈ 3136) で発動する。
        let data: Vec<String> = (0..5000).map(|i| ((i * 7) % 13).to_string()).collect();
        format!(
            r#"{{"type":"sparkline","data":{{"datasets":[{{"data":[{}]}}]}}{}}}"#,
            data.join(","),
            extra_opts
        )
    }

    #[test]
    fn huge_sparkline_is_decimated_by_default() {
        let scene = build_spec(&huge_sparkline_json(""));
        assert!(
            polyline_len(&scene) < 5000,
            "auto-on decimation should reduce 5000 pts"
        );
    }

    #[test]
    fn huge_sparkline_passthrough_when_disabled() {
        let json = huge_sparkline_json(r#","options":{"plugins":{"decimation":{"enabled":false}}}"#);
        let scene = build_spec(&json);
        assert_eq!(
            polyline_len(&scene),
            5000,
            "enabled:false must keep all points (byte-identity path)"
        );
    }

    #[test]
    fn small_sparkline_below_threshold_keeps_all_points() {
        let scene = build_spec(r#"{"type":"sparkline","data":{"datasets":[{"data":[3,1,4,1,5,9,2,6]}]}}"#);
        assert_eq!(polyline_len(&scene), 8, "below threshold: no decimation");
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p fulgur-chart --lib layout::sparkline`
Expected: `huge_sparkline_is_decimated_by_default` FAIL（現状は間引かず 5000 点のまま）。他2つは PASS でもよい。

**Step 3: Write minimal implementation**

`sparkline.rs::build` の系列ループを書き換える。現状の `pts: Vec<(f64,f64)>`（:40-50）と area/polyline（:52-96）を以下に置換:

```rust
    // plot_width は間引き判定に使う（論理ピクセル空間、Frame は持たない）。
    let plot_width = plot_right - plot_left;

    for ser in &spec.series {
        let count = ser.values.len();
        if count == 0 {
            continue;
        }
        // (x, y, index)。x はエッジ対エッジ配置（1点系列は中央）。
        let pts: Vec<(f64, f64, usize)> = (0..count)
            .map(|i| {
                let x = if max_count == 1 {
                    (plot_left + plot_right) / 2.0
                } else {
                    plot_left + i as f64 * (plot_right - plot_left) / (max_count - 1) as f64
                };
                (x, ys.map(ser.values[i]), i)
            })
            .collect();

        // sparkline は gap 分割を持たないため系列全体を単一セグメントとして間引く。
        // 判定は系列全体の点数で（line と同じ Chart.js セマンティクス）。
        // no-fire 時は pts をそのまま使い、変更前とバイト不変を保つ。
        let pts: Vec<(f64, f64, usize)> =
            match crate::layout::decimate::resolve(&spec.decimation, plot_width, count) {
                Some((algo, samples)) => {
                    crate::layout::decimate::decimate_one(&pts, algo, samples)
                }
                None => pts,
            };

        // area（背面）
        if ser.area && pts.len() >= 2 {
            let baseline_y = ys.map(0.0_f64.clamp(domain_min, domain_max));
            let mut d = String::new();
            for (k, &(x, y, _)) in pts.iter().enumerate() {
                let cmd = if k == 0 { 'M' } else { 'L' };
                write!(d, "{} {} {} ", cmd, fmt_num(x), fmt_num(y)).unwrap();
            }
            let (last_x, _, _) = pts[pts.len() - 1];
            let (first_x, _, _) = pts[0];
            write!(
                d,
                "L {} {} L {} {} Z",
                fmt_num(last_x),
                fmt_num(baseline_y),
                fmt_num(first_x),
                fmt_num(baseline_y)
            )
            .unwrap();
            items.push(Prim::Path {
                d,
                fill: Some(ser.fill_at(0)),
                stroke: None,
                stroke_width: 0.0,
            });
        }

        // 折れ線
        if pts.len() >= 2 {
            if ser.tension <= 0.0 {
                items.push(Prim::Polyline {
                    points: pts.iter().map(|&(x, y, _)| (x, y)).collect(),
                    stroke: ser.stroke_at(0),
                    stroke_width: ser.stroke_width,
                });
            } else {
                let xy: Vec<(f64, f64)> = pts.iter().map(|&(x, y, _)| (x, y)).collect();
                let d = catmull_rom_path(&xy, ser.tension);
                items.push(Prim::Path {
                    d,
                    fill: None,
                    stroke: Some(ser.stroke_at(0)),
                    stroke_width: ser.stroke_width,
                });
            }
        }
        // マーカーなし・データラベルなし
    }
```

注意:
- `catmull_rom_path` は `&[(f64,f64)]` を取るため `xy` に変換してから渡す（シグネチャ変更不要）。
- no-fire 経路で `pts.iter().map(|&(x,y,_)| (x,y))` は元の `(x,y)` と同一 → area/polyline のバイト不変。

**Step 4: Run tests to verify they pass**

Run: `cargo test -p fulgur-chart --lib layout::sparkline`
Expected: PASS（3テスト）

Run: `cargo test -p fulgur-chart --test render_sparkline`
Expected: PASS（既存 snapshot 不変 = 受け入れ#1・#4）

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/sparkline.rs
git commit -m "feat(chart): decimate huge sparklines (single-segment, auto-on)"
```

---

## Task 3: 決定性・algorithm 切替・tension のテスト強化

**Files:**
- Test: `crates/fulgur-chart/src/layout/sparkline.rs`（`mod tests` に追記）

**Step 1: Write the tests**

```rust
    #[test]
    fn sparkline_decimation_is_deterministic() {
        let json = huge_sparkline_json("");
        let a = crate::render::render_chart(&chartjs::parse(&json, false).unwrap());
        let b = crate::render::render_chart(&chartjs::parse(&json, false).unwrap());
        assert_eq!(a, b, "same input must yield identical SVG");
    }

    #[test]
    fn sparkline_lttb_reduces_to_samples_order() {
        // lttb: samples ≈ plot_width。5000 点 → samples 前後まで減る。
        let json = huge_sparkline_json(
            r#","options":{"plugins":{"decimation":{"algorithm":"lttb","samples":200}}}"#,
        );
        let scene = build_spec(&json);
        assert!(polyline_len(&scene) <= 200, "lttb should hit samples cap");
    }

    #[test]
    fn huge_sparkline_with_tension_still_decimates_and_renders() {
        // tension>0 は Path になる（Polyline 点数は数えられない）。先に間引き→spline。
        // SVG が有限で妥当な範囲に収まることを確認（間引きが効かないと巨大化する）。
        let data: Vec<String> = (0..5000).map(|i| ((i * 7) % 13).to_string()).collect();
        let json = format!(
            r#"{{"type":"sparkline","data":{{"datasets":[{{"data":[{}],"tension":0.4}}]}}}}"#,
            data.join(",")
        );
        let svg = crate::render::render_chart(&chartjs::parse(&json, false).unwrap());
        assert!(svg.contains("<path"), "tension → Bezier path");
        assert!(!svg.contains("NaN") && !svg.contains("inf"));
        assert!(svg.len() < 200_000, "decimated spline SVG should be bounded");
    }
```

**Step 2: Run tests**

Run: `cargo test -p fulgur-chart --lib layout::sparkline`
Expected: PASS

**Step 3: Commit**

```bash
git add crates/fulgur-chart/src/layout/sparkline.rs
git commit -m "test(chart): sparkline decimation determinism / lttb / tension"
```

---

## Task 4: （任意）bench ケース sparkline_large / sparkline_large_decimated

`line_large` / `line_large_decimated`（cases.rs:31-38）を mirror。off-path ベースラインと decimation-on を両方置く。

**Files:**
- Modify: `crates/fulgur-chart/benches/cases.rs`

**Step 1: Add cases**

`line_large_decimated` ケースの直後に、sparkline 版を追加（`type:"sparkline"`、5000〜10000点、decimated 版は既定 auto-on、baseline 版は `enabled:false`）。既存ケースの生成関数シグネチャに合わせること（`cases.rs` の既存パターンを踏襲）。

**Step 2: Verify bench compiles**

Run: `cargo bench -p fulgur-chart --no-run`
Expected: コンパイル成功

**Step 3: Commit**

```bash
git add crates/fulgur-chart/benches/cases.rs
git commit -m "bench(chart): add sparkline_large decimation cases"
```

---

## Task 5: CHANGELOG + 目視確認 + 最終ゲート

**Files:**
- Modify: `CHANGELOG.md`

**Step 1: CHANGELOG 追記**

Unreleased セクションに、43h と同様のトーンで sparkline の decimation 対応を1行追記（auto-on・`enabled:false` で opt-out・schema 広告）。

**Step 2: 実物目視確認（受け入れ条件）**

巨大 sparkline を実際にレンダして線がクリーンか目で確認する（「綺麗」を思い込みで断定しない）:

```bash
# CLI/example 経由で 5000 点 sparkline を SVG/PNG 出力し目視
# （examples/ or fulgur CLI の使い方はリポの README/examples を参照）
```

**Step 3: 最終品質ゲート**

```bash
cargo fmt --check
cargo clippy -p fulgur-chart --all-targets -- -D warnings
cargo test -p fulgur-chart
```
Expected: fmt クリーン、clippy 0 警告、全テスト緑。

**Step 4: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs(chart): note sparkline decimation in CHANGELOG"
```

---

## 受け入れ条件（beads issue と一致）

1. `enabled:false` 時、巨大 sparkline でも変更前とバイト一致（`huge_sparkline_passthrough_when_disabled` + 既存 snapshot）。
2. 既定（auto-on）で巨大 sparkline が間引かれ SVG/PNG が決定的に一致。
3. min-max / lttb 両方が動作し `algorithm` で切替可能。
4. 既存 sparkline snapshot 不変（threshold 未満で回帰なし）。
5. schema と strict parser が sparkline の decimation キーで一致（parity テスト緑）。
6. 全テスト緑・clippy0・fmt クリーン。CHANGELOG に sparkline decimation 対応を記載。
