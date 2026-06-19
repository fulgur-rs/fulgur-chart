use std::io::{Read, Write};

use clap::{Parser, Subcommand, ValueEnum};

/// Render chart.js-compatible JSON specs to deterministic static SVG/PNG.
#[derive(Parser)]
#[command(name = "fulgur-chart", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Render a spec (JSON) to SVG or PNG.
    Render(RenderArgs),
    /// Print the JSON Schema for a supported input DSL.
    Schema(SchemaArgs),
}

#[derive(Parser)]
struct SchemaArgs {
    /// DSL whose schema to output (chartjs or vegalite).
    #[arg(long, default_value = "chartjs")]
    dsl: String,
}

#[derive(Parser)]
struct RenderArgs {
    /// Input spec file path(s). Use '-' to read from stdin.
    /// Multiple files require --out-dir ('-' not allowed in batch mode).
    #[arg(num_args = 1..)]
    spec: Vec<String>,

    /// Output path. Use '-' for stdout. Required in single-spec mode.
    #[arg(short, long)]
    output: Option<String>,

    /// Output directory for batch mode. Each spec is written as <out-dir>/<stem>.<ext>.
    #[arg(long)]
    out_dir: Option<String>,

    /// Output format. Inferred from the output extension (.png → png, otherwise svg).
    /// Defaults to svg when --out-dir is used.
    #[arg(long, value_enum)]
    format: Option<Format>,

    /// Override the chart width from the spec.
    #[arg(long)]
    width: Option<f64>,

    /// Override the chart height from the spec.
    #[arg(long)]
    height: Option<f64>,

    /// Reject unknown or unsupported keys (strict mode).
    #[arg(long)]
    strict: bool,

    /// Input DSL. Supported values: chartjs, vegalite.
    #[arg(long, default_value = "chartjs")]
    dsl: String,

    /// Scale factor for PNG output (1.0 = 1x).
    #[arg(long, default_value_t = 1.0)]
    scale: f32,

    /// Font file for text metrics and rendering. Defaults to the bundled font.
    #[arg(long)]
    font: Option<String>,
}

#[derive(Clone, ValueEnum)]
enum Format {
    Svg,
    Png,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Render(args) => run_render(args),
        Command::Schema(args) => run_schema(args),
    }
}

fn run_render(args: RenderArgs) {
    // Validate DSL; only chartjs and vegalite are supported.
    if args.dsl != "chartjs" && args.dsl != "vegalite" {
        eprintln!(
            "error: unsupported DSL '{}' (supported: chartjs, vegalite)",
            args.dsl
        );
        std::process::exit(1);
    }

    // Load the font once if --font is given; reused across metric/SVG/PNG stages and all batch files.
    let font_bytes: Option<Vec<u8>> = match &args.font {
        Some(path) => match std::fs::read(path) {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!("error: failed to read font '{path}': {e}");
                std::process::exit(1);
            }
        },
        None => None,
    };

    // Dispatch to single or batch mode based on whether --out-dir was given.
    match &args.out_dir {
        None => run_single(&args, &font_bytes),
        Some(out_dir) => run_batch(&args, out_dir, &font_bytes),
    }
}

/// Single-spec mode: render one spec and write to output ('-' = stdout).
fn run_single(args: &RenderArgs, font_bytes: &Option<Vec<u8>>) {
    // Exactly one spec is required; multiple specs need --out-dir.
    if args.spec.is_empty() {
        eprintln!("error: no input spec provided");
        std::process::exit(1);
    }
    if args.spec.len() > 1 {
        eprintln!("error: multiple input specs require --out-dir");
        std::process::exit(1);
    }
    let spec_path = &args.spec[0];

    // --output is required in single-spec mode.
    let output = match &args.output {
        Some(o) => o,
        None => {
            eprintln!("error: --output (-o) is required for single-spec mode");
            std::process::exit(1);
        }
    };

    // Read spec from stdin ('-') or file; exit 1 on failure.
    let json = match read_spec(spec_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read input: {e}");
            std::process::exit(1);
        }
    };

    // Determine output format: explicit flag > infer from extension > default svg.
    let format = args.format.clone().unwrap_or_else(|| detect_format(output));

    let bytes = match render_one(&json, args, &format, font_bytes) {
        Ok(b) => b,
        Err((code, msg)) => {
            eprintln!("{msg}");
            std::process::exit(code);
        }
    };

    // Write to stdout ('-') or file; exit 3 on IO failure.
    if let Err(e) = write_output(output, &bytes) {
        eprintln!("error: write failed: {e}");
        std::process::exit(3);
    }
}

/// Batch mode: render multiple specs and write each to <out-dir>/<stem>.<ext>.
/// Processes inputs in order; aborts on first error (exit 1=input, 2=strict, 3=IO/png).
fn run_batch(args: &RenderArgs, out_dir: &str, font_bytes: &Option<Vec<u8>>) {
    // --out-dir and --output are mutually exclusive.
    if args.output.is_some() {
        eprintln!("error: --out-dir and --output cannot be used together");
        std::process::exit(1);
    }

    // In batch mode there is no output path to inspect, so the format defaults to svg.
    let format = args.format.clone().unwrap_or(Format::Svg);
    let ext = match format {
        Format::Svg => "svg",
        Format::Png => "png",
    };

    // Phase 1: validate and render all inputs before writing anything.
    // Abort on first error so no partial output is left on disk.
    // Also detect stem collisions upfront (e.g. "foo/a.json" and "bar/a.json" would both write "a.<ext>").
    let mut seen_stems: Vec<String> = Vec::new();
    let mut outputs: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
    for spec_path in &args.spec {
        // Stdin ('-') is not supported in batch mode.
        if spec_path == "-" {
            eprintln!("error: stdin ('-') is not supported in batch mode");
            std::process::exit(1);
        }
        // Output filename stem (basename without extension).
        let stem = match std::path::Path::new(spec_path).file_stem() {
            Some(s) => s.to_string_lossy().into_owned(),
            None => {
                eprintln!("error: cannot determine output stem for '{spec_path}'");
                std::process::exit(1);
            }
        };
        if seen_stems.contains(&stem) {
            eprintln!("error: output name conflict: multiple inputs would produce '{stem}.{ext}'");
            std::process::exit(1);
        }
        seen_stems.push(stem.clone());

        // Read spec from file; exit 1 on failure.
        let json = match std::fs::read_to_string(spec_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to read '{spec_path}': {e}");
                std::process::exit(1);
            }
        };

        // Render but don't write yet; abort with the relevant exit code on failure.
        let bytes = match render_one(&json, args, &format, font_bytes) {
            Ok(b) => b,
            Err((code, msg)) => {
                eprintln!("{spec_path}: {msg}");
                std::process::exit(code);
            }
        };
        let out_path = std::path::Path::new(out_dir).join(format!("{stem}.{ext}"));
        outputs.push((out_path, bytes));
    }

    // Phase 2: all inputs succeeded — now create the output directory and write files.
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("error: failed to create output directory '{out_dir}': {e}");
        std::process::exit(3);
    }
    // Pre-flight: abort before writing anything if any output path is blocked by a non-file
    // (directory, symlink to dir, etc.). Cannot guarantee atomicity against mid-write IO errors
    // such as disk full, but this catches the common case of accidental directory collisions.
    for (out_path, _) in &outputs {
        if out_path.exists() && !out_path.is_file() {
            eprintln!("error: output path is not a file: {}", out_path.display());
            std::process::exit(3);
        }
    }
    for (out_path, bytes) in &outputs {
        if let Err(e) = std::fs::write(out_path, bytes) {
            eprintln!("error: write failed '{}': {e}", out_path.display());
            std::process::exit(3);
        }
    }
}

/// Parse a spec JSON string to IR using the specified DSL (chartjs or vegalite).
fn parse_spec(json: &str, dsl: &str, strict: bool) -> Result<fulgur_chart::ir::ChartSpec, String> {
    match dsl {
        "vegalite" => fulgur_chart::frontend::vegalite::parse(json, strict),
        _ => fulgur_chart::frontend::chartjs::parse(json, strict), // "chartjs"
    }
}

/// Render one spec JSON string to output bytes. Shared by single and batch modes.
/// Returns Err((exit_code, message)) on failure: 1=input/render, 2=strict, 3=png/IO.
fn render_one(
    json: &str,
    args: &RenderArgs,
    format: &Format,
    font_bytes: &Option<Vec<u8>>,
) -> Result<Vec<u8>, (i32, String)> {
    // Parse non-strictly; JSON/structure/type errors exit 1.
    let mut spec_ir =
        parse_spec(json, &args.dsl, false).map_err(|e| (1, format!("error: parse failed: {e}")))?;

    // When --strict is set, re-parse with strict mode to catch unknown keys (exit 2).
    // Rendering still uses the non-strict IR parsed above.
    if args.strict {
        parse_spec(json, &args.dsl, true)
            .map_err(|e| (2, format!("error: strict violation: {e}")))?;
    }

    // Apply CLI width/height overrides.
    if let Some(w) = args.width {
        spec_ir.width = w;
    }
    if let Some(h) = args.height {
        spec_ir.height = h;
    }

    // Render SVG.
    let svg = match font_bytes {
        Some(bytes) => fulgur_chart::render::render_chart_with_font(&spec_ir, bytes)
            .map_err(|e| (1, format!("error: render failed: {e}")))?,
        None => fulgur_chart::render::render_chart(&spec_ir),
    };

    // Rasterize to PNG when requested.
    match format {
        Format::Svg => Ok(svg.into_bytes()),
        Format::Png => {
            let res = match font_bytes {
                Some(fb) => fulgur_chart::raster::svg_to_png_with_font(&svg, args.scale, fb),
                None => fulgur_chart::raster::svg_to_png(&svg, args.scale),
            };
            res.map_err(|e| (3, format!("error: PNG conversion failed: {e}")))
        }
    }
}

fn read_spec(path: &str) -> std::io::Result<String> {
    if path == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(path)
    }
}

fn write_output(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    if path == "-" {
        let mut out = std::io::stdout();
        out.write_all(bytes)?;
        out.flush()?;
        Ok(())
    } else {
        std::fs::write(path, bytes)
    }
}

fn detect_format(output: &str) -> Format {
    if output != "-"
        && std::path::Path::new(output)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        Format::Png
    } else {
        Format::Svg
    }
}

#[allow(dead_code)]
fn detect_dsl(json: &str) -> Result<&'static str, String> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| format!("error: invalid JSON: {e}"))?;
    if v.get("mark").is_some() {
        return Ok("vegalite");
    }
    if v.get("type").is_some() {
        return Ok("chartjs");
    }
    Err("error: cannot auto-detect DSL: specify --dsl chartjs or --dsl vegalite".to_string())
}

fn run_schema(args: SchemaArgs) {
    let json = match args.dsl.as_str() {
        "chartjs" => {
            let schema = schemars::schema_for!(fulgur_chart::schema::ChartJsSpec);
            serde_json::to_string_pretty(&schema).expect("schema serialization failed")
        }
        "vegalite" => {
            let schema = schemars::schema_for!(fulgur_chart::schema::VegaLiteSpec);
            serde_json::to_string_pretty(&schema).expect("schema serialization failed")
        }
        other => {
            eprintln!("error: unsupported DSL '{other}' (supported: chartjs, vegalite)");
            std::process::exit(1);
        }
    };
    println!("{json}");
}

#[cfg(test)]
mod detect_dsl_tests {
    use super::detect_dsl;

    #[test]
    fn type_key_detects_chartjs() {
        assert_eq!(detect_dsl(r#"{"type":"bar","data":{}}"#).unwrap(), "chartjs");
    }

    #[test]
    fn mark_key_detects_vegalite() {
        assert_eq!(detect_dsl(r#"{"mark":"bar","data":{"values":[]}}"#).unwrap(), "vegalite");
    }

    #[test]
    fn mark_takes_priority_over_type() {
        assert_eq!(detect_dsl(r#"{"mark":"bar","type":"x"}"#).unwrap(), "vegalite");
    }

    #[test]
    fn no_known_key_is_err() {
        assert!(detect_dsl(r#"{"labels":[]}"#).is_err());
    }

    #[test]
    fn invalid_json_is_err() {
        assert!(detect_dsl("not json").is_err());
    }
}
