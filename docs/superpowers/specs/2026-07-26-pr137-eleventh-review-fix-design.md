# PR #137 Eleventh Review Fix Design

## Context

The remaining current thread, `PRRT_kwDOS-i3R86T1Qb5`, identifies that
`validate_line_channel_types` is reachable only through the strict-only
unknown-key preflight. Non-strict parsing therefore ignores recognized invalid
line type declarations and can reinterpret them as a different supported
chart shape.

Unknown future keys remain intentionally tolerated in non-strict mode, but a
recognized `type` field has semantic meaning and must not be silently
discarded.

## Shared semantic validation

After the top-level object, mark, and encoding object have been parsed,
unconditionally run `validate_line_channel_types` for line charts. Keep the
existing strict preflight call in `check_line_keys` so strict-mode error
precedence relative to structural unknown-key checks remains unchanged. The
second strict invocation is harmless and reached only after the same semantic
validation has already succeeded.

The supported subset remains unchanged:

- `x.type`: `temporal`, `nominal`, or `ordinal`;
- `y.type`: `quantitative`;
- `color.type`: `nominal` or `ordinal`;
- missing or JSON `null` types remain accepted and inferred by existing logic.

Non-string recognized values and unsupported strings return the same exact
error in strict and non-strict modes. Unknown keys continue to be ignored only
in non-strict mode.

## Tests and completion

Cross-mode tests cover number, boolean, array, and unsupported string values
for x/y/color; compatibility tests preserve missing/null types and the
strict/non-strict unknown-key boundary.

After independent reviews, rebase on current `origin/main`, run all focused
and workspace gates, and require 100% committed-HEAD changed-line coverage.
Push, reply to and resolve the exact thread, fresh-fetch all review state,
require zero unresolved threads, watch CI to terminal green, close the Bead,
push Beads state, and verify a clean synchronized branch. Do not merge the PR.
