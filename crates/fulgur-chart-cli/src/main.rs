use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use clap::{Parser, Subcommand, ValueEnum};
use jrsonnet_evaluator::{
    AsPathLike, FileImportResolver, IStr, ImportResolver, InitialContextBuilder, ObjValueBuilder,
    Source, SourcePath, State, Thunk, Val,
    error::Result as JrsonnetResult,
    manifest::{JsonFormat, ManifestFormat},
    trace::PathResolver,
};
use jrsonnet_stdlib::{Settings, StdTracePrinter, stdlib_uncached};

/// Jsonnet 入力の最大サイズ（1 MiB）。YAML bomb 等の事前評価 DoS を抑止する。
const MAX_JSONNET_SOURCE_BYTES: usize = 1024 * 1024;

/// std.parseYaml を除去したカスタム ContextInitializer。
/// parseYaml は YAML anchor bomb による DoS を起こし得るため公開しない。
#[derive(jrsonnet_gcmodule::Trace, Clone)]
struct FulgurContextInitializer {
    inner: jrsonnet_stdlib::ContextInitializer,
    patched_stdlib_obj: jrsonnet_evaluator::ObjValue,
}

impl FulgurContextInitializer {
    fn new(resolver: PathResolver) -> Self {
        let settings = Settings {
            ext_vars: HashMap::new(),
            ext_natives: HashMap::new(),
            trace_printer: Rc::new(StdTracePrinter::new(resolver.clone())),
            path_resolver: resolver.clone(),
        };
        let settings_cc = jrsonnet_gcmodule::Cc::new(RefCell::new(settings));
        let base_stdlib = stdlib_uncached(settings_cc);

        let mut b = ObjValueBuilder::new();
        b.with_super(base_stdlib);
        // IStr は interior mutability を持つが、インターン済みで実質不変。
        // jrsonnet の with_fields_omitted は FxHashSet<IStr> を要求するため抑制する。
        #[allow(clippy::mutable_key_type)]
        let mut omit = jrsonnet_evaluator::rustc_hash::FxHashSet::default();
        omit.insert(IStr::from("parseYaml"));
        b.with_fields_omitted(omit);
        let patched_stdlib_obj = b.build();

        Self {
            inner: jrsonnet_stdlib::ContextInitializer::new(resolver),
            patched_stdlib_obj,
        }
    }
}

impl jrsonnet_evaluator::ContextInitializer for FulgurContextInitializer {
    fn populate(&self, source: Source, builder: &mut InitialContextBuilder) {
        let mut b = ObjValueBuilder::new();
        b.with_super(self.patched_stdlib_obj.clone());
        b.field("thisFile").hide().value({
            let sp = source.source_path();
            sp.path().map_or_else(
                || sp.to_string(),
                |p| self.inner.settings().path_resolver.resolve(p),
            )
        });
        builder.bind("std", Thunk::evaluated(Val::Obj(b.build())));
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// .jsonnet ファイルのディレクトリを起点に import を許可するが、
/// そのディレクトリの外（`../` や絶対パス）へのアクセスはブロックするリゾルバ。
struct SandboxedImportResolver {
    inner: FileImportResolver,
    root: PathBuf,
}

impl SandboxedImportResolver {
    fn new(jsonnet_path: &Path) -> std::io::Result<Self> {
        let root = jsonnet_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()?;
        Ok(Self {
            inner: FileImportResolver::default(),
            root,
        })
    }
}

// SandboxedImportResolver は GC 管理ヒープを持たない（PathBuf のみ）。
impl jrsonnet_gcmodule::Trace for SandboxedImportResolver {
    fn trace(&self, _tracer: &mut jrsonnet_gcmodule::Tracer) {}
    fn is_type_tracked() -> bool {
        false
    }
}
// SAFETY: GC ヒープへの参照を持たないため循環しない。
unsafe impl jrsonnet_gcmodule::Acyclic for SandboxedImportResolver {}

impl ImportResolver for SandboxedImportResolver {
    fn resolve_from(&self, from: &SourcePath, path: &dyn AsPathLike) -> JrsonnetResult<SourcePath> {
        let resolved = self.inner.resolve_from(from, path)?;
        match resolved.path() {
            // FIFO・SourceVirtual などファイルシステムパスがない import は拒否する。
            None => {
                return Err(jrsonnet_evaluator::Error::new(
                    jrsonnet_evaluator::RuntimeError(
                        "import of non-filesystem source is not allowed in sandbox mode".into(),
                    ),
                ));
            }
            Some(p) => {
                // シンボリックリンクを完全展開してからサンドボックス境界をチェックする。
                let canonical = p.canonicalize().map_err(|e| {
                    jrsonnet_evaluator::Error::new(jrsonnet_evaluator::RuntimeError(
                        format!("failed to canonicalize import path '{}': {e}", p.display()).into(),
                    ))
                })?;
                if !canonical.starts_with(&self.root) {
                    return Err(jrsonnet_evaluator::Error::new(
                        jrsonnet_evaluator::RuntimeError(
                            format!(
                                "import '{}' escapes the sandbox root '{}'",
                                canonical.display(),
                                self.root.display()
                            )
                            .into(),
                        ),
                    ));
                }
            }
        }
        Ok(resolved)
    }

    fn load_file_contents(&self, resolved: &SourcePath) -> JrsonnetResult<Vec<u8>> {
        let bytes = self.inner.load_file_contents(resolved)?;
        if bytes.len() > MAX_JSONNET_SOURCE_BYTES {
            return Err(jrsonnet_evaluator::Error::new(
                jrsonnet_evaluator::RuntimeError(
                    format!(
                        "imported file exceeds the {MAX_JSONNET_SOURCE_BYTES}-byte limit ({} bytes)",
                        bytes.len()
                    )
                    .into(),
                ),
            ));
        }
        Ok(bytes)
    }
}

/// Render chart.js-compatible JSON specs to deterministic static SVG/PNG.
#[derive(Parser)]
#[command(
    name = "fulgur-chart",
    version,
    long_about = "Render chart.js-compatible JSON specs to deterministic static SVG/PNG.

Converts JSON chart specifications into SVG or PNG with pixel-identical output across
runs. Suitable for server-side chart generation, CI pipelines, and AI agent workflows
that need reproducible chart images.

DSL SUPPORT:
  chartjs   Chart.js v4 JSON (auto-detected when the spec has a top-level \"type\" key)
  vegalite  Vega-Lite JSON  (auto-detected when the spec has a top-level \"mark\" key)

Run 'fulgur-chart <COMMAND> --help' for subcommand examples and exit-code details."
)]
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
    /// Emit fulgur's resolved semantic model (colors, axis ticks, counts) as JSON.
    Inspect(InspectArgs),
}

#[derive(Parser)]
#[command(after_long_help = "EXAMPLES:
  # Print JSON Schema for chart.js specs (default)
  fulgur-chart schema                                # print chartjs JSON Schema
  fulgur-chart schema --dsl vegalite                 # print Vega-Lite JSON Schema
  fulgur-chart schema | python3 -m json.tool         # pretty-print and validate

EXIT CODES:
  0  Schema printed successfully
  1  Unsupported --dsl value")]
struct SchemaArgs {
    /// DSL whose schema to output (chartjs or vegalite).
    #[arg(long, default_value = "chartjs")]
    dsl: String,
}

#[derive(Parser)]
#[command(after_long_help = "EXAMPLES:
  # Inspect a chart.js spec and print the semantic model to stdout
  fulgur-chart inspect spec.json -o -

  # Read spec from stdin, write model JSON to a file
  cat spec.json | fulgur-chart inspect - -o model.json

  # Override canvas size before inspecting
  fulgur-chart inspect spec.json -o - --width 800 --height 400

  # Inspect a Vega-Lite spec (DSL auto-detected from top-level \"mark\" key)
  fulgur-chart inspect vega.json -o model.json

  # Inspect a Jsonnet spec (auto-detected from .jsonnet extension)
  fulgur-chart inspect spec.jsonnet -o -

  # Inspect Jsonnet from stdin
  printf 'local n=3; {type:\"bar\",data:{labels:[\"A\",\"B\",\"C\"],datasets:[{data:[n,n*2,n*3]}]}}' \\
    | fulgur-chart inspect - -o - --jsonnet

OUTPUT:
  Structured JSON with these top-level keys:
    meta    chart type and canvas dimensions
    series  per-dataset label, color, and data-point list
    axes    resolved tick values and labels for x/y axes

EXIT CODES:
  0  Model emitted successfully
  1  Input error: bad JSON, unsupported DSL, missing spec, or invalid dimensions
  3  I/O error: write failure")]
struct InspectArgs {
    /// Input spec file path. Use '-' to read from stdin.
    spec: String,
    /// Output path. Use '-' for stdout (default).
    #[arg(short, long, default_value = "-")]
    output: String,
    /// Input DSL (chartjs or vegalite). Auto-detected when omitted.
    #[arg(long)]
    dsl: Option<String>,
    /// Override chart width.
    #[arg(long)]
    width: Option<f64>,
    /// Override chart height.
    #[arg(long)]
    height: Option<f64>,
    /// Treat stdin ('-') input as Jsonnet instead of raw JSON.
    /// For file input, use a .jsonnet extension (auto-detected, no flag needed).
    /// Note: file imports (import/importstr) are disabled for stdin input.
    #[arg(long)]
    jsonnet: bool,
    /// Font file for text metrics. Defaults to the bundled font.
    #[arg(long)]
    font: Option<String>,
}

#[derive(Parser)]
#[command(after_long_help = "EXAMPLES:
  # Render a chart.js spec from stdin to SVG on stdout
  echo '{\"type\":\"bar\",\"data\":{\"labels\":[\"A\",\"B\"],\"datasets\":[{\"data\":[1,2]}]}}' \\
    | fulgur-chart render - -o -

  # Render a spec file to an SVG file
  fulgur-chart render spec.json -o chart.svg

  # Render to PNG (format inferred from extension)
  fulgur-chart render spec.json -o chart.png

  # Render to PNG at 2× resolution
  fulgur-chart render spec.json -o chart.png --scale 2.0

  # Batch: render multiple specs into a directory as SVG files
  fulgur-chart render a.json b.json c.json --out-dir ./out/

  # Batch: render multiple specs as PNG
  fulgur-chart render a.json b.json --out-dir ./out/ --format png

  # Strict mode: fail (exit 2) if spec contains unknown keys
  fulgur-chart render spec.json -o chart.svg --strict

  # Force a specific DSL instead of auto-detecting
  fulgur-chart render spec.json -o chart.svg --dsl vegalite

  # Override canvas dimensions from the spec
  fulgur-chart render spec.json -o chart.svg --width 1200 --height 600

  # Render a Jsonnet spec (auto-detected from .jsonnet extension, no flag needed)
  fulgur-chart render spec.jsonnet -o chart.svg

  # Write a Jsonnet spec inline and pipe it to render
  printf 'local xs=std.range(1,6); {type:\"bar\",data:{labels:std.map(std.toString,xs),datasets:[{label:\"x²\",data:std.map(function(x) x*x, xs)}]}}' \\
    | fulgur-chart render - -o - --jsonnet

DSL AUTO-DETECT:
  chartjs   detected when the spec has a top-level \"type\" key
  vegalite  detected when the spec has a top-level \"mark\" key
  Use --dsl to override when auto-detection fails (exit 1).

JSONNET INPUT:
  Jsonnet is a superset of JSON with comments, local variables, functions, and
  imports. Use it to write reusable or programmatically generated chart specs.

  File input — use a .jsonnet extension (evaluated automatically, no flag needed):
    fulgur-chart render spec.jsonnet -o chart.svg

  Stdin input — add --jsonnet to treat the pipe as Jsonnet:
    printf 'local color=\"#36a2eb\"; {type:\"bar\",data:{labels:[\"A\",\"B\",\"C\"],datasets:[{label:\"x\",data:[1,2,3],backgroundColor:color}]}}' \\
      | fulgur-chart render - -o - --jsonnet

  The full Jsonnet standard library (std.*) is available:
    std.range(from, to)          integer range, e.g. std.range(0, 11) → [0..11]
    std.map(fn, arr)             transform array, e.g. std.map(std.toString, xs)
    std.filter(fn, arr)          keep elements matching a predicate
    std.format(fmt, args)        string formatting, e.g. std.format(\"Q%d\", quarter) or \"Q%d\" % quarter
    std.join(sep, arr)           join strings with a separator
    std.sort(arr)                sort an array
    std.length(arr)              length of array or string
    std.objectFields(obj)        object keys as an array
    std.sin/cos/sqrt/floor/ceil  math functions
    std.toString(v)              convert a value to string

  Imports are resolved relative to the .jsonnet file's directory:
    import \"palette.libsonnet\"       # file in the same directory
    import \"shared/colors.libsonnet\" # relative subdirectory path
  Note: file imports (import/importstr/importbin) are disabled for stdin input (--jsonnet).
  To use imports, save the spec as a .jsonnet file and pass it as a file path instead.

EXIT CODES:
  0  Rendered successfully
  1  Input error: bad JSON/Jsonnet, unsupported DSL, missing/unreadable spec, render
     failure, or invalid dimensions (max 32768px per axis)
  2  Strict violation: spec contains unknown or unsupported keys (only with --strict)
  3  I/O error: output write failure or PNG conversion failure")]
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

    /// Treat stdin ('-') input as Jsonnet instead of raw JSON.
    /// For file input, use a .jsonnet extension (auto-detected, no flag needed).
    /// Note: file imports (import/importstr) are disabled for stdin input.
    #[arg(long)]
    jsonnet: bool,

    /// Input DSL (chartjs or vegalite). Auto-detected from the spec when omitted.
    #[arg(long)]
    dsl: Option<String>,

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
    Webp,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Render(args) => run_render(args),
        Command::Schema(args) => run_schema(args),
        Command::Inspect(args) => run_inspect(args),
    }
}

fn build_jrsonnet_file_state(jsonnet_path: &Path) -> Result<State, String> {
    let resolver = SandboxedImportResolver::new(jsonnet_path)
        .map_err(|e| format!("failed to resolve sandbox root: {e}"))?;
    let mut b = State::builder();
    b.import_resolver(resolver);
    b.context_initializer(FulgurContextInitializer::new(
        PathResolver::new_cwd_fallback(),
    ));
    Ok(b.build())
}

fn build_jrsonnet_snippet_state() -> State {
    let mut b = State::builder();
    b.context_initializer(FulgurContextInitializer::new(
        PathResolver::new_cwd_fallback(),
    ));
    b.build()
}

/// Jsonnet ソース文字列を JSON に評価（サンドボックスモード）。
///
/// ファイル import resolver を登録しないため、import/importstr/importbin は評価時エラーになる。
/// stdin 入力とバッチモード（自動処理）の両方で使用する。
fn evaluate_jsonnet_sandboxed(source_name: &str, src: &str) -> Result<String, String> {
    if src.len() > MAX_JSONNET_SOURCE_BYTES {
        return Err(format!(
            "Jsonnet source exceeds the {MAX_JSONNET_SOURCE_BYTES}-byte limit ({} bytes)",
            src.len()
        ));
    }
    let state = build_jrsonnet_snippet_state();
    let _guard = state.enter();
    let val = state
        .evaluate_snippet(
            jrsonnet_evaluator::IStr::from(source_name),
            jrsonnet_evaluator::IStr::from(src),
        )
        .map_err(|e| format!("{e}"))?;
    JsonFormat::default()
        .manifest(val)
        .map_err(|e| format!("{e}"))
}

/// Jsonnet ソース文字列を JSON に評価（stdin 用）。
fn evaluate_jsonnet_snippet(src: &str) -> Result<String, String> {
    evaluate_jsonnet_sandboxed("(stdin)", src)
}

/// .jsonnet ファイルを JSON に評価。import は同ディレクトリ内に制限（sandbox）。
fn evaluate_jsonnet_file(path: &Path) -> Result<String, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("{e}"))?;
    if src.len() > MAX_JSONNET_SOURCE_BYTES {
        return Err(format!(
            "Jsonnet source exceeds the {MAX_JSONNET_SOURCE_BYTES}-byte limit ({} bytes)",
            src.len()
        ));
    }
    let state = build_jrsonnet_file_state(path)?;
    let _guard = state.enter();
    let val = state.import(path).map_err(|e| format!("{e}"))?;
    JsonFormat::default()
        .manifest(val)
        .map_err(|e| format!("{e}"))
}

/// パスが .jsonnet 拡張子を持つか判定（大文字小文字を区別しない）。
fn is_jsonnet_path(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("jsonnet"))
}

/// Top-level render subcommand: validates explicit --dsl, loads font, dispatches to single or batch mode.
fn run_render(args: RenderArgs) {
    // Validate explicit DSL; only chartjs and vegalite are supported.
    if let Some(dsl) = &args.dsl
        && dsl != "chartjs"
        && dsl != "vegalite"
    {
        eprintln!("error: unsupported DSL '{dsl}' (supported: chartjs, vegalite)");
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

    // --jsonnet はファイル入力と組み合わせ不可（.jsonnet 拡張子を使うこと）
    if args.jsonnet && spec_path != "-" {
        eprintln!(
            "error: --jsonnet is only valid with stdin ('-'). For .jsonnet files, use the .jsonnet extension."
        );
        std::process::exit(1);
    }

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

    // Jsonnet の評価: --jsonnet フラグ（stdin 専用）または .jsonnet 拡張子
    let json = if args.jsonnet {
        match evaluate_jsonnet_snippet(&json) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("error: jsonnet evaluation failed: {e}");
                std::process::exit(1);
            }
        }
    } else if is_jsonnet_path(spec_path) {
        match evaluate_jsonnet_file(std::path::Path::new(spec_path)) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("error: jsonnet evaluation failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        json
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
    if args.jsonnet {
        eprintln!("error: --jsonnet is not valid in batch mode (use the .jsonnet extension)");
        std::process::exit(1);
    }

    // In batch mode there is no output path to inspect, so the format defaults to svg.
    let format = args.format.clone().unwrap_or(Format::Svg);
    let ext = match format {
        Format::Svg => "svg",
        Format::Png => "png",
        Format::Webp => "webp",
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

        // Evaluate or read the spec.
        let json = if is_jsonnet_path(spec_path) {
            match evaluate_jsonnet_file(std::path::Path::new(spec_path)) {
                Ok(j) => j,
                Err(e) => {
                    eprintln!("{spec_path}: error: jsonnet evaluation failed: {e}");
                    std::process::exit(1);
                }
            }
        } else {
            match std::fs::read_to_string(spec_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: failed to read '{spec_path}': {e}");
                    std::process::exit(1);
                }
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
    // Resolve DSL: use explicit --dsl, or auto-detect from the spec's top-level keys.
    let dsl: &str = match &args.dsl {
        Some(d) => d.as_str(),
        None => detect_dsl(json).map_err(|e| (1, e))?,
    };

    // Parse non-strictly; JSON/structure/type errors exit 1.
    let mut spec_ir =
        parse_spec(json, dsl, false).map_err(|e| (1, format!("error: parse failed: {e}")))?;

    // When --strict is set, re-parse with strict mode to catch unknown keys (exit 2).
    // Rendering still uses the non-strict IR parsed above.
    if args.strict {
        parse_spec(json, dsl, true).map_err(|e| (2, format!("error: strict violation: {e}")))?;
    }

    // Apply CLI width/height overrides.
    if let Some(w) = args.width {
        spec_ir.width = w;
    }
    if let Some(h) = args.height {
        spec_ir.height = h;
    }

    // Validate input bounds (DoS prevention). Called after overrides so --width/--height
    // are also checked. Exceeding a limit is a client error (exit 1).
    fulgur_chart::guard::validate_spec(&spec_ir, &fulgur_chart::guard::InputLimits::default())
        .map_err(|e| (1, format!("error: {e}")))?;

    match format {
        Format::Svg => {
            // SVG 経路は変更なし。
            let svg = match font_bytes {
                Some(bytes) => fulgur_chart::render::render_chart_with_font(&spec_ir, bytes)
                    .map_err(|e| (1, format!("error: render failed: {e}")))?,
                None => fulgur_chart::render::render_chart(&spec_ir),
            };
            Ok(svg.into_bytes())
        }
        Format::Png => {
            // PNG は SVG 文字列を経由しない直接描画（メモリ効率・速度向上）。
            // 圧縮はライブラリ既定（Balanced: 高速のままサイズを大幅削減、ロスレス）。
            let fb = font_bytes
                .as_deref()
                .unwrap_or(fulgur_chart::font::DEFAULT_FONT);
            fulgur_chart::raster_direct::render_chart_to_png(&spec_ir, args.scale, fb)
                .map_err(|e| (3, format!("error: PNG conversion failed: {e}")))
        }
        Format::Webp => {
            let fb = font_bytes
                .as_deref()
                .unwrap_or(fulgur_chart::font::DEFAULT_FONT);
            fulgur_chart::raster_direct::render_chart_to_webp(&spec_ir, args.scale, fb)
                .map_err(|e| (3, format!("error: WebP conversion failed: {e}")))
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
    if output == "-" {
        return Format::Svg;
    }
    let ext = std::path::Path::new(output).extension();
    if ext.is_some_and(|e| e.eq_ignore_ascii_case("png")) {
        Format::Png
    } else if ext.is_some_and(|e| e.eq_ignore_ascii_case("webp")) {
        Format::Webp
    } else {
        Format::Svg
    }
}

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

/// inspect サブコマンド: IR + layout から意味モデルを構築し pretty JSON で出力する。
fn run_inspect(args: InspectArgs) {
    // --jsonnet は stdin 専用。.jsonnet 拡張子は自動検出。stdin を消費する前に検証。
    if args.jsonnet && args.spec != "-" {
        eprintln!(
            "error: --jsonnet is only valid with stdin ('-'). For .jsonnet files, use the .jsonnet extension."
        );
        std::process::exit(1);
    }
    let json = match read_spec(&args.spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read input: {e}");
            std::process::exit(1);
        }
    };
    let json = if args.jsonnet {
        match evaluate_jsonnet_snippet(&json) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("error: jsonnet evaluation failed: {e}");
                std::process::exit(1);
            }
        }
    } else if is_jsonnet_path(&args.spec) {
        match evaluate_jsonnet_file(std::path::Path::new(&args.spec)) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("error: jsonnet evaluation failed: {e}");
                std::process::exit(1);
            }
        }
    } else {
        json
    };
    let dsl: String = match &args.dsl {
        Some(d) => {
            if d != "chartjs" && d != "vegalite" {
                eprintln!("error: unsupported DSL '{d}' (supported: chartjs, vegalite)");
                std::process::exit(1);
            }
            d.clone()
        }
        None => match detect_dsl(&json) {
            Ok(d) => d.to_string(),
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
    };
    let mut spec_ir = match parse_spec(&json, &dsl, false) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: parse failed: {e}");
            std::process::exit(1);
        }
    };
    if let Some(w) = args.width {
        spec_ir.width = w;
    }
    if let Some(h) = args.height {
        spec_ir.height = h;
    }
    if let Err(e) =
        fulgur_chart::guard::validate_spec(&spec_ir, &fulgur_chart::guard::InputLimits::default())
    {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
    // フォント読込(測定器用)。--font 指定がなければバンドルフォント。
    // バンドルフォントは静的バイナリ埋め込み(数 MB)なので Cow で borrow し、
    // デフォルト時のヒープコピーを避ける。
    let font_bytes: std::borrow::Cow<'static, [u8]> = match &args.font {
        Some(path) => match std::fs::read(path) {
            Ok(b) => std::borrow::Cow::Owned(b),
            Err(e) => {
                eprintln!("error: failed to read font '{path}': {e}");
                std::process::exit(1);
            }
        },
        None => std::borrow::Cow::Borrowed(fulgur_chart::font::DEFAULT_FONT),
    };
    let measurer = match fulgur_chart::text::TextMeasurer::new(&font_bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: font load failed: {e}");
            std::process::exit(1);
        }
    };
    let model = fulgur_chart::model::build_model(&spec_ir, &measurer);
    let out = serde_json::to_string_pretty(&model).expect("model serialization failed");
    if let Err(e) = write_output(&args.output, out.as_bytes()) {
        eprintln!("error: write failed: {e}");
        std::process::exit(3);
    }
}

#[cfg(test)]
mod detect_dsl_tests {
    use super::detect_dsl;

    #[test]
    fn type_key_detects_chartjs() {
        assert_eq!(
            detect_dsl(r#"{"type":"bar","data":{}}"#).unwrap(),
            "chartjs"
        );
    }

    #[test]
    fn mark_key_detects_vegalite() {
        assert_eq!(
            detect_dsl(r#"{"mark":"bar","data":{"values":[]}}"#).unwrap(),
            "vegalite"
        );
    }

    #[test]
    fn mark_takes_priority_over_type() {
        assert_eq!(
            detect_dsl(r#"{"mark":"bar","type":"x"}"#).unwrap(),
            "vegalite"
        );
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
