use fulgur_chart::guard::{validate_spec, InputLimits};
use magnus::{
    function,
    prelude::*,
    scan_args::{get_kwargs, scan_args},
    Error, ExceptionClass, RHash, RString, Ruby, Value,
};

// --- error helpers (classification is by CALL SITE, never by parsing the message) ---

fn exc_class(ruby: &Ruby, name: &str) -> ExceptionClass {
    let module = ruby.define_module("Fulgur").expect("Fulgur module defined in init");
    module
        .const_get::<_, ExceptionClass>(name)
        .expect("error class defined in init")
}

fn parse_err(ruby: &Ruby, msg: impl Into<String>) -> Error {
    Error::new(exc_class(ruby, "ParseError"), msg.into())
}

fn strict_err(ruby: &Ruby, msg: impl Into<String>) -> Error {
    Error::new(exc_class(ruby, "StrictError"), msg.into())
}

// Consumed by render_image (Task 3); kept here so the image path classifies raster
// errors as RenderError. Not used on the SVG path.
#[allow(dead_code)]
fn render_err(ruby: &Ruby, msg: impl Into<String>) -> Error {
    Error::new(exc_class(ruby, "RenderError"), msg.into())
}

// --- DSL detection + parse (mirrors fulgur-chart-cli `detect_dsl` / `parse_spec`) ---

/// Lightweight serde helper that only deserialises the top-level keys used for DSL detection.
#[derive(serde::Deserialize)]
struct DslDetector {
    mark: Option<serde::de::IgnoredAny>,
    #[serde(rename = "type")]
    r#type: Option<serde::de::IgnoredAny>,
}

/// Infer DSL from spec JSON: `mark` key → vegalite, `type` key → chartjs, neither → Err.
fn detect_dsl(json: &str) -> Result<&'static str, String> {
    let d: DslDetector =
        serde_json::from_str(json).map_err(|e| format!("error: invalid JSON: {e}"))?;
    if d.mark.is_some() {
        return Ok("vegalite");
    }
    if d.r#type.is_some() {
        return Ok("chartjs");
    }
    Err("error: cannot auto-detect DSL: specify dsl: 'chartjs' or 'vegalite'".to_string())
}

/// Parse a spec JSON string to IR using the specified DSL (chartjs or vegalite).
fn parse_spec(json: &str, dsl: &str, strict: bool) -> Result<fulgur_chart::ir::ChartSpec, String> {
    match dsl {
        "vegalite" => fulgur_chart::frontend::vegalite::parse(json, strict),
        _ => fulgur_chart::frontend::chartjs::parse(json, strict), // "chartjs"
    }
}

// --- RenderOptions ---

#[derive(Default)]
struct Opts {
    width: Option<f64>,
    height: Option<f64>,
    // Consumed by render_image (Task 3); the SVG path ignores scale.
    #[allow(dead_code)]
    scale: f32,
    strict: bool,
    dsl: Option<String>,
    font: Option<Vec<u8>>,
}

/// Parse optional RenderOptions from the kwargs hash. Tolerates extra keys (e.g. `format:`
/// passed by render_image in Task 3) via the trailing `RHash` splat, so unknown keys do not
/// raise here.
fn parse_opts(ruby: &Ruby, kw: RHash) -> Result<Opts, Error> {
    let args = get_kwargs::<
        _,
        (),
        (
            Option<f64>,
            Option<f64>,
            Option<f64>,
            Option<bool>,
            Option<String>,
            Option<RString>,
        ),
        RHash, // splat: collect + ignore unknown keys
    >(kw, &[], &["width", "height", "scale", "strict", "dsl", "font"])?;
    let (width, height, scale, strict, dsl, font) = args.optional;

    if let Some(d) = &dsl {
        if d != "chartjs" && d != "vegalite" {
            return Err(parse_err(ruby, format!("error: unsupported DSL '{d}'")));
        }
    }

    // Copy the font bytes out of the Ruby string immediately; the borrow is unsafe and must
    // not outlive any subsequent VM allocation.
    let font = font.map(|s| unsafe { s.as_slice().to_vec() });

    Ok(Opts {
        width,
        height,
        scale: scale.map(|s| s as f32).unwrap_or(1.0),
        strict: strict.unwrap_or(false),
        dsl,
        font,
    })
}

/// Build and validate the IR, mirroring the processing order of `render_one`.
/// Reusable by render_svg / render_image (Task 3) / schema-less paths.
fn build_ir(
    ruby: &Ruby,
    spec_json: &str,
    opts: &Opts,
) -> Result<fulgur_chart::ir::ChartSpec, Error> {
    // 1. Resolve DSL: explicit opts.dsl OR auto-detect.
    let dsl: &str = match &opts.dsl {
        Some(d) => d.as_str(),
        None => detect_dsl(spec_json).map_err(|e| parse_err(ruby, e))?,
    };

    // 2. Parse NON-strict → IR (render from this).
    let mut ir = parse_spec(spec_json, dsl, false)
        .map_err(|e| parse_err(ruby, format!("error: parse failed: {e}")))?;

    // 3. If strict, re-parse with strict=true (discard IR; unknown key → StrictError).
    if opts.strict {
        parse_spec(spec_json, dsl, true)
            .map_err(|e| strict_err(ruby, format!("error: strict violation: {e}")))?;
    }

    // 4. Apply width/height overrides BEFORE guard.
    if let Some(w) = opts.width {
        ir.width = w;
    }
    if let Some(h) = opts.height {
        ir.height = h;
    }

    // 5. Guard (failure → ParseError).
    validate_spec(&ir, &InputLimits::default()).map_err(|e| parse_err(ruby, format!("error: {e}")))?;

    Ok(ir)
}

// --- public API: render_svg ---

fn render_svg(ruby: &Ruby, args: &[Value]) -> Result<RString, Error> {
    let scanned = scan_args::<(String,), (), (), (), RHash, ()>(args)?;
    let (spec_json,) = scanned.required;
    let opts = parse_opts(ruby, scanned.keywords)?;
    let ir = build_ir(ruby, &spec_json, &opts)?;

    // 6. Render: font present → render_chart_with_font (Err → ParseError on the SVG path);
    //    else render_chart.
    let svg = match &opts.font {
        Some(bytes) => fulgur_chart::render::render_chart_with_font(&ir, bytes)
            .map_err(|e| parse_err(ruby, format!("error: render failed: {e}")))?,
        None => fulgur_chart::render::render_chart(&ir),
    };
    Ok(ruby.str_new(&svg))
}

fn version() -> String {
    fulgur_chart::version().to_string()
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    let module = ruby.define_module("Fulgur")?;

    // Canonical error hierarchy (single source of truth). lib/fulgur_chart.rb only aliases
    // FulgurChart = Fulgur and does NOT redefine these.
    let std_err = ruby.exception_standard_error();
    let parse = module.define_error("ParseError", std_err)?;
    module.define_error("StrictError", parse)?;
    module.define_error("RenderError", std_err)?;

    module.define_module_function("version", function!(version, 0))?;
    module.define_module_function("render_svg", function!(render_svg, -1))?;
    Ok(())
}
