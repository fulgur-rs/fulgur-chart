use fulgur_chart::guard::{InputLimits, validate_spec};
use wasm_bindgen::prelude::*;

// --- error classification (by CALL SITE, never by parsing the message) ---
//
// The native layer NEVER throws: it returns a discriminated `RenderResult` and the JS
// wrapper maps `code` -> error class (FulgurParseError / StrictError / RenderError). This
// mirrors the Node binding and avoids constructing JS Error subclasses from Rust.
const PARSE_ERROR: &str = "PARSE_ERROR";
const STRICT_ERROR: &str = "STRICT_ERROR";
const RENDER_ERROR: &str = "RENDER_ERROR";

/// Discriminated render result. Exactly one of (svg, png) is set when `ok`; otherwise
/// (code, message) describe the failure. Exposed to JS via explicit getters so that
/// `png` surfaces as a `Uint8Array` (Vec<u8>) and the string fields as `string`.
#[wasm_bindgen]
pub struct RenderResult {
    ok: bool,
    svg: Option<String>,
    png: Option<Vec<u8>>,
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
            code: None,
            message: None,
        }
    }
    fn ok_png(b: Vec<u8>) -> Self {
        Self {
            ok: true,
            svg: None,
            png: Some(b),
            code: None,
            message: None,
        }
    }
    fn err(code: &str, message: String) -> Self {
        Self {
            ok: false,
            svg: None,
            png: None,
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

    // 5. Guard (failure -> ParseError).
    validate_spec(&ir, &InputLimits::default()).map_err(|e| (PARSE_ERROR, e))?;

    // 6. Render by format.
    match format {
        "svg" => {
            // Font present -> render_chart_with_font (Err -> ParseError on the SVG path);
            // else the bundled-font render.
            let svg = match font {
                Some(bytes) => fulgur_chart::render::render_chart_with_font(&ir, bytes)
                    .map_err(|e| (PARSE_ERROR, e))?,
                None => fulgur_chart::render::render_chart(&ir),
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
        other => Err((
            PARSE_ERROR,
            format!("unsupported format '{other}' (supported: svg, png)"),
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
    let s = match dsl.as_str() {
        "chartjs" => schemars::schema_for!(fulgur_chart::schema::ChartJsSpec),
        "vegalite" => schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec),
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
    match serde_json::to_string(&s) {
        Ok(json) => SchemaResult {
            ok: true,
            value: Some(json),
            code: None,
            message: None,
        },
        Err(e) => SchemaResult {
            ok: false,
            value: None,
            code: Some(RENDER_ERROR.to_string()),
            message: Some(format!("schema serialization: {e}")),
        },
    }
}

/// Return the crate version string (mirrors the CLI / other bindings).
#[wasm_bindgen]
pub fn version() -> String {
    fulgur_chart::version().to_string()
}
