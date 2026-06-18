use std::io::{Read, Write};

use clap::{Parser, Subcommand, ValueEnum};

/// fulgur-chart: chart.js 互換 JSON spec から決定的な静的 SVG を生成する CLI。
#[derive(Parser)]
#[command(name = "fulgur-chart", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// spec(JSON) を SVG にレンダリングする。
    Render(RenderArgs),
    /// 対応 DSL の JSON Schema を stdout に出力する。
    Schema(SchemaArgs),
}

#[derive(Parser)]
struct SchemaArgs {
    /// 出力する DSL のスキーマ(chartjs / vegalite)。
    #[arg(long, default_value = "chartjs")]
    dsl: String,
}

#[derive(Parser)]
struct RenderArgs {
    /// spec ファイルパス(1 つ以上)。`-` で標準入力から読み込む。
    /// 複数指定時は --out-dir が必須(`-` は不可)。
    #[arg(num_args = 1..)]
    spec: Vec<String>,

    /// 出力先パス。`-` で標準出力へ書き出す。単一 spec 時に必須。
    #[arg(short, long)]
    output: Option<String>,

    /// 複数 spec を一括出力するディレクトリ。各 spec は `<out-dir>/<stem>.<ext>` に書き出す。
    #[arg(long)]
    out_dir: Option<String>,

    /// 出力フォーマット。省略時は output 拡張子で判定する(.png→png, それ以外/stdout→svg)。
    /// --out-dir 指定時は省略すると svg。
    #[arg(long, value_enum)]
    format: Option<Format>,

    /// 指定すると spec の幅を上書きする。
    #[arg(long)]
    width: Option<f64>,

    /// 指定すると spec の高さを上書きする。
    #[arg(long)]
    height: Option<f64>,

    /// 未知/非対応キーをエラーにする(strict モード)。
    #[arg(long)]
    strict: bool,

    /// 入力 DSL。既定は chartjs(対応: chartjs, vegalite)。
    #[arg(long, default_value = "chartjs")]
    dsl: String,

    /// PNG 出力時の解像度倍率（1.0=等倍）。
    #[arg(long, default_value_t = 1.0)]
    scale: f32,

    /// 計測・SVG・PNG で使うフォントファイルパス。省略時は同梱フォント。
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
    // 1. DSL チェック。chartjs / vegalite 以外は未対応(入力指定エラー扱い)。
    if args.dsl != "chartjs" && args.dsl != "vegalite" {
        eprintln!("error: unsupported DSL '{}' (supported: chartjs, vegalite)", args.dsl);
        std::process::exit(1);
    }

    // 2. フォント読込: --font 指定時はファイルを1度だけ読み、計測/SVG/PNG の三者で
    //    同一バイト列を使う。未指定なら同梱フォント(従来動作で byte 一致)。
    //    バッチ時も全ファイルで同じバイト列を使い回す(再読込しない)。
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

    // 3. モード判定: --out-dir 指定の有無でバッチ/単一を分岐。
    match &args.out_dir {
        None => run_single(&args, &font_bytes),
        Some(out_dir) => run_batch(&args, out_dir, &font_bytes),
    }
}

/// 単一モード: spec を 1 つだけ受け取り、output(`-`=stdout)へ書き出す。
/// 出力バイト列は従来動作と完全一致する。
fn run_single(args: &RenderArgs, font_bytes: &Option<Vec<u8>>) {
    // spec はちょうど 1 つでなければならない。複数は --out-dir が必要。
    if args.spec.is_empty() {
        eprintln!("error: no input spec provided");
        std::process::exit(1);
    }
    if args.spec.len() > 1 {
        eprintln!("error: multiple input specs require --out-dir");
        std::process::exit(1);
    }
    let spec_path = &args.spec[0];

    // output(`-o`)は必須。
    let output = match &args.output {
        Some(o) => o,
        None => {
            eprintln!("error: --output (-o) is required for single-spec mode");
            std::process::exit(1);
        }
    };

    // spec 読み込み。`-` は stdin、それ以外はファイル。読めなければ exit 1。
    let json = match read_spec(spec_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: failed to read input: {e}");
            std::process::exit(1);
        }
    };

    // format 決定: --format 明示 > output 拡張子(.png→png) > 既定 svg。
    let format = args.format.clone().unwrap_or_else(|| detect_format(output));

    // 出力生成。
    let bytes = match render_one(&json, args, &format, font_bytes) {
        Ok(b) => b,
        Err((code, msg)) => {
            eprintln!("{msg}");
            std::process::exit(code);
        }
    };

    // 書き出し。`-` は stdout、それ以外はファイル。IO 失敗は exit 3。
    if let Err(e) = write_output(output, &bytes) {
        eprintln!("error: write failed: {e}");
        std::process::exit(3);
    }
}

/// バッチモード: 複数 spec を out_dir へ `<stem>.<ext>` で一括出力する。
/// 入力順に処理し、最初のエラーで該当コード(input=1/strict=2/IO・png=3)で打ち切る。
fn run_batch(args: &RenderArgs, out_dir: &str, font_bytes: &Option<Vec<u8>>) {
    // --out-dir と --output(-o)は併用不可。
    if args.output.is_some() {
        eprintln!("error: --out-dir and --output cannot be used together");
        std::process::exit(1);
    }

    // バッチでは output 拡張子が無いので、--format(既定 svg)から拡張子を決める。
    let format = args.format.clone().unwrap_or(Format::Svg);
    let ext = match format {
        Format::Svg => "svg",
        Format::Png => "png",
    };

    // フェーズ1: 全入力を検証・レンダリングしてから(まだ書き出さず)結果を貯める。
    // 1 件でも失敗すれば、ディレクトリ作成も書き出しもせず打ち切る(部分出力を残さない)。
    // 併せて出力名(stem)の衝突も事前検出する(`foo/a.json` と `bar/a.json` は同じ出力)。
    let mut seen_stems: Vec<String> = Vec::new();
    let mut outputs: Vec<(std::path::PathBuf, Vec<u8>)> = Vec::new();
    for spec_path in &args.spec {
        // バッチでは stdin(`-`)は不可。
        if spec_path == "-" {
            eprintln!("error: stdin ('-') is not supported in batch mode");
            std::process::exit(1);
        }
        // 出力ファイル名のステム。
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

        // spec 読み込み(ファイルのみ)。読めなければ exit 1。
        let json = match std::fs::read_to_string(spec_path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: failed to read '{spec_path}': {e}");
                std::process::exit(1);
            }
        };

        // 出力生成(ここでは書き出さない)。失敗は該当コードで打ち切り。
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

    // フェーズ2: 全件成功したので、ここで初めてディレクトリ作成と書き出しを行う。
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("error: failed to create output directory '{out_dir}': {e}");
        std::process::exit(3);
    }
    // 書き込み前 preflight: 既存の非ファイル(ディレクトリ等)が出力先を塞いでいたら、
    // 1 件も書く前に中止する(途中 write 失敗で部分成果物を残すのを避ける。
    // ディスク満杯など書き込み途中の IO 失敗までは本質的に保証できない)。
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

/// 指定された DSL に応じて spec(JSON 文字列)を IR(ChartSpec) にパースする。
/// chartjs(既定)はフロントエンド chartjs、vegalite は frontend vegalite を使う。
fn parse_spec(json: &str, dsl: &str, strict: bool) -> Result<fulgur_chart::ir::ChartSpec, String> {
    match dsl {
        "vegalite" => fulgur_chart::frontend::vegalite::parse(json, strict),
        _ => fulgur_chart::frontend::chartjs::parse(json, strict), // "chartjs"
    }
}

/// 1 つの spec(JSON 文字列)と引数から出力バイト列を生成する。
/// 単一/バッチ両モードで共有し、同一入力なら同一バイト列を返す(決定的)。
/// 失敗時は `(exit_code, message)` を返す。input/render=1, strict=2, png=3。
fn render_one(
    json: &str,
    args: &RenderArgs,
    format: &Format,
    font_bytes: &Option<Vec<u8>>,
) -> Result<Vec<u8>, (i32, String)> {
    // パース(非strict)。構造/JSON/type エラーは exit 1。
    let mut spec_ir =
        parse_spec(json, &args.dsl, false).map_err(|e| (1, format!("入力エラー: {e}")))?;

    // strict 指定時は再パースして未知キーを検出。違反は exit 2。
    // (検証用のゲートであり、レンダリングは非strict の ir から行う。)
    if args.strict {
        parse_spec(json, &args.dsl, true).map_err(|e| (2, format!("strict 違反: {e}")))?;
    }

    // width/height 上書き。
    if let Some(w) = args.width {
        spec_ir.width = w;
    }
    if let Some(h) = args.height {
        spec_ir.height = h;
    }

    // SVG 生成。
    let svg = match font_bytes {
        Some(bytes) => fulgur_chart::render::render_chart_with_font(&spec_ir, bytes)
            .map_err(|e| (1, format!("レンダリングエラー: {e}")))?,
        None => fulgur_chart::render::render_chart(&spec_ir),
    };

    // PNG 指定時はラスタライズ。
    match format {
        Format::Svg => Ok(svg.into_bytes()),
        Format::Png => {
            let res = match font_bytes {
                Some(fb) => fulgur_chart::raster::svg_to_png_with_font(&svg, args.scale, fb),
                None => fulgur_chart::raster::svg_to_png(&svg, args.scale),
            };
            res.map_err(|e| (3, format!("PNG 変換エラー: {e}")))
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
