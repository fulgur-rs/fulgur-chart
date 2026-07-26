# PR #137 Eleventh Review Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` task by task.

**Goal:** Reject recognized invalid Vega-Lite line channel types consistently
in strict and non-strict parsing.

**Architecture:** Invoke the existing line semantic type validator from the
main parse path while retaining strict-only structural allow-list checks and
their existing error ordering.

**Tech Stack:** Rust 2024, serde_json, fulgur-chart Vega-Lite frontend, cargo
test, cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global constraints

- Preserve non-strict unknown-key tolerance.
- Preserve strict unknown-key checks and current error precedence.
- Preserve missing/null type inference.
- Do not broaden the supported line type subset.
- Add regressions first and observe RED.
- Final executable changed-line coverage against `origin/main...HEAD` is 100%.

---

### Task 1: Validate line channel types in both modes

**Files:**
- Modify: `crates/fulgur-chart/src/frontend/vegalite.rs`
- Modify/test: `crates/fulgur-chart/tests/frontend_vegalite.rs`

- [ ] Add cross-mode exact-error tests for non-string and unsupported
  `x.type`, `y.type`, and `color.type` values.
- [ ] Add compatibility coverage for missing/null types and non-strict unknown
  keys versus strict rejection.
- [ ] Run focused tests and capture non-strict RED.
- [ ] Invoke `validate_line_channel_types` from the parsed line path without
  removing the strict preflight invocation.
- [ ] Run focused and full Vega-Lite frontend suites, formatting, and diff
  checks.
- [ ] Commit with `fix(vegalite): validate line types in both modes`.

### Task 2: Review, verify, publish, and resolve

- [ ] Obtain independent specification and quality reviews.
- [ ] Rebase on current `origin/main`.
- [ ] Run formatting, focused tests, full workspace tests, and clippy.
- [ ] Run committed-HEAD patch coverage and reach 100%.
- [ ] Update `fulgur-chart-8nx`, `bd dolt push`, and push the branch.
- [ ] Reply to and resolve `PRRT_kwDOS-i3R86T1Qb5`.
- [ ] Fresh-fetch PR #137 threads and require zero unresolved.
- [ ] Watch PR #137 checks to terminal green.
- [ ] Close `fulgur-chart-8nx`, push Beads state, and verify clean/up-to-date
  repository status. Do not merge the PR.
