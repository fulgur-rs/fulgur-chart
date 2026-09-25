# QuickChart Violin Chart Design

Status: Draft for review
Beads issue: `fulgur-chart-ysr`

## Goal

Add static QuickChart-compatible `violin` and `horizontalViolin` charts. Each category shows a symmetric density shape computed from its raw numeric samples. Existing boxplot input and rendering remain unchanged.

QuickChart documents both `violin` and `horizontalViolin`. Its violin example uses nested arrays in `datasets[].data`, with one sample array per category. The maintained Chart.js plugin evaluates a Gaussian KDE at 100 sample positions by default and displays median and mean markers.

References:

- [QuickChart chart types](https://quickchart.io/documentation/chart-types/)
- [Violin KDE sampling](https://github.com/sgratzl/chartjs-chart-boxplot/blob/main/src/data.ts)
- [Gaussian kernel and bandwidth](https://github.com/sgratzl/boxplots/blob/main/src/kde.ts)
- [Violin element markers and symmetric outline](https://github.com/sgratzl/chartjs-chart-boxplot/blob/main/src/elements/Violin.ts)

## Public Input

The new chart types are `"violin"` and `"horizontalViolin"`. Both use the same top-level `data.labels`, `data.datasets[]`, `width`, `height`, `options.plugins`, `options.scales`, and `options.theme` conventions as other Cartesian Chart.js inputs. `violin` places categories on x and values on y; `horizontalViolin` places categories on y and values on x.

Each dataset's `data` is an array aligned with `labels`. Each non-null entry is an array of raw numeric samples; null sample values are allowed and ignored, for example:

```json
{
  "type": "violin",
  "data": {
    "labels": ["A", "B"],
    "datasets": [{
      "label": "Measurements",
      "data": [[2, 3, 4, 4, 8], [1, 2, 2, 5, 9]]
    }]
  }
}
```

An outer `null`, an empty sample array, or a group containing no finite samples produces no violin for that category. Null samples inside a group are ignored for KDE, matching the upstream statistics helper, but their input slots still count toward the resource limit. Non-numeric non-null samples are rejected by schema parsing. Boxplot remains a separate `ChartKind::BoxPlot`, because its nested arrays contain a five-number summary rather than observations.

No KDE tuning fields are added in this issue. The number of density positions is fixed at 100 to match the plugin default. Dataset label, fill color, border color, and border width follow the existing boxplot dataset behavior.

## Internal Model and Data Flow

- Add `ChartKind::Violin { horizontal: bool }` and a dedicated `Series.violin_samples: Vec<Vec<Option<f64>>>` field. Retaining null slots lets the guard count the complete input size while layout filters missing samples. Existing `Series.box_points` and `ChartKind::BoxPlot` keep their current meaning.
- Add `Violin` and `HorizontalViolin` variants to the Chart.js schema and parser. Reuse the existing nested numeric data representation where possible, while keeping the violin-specific conversion separate from five-number boxplot conversion. Reject inner null values for boxplot as before.
- Preserve the current untagged parser's shape precedence: nested arrays without inner nulls are parsed by the existing `Boxes` variant; add a nested optional-sample variant for inner nulls. An all-null outer array such as `[null, null]` can match the flat `Nums` variant first, so accept it for violin and convert each entry to an empty group. Continue rejecting non-null flat numeric arrays for violin, and leave boxplot's current interpretation unchanged. Spell the schema variant `horizontalViolin` explicitly because the root enum's default rename rule lowercases variant names.
- Treat the outer data-array length as the number of category slots for the series. The semantic model reports one element per category slot; resource validation counts every raw sample slot, including nulls.
- Add a dedicated `layout::violin` module. It shares the existing categorical index-axis frame and numeric value-axis rules, uses each category's samples to extend the automatic value domain, and respects hard user axis bounds in either orientation.
- In rendered geometry, vertical violin uses category x/value y and horizontal violin uses category y/value x. Keep the public model's axis normalization consistent with horizontal bars: model x remains categorical and model y remains the value axis in both orientations.
- Report the original public chart type in the semantic model: `violin` for vertical and `horizontalViolin` for horizontal input.
- Add `Violin` handling to layout dispatch, model type naming and normalized axes, element counts, and input guards.

## KDE and Rendering

For a sample group with at least two finite values, use the upstream Gaussian KDE and normal-reference bandwidth:

```text
h = 1.06 * min(sample_standard_deviation, IQR / 1.34) * n^(-1/5)
```

Use type-7 quartiles for IQR. Evaluate 100 evenly spaced value positions from the group's minimum through maximum. Normalize each group's density by that group's maximum estimate, then map it to half the category slot allocated to that dataset. This yields a symmetric violin centered on the category position and keeps dataset violins grouped within each category. For `violin`, density controls horizontal width and the value controls y; for `horizontalViolin`, density controls vertical height and the value controls x. Draw each body as one closed, filled and stroked `Prim::Path`.

Draw the median as a small diamond and the arithmetic mean as a small circle, oriented along the value axis and matching the upstream violin element's default markers. Do not overlay a box, whiskers, or raw sample jitter points.

When a group contains one sample or the computed bandwidth is non-positive or non-finite (including groups with zero IQR), use a positive fallback bandwidth equal to one percent of the computed value-axis span, with a small finite lower bound. Evaluate around the sample mean over three fallback bandwidths on each side so the group remains visible. This also avoids division by zero in the KDE.

Density output must remain finite. Empty or all-missing groups are skipped. Every generated path is clipped to the plot frame so explicit hard min/max bounds are honored.

## Resource Limits

- Count all raw sample slots, including nulls, toward `InputLimits.max_total_data_points`.
- Estimate KDE work as `finite_sample_count * 100`, using saturating arithmetic. Reject input whose KDE work exceeds `max_total_data_points.saturating_mul(100)`.
- Count at most three categorical primitives per non-empty group (body path, median marker, mean marker) against `max_categorical_primitives`.
- Keep the work linear in `sample_count * 100`; do not add an unbounded adaptive sampling mode.

## Documentation and Verification

- Add `violin` and `horizontalViolin` to the README's supported chart types and describe `datasets[].data` as nested raw sample arrays.
- Add vertical and horizontal violin examples with multiple categories and non-symmetric distributions, plus committed PNG goldens.
- Add parser tests for both orientations, null/empty samples (including an all-null outer array), rejection of non-null flat numeric arrays, malformed samples, and ChartJsSpec schema round-trip.
- Add layout tests for a symmetric body, grouped datasets, oriented mean/median markers, value-domain coverage, hard value bounds, and singleton/constant groups in both orientations. Add model tests confirming category-x/value-y normalization for both orientations.
- Add guard tests for raw observation count and KDE work limits.
- Run the relevant core Rust test suite, golden PNG verification, formatter, and CI before PR merge.

## Out of Scope

- `horizontalBoxPlot`.
- Boxplot overlay, scatter/item jitter, outlier styling, tooltips, and animation.
- User-configurable KDE kernels, bandwidth, density points, or violin widths.
- Changing existing boxplot parsing, layout, or styling.
