# PR #137 Twelfth Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` (recommended) or
> `superpowers:executing-plans` to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Align temporal color validation, legend activation, and marker-radius
safety checks with the rendering semantics exercised by PR #137.

**Architecture:** Keep strict structural checks separate from shared semantic
validation. Reuse the existing supported temporal-title predicate for common
legend activation, and make radius validation select only the raw radius source
that an eligible rendered marker would consume.

**Tech Stack:** Rust 2024, serde_json, fulgur-chart IR/layout/guard, cargo test,
cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global Constraints

- Preserve non-strict unknown-key tolerance.
- Preserve missing/null optional Vega-Lite color channels.
- Preserve named categorical legends and temporal PlotArea right legend titles.
- Preserve the existing radius limit of exactly `32768` pixels and exact error
  messages.
- Add each regression first and observe RED before production changes.
- Final executable changed-line coverage against `origin/main...HEAD` is 100%.
- Reply to and resolve only the three supplied review threads after the fixes
  are pushed and verified.
- Do not merge PR #137.

---

### Task 1: Require a field for a present temporal color channel

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- Consumes: parsed `encoding: &Map<String, Value>` and the existing
  `json_value_type` error formatter.
- Produces:
  `fn validate_temporal_color_channel(encoding: &Map<String, Value>) -> Result<(), String>`.

- [ ] **Step 1: Replace the strict-only regression with a cross-mode regression**

Change the existing test to assert the exact error in both modes:

```rust
#[test]
fn temporal_line_requires_color_field_in_both_modes() {
    let json = DOGFOOD_SHAPE.replace(r#""field":"metric","#, "");
    for strict in [false, true] {
        assert_eq!(
            vegalite::parse(&json, strict).unwrap_err(),
            "encoding.color.field is required",
            "strict={strict}"
        );
    }
}
```

Add cases proving a non-object color reports
`encoding.color must be an object`, while an absent or null color remains
accepted in both modes. Keep an unknown `encoding.color.futureOption` accepted
in non-strict and rejected in strict mode.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_requires_color_field_in_both_modes -- --exact
```

Expected: FAIL because non-strict parsing returns `Ok`.

- [ ] **Step 3: Add the shared temporal color validator**

Add beside `validate_temporal_color_scheme`:

```rust
fn validate_temporal_color_channel(encoding: &Map<String, Value>) -> Result<(), String> {
    let Some(value) = encoding.get("color").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let color = value
        .as_object()
        .ok_or_else(|| "encoding.color must be an object".to_string())?;
    match color.get("field") {
        Some(Value::String(_)) => Ok(()),
        Some(other) => Err(format!(
            "encoding.color.field must be a string, got {}",
            json_value_type(other)
        )),
        None => Err("encoding.color.field is required".to_string()),
    }
}
```

Call it at the start of the `if temporal_line` block, before
`validate_temporal_color_scheme(encoding)?`.

- [ ] **Step 4: Run focused and frontend suites**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_requires_color_field_in_both_modes -- --exact
cargo test -p fulgur-chart --test frontend_vegalite
```

Expected: both PASS.

- [ ] **Step 5: Commit the independently testable fix**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "fix(vegalite): require temporal color fields"
```

### Task 2: Scope legend-title activation to supported temporal legends

**Files:**
- Modify/test: `crates/fulgur-chart/src/layout/common.rs`

**Interfaces:**
- Consumes:
  `fn temporal_plot_right_legend_title(spec: &ChartSpec) -> Option<&str>`.
- Produces: a `has_legend` predicate that activates for a supported title or at
  least one non-empty series name.

- [ ] **Step 1: Add the unnamed categorical reviewer regression**

Add a test next to
`categorical_canvas_ignores_legend_title_without_changing_scene`:

```rust
#[test]
fn categorical_canvas_title_does_not_activate_unnamed_legend() {
    let mut baseline = make_bar_spec(3, 600.0);
    baseline.series[0].name.clear();
    baseline.legend = LegendPos::Right;
    let mut titled = baseline.clone();
    titled.legend_title = Some("unsupported title".into());

    let m = TextMeasurer::new(DEFAULT_FONT).unwrap();
    assert_eq!(compute(&titled, &m).plot_right, compute(&baseline, &m).plot_right);
    assert_eq!(
        crate::layout::build_scene(&titled, &m),
        crate::layout::build_scene(&baseline, &m)
    );
}
```

Keep the existing temporal dogfood assertions as the positive preservation
case.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test -p fulgur-chart --lib layout::common::tests::categorical_canvas_title_does_not_activate_unnamed_legend -- --exact
```

Expected: FAIL because the title activates a right legend and blank entry.

- [ ] **Step 3: Change only the activation predicate**

Replace the title half of `has_legend` with the supported-title helper:

```rust
fn has_legend(spec: &ChartSpec) -> bool {
    matches!(
        spec.legend,
        LegendPos::Top | LegendPos::Bottom | LegendPos::Left | LegendPos::Right
    ) && (temporal_plot_right_legend_title(spec).is_some()
        || spec.series.iter().any(|series| !series.name.is_empty()))
}
```

- [ ] **Step 4: Run common-layout tests**

Run:

```bash
cargo test -p fulgur-chart --lib layout::common::tests
cargo test -p fulgur-chart --test render_legend
cargo test -p fulgur-chart --test render_vegalite_temporal_line
```

Expected: all PASS, including temporal title rendering and categorical scene
stability.

- [ ] **Step 5: Commit the independently testable fix**

```bash
git add crates/fulgur-chart/src/layout/common.rs
git commit -m "fix(layout): scope legend title activation"
```

### Task 3: Validate only effective marker-radius sources

**Files:**
- Modify/test: `crates/fulgur-chart/src/guard.rs`
- Test: `crates/fulgur-chart/tests/render_line.rs`
- Test: `crates/fulgur-chart/tests/render_scatter.rs`
- Test: `crates/fulgur-chart/tests/render_bubble.rs`

**Interfaces:**
- Consumes: `ChartSpec`, `ChartKind`, series values/categories, point x/y/r,
  and `MAX_MARKER_RADIUS_PX`.
- Produces: chart-kind-aware `validate_marker_radii(&ChartSpec)` with unchanged
  `Result<(), String>` and error strings.

- [ ] **Step 1: Add guard-level reviewer reproductions**

Replace generic kind-independent radius tests with cases built through
`chartjs::parse`:

```rust
#[test]
fn unused_marker_radii_are_accepted() {
    for json in [
        r#"{"type":"bar","data":{"labels":["a"],"datasets":[{"data":[1],"pointRadius":1e40}]}}"#,
        r#"{"type":"pie","data":{"labels":["a"],"datasets":[{"data":[1],"pointRadius":1e40}]}}"#,
        r#"{"type":"scatter","data":{"datasets":[{"data":[{"x":1,"y":2,"r":1e40}]}]}}"#,
        r#"{"type":"bubble","data":{"datasets":[{"pointRadius":1e40,"data":[{"x":1,"y":2,"r":3}]}]}}"#,
    ] {
        let spec = chartjs::parse(json, false).unwrap();
        assert!(validate_spec(&spec, &default_limits()).is_ok(), "{json}");
    }
}
```

Add positive failure cases for an effective line/scatter `pointRadius`, bubble
fallback `pointRadius`, and bubble `point.r`. Add preservation cases showing an
invalid dataset radius is ignored when every finite bubble point supplies a
safe `r`, while it fails if any finite point omits `r`.

- [ ] **Step 2: Run the new guard tests and verify RED**

Run:

```bash
cargo test -p fulgur-chart --lib guard::tests::unused_marker_radii_are_accepted -- --exact
```

Expected: FAIL on the first currently scanned unused radius.

- [ ] **Step 3: Implement chart-kind-aware candidate selection**

Rewrite `validate_marker_radii` around two unchanged error helpers:

```rust
pub(crate) fn validate_marker_radii(spec: &ChartSpec) -> Result<(), String> {
    let unsupported = |radius: f64| !radius.is_finite() || radius > MAX_MARKER_RADIUS_PX;
    let point_radius_error =
        || Err("pointRadius must be finite and no greater than 32768".to_string());
    let point_r_error =
        || Err("point.r must be finite and no greater than 32768".to_string());

    match &spec.kind {
        ChartKind::Line => {
            for series in &spec.series {
                let reaches_marker = series
                    .values
                    .iter()
                    .take(spec.categories.len())
                    .any(|value| value.is_finite());
                if reaches_marker && series.point_radius.is_some_and(unsupported) {
                    return point_radius_error();
                }
            }
        }
        ChartKind::Scatter => {
            for series in &spec.series {
                let reaches_marker = series
                    .points
                    .iter()
                    .any(|point| point.x.is_finite() && point.y.is_finite());
                if reaches_marker && series.point_radius.is_some_and(unsupported) {
                    return point_radius_error();
                }
            }
        }
        ChartKind::Bubble => {
            for series in &spec.series {
                for point in series
                    .points
                    .iter()
                    .filter(|point| point.x.is_finite() && point.y.is_finite())
                {
                    if let Some(radius) = point.r {
                        if unsupported(radius) {
                            return point_r_error();
                        }
                    } else if series.point_radius.is_some_and(unsupported) {
                        return point_radius_error();
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}
```

Do not change layout radius fallback behavior or the fallible renderer call
sites.

- [ ] **Step 4: Run guard and renderer radius suites**

Run:

```bash
cargo test -p fulgur-chart --lib guard::tests
cargo test -p fulgur-chart --test render_line
cargo test -p fulgur-chart --test render_scatter
cargo test -p fulgur-chart --test render_bubble
```

Expected: all PASS with the same error across fallible SVG/PNG/WebP APIs for
effective oversized radii.

- [ ] **Step 5: Commit the independently testable fix**

```bash
git add crates/fulgur-chart/src/guard.rs crates/fulgur-chart/tests/render_line.rs crates/fulgur-chart/tests/render_scatter.rs crates/fulgur-chart/tests/render_bubble.rs
git commit -m "fix(guard): validate effective marker radii"
```

### Task 4: Review, verify, publish, and resolve

**Files:**
- Modify: `docs/superpowers/specs/2026-07-26-pr137-twelfth-review-fixes-design.md`
- Modify: `docs/superpowers/plans/2026-07-26-pr137-twelfth-review-fixes.md`
- External state: Bead `fulgur-chart-8nx`, PR #137 branch and review threads

**Interfaces:**
- Consumes: the three committed fixes and exact thread IDs
  `PRRT_kwDOS-i3R86T1swv`, `PRRT_kwDOS-i3R86T1sww`,
  `PRRT_kwDOS-i3R86T1swx`.
- Produces: pushed clean branch, pushed closed Bead, exact thread replies,
  resolved targets, zero unresolved threads, and terminal-green CI.

- [ ] **Step 1: Commit the approved design and plan**

```bash
git add docs/superpowers/specs/2026-07-26-pr137-twelfth-review-fixes-design.md docs/superpowers/plans/2026-07-26-pr137-twelfth-review-fixes.md
git commit -m "docs: plan twelfth PR review fixes"
```

- [ ] **Step 2: Rebase on current main and run all quality gates**

```bash
git fetch origin
git rebase origin/main
cargo fmt --all -- --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
git diff --check origin/main...HEAD
git show --check --oneline HEAD
```

Expected: clean rebase and every command exits 0.

- [ ] **Step 3: Measure committed-HEAD patch coverage**

Run workspace `cargo llvm-cov` using the repository's established LCOV
workflow, then compare executable Rust lines in `origin/main...HEAD`.

Expected: `100.00%` changed-line coverage and zero misses. Add focused tests
for any real uncovered branch; do not exclude production code.

- [ ] **Step 4: Update Beads and publish**

```bash
bd update fulgur-chart-8nx --notes "Round 12 implementation verified; include exact gates and coverage counts."
bd dolt push
git pull --rebase
git push
git status --short --branch
```

Expected: push succeeds and the branch is synchronized with its upstream.

- [ ] **Step 5: Reply to and resolve the exact threads**

Use `addPullRequestReviewThreadReply` for each target with its focused tests and
implementation summary. Only after a reply succeeds, call
`resolveReviewThread` for that same thread.

Expected: all three mutations return success; no unrelated thread is mutated.

- [ ] **Step 6: Fresh-fetch review state and watch CI**

Fresh-fetch all PR #137 review threads with pagination and verify all three
targets are resolved and the PR has zero unresolved current threads. Then run:

```bash
gh pr checks 137 --watch
```

Expected: all required checks reach terminal green; expected non-required
skips are reported as skips.

- [ ] **Step 7: Close and push the Bead, then verify final state**

```bash
bd close fulgur-chart-8nx --reason "All Round 12 review fixes verified, pushed, replied/resolved, zero unresolved threads, CI green."
bd dolt push
git push
git status --short --branch
```

Expected: Bead is closed and pushed; worktree is clean and up to date. Do not
merge PR #137.
