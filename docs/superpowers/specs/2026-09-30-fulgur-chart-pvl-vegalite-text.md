# Vega-Lite Text Mark: Coordinate Label MVP

## Goal

Support Vega-Lite `mark: "text"` as labels positioned at quantitative x/y coordinates, both as a unit chart and as a leaf in a Cartesian `layer`.

## Input contract

- Data is inline `data.values`; URL data and transforms remain unsupported.
- Both `encoding.x` and `encoding.y` are required field definitions. Their values must be finite numbers; the channel type may be omitted or `quantitative`. Explicit categorical, temporal, or other types fail during parsing.
- Text comes from exactly one source: `encoding.text.field`, `encoding.text.value`, or a constant `mark.text`. Field values must be non-null primitive scalars and are rendered using their JSON scalar text. Supplying multiple text sources is an error. `encoding.text.format` and conditions are explicit errors.
- `encoding.color` accepts a constant CSS color or a nominal/ordinal field resolved through the existing palette and layer scale rules. `encoding.size` accepts a positive constant font size or a quantitative field mapped over its finite data domain to the Vega-Lite default range `[8, 40]` px. `encoding.opacity` accepts a constant in `[0, 1]` or a quantitative field mapped to `[0.3, 0.8]`.
- Mark properties support `color`, `opacity`, `font`, `fontSize`, `fontWeight`, `fontStyle`, `align`, `baseline`, `angle`, `dx`, and `dy`. Encoding channels override corresponding mark defaults. Font size defaults to 11 px; text defaults to centered horizontal alignment and middle baseline. Supported align values are `left`, `center`, `right`; baseline values are `top`, `middle`, `bottom`.
- Mark text is a constant literal string. Templates, custom formatting, conditions, multiline content, and truncation (`limit`) are unsupported and fail explicitly.
- Position scales are linear. Polar and geographic positioning are unsupported.

## Rendering and composition

- Add dedicated `VegaText` IR carrying each label's text and resolved styles. Reuse the shared quantitative Cartesian axis mapping so text labels and existing scatter marks use the same coordinate and bounds rules.
- Use `StyledText` scene primitives so SVG and raster backends share text styling and clipping behavior. Add an explicit vertical baseline field to the scene primitive so SVG and raster renderers apply `top`/`middle`/`bottom` consistently. Apply `dx`/`dy` in pixels and `angle` in degrees. Convert opacity into the resolved fill alpha.
- A text unit can appear alone or as a leaf under a supported layer. Layer leaves retain the existing shared plot frame and scale resolution. This feature does not add text leaves to concat or unrelated composition forms.
- Text row count, text byte length, and primitive count are checked against the existing `InputLimits` before scene allocation, for standalone and composed charts.
- Keep native and WASM parsing/rendering behavior identical.

## Validation

- Add frontend tests for unit and layer parsing, constant/field channels, style resolution, and explicit errors for unsupported positions, channels, and properties.
- Add native Scene/SVG/render coverage and a deterministic PNG example/golden.
- Add native/WASM integration coverage using the same example and representative rejection cases.
- Verify existing Vega-Lite unit and composition tests remain unchanged.

## Out of scope

Categorical text tables, temporal/polar/geographic positions, URL data, transforms, custom formatters, conditional encodings, text templates, multiline layout, truncation, and text-specific legends.
