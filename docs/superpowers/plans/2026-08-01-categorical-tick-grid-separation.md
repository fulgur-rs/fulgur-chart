# Categorical Tick and Grid Compatibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Preserve a categorical grid segment for every auto-selected tick, including ticks whose label text is empty.

**Architecture:** `draw_frame` will derive the selected category indices once from its existing auto-skip step. The batched grid path consumes every selected index, while text emission additionally requires a non-empty label. This retains a single `Prim::Path` for categorical grid lines.

**Tech Stack:** Rust, `fulgur-chart` layout unit tests, Cargo.

## Global Constraints

- Match Chart.js categorical tick behavior: grid segments follow selected ticks, not only visible text.
- Keep the current automatic step calculation and line/bar coordinate functions unchanged.
- Keep categorical grid segments batched in one `Prim::Path`.
- Add a behavior-level regression test before production code changes.

---

### Task 1: Separate selected ticks from non-empty label rendering

**Files:**

- Modify: `crates/fulgur-chart/src/layout/common.rs:780-842`
- Test: `crates/fulgur-chart/src/layout/common.rs:1848-1887`

**Interfaces:**

- Consumes: `spec.categories`, the computed `step`, and `x_axis.grid` in `draw_frame`.
- Produces: one batched `Prim::Path` subpath per selected category tick, with `Prim::Text` only for selected non-empty labels.

- [ ] **Step 1: Write the failing regression test**

Add a `categorical_x_grid_keeps_tick_for_empty_label` unit test. Construct a three-category bar spec, replace the middle category with an empty string, call `draw_frame`, and assert that the grid path has exactly three `M ` subpaths while only two centered text primitives are present.

```rust
assert_eq!(grid_path.matches("M ").count(), 3);
assert_eq!(label_count, 2);
```

- [ ] **Step 2: Run the regression test and verify it fails**

Run:

```bash
cargo test -p fulgur-chart categorical_x_grid_keeps_tick_for_empty_label
```

Expected: FAIL because the current combined `cat.is_empty() || i % step != 0` guard omits the middle grid subpath.

- [ ] **Step 3: Implement the minimal separation**

Replace the combined guard with a selected-tick guard, append a grid segment for every selected index, and use a second `if cat.is_empty() { continue; }` only before pushing `Prim::Text`.

```rust
if i % step != 0 {
    continue;
}
// append grid segment
if cat.is_empty() {
    continue;
}
// push Prim::Text
```

- [ ] **Step 4: Run focused and crate tests**

Run:

```bash
cargo test -p fulgur-chart categorical_x_grid_keeps_tick_for_empty_label
cargo test -p fulgur-chart
```

Expected: both commands PASS, including the existing dense-category auto-skip test.

- [ ] **Step 5: Run quality checks**

Run:

```bash
cargo fmt --check
cargo clippy -p fulgur-chart --all-targets -- -D warnings
cargo test -p fulgur-chart --test golden
cargo bench -p fulgur-chart --bench membench --features dhat-heap --locked -- --check
```

Expected: all commands PASS with the committed benchmark baseline unchanged.

- [ ] **Step 6: Commit the implementation**

```bash
git add crates/fulgur-chart/src/layout/common.rs docs/superpowers/plans/2026-08-01-categorical-tick-grid-separation.md
git commit -m "fix(chartjs): retain grid for empty category tick"
```
