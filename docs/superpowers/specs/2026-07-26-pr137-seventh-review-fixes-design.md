# PR #137 Seventh Review Fixes

## Scope

Address the three valid unresolved review threads discovered after the sixth
review fixes were published:

1. Preserve meaningful y-axis scale for tiny finite temporal values.
2. Reject a recognized non-object temporal `config.axis` in both modes.
3. Reject over-limit temporal group strings before cloning them.

The changes retain the existing Vega-Lite dogfood scale for ordinary values,
non-strict unknown-key tolerance, exact label-limit errors, and all existing
allocation and tick-count bounds.

## Tiny finite Vega scale

For a valid non-degenerate finite domain, `vega_nice_ticks` derives its span
and raw step directly from the represented data magnitude. It no longer raises
positive raw steps to the absolute `f64::EPSILON` value.

Step selection is accepted only when the raw and nice steps are finite and
strictly positive. Degenerate domains, overflowed spans, and underflowed or
otherwise unusable steps fall back to the existing bounded tick path. This
keeps extreme inputs finite without destroying the relative scale of small
positive, negative, or mixed domains.

## Temporal axis container validation

Missing or JSON `null` `config.axis` retains default grid behavior. A present
non-null value must be an object in strict and non-strict modes, otherwise:

```text
config.axis must be an object
```

Once the object is established, existing shared validation for `grid` and
`gridOpacity` is unchanged. Non-strict mode continues to ignore unknown axis
keys; strict mode retains its allow-list.

## Pre-clone temporal group guard

`temporal_group` receives `max_label_bytes` and owns legend-name length
validation. In the string arm it checks the borrowed `&str` length before
constructing the key, name, or ordering strings. Numeric and boolean group
names are generated once, checked against the same limit, and then moved into
the result.

The caller's post-construction label check is removed. Error text remains:

```text
temporal legend label length <N> bytes exceeds limit <M>
```

## Verification

Red-green tests cover:

- positive `0..1e-20`, negative `-1e-20..0`, and mixed `-1e-20..1e-20`
  domains retaining scale-relative finite ticks;
- ordinary dogfood and extreme-domain scale regressions;
- non-object `config.axis` JSON kinds in strict and non-strict modes;
- missing/null axis and non-strict unknown-key compatibility;
- over-limit borrowed group strings rejected inside `temporal_group`;
- numeric and boolean generated names retaining the same byte limit.

Completion requires all repository quality gates, final committed-HEAD
changed-line coverage of 100%, exact replies/resolutions for the three
remaining threads, zero unresolved threads, green PR checks, Beads closure,
and a clean pushed branch.
