# PR #137 Ninth Review Fix Design

## Context

The remaining current review thread, `PRRT_kwDOS-i3R86Ty4gU`, identifies an
overflow in scale normalization. A valid finite mixed domain such as
`[-1e308, 1e308]` has an infinite `d1 - d0`. The current
`LinearScale::map` consequently evaluates an endpoint as `inf / inf`, emits
`NaN`, and the SVG number formatter later collapses the coordinate to zero.

Rejecting these finite values or narrowing the tick domain would lose accepted
data. The repair belongs in `LinearScale`, which owns value-to-pixel
normalization for every renderer.

## Overflow-safe normalization

Keep the existing direct calculation for ordinary finite spans so established
output remains byte-stable:

```text
t = (v - d0) / (d1 - d0)
```

When the domain span overflows despite finite inputs, normalize all three
domain values by the largest endpoint magnitude first:

```text
scale = max(abs(d0), abs(d1))
t = (v / scale - d0 / scale) / (d1 / scale - d0 / scale)
```

The scaled endpoints remain bounded, their difference is finite, and endpoint
and midpoint mappings preserve the original affine relationship. Degenerate
domains retain the existing `p0` fallback. Existing behavior for explicitly
non-finite constructor inputs is not broadened into a new public contract.

## Tests

Test first at the responsibility boundary:

- `LinearScale(-1e308, 1e308, 300, 0)` maps the lower endpoint to `300`, zero
  to `150`, and the upper endpoint to `0`, all finitely;
- the non-inverted pixel range maps the same points to `0`, `200`, and `400`;
- ordinary scale tests remain unchanged.

Add an accepted Vega-Lite temporal-line regression with two timestamps and
mixed extreme y-values. Inspect the produced scene or SVG geometry closely
enough to prove the two values occupy opposite finite plot endpoints rather
than merely asserting that the serialized SVG lacks `NaN` (the formatter can
hide `NaN` as zero).

## Completion

After independent specification and quality review, rebase on `origin/main`,
run all focused and workspace gates, and measure committed-HEAD changed-line
coverage against `origin/main...HEAD`; it must remain 100%. Push, reply to and
resolve the exact thread, fresh-fetch review state, require zero unresolved
threads, watch PR checks to terminal green, close the Bead, push Beads state,
and verify the branch is clean and synchronized.
