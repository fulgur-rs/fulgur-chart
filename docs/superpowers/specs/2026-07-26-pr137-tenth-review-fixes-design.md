# PR #137 Tenth Review Fixes Design

## Context

Two current review threads expose cross-format differences:

1. `Prim::Text.rotate_deg` is serialized by SVG but ignored by the direct
   raster backend. Temporal y-axis titles therefore render vertically in SVG
   and horizontally/clipped in PNG and WebP.
2. A finite Chart.js `pointRadius` above the raster backend's `f32` range is
   retained in SVG but becomes infinity during raster conversion, causing
   tiny-skia to omit the marker silently.

Both fixes should preserve the Scene/IR contract across output formats rather
than special-case the temporal dogfood fixture.

## Raster text rotation

Treat `rotate_deg` as part of `Prim::Text` rendering in the direct raster path.
Compose a tiny-skia rotation around the text anchor `(x, y)` with the existing
per-glyph translation/scale and the outer device-scale transform. The ordering
must match SVG's `rotate(angle, x, y)`: lay out text in user space, rotate it
around the anchor, then apply output scaling.

Missing rotation retains the current byte path. Non-finite rotation values
must not introduce non-finite transforms or panics; they should use the
existing unrotated defensive behavior.

Tests will compare the non-transparent pixel bounds of identical horizontal
and `-90°` text, at normal and scaled output, and add a temporal y-axis title
PNG regression proving its vertical extent exceeds its horizontal extent and
remains inside the reserved scene band.

## Shared point-radius guard

Reject unsupported explicit series and per-point radii before any fallible
output backend runs. Put the checks in one radius-only helper called by
`guard::validate_spec_base` and by the fallible SVG, PNG, and WebP render
entry points without broadening their unrelated input-policy contract:

- `None`, zero, and negative finite values preserve existing marker-default or
  marker-suppression semantics;
- positive finite radii through `DEFAULT_MAX_DIMENSION_PX` (`32768`) remain
  accepted because a larger marker adds no useful visible distinction within
  the maximum supported scene dimension;
- non-finite radii and positive values above `32768` return stable
  field-specific validation errors.

The raster path additionally validates every circle after output scaling. Its
center, radius, and positive half-stroke extent must remain within a
conservative device-coordinate span derived from tiny-skia's signed 24.8
fixed-point edge representation; otherwise rendering returns an error before
the scan converter. Tests cover direct IR, series and Bubble point radii,
Chart.js `1e40`, the reported panic inputs, the exact shared boundary, and the
device-space scale boundary.

## Completion

Each fix is developed test-first and independently reviewed. Rebase on current
`origin/main`, run focused and full workspace gates, and require 100%
committed-HEAD changed-line coverage. Push, reply to and resolve both exact
threads, fresh-fetch review state, require zero unresolved threads, watch PR
checks to terminal green, close the Bead, push Beads state, and verify the
branch is clean and synchronized. Do not merge the PR.
