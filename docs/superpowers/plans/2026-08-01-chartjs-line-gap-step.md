# Chart.js Line Gap and Step Implementation Plan

> **For agentic workers:** execute this plan in the existing `feat/nif-line-gap-step`
> worktree. Beads `fulgur-chart-nif` is the sole task tracker; this document deliberately
> contains no Markdown task checkboxes.

**Goal:** Make root Chart.js line datasets render null-spanning and stepped paths compatibly
with `spanGaps` and `stepped`.

**Architecture:** The typed schema and raw Chart.js frontend accept the same restricted public
forms, which are mapped to schema-independent `Series` fields. `layout::line` segments valid
points according to the IR gap policy, then expands each retained segment into a step polyline
when requested. Area fill consumes that same expanded geometry.

**Tech stack:** Rust, Serde/Schemars, the existing Fulgur IR and scene primitives, Cargo tests.

---

## File map

| File | Responsibility |
| --- | --- |
| `crates/fulgur-chart/src/schema/chartjs.rs` | Typed public LineDataset fields and schema tests. |
| `crates/fulgur-chart/src/frontend/chartjs.rs` | Raw JSON parsing and conversion to IR. |
| `crates/fulgur-chart/src/ir.rs` | Schema-independent line gap and step representation. |
| `crates/fulgur-chart/src/layout/line.rs` | Segment construction and exact step geometry. |
| `crates/fulgur-chart/tests/frontend_chartjs.rs` | Strict schema/parser parity regression coverage. |

## Task 1: Define and map the public contract

Files:

- Modify: `crates/fulgur-chart/src/schema/chartjs.rs:181-207`
- Modify: `crates/fulgur-chart/src/frontend/chartjs.rs:170-194, 779-790`
- Modify: `crates/fulgur-chart/src/ir.rs:83-117` and every `Series` literal
- Test: `crates/fulgur-chart/src/schema/chartjs.rs:1338-1350`
- Test: `crates/fulgur-chart/src/frontend/chartjs.rs` test module
- Test: `crates/fulgur-chart/tests/frontend_chartjs.rs`

1. Add RED schema tests that deserialize `spanGaps: true`, `stepped: false`, `stepped: true`,
   and each named step; assert that an invalid string such as `"left"` is rejected. Add a
   strict frontend test with both fields so schema acceptance and `parse(..., true)` remain in
   lockstep. Run `cargo test -p fulgur-chart --locked schema::chartjs::tests` and the named
   frontend test; both must fail because the keys are currently unknown.

2. Define the typed input enum and fields:

   ```rust
   #[derive(Serialize, Deserialize, JsonSchema)]
   #[serde(untagged)]
   pub enum Stepped {
       Bool(bool),
       Mode(SteppedMode),
   }

   #[derive(Serialize, Deserialize, JsonSchema)]
   #[serde(rename_all = "lowercase")]
   pub enum SteppedMode { Before, After, Middle }

   pub span_gaps: Option<bool>,
   pub stepped: Option<Stepped>,
   ```

   `SteppedMode` accepts only the three exact literals, so invalid strings are rejected without
   an unrestricted `String` variant.

3. Add IR-only behavior fields and defaults:

   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   pub enum StepMode { Before, After, Middle }

   pub span_gaps: bool,
   pub step_mode: Option<StepMode>,
   ```

   Add the fields to every `Series` literal with `false` and `None` defaults. This preserves
   all non-line call sites and makes the compiler identify missed construction sites.

4. Extend `RawDataset` with `#[serde(rename = "spanGaps", default)] span_gaps: bool` and an
   untagged raw step enum. Map `true` to `Some(StepMode::Before)`, `false` to `None`, and the
   three exact strings to their corresponding modes. Construct `interpolation` normally but
   make layout choose `step_mode` first. Run the focused tests again; they must pass.

## Task 2: Render gap and step geometry

Files:

- Modify: `crates/fulgur-chart/src/layout/line.rs:57-175`
- Test: `crates/fulgur-chart/src/layout/line.rs` test module

1. Add RED layout tests with `[1, null, 3]`: the default must produce no two-point polyline,
   while `spanGaps: true` must produce one. Add exact-point assertions for `[1, 3]`:

   ```text
   before: (x0,y0), (x1,y0), (x1,y1)
   after:  (x0,y0), (x0,y1), (x1,y1)
   middle: (x0,y0), ((x0+x1)/2,y0), ((x0+x1)/2,y1), (x1,y1)
   ```

   Add a nonzero-tension `stepped: "middle"` case and assert it emits `Prim::Polyline`, not
   a cubic `Prim::Path`. Run each named test and confirm the expected missing-feature failure.

2. Extract a private `line_segments(valid, span_gaps)` helper. With `span_gaps` true it returns
   `vec![valid]` when nonempty; otherwise it preserves the existing category-discontinuity split.
   Keep decimation after this helper, so a bridged line is decimated as one segment.

3. Extract `step_points(points, mode) -> Vec<(f64, f64)>`. Start with the first point and append
   the two or three legs above per pair. For an absent mode return the original points unchanged.
   Use it for the polyline branch before consulting `LineInterpolation`; if it returns expanded
   points, emit `Prim::Polyline` and skip Catmull-Rom/monotone.

4. Build each filled polygon from the same expanded points, then append the baseline legs. This
   makes `spanGaps: true` bridge an area and keeps all three step variants visually consistent
   with their line. Add a fill regression that checks the step corner coordinates occur in its
   `Prim::Path` data. Run `cargo test -p fulgur-chart --locked layout::line::tests` and confirm
   all added tests pass.

## Task 3: Final compatibility verification

Files:

- Test: all changed tests above

1. Run `cargo fmt --check`, `cargo test -p fulgur-chart --locked`,
   `cargo clippy -p fulgur-chart --all-targets --locked -- -D warnings`, and `git diff --check`.
2. Run the repository changed-line coverage command against `origin/main`; if any changed
   executable line is uncovered, add a behavior-level regression test and rerun the command.
3. Review `git diff origin/main...HEAD` for scope: only the five mapped files plus this design
   and plan document are allowed. Commit the implementation with an English conventional title,
   push, report the exact verification outcomes, and close `fulgur-chart-nif` only after the
   acceptance criteria are demonstrably met on the pushed branch.

## Plan self-review

The plan covers each approved requirement: typed input, independent IR representation, null
segmentation, all step modes, stepped-over-tension precedence, shared area geometry, and focused
through package-level verification. It intentionally excludes numeric `spanGaps` and mixed
charts as specified. Field and enum names are used consistently throughout.
