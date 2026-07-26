# PNG membench Design

## Goal

Extend the deterministic `dhat` memory gate so every existing representative
Chart.js benchmark case measures both the JSON-to-SVG path and the
JSON-to-PNG path.

## Scope

- Keep the existing nine benchmark inputs from `benches/cases.rs`.
- Preserve every current SVG baseline key, such as `bar_small`.
- Add one PNG baseline key per case with the `_png` suffix, such as
  `bar_small_png`.
- Measure PNG through `render_chart_to_png_default(&spec, 1.0)` so the gate
  covers parsing, scene construction, tiny-skia rasterization, and PNG
  encoding.
- Continue comparing cumulative `alloc_bytes` and recording `alloc_blocks`
  through the existing `HeapStats` delta mechanism.
- Update the benchmark documentation to state that both output paths are
  gated.

Changing the regression threshold, benchmark inputs, raster scale, CI job, or
comparison semantics is outside this issue.

## Architecture

Add a small bench-only `membench_targets.rs` module that expands
`cases::all()` into ordered measurement targets. Each target contains a stable
baseline name, a reference to its input case, and an output kind (`Svg` or
`Png`). SVG targets retain their original names; PNG targets append `_png`.

`membench.rs::measure()` iterates those targets. It parses each target's JSON
inside the measured interval, dispatches to `render_chart` or
`render_chart_to_png_default`, then records the existing `HeapStats` deltas
under the target name. PNG rendering failures remain fatal because benchmark
fixtures are controlled inputs and a skipped measurement would make the gate
misleading.

The target-expansion module is shared with an integration test through
`#[path]`, following the existing `cases.rs` and `membench_check.rs` pattern
without adding public library API.

## Data Flow

1. Generate the nine deterministic JSON cases.
2. Expand each case into one SVG target and one PNG target.
3. Capture `HeapStats` before the target.
4. Parse JSON and render the selected output.
5. Capture `HeapStats` after the target.
6. Store the byte/block deltas in the baseline map.
7. Compare all 18 entries with the committed baseline through the unchanged
   regression checker.

## Testing

- A new integration test first verifies that target expansion produces exactly
  two targets per case, preserves SVG names, assigns `_png` names, covers both
  output kinds, and produces no duplicate baseline keys.
- The existing benchmark-case integration test also renders every case to PNG
  and validates the PNG signature, proving the new measured path accepts every
  fixture.
- Run the focused integration tests, the membench baseline update/check cycle,
  `cargo fmt --check`, and clippy for the feature-gated bench target.
- Run the repository's relevant broader test gate before completion.

## Baseline Compatibility

Existing SVG entries are not renamed, so their historical values and regression
comparisons remain intact. The generated baseline adds only the nine `_png`
entries plus any normal compiler-dependent refresh to existing values produced
by the documented `--update` command.
