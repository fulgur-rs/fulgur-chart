use fulgur_chart::guard::{validate_spec, InputLimits};
use magnus::{
    function,
    prelude::*,
    scan_args::{get_kwargs, scan_args},
    Error, ExceptionClass, RHash, RString, Ruby, Value,
};

// --- error helpers (classification is by CALL SITE, never by parsing the message) ---

fn exc_class(ruby: &Ruby, name: &str) -> ExceptionClass {
    let module = ruby
        .define_module("FulgurChart")
        .expect("FulgurChart module defined in init");
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

// The image path classifies raster errors as RenderError (asymmetry vs the SVG path,
// which maps font/render failures to ParseError).
fn render_err(ruby: &Ruby, msg: impl Into<String>) -> Error {
    Error::new(exc_class(ruby, "RenderError"), msg.into())
}

/// Coerce a Ruby argument to a String, accepting both String and Symbol (idiomatic Ruby lets
/// callers pass `dsl: :chartjs` / `format: :png`). magnus's String conversion rejects Symbols,
/// so without this `to_s` coercion a symbol would raise TypeError instead of being accepted.
fn coerce_string(v: Value) -> Result<String, Error> {
    v.funcall("to_s", ())
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
    let d: DslDetector = serde_json::from_str(json).map_err(|e| format!("invalid JSON: {e}"))?;
    if d.mark.is_some() {
        return Ok("vegalite");
    }
    if d.r#type.is_some() {
        return Ok("chartjs");
    }
    Err("cannot auto-detect DSL: specify dsl: 'chartjs' or 'vegalite'".to_string())
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
    // Consumed by the image path (render_chart_to_png); the SVG path ignores scale.
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
            Option<Value>,
            Option<RString>,
        ),
        RHash, // splat: collect + ignore unknown keys
    >(
        kw,
        &[],
        &["width", "height", "scale", "strict", "dsl", "font"],
    )?;
    let (width, height, scale, strict, dsl_val, font) = args.optional;

    // Accept String or Symbol for `dsl`; an explicit nil arrives as None (→ auto-detect).
    let dsl = match dsl_val {
        Some(v) => {
            let d = coerce_string(v)?;
            if d != "chartjs" && d != "vegalite" {
                return Err(parse_err(ruby, format!("unsupported DSL '{d}'")));
            }
            Some(d)
        }
        None => None,
    };

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
    // Contract §3: propagate the core's error String verbatim. The exception class — not a
    // message prefix — conveys parse/strict/render, so no CLI-style "error: ..." decoration.
    let mut ir = parse_spec(spec_json, dsl, false).map_err(|e| parse_err(ruby, e))?;

    // 3. If strict, re-parse with strict=true (discard IR; unknown key → StrictError).
    if opts.strict {
        parse_spec(spec_json, dsl, true).map_err(|e| strict_err(ruby, e))?;
    }

    // 4. Apply width/height overrides BEFORE guard.
    if let Some(w) = opts.width {
        ir.width = w;
    }
    if let Some(h) = opts.height {
        ir.height = h;
    }

    // 5. Guard (failure → ParseError).
    validate_spec(&ir, &InputLimits::default()).map_err(|e| parse_err(ruby, e))?;

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
            .map_err(|e| parse_err(ruby, e))?,
        None => fulgur_chart::render::render_chart(&ir),
    };
    Ok(ruby.str_new(&svg))
}

// --- public API: render_image / render_png ---

/// spec_json + Opts → binary PNG String (shared by render_image / render_png).
fn render_png_string(ruby: &Ruby, spec_json: &str, opts: &Opts) -> Result<RString, Error> {
    let ir = build_ir(ruby, spec_json, opts)?;
    let fb: &[u8] = opts
        .font
        .as_deref()
        .unwrap_or(fulgur_chart::font::DEFAULT_FONT);
    // Invalid font on the image path → RenderError (the SVG path maps this to ParseError).
    let png = fulgur_chart::raster_direct::render_chart_to_png(&ir, opts.scale, fb)
        .map_err(|e| render_err(ruby, e))?;
    Ok(ruby.str_from_slice(&png)) // ASCII-8BIT (BINARY) String
}

fn render_image(ruby: &Ruby, args: &[Value]) -> Result<RString, Error> {
    let scanned = scan_args::<(String,), (), (), (), RHash, ()>(args)?;
    let (spec_json,) = scanned.required;
    // Required `format` kwarg (String or Symbol). The trailing `RHash` splat tolerates leftover
    // RenderOptions keys (width/height/scale/strict/dsl/font); the splat-less form would raise.
    let kw = get_kwargs::<_, (Value,), (), RHash>(scanned.keywords, &["format"], &[])?;
    let (format_val,) = kw.required;
    let format = coerce_string(format_val)?;
    if format != "png" {
        return Err(parse_err(
            ruby,
            format!("unsupported format '{format}' (supported: png)"),
        ));
    }
    // Re-scan the same kwargs for RenderOptions; `get_kwargs` reads (does not mutate) the hash,
    // so the earlier `format` extraction does not interfere, and `format` is ignored here.
    let opts = parse_opts(ruby, scanned.keywords)?;
    render_png_string(ruby, &spec_json, &opts)
}

fn render_png(ruby: &Ruby, args: &[Value]) -> Result<RString, Error> {
    let scanned = scan_args::<(String,), (), (), (), RHash, ()>(args)?;
    let (spec_json,) = scanned.required;
    let opts = parse_opts(ruby, scanned.keywords)?;
    render_png_string(ruby, &spec_json, &opts)
}

// --- public API: schema ---

/// Return the JSON Schema (compact JSON String) for the given DSL (String or Symbol).
/// Mirrors the CLI's `run_schema`; unknown DSL → ParseError (consistent with `parse_opts`).
fn schema(ruby: &Ruby, dsl: Value) -> Result<String, Error> {
    let dsl = coerce_string(dsl)?;
    let s = match dsl.as_str() {
        "chartjs" => schemars::schema_for!(fulgur_chart::schema::ChartJsSpec),
        "vegalite" => schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec),
        other => {
            return Err(parse_err(
                ruby,
                format!("unsupported DSL '{other}' (supported: chartjs, vegalite)"),
            ))
        }
    };
    serde_json::to_string(&s).map_err(|e| render_err(ruby, format!("schema serialization: {e}")))
}

fn version() -> String {
    fulgur_chart::version().to_string()
}

#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // Module name is `FulgurChart` (NOT `Fulgur`): a top-level `Fulgur` would collide with the
    // Fulgur PDF library if both gems are loaded in the same process.
    let module = ruby.define_module("FulgurChart")?;

    // Canonical error hierarchy (single source of truth). lib/fulgur_chart.rb does not redefine
    // these or alias anything.
    let std_err = ruby.exception_standard_error();
    let parse = module.define_error("ParseError", std_err)?;
    module.define_error("StrictError", parse)?;
    module.define_error("RenderError", std_err)?;

    module.define_module_function("version", function!(version, 0))?;
    module.define_module_function("render_svg", function!(render_svg, -1))?;
    module.define_module_function("render_image", function!(render_image, -1))?;
    module.define_module_function("render_png", function!(render_png, -1))?;
    module.define_module_function("schema", function!(schema, 1))?;
    Ok(())
}
