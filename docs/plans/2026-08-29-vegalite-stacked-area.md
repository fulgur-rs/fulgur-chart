# Vega-Lite Stacked Area Chart Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Support Vega-Lite `mark: "area"` (temporal and categorical), stacked by default when a `color` channel is present and de-stackable via `encoding.y.stack: null`, matching real Vega-Lite semantics.

**Architecture:** Area is a `ChartKind::Line`-family chart distinguished per-series by `Series.area = true` (already how chart.js `fill: true` line charts work). We add a `stacked: bool` field to `ChartKind::Line` (mirroring `ChartKind::Bar`'s `value_stacked`), reuse the existing stacked-bar `value_domain` positive/negative-sum logic for the y-axis domain, and add a cumulative-offset code path to `layout/line.rs` that computes per-category `(near, far)` running totals (separate running totals for positive and negative values, matching Vega-Lite's `stack: "zero"` behavior) and draws the stroke/markers at `far` and closes the area polygon at `near`. The Vega-Lite frontend gains an `"area"` mark and a `y.stack` reader; chart.js is untouched (`stacked` is always `false` there — existing goldens stay byte-identical).

**Tech Stack:** Rust, cargo test (snapshot + unit tests via `insta`/plain assert), serde_json, schemars.

**Design doc:** `docs/plans/2026-08-29-vegalite-stacked-area-design.md`
**beads:** fulgur-chart-boo

**Worktree:** `.worktrees/vegalite-stacked-area`, branch `feat/vegalite-stacked-area`. Baseline: 643+ tests passing / 0 failed (verified at worktree setup).

---

## Task 1: IR — add `stacked` to `ChartKind::Line`

This is a mechanical refactor: `ChartKind::Line` (unit variant) becomes `ChartKind::Line { stacked: bool }`. Every match site in the crate needs updating. `cargo build` will catch any site missed below.

**Files:**
- Modify: `src/ir.rs:345` (enum definition)
- Modify: `src/ir.rs:709` (test fixture construction)
- Modify: `src/layout/mod.rs:31`
- Modify: `src/frontend/chartjs.rs:631, 638, 736, 767, 802, 943`
- Modify: `src/frontend/vegalite.rs:53, 62, 67, 134, 412, 578`
- Modify: `src/model.rs:117, 192, 262, 419, 441, 505`
- Modify: `src/guard.rs:182, 260, 648, 698`
- Modify: `src/layout/common.rs:566, 723, 1020, 1101, 1472, 2624`
- Modify: `tests/frontend_vegalite.rs:1336`
- Modify: `tests/frontend_chartjs.rs:409`

**Step 1: Change the enum definition**

In `src/ir.rs` around line 345, replace:

```rust
    Line, // area/tension は Series 側
```

with:

```rust
    Line {
        /// 積み上げ area。Vega-Lite の mark:"area" + color channel で既定 true
        /// (encoding.y.stack: null で false)。chart.js フロントエンドは常に false を
        /// 設定する(bar と異なり chart.js 側の line/area stacked は未対応、fulgur-chart-nhb)。
        stacked: bool,
    }, // area/tension は Series 側
```

**Step 2: Fix every call site**

Two mechanical rules cover every site listed above:

- **Bare pattern** (`matches!(x, ChartKind::Line)`, a match arm `ChartKind::Line => ...`, or `ChartKind::Line` inside a `|`-chain or tuple pattern): add `{ .. }`, e.g. `ChartKind::Line { .. }`.
- **Construction** (`ChartKind::Line` used as a value, e.g. `kind: ChartKind::Line,` or `"line" => ChartKind::Line,`): use `ChartKind::Line { stacked: false }`.

Apply per file:

- `src/ir.rs:709` (construction, in `#[cfg(test)] mod radial_axis_tests`): `kind: ChartKind::Line,` → `kind: ChartKind::Line { stacked: false },`
- `src/layout/mod.rs:31` (pattern): `ChartKind::Line => line::build(spec, m),` → `ChartKind::Line { .. } => line::build(spec, m),`
- `src/frontend/chartjs.rs:631` (construction, `else if is_mixable_base && has_line && !has_bar { ChartKind::Line }`): → `ChartKind::Line { stacked: false }`
- `src/frontend/chartjs.rs:638` (construction): `"line" => ChartKind::Line,` → `"line" => ChartKind::Line { stacked: false },`
- `src/frontend/chartjs.rs:736` (pattern, inside `matches!(kind, crate::ir::ChartKind::Line | ...)`): `crate::ir::ChartKind::Line` → `crate::ir::ChartKind::Line { .. }`
- `src/frontend/chartjs.rs:767` (pattern): `!matches!(kind, ChartKind::Line)` → `!matches!(kind, ChartKind::Line { .. })`
- `src/frontend/chartjs.rs:802` (pattern, inside a `|`-chain): `} | ChartKind::Line` → `} | ChartKind::Line { .. }`
- `src/frontend/chartjs.rs:943` (pattern): `matches!(kind, ChartKind::Line)` → `matches!(kind, ChartKind::Line { .. })`
- `src/frontend/vegalite.rs:53, 62, 134` (pattern): `matches!(kind, ChartKind::Line)` → `matches!(kind, ChartKind::Line { .. })`
- `src/frontend/vegalite.rs:67` (pattern): `ChartKind::Bar { .. } | ChartKind::Line => {` → `ChartKind::Bar { .. } | ChartKind::Line { .. } => {`
- `src/frontend/vegalite.rs:412` (construction): `"line" => Ok(ChartKind::Line),` → `"line" => Ok(ChartKind::Line { stacked: false }),`
- `src/frontend/vegalite.rs:578` (pattern): `ChartKind::Line => SeriesType::Line,` → `ChartKind::Line { .. } => SeriesType::Line,`
- `src/model.rs:117` (pattern): `matches!(spec.kind, ChartKind::Line)` → `matches!(spec.kind, ChartKind::Line { .. })`
- `src/model.rs:192` (pattern, match arm): `ChartKind::Line => {` → `ChartKind::Line { .. } => {`
- `src/model.rs:262` (pattern, match arm): `ChartKind::Line => "line",` → `ChartKind::Line { .. } => "line",`
- `src/model.rs:419` (pattern, tuple): `if let (ChartKind::Line, XPositions::Temporal { unix_millis }) = ...` → `(ChartKind::Line { .. }, XPositions::Temporal { unix_millis })`
- `src/model.rs:441` (pattern, `|`-chain): `| ChartKind::Line` → `| ChartKind::Line { .. }`
- `src/model.rs:505` (pattern, tuple): `(ChartKind::Line, XPositions::Temporal { .. })` → `(ChartKind::Line { .. }, XPositions::Temporal { .. })`
- `src/guard.rs:182, 260, 698` (pattern): `matches!(spec.kind, ChartKind::Line)` → `matches!(spec.kind, ChartKind::Line { .. })`
- `src/guard.rs:648` (pattern, match arm): `ChartKind::Line => {` → `ChartKind::Line { .. } => {`
- `src/layout/common.rs:566, 723, 1020, 1101` (pattern): `matches!(spec.kind, ChartKind::Line)` → `matches!(spec.kind, ChartKind::Line { .. })`
- `src/layout/common.rs:1472, 2624` (construction): `spec.kind = ChartKind::Line;` → `spec.kind = ChartKind::Line { stacked: false };`
- `tests/frontend_vegalite.rs:1336`, `tests/frontend_chartjs.rs:409` (pattern): `assert!(matches!(spec.kind, ChartKind::Line));` → `assert!(matches!(spec.kind, ChartKind::Line { .. }));`

Leave `src/frontend/chartjs.rs:3950` alone — it's a comment, not code.

**Step 3: Build and fix any remaining compile errors**

Run: `cargo build -p fulgur-chart --all-targets 2>&1 | grep -E "^error"`

Expected: empty output. If the compiler surfaces additional sites not listed above, apply the same two rules (bare pattern → add `{ .. }`; construction → add `{ stacked: false }`) — this list was gathered by `grep -rn "ChartKind::Line" src/ tests/` and should be exhaustive, but the compiler is the source of truth.

**Step 4: Run the full test suite**

Run: `cargo test -p fulgur-chart 2>&1 | grep -E "test result:|FAILED"`

Expected: every `test result:` line reads `ok. ... 0 failed`, identical counts to the baseline. This is a pure refactor — no golden/snapshot output should change.

**Step 5: Clippy**

Run: `cargo clippy -p fulgur-chart --all-targets 2>&1 | grep -E "warning|error"`

Expected: no new warnings.

**Step 6: Commit**

```bash
git add -A
git commit -m "refactor(ir): add stacked field to ChartKind::Line

Mechanical variant-shape change (ChartKind::Line -> Line { stacked: bool }),
mirroring ChartKind::Bar's value_stacked. chart.js frontend keeps stacked
always false; no rendering behavior changes in this commit."
```

---

## Task 2: `value_domain` — stacked y-domain for `ChartKind::Line`

**Files:**
- Modify: `src/layout/common.rs:254-260` (the `value_domain` stacked-branch condition)
- Test: `src/layout/common.rs` (`mod tests`, near the existing `make_bar_spec` helper)

**Step 1: Write the failing test**

Add to `src/layout/common.rs`'s `mod tests` (near `make_bar_spec`):

```rust
#[test]
fn value_domain_sums_stacked_line_series_independently_by_sign() {
    let mut spec = make_bar_spec(2, 720.0);
    spec.kind = ChartKind::Line { stacked: true };
    spec.series = vec![
        Series {
            name: "a".to_string(),
            values: vec![10.0, 20.0],
            points: Vec::<Point>::new(),
            fill: vec![crate::palette::PALETTE[0]],
            stroke: vec![crate::palette::PALETTE[0]],
            stroke_width: 2.0,
            area: true,
            interpolation: LineInterpolation::Linear,
            span_gaps: false,
            step_mode: None,
            series_type: SeriesType::Line,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
        },
        Series {
            name: "b".to_string(),
            values: vec![5.0, -8.0],
            points: Vec::<Point>::new(),
            fill: vec![crate::palette::PALETTE[1]],
            stroke: vec![crate::palette::PALETTE[1]],
            stroke_width: 2.0,
            area: true,
            interpolation: LineInterpolation::Linear,
            span_gaps: false,
            step_mode: None,
            series_type: SeriesType::Line,
            point_radius: None,
            box_points: vec![],
            tree: vec![],
            links: vec![],
        },
    ];
    let (lo, hi) = value_domain(&spec, &spec.y_axis);
    // cat0: 10+5=15 positive-only -> hi covers 15; cat1: 20 positive, -8 negative independently.
    assert_eq!(hi, 20.0);
    assert_eq!(lo, -8.0);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p fulgur-chart --lib value_domain_sums_stacked_line_series_independently_by_sign -- --nocapture`

Expected: FAIL. With `stacked: true` but the domain code not yet recognizing it, the branch falls through to the plain min/max-of-individual-values path, giving `hi == 20.0` (coincidentally right) but `lo == -8.0` (also coincidentally right since begin_at_zero defaults true and 0.min(-8)=-8) — **if this test doesn't fail as written, strengthen it**: add a third series so the stacked positive sum at some category exceeds any individual value, e.g. add `values: vec![50.0, 0.0]` to series "a" isn't enough since chart.js/plain path would also report max individual as 50. Use category 0 instead: series values `a=[10.0, 20.0]`, `b=[5.0, -8.0]`, `c=[8.0, 0.0]` (add a third series) so cat0 positive sum = 10+5+8=23 while max individual value is 20 — assert `hi == 23.0`, which the non-stacked (individual-max) path would report as `20.0`, giving a real red before Step 3.

**Step 3: Implement**

In `src/layout/common.rs`, change the condition at the top of `value_domain` (around line 254):

```rust
    if matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        }
    ) {
```

to:

```rust
    // Line の stacked area は Bar の value_stacked と同じ「カテゴリごと正負サム独立集計」
    // ロジックを共有する。ここでの積み上げは常に線形軸前提(VL は log y 軸を公開しておらず、
    // chart.js は stacked を常に false にするため、対数軸との組み合わせは到達不能)。
    if matches!(
        spec.kind,
        crate::ir::ChartKind::Bar {
            value_stacked: true,
            ..
        } | crate::ir::ChartKind::Line { stacked: true }
    ) {
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p fulgur-chart --lib value_domain_sums_stacked_line_series_independently_by_sign -- --nocapture`

Expected: PASS.

**Step 5: Run full suite + clippy**

Run: `cargo test -p fulgur-chart 2>&1 | grep -E "test result:|FAILED"` — expected all green, same counts as Task 1 plus one new test.
Run: `cargo clippy -p fulgur-chart --all-targets 2>&1 | grep -E "warning|error"` — expected clean.

**Step 6: Commit**

```bash
git add src/layout/common.rs
git commit -m "feat(layout): stacked y-domain for ChartKind::Line

Reuses the stacked-bar value_domain branch (per-category positive/negative
sum, independent by sign) for ChartKind::Line { stacked: true }."
```

---

## Task 3: `layout/line.rs` — stacked area geometry

This is the core rendering change. Per §1 of the advisor review baked into this plan:
- Stroke/markers are drawn at each series' **far** offset (cumulative total through and including this series, same-signed running total).
- The area polygon closes against each series' **near** offset (cumulative total *before* this series, on the same side of zero).
- The stacked path **must not** decimate or gap-split: `decimate::resolve`'s threshold is `plot_width_px * 4.0` (default `Decimation::default()` has `enabled: true`, `threshold: None`, so it auto-triggers on point count alone — see `src/layout/decimate.rs:111-127`). Decimating each series independently would desync the bands (series A's surviving x-positions wouldn't match series B's), corrupting the stack. The Vega-Lite frontend never sets `span_gaps`/`step_mode` and (per the design doc's verified precondition) always produces a dense value at every category for every series when `color` is used, so the stacked path can safely assume one full-length segment with no gaps.

**Files:**
- Modify: `src/layout/line.rs` (add `stack_offsets` helper, `build()`, `line_points()`)
- Test: `src/layout/line.rs` (`mod tests`)

**Step 1: Write the failing tests**

Add a spec-builder helper and three tests to `src/layout/line.rs`'s `mod tests` (it currently only has `pts_for`/`scene_for` which go through `chartjs::parse`, and chart.js can never produce `stacked: true` — write a direct `ChartSpec` builder instead):

```rust
fn stacked_area_spec(categories: Vec<&str>, series: Vec<(&str, Vec<f64>)>) -> ChartSpec {
    use crate::ir::{
        AxisBorder, AxisGrid, AxisSpec, Decimation, LegendPos, Point, ScaleKind, SizeMode, Theme,
        XPositions,
    };
    let palette = crate::palette::PALETTE.to_vec();
    let axis = AxisSpec {
        title: None,
        min: None,
        max: None,
        suggested_min: None,
        suggested_max: None,
        begin_at_zero: true,
        offset: false,
        grid: AxisGrid::default(),
        border: AxisBorder::default(),
        scale_kind: ScaleKind::Linear,
    };
    ChartSpec {
        kind: ChartKind::Line { stacked: true },
        categories: categories.into_iter().map(str::to_string).collect(),
        x_positions: XPositions::Category,
        series: series
            .into_iter()
            .enumerate()
            .map(|(i, (name, values))| {
                let color = palette[i % palette.len()];
                crate::ir::Series {
                    name: name.to_string(),
                    values,
                    points: Vec::<Point>::new(),
                    fill: vec![color],
                    stroke: vec![color],
                    stroke_width: 2.0,
                    area: true,
                    interpolation: crate::ir::LineInterpolation::Linear,
                    span_gaps: false,
                    step_mode: None,
                    series_type: crate::ir::SeriesType::Line,
                    point_radius: None,
                    box_points: vec![],
                    tree: vec![],
                    links: vec![],
                }
            })
            .collect(),
        x_axis: axis.clone(),
        y_axis: axis,
        legend: LegendPos::None,
        legend_title: None,
        title: None,
        width: 720.0,
        height: 400.0,
        size_mode: SizeMode::Canvas,
        data_labels: false,
        theme: Theme::default(),
        decimation: Decimation::default(),
        radial_axis: None,
    }
}

#[test]
fn stacked_area_bands_are_contiguous() {
    let spec = stacked_area_spec(
        vec!["a", "b"],
        vec![("s0", vec![10.0, 20.0]), ("s1", vec![5.0, 15.0])],
    );
    let frame = common::compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
    let scene = build(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
    let markers: Vec<(f64, f64)> = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Circle { cx, cy, .. } => Some((*cx, *cy)),
            _ => None,
        })
        .collect();
    // s0 at cat "a": far = 10 (bottom band). s1 at cat "a": near = 10, far = 15 (top band).
    let s0_a_y = frame.ys.map(10.0);
    let s1_a_y = frame.ys.map(15.0);
    assert!(
        markers.iter().any(|&(_, y)| (y - s0_a_y).abs() < 1e-6),
        "series 0 marker must sit at its cumulative top (10)"
    );
    assert!(
        markers.iter().any(|&(_, y)| (y - s1_a_y).abs() < 1e-6),
        "series 1 marker must sit at its cumulative top (10+5=15)"
    );
}

#[test]
fn stacked_area_top_band_stays_within_plot_bounds() {
    let spec = stacked_area_spec(
        vec!["a", "b", "c"],
        vec![("s0", vec![10.0, 20.0, 30.0]), ("s1", vec![5.0, 15.0, 25.0])],
    );
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = common::compute(&spec, &m);
    let scene = build(&spec, &m);
    let top_ys: Vec<f64> = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Circle { cy, .. } => Some(*cy),
            _ => None,
        })
        .collect();
    for y in top_ys {
        assert!(
            y >= frame.plot_top - 1e-6 && y <= frame.plot_bottom + 1e-6,
            "marker y={y} escaped plot bounds [{}, {}]",
            frame.plot_top,
            frame.plot_bottom
        );
    }
}

#[test]
fn stacked_area_skips_decimation_above_threshold() {
    // plot_width for an 800px-wide chart is well under 800, so the default decimation
    // threshold (plot_width_px * 4.0) is comfortably under 3200. Use 4000 categories,
    // two series, to force src/layout/decimate.rs::resolve to trigger for a *naive*
    // per-series-independent decimation path, then assert the stack stays aligned
    // everywhere (not just at a hand-picked few indices).
    let n = 4000;
    let categories: Vec<String> = (0..n).map(|i| format!("c{i}")).collect();
    let s0_values: Vec<f64> = (0..n).map(|i| (i % 7) as f64 + 1.0).collect();
    let s1_values: Vec<f64> = (0..n).map(|i| (i % 5) as f64 + 1.0).collect();
    let categories_ref: Vec<&str> = categories.iter().map(String::as_str).collect();
    let spec = stacked_area_spec(
        categories_ref,
        vec![("s0", s0_values.clone()), ("s1", s1_values.clone())],
    );
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let frame = common::compute(&spec, &m);
    let scene = build(&spec, &m);
    let mut markers: Vec<(f64, f64)> = scene
        .items
        .iter()
        .filter_map(|item| match item {
            Prim::Circle { cx, cy, .. } => Some((*cx, *cy)),
            _ => None,
        })
        .collect();
    assert_eq!(
        markers.len(),
        n * 2,
        "stacked area must not decimate away any marker (would desync the bands)"
    );
    markers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for i in 0..n {
        let expected_s0_far = frame.ys.map(s0_values[i]);
        let expected_s1_far = frame.ys.map(s0_values[i] + s1_values[i]);
        let got: Vec<f64> = markers[i * 2..i * 2 + 2].iter().map(|&(_, y)| y).collect();
        assert!(
            got.iter().any(|&y| (y - expected_s0_far).abs() < 1e-6),
            "category {i}: s0 far offset missing (decimation likely desynced the stack)"
        );
        assert!(
            got.iter().any(|&y| (y - expected_s1_far).abs() < 1e-6),
            "category {i}: s1 far offset missing"
        );
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p fulgur-chart --lib stacked_area -- --nocapture`

Expected: FAIL (or does not compile, since `stacked: true` support doesn't exist yet in `build()` — the geometry still treats every series independently from a fixed 0 baseline).

**Step 3: Implement `stack_offsets` and wire it into `build()`/`line_points()`**

Add above `pub fn build` in `src/layout/line.rs`:

```rust
/// 積み上げ area のカテゴリ・系列ごとの (near, far) オフセットを計算する。
/// 正値は正側の running total、負値は負側の running total に独立で積む
/// (Vega-Lite の stack:"zero" と同じ; `value_domain` の正負サム分離と対応する)。
/// near = この系列を足す前の running total(baseline 側/隣接帯との共有辺)、
/// far = 足した後の running total(stroke/marker を置く辺)。非有限値は 0 として扱う
/// (積み上げ上の欠損補完; Vega-Lite の stack transform と同じ)。
fn stack_offsets(spec: &ChartSpec) -> Vec<Vec<(f64, f64)>> {
    let n = spec.categories.len();
    let mut pos_running = vec![0.0_f64; n];
    let mut neg_running = vec![0.0_f64; n];
    spec.series
        .iter()
        .map(|ser| {
            (0..n)
                .map(|i| {
                    let v = ser
                        .values
                        .get(i)
                        .copied()
                        .filter(|v| v.is_finite())
                        .unwrap_or(0.0);
                    let running = if v >= 0.0 {
                        &mut pos_running[i]
                    } else {
                        &mut neg_running[i]
                    };
                    let near = *running;
                    *running += v;
                    (near, *running)
                })
                .collect()
        })
        .collect()
}
```

Update `line_points()` (for model/geometry-introspection consistency with the rendered output) — add near the top, after `let mut pts = Vec::new();`:

```rust
    let stacked = matches!(spec.kind, ChartKind::Line { stacked: true });
    let offsets = stacked.then(|| stack_offsets(spec));
```

then inside the loop, replace the marker-value lookup:

```rust
        for i in 0..spec.categories.len() {
            let Some(&v) = ser.values.get(i) else {
                continue;
            };
            if !v.is_finite() {
                continue;
            }
            if is_log && v == 0.0 {
                continue;
            }
            let x = common::line_x(spec, frame, i);
            pts.push(crate::layout::scatter::PointBox {
                series: sidx,
                index: i,
                kind: "line",
                cx: x,
                cy: frame.ys.map(v),
                r: MARKER_R,
            });
        }
```

with:

```rust
        for i in 0..spec.categories.len() {
            let Some(&v) = ser.values.get(i) else {
                continue;
            };
            if !v.is_finite() {
                continue;
            }
            if is_log && v == 0.0 {
                continue;
            }
            let plot_y = if let Some(offsets) = &offsets {
                offsets[sidx][i].1 // far
            } else {
                v
            };
            let x = common::line_x(spec, frame, i);
            pts.push(crate::layout::scatter::PointBox {
                series: sidx,
                index: i,
                kind: "line",
                cx: x,
                cy: frame.ys.map(plot_y),
                r: MARKER_R,
            });
        }
```

Now update `build()`. Add right after `let is_log = ...;`:

```rust
    let stacked = matches!(spec.kind, ChartKind::Line { stacked: true });
    // 積み上げは常に密なデータ前提(色分け系列は必ず全カテゴリで値を持つ; VL フロントエンドが
    // build_categorical/build_temporal_line で保証する)なので gap 分割・間引きを行わない。
    // 複数系列を独立に間引くと x 位置がずれてスタックが破綻するため意図的にスキップする。
    let offsets = stacked.then(|| stack_offsets(spec));
```

Change `for ser in &spec.series {` to `for (si, ser) in spec.series.iter().enumerate() {`.

Replace the `valid` computation:

```rust
        let valid: Vec<(f64, f64, usize)> = (0..spec.categories.len())
            .filter_map(|i| {
                let v = ser.values.get(i).copied()?;
                if !v.is_finite() {
                    return None;
                }
                if is_log && v == 0.0 {
                    return None;
                }
                let x = common::line_x(spec, &frame, i);
                Some((x, frame.ys.map(v), i))
            })
            .collect();
```

with:

```rust
        let valid: Vec<(f64, f64, usize)> = (0..spec.categories.len())
            .filter_map(|i| {
                let x = common::line_x(spec, &frame, i);
                if let Some(offsets) = &offsets {
                    Some((x, frame.ys.map(offsets[si][i].1), i))
                } else {
                    let v = ser.values.get(i).copied()?;
                    if !v.is_finite() {
                        return None;
                    }
                    if is_log && v == 0.0 {
                        return None;
                    }
                    Some((x, frame.ys.map(v), i))
                }
            })
            .collect();
```

Replace the area-fill polygon closing (inside `if ser.area { ... for seg in &segments { ... } }`):

```rust
                write!(
                    d,
                    "L {} {} L {} {} Z",
                    fmt_num(last_x),
                    fmt_num(baseline_y),
                    fmt_num(first_x),
                    fmt_num(baseline_y)
                )
                .unwrap();
```

with:

```rust
                if let Some(offsets) = &offsets {
                    for &(_, _, cat) in seg.iter().rev() {
                        let near_x = common::line_x(spec, &frame, cat);
                        let near_y = frame.ys.map(offsets[si][cat].0);
                        write!(d, "L {} {} ", fmt_num(near_x), fmt_num(near_y)).unwrap();
                    }
                    write!(d, "Z").unwrap();
                } else {
                    write!(
                        d,
                        "L {} {} L {} {} Z",
                        fmt_num(last_x),
                        fmt_num(baseline_y),
                        fmt_num(first_x),
                        fmt_num(baseline_y)
                    )
                    .unwrap();
                }
```

`baseline_y` is still computed unconditionally above the loop (`let baseline_y = frame.ys.map(0.0_f64.clamp(...));`) — leave it, it's simply unused when `offsets.is_some()` on that call path (still read by the `else` branch, so no unused-variable warning). `first_x`/`last_x` remain used by the `else` branch too.

Everything else in `build()` (stroke drawing, marker drawing, data-label drawing) reads `segments`/`valid`, which already carry the correct `far`-offset y positions when stacked — no further changes needed there.

**Step 4: Run tests to verify they pass**

Run: `cargo test -p fulgur-chart --lib stacked_area -- --nocapture`

Expected: PASS, all four new tests (`stacked_area_bands_are_contiguous`, `stacked_area_top_band_stays_within_plot_bounds`, `stacked_area_skips_decimation_above_threshold`, and the Task 2 domain test already committed).

**Step 5: Run full suite + clippy**

Run: `cargo test -p fulgur-chart 2>&1 | grep -E "test result:|FAILED"` — all green, non-stacked line/area goldens byte-identical (this is a pure additive branch keyed on `stacked`, `stacked` is always `false` outside the new tests).
Run: `cargo clippy -p fulgur-chart --all-targets 2>&1 | grep -E "warning|error"` — clean.

**Step 6: Commit**

```bash
git add src/layout/line.rs
git commit -m "feat(layout): stacked area geometry (line.rs)

Adds stack_offsets() (per-category (near, far) running totals, positive
and negative sides tracked independently). build() draws stroke/markers
at far, closes the area polygon at near. Stacked charts skip decimation
and gap-splitting (verified dense by the VL frontend), since decimating
series independently would desync the bands."
```

---

## Task 4: Schema — `MarkArea*`, `y.stack`, `VlTemporalAreaSpec`/`VlCategoricalAreaSpec`

`schema/vegalite.rs` types are exercised directly via `serde_json::from_str::<VegaLiteSpec>(...)` in `tests/frontend_vegalite.rs` (cross-checked against the hand-written strict parser — see Task 5), not just used for schemars generation. `stack`'s three states (absent / `"zero"` / `null`) only need to be **accepted or rejected** correctly here — nothing in the codebase reads a deserialized `VegaLiteSpec` value back out, so a plain `Option<T>` (which naturally collapses `null` and absent to `None`, and accepts `"zero"` as `Some(Zero)`) is sufficient; no custom double-`Option` deserializer needed.

**Files:**
- Modify: `src/schema/vegalite.rs`
- Test: `src/schema/vegalite.rs` (inline unit tests, if this file has any) or `tests/frontend_vegalite.rs` (see Task 5 for the cross-check tests — schema-only accept/reject tests can live here too)

**Step 1: Add `MarkArea*` types**

Add after the existing `MarkRect`/before `VlBarSpec` (or anywhere in the "Mark constant types" section), mirroring `MarkLine*`:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkAreaName {
    Area,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkAreaObject {
    #[serde(rename = "type")]
    pub mark_type: MarkAreaName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolate: Option<VlLineInterpolation>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkArea {
    String(MarkAreaName),
    Object(MarkAreaObject),
}
```

(No `point` field — area+point overlay is explicitly out of scope per the design doc.)

**Step 2: Add the `y.stack` type**

Add near `VlQuantitativeType`:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlStackMode {
    Zero,
}
```

**Step 3: Add temporal area spec**

Mirror `VlTemporalLineSpec`/`VlTemporalLineEncoding`/`VlTemporalYChannel`, but with a y-channel carrying `stack`:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalAreaSpec {
    pub mark: MarkArea,
    pub data: VlData,
    pub encoding: VlTemporalAreaEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<VlConfig>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalAreaEncoding {
    pub x: VlTemporalXChannel,
    pub y: VlTemporalAreaYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlTemporalColorChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlTemporalAreaYChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<VlStackMode>,
}
```

**Step 4: Add categorical area spec**

Mirror `VlCategoricalLineSpec`/`VlCategoricalLineEncoding`, y-channel with `stack`:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalAreaSpec {
    pub mark: MarkArea,
    pub data: VlData,
    pub encoding: VlCategoricalAreaEncoding,
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<VlTitle>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalAreaEncoding {
    pub x: VlCategoricalXChannel,
    pub y: VlCategoricalAreaYChannel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<VlCategoricalColorChannel>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlCategoricalAreaYChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<VlQuantitativeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<VlStackMode>,
}
```

**Step 5: Register in `VegaLiteSpec`**

```rust
pub enum VegaLiteSpec {
    Bar(VlBarSpec),
    TemporalLine(VlTemporalLineSpec),
    CategoricalLine(VlCategoricalLineSpec),
    TemporalArea(VlTemporalAreaSpec),
    CategoricalArea(VlCategoricalAreaSpec),
    Point(VlPointSpec),
    Circle(VlCircleSpec),
    Arc(VlArcSpec),
    Rect(VlRectSpec),
}
```

**Step 6: Write schema-only accept/reject tests**

Add to `tests/frontend_vegalite.rs` (near the other `VegaLiteSpec`-deserializing tests):

```rust
#[test]
fn typed_area_schema_accepts_stack_null_and_zero_and_absent() {
    let base = |stack: &str| {
        format!(
            r#"{{"mark":"area","data":{{"values":[{{"x":"a","y":1,"g":"p"}}]}},
               "encoding":{{"x":{{"field":"x","type":"nominal"}},
               "y":{{"field":"y","type":"quantitative"{stack}}},
               "color":{{"field":"g","type":"nominal"}}}}}}"#
        )
    };
    for stack in ["", r#","stack":null"#, r#","stack":"zero""#] {
        assert!(
            serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&base(stack)).is_ok(),
            "stack variant {stack:?} should be accepted"
        );
    }
    assert!(
        serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&base(r#","stack":"center""#))
            .is_err(),
        "unsupported stack mode must be rejected"
    );
}
```

**Step 7: Run tests**

Run: `cargo test -p fulgur-chart typed_area_schema -- --nocapture` — expected PASS.
Run: `cargo build -p fulgur-chart 2>&1 | grep -E "^error"` — expected empty (schemars derive must succeed on the new types).

**Step 8: Commit**

```bash
git add src/schema/vegalite.rs tests/frontend_vegalite.rs
git commit -m "feat(schema): Vega-Lite area mark types (temporal + categorical)

MarkArea{Name,Object} mirrors MarkLine* minus the point sub-option
(out of scope). y.stack: Option<VlStackMode> accepts absent/null/"zero" -
nothing deserializes these types back into runtime values (parse_with_limits
reads raw serde_json::Value directly), so accept/reject correctness is all
that's required."
```

---

## Task 5: Frontend — wire `"area"` into `frontend/vegalite.rs`

**Files:**
- Modify: `src/frontend/vegalite.rs`
- Test: `tests/frontend_vegalite.rs`

**Step 1: Write failing frontend unit tests**

Add to `tests/frontend_vegalite.rs`:

```rust
const CATEGORICAL_AREA_STACKED: &str = r#"{
    "mark": "area",
    "data": {"values": [
        {"month": "Jan", "kind": "A", "sales": 10},
        {"month": "Jan", "kind": "B", "sales": 5},
        {"month": "Feb", "kind": "A", "sales": 20},
        {"month": "Feb", "kind": "B", "sales": 15}
    ]},
    "encoding": {
        "x": {"field": "month", "type": "ordinal"},
        "y": {"field": "sales", "type": "quantitative"},
        "color": {"field": "kind", "type": "nominal"}
    }
}"#;

#[test]
fn area_with_color_defaults_to_stacked() {
    let spec = vegalite::parse(CATEGORICAL_AREA_STACKED, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line { stacked: true }));
    assert!(spec.series.iter().all(|s| s.area));
}

#[test]
fn area_stack_null_disables_stacking() {
    let json = CATEGORICAL_AREA_STACKED.replace(
        r#""y": {"field": "sales", "type": "quantitative"}"#,
        r#""y": {"field": "sales", "type": "quantitative", "stack": null}"#,
    );
    let spec = vegalite::parse(&json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line { stacked: false }));
}

#[test]
fn area_without_color_is_never_stacked() {
    let json = r#"{
        "mark": "area",
        "data": {"values": [{"x":"a","y":1},{"x":"b","y":2}]},
        "encoding": {
            "x": {"field": "x", "type": "nominal"},
            "y": {"field": "y", "type": "quantitative"}
        }
    }"#;
    let spec = vegalite::parse(json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line { stacked: false }));
    assert_eq!(spec.series.len(), 1);
    assert!(spec.series[0].area);
}

#[test]
fn area_stack_zero_is_explicit_stacked() {
    let json = CATEGORICAL_AREA_STACKED.replace(
        r#""y": {"field": "sales", "type": "quantitative"}"#,
        r#""y": {"field": "sales", "type": "quantitative", "stack": "zero"}"#,
    );
    let spec = vegalite::parse(&json, false).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line { stacked: true }));
}

#[test]
fn single_series_area_matches_line_geometry_besides_fill() {
    // area (no color) must produce the same series shape as an equivalent line mark,
    // differing only in Series.area.
    let area_json = r#"{"mark":"area","data":{"values":[{"x":"a","y":3},{"x":"b","y":7}]},
        "encoding":{"x":{"field":"x","type":"nominal"},"y":{"field":"y","type":"quantitative"}}}"#;
    let line_json = area_json.replace(r#""mark":"area""#, r#""mark":"line""#);
    let area_spec = vegalite::parse(area_json, false).unwrap();
    let line_spec = vegalite::parse(line_json.as_str(), false).unwrap();
    assert!(area_spec.series[0].area);
    assert!(!line_spec.series[0].area);
    assert_eq!(area_spec.series[0].values, line_spec.series[0].values);
}
```

**Step 2: Run to verify failure**

Run: `cargo test -p fulgur-chart --test frontend_vegalite area_ -- --nocapture`

Expected: FAIL — `parse_mark` doesn't recognize `"area"` yet (`mark がありません` or an untyped-mark error).

**Step 3: Implement**

In `src/frontend/vegalite.rs`:

**3a. `parse_mark`** — add an `"area"` arm right after the `"line"` arm (around line 412):

```rust
        "line" => Ok(ChartKind::Line { stacked: false }),
        "area" => Ok(ChartKind::Line { stacked: false }),
```

**3b. Compute the `stacked` flag and area flag once `encoding`/`color_field` are known.** Right after `let color_field = channel_field(encoding, "color");` (around line 59), add:

```rust
    let is_area = read_mark_name(top) == Some("area");
    if is_area {
        let stacked = color_field.is_some() && !y_stack_disabled(encoding);
        kind = ChartKind::Line { stacked };
    }
```

Add the helper near `channel_field`/`channel_type`:

```rust
/// encoding.y.stack が JSON null か(= 明示的に積み上げ解除)。省略/"zero" は false
/// (積み上げ既定を維持)。
fn y_stack_disabled(encoding: &Map<String, Value>) -> bool {
    encoding
        .get("y")
        .and_then(Value::as_object)
        .and_then(|y| y.get("stack"))
        .is_some_and(Value::is_null)
}
```

**3c. Mark series as area.** Right after the block that produces `let (series, categories, temporal_x_domain) = ...` (around line 277), change the binding to `let (mut series, categories, temporal_x_domain) = ...` and add:

```rust
    if is_area {
        for s in &mut series {
            s.area = true;
        }
    }
```

**Step 4: Run to verify pass**

Run: `cargo test -p fulgur-chart --test frontend_vegalite area_ -- --nocapture` and `cargo test -p fulgur-chart --test frontend_vegalite single_series_area -- --nocapture`

Expected: PASS.

**Step 5: Strict-mode (`--strict`) support**

Scope decision (documented here, not just in code): area's `--strict` validation covers structural correctness (unknown top-level keys, unknown encoding-channel keys, `y.stack`'s value) but does **not** replicate `check_line_keys`'s full temporal-vs-categorical fine-grained restrictions (categorical-only rejection of `mark.interpolate`, `encoding.{x,y}.title`, `encoding.color.{title,scale}`, `background`/`config`). Those exist for line because `build_categorical` silently ignores them; the same silent-ignore applies to area's categorical path (it shares `build_categorical`), so this is a known, pre-existing-style gap, not a new regression — file a follow-up beads issue in Task 6 rather than generalizing `check_line_keys` (a shared validator would need per-mark deltas in half a dozen places for two mark allowlists that already diverge — not worth it for the acceptance criteria here per the design review).

In `src/frontend/vegalite.rs`'s `check_unknown_keys`:

**5a.** Add `"area"` to the top-level allowed-keys arm (around line 1425), mirroring line's list:

```rust
    let top_allowed: &[&str] = match read_mark_name(top) {
        Some("line") => &[
            "mark",
            "data",
            "encoding",
            "$schema",
            "width",
            "height",
            "title",
            "background",
            "config",
        ],
        Some("area") => &[
            "mark",
            "data",
            "encoding",
            "$schema",
            "width",
            "height",
            "title",
            "background",
            "config",
        ],
        _ => &[
            "mark", "data", "encoding", "$schema", "width", "height", "title",
        ],
    };
```

**5b.** Add `"area"` to the encoding-channel allowlist arm (around line 1452):

```rust
        let allowed: &[&str] = match read_mark_name(top) {
            Some("bar" | "line" | "point" | "circle" | "area") => &["x", "y", "color"],
            Some("arc") => &["theta", "color", "x", "y"],
            Some("rect") => &["x", "y", "color"],
            _ => return Ok(()),
        };
```

**5c.** Extend the per-channel allowlist to admit `stack` on area's `y` channel (around line 1464):

```rust
                let channel_allowed: &[&str] =
                    if matches!(read_mark_name(top), Some("rect")) && *channel == "color" {
                        &["field", "type", "aggregate"]
                    } else if matches!(read_mark_name(top), Some("area")) && *channel == "y" {
                        &["field", "type", "stack"]
                    } else {
                        &["field", "type"]
                    };
```

**5d.** Validate `y.stack`'s value (after the existing rect-specific block, before the closing of the `if let Some(encoding) = ...` block):

```rust
        if matches!(read_mark_name(top), Some("area")) {
            if let Some(stack) = encoding
                .get("y")
                .and_then(Value::as_object)
                .and_then(|y| y.get("stack"))
            {
                match stack {
                    Value::Null => {}
                    Value::String(s) if s == "zero" => {}
                    Value::String(_) => {
                        return Err("encoding.y.stack must be \"zero\" or null".to_string());
                    }
                    other => {
                        return Err(format!(
                            "encoding.y.stack must be a string or null, got {}",
                            json_value_type(other)
                        ));
                    }
                }
            }
        }
```

**Step 6: Write strict-mode tests**

Add to `tests/frontend_vegalite.rs`:

```rust
#[test]
fn strict_area_rejects_invalid_stack_value() {
    let json = CATEGORICAL_AREA_STACKED.replace(
        r#""y": {"field": "sales", "type": "quantitative"}"#,
        r#""y": {"field": "sales", "type": "quantitative", "stack": "center"}"#,
    );
    let err = vegalite::parse(&json, true).unwrap_err();
    assert!(err.contains("encoding.y.stack"), "unexpected error: {err}");
    assert!(
        serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&json).is_err(),
        "typed schema must also reject stack: \"center\""
    );
}

#[test]
fn dogfood_categorical_area_is_accepted_by_typed_schema_and_strict_parser() {
    assert!(
        serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(CATEGORICAL_AREA_STACKED)
            .is_ok()
    );
    assert!(vegalite::parse(CATEGORICAL_AREA_STACKED, true).is_ok());
}
```

**Step 7: Run tests + full suite + clippy**

Run: `cargo test -p fulgur-chart 2>&1 | grep -E "test result:|FAILED"` — all green.
Run: `cargo clippy -p fulgur-chart --all-targets 2>&1 | grep -E "warning|error"` — clean.

**Step 8: Commit**

```bash
git add src/frontend/vegalite.rs tests/frontend_vegalite.rs
git commit -m "feat(vegalite): parse mark:\"area\", wire stacked + Series.area

color channel present + y.stack != null -> stacked (real Vega-Lite default).
--strict validates encoding.y.stack's value and the usual key allowlists;
line's categorical/temporal-only restrictions are intentionally not
replicated for area (build_categorical already silently ignores the
equivalent options for line; same applies here, tracked as a follow-up)."
```

---

## Task 6: Temporal area wiring, examples, end-to-end tests, docs

**Files:**
- Modify: `src/frontend/vegalite.rs` (temporal dispatch already routes through `matches!(kind, ChartKind::Line { .. })` after Task 1 — verify `is_area`/`stacked` computation from Task 5 also applies before the temporal dispatch runs)
- Create: `crates/fulgur-chart/tests/render_vega_area.rs`
- Create: `examples/specs/vegalite-area-stacked.json`
- Modify: `README.md`, `examples/README.md`
- New beads issue: strict-mode area/line parity follow-up

**Step 1: Verify temporal area end-to-end (write the test first)**

Create `crates/fulgur-chart/tests/render_vega_area.rs`. Mirror `tests/render_line.rs`'s actual API
(`fulgur_chart::render::render_chart(&ChartSpec) -> String` for SVG — it takes the spec directly,
not a `Scene`/measurer pair):

```rust
use fulgur_chart::frontend::vegalite;
use fulgur_chart::ir::ChartKind;
use fulgur_chart::render::render_chart;

fn render(json: &str) -> String {
    render_chart(&vegalite::parse(json, true).unwrap())
}

const TEMPORAL_AREA_STACKED: &str = r#"{
    "mark": "area",
    "data": {"values": [
        {"t": "2024-01-01T00:00:00Z", "kind": "A", "v": 10},
        {"t": "2024-01-01T00:00:00Z", "kind": "B", "v": 4},
        {"t": "2024-01-02T00:00:00Z", "kind": "A", "v": 12},
        {"t": "2024-01-02T00:00:00Z", "kind": "B", "v": 6}
    ]},
    "encoding": {
        "x": {"field": "t", "type": "temporal"},
        "y": {"field": "v", "type": "quantitative"},
        "color": {"field": "kind", "type": "nominal"}
    }
}"#;

#[test]
fn temporal_area_with_color_is_stacked_and_renders() {
    let spec = vegalite::parse(TEMPORAL_AREA_STACKED, true).unwrap();
    assert!(matches!(spec.kind, ChartKind::Line { stacked: true }));
    assert!(spec.series.iter().all(|s| s.area));
    let svg = render(TEMPORAL_AREA_STACKED);
    assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
    assert!(!svg.contains("NaN") && !svg.contains("inf"));
}
```

Run: `cargo test -p fulgur-chart --test render_vega_area -- --nocapture`

Expected: PASS if Tasks 1–5 are correctly wired (temporal dispatch already goes through the generic `ChartKind::Line { .. }` / `is_area` logic from Task 5 — this test is a regression net, not new production code). If it fails, the temporal branch in `frontend/vegalite.rs` (the `if temporal_line { ... build_temporal_line(...) ... }` block around line 226) runs *before* the `is_area`/`stacked` patch from Task 5 step 3b only if that patch was placed after `color_field` but before `temporal_data` is computed — confirm the ordering; `kind` must be patched to `ChartKind::Line { stacked }` before `Ok(ChartSpec { kind, series, ... })` is constructed, which it is (single assignment near the top, read once at the bottom), so this should already pass. Treat any failure here as a real bug in Task 5, not a new task.

**Step 2: Snapshot test for stacked rendering (categorical)**

Add to the same file:

```rust
#[test]
fn categorical_stacked_area_snapshot() {
    let json = r#"{
        "mark": "area",
        "data": {"values": [
            {"month": "Jan", "kind": "A", "sales": 10},
            {"month": "Jan", "kind": "B", "sales": 5},
            {"month": "Feb", "kind": "A", "sales": 20},
            {"month": "Feb", "kind": "B", "sales": 15},
            {"month": "Mar", "kind": "A", "sales": 8},
            {"month": "Mar", "kind": "B", "sales": 12}
        ]},
        "encoding": {
            "x": {"field": "month", "type": "ordinal"},
            "y": {"field": "sales", "type": "quantitative"},
            "color": {"field": "kind", "type": "nominal"}
        }
    }"#;
    let svg = render(json);
    insta::assert_snapshot!(svg);
}
```

Run: `cargo test -p fulgur-chart --test render_vega_area categorical_stacked_area_snapshot -- --nocapture`

First run creates a `.snap.new` file (via `insta`). Review it by hand (`cat crates/fulgur-chart/tests/snapshots/render_vega_area__categorical_stacked_area_snapshot.snap.new`) — confirm the two `<path>` fills don't overlap incorrectly and the second series' path visually sits on top of the first (spot-check a couple of coordinates against `frame.ys.map(...)` by hand for one category). Accept with `cargo insta accept` (or `INSTA_UPDATE=always cargo test ...`) once satisfied.

**Step 3: Add an example spec**

Create `examples/specs/vegalite-area-stacked.json`:

```json
{
  "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
  "mark": "area",
  "data": {
    "values": [
      { "month": "Jan", "kind": "Organic", "visits": 120 },
      { "month": "Jan", "kind": "Paid", "visits": 60 },
      { "month": "Feb", "kind": "Organic", "visits": 150 },
      { "month": "Feb", "kind": "Paid", "visits": 90 },
      { "month": "Mar", "kind": "Organic", "visits": 170 },
      { "month": "Mar", "kind": "Paid", "visits": 80 },
      { "month": "Apr", "kind": "Organic", "visits": 200 },
      { "month": "Apr", "kind": "Paid", "visits": 110 }
    ]
  },
  "encoding": {
    "x": { "field": "month", "type": "ordinal" },
    "y": { "field": "visits", "type": "quantitative" },
    "color": { "field": "kind", "type": "nominal" }
  },
  "title": "Monthly Visits (stacked)"
}
```

Run: `cargo run -q -p fulgur-chart-cli -- render examples/specs/vegalite-area-stacked.json -o examples/out/vegalite-area-stacked.svg --dsl vegalite --strict`
Run: `cargo run -q -p fulgur-chart-cli -- render examples/specs/vegalite-area-stacked.json -o examples/out/vegalite-area-stacked.png --dsl vegalite --format png`

Expected: both succeed, producing non-empty files. Open the SVG (or eyeball the PNG) to confirm two visually stacked bands, no overlap/inversion.

**Step 4: Docs**

`README.md:188` — update the supported-subset line to mention area:

```
Supported subset: `mark` (`bar` / `line` / `area` / `point` → scatter / `circle` → scatter /
`arc` → pie / `rect` → heatmap), inline `data.values`, and `encoding` fields `x` / `y` /
`color` / `theta`. `area` stacks by default when `color` is present (`encoding.y.stack: null`
to disable), matching Vega-Lite. The Tableau10 color palette is applied automatically to
Vega-Lite specs. Input is converted to a shared intermediate representation, so output
determinism and Fulgur integration are identical to chart.js input.
```

(This also fixes the pre-existing staleness where `circle`/`rect` were already missing from this line — check the current line contents before editing since it may have drifted further since this plan was written.)

`examples/README.md` — add `vegalite-area-stacked.json` to the Vega-Lite bullet list (and fix the "only vegalite.json is Vega-Lite" line if it's still there, since it's already stale relative to the existing `vegalite-*.json` files — confirm current contents before editing).

**Step 5: Full regression pass**

Run: `cargo test -p fulgur-chart 2>&1 | grep -E "test result:|FAILED"` — all green.
Run: `cargo test --workspace 2>&1 | grep -E "test result:|FAILED"` — all green (catches CLI/bindings crates that might reference `ChartKind::Line` or the schema).
Run: `cargo clippy --workspace --all-targets 2>&1 | grep -E "warning|error"` — clean.

**Step 6: File the deferred follow-up**

```bash
bd create --title "Vega-Lite area: replicate line's categorical/temporal strict-mode parity" \
  --type task -p 3 \
  --description "check_unknown_keys validates area's y.stack value and basic key allowlists (fulgur-chart-boo) but does not replicate check_line_keys's categorical-only rejection of mark.interpolate / encoding.{x,y}.title / encoding.color.{title,scale} / background / config. Those are silently accepted (and ignored, since build_categorical doesn't read them) for categorical area, same pre-existing gap as categorical line already has informally documented via check_line_keys's asymmetric restrictions. Low priority: cosmetic strict-mode completeness, not a correctness bug." \
  --label chartjs-compat
bd dep add <new-id> --blocked-by fulgur-chart-boo # if the CLI supports it; otherwise note the relationship in the description
```

**Step 7: Update the design doc's beads issue and close it**

```bash
bd update fulgur-chart-boo --status closed --notes "Implemented: area mark (temporal + categorical), stacked by default with color channel, encoding.y.stack: null to disable. See docs/plans/2026-08-29-vegalite-stacked-area.md and docs/plans/2026-08-29-vegalite-stacked-area-design.md."
```

**Step 8: Commit**

```bash
git add crates/fulgur-chart/tests/render_vega_area.rs crates/fulgur-chart/tests/snapshots/ \
  examples/specs/vegalite-area-stacked.json examples/out/vegalite-area-stacked.svg \
  examples/out/vegalite-area-stacked.png README.md examples/README.md
git commit -m "test(vegalite): end-to-end stacked area coverage + example

Temporal + categorical integration tests, a stacked-area example spec/output,
and README updates for the new mark."
```

**Step 9: Push (per this repo's session-completion protocol in CLAUDE.md)**

This work happens in a worktree (`feat/vegalite-stacked-area`), not `main`. Follow the repo's normal PR flow once all tasks are green: push the branch, open a PR, and only merge/close after review — do not push directly to `main`. Confirm with the user before opening the PR (this plan doesn't assume merge authority).

---

## Not attempted (explicitly out of scope, matches the design doc)

- chart.js `fill: 'stack'` / relative-index / `{target, above, below}` fill modes (`fulgur-chart-hdf`).
- Multiple stack groups (`fulgur-chart-nhb`).
- `area` + `point` marker overlay.
- Full strict-mode parity between `check_line_keys` and area's categorical/temporal-only restrictions (filed as a follow-up in Task 6, Step 6).
- chart.js-side `scales.y.stacked` support for `type: "line"` + `fill: true` datasets (the IR now supports it via `ChartKind::Line { stacked: true }`, but `frontend/chartjs.rs` intentionally always passes `stacked: false` — wiring it up is a separate, chart.js-parity-scoped change).
