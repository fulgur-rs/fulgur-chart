# PR #137 Eighth Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` to implement this plan task by task.

**Goal:** Resolve the two remaining PR #137 findings by matching D3's
interval-aware monotone-X tangents and validating the temporal top-level
`config` container consistently.

**Architecture:** Replace the local unweighted tangent construction with
D3-shaped `slope3` and `slope2` helpers while retaining finite-output guards.
Centralize optional top-level temporal config parsing in one borrowed-object
reader used by view and axis semantics.

**Tech Stack:** Rust 2024, serde_json, fulgur-chart layout/frontend modules,
cargo test, cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global constraints

- Preserve two-point linear monotone output and finite SVG tokens for malformed
  numeric inputs.
- Use real x intervals and D3's current endpoint correction.
- Preserve missing/null temporal config defaults.
- Preserve non-strict unknown-key tolerance and strict allow-lists.
- Add each regression test before its production change and observe RED.
- Final executable changed-line coverage against `origin/main...HEAD` is 100%.

---

### Task 1: Match D3 monotone-X tangents

**Files:**
- Modify/test: `crates/fulgur-chart/src/layout/monotone.rs`

- [ ] Add an exact irregular-spacing regression for
  `[(0, 0), (1, 10), (3, 12)]` whose control points reflect D3 `slope3` and
  one-sided `slope2` calculations.
- [ ] Run the focused test and confirm it fails against the unweighted/raw
  endpoint implementation.
- [ ] Replace `tangent(prev, next)` with interval-aware interior and endpoint
  helpers equivalent to D3 `slope3`/`slope2`.
- [ ] Retain finite normalization, zero-width safety, two-point fallback, and
  bounded control output.
- [ ] Run all `layout::monotone` tests.
- [ ] Commit with `fix(layout): match D3 monotone tangents`.

### Task 2: Validate top-level temporal config across modes

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

- [ ] Add a cross-mode regression replacing top-level `config` with `42`,
  `"config"`, `true`, and `[]`; assert exact
  `config must be an object`.
- [ ] Add/extend compatibility coverage for missing, null, and unknown object
  keys in non-strict mode while strict still rejects unknown keys.
- [ ] Run the focused test and confirm non-strict parsing fails the assertion.
- [ ] Add a shared borrowed `temporal_config` reader and use it from both
  `validate_temporal_view` and `temporal_axis_grid`.
- [ ] Run focused and full Vega-Lite frontend tests.
- [ ] Commit with `fix(vegalite): validate temporal config container`.

### Task 3: Verify, publish, and resolve review

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run focused monotone and Vega-Lite tests.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- [ ] Run committed-HEAD patch coverage and reach 100%.
- [ ] Obtain a final code review of `origin/main...HEAD`.
- [ ] Commit any coverage-only tests separately.
- [ ] Rebase on current `origin/main`, rerun affected gates if HEAD changes,
  update Beads evidence, `bd dolt push`, and push the branch.
- [ ] Reply to and resolve `PRRT_kwDOS-i3R86Tx9qH` and
  `PRRT_kwDOS-i3R86Tx9qJ`.
- [ ] Fetch review state again and require zero unresolved threads.
- [ ] Watch PR #137 checks to terminal green.
- [ ] Close `fulgur-chart-8nx`, push Beads state, and verify clean/up-to-date
  repository status.
