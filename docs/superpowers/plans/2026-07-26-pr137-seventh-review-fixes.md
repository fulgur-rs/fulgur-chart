# PR #137 Seventh Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the final three PR #137 review findings by preserving tiny finite scale, validating the temporal axis container consistently, and moving label guards before string cloning.

**Architecture:** Remove absolute-scale assumptions from the Vega tick wrapper while retaining bounded fallback, make `config.axis` parsing explicitly fallible before field semantics, and move temporal group-name validation into the constructor at the earliest borrowed-data boundary.

**Tech Stack:** Rust 2024, serde_json, fulgur-chart scale/frontend modules, cargo test, cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global Constraints

- Preserve the existing ordinary-value Vega dogfood domain and bounded behavior for invalid/extreme inputs.
- Preserve missing/null temporal axis defaults, non-strict unknown-key tolerance, and strict allow-lists.
- Preserve exact temporal legend label-limit errors for strings, numbers, and booleans.
- Reject over-limit borrowed strings before creating any owned group copy.
- Add each regression test before its production change and observe the expected failure.
- Final executable changed-line coverage against `origin/main...HEAD` must be 100%.

---

### Task 1: Preserve scale for tiny finite temporal values

**Files:**
- Modify: `crates/fulgur-chart/src/scale.rs`
- Test: `crates/fulgur-chart/src/scale.rs`

**Interfaces:**
- Consumes: finite `data_min`, `data_max`, and PlotArea height
- Produces: `NiceTicks` whose magnitude remains relative to a non-degenerate tiny domain

- [ ] **Step 1: Add failing tiny-domain regressions**

Add `vega_nice_ticks_preserves_tiny_finite_domains`. For these domains:

```rust
[(0.0, 1e-20), (-1e-20, 0.0), (-1e-20, 1e-20)]
```

assert:

- `min`, `max`, `step`, and every tick are finite;
- `step > 0.0`;
- the returned domain contains both input endpoints;
- `max - min <= 5e-20`, proving it was not widened to epsilon magnitude;
- at least two distinct input endpoints map to visibly distinct normalized
  positions through `LinearScale`.

- [ ] **Step 2: Run focused RED**

```bash
cargo test -p fulgur-chart scale::tests::vega_nice_ticks_preserves_tiny_finite_domains -- --exact
```

Expected: failure because the absolute epsilon floor expands the domain.

- [ ] **Step 3: Replace absolute floors with guarded relative step selection**

After finite/order validation, compute the raw span without `.max(EPSILON)`.
If the span is non-finite or non-positive, return `nice_ticks`.

Add a private helper:

```rust
fn finite_nice_step(numerator: f64, target: usize) -> Option<f64> {
    let raw_step = numerator / target.max(1) as f64;
    if !raw_step.is_finite() || raw_step <= 0.0 {
        return None;
    }
    let step = nice_step(raw_step);
    (step.is_finite() && step > 0.0).then_some(step)
}
```

Use it in positive, negative, and mixed branches in place of all three
`max(f64::EPSILON)` expressions. If it returns `None`, fall back to
`nice_ticks(data_min, data_max, target)`. Retain every existing finite
boundary check and bounded tick generator.

- [ ] **Step 4: Run GREEN and scale regressions**

```bash
cargo test -p fulgur-chart scale::tests::vega_nice_ticks_preserves_tiny_finite_domains -- --exact
cargo test -p fulgur-chart scale::tests::
```

Expected: all pass, including ordinary dogfood, negative/mixed, flat, and
extreme finite-domain tests.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/scale.rs
git commit -m "fix(scale): preserve tiny Vega domains"
```

### Task 2: Reject non-object temporal axis configuration

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

**Interfaces:**
- Consumes: optional `config.axis`
- Produces: absent/null defaults, object semantics, or exact object-type error

- [ ] **Step 1: Add failing cross-mode type tests**

Add `temporal_line_rejects_non_object_axis_config_in_both_modes`. Replace the
dogfood fixture's axis object with each of:

```text
42
"axis"
true
[]
```

For strict and non-strict parsing, assert the exact error:

```text
config.axis must be an object
```

- [ ] **Step 2: Add compatibility tests**

Extend or add a test proving:

- missing and JSON `null` `config.axis` preserve default displayed grid in
  both modes;
- a non-strict axis object containing existing valid fields plus
  `"futureOption": true` remains accepted;
- the corresponding unknown key remains rejected in strict mode.

- [ ] **Step 3: Run focused RED**

```bash
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_rejects_non_object_axis_config_in_both_modes -- --exact
```

Expected: non-strict mode silently accepts the scalar/array values.

- [ ] **Step 4: Make the container parse explicit**

In `temporal_axis_grid`, obtain the raw non-null axis value before converting:

```rust
let axis = top
    .get("config")
    .and_then(Value::as_object)
    .and_then(|config| config.get("axis"))
    .filter(|value| !value.is_null())
    .map(|value| {
        value
            .as_object()
            .ok_or_else(|| "config.axis must be an object".to_string())
    })
    .transpose()?;
```

Keep the existing `grid` and `gridOpacity` checks and default behavior. Do not
move the strict unknown-key allow-list or reject unknown keys in non-strict
mode.

- [ ] **Step 5: Run GREEN and frontend regressions**

```bash
cargo test -p fulgur-chart --test frontend_vegalite temporal_line_rejects_non_object_axis_config_in_both_modes -- --exact
cargo test -p fulgur-chart --test frontend_vegalite
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs crates/fulgur-chart/tests/frontend_vegalite.rs
git commit -m "fix(vegalite): validate temporal axis config"
```

### Task 3: Guard temporal group strings before cloning

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/src/frontend/vegalite.rs`

**Interfaces:**
- Consumes: borrowed optional group value, field name, and `max_label_bytes`
- Produces: validated `TemporalGroup` without pre-rejection string copies

- [ ] **Step 1: Add failing constructor-boundary tests**

Change the private tests to call:

```rust
temporal_group(Some(&json!("group-label-over-limit")), "group", 20)
```

and assert the exact length-limit error. This is compile-RED until the limit
becomes part of the constructor boundary.

Add generated-name cases proving `"42"` and `"true"` are accepted at their
exact byte lengths and rejected when the supplied limit is one byte smaller.

- [ ] **Step 2: Run compile RED**

```bash
cargo test -p fulgur-chart frontend::vegalite::temporal_line_tests::temporal_line_groups_normalize_scalars_and_bound_errors -- --exact
```

Expected: compilation fails because `temporal_group` does not accept the limit.

- [ ] **Step 3: Move validation into `temporal_group`**

Add `max_label_bytes: usize` to the function. Use a small shared error builder
or check helper that preserves:

```text
temporal legend label length <N> bytes exceeds limit <M>
```

In the string arm, check `value.len()` on the borrowed `&String` before any
clone. Only then construct the three owned fields.

For number and boolean arms, create the display name once, validate it, then
move it into `TemporalGroup.name`; retain the copyable key/order values.

Pass `limits.max_label_bytes` from the build caller and remove the caller's
post-construction length check. Update all private test call sites.

- [ ] **Step 4: Run GREEN and temporal frontend regressions**

```bash
cargo test -p fulgur-chart frontend::vegalite::temporal_line_tests::temporal_line_groups_normalize_scalars_and_bound_errors -- --exact
cargo test -p fulgur-chart frontend::vegalite::temporal_line_tests::
cargo test -p fulgur-chart --test frontend_vegalite
```

Expected: all pass with unchanged external errors and ordering.

- [ ] **Step 5: Commit**

```bash
git add crates/fulgur-chart/src/frontend/vegalite.rs
git commit -m "fix(vegalite): guard group labels before cloning"
```

### Task 4: Verify, publish, and resolve the final threads

**Files:**
- No source changes expected
- Update tracker: `fulgur-chart-8nx`

**Interfaces:**
- Consumes: Tasks 1-3 and thread IDs
- Produces: 100% coverage, pushed branch, zero unresolved, green checks, closed tracker

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
cargo llvm-cov --workspace --locked --lcov --output-path /tmp/fulgur-chart-pr137-round7.info
git diff --unified=0 origin/main...HEAD -- '*.rs'
```

Use the established PR #137 aggregation method. Require 100.00%, misses 0.

- [ ] **Step 3: Publish and resolve exact threads**

Update `fulgur-chart-8nx`, then run:

```bash
git pull --rebase
bd dolt push
git push
```

Reply via `addPullRequestReviewThreadReply`, then resolve after reply success:

- `PRRT_kwDOS-i3R86TxpmK` — relative tiny-domain Vega scale.
- `PRRT_kwDOS-i3R86TxpmL` — cross-mode temporal axis object validation.
- `PRRT_kwDOS-i3R86TxpmM` — pre-clone temporal group label guard.

- [ ] **Step 4: Verify zero unresolved and green CI**

Fresh-fetch all threads and require zero unresolved. Then:

```bash
gh pr checks 137 --watch
```

Do not silently resolve newly discovered valid findings.

- [ ] **Step 5: Close Beads and final sync**

```bash
bd close fulgur-chart-8nx --reason "All PR #137 review fixes implemented, verified at 100% patch coverage, pushed, replied, and resolved with green checks."
git pull --rebase
bd dolt push
git push
git status --short --branch
```

Expected: closed/pushed bead, clean branch equal to upstream, active PR
worktree retained.
