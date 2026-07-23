# Vega-Lite Temporal Line Dogfood Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task.
> Project task state is tracked in Beads under `fulgur-chart-6an`; numbered
> steps intentionally replace Markdown checkboxes to comply with AGENTS.md.

**Goal:** Render the `flpdf-qtest` nightly metrics Vega-Lite temporal-line spec
with equivalent time spacing, colors, monotone curves, axes, grid, legend, and
plot-size semantics while preserving existing Chart.js output.

**Architecture:** Add explicit temporal x positions, interpolation, and size
semantics to the IR; parse RFC 3339 input into a shared sorted domain; then
teach the shared frame and line renderer to consume those contracts. Keep date
parsing/ticks and monotone geometry in focused modules, and activate the new
layout path only for the supported Vega-Lite temporal line.

**Tech Stack:** Rust, serde/serde_json, schemars, `time` 0.3, tiny-skia,
insta snapshots, cargo-llvm-cov, Beads (`bd`).

## Global Constraints

- Work in
  `/home/ubuntu/fulgur-chart/.worktrees/6an-vegalite-dogfood-parity` on
  `feat/6an-vegalite-dogfood-parity`.
- The branch is stacked on `feat/s7o-axis-styling` / PR #136; do not merge
  parent commits into this feature's final PR.
- Temporal input v1 accepts RFC 3339 strings only.
- Existing Chart.js and categorical Vega-Lite output must remain byte-stable.
- Vega SVG DOM, ARIA, browser font metrics, and pixel identity are out of scope.
- New user-facing errors are English and must not echo the full JSON document.
- All date calculations and labels are UTC and deterministic on native and
  wasm targets.
- The dogfood plot area must be exactly 720 × 320; axes and legend expand the
  outer scene.
- Run tests after every file-editing task and commit each task independently.
- Final Codecov patch coverage must be 100% against the final committed `HEAD`.
- Use `bd update <id> --claim` before a task, `bd close <id>` after its gates
  pass, and `bd dolt push` after tracker changes.

---

## File map

### New focused modules

- `crates/fulgur-chart/src/temporal.rs`
  - RFC 3339 normalization
  - bounded UTC tick interval selection
  - deterministic compact UTC labels
- `crates/fulgur-chart/src/layout/monotone.rs`
  - monotone-X cubic Hermite control points and SVG path construction
- `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`
  - end-to-end semantic, snapshot, SVG, and raster assertions
- `crates/fulgur-chart/tests/fixtures/vegalite-temporal-line.json`
  - reduced `flpdf-qtest`-shaped spec
- `examples/specs/vegalite-temporal-line.json`
  - user-visible copy of the supported contract

### Existing files with clear ownership

- `Cargo.toml`, `crates/fulgur-chart/Cargo.toml`, `Cargo.lock`
  - `time` dependency
- `crates/fulgur-chart/src/ir.rs`
  - `XPositions`, `LineInterpolation`, `SizeMode`, `legend_title`
- `crates/fulgur-chart/src/guard.rs`
  - temporal-domain shape and ordering invariants
- `crates/fulgur-chart/src/schema/vegalite.rs`
  - typed temporal-line schema
- `crates/fulgur-chart/src/frontend/vegalite.rs`
  - strict validation and Vega-Lite-to-IR conversion
- `crates/fulgur-chart/src/frontend/chartjs.rs`
  - map existing tension behavior into `LineInterpolation`
- `crates/fulgur-chart/src/scale.rs`
  - Vega-style quantitative y-domain/ticks
- `crates/fulgur-chart/src/layout/common.rs`
  - temporal x scale, plot-area sizing, x ticks/grid, titled legend
- `crates/fulgur-chart/src/layout/line.rs`
  - positioned points, interpolation dispatch, explicit marker radius
- `crates/fulgur-chart/src/layout/mod.rs`
  - background uses actual scene dimensions
- `crates/fulgur-chart/src/model.rs`
  - temporal axis metadata, actual outer dimensions, positioned geometry
- `crates/fulgur-chart/tests/frontend_vegalite.rs`
  - parser and strict-contract tests
- `crates/fulgur-chart/tests/render_line.rs`
  - Chart.js byte-stability and marker regression tests

Mechanical `ChartSpec` and `Series` initializer updates are confined to files
returned by:

```bash
rg -l 'ChartSpec \{' crates/fulgur-chart/src crates/fulgur-chart/tests
rg -l 'Series \{' crates/fulgur-chart/src crates/fulgur-chart/tests
```

## Spec coverage map

- RFC 3339 contract, UTC normalization, bounded errors: Tasks 2 and 4
- Shared sorted x domain, duplicate aggregation, dense-series invariant:
  Tasks 1 and 4
- Nominal color order and tableau10 mapping: Tasks 3, 4, and 7
- Explicit linear/Catmull-Rom/monotone semantics: Tasks 1 and 6
- Temporal ticks, compact labels, vertical grid, and grid opacity: Tasks 2,
  4, 5, and 7
- Axis titles and titled right legend: Tasks 4, 5, and 7
- Exact plot size with expanded outer canvas: Tasks 1, 5, and 7
- Vega-style y domain `0..65` for the dogfood data: Tasks 2, 5, and 7
- SVG/PNG/WebP-compatible cubic geometry and model parity: Tasks 6 and 7
- Full live dogfood comparison, wasm check, and 100% patch coverage: Task 8
- Existing Chart.js byte stability: regression gates in Tasks 1, 5, 6, and 8

---

### Task 1 (`fulgur-chart-6an.1`): Add IR contracts and guard invariants

**Files:**

- Modify: `crates/fulgur-chart/src/ir.rs`
- Modify: `crates/fulgur-chart/src/guard.rs`
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs`
- Modify: all `ChartSpec {` and `Series {` initializer files listed in the file
  map

**Interfaces:**

- Produces:
  - `XPositions::{Category, Temporal { unix_millis: Vec<i64> }}`
  - `LineInterpolation::{Linear, CatmullRom { tension: f64 }, Monotone}`
  - `SizeMode::{Canvas, PlotArea}`
  - `ChartSpec::{x_positions, size_mode, legend_title}`
- Consumes: PR #136 `AxisTitle`, `AxisGrid`, and `AxisBorder`

**Step 1: Claim the Bead**

```bash
bd update fulgur-chart-6an.1 --claim
```

Expected: issue becomes `in_progress`.

**Step 2: Write failing IR and guard tests**

Add to `ir.rs` tests:

```rust
#[test]
fn new_line_contracts_have_backward_compatible_defaults() {
    assert_eq!(XPositions::default(), XPositions::Category);
    assert_eq!(LineInterpolation::default(), LineInterpolation::Linear);
    assert_eq!(SizeMode::default(), SizeMode::Canvas);
}
```

Add to `guard.rs` tests:

```rust
#[test]
fn temporal_positions_must_match_categories() {
    let mut spec = base_spec();
    spec.categories = vec!["a".into(), "b".into()];
    spec.x_positions = XPositions::Temporal {
        unix_millis: vec![1],
    };
    let err = validate_spec(&spec, &default_limits()).unwrap_err();
    assert!(err.contains("temporal x position count"));
}

#[test]
fn temporal_positions_must_be_strictly_increasing() {
    let mut spec = base_spec();
    spec.categories = vec!["a".into(), "b".into()];
    spec.series[0].values = vec![1.0, 2.0];
    spec.x_positions = XPositions::Temporal {
        unix_millis: vec![2, 2],
    };
    let err = validate_spec(&spec, &default_limits()).unwrap_err();
    assert!(err.contains("strictly increasing"));
}
```

**Step 3: Run the focused tests and verify RED**

```bash
cargo test -p fulgur-chart new_line_contracts_have_backward_compatible_defaults
cargo test -p fulgur-chart temporal_positions_must_
```

Expected: compile failure because the new types and fields do not exist.

**Step 4: Add the explicit IR types**

Add to `ir.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Default)]
pub enum XPositions {
    #[default]
    Category,
    Temporal { unix_millis: Vec<i64> },
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum LineInterpolation {
    #[default]
    Linear,
    CatmullRom { tension: f64 },
    Monotone,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum SizeMode {
    #[default]
    Canvas,
    PlotArea,
}
```

Replace `Series.tension` with:

```rust
pub interpolation: LineInterpolation,
```

Extend `ChartSpec` with:

```rust
pub x_positions: XPositions,
pub size_mode: SizeMode,
pub legend_title: Option<String>,
```

All existing initializers use:

```rust
x_positions: XPositions::Category,
size_mode: SizeMode::Canvas,
legend_title: None,
```

All existing straight series use:

```rust
interpolation: LineInterpolation::Linear,
```

In `frontend/chartjs.rs`, preserve existing tension behavior:

```rust
fn line_interpolation(tension: f64) -> LineInterpolation {
    if tension <= 0.0 {
        LineInterpolation::Linear
    } else {
        LineInterpolation::CatmullRom { tension }
    }
}
```

**Step 5: Add guard validation**

In `validate_spec`, before primitive counting:

```rust
if let XPositions::Temporal { unix_millis } = &spec.x_positions {
    if unix_millis.len() != spec.categories.len() {
        return Err(format!(
            "temporal x position count {} does not match category count {}",
            unix_millis.len(),
            spec.categories.len()
        ));
    }
    if spec
        .series
        .iter()
        .any(|series| series.values.len() != unix_millis.len())
    {
        return Err(
            "temporal x position count does not match every line series".to_string()
        );
    }
    if unix_millis.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("temporal x positions must be strictly increasing".to_string());
    }
}
```

Also reject `XPositions::Temporal` unless `kind == ChartKind::Line`.

**Step 6: Run formatting and all crate tests**

```bash
cargo fmt --all
cargo test -p fulgur-chart
```

Expected: all existing tests plus the new IR/guard tests pass; no snapshot
changes.

**Step 7: Commit and close**

```bash
git add crates/fulgur-chart/src crates/fulgur-chart/tests
git commit -m "feat(ir): add positioned line contracts"
bd close fulgur-chart-6an.1
bd dolt push
git push
```

---

### Task 2 (`fulgur-chart-6an.2`): Add RFC 3339 and Vega scale utilities

**Files:**

- Modify: `Cargo.toml`
- Modify: `crates/fulgur-chart/Cargo.toml`
- Modify: `Cargo.lock`
- Create: `crates/fulgur-chart/src/temporal.rs`
- Modify: `crates/fulgur-chart/src/lib.rs`
- Modify: `crates/fulgur-chart/src/scale.rs`

**Interfaces:**

- Produces:
  - `parse_rfc3339_millis(field: &str, raw: &str) -> Result<i64, String>`
  - `temporal_ticks(min_ms: i64, max_ms: i64, plot_width: f64)
    -> Vec<TemporalTick>`
  - `TemporalTick { unix_millis: i64, label: String }`
  - `vega_nice_ticks(data_min: f64, data_max: f64, plot_height: f64)
    -> NiceTicks`

**Step 1: Claim the Bead and write failing tests**

```bash
bd update fulgur-chart-6an.2 --claim
```

Create `temporal.rs` with tests first:

```rust
#[test]
fn equivalent_offsets_normalize_to_same_millis() {
    let z = parse_rfc3339_millis("timestamp", "2026-07-22T19:18:38Z").unwrap();
    let offset =
        parse_rfc3339_millis("timestamp", "2026-07-23T04:18:38+09:00").unwrap();
    assert_eq!(z, offset);
}

#[test]
fn invalid_timestamp_error_is_bounded_and_identifies_field() {
    let err = parse_rfc3339_millis("timestamp", "not-a-date").unwrap_err();
    assert!(err.contains("timestamp"));
    assert!(err.contains("not-a-date"));
    assert!(err.len() < 160);
}

#[test]
fn dogfood_range_uses_two_day_ticks() {
    let min = parse_rfc3339_millis("x", "2026-06-05T19:55:20Z").unwrap();
    let max = parse_rfc3339_millis("x", "2026-07-22T19:18:38Z").unwrap();
    let ticks = temporal_ticks(min, max, 720.0);
    assert!(ticks.len() <= 24);
    assert!(
        ticks.windows(2).all(|w| w[1].unix_millis - w[0].unix_millis
            == 2 * 86_400_000)
    );
}
```

Add to `scale.rs` tests:

```rust
#[test]
fn vega_dogfood_domain_is_zero_to_sixty_five() {
    let ticks = vega_nice_ticks(0.0, 61.0, 320.0);
    assert_eq!((ticks.min, ticks.max, ticks.step), (0.0, 65.0, 10.0));
    assert_eq!(ticks.ticks, vec![0.0, 10.0, 20.0, 30.0, 40.0, 50.0, 60.0]);
}
```

**Step 2: Run tests and verify RED**

```bash
cargo test -p fulgur-chart temporal::
cargo test -p fulgur-chart vega_dogfood_domain_is_zero_to_sixty_five
```

Expected: module/functions are missing.

**Step 3: Add the dependency**

In workspace `Cargo.toml`:

```toml
time = { version = "0.3", features = ["formatting", "parsing"] }
```

In `crates/fulgur-chart/Cargo.toml`:

```toml
time = { workspace = true }
```

Expose `pub mod temporal;` from `lib.rs`.

**Step 4: Implement bounded RFC 3339 normalization**

Use `time::format_description::well_known::Rfc3339`:

```rust
pub fn parse_rfc3339_millis(field: &str, raw: &str) -> Result<i64, String> {
    let parsed = time::OffsetDateTime::parse(raw, &Rfc3339).map_err(|_| {
        let shown: String = raw.chars().take(80).collect();
        format!("field {field} contains invalid RFC 3339 timestamp: {shown:?}")
    })?;
    i64::try_from(parsed.unix_timestamp_nanos() / 1_000_000)
        .map_err(|_| format!("field {field} timestamp is outside the supported range"))
}
```

Define the tick table in milliseconds and select the first interval for which
the aligned tick count is no greater than
`clamp(floor(plot_width / 30), 2, 24)`. Guard the output with a hard maximum of
24 entries.

Format labels from UTC dates as `Mon DD` at the first visible tick in a month
and `DD` otherwise. Do not consult system locale or timezone.

**Step 5: Implement exact dogfood y tick policy**

In `scale.rs`, implement:

```rust
pub fn vega_nice_ticks(data_min: f64, data_max: f64, plot_height: f64) -> NiceTicks
```

For positive-only data:

```rust
let span = (data_max - data_min).max(f64::EPSILON);
let padded_max = data_max + span * 0.05;
let target = (plot_height / 40.0).floor().clamp(2.0, 10.0) as usize;
let step = nice_step((padded_max / target as f64).max(f64::EPSILON));
let max = (padded_max / (step / 2.0)).ceil() * (step / 2.0);
```

Emit full-step ticks `<= max`, while the scale domain retains the half-step
maximum. Mirror the policy for negative-only data and pad both sides for mixed
signs. Fall back to `nice_ticks` for empty or non-finite input.

**Step 6: Run native and wasm-compatible tests**

```bash
cargo fmt --all
cargo test -p fulgur-chart temporal::
cargo test -p fulgur-chart scale::
cargo check -p fulgur-chart --target wasm32-unknown-unknown
```

Expected: all pass.

**Step 7: Commit and close**

```bash
git add Cargo.toml Cargo.lock crates/fulgur-chart/Cargo.toml \
  crates/fulgur-chart/src/lib.rs crates/fulgur-chart/src/temporal.rs \
  crates/fulgur-chart/src/scale.rs
git commit -m "feat(vegalite): add deterministic temporal scales"
bd close fulgur-chart-6an.2
bd dolt push
git push
```

---

### Task 3 (`fulgur-chart-6an.3`): Type the supported Vega-Lite line schema

**Files:**

- Modify: `crates/fulgur-chart/src/schema/vegalite.rs`
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**

- Produces:
  - `VlLineEncoding`
  - `VlLineChannel`
  - `VlColorChannel`
  - `VlColorScale`
  - `VlLineInterpolation`
  - `VlConfig` and `VlAxisConfig`
- Later Task 4 consumes the same JSON keys through the manual frontend.

**Step 1: Claim and write strict/schema RED tests**

```bash
bd update fulgur-chart-6an.3 --claim
```

Add:

```rust
const DOGFOOD_SHAPE: &str = r##"{
  "$schema":"https://vega.github.io/schema/vega-lite/v5.json",
  "title":"qtest nightly trend",
  "width":720,
  "height":320,
  "background":"white",
  "data":{"values":[
    {"timestamp":"2026-07-21T19:21:53Z","metric":"regressions","value":0}
  ]},
  "mark":{"type":"line","point":true,"interpolate":"monotone"},
  "encoding":{
    "x":{"field":"timestamp","type":"temporal","title":"date"},
    "y":{"field":"value","type":"quantitative","title":"subtests"},
    "color":{"field":"metric","type":"nominal","title":"metric",
             "scale":{"scheme":"tableau10"}}
  },
  "config":{"view":{"stroke":null},
            "axis":{"grid":true,"gridOpacity":0.15}}
}"##;

#[test]
fn dogfood_shape_is_accepted_by_typed_schema_and_strict_parser() {
    let _: fulgur_chart::schema::VegaLiteSpec =
        serde_json::from_str(DOGFOOD_SHAPE).unwrap();
    assert!(vegalite::parse(DOGFOOD_SHAPE, true).is_ok());
}
```

Add typo tests for `interpolatee`, `gridOpacit`, and `scheeme`, asserting the
error contains the full key path.

**Step 2: Run and verify RED**

```bash
cargo test -p fulgur-chart dogfood_shape_is_accepted
cargo test -p fulgur-chart strict_temporal_line_rejects_
```

Expected: schema/strict parser rejects currently unsupported keys.

**Step 3: Add line-only typed structures**

Use:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlLineInterpolation {
    Linear,
    Monotone,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkLineObject {
    #[serde(rename = "type")]
    pub mark_type: MarkLineName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolate: Option<VlLineInterpolation>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VlLineChannel {
    pub field: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}
```

Create a separate color channel whose `scale.scheme` enum only permits
`tableau10`. Change `VlLineSpec.encoding` from `VlBarEncoding` to
`VlLineEncoding`, and add optional `background` and `config`.

`VlAxisConfig` is:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VlAxisConfig {
    pub grid: Option<bool>,
    pub grid_opacity: Option<f64>,
}
```

Model `config.view.stroke` as `Option<serde_json::Value>` so the explicit null
from the fixture is accepted without claiming rendering support.

**Step 4: Extend manual strict validation**

Make top-level `background` and `config` legal only where modeled. For line:

- mark object keys: `type`, `point`, `interpolate`
- x/y channel keys: `field`, `type`, `title`
- color channel keys: `field`, `type`, `title`, `scale`
- color scale keys: `scheme`
- config keys: `view`, `axis`
- config.axis keys: `grid`, `gridOpacity`

Validate types and supported enum values, including `0.0 <= gridOpacity <= 1.0`.

**Step 5: Run all frontend tests**

```bash
cargo fmt --all
cargo test -p fulgur-chart --test frontend_vegalite
```

Expected: all old tests and new dogfood/typo tests pass.

**Step 6: Commit and close**

```bash
git add crates/fulgur-chart/src/schema/vegalite.rs \
  crates/fulgur-chart/src/frontend/vegalite.rs \
  crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "feat(schema): type Vega-Lite temporal line options"
bd close fulgur-chart-6an.3
bd dolt push
git push
```

---

### Task 4 (`fulgur-chart-6an.4`): Convert temporal Vega-Lite data to IR

**Files:**

- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**

- Consumes:
  - Task 1 IR contracts
  - Task 2 `parse_rfc3339_millis`
  - Task 3 supported schema keys
- Produces:
  - `temporal_domain(records, field) -> Result<Vec<(i64, String)>, String>`
  - a fully populated temporal `ChartSpec`

**Step 1: Claim and write conversion RED tests**

```bash
bd update fulgur-chart-6an.4 --claim
```

Add tests for:

```rust
#[test]
fn temporal_line_sorts_x_and_nominal_color_domain() {
    let spec = vegalite::parse(DOGFOOD_MULTI_SERIES, true).unwrap();
    let expected = vec![
        parse_rfc3339_millis("timestamp", "2026-06-29T19:00:00Z").unwrap(),
        parse_rfc3339_millis("timestamp", "2026-07-01T19:00:00Z").unwrap(),
    ];
    assert_eq!(spec.x_positions, XPositions::Temporal { unix_millis: expected });
    assert_eq!(
        spec.series.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        vec!["allowlist", "candidates", "regressions"]
    );
    assert_eq!(
        spec.series.iter().map(|s| s.stroke[0]).collect::<Vec<_>>(),
        VEGALITE_PALETTE[..3]
    );
}
```

Also add tests for:

- out-of-order records
- offset-equivalent duplicate timestamps aggregated once
- invalid/null/non-string timestamp errors
- sparse `(timestamp, metric)` rejection after sorting
- `point: true`, `monotone`, channel titles, right legend, `PlotArea`,
  line width 2, and grid alpha 0.15 in IR

**Step 2: Run and verify RED**

```bash
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_
```

Expected: x positions remain categorical and metadata is absent.

**Step 3: Add small parsing helpers**

Implement:

```rust
fn channel_type(encoding: &Map<String, Value>, name: &str) -> Option<&str>;
fn channel_title(
    encoding: &Map<String, Value>,
    name: &str,
    fallback_field: &str,
) -> String;
fn temporal_domain(
    records: &[Map<String, Value>],
    field: &str,
) -> Result<Vec<(i64, String)>, String>;
```

`temporal_domain` parses all values, inserts by normalized milliseconds into a
sorted `BTreeMap`, and retains one source label per instant.

**Step 4: Build temporal colored series**

For line + temporal x:

1. Build the sorted temporal domain.
2. Sort nominal group names with `sort_unstable`.
3. For each group and instant, sum matching y values.
4. Reject a missing pair instead of inserting zero.
5. Assign palette colors after sorting.

Populate:

```rust
x_positions: XPositions::Temporal { unix_millis },
size_mode: SizeMode::PlotArea,
legend: LegendPos::Right,
legend_title: color_field.map(|f| channel_title(encoding, "color", f)),
```

Use `AxisTitle` for x/y titles, apply `gridOpacity` by multiplying the
Vega-Lite theme grid color alpha, and set:

```rust
stroke_width: 2.0,
interpolation: LineInterpolation::Monotone,
point_radius: Some(if point { 3.0 } else { 0.0 }),
```

Set both Vega axes to `grid.draw_ticks = true`; this is an explicit Vega
frontend choice and does not change PR #136's Chart.js-compatible default.

Parse top-level `background` through the existing color parser. The dogfood
value `"white"` resolves to an opaque white `theme.background`; invalid
recognized colors return an English parse error.

Only the supported temporal-line path gets these semantics. Existing
categorical builders retain first-seen categories and current sizing.

**Step 5: Run frontend and guard suites**

```bash
cargo fmt --all
cargo test -p fulgur-chart --test frontend_vegalite
cargo test -p fulgur-chart guard::
```

Expected: all pass.

**Step 6: Commit and close**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs \
  crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "feat(vegalite): convert temporal lines to positioned IR"
bd close fulgur-chart-6an.4
bd dolt push
git push
```

---

### Task 5 (`fulgur-chart-6an.5`): Render temporal frame, axes, legend, and plot size

**Files:**

- Modify: `crates/fulgur-chart/src/layout/common.rs`
- Modify: `crates/fulgur-chart/src/layout/line.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`
- Test: `crates/fulgur-chart/src/layout/common.rs`
- Test: `crates/fulgur-chart/src/layout/line.rs`

**Interfaces:**

- Consumes Task 1 `XPositions`/`SizeMode`, Task 2 ticks, Task 4 populated IR.
- Produces:
  - `Frame::{scene_width, scene_height, temporal_ticks}`
  - `line_x(spec, frame, index) -> f64`
  - plot-area sizing and temporal axis primitives

**Step 1: Claim and write geometry RED tests**

```bash
bd update fulgur-chart-6an.5 --claim
```

Add tests:

```rust
#[test]
fn temporal_x_distances_follow_elapsed_time() {
    let spec = temporal_spec(vec![0, 86_400_000, 3 * 86_400_000]);
    let frame = compute(&spec, &measurer());
    let x0 = line_x(&spec, &frame, 0);
    let x1 = line_x(&spec, &frame, 1);
    let x2 = line_x(&spec, &frame, 2);
    assert!(((x1 - x0) / (x2 - x0) - 1.0 / 3.0).abs() < 1e-9);
}

#[test]
fn plot_area_mode_preserves_requested_plot_size() {
    let spec = temporal_dogfood_spec();
    let frame = compute(&spec, &measurer());
    assert_eq!(frame.plot_right - frame.plot_left, 720.0);
    assert_eq!(frame.plot_bottom - frame.plot_top, 320.0);
    assert!(frame.scene_width > 720.0);
    assert!(frame.scene_height > 320.0);
}

#[test]
fn singleton_temporal_domain_maps_to_plot_center() {
    let spec = temporal_spec(vec![42]);
    let frame = compute(&spec, &measurer());
    assert_eq!(
        line_x(&spec, &frame, 0),
        (frame.plot_left + frame.plot_right) / 2.0
    );
}
```

Add primitive assertions for vertical grid alpha, x tick labels, axis titles,
and legend title above right-side entries.

**Step 2: Run and verify RED**

```bash
cargo test -p fulgur-chart layout::common::tests::temporal_
cargo test -p fulgur-chart layout::common::tests::plot_area_
```

Expected: missing frame fields/functions.

**Step 3: Extend `Frame` and centralize x mapping**

Add:

```rust
pub struct Frame {
    pub scene_width: f64,
    pub scene_height: f64,
    // existing fields
    pub temporal_ticks: Vec<TemporalTick>,
}

pub fn line_x(spec: &ChartSpec, frame: &Frame, index: usize) -> f64 {
    match &spec.x_positions {
        XPositions::Category => {
            line_category_x(spec, frame, index, spec.categories.len().max(1))
        }
        XPositions::Temporal { unix_millis } => {
            let min = *unix_millis.first().unwrap_or(&0);
            let max = *unix_millis.last().unwrap_or(&min);
            if min == max {
                (frame.plot_left + frame.plot_right) / 2.0
            } else {
                let ratio = (unix_millis[index] - min) as f64 / (max - min) as f64;
                frame.plot_left + ratio * (frame.plot_right - frame.plot_left)
            }
        }
    }
}
```

Use `line_x` in `line_points`, render geometry, labels, and model geometry.

**Step 4: Implement `PlotArea` frame sizing**

For `Canvas`, preserve the current equations exactly.

For `PlotArea`:

```rust
let plot_left = OUTER_PAD + y_axis_w;
let plot_top = OUTER_PAD + title_band;
let plot_right = plot_left + spec.width;
let plot_bottom = plot_top + spec.height;
let scene_width = plot_right + OUTER_PAD + legend_right;
let scene_height =
    plot_bottom + X_LABEL_BAND + x_title_h + OUTER_PAD + legend_bottom;
```

Compute right legend width with `legend_title` included. Position it relative
to `plot_right`, not `spec.width`.

Use Task 2 `vega_nice_ticks` only for temporal lines; Canvas mode continues to
use `nice_ticks`.

**Step 5: Draw temporal x-axis and titled legend**

When `XPositions::Temporal`:

- draw vertical grid for every temporal tick if `x_axis.grid.display`
- draw bottom tick marks if `x_axis.grid.draw_ticks`
- measure labels and deterministically skip overlaps without removing grid
- draw x title through PR #136 `AxisTitle`
- draw `legend_title` above vertical legend entries

Update `draw_vertical_legend` to accept:

```rust
title: Option<&str>
```

and reserve one `LEGEND_ROW_H` row when present.

**Step 6: Use actual scene size everywhere**

`line::build` returns:

```rust
Scene {
    width: frame.scene_width,
    height: frame.scene_height,
    items,
}
```

In `layout/mod.rs`, size the theme background from the built scene:

```rust
w: scene.width,
h: scene.height,
```

instead of `spec.width`/`spec.height`.

**Step 7: Run focused and regression tests**

```bash
cargo fmt --all
cargo test -p fulgur-chart layout::common::
cargo test -p fulgur-chart --test render_line
cargo test -p fulgur-chart --test render_legend
cargo test -p fulgur-chart --test golden_png
```

Expected: new temporal tests pass and existing snapshots remain unchanged.

**Step 8: Commit and close**

```bash
git add crates/fulgur-chart/src/layout/common.rs \
  crates/fulgur-chart/src/layout/line.rs \
  crates/fulgur-chart/src/layout/mod.rs
git commit -m "feat(layout): render temporal Vega-Lite axes"
bd close fulgur-chart-6an.5
bd dolt push
git push
```

---

### Task 6 (`fulgur-chart-6an.6`): Add monotone interpolation and explicit points

**Files:**

- Create: `crates/fulgur-chart/src/layout/monotone.rs`
- Modify: `crates/fulgur-chart/src/layout/mod.rs`
- Modify: `crates/fulgur-chart/src/layout/line.rs`
- Test: `crates/fulgur-chart/src/layout/monotone.rs`
- Test: `crates/fulgur-chart/tests/render_line.rs`

**Interfaces:**

- Produces:
  - `monotone_path(points: &[(f64, f64)]) -> String`
- Consumes Task 1 `LineInterpolation` and Task 5 positioned points.

**Step 1: Claim and write RED geometry tests**

```bash
bd update fulgur-chart-6an.6 --claim
```

Create module tests:

```rust
#[test]
fn monotone_path_uses_cubics_without_non_finite_values() {
    let path = monotone_path(&[(0.0, 0.0), (1.0, 10.0), (3.0, 12.0)]);
    assert!(path.starts_with("M 0 0 C "));
    assert!(!path.contains("NaN"));
    assert!(!path.contains("inf"));
}

#[test]
fn two_points_degrade_to_a_line() {
    assert_eq!(monotone_path(&[(0.0, 1.0), (2.0, 3.0)]), "M 0 1 L 2 3");
}
```

Add a control-point helper test that parses each cubic segment and asserts
both y control points lie between that segment's endpoint y values for
increasing, decreasing, flat, and local-extrema fixtures.

Add `render_line.rs` tests proving explicit `point_radius = Some(0.0)`
suppresses markers and `None` retains current markers.

**Step 2: Run and verify RED**

```bash
cargo test -p fulgur-chart layout::monotone::
cargo test -p fulgur-chart --test render_line explicit_point_
```

Expected: module missing; non-decimated explicit zero still draws markers.

**Step 3: Implement monotone-X slopes**

Use x-aware secant slopes:

```rust
fn secant(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.1 - a.1) / (b.0 - a.0)
}

fn tangent(prev: f64, next: f64) -> f64 {
    if prev == 0.0 || next == 0.0 || prev.signum() != next.signum() {
        0.0
    } else {
        let candidate = (prev + next) / 2.0;
        candidate.signum() * candidate.abs().min(3.0 * prev.abs().min(next.abs()))
    }
}
```

Compute endpoint tangents from their adjacent secants. For segment width `h`,
emit cubic controls:

```rust
let cp1 = (p0.0 + h / 3.0, p0.1 + m0 * h / 3.0);
let cp2 = (p1.0 - h / 3.0, p1.1 - m1 * h / 3.0);
```

Clamp each control y to the closed endpoint range as a final numeric guard.
Use `fmt_num` for every coordinate.

**Step 4: Dispatch explicit interpolation**

In `line.rs`:

```rust
match ser.interpolation {
    LineInterpolation::Linear => Prim::Polyline { /* existing */ },
    LineInterpolation::CatmullRom { tension } => {
        Prim::Path { d: catmull_rom_path(&xy, tension), /* existing */ }
    }
    LineInterpolation::Monotone => {
        Prim::Path { d: monotone_path(&xy), /* same stroke */ }
    }
}
```

For markers, make explicit values authoritative even when not decimated:

```rust
let r = match (decimated, ser.point_radius) {
    (_, Some(r)) if r > 0.0 => Some(r),
    (_, Some(_)) => None,
    (false, None) => Some(MARKER_R),
    (true, None) if seg.len() < 2 => Some(MARKER_R),
    (true, None) => None,
};
```

**Step 5: Run geometry, SVG, and raster tests**

```bash
cargo fmt --all
cargo test -p fulgur-chart layout::monotone::
cargo test -p fulgur-chart --test render_line
cargo test -p fulgur-chart raster_direct::tests::line_chart_with_tension_renders
```

Expected: all pass; existing Catmull-Rom snapshot remains byte-identical.

**Step 6: Commit and close**

```bash
git add crates/fulgur-chart/src/layout/monotone.rs \
  crates/fulgur-chart/src/layout/mod.rs \
  crates/fulgur-chart/src/layout/line.rs \
  crates/fulgur-chart/tests/render_line.rs
git commit -m "feat(line): add monotone interpolation"
bd close fulgur-chart-6an.6
bd dolt push
git push
```

---

### Task 7 (`fulgur-chart-6an.7`): Add dogfood end-to-end and model parity

**Files:**

- Create: `crates/fulgur-chart/tests/fixtures/vegalite-temporal-line.json`
- Create: `examples/specs/vegalite-temporal-line.json`
- Create: `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`
- Create: generated insta snapshot under
  `crates/fulgur-chart/tests/snapshots/`
- Modify: `crates/fulgur-chart/src/model.rs`
- Modify: `crates/fulgur-chart/tests/inspect_model.rs`

**Interfaces:**

- Consumes the completed parser, temporal frame, and monotone renderer.
- Produces an end-to-end acceptance fixture and accurate semantic model.

**Step 1: Claim and add the reduced fixture**

```bash
bd update fulgur-chart-6an.7 --claim
```

The fixture must use the exact dogfood shape and include:

- June 29, July 1, and July 5 timestamps
- uneven 2-day then 4-day spacing
- all three metrics at every timestamp
- values containing zero, a flat segment, and an increase
- intentionally non-sorted input records

Keep `$schema`, title, 720 × 320, white background, monotone+points,
temporal/quantitative/nominal channels, tableau10, and grid opacity 0.15.

**Step 2: Write end-to-end RED tests**

Create:

```rust
fn fixture() -> &'static str {
    include_str!("fixtures/vegalite-temporal-line.json")
}

#[test]
fn dogfood_fixture_renders_in_strict_and_non_strict_modes() {
    for strict in [false, true] {
        let spec = vegalite::parse(fixture(), strict).unwrap();
        validate_spec(&spec, &InputLimits::default()).unwrap();
        let svg = render_chart(&spec);
        assert!(svg.contains("qtest nightly trend"));
        assert!(svg.contains(">date</text>"));
        assert!(svg.contains(">subtests</text>"));
        assert!(svg.contains(">metric</text>"));
        assert!(svg.contains("stroke-opacity=\"0.15\""));
        assert!(svg.contains("<path"));
        assert!(svg.contains(" C "));
    }
}
```

Add tests for exact series/color order, x-distance ratio, `Frame` plot size,
outer SVG dimensions, circle count, deterministic repeated SVG, and successful
PNG decode.

**Step 3: Run and verify RED**

```bash
cargo test -p fulgur-chart --test render_vegalite_temporal_line
```

Expected: model/dimension assertions fail until Task 7 changes are present;
if render assertions already pass, preserve them as acceptance coverage.

**Step 4: Make the semantic model temporal-aware**

Add:

```rust
fn temporal_axis(unix_millis: &[i64], ticks: &[TemporalTick]) -> AxisModel {
    AxisModel {
        kind: "temporal".to_string(),
        labels: Some(ticks.iter().map(|t| t.label.clone()).collect()),
        min: unix_millis.first().map(|v| *v as f64),
        max: unix_millis.last().map(|v| *v as f64),
        step: ticks.windows(2).next().map(|w| {
            (w[1].unix_millis - w[0].unix_millis) as f64
        }),
        ticks: Some(ticks.iter().map(|t| t.unix_millis as f64).collect()),
    }
}
```

In `compute_axes`, return this axis for temporal lines. In `build_model`, use
`common::compute` to set `model.meta.width/height` from actual scene dimensions
and to compute positioned line geometry.

**Step 5: Accept and inspect the snapshot**

```bash
INSTA_UPDATE=always cargo test -p fulgur-chart \
  --test render_vegalite_temporal_line
git diff -- crates/fulgur-chart/tests/snapshots
```

Inspect the snapshot for all three colors, right legend title, temporal labels,
vertical/horizontal grid, axis titles, cubic paths, points, and expanded
canvas. Do not accept unrelated changes.

**Step 6: Run all affected suites**

```bash
cargo fmt --all
cargo test -p fulgur-chart --test frontend_vegalite
cargo test -p fulgur-chart --test render_vegalite_temporal_line
cargo test -p fulgur-chart --test inspect_model
cargo test -p fulgur-chart --test golden_png
```

Expected: all pass.

**Step 7: Commit and close**

```bash
git add crates/fulgur-chart/src/model.rs \
  crates/fulgur-chart/tests/inspect_model.rs \
  crates/fulgur-chart/tests/fixtures/vegalite-temporal-line.json \
  crates/fulgur-chart/tests/render_vegalite_temporal_line.rs \
  crates/fulgur-chart/tests/snapshots \
  examples/specs/vegalite-temporal-line.json
git commit -m "test(vegalite): cover temporal dogfood parity"
bd close fulgur-chart-6an.7
bd dolt push
git push
```

---

### Task 8 (`fulgur-chart-6an.8`): Verify full dogfood data and final gates

**Files:**

- Modify only files with uncovered changed lines or confirmed documentation
  inaccuracies.
- Do not commit `/tmp` comparison artifacts or the evolving external
  `metrics.jsonl`.

**Interfaces:**

- Consumes the complete feature.
- Produces final committed, pushed, CI-green stacked branch and evidence for
  closing `fulgur-chart-6an`.

**Step 1: Claim and confirm clean stack**

```bash
bd update fulgur-chart-6an.8 --claim
git fetch origin
git rebase origin/feat/s7o-axis-styling
git status --short --branch
```

Expected: clean feature branch, ahead only by 6an commits.

**Step 2: Reconstruct the live dogfood spec outside the repository**

```bash
curl -L --fail --silent --show-error \
  -o /tmp/flpdf-qtest-metrics.jsonl \
  https://raw.githubusercontent.com/fulgur-rs/flpdf-qtest/metrics-data/metrics.jsonl
curl -L --fail --silent --show-error \
  -o /tmp/flpdf-qtest-vega.svg \
  https://raw.githubusercontent.com/fulgur-rs/flpdf-qtest/metrics-data/trend.svg
curl -L --fail --silent --show-error \
  -o /tmp/plot-metrics.py \
  https://raw.githubusercontent.com/fulgur-rs/flpdf-qtest/main/scripts/plot-metrics.py
python3 - <<'PY'
import importlib.util
import json
from pathlib import Path

module_spec = importlib.util.spec_from_file_location("plot_metrics", "/tmp/plot-metrics.py")
plot_metrics = importlib.util.module_from_spec(module_spec)
module_spec.loader.exec_module(plot_metrics)
records = plot_metrics.load_records(Path("/tmp/flpdf-qtest-metrics.jsonl"))
Path("/tmp/flpdf-qtest-spec.json").write_text(
    json.dumps(plot_metrics.build_spec(records), indent=2),
    encoding="utf-8",
)
PY
```

The checked-in `trend.svg` remains the Vega reference; the snippet deliberately
uses only pure `load_records` and `build_spec`, so `vl_convert` is not required.
Do not install Python packages into the repository environment.

Render fulgur:

```bash
cargo run -p fulgur-chart-cli -- \
  render /tmp/flpdf-qtest-spec.json \
  -o /tmp/flpdf-qtest-fulgur.svg \
  --dsl vegalite
```

**Step 3: Inspect semantic parity**

Verify:

```bash
rg -o '#4c78a8|#f58518|#e45756' /tmp/flpdf-qtest-fulgur.svg | sort -u
rg -n '>date</text>|>subtests</text>|>metric</text>| C ' \
  /tmp/flpdf-qtest-fulgur.svg
head -c 160 /tmp/flpdf-qtest-fulgur.svg
```

Expected:

- all three colors are present with the intended series mapping
- date, subtests, and metric titles exist
- cubic `C` commands exist
- outer SVG dimensions exceed 720 × 320
- the tested frame reports a plot width/height of exactly 720 × 320

Open or rasterize both SVGs for a final side-by-side visual check when a local
SVG viewer is available. Visual differences listed as non-goals do not block.

**Step 4: Run the full quality gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p fulgur-chart
cargo check -p fulgur-chart --target wasm32-unknown-unknown
```

Expected: all exit 0.

**Step 5: Generate coverage from final committed code**

Commit any last test-only coverage fixes first, then:

```bash
cargo llvm-cov --workspace --locked --lcov --output-path /tmp/fulgur-lcov.info
git status --short
```

Expected: coverage succeeds and the worktree is clean. Push and inspect the
CodeCov patch check:

```bash
git push
gh pr view --json number,url >/dev/null 2>&1 || \
  gh pr create --draft \
    --base feat/s7o-axis-styling \
    --head feat/6an-vegalite-dogfood-parity \
    --title "feat(vegalite): match temporal line dogfood semantics" \
    --body-file /tmp/fulgur-chart-6an-pr.md
gh pr checks --watch
```

After the semantic checks pass, create `/tmp/fulgur-chart-6an-pr.md` with
`apply_patch` using this exact body:

```markdown
## Summary

- add RFC 3339 temporal positioning for Vega-Lite line marks
- match nominal tableau10 colors, monotone curves, axes, grid, and right legend
- preserve the requested 720 x 320 plot area inside an expanded outer canvas

## Dogfood verification

- Compared against `fulgur-rs/flpdf-qtest` branch `metrics-data`
- Verified series colors, temporal spacing, axis and legend titles, monotone
  paths, and the 720 x 320 plot area

## Tests

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets --locked -- -D warnings`
- `cargo test -p fulgur-chart`
- `cargo check -p fulgur-chart --target wasm32-unknown-unknown`

## Dependency

Stacked on #136. Keep this PR draft until the parent merges.

Closes fulgur-chart-6an
```

Keep the PR draft while its base PR is open.

Expected: Codecov patch coverage is exactly 100%. If it is lower, use the
changed-line annotations to add a behavior-focused test, commit it, regenerate
coverage from the new `HEAD`, push, and repeat.

**Step 6: Close Beads and verify remote state**

```bash
bd close fulgur-chart-6an.8
bd close fulgur-chart-6an
bd dolt push
git pull --rebase
git push
git status --short --branch
```

Expected: branch is clean and up to date with
`origin/feat/6an-vegalite-dogfood-parity`; all child issues and the parent are
closed.
