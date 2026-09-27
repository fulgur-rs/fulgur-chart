# fulgur-chart-312: Vega-Lite geoshape

**Date:** 2026-09-27

**Status:** Approved for implementation

**Issue:** `fulgur-chart-312`

## Goal

Add Vega-Lite `mark: "geoshape"` for inline GeoJSON and choropleth rendering. Native and WASM callers must use the same parser, projection, layout, guard, and scene-building path.

The first implementation covers every GeoJSON geometry type and wrapper, the complete projection type enum in the current Vega-Lite v6 schema, and automatic fitting of projected geometry to the plot area.

## Input and rendering contract

- Accept `data.values` as inline records, a GeoJSON Feature array, or a single GeoJSON Feature/FeatureCollection. In ordinary records, `encoding.shape.field` contains a GeoJSON Geometry, Feature, or FeatureCollection. Feature arrays and FeatureCollections normalize to one record per feature, exposing its `geometry` and `properties`; `encoding.shape.field` may select a GeoJSON field, and otherwise the feature geometry is used. GeometryCollection is handled recursively. Feature properties remain available to color encoding.
- Support Point, MultiPoint, LineString, MultiLineString, Polygon, MultiPolygon, and GeometryCollection. Lines render as unfilled paths; polygons preserve interior rings; points use the projection's `pointRadius` default of 4.5 px.
- Use `encoding.color` for choropleths. Quantitative values use a continuous interpolation over the data domain; nominal and ordinal values use the existing categorical palette behavior. Resolve a color field from the record first, then from the corresponding GeoJSON Feature properties, so either source can drive the map. Constant `color` and the mark's fill/stroke styling remain available.
- Preserve ordinary chart sizing, title, background, and output behavior. Projected marks occupy the plot area and are emitted as the existing scene path/circle primitives, so SVG and raster renderers consume the same geometry.

## Projection contract

Accept the 16 projection types enumerated by the current Vega-Lite v6 JSON schema:

`albers`, `albersUsa`, `azimuthalEqualArea`, `azimuthalEquidistant`, `conicConformal`, `conicEqualArea`, `conicEquidistant`, `equalEarth`, `equirectangular`, `gnomonic`, `identity`, `mercator`, `naturalEarth1`, `orthographic`, `stereographic`, and `transverseMercator`.

`mollweide` is not in that Vega-Lite v6 enum and is not included. Unknown names return an error. Projection type matching is case-insensitive, in line with Vega-Lite.

The default projection is `equalEarth`, matching Vega-Lite.

Support the common projection properties used by those types: `center`, `rotate`, `clipAngle`, `clipExtent`, `parallels`, `pointRadius`, `precision`, `scale`, and `translate`; support `reflectX` and `reflectY` for `identity`. When no explicit scale/translation fixes the view transform, fit the projected geometry bounds into the plot area with a small inner margin while preserving aspect ratio. Explicit projection values take precedence over the automatically computed values. Apply projection clipping before scene emission.

Use a shared Rust projection/GeoJSON path layer and adapt its output to Fulgur's restricted SVG path syntax. The adapter must consume projected geometry/path events and emit explicit, whitespace-separated numeric path tokens accepted by both SVG serialization and the raster path parser; it must not forward compact third-party path strings. Keep any backend dependency pure Rust and usable without browser APIs so the same implementation compiles for WASM.

The candidate backend is `d3_geo_rs` 3.1.4 (MIT), with default features disabled to omit its optional `web-sys` rendering feature. Its documented projection set covers the Vega-Lite enum except `naturalEarth1`, which must be supplied by a local projector or an equivalent backend. The current v6 enum's `identity` behavior can use a direct planar transform with the standard scale/translation/reflection settings. The crate declares `getrandom` 0.2.16 with its `js` feature enabled. Before adopting it, verify `wasm32-unknown-unknown` compilation; do not add a redundant target-specific `getrandom` feature if the pinned backend already supplies it.

## Internal design

- Add a dedicated geoshape payload to the chart IR, containing validated GeoJSON features, resolved per-feature style values, and projection settings. Keep nested GeoJSON out of the existing numeric `Series` representation.
- Add `ChartKind::GeoShape` and a dedicated layout module. Extend the Vega-Lite parser, typed schema, chart model type name, layout dispatch, and strict-key checks consistently.
- Parse and validate GeoJSON before allocating projected paths. Enforce limits for feature count, total coordinate vertices, and emitted path/primitive count through the existing input guard. Reject malformed coordinate nesting, non-finite values, and unsupported data formats with path-aware errors.
- Treat a GeoJSON Feature whose `geometry` is `null` as valid empty geometry; reject malformed Features and malformed non-null geometries.
- Normalize polygon ring winding before emission so holes render consistently in the existing nonzero-winding SVG and raster path pipelines.
- Preserve feature order in the scene to keep draw order deterministic. A FeatureCollection expands in source order.

## Errors and explicit non-goals

- URL data and TopoJSON are unsupported in this issue. Return an actionable error that names the unsupported source/format and points users to inline GeoJSON.
- Do not add network access, TopoJSON decoding, graticule generation, map tiles, or longitude/latitude point mark projections outside `geoshape`.
- Do not silently omit an unsupported geometry, projection, or malformed feature; fail with a useful parser error.

## Validation plan

- Parser/schema coverage for each GeoJSON wrapper and geometry kind, projection names, properties, and unsupported URL/TopoJSON cases.
- Geometry tests for polygon holes, multipolygons, line and point output, stable feature order, projection clipping, and auto-fit bounds.
- Exercise every supported projection with representative input and assert finite, in-bounds output; include `naturalEarth1` specifically.
- Native and WASM compile/render checks use the same geoshape fixtures. Add a compact inline choropleth example and an SVG/raster golden to pin the rendering contract.
- Add guard tests for feature, vertex, and primitive limits, including over-limit inputs rejected before path allocation.

## References

- [Vega-Lite geoshape](https://vega.github.io/vega-lite/docs/geoshape.html)
- [Vega-Lite projection documentation](https://vega.github.io/vega-lite/docs/projection.html)
- [Vega-Lite v6 JSON schema projection enum](https://vega.github.io/schema/vega-lite/v6.json) (`definitions.ProjectionType`)
- [Vega projections](https://vega.github.io/vega/docs/projections/)
- [`d3_geo_rs` 3.1.4 API and feature documentation](https://docs.rs/d3_geo_rs/latest/d3_geo_rs/)
- [`getrandom` 0.2.16 feature documentation](https://docs.rs/crate/getrandom/0.2.16/features)
