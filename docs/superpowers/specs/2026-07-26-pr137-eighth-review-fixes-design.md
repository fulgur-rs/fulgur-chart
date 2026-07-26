# PR #137 Eighth Review Fixes Design

## Context

Two current review threads remain after the seventh review round:

1. `monotone_path` computes secants from real x widths, but combines adjacent
   secants with an unweighted average and uses raw endpoint secants. This
   differs from D3 `curveMonotoneX`, which Vega uses for monotone line marks.
2. The non-strict temporal-line path treats a present non-object top-level
   `config` as though it were absent, while strict parsing reports
   `config must be an object`.

The goal is semantic Vega-Lite parity without broadening the accepted temporal
input surface or changing non-strict unknown-key tolerance.

## Monotone-X interpolation

Use D3's current `curveMonotoneX` tangent construction as the oracle:

- calculate adjacent secants from their actual x widths;
- calculate each interior tangent with the interval-width-weighted Steffen
  candidate used by D3's `slope3`;
- derive the first and last one-sided tangents with D3's `slope2` correction;
- convert the resulting Hermite tangents to cubic Bézier control points using
  one third of each segment's x width.

Retain the existing defensive contract for non-finite coordinates and
zero-width segments: emitted SVG path tokens must remain finite. Keep the
two-point linear fallback and the existing formatting behavior.

Regression tests will pin an irregularly spaced three-point path to the D3
control values, prove equal-spacing behavior remains deterministic, and retain
duplicate-x/non-finite safety checks.

## Top-level temporal config validation

Introduce one shared reader for the optional top-level `config` container:

- missing or JSON `null` returns no config;
- an object returns the borrowed object;
- every other JSON type returns exactly `config must be an object`.

Both temporal view validation and temporal axis-grid parsing will use this
reader. Strict parsing keeps its existing allow-list checks; non-strict parsing
continues to tolerate unknown object keys. Only the recognized container type
error becomes consistent across modes.

Regression tests will cover scalar, boolean, string, and array values in both
strict and non-strict modes, plus missing/null and future-key compatibility.

## Verification and publication

Each production change is test-driven and independently reviewed. After both
fixes, run formatting, targeted tests, the full workspace test suite, clippy,
and committed-HEAD changed-line coverage against `origin/main...HEAD`. Coverage
must be 100%. Then reply to and resolve the exact two review threads, confirm
there are no unresolved threads, watch PR checks to terminal green, update and
close the Bead, rebase/push all repository and Beads state, and verify the
branch is clean and up to date.
