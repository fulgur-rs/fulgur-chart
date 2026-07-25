# PR #137 Sixth Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the final three PR #137 review threads while preserving PlotArea geometry, render API compatibility and safety, and truthful temporal model metadata.

**Architecture:** Extend the existing measured PlotArea side-band calculation to all centered horizontal titles. Add limits-aware render entry points behind source-compatible default-limit wrappers. Derive temporal model step only from a uniform complete adjacent-delta sequence.

**Tech Stack:** Rust 2024, fulgur-chart layout/guard/render/model modules, cargo test, cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global Constraints

- Preserve requested PlotArea width and height, temporal tick coordinates, right-legend placement, and all Canvas geometry.
- Preserve every existing public render function signature and its default-limit behavior.
- Preserve raster pixel-area and WebP axis hard stops regardless of caller-supplied `InputLimits`.
- Keep explicit temporal ticks authoritative; report `step` only for a genuinely constant millisecond interval.
- Add each regression test before its production change and observe the expected failure.
- Final executable changed-line coverage against `origin/main...HEAD` must be 100%.

---

### Task 1: Contain long top-level titles in PlotArea scenes

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs`
- Test: `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`

**Interfaces:**
- Consumes: `spec.title`, `TITLE_FONT`, the existing centered x-title overflow
- Produces: one maximum centered-title side overflow used by PlotArea only

- [ ] **Step 1: Add the failing containment regression**

Add `plot_area_contains_long_centered_chart_title`. Mutate the temporal fixture
from width `720` to `24` and replace the top-level
`"qtest nightly trend"` title with a long unique title. Build both the frame
and scene with the same measurer. Find the `Prim::Text` with that content,
`Anchor::Middle`, and no rotation, then assert:

```rust
let half_extent = m.width(CHART_TITLE, size as f32) as f64 / 2.0;
assert!(x - half_extent >= 0.0);
assert!(x + half_extent <= scene.width);
assert_eq!(frame.plot_right - frame.plot_left, spec.width);
assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);
```

Extract the established right-legend checks from
`plot_area_contains_long_centered_x_axis_title` into a test helper accepting
`(&Scene, &Frame, &ChartSpec, &TextMeasurer)`. Call it from both long-title
tests. It must require every swatch and text anchor to begin at or to the right
of `frame.plot_right`, and every measured right edge to remain within
`scene.width`; do not duplicate the assertion block.

- [ ] **Step 2: Run focused RED**

```bash
cargo test -p fulgur-chart --test render_vegalite_temporal_line plot_area_contains_long_centered_chart_title -- --exact
```

Expected: failure because the top-level title extends outside the scene.

- [ ] **Step 3: Generalize the centered-title overflow**

In `common::compute`, calculate top-level chart-title overflow at `TITLE_FONT`
and centered x-axis-title overflow at its resolved font. Take their maximum:

```rust
let chart_title_side_overflow = spec
    .title
    .as_ref()
    .map(|title| {
        ((m.width(title, TITLE_FONT as f32) as f64 - spec.width) / 2.0).max(0.0)
    })
    .unwrap_or(0.0);
let x_axis_title_side_overflow = /* existing centered x-title calculation */;
let centered_title_side_overflow =
    chart_title_side_overflow.max(x_axis_title_side_overflow);
```

Compute it only for `SizeMode::PlotArea`, rename the existing x-specific
variable accordingly, and use
`OUTER_PAD + centered_title_side_overflow` for both PlotArea side-band maxima.
Do not alter drawing anchors, temporal tick generation, or the Canvas branch.

- [ ] **Step 4: Run GREEN and layout regressions**

```bash
cargo test -p fulgur-chart --test render_vegalite_temporal_line plot_area_contains_long_centered_chart_title -- --exact
cargo test -p fulgur-chart --test render_vegalite_temporal_line
cargo test -p fulgur-chart layout::common::tests::
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/layout/common.rs crates/fulgur-chart/tests/render_vegalite_temporal_line.rs
git commit -m "fix(layout): contain PlotArea chart titles"
```

### Task 2: Preserve caller-supplied limits in custom-font rendering

**Files:**
- Modify: `crates/fulgur-chart/src/render.rs`
- Modify: `crates/fulgur-chart/src/raster_direct.rs`
- Test: `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`

**Interfaces:**
- Adds: `render_chart_with_font_and_limits`
- Adds: `render_chart_to_png_with_limits`
- Adds: `render_chart_to_webp_with_limits`
- Preserves: all existing render signatures and default-limit behavior

- [ ] **Step 1: Add failing API and behavior tests**

Extend imports for the three new functions. Create a temporal PlotArea spec
whose requested width equals `InputLimits::default().max_dimension_px`, so its
outer scene exceeds the default dimension limit. Define relaxed limits with
`max_dimension_px` increased enough to contain the measured scene.

First prove `validate_spec_with_measurer(&spec, &relaxed, &m)` succeeds and
each existing API still returns a `scene width` error. Then assert the new SVG
API returns SVG and the new PNG/WebP APIs return non-empty bytes. Pass scale
`0.01` to raster variants so this semantic test does not allocate a large
pixmap.

Add a stricter supplied-limits case proving each new variant returns the same
scene-bound error when its explicit limits are too small.

- [ ] **Step 2: Run compile/behavior RED**

```bash
cargo test -p fulgur-chart --test render_vegalite_temporal_line custom_font_render_paths_preserve_caller_supplied_limits -- --exact
```

Expected: compilation fails because the three limits-aware APIs do not exist.

- [ ] **Step 3: Add the limits-aware SVG wrapper**

Implement:

```rust
pub fn render_chart_with_font_and_limits(
    spec: &crate::ir::ChartSpec,
    font_bytes: &[u8],
    limits: &crate::guard::InputLimits,
) -> Result<String, String>
```

Move the existing font parsing, measured PlotArea validation, family-name
resolution, and rendering into it. Make `render_chart_with_font` delegate with
`&InputLimits::default()`.

- [ ] **Step 4: Add limits-aware raster wrappers without weakening hard stops**

Implement:

```rust
pub fn render_chart_to_png_with_limits(
    spec: &crate::ir::ChartSpec,
    scale: f32,
    font_bytes: &[u8],
    limits: &crate::guard::InputLimits,
) -> Result<Vec<u8>, String>

pub fn render_chart_to_webp_with_limits(
    spec: &crate::ir::ChartSpec,
    scale: f32,
    font_bytes: &[u8],
    limits: &crate::guard::InputLimits,
) -> Result<Vec<u8>, String>
```

Extract private helpers taking both limits and PNG compression as necessary.
Existing `render_chart_to_png`, `render_chart_to_png_with`, and
`render_chart_to_webp` must delegate with `InputLimits::default()`. Only the
measured PlotArea-scene guard receives caller limits; `PNG_LIMITS` and
`WEBP_LIMITS` remain unchanged and are still applied immediately before
pixmap allocation.

- [ ] **Step 5: Run GREEN and render regressions**

```bash
cargo test -p fulgur-chart --test render_vegalite_temporal_line custom_font_render_paths_preserve_caller_supplied_limits -- --exact
cargo test -p fulgur-chart --test render_vegalite_temporal_line
cargo test -p fulgur-chart render::tests::
cargo test -p fulgur-chart raster_direct::tests::
```

Expected: all pass with existing default rejection and raster hard-stop tests
unchanged.

- [ ] **Step 6: Commit**

```bash
git add crates/fulgur-chart/src/render.rs crates/fulgur-chart/src/raster_direct.rs crates/fulgur-chart/tests/render_vegalite_temporal_line.rs
git commit -m "fix(render): preserve caller input limits"
```

### Task 3: Report temporal model step only for uniform ticks

**Files:**
- Modify: `crates/fulgur-chart/src/model.rs`
- Test: `crates/fulgur-chart/src/model.rs`
- Test: `crates/fulgur-chart/tests/inspect_model.rs`

**Interfaces:**
- Consumes: the complete emitted `&[TemporalTick]`
- Produces: `AxisModel.step = Some(delta)` only when every adjacent delta is equal

- [ ] **Step 1: Add failing unit regressions**

Inside `model.rs` tests, construct `TemporalTick` vectors directly:

```rust
let fixed = ticks(&[0, 86_400_000, 172_800_000]);
assert_eq!(temporal_axis(&[], &fixed).step, Some(86_400_000.0));

let calendar = ticks(&[0, 31 * DAY, (31 + 28) * DAY]);
assert_eq!(temporal_axis(&[], &calendar).step, None);
```

Use valid `i64` constants and simple labels. Also assert zero/one tick yields
`None`, two ticks yields its single delta, and a reversed uniform sequence
preserves the negative step. Update
`temporal_line_model_uses_scene_dimensions_and_temporal_axis` to calculate its
expected step from all windows, not only the first.

- [ ] **Step 2: Run focused RED**

```bash
cargo test -p fulgur-chart model::tests::temporal_axis_reports_only_uniform_step -- --exact
```

Expected: irregular calendar-like deltas incorrectly return the first delta.

- [ ] **Step 3: Check every delta with widened subtraction**

Add a private helper or implement locally:

```rust
let step = ticks.windows(2).next().and_then(|first| {
    let expected =
        i128::from(first[1].unix_millis) - i128::from(first[0].unix_millis);
    ticks
        .windows(2)
        .all(|window| {
            i128::from(window[1].unix_millis) - i128::from(window[0].unix_millis)
                == expected
        })
        .then_some(expected as f64)
});
```

Assign this result to `AxisModel.step`. Leave labels, min/max, explicit ticks,
and tick counts unchanged.

- [ ] **Step 4: Run GREEN and model regressions**

```bash
cargo test -p fulgur-chart model::tests::temporal_axis_reports_only_uniform_step -- --exact
cargo test -p fulgur-chart --test inspect_model
cargo test -p fulgur-chart model::tests::
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/model.rs crates/fulgur-chart/tests/inspect_model.rs
git commit -m "fix(model): omit irregular temporal step"
```

### Task 4: Verify, publish, and resolve the final threads

**Files:**
- No source changes expected
- Update tracker: `fulgur-chart-8nx`

**Interfaces:**
- Consumes: Tasks 1-3 and unresolved thread IDs
- Produces: 100% coverage, pushed branch, zero unresolved threads, green checks, closed tracker

- [ ] **Step 1: Run all local gates**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p fulgur-chart --locked
cargo test -p chart-server --locked
cargo check -p fulgur-chart --target wasm32-unknown-unknown --locked
git diff --check
```

- [ ] **Step 2: Regenerate final committed-HEAD coverage**

```bash
cargo llvm-cov --workspace --locked --lcov --output-path /tmp/fulgur-chart-pr137-round6.info
git diff --unified=0 origin/main...HEAD -- '*.rs'
```

Use the same repository/CI aggregation method recorded in
`task-4-report.md`. Expected: 100.00%, misses 0. Add a behavior-focused test
and regenerate if any executable changed line is missed.

- [ ] **Step 3: Rebase, push, reply, and resolve**

Update `fulgur-chart-8nx` with gates and coverage, then:

```bash
git pull --rebase
bd dolt push
git push
```

Reply via `addPullRequestReviewThreadReply`, then resolve only after successful
reply:

- `PRRT_kwDOS-i3R86Txcpr` — top-level chart-title containment.
- `PRRT_kwDOS-i3R86Txcps` — limits-aware SVG/PNG/WebP APIs with safe defaults.
- `PRRT_kwDOS-i3R86Txcpt` — uniform-only temporal model step.

Each reply names the commit and exact regression tests.

- [ ] **Step 4: Verify zero unresolved and green CI**

Run the bundled `fetch_comments.py` workflow and require zero unresolved
threads. Then:

```bash
gh pr checks 137 --watch
```

Fix only in-scope failures, with focused tests and regenerated coverage.

- [ ] **Step 5: Close Beads and perform the mandatory final push**

```bash
bd close fulgur-chart-8nx --reason "All PR #137 review fixes implemented, verified at 100% patch coverage, pushed, replied, and resolved with green checks."
git pull --rebase
bd dolt push
git push
git status --short --branch
```

Expected: bead closed/pushed, branch clean and equal to upstream. Keep the
active PR worktree.
