# PR #137 Sixth Review Fixes

## Scope

Address the three valid unresolved review threads discovered after the fifth
review fixes were published:

1. Keep a long top-level chart title inside a narrow temporal PlotArea scene.
2. Let custom-font SVG, PNG, and WebP rendering preserve caller-supplied input
   limits.
3. Avoid claiming a constant temporal model step for irregular calendar ticks.

Existing render APIs remain source-compatible and retain default-limit safety.
Requested PlotArea dimensions, temporal tick coordinates, right-legend
placement, and Canvas geometry remain unchanged.

## PlotArea chart-title containment

The shared PlotArea frame measures both centered horizontal title consumers:

- the top-level chart title at `TITLE_FONT`;
- a centered x-axis title at its resolved font size.

Each title contributes `max((measured_width - requested_plot_width) / 2, 0)`.
The larger contribution becomes the minimum symmetric side overflow. Existing
y-axis, temporal edge-label, and right-legend bands continue to participate via
`max`, so only missing scene space is added. The title drawing anchors and
requested plot width and height do not change.

## Caller-supplied render limits

Add API variants that accept `&InputLimits`:

```text
render_chart_with_font_and_limits
render_chart_to_png_with_limits
render_chart_to_webp_with_limits
```

Existing `render_chart_with_font`, `render_chart_to_png`,
`render_chart_to_png_with`, and `render_chart_to_webp` delegate using
`InputLimits::default()`. This preserves their current safety behavior and
avoids embedding request policy in `ChartSpec`.

The new variants use the supplied limits for the existing custom-font
PlotArea-scene validation. Raster format hard stops for pixel area and WebP
axis size remain mandatory and independent of `InputLimits`. PNG compression
selection remains on the existing default-limit API; no unrelated options
object or breaking signature change is introduced.

## Temporal model step

`AxisModel.step` is populated only when at least two emitted temporal ticks
exist and every adjacent millisecond delta is identical. Delta calculation
widens to `i128` before subtraction. Fixed-duration ticks continue reporting a
numeric step; monthly and yearly sequences whose UTC durations vary report
`None`. The explicit tick array remains authoritative in all cases.

## Verification

Red-green tests cover:

- a narrow PlotArea with a long top-level title fully inside the scene;
- unchanged requested plot dimensions and right-legend containment;
- default render APIs still rejecting a scene beyond default limits;
- all three custom-limit render variants accepting the same scene under
  relaxed limits;
- stricter supplied limits still rejecting it;
- constant temporal tick deltas returning a numeric step;
- irregular month/year deltas returning no step.

Completion requires the full repository quality gates, final committed-HEAD
changed-line coverage of 100%, exact replies and resolutions for the three
remaining threads, zero unresolved threads, green PR checks, Beads closure,
and a clean pushed branch.
