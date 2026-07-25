# PR #137 Fifth Review Fixes

## Scope

Address the three unresolved PR #137 regressions without broadening Vega-Lite
v1 support:

1. Reject non-string temporal channel titles in strict and non-strict modes.
2. Reject unsupported `config.view` values in strict and non-strict modes.
3. Keep a centered temporal x-axis title inside a PlotArea scene.

Unknown keys remain tolerated in non-strict mode. Temporal inputs remain RFC
3339-only, and requested PlotArea width and height remain authoritative.

## Parser validation

Temporal `encoding.x.title`, `encoding.y.title`, and `encoding.color.title`
share one semantic reader. A missing or JSON `null` title falls back to the
channel field name. A string is preserved, including an empty string. Any
other JSON type returns:

```text
encoding.<channel>.title must be a string, got <type>
```

The same reader is used after both strict and non-strict parsing, so recognized
invalid values cannot silently become field-name fallbacks. Strict mode retains
its existing unknown-key checks.

Temporal `config.view` also receives shared semantic validation. Missing or
JSON `null` configuration remains accepted. A present `config.view` must be an
object, and a present `stroke` must be JSON `null`; otherwise parsing returns
the existing English errors:

```text
config.view must be an object
config.view.stroke must be null
```

Non-strict mode continues to ignore unknown view keys. Strict mode separately
retains its allow-list validation.

## PlotArea layout

For a centered temporal x-axis title, layout measures the title using its
resolved font size. If half the measured width exceeds half the requested plot
width, the excess becomes the minimum side band required on both sides.

The left plot origin is shifted only when the existing y-axis/tick band is too
narrow. The trailing scene band is expanded only when the existing final-tick
or right-legend band is too narrow. The requested plot width, plot height,
right legend placement, temporal tick positions, and Canvas layout are
preserved.

## Verification

Red-green tests cover:

- number, boolean, object, and array titles on x, y, and color in both modes;
- missing and `null` titles retaining field-name fallback;
- non-object `config.view` and non-null `config.view.stroke` in both modes;
- missing and `null` view configuration remaining accepted;
- a narrow PlotArea with a long centered x-axis title fitting within the scene;
- unchanged requested plot dimensions and right-legend containment.

Completion requires formatting, workspace clippy, fulgur-chart and chart-server
tests, wasm32 checking, changed-line coverage of 100%, thread replies and
resolution, successful push, and green PR checks.
