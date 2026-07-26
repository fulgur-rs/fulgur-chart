# PR #137 Ninth Review Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` to implement this plan task by task.

**Goal:** Keep valid mixed extreme temporal domains finitely and correctly
mappable without changing ordinary scale output.

**Architecture:** Add a fallback normalization path inside `LinearScale::map`
that rescales finite domain values only when direct span subtraction
overflows. Pin the behavior at both scale and Vega-Lite render boundaries.

**Tech Stack:** Rust 2024, fulgur-chart scale/layout/frontend modules, cargo
test, cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global constraints

- Preserve the existing direct mapping calculation for ordinary finite spans.
- Preserve degenerate-domain behavior.
- Do not reject valid finite temporal values or narrow their domain.
- End-to-end tests must detect wrong finite geometry, not only serialized
  `NaN`.
- Add regressions before production changes and observe RED.
- Final executable changed-line coverage against `origin/main...HEAD` is 100%.

---

### Task 1: Map overflowing finite domains safely

**Files:**
- Modify/test: `crates/fulgur-chart/src/scale.rs`
- Modify/test as appropriate:
  `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`

- [ ] Add exact/approximate endpoint and midpoint tests for inverted and normal
  pixel ranges over `[-1e308, 1e308]`.
- [ ] Add an accepted temporal-line regression proving extreme y-values map to
  opposite finite plot positions.
- [ ] Run focused tests and capture the current `NaN`/collapsed-geometry RED.
- [ ] Add the smallest overflow-only rescaled normalization fallback to
  `LinearScale::map`.
- [ ] Run scale and temporal render suites, formatting, and diff checks.
- [ ] Commit with `fix(scale): map extreme mixed domains safely`.

### Task 2: Review, verify, publish, and resolve

- [ ] Obtain independent specification and code-quality reviews.
- [ ] Rebase on current `origin/main`.
- [ ] Run formatting, focused tests, full workspace tests, and clippy.
- [ ] Run committed-HEAD patch coverage and reach 100%.
- [ ] Commit any coverage-only tests separately and re-review if needed.
- [ ] Update `fulgur-chart-8nx`, `bd dolt push`, and push the branch.
- [ ] Reply to and resolve `PRRT_kwDOS-i3R86Ty4gU`.
- [ ] Fresh-fetch PR #137 threads and require zero unresolved.
- [ ] Watch PR #137 checks to terminal green.
- [ ] Close `fulgur-chart-8nx`, push Beads state, and verify clean/up-to-date
  repository status. Do not merge the PR.
