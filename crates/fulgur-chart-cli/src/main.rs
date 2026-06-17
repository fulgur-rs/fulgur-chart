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
}

#[derive(Parser)]
struct RenderArgs {
    /// spec ファイルパス。`-` で標準入力から読み込む。
    spec: String,

    /// 出力先パス。`-` で標準出力へ書き出す。
    #[arg(short, long)]
    output: String,

    /// 出力フォーマット。省略時は output 拡張子で判定する(.png→png, それ以外/stdout→svg)。
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

    /// 入力 DSL。既定は chartjs。
    #[arg(long, default_value = "chartjs")]
    dsl: String,

    /// PNG 出力時の解像度倍率（1.0=等倍）。
    #[arg(long, default_value_t = 1.0)]
    scale: f32,
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
    }
}

fn run_render(args: RenderArgs) {
    // 1. DSL チェック。chartjs 以外は未対応(入力指定エラー扱い)。
    if args.dsl != "chartjs" {
        eprintln!("未対応の DSL です: {} (対応: chartjs)", args.dsl);
        std::process::exit(1);
    }

    // 2. spec 読み込み。`-` は stdin、それ以外はファイル。読めなければ exit 1。
    let json = match read_spec(&args.spec) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("入力読み込みエラー: {e}");
            std::process::exit(1);
        }
    };

    // 3. パース(非strict)。構造/JSON/type エラーは exit 1。
    let mut spec_ir = match fulgur_chart::frontend::chartjs::parse(&json, false) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("入力エラー: {e}");
            std::process::exit(1);
        }
    };

    // 4. strict 指定時は再パースして未知キーを検出。違反は exit 2。
    if args.strict {
        if let Err(e) = fulgur_chart::frontend::chartjs::parse(&json, true) {
            eprintln!("strict 違反: {e}");
            std::process::exit(2);
        }
    }

    // 5. width/height 上書き。
    if let Some(w) = args.width {
        spec_ir.width = w;
    }
    if let Some(h) = args.height {
        spec_ir.height = h;
    }

    // 6. format 決定: --format 明示 > output 拡張子(.png→png) > 既定 svg。
    let format = args.format.unwrap_or_else(|| detect_format(&args.output));

    // 7. 出力生成。SVG を生成し、PNG 指定時はラスタライズする。
    let svg = fulgur_chart::render::render_chart(&spec_ir);
    let bytes = match format {
        Format::Svg => svg.into_bytes(),
        Format::Png => match fulgur_chart::raster::svg_to_png(&svg, args.scale) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("PNG 変換エラー: {e}");
                std::process::exit(3);
            }
        },
    };

    // 8. 書き出し。`-` は stdout、それ以外はファイル。IO 失敗は exit 3。
    if let Err(e) = write_output(&args.output, &bytes) {
        eprintln!("出力エラー: {e}");
        std::process::exit(3);
    }

    // 9. 正常終了(暗黙の exit 0)。
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
