# Vega-Lite layer / concat composition

## Purpose

Vega-Lite input currently describes one view at a time. This change adds nested `layer`, `hconcat`, and `vconcat` views so users can express overlaid marks and side-by-side or vertically stacked charts through the existing native and WASM render paths. Existing unit-mark behavior, child order, SVG image references, and PNG/WebP image errors remain unchanged.

## Scope

- Accept every Vega-Lite unit mark already accepted by the frontend as a composition leaf, including the existing special parsers for boxplot, error marks, geoshape, and image.
- Accept recursive `layer`, `hconcat`, and `vconcat` nodes and preserve child order.
- Resolve inherited inline data and encoding before parsing each unit leaf.
- Implement shared/independent scale, axis, and legend resolution for the channels supported by the existing leaf parsers.
- Compose output through the shared Scene pipeline so SVG, PNG, WebP, native, and WASM use the same geometry and ordering.
- Keep unit specs on the existing `ChartSpec` path. Add a recursive Vega-Lite composition variant to the IR for composition roots and nested nodes.

Explicitly reject `transform`, `facet`, `repeat`, general `concat`, and URL-backed data at any nesting depth. This issue does not add marks or encoding channels, selection/parameter support, or data transforms.

## Input and inheritance contract

A composition node has exactly one of `layer`, `hconcat`, or `vconcat`. Its child array must contain at least one object spec. Any currently supported unit mark can be a leaf. Layer children must be compatible Cartesian views; `arc` and `geoshape` leaves remain valid in concat nodes but cannot share a layer plot frame. Nested composition children are parsed recursively; a `layer` can contain unit or nested-layer children, while concat nodes can contain unit or supported composition children. Invalid combinations return a path-qualified error.

Inline `data.values` is inherited down the composition tree. A child `data` object replaces its inherited data; values are never concatenated implicitly. Each leaf must resolve to an inline array of records. `data.url` is rejected even in non-strict mode.

Layer-level `encoding` mappings are inherited by descendants. Mappings are merged by channel: a closer child mapping replaces the inherited mapping for that channel, while other inherited channels remain available. The same rule applies through nested layer nodes. Concat nodes cannot declare `encoding`, matching Vega-Lite's shared-encoding contract for layers. Each leaf then goes through its existing mark-specific channel validation; a missing or incompatible required channel reports the leaf path.

The parser rejects `transform`, `facet`, `repeat`, and `concat` wherever they appear, regardless of strict mode. Strict mode continues to reject unknown keys using the existing allowlists. A composition object cannot also contain `mark`; a unit object cannot also contain a composition operator.

`resolve` applies to its node and descendants. Per channel, the nearest explicit setting wins; unspecified settings inherit from the parent composition node, and the top-level composition uses the defaults for its kind. Supported scale channels are `x`, `y`, `color`, and `size`; supported axis channels are `x` and `y`; supported legend channels are `color` and `size`. Other resolution channels fail with a path-qualified error because the current leaf parsers do not produce them.

Resolution defaults follow Vega-Lite: layer scales, axes, and legends are shared; concat position scales and axes are independent, and non-position scales and legends are shared. Explicit `shared` and `independent` values are honored when the resulting combination is supported by the child marks. An independent scale requires its corresponding axis or legend to be independent; explicitly combining an independent scale with a shared guide is an error. With a shared scale, its guide may still be independently rendered when requested. Shared scales union compatible child domains: categorical values preserve first-seen child and input order; numeric and temporal domains use the finite extrema. Incompatible channel types or scale kinds under a shared scale fail instead of silently using one child's domain. Shared color and size legends use the resolved shared domain and are drawn once.

Axis sharing is valid for `layer`; concat axes remain independent, matching Vega-Lite's current concat contract. An explicit shared concat axis is rejected. In layers, independent axes are supported only when the corresponding scale is also independent. For independent axes, the first y-axis is placed left of the plot and later y-axes are placed to its right in layer order; the first x-axis is below the plot and later x-axes are above it in layer order. Each added axis receives its own gutter. Conflicting axis orientations or incompatible plot families are rejected with a path-qualified error.

## IR and parsing

Add recursive `VegaCompositionNode` data with `Unit`, `Layer`, `HConcat`, and `VConcat` variants. Composition nodes retain ordered children, their local resolution, and concat spacing. A unit leaf contains the fully parsed existing `ChartSpec`; there is no second implementation of leaf mark parsing.

The public `VegaLiteSpec` schema becomes recursive and describes the three composition forms as well as existing unit forms. Parsing dispatches composition objects before unit-mark parsing, checks unsupported constructs and resource bounds before building leaf data structures, resolves inherited data/encoding, then calls the current unit parser for each leaf. Error paths use stable JSON-style paths such as `layer[1].encoding.y`.

Add composition guard limits to `InputLimits`: maximum recursive composition depth (default 32) and maximum unit-view count (default 256). The parser checks depth before descending and counts leaves before parsing them. Empty children, too many children/views, invalid spacing, and a child cell below the configured minimum dimension are errors before Scene allocation.

## Layout and output

Add a shared `vega_composition` layout module. It recursively lowers the IR into Scene groups and keeps array order for paint order: earlier layer children are behind later children. Layer children share a common plot rectangle; their resolved axes and legends are emitted according to the node resolution, while each leaf contributes its mark primitives inside the same clipped plot rectangle.

For `hconcat` and `vconcat`, child scenes are placed in order with `Prim::Group` translations. A node's `width` and `height` are inherited as per-view defaults; explicit unit dimensions override those defaults. A layer requires equal resolved child dimensions. An hconcat output width is the sum of child widths and gaps, with the maximum child height; vconcat uses the maximum child width and the sum of heights and gaps. `spacing` defaults to 20 px and must be finite, nonnegative, and within the dimension guard. The outer scene reports the derived dimensions. Existing leaf titles remain inside their own views; a title on any composition node is drawn once above that node's composed scene, and the node's child area is reduced to make room. A root background is applied once to the full output.

Nested scenes retain their clips and translations. A Vega-Lite image leaf remains an SVG `<image>` reference; raster renderers continue to return the existing explicit unsupported-image error if any composed leaf contains an image mark.

## Failure behavior

Malformed composition nodes, unsupported constructs, inheritance failures, incompatible shared domains, invalid resolution combinations, and resource-limit violations return `Err` with the failing composition path. No unsupported property is silently ignored in non-strict mode. Existing leaf errors remain mark-specific and are prefixed with the leaf path. No panic or partial Scene is returned for invalid input.

## Validation

- Schema tests accept each composition form and recursive supported nesting, and reject malformed arrays, mixed unit/composition objects, and unsupported operators.
- Frontend tests cover parent data/encoding inheritance, nearest-child overrides, child data replacement, resolution defaults and overrides, path-qualified errors, and strict/non-strict rejection of unsupported constructs.
- IR tests verify recursive node shape, child order, inherited leaf specs, and composition guard boundaries.
- Native layout tests cover layer paint order and common domains/guides, independent layer axes, hconcat/vconcat positions and dimensions, nested composition, clipping, shared legends, and incompatible shared-scale errors.
- Add one layer example and one nested concat example; include them in golden SVG/PNG coverage. Compare native and WASM results for the same examples and error cases.
- Run the workspace test suite, WASM runtime tests, schema generation/parity checks, and golden image tests before merge.

## References

- [Vega-Lite layering](https://vega.github.io/vega-lite/docs/layer.html)
- [Vega-Lite concatenation](https://vega.github.io/vega-lite/docs/concat.html)
- [Vega-Lite scale and guide resolution](https://vega.github.io/vega-lite/docs/resolve.html)
