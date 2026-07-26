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

Reject unsupported explicit series radii before any output backend runs. Add
the check to `guard::validate_spec_base`, which is shared by SVG, PNG, and WebP
render entry points:

- `None`, zero, and negative finite values preserve existing marker-default or
  marker-suppression semantics;
- positive finite radii through `f32::MAX` remain accepted;
- non-finite radii and positive values above `f32::MAX` return one stable
  validation error.

This boundary prevents `f64` values accepted by Scene/SVG from becoming
infinite during tiny-skia conversion while keeping parsing independent from a
specific output format. Tests will cover direct IR validation and Chart.js
render entry points, including `1e40`, and prove SVG/PNG/WebP reject the same
spec before drawing.

## Completion

Each fix is developed test-first and independently reviewed. Rebase on current
`origin/main`, run focused and full workspace gates, and require 100%
committed-HEAD changed-line coverage. Push, reply to and resolve both exact
threads, fresh-fetch review state, require zero unresolved threads, watch PR
checks to terminal green, close the Bead, push Beads state, and verify the
branch is clean and synchronized. Do not merge the PR.
