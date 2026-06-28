use fulgur_chart::guard::{InputLimits, validate_spec};
use napi::bindgen_prelude::Buffer;
use napi_derive::napi;

// --- error classification (by CALL SITE, never by parsing the message) ---
//
// The native layer NEVER throws: it returns a discriminated `RenderResult` and the JS
// wrapper maps `code` -> error class (FulgurParseError / StrictError / RenderError). This
// avoids betting on napi custom-error-code marshaling while keeping classification in Rust.
const PARSE_ERROR: &str = "PARSE_ERROR";
const STRICT_ERROR: &str = "STRICT_ERROR";
const RENDER_ERROR: &str = "RENDER_ERROR";

/// RenderOptions, mapped from the JS options object. All fields optional.
#[napi(object)]
pub struct RenderOptions {
    pub width: Option<f64>,
    pub height: Option<f64>,
    /// Raster scale factor (ignored when rendering SVG).
    pub scale: Option<f64>,
    /// Reject unknown keys.
    pub strict: Option<bool>,
    /// Force the input DSL ("chartjs"/"vegalite"). Omit to auto-detect.
    pub dsl: Option<String>,
    /// TrueType/OpenType font bytes. Omit to use the bundled Noto Sans JP.
    pub font: Option<Buffer>,
}

/// Discriminated render result. Exactly one of (svg, png) is set when `ok`; otherwise
/// (code, message) describe the failure.
#[napi(object)]
pub struct RenderResult {
    pub ok: bool,
    pub svg: Option<String>,
    pub png: Option<Buffer>,
    pub webp: Option<Buffer>,
    pub code: Option<String>,
    pub message: Option<String>,
}

impl RenderResult {
    fn svg(s: String) -> Self {
        Self {
            ok: true,
            svg: Some(s),
            png: None,
            webp: None,
            code: None,
            message: None,
        }
    }
    fn png(b: Vec<u8>) -> Self {
        Self {
            ok: true,
            svg: None,
            png: Some(b.into()),
            webp: None,
            code: None,
            message: None,
        }
    }
    fn webp(b: Vec<u8>) -> Self {
        Self {
            ok: true,
            svg: None,
            png: None,
            webp: Some(b.into()),
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

// --- DSL detection + parse (mirrors fulgur-chart-cli / the Ruby binding) ---

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

/// Build + validate the IR, then render. Mirrors the Ruby binding's `build_ir` + format match.
/// Returns `(code, message)` on failure; classification is decided here, at the call site.
fn render_inner(
    spec_json: &str,
    format: &str,
    options: Option<RenderOptions>,
) -> Result<Output, (&'static str, String)> {
    let opts = options;
    let dsl_opt = opts.as_ref().and_then(|o| o.dsl.clone());
    let strict = opts.as_ref().and_then(|o| o.strict).unwrap_or(false);
    let width = opts.as_ref().and_then(|o| o.width);
    let height = opts.as_ref().and_then(|o| o.height);
    let scale = opts.as_ref().and_then(|o| o.scale).unwrap_or(1.0) as f32;
    let font: Option<&[u8]> = opts.as_ref().and_then(|o| o.font.as_deref());

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
        "webp" => {
            let fb = font.as_deref().unwrap_or(fulgur_chart::font::DEFAULT_FONT);
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
#[napi]
pub fn render(spec_json: String, format: String, options: Option<RenderOptions>) -> RenderResult {
    match render_inner(&spec_json, &format, options) {
        Ok(Output::Svg(s)) => RenderResult::svg(s),
        Ok(Output::Png(b)) => RenderResult::png(b),
        Ok(Output::Webp(b)) => RenderResult::webp(b),
        Err((code, message)) => RenderResult::err(code, message),
    }
}

/// Discriminated schema result (same never-throw convention as `RenderResult`).
#[napi(object)]
pub struct SchemaResult {
    pub ok: bool,
    pub value: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
}

/// Return the JSON Schema (compact JSON string) for the given DSL ("chartjs"/"vegalite").
/// Mirrors the CLI's `run_schema`; unknown DSL -> ParseError. Never throws.
#[napi]
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
#[napi]
pub fn version() -> String {
    fulgur_chart::version().to_string()
}
