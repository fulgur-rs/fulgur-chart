use fulgur_chart::guard::InputLimits;
use wasm_bindgen::prelude::*;

#[cfg(all(feature = "bundled-font", feature = "no-default-font"))]
compile_error!(
    "features `bundled-font` and `no-default-font` are mutually exclusive; disable defaults for a slim build"
);

// --- error classification (by CALL SITE, never by parsing the message) ---
//
// The native layer NEVER throws: it returns a discriminated `RenderResult` and the JS
// wrapper maps `code` -> error class (FulgurParseError / StrictError / RenderError). This
// mirrors the Node binding and avoids constructing JS Error subclasses from Rust.
const PARSE_ERROR: &str = "PARSE_ERROR";
const STRICT_ERROR: &str = "STRICT_ERROR";
const RENDER_ERROR: &str = "RENDER_ERROR";

/// Discriminated render result. Exactly one of (svg, png, webp) is set when `ok`; otherwise
/// (code, message) describe the failure. Exposed to JS via explicit getters so that
/// `png`/`webp` surfaces as a `Uint8Array` (Vec<u8>) and the string fields as `string`.
#[wasm_bindgen]
pub struct RenderResult {
    ok: bool,
    svg: Option<String>,
    png: Option<Vec<u8>>,
    webp: Option<Vec<u8>>,
    code: Option<String>,
    message: Option<String>,
}

#[wasm_bindgen]
impl RenderResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }
    #[wasm_bindgen(getter)]
    pub fn svg(&self) -> Option<String> {
        self.svg.clone()
    }
    /// `Uint8Array | undefined` on the JS side.
    #[wasm_bindgen(getter)]
    pub fn png(&self) -> Option<Vec<u8>> {
        self.png.clone()
    }
    /// `Uint8Array | undefined` on the JS side.
    #[wasm_bindgen(getter)]
    pub fn webp(&self) -> Option<Vec<u8>> {
        self.webp.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> Option<String> {
        self.code.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> Option<String> {
        self.message.clone()
    }
}

impl RenderResult {
    fn ok_svg(s: String) -> Self {
        Self {
            ok: true,
            svg: Some(s),
            png: None,
            webp: None,
            code: None,
            message: None,
        }
    }
    fn ok_png(b: Vec<u8>) -> Self {
        Self {
            ok: true,
            svg: None,
            png: Some(b),
            webp: None,
            code: None,
            message: None,
        }
    }
    fn ok_webp(b: Vec<u8>) -> Self {
        Self {
            ok: true,
            svg: None,
            png: None,
            webp: Some(b),
            code: None,
            message: None,
        }
    }
    fn err(code: &str, message: String) -> Self {
        Self {
            ok: false,
            svg: None,
            png: None,
            webp: None,
            code: Some(code.to_string()),
            message: Some(message),
        }
    }
}

// --- DSL detection + parse (mirrors the Node / Ruby bindings) ---

#[derive(serde::Deserialize)]
struct DslDetector {
    mark: Option<serde::de::IgnoredAny>,
    #[serde(rename = "type")]
    r#type: Option<serde::de::IgnoredAny>,
}

/// Infer DSL from spec JSON: `mark` key -> vegalite, `type` key -> chartjs, neither -> Err.
fn detect_dsl(json: &str) -> Result<&'static str, String> {
    let d: DslDetector = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    if d.mark.is_some() {
        return Ok("vegalite");
    }
    if d.r#type.is_some() {
        return Ok("chartjs");
    }
    Err("cannot auto-detect DSL: specify dsl: 'chartjs' or 'vegalite'".to_string())
}

/// Parse a spec JSON string to IR using the specified DSL.
fn parse_spec(json: &str, dsl: &str, strict: bool) -> Result<fulgur_chart::ir::ChartSpec, String> {
    match dsl {
        "vegalite" => fulgur_chart::frontend::vegalite::parse(json, strict),
        _ => fulgur_chart::frontend::chartjs::parse(json, strict), // "chartjs"
    }
}

enum Output {
    Svg(String),
    Png(Vec<u8>),
    Webp(Vec<u8>),
}

/// Build + validate the IR, then render. Mirrors the Node binding's `render_inner`.
/// Returns `(code, message)` on failure; classification is decided here, at the call site.
#[allow(clippy::too_many_arguments)]
fn render_inner(
    spec_json: &str,
    format: &str,
    width: Option<f64>,
    height: Option<f64>,
    scale: Option<f64>,
    strict: Option<bool>,
    dsl_opt: Option<String>,
    font: Option<&[u8]>,
) -> Result<Output, (&'static str, String)> {
    let strict = strict.unwrap_or(false);
    let scale = scale.unwrap_or(1.0) as f32;

    // 1. Resolve DSL: explicit OR auto-detect.
    let dsl: String = match dsl_opt {
        Some(d) => {
            if d != "chartjs" && d != "vegalite" {
                return Err((PARSE_ERROR, format!("unsupported DSL '{d}'")));
            }
            d
        }
        None => detect_dsl(spec_json)
            .map_err(|e| (PARSE_ERROR, e))?
            .to_string(),
    };

    // 2. Parse NON-strict -> IR (render from this).
    let mut ir = parse_spec(spec_json, &dsl, false).map_err(|e| (PARSE_ERROR, e))?;

    // 3. If strict, re-parse with strict=true (unknown key -> StrictError).
    if strict {
        parse_spec(spec_json, &dsl, true).map_err(|e| (STRICT_ERROR, e))?;
    }

    // 4. Apply width/height overrides BEFORE guard.
    if let Some(w) = width {
        ir.width = w;
    }
    if let Some(h) = height {
        ir.height = h;
    }

    // 5. Guard (failure -> ParseError). The slim build measures plot-area
    // line charts with the caller's font because the default font is absent.
    #[cfg(feature = "bundled-font")]
    fulgur_chart::guard::validate_spec(&ir, &InputLimits::default())
        .map_err(|e| (PARSE_ERROR, e))?;

    #[cfg(not(feature = "bundled-font"))]
    {
        let font_bytes = font.ok_or_else(|| {
            (
                PARSE_ERROR,
                "font bytes are required in a no-default-font build".to_string(),
            )
        })?;
        let measurer = fulgur_chart::text::TextMeasurer::new(font_bytes).map_err(|e| {
            let code = if format == "svg" {
                PARSE_ERROR
            } else {
                RENDER_ERROR
            };
            (code, e)
        })?;
        fulgur_chart::guard::validate_spec_with_measurer(&ir, &InputLimits::default(), &measurer)
            .map_err(|e| (PARSE_ERROR, e))?;
    }

    // 6. Render by format.
    match format {
        "svg" => {
            // Font present -> render_chart_with_font (Err -> ParseError on the SVG path);
            // else the bundled-font render.
            let svg = match font {
                Some(bytes) => fulgur_chart::render::render_chart_with_font(&ir, bytes)
                    .map_err(|e| (PARSE_ERROR, e))?,
                #[cfg(feature = "bundled-font")]
                None => {
                    fulgur_chart::render::render_chart_with_limits(&ir, &InputLimits::default())
                        .map_err(|e| (PARSE_ERROR, e))?
                }
                #[cfg(not(feature = "bundled-font"))]
                None => {
                    return Err((
                        PARSE_ERROR,
                        "font bytes are required in a no-default-font build".to_string(),
                    ));
                }
            };
            Ok(Output::Svg(svg))
        }
        "png" => {
            let fb: &[u8] = font.unwrap_or(fulgur_chart::font::DEFAULT_FONT);
            // Invalid font on the image path -> RenderError (the SVG path maps this to ParseError).
            let png = fulgur_chart::raster_direct::render_chart_to_png(&ir, scale, fb)
                .map_err(|e| (RENDER_ERROR, e))?;
            Ok(Output::Png(png))
        }
        "webp" => {
            let fb: &[u8] = font.unwrap_or(fulgur_chart::font::DEFAULT_FONT);
            let webp = fulgur_chart::raster_direct::render_chart_to_webp(&ir, scale, fb)
                .map_err(|e| (RENDER_ERROR, e))?;
            Ok(Output::Webp(webp))
        }
        other => Err((
            PARSE_ERROR,
            format!("unsupported format '{other}' (supported: svg, png, webp)"),
        )),
    }
}

/// Low-level render primitive. Never throws; returns a discriminated `RenderResult`.
/// The JS `Builder` (`build(...)`) is the intended API and calls this under the hood.
/// Options are passed positionally (wasm-bindgen has no JS-object -> struct auto-map);
/// the JS wrapper unpacks its options object into these arguments.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn render(
    spec_json: String,
    format: String,
    width: Option<f64>,
    height: Option<f64>,
    scale: Option<f64>,
    strict: Option<bool>,
    dsl: Option<String>,
    font: Option<Vec<u8>>,
) -> RenderResult {
    match render_inner(
        &spec_json,
        &format,
        width,
        height,
        scale,
        strict,
        dsl,
        font.as_deref(),
    ) {
        Ok(Output::Svg(s)) => RenderResult::ok_svg(s),
        Ok(Output::Png(b)) => RenderResult::ok_png(b),
        Ok(Output::Webp(b)) => RenderResult::ok_webp(b),
        Err((code, message)) => RenderResult::err(code, message),
    }
}

/// Discriminated schema result (same never-throw convention as `RenderResult`).
#[wasm_bindgen]
pub struct SchemaResult {
    ok: bool,
    value: Option<String>,
    code: Option<String>,
    message: Option<String>,
}

#[wasm_bindgen]
impl SchemaResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> Option<String> {
        self.value.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> Option<String> {
        self.code.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> Option<String> {
        self.message.clone()
    }
}

/// Return the JSON Schema (compact JSON string) for the given DSL ("chartjs"/"vegalite").
/// Unknown DSL -> ParseError. Never throws.
#[wasm_bindgen]
pub fn schema(dsl: String) -> SchemaResult {
    let json = match dsl.as_str() {
        "chartjs" => include_str!("chartjs-schema.json"),
        "vegalite" => include_str!("vegalite-schema.json"),
        other => {
            return SchemaResult {
                ok: false,
                value: None,
                code: Some(PARSE_ERROR.to_string()),
                message: Some(format!(
                    "unsupported DSL '{other}' (supported: chartjs, vegalite)"
                )),
            };
        }
    };
    SchemaResult {
        ok: true,
        value: Some(json.to_string()),
        code: None,
        message: None,
    }
}

/// Return the crate version string (mirrors the CLI / other bindings).
#[wasm_bindgen]
pub fn version() -> String {
    fulgur_chart::version().to_string()
}

#[cfg(test)]
mod schema_fixture_tests {
    #[test]
    fn embedded_schemas_match_rust_schema_types() {
        let chartjs =
            serde_json::to_string(&schemars::schema_for!(fulgur_chart::schema::ChartJsSpec))
                .unwrap();
        let vegalite =
            serde_json::to_string(&schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec))
                .unwrap();

        for (dsl, actual, expected) in [
            ("chartjs", chartjs, include_str!("chartjs-schema.json")),
            ("vegalite", vegalite, include_str!("vegalite-schema.json")),
        ] {
            assert!(actual == expected, "embedded {dsl} schema is stale");
        }
    }

    #[test]
    fn embedded_vegalite_schema_covers_rule_and_error_marks_and_rejects_unknown_keys() {
        use serde_json::Value;

        let embedded_schema: Value = serde_json::from_str(include_str!("vegalite-schema.json"))
            .expect("embedded Vega-Lite schema is valid JSON");
        assert_eq!(
            embedded_schema["$defs"]["MarkRuleObject"]["properties"]["strokeDash"]["maxItems"],
            serde_json::json!(fulgur_chart::guard::MAX_BORDER_DASH_ELEMENTS),
            "rule strokeDash schema must expose its bounded length"
        );

        for (name, example) in [
            (
                "raw errorbar",
                include_str!("../../../../examples/specs/vegalite-errorbar-raw.json"),
            ),
            (
                "pre-aggregated errorbar",
                include_str!("../../../../examples/specs/vegalite-errorbar-preaggregated.json"),
            ),
            (
                "raw errorband",
                include_str!("../../../../examples/specs/vegalite-errorband-raw.json"),
            ),
            (
                "pre-aggregated errorband",
                include_str!("../../../../examples/specs/vegalite-errorband-preaggregated.json"),
            ),
            (
                "rule mark",
                include_str!("../../../../examples/specs/vegalite-rule.json"),
            ),
            (
                "layer composition",
                include_str!("../../../../examples/specs/vegalite-layer.json"),
            ),
            (
                "nested concat composition",
                include_str!("../../../../examples/specs/vegalite-nested-concat.json"),
            ),
        ] {
            serde_json::from_str::<fulgur_chart::schema::VegaLiteSpec>(example)
                .unwrap_or_else(|error| panic!("{name} example rejected by schema: {error}"));
        }

        let mut unsupported_style: Value = serde_json::from_str(include_str!(
            "../../../../examples/specs/vegalite-errorband-preaggregated.json"
        ))
        .unwrap();
        unsupported_style["mark"]["borders"]["futureOption"] = Value::Bool(true);
        assert!(
            serde_json::from_value::<fulgur_chart::schema::VegaLiteSpec>(unsupported_style)
                .is_err(),
            "unsupported errorband part styles must be rejected"
        );

        let mut unsupported_channel: Value = serde_json::from_str(include_str!(
            "../../../../examples/specs/vegalite-errorbar-raw.json"
        ))
        .unwrap();
        unsupported_channel["encoding"]["size"] = serde_json::json!({"field":"value"});
        assert!(
            serde_json::from_value::<fulgur_chart::schema::VegaLiteSpec>(unsupported_channel)
                .is_err(),
            "unsupported error mark encoding channels must be rejected"
        );

        let mut unsupported_rule_style: Value = serde_json::from_str(include_str!(
            "../../../../examples/specs/vegalite-rule.json"
        ))
        .unwrap();
        unsupported_rule_style["mark"]["futureOption"] = Value::Bool(true);
        assert!(
            serde_json::from_value::<fulgur_chart::schema::VegaLiteSpec>(unsupported_rule_style)
                .is_err(),
            "unsupported rule mark styles must be rejected"
        );
    }

    #[test]
    fn embedded_chartjs_schema_includes_title_and_subtitle_for_all_kinds() {
        let embedded: serde_json::Value =
            serde_json::from_str(include_str!("chartjs-schema.json")).unwrap();
        let generated =
            serde_json::to_value(schemars::schema_for!(fulgur_chart::schema::ChartJsSpec)).unwrap();
        assert_eq!(embedded, generated, "embedded Chart.js schema is stale");

        let definitions = embedded["$defs"].as_object().expect("schema definitions");
        for plugin_type in [
            "BarPlugins",
            "CommonPlugins",
            "GaugePlugins",
            "ProgressPlugins",
            "MatrixPlugins",
            "TreemapPlugins",
            "SparklinePlugins",
            "OutlabeledPiePlugins",
            "WordCloudPlugins",
            "SankeyPlugins",
        ] {
            let properties = definitions[plugin_type]["properties"]
                .as_object()
                .unwrap_or_else(|| panic!("{plugin_type} schema properties missing"));
            assert!(
                properties.contains_key("title"),
                "{plugin_type} schema must expose title"
            );
            assert!(
                properties.contains_key("subtitle"),
                "{plugin_type} schema must expose subtitle"
            );
        }
    }
}
