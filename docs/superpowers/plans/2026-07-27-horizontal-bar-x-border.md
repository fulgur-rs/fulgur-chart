# Horizontal Bar X-Axis Border Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Draw the configured Chart.js x-axis border along the bottom of horizontal bar charts.

**Architecture:** Keep the transposed horizontal-bar layout self-contained in `layout::bar::build_horizontal`. Emit one additional `Prim::Line` for `spec.x_axis.border` at `plot_bottom`, alongside the existing independent y-axis border, without refactoring shared frame rendering.

**Tech Stack:** Rust 2024, fulgur-chart IR and scene primitives, insta snapshots, cargo test, cargo clippy, cargo llvm-cov, Beads

## Global Constraints

- Match the observed Chart.js 4.5.1 horizontal x scale: `position: "bottom"` with its border at the plot bottom.
- Honor `spec.x_axis.border.display`, `color`, `width`, and `dash`.
- Fall back to `spec.theme.text_color` when the x-axis border color is absent.
- Preserve the existing y-axis border, grid, tick, title, bar geometry, and legend behavior.
- Do not refactor `common::draw_frame` or modify vertical bar, scatter, or boxplot rendering.
- Add regression tests before production code and observe the expected RED failures.
- Final executable changed-line coverage against `origin/main...HEAD` must be 100%.

---

### Task 1: Add test-driven horizontal x-axis border rendering

**Files:**
- Modify/test: `crates/fulgur-chart/src/layout/bar.rs:371-386`
- Modify/test: `crates/fulgur-chart/src/layout/bar.rs:802-963`

**Interfaces:**
- Consumes: `ChartSpec.x_axis.border: AxisBorder`, `Prim::Line`, and the existing `plot_left`, `plot_right`, `plot_bottom`, and `ink` values in `build_horizontal`
- Produces: one bottom horizontal `Prim::Line` when `x_axis.border.display` is true; no new public API

- [ ] **Step 1: Add a failing style-propagation test**

Add this test to `horizontal_axis_style_tests`:

```rust
#[test]
fn horizontal_x_border_style_reaches_bottom_baseline() {
    let spec = parse(
        r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
            "options":{"indexAxis":"y","scales":{"x":{"border":{
                "color":"#123456","width":3,"dash":[5,2]
            }}}}}"##,
    );
    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    let scene = build(&spec, &m);
    let baseline = scene.items.iter().find_map(|p| match p {
        Prim::Line {
            x1,
            x2,
            y1,
            y2,
            stroke,
            stroke_width,
            dash,
        } if (*y1 - *y2).abs() < 0.01
            && (*x1 - *x2).abs() > 1.0
            && stroke.r == 0x12
            && stroke.g == 0x34
            && stroke.b == 0x56 =>
        {
            Some((*stroke_width, dash.as_slice()))
        }
        _ => None,
    });
    let (width, dash) = baseline.expect("x_axis.border should draw a bottom baseline");
    assert!((width - 3.0).abs() < 1e-9);
    assert_eq!(dash, &[5.0, 2.0]);
}
```

- [ ] **Step 2: Add a failing display-control test**

Add this test to the same module. It tests `display` as one behavior by comparing otherwise identical visible and hidden configurations:

```rust
#[test]
fn horizontal_x_border_display_controls_bottom_baseline() {
    fn count_marker_border(scene: &Scene) -> usize {
        scene
            .items
            .iter()
            .filter(|p| {
                matches!(p,
                    Prim::Line { x1, x2, y1, y2, stroke, .. }
                        if (*y1 - *y2).abs() < 0.01
                            && (*x1 - *x2).abs() > 1.0
                            && stroke.r == 0x22
                            && stroke.g == 0x44
                            && stroke.b == 0x66
                )
            })
            .count()
    }

    let visible = scene_for(
        r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
            "options":{"indexAxis":"y","scales":{"x":{"border":{
                "display":true,"color":"#224466"
            }}}}}"##,
    );
    let hidden = scene_for(
        r##"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[10,20]}]},
            "options":{"indexAxis":"y","scales":{"x":{"border":{
                "display":false,"color":"#224466"
            }}}}}"##,
    );

    assert_eq!(count_marker_border(&visible), 1);
    assert_eq!(count_marker_border(&hidden), 0);
}
```

- [ ] **Step 3: Run the focused tests and verify RED**

Run:

```bash
cargo test -p fulgur-chart layout::bar::horizontal_axis_style_tests::horizontal_x_border_
```

Expected: both new tests fail because `build_horizontal` currently emits no horizontal line using `x_axis.border`.

- [ ] **Step 4: Add the minimal production rendering**

Replace the obsolete “do not draw the bottom border” comment before the existing y-axis border with this x-axis block, then retain the y-axis block unchanged:

```rust
// 3. 底辺の値軸線(X のボーダー)。x_axis.border が水平線を支配する。
let x_border = &spec.x_axis.border;
if x_border.display {
    let border_color = x_border.color.unwrap_or(ink);
    items.push(Prim::Line {
        x1: plot_left,
        y1: plot_bottom,
        x2: plot_right,
        y2: plot_bottom,
        stroke: border_color,
        stroke_width: x_border.width,
        dash: x_border.dash.clone(),
    });
}

// 3a. 左軸線(カテゴリ軸=Y のボーダー)。y_axis.border が縦のカテゴリ軸線を支配する。
```

Renumber the following tick comment from `3b` to `3c`; do not alter tick behavior.

- [ ] **Step 5: Run focused tests and verify GREEN**

Run:

```bash
cargo test -p fulgur-chart layout::bar::horizontal_axis_style_tests
```

Expected: all horizontal-axis style tests pass, including style propagation and the visible/hidden comparison.

- [ ] **Step 6: Format and commit the behavior**

Run:

```bash
cargo fmt --all
git add crates/fulgur-chart/src/layout/bar.rs
git commit -m "fix(chartjs): draw horizontal bar x-axis border"
```

Expected: one commit containing only the tests and minimal `build_horizontal` change.

---

### Task 2: Accept only the expected horizontal snapshot changes

**Files:**
- Test: `crates/fulgur-chart/tests/render_bar.rs`
- Test: `crates/fulgur-chart/tests/render_stacked_bar.rs`
- Modify: `crates/fulgur-chart/tests/snapshots/render_bar__horizontal_bar_snapshot.snap`
- Modify: `crates/fulgur-chart/tests/snapshots/render_stacked_bar__horizontal_stacked_snapshot.snap`

**Interfaces:**
- Consumes: the new default bottom-border `Prim::Line` from Task 1
- Produces: byte-exact SVG snapshots that include one bottom baseline and no unrelated output changes

- [ ] **Step 1: Run the affected snapshots without accepting**

Run:

```bash
cargo test -p fulgur-chart --test render_bar horizontal_bar_snapshot
cargo test -p fulgur-chart --test render_stacked_bar horizontal_stacked_snapshot
```

Expected: both snapshot tests fail and produce `.snap.new` files whose only semantic addition is one horizontal `<line>` spanning the plot bottom with default text color, width `1`, and no dash.

- [ ] **Step 2: Review the pending snapshot output**

Run:

```bash
git diff --no-index crates/fulgur-chart/tests/snapshots/render_bar__horizontal_bar_snapshot.snap crates/fulgur-chart/tests/snapshots/render_bar__horizontal_bar_snapshot.snap.new
git diff --no-index crates/fulgur-chart/tests/snapshots/render_stacked_bar__horizontal_stacked_snapshot.snap crates/fulgur-chart/tests/snapshots/render_stacked_bar__horizontal_stacked_snapshot.snap.new
```

Expected: each `git diff --no-index` exits `1` because the snapshots differ, and
the diff contains only one added horizontal `<line>` in the serialized SVG. No
text, rectangle, title, legend, grid, tick, or existing coordinate changes are
allowed. Stop and investigate if any unrelated snapshot content changes.

- [ ] **Step 3: Accept and rerun the focused snapshots**

Run:

```bash
INSTA_UPDATE=always cargo test -p fulgur-chart --test render_bar horizontal_bar_snapshot
INSTA_UPDATE=always cargo test -p fulgur-chart --test render_stacked_bar horizontal_stacked_snapshot
cargo test -p fulgur-chart --test render_bar
cargo test -p fulgur-chart --test render_stacked_bar
```

Expected: both suites pass and no `.snap.new` files remain.

- [ ] **Step 4: Commit the snapshot updates**

Run:

```bash
git add crates/fulgur-chart/tests/snapshots/render_bar__horizontal_bar_snapshot.snap crates/fulgur-chart/tests/snapshots/render_stacked_bar__horizontal_stacked_snapshot.snap
git commit -m "test(chartjs): update horizontal bar border snapshots"
```

Expected: a snapshot-only commit containing exactly the two expected files.

---

### Task 3: Verify, close Bead, and push all work

**Files:**
- Verify: all committed branch changes
- Update: Bead `fulgur-chart-bni`

**Interfaces:**
- Consumes: the implementation and snapshot commits from Tasks 1 and 2
- Produces: formatted, tested, clippy-clean code; 100% executable patch coverage; closed and synchronized Beads state; pushed branch

- [ ] **Step 1: Run local quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Expected: every command exits 0 with no warnings. If formatting changes a file, commit it with the owning task and rerun all gates.

- [ ] **Step 2: Generate final committed-HEAD coverage**

Run:

```bash
cargo llvm-cov --workspace --locked --lcov --output-path /tmp/fulgur-chart-bni.info
git diff --unified=0 origin/main...HEAD -- '*.rs'
```

Parse every added executable Rust line from the zero-context diff, including the
new unit-test bodies. Exclude only blank lines, comments, attributes, and
braces-only lines. Match each remaining `path:line` to its LCOV
`DA:<line>,<hits>` record. Expected: `100.00%`, with no missed changed
executable line. If any line is missed, add a behavior-focused test, observe the
relevant RED when possible, rerun the affected test, commit, and regenerate
coverage from the new committed `HEAD`.

- [ ] **Step 3: Record completion and close the Bead**

Run:

```bash
bd update fulgur-chart-bni --notes "Chart.js 4.5.1 の horizontal x scale と同様に底辺 border を描画。display/color/width/dash の回帰テスト、horizontal snapshots、workspace tests/clippy、100% patch coverage を確認。"
bd close fulgur-chart-bni --reason "Horizontal bar x-axis border parity implemented and verified."
```

Expected: `fulgur-chart-bni` is closed with verification evidence.

- [ ] **Step 4: Synchronize and push**

Before pushing, report that the branch and Beads state will be published. Then run:

```bash
git pull --rebase
bd dolt push
git push -u origin feat/bni-horizontal-x-border
git status --short --branch
```

Expected: the branch push and Beads push succeed; status is clean and shows `feat/bni-horizontal-x-border` up to date with its origin branch. Do not create a pull request unless separately requested.
