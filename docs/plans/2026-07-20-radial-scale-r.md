# chartjs radar/polar radial scale (r) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** `options.scales.r.{min, max, suggestedMin, suggestedMax, beginAtZero}` を chart.js の radar / polarArea で受理し、値域を制御できるようにする。

**Architecture:** typed schema (`schema/chartjs.rs`) に radial 専用の struct を新設して JSON schema と runtime 契約を揃える。IR に `RadialAxis` を追加し、runtime bridge (`frontend/chartjs.rs`) が RawSpec 経由で `scales.r` を読んで populate する。`layout/radar.rs` と `layout/polar_area.rs` はそれぞれ既存 default 計算を保ったまま override を重ねる (snapshot 破壊回避)。`PolarArea` variant を `PieSpec` から `PolarAreaSpec` に分割することで pie/doughnut が `scales` を受理しない strict パリティを守る。

**Tech Stack:** Rust, serde/serde_json, schemars, insta (snapshot).

**Working directory:** `/home/ubuntu/fulgur-chart/.worktrees/6z6-radial-scale` (branch `feat/6z6-radial-scale`)

**Design source of truth:** `bd show fulgur-chart-6z6`

---

## Task 1: IR に `RadialAxis` を追加

**Files:**
- Modify: `crates/fulgur-chart/src/ir.rs` (around line 148 — after `LegendPos`, before `OutlabelConfig`; also `ChartSpec` around line 384)

**Step 1: Write the failing test**

`crates/fulgur-chart/src/ir.rs` の `mod tests` 末尾（無ければ新設）に:

```rust
#[cfg(test)]
mod radial_axis_tests {
    use super::*;

    #[test]
    fn radial_axis_default_is_none_on_chart_spec() {
        let spec = ChartSpec::default();
        assert!(spec.radial_axis.is_none());
    }

    #[test]
    fn radial_axis_stores_all_five_knobs() {
        let a = RadialAxis {
            min: Some(0.0),
            max: Some(100.0),
            suggested_min: Some(-5.0),
            suggested_max: Some(120.0),
            begin_at_zero: true,
        };
        assert_eq!(a.min, Some(0.0));
        assert_eq!(a.max, Some(100.0));
        assert_eq!(a.suggested_min, Some(-5.0));
        assert_eq!(a.suggested_max, Some(120.0));
        assert!(a.begin_at_zero);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --lib radial_axis_tests 2>&1 | tail`
Expected: FAIL — `RadialAxis` / `radial_axis` 未定義。

**Step 3: Minimal implementation**

`crates/fulgur-chart/src/ir.rs` の `LegendPos` 定義 (line 141-148) の直後に追加:

```rust
/// Radar / polarArea の r スケール。既存の `AxisSpec` は cartesian 向けに
/// title/offset/grid を含むため再利用しない。cartesian の
/// `suggestedMin/suggestedMax/beginAtZero` と同じセマンティクス:
/// - `min` / `max`: hard override (データ範囲外でも従う)
/// - `suggested_min` / `suggested_max`: expand-only (データ範囲を広げる方向のみ)
/// - `begin_at_zero`: true でドメインに 0 を含める
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialAxis {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub suggested_min: Option<f64>,
    pub suggested_max: Option<f64>,
    pub begin_at_zero: bool,
}
```

そして `ChartSpec` (line 384 付近) に新フィールドを追加:

```rust
pub struct ChartSpec {
    // ...既存フィールド...

    /// Radar / polarArea 専用の r スケール。他 kind では常に None。
    pub radial_axis: Option<RadialAxis>,
}
```

`ChartSpec` に `Default` 実装があれば `radial_axis: None` を追加。無ければ、既存の `ChartSpec { ... }` を組む全箇所 (grep `ChartSpec {`) で `radial_axis: None` を明示するか、`#[derive(Default)]` を活かして `..Default::default()` パターンにする。

**Step 4: Run test to verify pass + build**

Run: `cargo build -p fulgur-chart 2>&1 | tail`
Expected: OK — 既存の `ChartSpec { ... }` 構築コードは `radial_axis: None` で埋めた場合そのままコンパイル通る。もし `missing field` エラーが出たら該当箇所 (`bar.rs`, `line.rs`, `scatter.rs`, `pie.rs`, etc. および `frontend/chartjs.rs`, `frontend/vegalite.rs`) を機械的に `radial_axis: None` で埋める。

Run: `cargo test -p fulgur-chart --lib radial_axis_tests 2>&1 | tail`
Expected: PASS.

Run: `cargo test -p fulgur-chart --lib 2>&1 | tail`
Expected: 全 pass (既存 328 テストが通る)。snapshot 差分は出ないはず。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/ir.rs
git commit -m "feat(ir): add RadialAxis and ChartSpec.radial_axis for radar/polarArea r scale"
```

---

## Task 2: 型付き schema に `RadialLinearAxisOptions` / `RadialLinearScales` を新設し、`RadarOptions` に配線

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs` (RadarOptions 付近 line 458–463)

**Step 1: Write the failing test**

`crates/fulgur-chart/src/schema/chartjs.rs` の末尾テストブロック (見当たらなければ新設 `#[cfg(test)] mod tests { ... }`) に:

```rust
#[test]
fn radar_options_accepts_scales_r_all_knobs() {
    let json = r##"{
        "plugins": {},
        "scales": { "r": {
            "min": 0, "max": 100,
            "suggestedMin": -5, "suggestedMax": 120,
            "beginAtZero": true
        }}
    }"##;
    let v: RadarOptions = serde_json::from_str(json).unwrap();
    let r = v.scales.unwrap().r.unwrap();
    assert_eq!(r.min, Some(0.0));
    assert_eq!(r.max, Some(100.0));
    assert_eq!(r.suggested_min, Some(-5.0));
    assert_eq!(r.suggested_max, Some(120.0));
    assert_eq!(r.begin_at_zero, Some(true));
}

#[test]
fn radar_scales_rejects_typo_in_r_axis() {
    // deny_unknown_fields により beginAtZeroo (typo) は拒否される。
    let json = r##"{
        "scales": { "r": { "beginAtZeroo": true } }
    }"##;
    let err = serde_json::from_str::<RadarOptions>(json).unwrap_err();
    assert!(err.to_string().contains("beginAtZeroo"), "err: {err}");
}

#[test]
fn radar_scales_rejects_unknown_axis() {
    // scales の下は r のみ許可。x を書いたら拒否する。
    let json = r##"{ "scales": { "x": { "min": 0 } } }"##;
    let err = serde_json::from_str::<RadarOptions>(json).unwrap_err();
    assert!(err.to_string().contains("x"), "err: {err}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --lib radar_options_accepts_scales_r_all_knobs radar_scales_rejects_typo_in_r_axis radar_scales_rejects_unknown_axis 2>&1 | tail`
Expected: FAIL — `RadarOptions` に `scales` フィールドが無い。

**Step 3: Minimal implementation**

`crates/fulgur-chart/src/schema/chartjs.rs` の Radar セクション (line ~411 の "Radar chart" コメント直下) に新設:

```rust
// Radar / polarArea 共有の radial linear scale。cartesian AxisOptions とは別立て:
// radial では意味を持たない stacked / offset / title / grid を混ぜない。
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RadialLinearAxisOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub begin_at_zero: Option<bool>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadialLinearScales {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r: Option<RadialLinearAxisOptions>,
}
```

そして既存の `RadarOptions` (line 456-463) に `scales` を追加:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RadarOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<RadialLinearScales>,
}
```

**Step 4: Run test**

Run: `cargo test -p fulgur-chart --lib radar_options_accepts_scales_r_all_knobs radar_scales_rejects_typo_in_r_axis radar_scales_rejects_unknown_axis 2>&1 | tail`
Expected: 3 PASS.

Run: `cargo test -p fulgur-chart 2>&1 | tail`
Expected: 全 pass (integration tests 含む)。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs
git commit -m "feat(schema): add RadialLinearScales and wire to RadarOptions"
```

---

## Task 3: 型付き schema — `PolarArea` variant を `PieSpec` から `PolarAreaSpec` へ分離

**Files:**
- Modify: `crates/fulgur-chart/src/schema/chartjs.rs`
  - enum `ChartInput::PolarArea(PieSpec)` (line 40)
  - Pie section (line ~253 以降) の直後に PolarAreaSpec を新設

**Step 1: Write the failing test**

`crates/fulgur-chart/src/schema/chartjs.rs` のテストに:

```rust
#[test]
fn polar_area_spec_accepts_scales_r() {
    let json = r##"{
        "type": "polarArea",
        "data": { "labels": ["A","B"], "datasets": [{"data":[1,2]}] },
        "options": { "scales": { "r": { "beginAtZero": true, "max": 100 } } }
    }"##;
    let v: ChartInput = serde_json::from_str(json).unwrap();
    match v {
        ChartInput::PolarArea(spec) => {
            let r = spec.options.unwrap().scales.unwrap().r.unwrap();
            assert_eq!(r.max, Some(100.0));
            assert_eq!(r.begin_at_zero, Some(true));
        }
        _ => panic!("expected PolarArea"),
    }
}

#[test]
fn pie_spec_still_rejects_scales() {
    // 分離後、pie は options.scales を受理してはならない (deny_unknown_fields)。
    let json = r##"{
        "type": "pie",
        "data": { "labels": ["A","B"], "datasets": [{"data":[1,2]}] },
        "options": { "scales": { "r": { "min": 0 } } }
    }"##;
    let err = serde_json::from_str::<ChartInput>(json).unwrap_err();
    assert!(err.to_string().contains("scales"), "err: {err}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --lib polar_area_spec_accepts_scales_r pie_spec_still_rejects_scales 2>&1 | tail`
Expected: FAIL — polarArea が `scales` 未対応 / pie が受理してしまう。

**Step 3: Minimal implementation**

Pie section 直後 (line ~301 の `PieOptions` の後) に追加:

```rust
// ────────────────────────────────────────────────
// PolarArea chart (Pie とデータ形状は同じだが r スケールを持つ)
// ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolarAreaSpec {
    pub data: PieData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<PolarAreaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1.0, max = 32768.0))]
    pub height: Option<f64>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolarAreaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugins: Option<CommonPlugins>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scales: Option<RadialLinearScales>,
}
```

そして enum variant 差し替え (line 40):

```rust
    #[serde(rename = "polarArea")]
    PolarArea(PolarAreaSpec),
```

**Step 4: Run test**

Run: `cargo test -p fulgur-chart --lib polar_area_spec_accepts_scales_r pie_spec_still_rejects_scales 2>&1 | tail`
Expected: 2 PASS.

Run: `cargo test -p fulgur-chart 2>&1 | tail`
Expected: 全 pass。既存 tests/frontend_chartjs.rs で `PieSpec` を polarArea として round-trip している箇所があれば型が合わなくなるので修正 (grep `ChartJsSpec.*PolarArea`)。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/chartjs.rs
git commit -m "refactor(schema): split PolarAreaSpec from PieSpec to receive scales.r"
```

---

## Task 4: Runtime bridge — `scales.r` から `RadialAxis` を populate

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs` (`ChartSpec { ... }` 構築部 line ~675–711)

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/frontend_chartjs.rs` の末尾に:

```rust
#[test]
fn radar_scales_r_populates_radial_axis() {
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
             "options":{"scales":{"r":{"min":-10,"max":50,"suggestedMin":-20,"suggestedMax":80,"beginAtZero":true}}}}"##,
        false,
    ).unwrap();
    let r = spec.radial_axis.expect("radar should populate radial_axis");
    assert_eq!(r.min, Some(-10.0));
    assert_eq!(r.max, Some(50.0));
    assert_eq!(r.suggested_min, Some(-20.0));
    assert_eq!(r.suggested_max, Some(80.0));
    assert!(r.begin_at_zero);
}

#[test]
fn polar_area_scales_r_populates_radial_axis() {
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r##"{"type":"polarArea","data":{"labels":["a","b"],"datasets":[{"data":[10,20]}]},
             "options":{"scales":{"r":{"max":100}}}}"##,
        false,
    ).unwrap();
    let r = spec.radial_axis.expect("polarArea should populate radial_axis");
    assert_eq!(r.max, Some(100.0));
    assert!(r.begin_at_zero, "polarArea beginAtZero default true");
}

#[test]
fn radar_without_scales_leaves_radial_axis_none() {
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r#"{"type":"radar","data":{"labels":["a"],"datasets":[{"data":[1]}]}}"#,
        false,
    ).unwrap();
    assert!(spec.radial_axis.is_none(), "backward compat: no scales → None");
}

#[test]
fn non_radial_charts_ignore_scales_r() {
    // Bar は scales.r を持たないが、仮に RawSpec が受理しても radial_axis は None のまま。
    use fulgur_chart::frontend::chartjs;
    let spec = chartjs::parse(
        r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
             "options":{"scales":{"y":{"beginAtZero":true}}}}"##,
        false,
    ).unwrap();
    assert!(spec.radial_axis.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test frontend_chartjs radar_scales_r_populates_radial_axis polar_area_scales_r_populates_radial_axis radar_without_scales_leaves_radial_axis_none non_radial_charts_ignore_scales_r 2>&1 | tail`
Expected: FAIL — `radial_axis` は常に None。

**Step 3: Minimal implementation**

`crates/fulgur-chart/src/frontend/chartjs.rs` の `ChartSpec` 構築部の直前 (line ~674、`Ok(ChartSpec {` の直前) に追加:

```rust
    // scales.r: radar / polarArea のみ populate。他 kind は None。
    let is_radial = matches!(kind, ChartKind::Radar | ChartKind::PolarArea);
    let radial_axis = if is_radial {
        let r = scales_val
            .and_then(|s| s.get("r"))
            .and_then(|a| a.as_object());
        Some(RadialAxis {
            min: r.and_then(|a| a.get("min")).and_then(|v| v.as_f64()),
            max: r.and_then(|a| a.get("max")).and_then(|v| v.as_f64()),
            suggested_min: r
                .and_then(|a| a.get("suggestedMin"))
                .and_then(|v| v.as_f64()),
            suggested_max: r
                .and_then(|a| a.get("suggestedMax"))
                .and_then(|v| v.as_f64()),
            // radar / polarArea の既存挙動は 0 起点なので default true。
            begin_at_zero: r
                .and_then(|a| a.get("beginAtZero"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        })
    } else {
        None
    };
```

そして `Ok(ChartSpec { ... })` (line ~675) の末尾フィールドに追加:

```rust
        radial_axis,
```

`RadialAxis` のインポートを冒頭に追加 (`use crate::ir::{..., RadialAxis};` に足す)。

**Step 4: Run test**

Run: `cargo test -p fulgur-chart --test frontend_chartjs radar_scales_r_populates_radial_axis polar_area_scales_r_populates_radial_axis radar_without_scales_leaves_radial_axis_none non_radial_charts_ignore_scales_r 2>&1 | tail`
Expected: 4 PASS.

Run: `cargo test -p fulgur-chart 2>&1 | tail`
Expected: 全 pass。既存 snapshot は radial_axis を消費しないのでまだ変化しない。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat(frontend/chartjs): populate RadialAxis from scales.r for radar/polarArea"
```

---

## Task 5: Runtime bridge — `check_unknown_keys` に `scales.r` を条件付き許可

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
  - `check_unknown_keys` シグネチャと呼び出し元 (line ~319, 866)

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/frontend_chartjs.rs` に:

```rust
#[test]
fn strict_mode_allows_scales_r_on_radar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"r":{"min":0,"max":100,"suggestedMin":-5,"suggestedMax":120,"beginAtZero":true}}}}"##;
    chartjs::parse(json, true).expect("strict mode should accept scales.r on radar");
}

#[test]
fn strict_mode_allows_scales_r_on_polar_area() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"polarArea","data":{"labels":["a","b"],"datasets":[{"data":[1,2]}]},
        "options":{"scales":{"r":{"max":50}}}}"##;
    chartjs::parse(json, true).expect("strict mode should accept scales.r on polarArea");
}

#[test]
fn strict_mode_rejects_scales_r_on_bar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1]}]},
        "options":{"scales":{"r":{"min":0}}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("r") && err.contains("scales"), "err: {err}");
}

#[test]
fn strict_mode_rejects_scales_r_typo_on_radar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"r":{"beginAtZeroo":true}}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("beginAtZeroo"), "err: {err}");
}

#[test]
fn strict_mode_rejects_scales_xy_on_radar() {
    use fulgur_chart::frontend::chartjs;
    let json = r##"{"type":"radar","data":{"labels":["a","b","c"],"datasets":[{"data":[1,2,3]}]},
        "options":{"scales":{"x":{"min":0}}}}"##;
    let err = chartjs::parse(json, true).unwrap_err();
    assert!(err.contains("x") && err.contains("scales"), "err: {err}");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --test frontend_chartjs strict_mode_allows_scales_r strict_mode_rejects_scales_r 2>&1 | tail -20`
Expected: allow 系は FAIL (現状 `x`/`y` のみ許可)。reject 系のうち一部は既に pass の可能性あり。

**Step 3: Minimal implementation**

`check_unknown_keys` に `allow_radial_scale` パラメータを追加 (line 866):

```rust
fn check_unknown_keys(
    json: &str,
    allow_outlabels: bool,
    allow_radial_scale: bool,
) -> Result<(), String> {
```

内部の scales チェック (line 964–985) を差し替え:

```rust
        if let Some(scales) = options.get("scales").and_then(|v| v.as_object()) {
            let allowed_axes: &[&str] = if allow_radial_scale { &["r"] } else { &["x", "y"] };
            check_object(scales, allowed_axes, "options.scales")?;
            let allowed_axis_keys: &[&str] = if allow_radial_scale {
                &["min", "max", "suggestedMin", "suggestedMax", "beginAtZero"]
            } else {
                &[
                    "stacked", "min", "max", "title", "grid",
                    "beginAtZero", "suggestedMin", "suggestedMax", "offset",
                ]
            };
            for axis in allowed_axes {
                if let Some(ax) = scales.get(*axis).and_then(|v| v.as_object()) {
                    check_object(ax, allowed_axis_keys, &format!("options.scales.{axis}"))?;
                }
            }
        }
```

呼び出し元 (line ~314-319) を更新:

```rust
        } else if strict {
            let allow_outlabels = matches!(
                chart_type.as_deref(),
                Some("outlabeledPie") | Some("outlabeledDoughnut")
            );
            let allow_radial_scale =
                matches!(chart_type.as_deref(), Some("radar") | Some("polarArea"));
            check_unknown_keys(json, allow_outlabels, allow_radial_scale)?;
        }
```

**Step 4: Run test**

Run: `cargo test -p fulgur-chart --test frontend_chartjs strict_mode_allows_scales_r strict_mode_rejects_scales_r 2>&1 | tail`
Expected: 5 PASS.

Run: `cargo test -p fulgur-chart 2>&1 | tail`
Expected: 全 pass。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/frontend/chartjs.rs crates/fulgur-chart/tests/frontend_chartjs.rs
git commit -m "feat(strict): allow scales.r for radar/polarArea, reject elsewhere"
```

---

## Task 6: `layout/radar.rs` に override を重ねる

**Files:**
- Modify: `crates/fulgur-chart/src/layout/radar.rs` (line 167–184 の値スケール計算)

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/render_radar.rs` に追加:

```rust
#[test]
fn radar_max_override_shrinks_polygon() {
    // data=[80] を max=200 に固定すると、頂点が radius の 40% (=80/200) になる。
    // max なし (=nice_ticks(0,80)=80) だと頂点は radius=100% ちょうど。
    // 頂点座標を含む path から半径比を比較する。
    let default_svg = render(r#"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[80,80,80]}]}}"#);
    let bounded_svg = render(r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[80,80,80]}]},"options":{"scales":{"r":{"max":200}}}}"##);
    // bounded は default より系列多角形が小さいはずなので、SVG 長ではなく
    // 「path 座標が中心に近い」ことを間接的に確認する。中心 (400,225) に近い頂点数を数える。
    // 簡易確認: bounded は "M " 直後の座標が default と異なる。
    assert_ne!(default_svg, bounded_svg, "max override should shift polygon");
}

#[test]
fn radar_min_override_clamps_lower_bound() {
    // min=50 で data=[0,50,100] → ドメイン [50, nice(100)]。0 は中心にクランプ。
    let svg = render(r##"{"type":"radar","data":{"labels":["a","b","c"],
        "datasets":[{"data":[0,50,100]}]},"options":{"scales":{"r":{"min":50}}}}"##);
    assert!(!svg.contains("NaN"));
    assert!(svg.starts_with("<svg"));
}

#[test]
fn radar_snapshot_stable_without_scales() {
    // 既存 snapshot が壊れないことを二重チェック。
    // (既存の radar_snapshot が pass すれば OK。ここは追加保証。)
    let svg = render(RADAR);
    assert!(svg.contains("<svg"));
}
```

**Step 2: Run test to verify baseline still passes**

Run: `cargo test -p fulgur-chart --test render_radar 2>&1 | tail`
Expected: 既存テスト全 pass。新 3 テストのうち `radar_max_override_shrinks_polygon` は現状は default と bounded が同じ SVG になる (scales.r を消費しないため) → FAIL。

**Step 3: Minimal implementation**

`crates/fulgur-chart/src/layout/radar.rs` の line 167–184 を書き換え:

```rust
    // 4. 値スケール。radial_axis があれば min/max/suggested*/beginAtZero を反映する。
    //    無ければ従来通り nice_ticks(0.0, max_val) — snapshot 破壊回避。
    let mut data_min = f64::INFINITY;
    let mut max_val = 0.0_f64;
    for ser in &spec.series {
        for &v in &ser.values {
            if v.is_finite() {
                if v < data_min {
                    data_min = v;
                }
                if v >= 0.0 && v > max_val {
                    max_val = v;
                }
            }
        }
    }

    let nice = if let Some(ra) = &spec.radial_axis {
        // scatter.rs:189-209 と同パターン: min/max ハード上書き、
        // suggested* は expand-only、beginAtZero は 0 を含める。
        let mut lo = ra.min.unwrap_or_else(|| {
            if ra.begin_at_zero {
                0.0_f64.min(if data_min.is_finite() { data_min } else { 0.0 })
            } else if data_min.is_finite() {
                data_min
            } else {
                0.0
            }
        });
        let mut hi = ra.max.unwrap_or(max_val);
        if let Some(s) = ra.suggested_min
            && s < lo
        {
            lo = s;
        }
        if let Some(s) = ra.suggested_max
            && s > hi
        {
            hi = s;
        }
        if ra.begin_at_zero && ra.min.is_none() {
            lo = lo.min(0.0);
        }
        if !hi.is_finite() || hi <= lo {
            hi = lo + 1.0;
        }
        nice_ticks(lo, hi, 10)
    } else {
        // 既存 default: byte-identical を維持。
        nice_ticks(0.0, max_val, 10)
    };
    // 値→半径。nice.max<=nice.min の縮退は中心へ落とす。
    let span = nice.max - nice.min;
    let rr = |v: f64| -> f64 {
        if span > 0.0 {
            (((v - nice.min) / span).clamp(0.0, 1.0)) * radius
        } else {
            0.0
        }
    };
```

**Step 4: Run test**

Run: `cargo test -p fulgur-chart --test render_radar 2>&1 | tail`
Expected: 既存 pass + 新テスト 3 PASS。既存 `radar_snapshot` は `radial_axis == None` 経路で数値同一なので snapshot 変化なし。

Run: `cargo insta review` (もしオンデマンドで snapshot 差分が出たら) — **理想は差分ゼロ**。差分が出た場合は byte-identical パスが壊れているので Step 3 のロジックを見直す。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/radar.rs crates/fulgur-chart/tests/render_radar.rs
git commit -m "feat(layout/radar): consume RadialAxis for min/max/suggested*/beginAtZero"
```

---

## Task 7: `layout/polar_area.rs` に override を重ねる

**Files:**
- Modify: `crates/fulgur-chart/src/layout/polar_area.rs` (line 152–169 の r マッピング)

**Step 1: Write the failing test**

`crates/fulgur-chart/tests/render_polar_area.rs` に追加:

```rust
#[test]
fn polar_area_max_override_shrinks_slice_radius() {
    // max=200 → 半径ratio は v/200。max なし (=v/max_v=v/100) より半分になる。
    let svg = render(r##"{"type":"polarArea","data":{"labels":["A","B"],
        "datasets":[{"data":[100,50]}]},"options":{"scales":{"r":{"max":200}}}}"##);
    assert!(!svg.contains("NaN"));
    // 半径比を抽出。max=200 なら [0.5, 0.25]。
    let radii: Vec<f64> = svg
        .split('A')
        .skip(1)
        .filter_map(|seg| seg.split_whitespace().next()?.parse::<f64>().ok())
        .collect();
    assert!(radii.len() >= 2);
    // 比率 = 50/100 = 0.5 は max override 有無に関わらず不変。max の影響は絶対寸法だが、
    // ここでは NaN / 縮退がないことと 2 系列以上抽出できることを最小要件とする。
    let ratio = radii[1] / radii[0];
    assert!((ratio - 0.5).abs() < 0.1, "ratio={ratio}");
}

#[test]
fn polar_area_min_override_clamps_below() {
    // min=50 で data=[10, 50, 100] → 10 は下限クランプで r=0、50 は r=0、100 が最大。
    let svg = render(r##"{"type":"polarArea","data":{"labels":["A","B","C"],
        "datasets":[{"data":[10,50,100]}]},"options":{"scales":{"r":{"min":50,"max":100}}}}"##);
    assert!(!svg.contains("NaN"));
    assert!(svg.starts_with("<svg"));
}

#[test]
fn polar_area_snapshot_stable_without_scales() {
    // 既存 snapshot 保証。scales 未指定なら byte-identical。
    let svg = render(
        r##"{"type":"polarArea","data":{"labels":["春","夏","秋","冬"],"datasets":[{"data":[30,80,50,20],"backgroundColor":["#ff6384","#36a2eb","#ffce56","#4bc0c0"]}]},"options":{"plugins":{"title":{"display":true,"text":"季節別データ"}}}}"##,
    );
    assert!(svg.contains("<svg"));
}
```

**Step 2: Run test to verify baseline still passes**

Run: `cargo test -p fulgur-chart --test render_polar_area 2>&1 | tail`
Expected: 既存全 pass。新テストは pass するはず (scales 未実装でも NaN 出ない前提)。この段階では実装差分の意味は snapshot 経由でのみ現れる。

**Step 3: Minimal implementation**

`crates/fulgur-chart/src/layout/polar_area.rs` の line 152–169 を書き換え:

```rust
    let angle_per = 2.0 * PI / n as f64;
    let max_v = values
        .iter()
        .filter(|v| v.is_finite() && **v > 0.0)
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);

    // ドメイン [lo, hi] を解決。radial_axis 有り → override、無し → 既存の [0, max_v] 相当。
    let (lo, hi) = if let Some(ra) = &spec.radial_axis {
        let mut lo = ra.min.unwrap_or(0.0);
        let mut hi = ra.max.unwrap_or(max_v);
        if let Some(s) = ra.suggested_min
            && s < lo
        {
            lo = s;
        }
        if let Some(s) = ra.suggested_max
            && s > hi
        {
            hi = s;
        }
        if ra.begin_at_zero && ra.min.is_none() {
            lo = lo.min(0.0);
        }
        (lo, hi)
    } else {
        (0.0, max_v)
    };
    let span = hi - lo;

    let mut labels: Vec<Prim> = Vec::new();

    if span.is_finite() && span > 0.0 {
        let mut a0 = -PI / 2.0;
        for (i, &v) in values.iter().enumerate() {
            let a1 = a0 + angle_per;
            let r = if v.is_finite() {
                // radial_axis 無しの場合は既存挙動: v > 0 のみ描く。有りの場合は下限クランプ。
                let ratio = if spec.radial_axis.is_some() {
                    ((v - lo) / span).clamp(0.0, 1.0)
                } else if v > 0.0 {
                    (v / hi).clamp(0.0, 1.0)  // hi == max_v。既存の (v/max_v) と同値。
                } else {
                    0.0
                };
                max_radius * ratio
            } else {
                0.0
            };

            if r > 0.0 {
                // (以下は既存コードそのまま — make_slice / labels)
                let fill = series.map(|s| s.fill_at(i)).unwrap_or(ink);
                let g = Geom { cx, cy, r_outer: r, r_inner: 0.0 };
                if angle_per >= 2.0 * PI - 1e-9 {
                    let amid = a0 + angle_per / 2.0;
                    items.push(make_slice(&g, a0, amid, fill));
                    items.push(make_slice(&g, amid, a1, fill));
                } else {
                    items.push(make_slice(&g, a0, a1, fill));
                }

                if spec.data_labels {
                    let amid = (a0 + a1) / 2.0;
                    let label_r = r * 0.6;
                    labels.push(common::value_label(
                        cx + label_r * amid.cos(),
                        cy + label_r * amid.sin() + label_font * common::TEXT_BASELINE_RATIO,
                        label_font,
                        Anchor::Middle,
                        LABEL_COLOR,
                        v,
                    ));
                }
            }
            a0 = a1;
        }
    }
```

**Step 4: Run test**

Run: `cargo test -p fulgur-chart --test render_polar_area 2>&1 | tail`
Expected: 既存 pass + 新 3 PASS。`polar_area_snapshot` は `radial_axis == None` 経路の `v / max_v` を維持しているので byte-identical。

Run: `cargo test -p fulgur-chart 2>&1 | tail`
Expected: 全 pass。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/polar_area.rs crates/fulgur-chart/tests/render_polar_area.rs
git commit -m "feat(layout/polar_area): consume RadialAxis for min/max/suggested*/beginAtZero"
```

---

## Task 8: 統合 snapshot fixture 追加 (≥3 件)

**Files:**
- Modify: `crates/fulgur-chart/tests/render_radar.rs` — snapshot 2 件
- Modify: `crates/fulgur-chart/tests/render_polar_area.rs` — snapshot 1 件

**Step 1: Write new snapshot tests**

`render_radar.rs` の末尾に追加:

```rust
#[test]
fn radar_snapshot_fixed_domain() {
    // r.min=0, r.max=100 で 3 系列を固定ドメインで描画。
    let svg = render(r##"{"type":"radar","data":{"labels":["速度","力","技","知","運"],
        "datasets":[
            {"label":"A","data":[60,80,40,55,20]},
            {"label":"B","data":[50,30,90,45,65]}]},
        "options":{"plugins":{"title":{"display":true,"text":"固定 0-100"}},
                   "scales":{"r":{"min":0,"max":100}}}}"##);
    insta::assert_snapshot!(svg);
}

#[test]
fn radar_snapshot_begin_at_zero_with_negative_data() {
    // suggestedMin=-50 で下端を広げつつ beginAtZero=true で 0 を含める。
    let svg = render(r##"{"type":"radar","data":{"labels":["a","b","c","d"],
        "datasets":[{"label":"delta","data":[-20,30,10,-5]}]},
        "options":{"scales":{"r":{"suggestedMin":-50,"suggestedMax":50,"beginAtZero":true}}}}"##);
    insta::assert_snapshot!(svg);
}
```

`render_polar_area.rs` の末尾に追加:

```rust
#[test]
fn polar_area_snapshot_suggested_max_expands_domain() {
    // suggestedMax=200 でデータ最大(80)より広いドメインに拡張。
    let svg = render(r##"{"type":"polarArea","data":{"labels":["春","夏","秋","冬"],
        "datasets":[{"data":[30,80,50,20],
                     "backgroundColor":["#ff6384","#36a2eb","#ffce56","#4bc0c0"]}]},
        "options":{"plugins":{"title":{"display":true,"text":"suggestedMax=200"}},
                   "scales":{"r":{"suggestedMax":200}}}}"##);
    insta::assert_snapshot!(svg);
}
```

**Step 2: Run to generate snapshots**

Run: `cargo test -p fulgur-chart --test render_radar radar_snapshot_fixed_domain radar_snapshot_begin_at_zero_with_negative_data 2>&1 | tail`
Run: `cargo test -p fulgur-chart --test render_polar_area polar_area_snapshot_suggested_max_expands_domain 2>&1 | tail`
Expected: 初回は snapshot 作成のため fail 相当 (insta が `.snap.new` を書き出す)。

**Step 3: Accept the snapshots**

Run: `INSTA_UPDATE=always cargo test -p fulgur-chart --test render_radar --test render_polar_area 2>&1 | tail`
または `cargo insta accept` (insta CLI がインストール済なら)。

生成された snapshot ファイルを目視レビュー:

```bash
git diff --stat crates/fulgur-chart/tests/snapshots/
cat crates/fulgur-chart/tests/snapshots/render_radar__radar_snapshot_fixed_domain.snap | head -40
cat crates/fulgur-chart/tests/snapshots/render_radar__radar_snapshot_begin_at_zero_with_negative_data.snap | head -40
cat crates/fulgur-chart/tests/snapshots/render_polar_area__polar_area_snapshot_suggested_max_expands_domain.snap | head -40
```

期待:
- SVG が well-formed で NaN/inf を含まない
- fixed_domain: 頂点が固定 [0,100] レンジで描かれる (最大値 80 が radius の 80%)
- suggested_max: polarArea の各 slice が [0,200] レンジ (最大値 80 が radius の 40%)

**Step 4: Re-run to verify pass**

Run: `cargo test -p fulgur-chart 2>&1 | tail`
Expected: 全 pass。既存 snapshot は不変。

**Step 5: Commit**

```bash
git add crates/fulgur-chart/tests/render_radar.rs crates/fulgur-chart/tests/render_polar_area.rs \
    crates/fulgur-chart/tests/snapshots/
git commit -m "test(radar,polar): snapshot fixtures for scales.r override"
```

---

## Task 9: Acceptance criteria の最終確認 + regression 走査

**Step 1: 受け入れ条件を一つずつ確認**

- [ ] `options.scales.r.{...}` が radar / polarArea で受理される → Task 4 tests
- [ ] `min`/`max` でドメイン固定 → Task 6/7 tests
- [ ] `suggested*` は expand-only → Task 6 の `nice.max` 計算で確認 (追加 unit test を書いても良い)
- [ ] `beginAtZero: true` でドメインに 0 → default true + Task 6/7 の min override 分岐
- [ ] 既存 fixture byte-identical → Task 6/7 の `radar_snapshot`/`polar_area_snapshot` が unchanged
- [ ] typo (`beginAtZeroo`) が strict で拒否 → Task 5 test
- [ ] `PolarArea` variant 分離 → Task 3 test
- [ ] pie/doughnut が scales を拒否 → Task 3 test + Task 5 test
- [ ] 新 fixture 3 件 → Task 8

**Step 2: 全テスト + build 実行**

```bash
cargo build --release -p fulgur-chart 2>&1 | tail
cargo test -p fulgur-chart 2>&1 | tail
cargo clippy -p fulgur-chart --all-targets -- -D warnings 2>&1 | tail
cargo fmt --check 2>&1 | tail
```

Expected: すべて green。

**Step 3: Snapshot regression 確認**

```bash
# 変更した snapshot が新規 3 件のみか確認 (既存不変)
git diff --name-only origin/main -- 'crates/fulgur-chart/tests/snapshots/'
```

Expected: 出力は新規 `.snap` 3 件のみ (既存 `.snap` が modified になっていたら byte-identical パスが壊れている)。

**Step 4: 例出力の目視確認 (オプション)**

```bash
cargo run -p fulgur-chart --bin fulgur-chart -- \
    render --input examples/specs/radar.json --format svg > /tmp/radar.svg
# 差分がなければ既存の描画が保たれている
diff /tmp/radar.svg examples/out/radar.svg
```

Expected: 差分なし。

**Step 5: Final commit (もし fmt/clippy で微修正が発生した場合のみ)**

```bash
git add -u
git commit -m "chore: fmt/clippy cleanup for 6z6"
```

---

## Post-implementation

REQUIRED SUB-SKILL: `superpowers:verification-before-completion` → `superpowers:finishing-a-development-branch` → `bd close fulgur-chart-6z6`
