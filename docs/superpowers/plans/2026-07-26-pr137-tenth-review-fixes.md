# PR #137 Tenth Review Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> `superpowers:subagent-driven-development` task by task.

**Goal:** Make rotated text and explicit line-marker radii consistent across
SVG, PNG, and WebP.

**Architecture:** Implement the existing `Prim::Text` rotation contract in the
direct raster renderer. Reject radii outside the raster numeric range in the
shared pre-render specification guard.

**Tech Stack:** Rust 2024, tiny-skia, ttf-parser, fulgur-chart Scene/guard
modules, cargo test, cargo clippy, cargo llvm-cov, GitHub GraphQL, Beads

## Global constraints

- Preserve unrotated raster text bytes and anchor layout.
- Match SVG rotation around the declared text anchor.
- Preserve zero/negative point-radius suppression semantics.
- Reject unsupported radius consistently before every output backend.
- Add regressions first and observe RED.
- Final executable changed-line coverage against `origin/main...HEAD` is 100%.

---

### Task 1: Render rotated text in PNG and WebP

**Files:**
- Modify/test: `crates/fulgur-chart/src/raster_direct.rs`
- Modify/test as appropriate:
  `crates/fulgur-chart/tests/render_vegalite_temporal_line.rs`

- [ ] Add raster pixel-bounds tests for horizontal and `-90°` anchored text at
  output scales 1 and 2.
- [ ] Add a temporal y-axis title PNG regression that detects horizontal,
  clipped rendering.
- [ ] Run focused tests and capture RED.
- [ ] Pass `rotate_deg` through the text path and compose rotation around
  `(x, y)` before the outer output transform.
- [ ] Keep missing/non-finite rotation on the existing unrotated path.
- [ ] Run raster and temporal render suites, formatting, and diff checks.
- [ ] Commit with `fix(raster): render rotated text`.

### Task 2: Reject unsupported explicit point radii

**Files:**
- Modify/test: `crates/fulgur-chart/src/guard.rs`
- Modify/test as appropriate:
  `crates/fulgur-chart/tests/render_line.rs`

- [ ] Add direct-IR guard tests for `None`, zero, negative, boundary, non-finite,
  and above-`f32::MAX` radii.
- [ ] Add Chart.js `pointRadius: 1e40` tests proving SVG, PNG, and WebP return
  the same validation error.
- [ ] Run focused tests and capture RED.
- [ ] Add one shared `validate_spec_base` check with a stable error.
- [ ] Run guard and line-render suites, formatting, and diff checks.
- [ ] Commit with `fix(guard): bound explicit point radii`.

### Task 3: Review, verify, publish, and resolve

- [ ] Obtain independent specification and quality reviews for Tasks 1 and 2.
- [ ] Rebase on current `origin/main`.
- [ ] Run formatting, focused tests, full workspace tests, and clippy.
- [ ] Run committed-HEAD patch coverage and reach 100%.
- [ ] Update `fulgur-chart-8nx`, `bd dolt push`, and push the branch.
- [ ] Reply to and resolve `PRRT_kwDOS-i3R86TzBls` and
  `PRRT_kwDOS-i3R86TzBlu`.
- [ ] Fresh-fetch PR #137 threads and require zero unresolved.
- [ ] Watch PR #137 checks to terminal green.
- [ ] Close `fulgur-chart-8nx`, push Beads state, and verify clean/up-to-date
  repository status. Do not merge the PR.
