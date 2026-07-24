# PR #137 Fourth Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the five current PR #137 review threads while preserving categorical-line compatibility, temporal PlotArea dimensions, and schema/runtime parity.

**Architecture:** Split the typed line schema into disjoint temporal and categorical variants, while leaving runtime chart dispatch unchanged. Fix the three runtime/layout issues at their existing validation and geometry boundaries: strict line validation, temporal coordinate interpolation, and shared PlotArea frame computation.

**Tech Stack:** Rust 2024, serde, schemars, cargo test, cargo clippy, cargo llvm-cov

## Global Constraints

- Preserve the requested PlotArea width and height.
- Preserve existing valid categorical-line and temporal dogfood inputs.
- Keep non-strict color-channel compatibility unchanged; the new missing-field rejection is strict-only.
- Use existing derive-generated schemars output; do not hand-write JSON Schema.
- Add each regression test before its production change and observe the expected failure.
- Final changed-line coverage against `origin/main...HEAD` must be 100%.

---

### Task 1: Make typed line schema match strict temporal/categorical boundaries

**Files:**
- Modify: `crates/fulgur-chart/src/schema/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- Consumes: `serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>()`
- Produces: disjoint `VlTemporalLineSpec` and `VlCategoricalLineSpec` variants in `VegaLiteSpec`

- [ ] **Step 1: Write failing typed-schema tests**

Add tests that prove invalid channel types and temporal-only categorical options are rejected:

```rust
#[test]
fn typed_line_schema_constrains_channel_types() {
    for json in [
        DOGFOOD_SHAPE.replace("\"temporal\"", "\"temporl\""),
        DOGFOOD_SHAPE.replace("\"quantitative\"", "\"nominal\""),
        DOGFOOD_SHAPE.replace("\"nominal\"", "\"quantitative\""),
    ] {
        assert!(
            serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&json).is_err(),
            "typed schema accepted unsupported line channel type"
        );
    }
}

#[test]
fn typed_categorical_line_schema_excludes_temporal_only_options() {
    let base = r#"{
        "mark":{"type":"line"},
        "data":{"values":[{"x":"a","y":1}]},
        "encoding":{
            "x":{"field":"x","type":"nominal"},
            "y":{"field":"y","type":"quantitative"}
        }
    }"#;
    let with_point = base.replace(
        r#""type":"line""#,
        r#""type":"line","point":false"#,
    );
    assert!(
        serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(&with_point).is_err()
    );
    assert!(serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(base).is_ok());
}
```

- [ ] **Step 2: Run tests and verify RED**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite typed_line_schema_constrains_channel_types -- --exact
cargo test -p fulgur-chart --test frontend_vegalite typed_categorical_line_schema_excludes_temporal_only_options -- --exact
```

Expected: both tests fail because the shared line schema accepts arbitrary channel type strings and temporal mark options.

- [ ] **Step 3: Split and constrain the typed schema**

Replace the shared line variant with:

```rust
pub enum VegaLiteSpec {
    Bar(VlBarSpec),
    TemporalLine(VlTemporalLineSpec),
    CategoricalLine(VlCategoricalLineSpec),
    Point(VlPointSpec),
    Circle(VlCircleSpec),
    Arc(VlArcSpec),
    Rect(VlRectSpec),
}
```

Keep the current extended `MarkLine` for temporal lines. Add a categorical mark object whose only field is `type`:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MarkCategoricalLineObject {
    #[serde(rename = "type")]
    pub mark_type: MarkLineName,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum MarkCategoricalLine {
    String(MarkLineName),
    Object(MarkCategoricalLineObject),
}
```

Define channel enums matching `validate_line_channel_types`:

```rust
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlTemporalType {
    Temporal,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlCategoricalType {
    Nominal,
    Ordinal,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum VlQuantitativeType {
    Quantitative,
}
```

Create temporal channel structs with titles and scale, requiring temporal x type:

```rust
pub struct VlTemporalXChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: VlTemporalType,
    pub title: Option<String>,
}

pub struct VlTemporalYChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: Option<VlQuantitativeType>,
    pub title: Option<String>,
}

pub struct VlTemporalColorChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: Option<VlCategoricalType>,
    pub title: Option<String>,
    pub scale: Option<VlColorScale>,
}
```

Create categorical channel structs without temporal-only title/scale fields:

```rust
pub struct VlCategoricalXChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: Option<VlCategoricalType>,
}

pub struct VlCategoricalYChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: Option<VlQuantitativeType>,
}

pub struct VlCategoricalColorChannel {
    pub field: String,
    #[serde(rename = "type")]
    pub field_type: Option<VlCategoricalType>,
}
```

Apply `#[serde(deny_unknown_fields)]` and the existing
`skip_serializing_if = "Option::is_none"` attributes to every new struct and optional field.
Use the existing top-level temporal fields only on `VlTemporalLineSpec`; keep categorical
top-level fields to mark, data, encoding, `$schema`, width, height, and title.

- [ ] **Step 4: Run schema tests and existing strict boundary tests**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite typed_line_schema_constrains_channel_types -- --exact
cargo test -p fulgur-chart --test frontend_vegalite typed_categorical_line_schema_excludes_temporal_only_options -- --exact
cargo test -p fulgur-chart --test frontend_vegalite dogfood_shape_is_accepted_by_typed_schema_and_strict_parser -- --exact
cargo test -p fulgur-chart --test frontend_vegalite strict_categorical_line_rejects_temporal_only_options -- --exact
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/schema/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "fix(schema): split temporal and categorical lines"
```

### Task 2: Require a strict line color field

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- Consumes: `check_line_keys(top, encoding)`
- Produces: strict error `encoding.color.field is required`

- [ ] **Step 1: Write the failing strict-parser test**

```rust
#[test]
fn strict_temporal_line_requires_color_field() {
    let json = DOGFOOD_SHAPE.replace(r#""field":"metric","#, "");
    let err = vegalite::parse(&json, true).unwrap_err();
    assert_eq!(err, "encoding.color.field is required");
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite strict_temporal_line_requires_color_field -- --exact
```

Expected: fail because the strict parser currently accepts the object and silently drops its title/scale semantics.

- [ ] **Step 3: Add the strict required-field check**

Immediately after `check_line_string(color, "field", "encoding.color.field")?`, add:

```rust
if !color.contains_key("field") {
    return Err("encoding.color.field is required".to_string());
}
```

Do not add this check to the non-strict parse path.

- [ ] **Step 4: Run focused and line frontend tests**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite strict_temporal_line_requires_color_field -- --exact
cargo test -p fulgur-chart --test frontend_vegalite
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "fix(vegalite): require strict line color fields"
```

### Task 3: Make temporal x interpolation overflow-safe

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs`
- Test: `crates/fulgur-chart/src/layout/common.rs`

**Interfaces:**
- Consumes: `temporal_x(frame, min: i64, max: i64, value: i64) -> f64`
- Produces: the same mapping without `i64` subtraction overflow

- [ ] **Step 1: Write the failing extreme-domain test**

```rust
#[test]
fn temporal_x_handles_full_i64_domain() {
    let spec = temporal_spec(vec![i64::MIN, 0, i64::MAX]);
    let frame = compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
    let left = line_x(&spec, &frame, 0);
    let middle = line_x(&spec, &frame, 1);
    let right = line_x(&spec, &frame, 2);

    assert_eq!(left, frame.plot_left);
    assert_eq!(right, frame.plot_right);
    assert!(((middle - left) / (right - left) - 0.5).abs() < 1e-9);
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p fulgur-chart layout::common::tests::temporal_x_handles_full_i64_domain -- --exact
```

Expected: fail with an overflow panic from `max - min`.

- [ ] **Step 3: Widen before subtraction**

Replace the ratio calculation with:

```rust
let numerator = value as i128 - min as i128;
let denominator = max as i128 - min as i128;
let ratio = numerator as f64 / denominator as f64;
```

- [ ] **Step 4: Run common layout tests**

Run:

```bash
cargo test -p fulgur-chart layout::common::tests::
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/common.rs
git commit -m "fix(layout): avoid temporal coordinate overflow"
```

### Task 4: Keep tall PlotArea vertical legends inside the scene

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs`
- Test: `crates/fulgur-chart/src/layout/common.rs`

**Interfaces:**
- Consumes: `compute(spec, measurer) -> Frame`, `LEGEND_ROW_H`
- Produces: symmetric PlotArea scene overflow for vertical legend groups

- [ ] **Step 1: Write the failing legend-bounds test**

```rust
#[test]
fn plot_area_scene_contains_tall_right_legend() {
    let mut spec = temporal_spec(vec![0, 1]);
    spec.height = 18.0;
    spec.legend = LegendPos::Right;
    spec.legend_title = Some("metric".into());
    let template = spec.series[0].clone();
    spec.series = (0..6)
        .map(|i| {
            let mut series = template.clone();
            series.name = format!("series-{i}");
            series
        })
        .collect();

    let frame = compute(&spec, &TextMeasurer::new(DEFAULT_FONT).unwrap());
    let group_h = (spec.series.len() + 1) as f64 * LEGEND_ROW_H;
    let start_y = (frame.plot_top + frame.plot_bottom - group_h) / 2.0;
    assert!(start_y >= 0.0);
    assert!(start_y + group_h <= frame.scene_height);
    assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);
}
```

- [ ] **Step 2: Run the test and verify RED**

Run:

```bash
cargo test -p fulgur-chart layout::common::tests::plot_area_scene_contains_tall_right_legend -- --exact
```

Expected: fail because `start_y` is negative with the current unexpanded PlotArea frame.

- [ ] **Step 3: Compute and apply vertical legend overflow**

Before the size-mode match, calculate:

```rust
let vertical_legend_rows = if legend
    && matches!(spec.legend, LegendPos::Left | LegendPos::Right)
{
    spec.series.len() + usize::from(temporal_plot_right_legend_title(spec).is_some())
} else {
    0
};
let vertical_legend_overflow = ((vertical_legend_rows as f64 * LEGEND_ROW_H - spec.height)
    / 2.0)
    .max(0.0);
```

In the PlotArea branch, shift and expand symmetrically:

```rust
let plot_top = OUTER_PAD + title_band + vertical_legend_overflow;
let plot_bottom = plot_top + spec.height;
let scene_height = plot_bottom
    + X_LABEL_BAND
    + x_title_h
    + OUTER_PAD
    + legend_bottom
    + vertical_legend_overflow;
```

Leave the Canvas branch unchanged.

- [ ] **Step 4: Run layout and temporal rendering tests**

Run:

```bash
cargo test -p fulgur-chart layout::common::tests::
cargo test -p fulgur-chart --test render_vegalite_temporal_line
```

Expected: all pass, including unchanged dogfood snapshots.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/common.rs
git commit -m "fix(layout): contain tall PlotArea legends"
```

### Task 5: Verify, publish, and resolve review threads

**Files:**
- No source changes expected
- Update tracker: `fulgur-chart-be3`

**Interfaces:**
- Consumes: the four implementation commits and five review thread IDs
- Produces: pushed branch, 100% patch coverage, replied and resolved GitHub threads

- [ ] **Step 1: Run full local quality gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p fulgur-chart
cargo test -p chart-server
cargo check -p fulgur-chart --target wasm32-unknown-unknown
```

Expected: every command exits 0.

- [ ] **Step 2: Generate final committed-HEAD coverage**

```bash
cargo llvm-cov --workspace --locked --lcov --output-path /tmp/fulgur-chart-pr137-round4.info
```

Calculate executable changed lines from `git diff --unified=0 origin/main...HEAD -- '*.rs'`
and match them to LCOV `DA` records. Expected: 100.00%, with no missed changed lines.

- [ ] **Step 3: Rebase and push**

```bash
git pull --rebase
bd dolt push
git push
git status --short --branch
```

Expected: branch is clean and up to date with its upstream.

- [ ] **Step 4: Reply to and resolve exact review threads**

Use `addPullRequestReviewThreadReply` and then `resolveReviewThread` for:

- `PRRT_kwDOS-i3R86TaA4z` — line channel schema enums
- `PRRT_kwDOS-i3R86TaA42` — temporal/categorical schema split
- `PRRT_kwDOS-i3R86TaA46` — strict color field requirement
- `PRRT_kwDOS-i3R86TaA48` — overflow-safe temporal interpolation
- `PRRT_kwDOS-i3R86TaA4_` — tall vertical legend bounds

Each reply must name the concrete fix and its regression test.

- [ ] **Step 5: Re-fetch thread and CI state**

Run the bundled `fetch_comments.py --pr 137` workflow and verify zero unresolved threads.
Wait for CodeRabbit to finish, fetch threads again, and report any remaining CI job separately
from review completion.

- [ ] **Step 6: Close tracker**

```bash
bd close fulgur-chart-be3 --reason "Five PR #137 review fixes implemented, verified, pushed, replied, and resolved."
bd dolt push
```

