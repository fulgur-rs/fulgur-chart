# Chart.js Time and Timeseries Scale Implementation Plan

**Goal:** Implement the approved `fulgur-chart-tu4` design for Chart.js temporal
axes while preserving charts without an explicit temporal axis.

**Architecture:** Add axis-local temporal scale/configuration IR, parse temporal
labels and coordinates at the Chart.js frontend boundary, and map time values
through shared horizontal/vertical scale helpers. Reuse and extend
`temporal.rs` for deterministic UTC parsing, rounding, ticks, and labels.

**Tech Stack:** Rust, serde/serde_json, schemars, `time` 0.3, existing SVG and
raster renderers, Beads (`bd`).

## Constraints

- Work in `/home/mitz/Work/oss/fulgur-chart/.worktrees/fulgur-chart-tu4-time-scale`
  on `feat/fulgur-chart-tu4-time-scale`.
- Follow `docs/superpowers/specs/2026-09-24-chartjs-time-scale-design.md`.
- Temporal configuration is per axis; both `time` and `timeseries` work on x
  and y for supported Cartesian chart families.
- Parse ISO-8601/RFC 3339 strings and finite epoch-millisecond numbers. The
  custom parser and formatter directive subsets are bounded and deterministic.
- UTC is used for offset-free input, rounding, calendar boundaries, and tick
  labels. JavaScript callbacks, local timezone behavior, and unrelated axis
  options are out of scope.
- Preserve null gaps, duplicate points, point order, category behavior, and all
  output for charts without an explicit temporal scale.
- Use TDD: add/execute a focused failing test before implementing each behavior;
  run the focused tests after the change.
- Keep input and tick work bounded by existing chart limits and temporal tick
  cap. Do not allocate an unbounded temporary domain from parsed values.
- User-facing parse/configuration errors are English and bounded.
- The issue is already claimed as `fulgur-chart-tu4`; close it only after the
  implementation and quality gates pass, then push Beads state.

## File map

### Core contracts

- `crates/fulgur-chart/src/ir.rs`
  - temporal `ScaleKind`, `TimeUnit`, `TimeOptions`, and temporal positions on
    the shared index axis; update axis/chart defaults and constructors
- `crates/fulgur-chart/src/guard.rs`
  - temporal option/range/position validation and chart-family invariants
- `crates/fulgur-chart/src/scale.rs`
  - shared time/timeseries axis mapping and configured tick integration if
    required by the existing value-scale boundary

### Parsing and schema

- `crates/fulgur-chart/src/schema/common.rs`
  - typed `time` options and public axis schema fields
- `crates/fulgur-chart/src/frontend/chartjs.rs`
  - time-axis validation, labels and data coordinate parsing, axis IR mapping,
    and strict-key allowlists
- `crates/fulgur-chart/src/temporal.rs`
  - ISO/RFC3339/custom parsing, epoch-millisecond clipping, UTC rounding,
    quarter-aware ticks, and configured UTC display formatting

### Rendering and model

- `crates/fulgur-chart/src/layout/common.rs`
  - shared temporal x/y domains, mapping, axis labels, grids, and tick layout
- `crates/fulgur-chart/src/layout/line.rs`
  - line geometry for temporal x and y scales
- `crates/fulgur-chart/src/layout/bar.rs`
  - vertical/horizontal temporal value and index axes, including time-derived
    sample spacing and existing grouped-bar width options
- `crates/fulgur-chart/src/layout/mixed.rs`
  - shared temporal mapping for mixed bar/line datasets
- `crates/fulgur-chart/src/layout/scatter.rs`
  - temporal x/y point projection and ticks
- `crates/fulgur-chart/src/model.rs`
  - public axis/tick and geometry metadata for all temporal Cartesian layouts

### Tests and user documentation

- `crates/fulgur-chart/src/temporal.rs` unit tests
- `crates/fulgur-chart/src/frontend/chartjs.rs` parser/configuration tests
- `crates/fulgur-chart/tests/frontend_chartjs.rs`
- `crates/fulgur-chart/tests/render_line.rs`
- `crates/fulgur-chart/tests/render_bar.rs`
- `crates/fulgur-chart/tests/render_scatter.rs`
- `crates/fulgur-chart/tests/render_mixed.rs`
- Chart.js schema snapshots and `examples/specs` temporal samples
- Compatibility documentation in `README.md` or the existing Chart.js docs

Mechanical `AxisSpec`, `ChartSpec`, and raw label/data initializer sites must
be found with `rg` before editing; do not assume the file map is exhaustive.

## Implementation tasks

### 1. Define temporal axis IR and invariants

1. Add failing IR/guard tests for default configuration, supported unit
   validation, temporal position length/range, and invalid combinations.
2. Run the focused tests and confirm they fail for the missing contracts.
3. Add axis-local `TimeOptions`, `TimeUnit`, `ScaleKind::Time`, and
   `ScaleKind::Timeseries`; preserve current defaults for all existing axis
   constructors.
4. Generalize the existing line-only temporal x-position contract to the
   regular chart index axis on x or y. Keep per-point scatter coordinates and
   value-axis values in their existing data vectors.
5. Validate temporal configuration and position/data invariants at the
   `ChartSpec` guard boundary. Update all IR constructors mechanically.
6. Run the focused IR/guard tests and all affected unit tests.

### 2. Implement bounded deterministic temporal operations

1. Add failing tests for default ISO date/time parsing, UTC offsets, custom
   parser directives, numeric epoch milliseconds, invalid formats, range
   rejection, rounding (including pre-epoch values), quarter boundaries,
   forced units, `minUnit`, custom display formats, and the tick cap.
2. Extend `temporal.rs` with strict parser/formatter helpers and a public unit
   representation. Keep current Vega-Lite default parser/tick behavior stable.
3. Add `quarter` to calendar stepping, interval selection, and start-of-unit
   rounding. Use Sunday as the UTC week boundary.
4. Extend temporal tick generation to accept `TimeOptions` and the axis's
   existing tick-count controls. Keep calendar stepping integer-safe and
   bounded.
5. Run focused temporal tests and existing Vega-Lite temporal tests.

### 3. Parse and validate Chart.js time options and data

1. Add failing schema/frontend tests for `time`/`timeseries` on x and y,
   supported `time` keys, invalid type/unit/parser/display formats, and
   rejection of unsupported callback/unknown keys.
2. Add the typed schema and runtime allowlist entries; verify generated JSON
   schema and runtime key validation agree.
3. Make labels/data coordinate inputs preserve number/string/null until axis
   configuration is known. Keep non-temporal parsing behavior unchanged and
   reject non-string categories where the current contract rejects them.
4. Parse temporal index labels for line/bar/mixed (including `indexAxis: "y"`),
   temporal value-axis dataset values, and scatter/bubble x/y coordinates.
   Apply rounding once at input conversion; retain point order, duplicates,
   and null gaps.
5. Add frontend tests for all input shapes and error bounds, then run existing
   Chart.js parser/schema tests.

### 4. Add shared temporal axis mapping and line/scatter behavior

1. Add failing layout/model tests showing elapsed spacing for `time`, equal
   spacing for `timeseries`, vertical temporal mapping, generated UTC ticks,
   grid positions, and axis metadata.
2. Implement a shared time/timeseries mapper using sorted unique timestamps
   for timeseries interpolation and elapsed milliseconds for time.
3. Route common frame ticks, labels, grid lines, and coordinates through the
   mapper. Preserve category x behavior and current Vega-Lite temporal line
   output when Chart.js temporal options are absent.
4. Integrate line x/y and scatter/bubble x/y layouts and semantic models.
5. Run the focused rendering/model tests and Chart.js/Vega-Lite regression
   suites.

### 5. Add temporal bar and mixed-chart behavior

1. Add failing tests for vertical and horizontal temporal index axes, temporal
   value axes, grouped bars with irregular gaps, fixed/flex/max bar thickness,
   null gaps, and mixed line/bar overlays.
2. Derive temporal sample spacing from adjacent unique timestamps and feed it
   through existing per-dataset bar geometry controls.
3. Integrate temporal mapping, baselines, clipping, labels, grid/ticks, and
   model geometry in vertical bars, horizontal bars, and mixed charts.
4. Run the focused bar/mixed tests and all Chart.js rendering regressions.

### 6. Publish examples and schema contract

1. Add a small temporal line example and a scatter example demonstrating both
   timestamp strings and epoch milliseconds, plus a `timeseries` example with
   irregular dates.
2. Regenerate/update committed schema artifacts and document supported axes,
   parser directives, UTC behavior, units, and limitations.
3. Check generated schema, examples, docs, and runtime allowlists for matching
   accepted keys and values.

### 7. Complete verification and prepare PR

1. Run focused temporal/frontend/layout tests, full `cargo test --workspace
   --locked`, formatting, lints, schema checks, and supported WASM builds.
2. Run the repository's required coverage/CI checks and inspect output diffs
   for unchanged non-temporal Chart.js and Vega-Lite fixtures.
3. Review the complete patch against the design and issue scope; fix every
   regression or uncovered accepted behavior before creating the PR.
4. Push the feature branch, create a PR linked to `fulgur-chart-tu4`, and wait
   until all required CI checks pass.
5. Merge only after green CI, close the Beads issue, push Beads data, and clean
   up this worktree and feature branch. Continue with the next ready issue
   toward the 20-merge goal.
