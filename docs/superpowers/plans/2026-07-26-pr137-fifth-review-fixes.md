# PR #137 Fifth Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the three current PR #137 review threads by making recognized temporal title and view errors mode-independent and keeping long centered x-axis titles inside PlotArea scenes.

**Architecture:** Keep strict mode responsible for unknown-key allow-lists, but move supported-value semantics into shared runtime readers used by both modes. Extend the existing PlotArea frame calculation with measured horizontal x-title overflow, without changing requested plot dimensions, temporal coordinates, right-legend placement, or Canvas layout.

**Tech Stack:** Rust 2024, serde_json, fulgur-chart scene/layout primitives, cargo test, cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global Constraints

- Preserve RFC 3339-only temporal input support.
- Preserve missing and JSON `null` channel-title fallback to the field name.
- Preserve non-strict tolerance of unknown channel, config, and view keys.
- Preserve strict unknown-key validation.
- Preserve requested PlotArea width and height, right-legend placement, temporal tick positions, and all Canvas geometry.
- Add each regression test before its production change and observe the expected failure.
- Use the same English error for recognized invalid values in strict and non-strict modes.
- Final executable changed-line coverage against `origin/main...HEAD` must be 100%.

---

### Task 1: Validate temporal channel titles in both parsing modes

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- Consumes: `encoding.{x,y,color}.title`
- Produces: `Result<String, String>` with field-name fallback or an exact typed error

- [ ] **Step 1: Write failing invalid-title regression tests**

Add a table-driven test covering all three supported temporal channels and every non-string, non-null JSON kind:

```rust
#[test]
fn temporal_line_rejects_invalid_channel_titles_in_both_modes() {
    for (channel, original) in [
        ("x", r#""title":"date""#),
        ("y", r#""title":"subtests""#),
        ("color", r#""title":"metric""#),
    ] {
        for (replacement, value_type) in [
            ("42", "number"),
            ("true", "boolean"),
            ("{}", "object"),
            ("[]", "array"),
        ] {
            let json = DOGFOOD_SHAPE.replace(
                original,
                &format!(r#""title":{replacement}"#),
            );
            let expected =
                format!("encoding.{channel}.title must be a string, got {value_type}");
            for strict in [false, true] {
                assert_eq!(
                    vegalite::parse(&json, strict).unwrap_err(),
                    expected,
                    "channel={channel}, strict={strict}"
                );
            }
        }
    }
}
```

- [ ] **Step 2: Write fallback-semantics regression tests**

Add a second table that replaces each title with JSON `null` or removes the title member. Parse in both modes and assert:

```rust
assert_eq!(spec.x_axis.title.as_ref().unwrap().text, "timestamp");
assert_eq!(spec.y_axis.title.as_ref().unwrap().text, "value");
assert_eq!(spec.legend_title.as_deref(), Some("metric"));
```

Construct one mutation at a time so the expected fallback can be attributed to the selected channel. Also retain a string-title case to prove strings, including `""`, are not replaced by the field name.

- [ ] **Step 3: Run focused tests and verify RED**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_rejects_invalid_channel_titles_in_both_modes -- --exact
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_channel_titles_preserve_string_and_field_fallbacks -- --exact
```

Expected: the invalid-title test fails in non-strict mode because `channel_title` silently falls back to the field name. The fallback test should pass or expose an accidental semantic difference that must be preserved by the implementation.

- [ ] **Step 4: Make the shared title reader semantic and fallible**

Change `channel_title` to:

```rust
fn channel_title(
    encoding: &Map<String, Value>,
    name: &str,
    fallback_field: &str,
) -> Result<String, String> {
    let title = encoding
        .get(name)
        .and_then(Value::as_object)
        .and_then(|channel| channel.get("title"));
    match title {
        None | Some(Value::Null) => Ok(fallback_field.to_owned()),
        Some(Value::String(value)) => Ok(value.clone()),
        Some(value) => Err(format!(
            "encoding.{name}.title must be a string, got {}",
            json_value_type(value)
        )),
    }
}
```

Before constructing `ChartSpec`, resolve only the titles that the temporal-line path consumes:

```rust
let x_axis_title = temporal_line
    .then(|| channel_title(encoding, "x", x_field.as_deref().unwrap_or_default()))
    .transpose()?;
let y_axis_title = temporal_line
    .then(|| channel_title(encoding, "y", y_field.as_deref().unwrap_or_default()))
    .transpose()?;
let legend_title = if temporal_line {
    color_field
        .as_deref()
        .map(|field| channel_title(encoding, "color", field))
        .transpose()?
} else {
    None
};
```

Map `x_axis_title` and `y_axis_title` into `AxisTitle` values in the existing `ChartSpec` constructor and assign `legend_title` directly. Do not weaken or remove strict allow-list checks.

- [ ] **Step 5: Run the frontend Vega-Lite test target**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite
```

Expected: all tests pass, including exact errors in both modes and strict unknown-key coverage.

- [ ] **Step 6: Commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "fix(vegalite): validate temporal channel titles"
```

### Task 2: Validate temporal `config.view` semantics in both modes

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- Consumes: optional `config.view` and optional `config.view.stroke`
- Produces: `config.view must be an object` or `config.view.stroke must be null`

- [ ] **Step 1: Write failing recognized-value tests**

Add a table-driven test for both parse modes:

```rust
#[test]
fn temporal_line_rejects_invalid_view_config_in_both_modes() {
    for (json, expected) in [
        (
            DOGFOOD_SHAPE.replace(
                r#""view":{"stroke":null}"#,
                r#""view":42"#,
            ),
            "config.view must be an object",
        ),
        (
            DOGFOOD_SHAPE.replace(
                r#""stroke":null"#,
                r##""stroke":"#ddd""##,
            ),
            "config.view.stroke must be null",
        ),
    ] {
        for strict in [false, true] {
            assert_eq!(
                vegalite::parse(&json, strict).unwrap_err(),
                expected,
                "strict={strict}"
            );
        }
    }
}
```

Extend the stroke case table with number, boolean, object, and array values so every non-null JSON kind is covered.

- [ ] **Step 2: Write accepted-semantics and mode-boundary tests**

Add cases proving missing and JSON `null` view configuration are accepted in both modes:

```rust
for json in [
    DOGFOOD_SHAPE.replace(r#""view":{"stroke":null},"#, ""),
    DOGFOOD_SHAPE.replace(r#""view":{"stroke":null}"#, r#""view":null"#),
] {
    for strict in [false, true] {
        vegalite::parse(&json, strict).unwrap();
    }
}
```

Add a non-strict-only assertion that `{"stroke":null,"futureOption":true}` remains accepted, while the existing strict `config.view.typo` case remains rejected.

- [ ] **Step 3: Run focused tests and verify RED**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_rejects_invalid_view_config_in_both_modes -- --exact
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_view_config_preserves_null_missing_and_mode_boundaries -- --exact
```

Expected: the invalid-view test fails in non-strict mode because `temporal_axis_grid` currently ignores `config.view`; acceptance and strict allow-list tests establish the compatibility boundary.

- [ ] **Step 4: Extract shared view semantic validation**

Add a helper next to `temporal_axis_grid`:

```rust
fn validate_temporal_view(top: &Map<String, Value>) -> Result<(), String> {
    let Some(view) = top
        .get("config")
        .and_then(Value::as_object)
        .and_then(|config| config.get("view"))
        .filter(|value| !value.is_null())
    else {
        return Ok(());
    };
    let view = view
        .as_object()
        .ok_or_else(|| "config.view must be an object".to_string())?;
    if view.get("stroke").is_some_and(|stroke| !stroke.is_null()) {
        return Err("config.view.stroke must be null".to_string());
    }
    Ok(())
}
```

Call `validate_temporal_view(top)?` in the temporal-line runtime path before `ChartSpec` construction, alongside the shared axis/config semantic readers.

In `check_line_keys`, retain:

- `config` object validation;
- `config` and `config.view` allow-list validation;
- `config.view` object validation needed before `check_line_object`.

Remove only the duplicate strict `stroke` value check after the shared helper is guaranteed to have run. Unknown view keys must still fail only in strict mode.

- [ ] **Step 5: Run the frontend Vega-Lite test target**

Run:

```bash
cargo test -p fulgur-chart --test frontend_vegalite
```

Expected: all tests pass with identical recognized-value errors in both modes and unchanged strict unknown-key behavior.

- [ ] **Step 6: Commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "fix(vegalite): validate temporal view config"
```

### Task 3: Contain centered x-axis titles in PlotArea scenes

**Files:**
- Modify: `crates/fulgur-chart/src/layout/common.rs`
- Test: `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`

**Interfaces:**
- Consumes: resolved x-axis title font and measured title width
- Produces: minimum symmetric PlotArea side bands sufficient for a centered title

- [ ] **Step 1: Write the failing scene-containment test**

Add a narrow PlotArea fixture with a long x-axis title:

```rust
#[test]
fn plot_area_contains_long_centered_x_axis_title() {
    const X_TITLE: &str = "a very long centered temporal x axis title";
    let json = fixture()
        .replace(r#""width": 640"#, r#""width": 24"#)
        .replace(
            r#""title": "date""#,
            &format!(r#""title": "{X_TITLE}""#),
        );
    let spec = vegalite::parse(&json, true).unwrap();
    let m = measurer();
    let scene = line::build(&spec, &m);
    let (x, size) = scene
        .items
        .iter()
        .find_map(|item| match item {
            Prim::Text {
                x,
                size,
                content,
                rotate_deg: None,
                anchor: Anchor::Middle,
                ..
            } if content == X_TITLE => Some((*x, *size)),
            _ => None,
        })
        .expect("centered x-axis title");
    let half_extent = m.width(X_TITLE, size as f32) as f64 / 2.0;

    assert!(x - half_extent >= 0.0);
    assert!(x + half_extent <= scene.width);
}
```

Use the existing scene/frame helpers to additionally assert:

```rust
assert_eq!(frame.plot_right - frame.plot_left, spec.width);
assert_eq!(frame.plot_bottom - frame.plot_top, spec.height);
```

For the fixture's right legend, collect swatch/text x-coordinates and assert they remain at or to the right of `frame.plot_right` and within `scene.width`.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
cargo test -p fulgur-chart --test render_vegalite_temporal_line plot_area_contains_long_centered_x_axis_title -- --exact
```

Expected: fail because the title's left edge is negative or its right edge exceeds the current scene width.

- [ ] **Step 3: Measure required PlotArea side overflow**

Before the size-mode match in `layout/common.rs`, compute:

```rust
let centered_x_title_side_overflow = if matches!(spec.size_mode, SizeMode::PlotArea) {
    spec.x_axis
        .title
        .as_ref()
        .filter(|title| matches!(title.align, AxisTitleAlign::Center))
        .map(|title| {
            let font = title.font_size.unwrap_or(spec.theme.font_size * 1.1);
            ((m.width(&title.text, font as f32) as f64 - spec.width) / 2.0).max(0.0)
        })
        .unwrap_or(0.0)
} else {
    0.0
};
```

Import or fully qualify `AxisTitleAlign` consistently with the surrounding module.

- [ ] **Step 4: Apply the minimum side bands only to PlotArea**

Update only the `SizeMode::PlotArea` branch:

```rust
let required_title_side_band = OUTER_PAD + centered_x_title_side_overflow;
let plot_left = (OUTER_PAD + y_axis_w)
    .max(temporal_edge_pad_left)
    .max(required_title_side_band);
let plot_top = OUTER_PAD + title_band + plot_area_vertical_overflow;
let plot_right = plot_left + spec.width;
let plot_bottom = plot_top + spec.height;
let trailing_band = (OUTER_PAD + legend_right)
    .max(temporal_edge_pad_right)
    .max(required_title_side_band);
```

Leave the Canvas branch and x-title drawing coordinates unchanged. This preserves the centered title anchor at the plot midpoint while adding only missing scene space on each side.

- [ ] **Step 5: Run layout and temporal-rendering regressions**

Run:

```bash
cargo test -p fulgur-chart --test render_vegalite_temporal_line
cargo test -p fulgur-chart layout::common::tests::
```

Expected: all tests pass, including requested PlotArea dimensions, edge tick containment, tall right legend containment, rotated y-title containment, and existing snapshots.

- [ ] **Step 6: Commit**

```bash
git add crates/fulgur-chart/src/layout/common.rs crates/fulgur-chart/tests/render_vegalite_temporal_line.rs
git commit -m "fix(layout): contain centered PlotArea x titles"
```

### Task 4: Verify, publish, and resolve the review threads

**Files:**
- No source changes expected
- Update tracker: `fulgur-chart-8nx`

**Interfaces:**
- Consumes: the three implementation commits and three review thread IDs
- Produces: formatted and tested branch, 100% patch coverage, pushed commits, replied and resolved threads, green PR checks

- [ ] **Step 1: Run full local quality gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p fulgur-chart --locked
cargo test -p chart-server --locked
cargo check -p fulgur-chart --target wasm32-unknown-unknown --locked
git diff --check
```

Expected: every command exits 0. If formatting changes files, apply `cargo fmt --all`, rerun the focused tests and all gates, then commit the formatting with the owning task rather than leaving a dirty tree.

- [ ] **Step 2: Generate final committed-HEAD coverage**

Generate LCOV from the final committed source:

```bash
cargo llvm-cov --workspace --locked --lcov --output-path /tmp/fulgur-chart-pr137-round5.info
git diff --unified=0 origin/main...HEAD -- '*.rs'
```

Parse added executable Rust line numbers from the zero-context diff, excluding blank lines, comments, attributes, braces-only lines, and test-only lines if the CI patch gate excludes them. Match each remaining `path:line` to its LCOV `DA:<line>,<hits>` record. Report covered/total and the exact missed lines. Expected: 100.00% and no missed changed executable line; if any line is missed, add a behavior-focused test, rerun the affected target, commit it, and regenerate coverage from the new committed `HEAD`.

- [ ] **Step 3: Update Beads and synchronize the branch**

Record the verification evidence on `fulgur-chart-8nx`, then run:

```bash
git pull --rebase
bd dolt push
git push
git status --short --branch
```

State before pushing that the branch contains the design, plan, and three reviewed fixes. Expected: push succeeds and status is clean and up to date with `origin/feat/6an-vegalite-dogfood-parity`.

- [ ] **Step 4: Reply to and resolve the exact review threads**

For each thread, call GitHub GraphQL `addPullRequestReviewThreadReply` with the concrete change and regression evidence, then call `resolveReviewThread` only after the reply succeeds:

- `PRRT_kwDOS-i3R86TxDNX` — shared fallible x/y/color title reader, exact errors in both modes, fallback tests.
- `PRRT_kwDOS-i3R86TxDNb` — shared `config.view`/`stroke` semantic validation, strict allow-list preserved.
- `PRRT_kwDOS-i3R86TxDNf` — measured PlotArea x-title side bands, containment and geometry/legend regression test.

Do not post a generic PR comment in place of exact thread replies.

- [ ] **Step 5: Re-fetch review and CI state**

Run the skill's bundled review-thread fetch workflow for PR #137 and verify all three target thread IDs are resolved and the unresolved-thread count is zero. Then run:

```bash
gh pr checks 137 --watch
```

If a check fails, inspect its logs, fix only an in-scope regression, rerun the relevant local gates and coverage, commit, rebase, push, and re-check threads before continuing.

- [ ] **Step 6: Close the tracker and perform the mandatory final push**

After checks are green:

```bash
bd close fulgur-chart-8nx --reason "Three PR #137 review fixes implemented, tested at 100% patch coverage, pushed, replied, and resolved."
git pull --rebase
bd dolt push
git push
git status --short --branch
```

Expected: the bead is closed and pushed; the worktree is clean and the branch is up to date with its upstream. Keep this active PR worktree in place.
