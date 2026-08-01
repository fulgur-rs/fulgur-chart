# Batched Categorical X-grid Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render visible categorical x-axis grid segments through one path, preserving chart output while restoring the memory gate.

**Architecture:** `layout::common::draw_frame` collects the grid segments for the visible category ticks into one SVG path before adding labels. The existing `Prim::Path` renderer draws the segments with the configured color and width; no public API or non-categorical layout changes.

**Tech Stack:** Rust, fulgur-chart Scene primitives, SVG renderer, insta snapshots, dhat membench.

## Global Constraints

- Only categorical x grids in `layout::common::draw_frame` may change.
- Use the existing auto-skip step for both labels and grid segments.
- Preserve `display`, `color`, `line_width`, and line-chart `offset` semantics.
- Do not update `membench_baseline.json`; `cargo bench ... -- --check` must pass.
- Regenerate committed deterministic SVG/PNG expectations only from the changed renderer output.

---

### Task 1: Batch categorical grid segments and prove the memory regression is removed

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs:781-832`
- Modify: `crates/fulgur-chart/tests/wasm_runtime.rs:69-133`
- Modify: `crates/fulgur-chart/tests/snapshots/*.snap`
- Modify: `crates/fulgur-chart/tests/golden/*.png`

**Interfaces:**
- Consumes: `Prim::Path { d: String, fill: Option<Color>, stroke: Option<Color>, stroke_width: f64 }`.
- Produces: one path containing `M {x} {plot_top} L {x} {plot_bottom}` for every visible categorical tick.

- [ ] **Step 1: Write the failing path-batching test**

  Replace the category-center assertion so a three-category bar chart expects exactly one
  `Prim::Path` with `fill: None`, the configured stroke color and width, and three
  `M ... L ...` vertical subpaths at the category centers. Update the auto-skip test
  to count the subpaths in this path and assert that count equals visible labels.

  ```rust
  let paths: Vec<_> = items.iter().filter_map(|item| match item {
      Prim::Path { d, fill: None, stroke: Some(stroke), stroke_width }
          if *stroke == grid && (*stroke_width - 2.5).abs() < 1e-9 => Some(d),
      _ => None,
  }).collect();
  assert_eq!(paths.len(), 1);
  assert_eq!(paths[0].matches("M ").count(), 3);
  ```

- [ ] **Step 2: Run the focused test to verify RED**

  Run: `cargo test -p fulgur-chart categorical_x_grid --lib`

  Expected: FAIL because the current implementation emits three `Prim::Line` items,
  not one `Prim::Path` with three segments.

- [ ] **Step 3: Emit one path for visible categorical ticks**

  In the category branch of `draw_frame`, collect each visible tick's existing x
  coordinate into one `String` using separate move/line subpaths. When
  `x_axis.grid.display` is true and the path is non-empty, push exactly one:

  ```rust
  items.push(Prim::Path {
      d: grid_path,
      fill: None,
      stroke: Some(grid_color),
      stroke_width: x_grid.line_width,
  });
  ```

  Keep the existing `Prim::Text` insertion for each visible label. Do not change the
  temporal branch or add a new public primitive.

- [ ] **Step 4: Run focused tests to verify GREEN**

  Run: `cargo test -p fulgur-chart categorical_x_grid --lib`

  Expected: PASS. The tests cover category centers, display false, line offset false,
  and auto-skip with one batched path.

- [ ] **Step 5: Regenerate rendering expectations and deterministic runtime values**

  Run: `INSTA_UPDATE=always cargo test -p fulgur-chart`

  Run: `UPDATE_GOLDEN=1 cargo test -p fulgur-chart --test golden_png`

  Run: `cargo test -p fulgur-chart --test wasm_runtime`

  Expected: snapshots and PNG goldens reflect the one-path SVG form; runtime SVG and
  linux-x86 PNG length/hash constants match the regenerated output.

- [ ] **Step 6: Verify quality gates and memory regression**

  Run: `cargo fmt --check`

  Run: `cargo clippy -p fulgur-chart --all-targets -- -D warnings`

  Run: `cargo test -p fulgur-chart`

  Run: `cargo bench -p fulgur-chart --bench membench --features dhat-heap --locked -- --check`

  Expected: all commands pass and `bar_small` remains within the existing 25% memory gate.

- [ ] **Step 7: Commit the focused CI fix**

  ```bash
  git add crates/fulgur-chart/src/layout/common.rs crates/fulgur-chart/tests/wasm_runtime.rs crates/fulgur-chart/tests/snapshots crates/fulgur-chart/tests/golden
  git commit -m "perf(chartjs): batch categorical x grids"
  ```
