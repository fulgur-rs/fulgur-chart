//! fulgur-chart API の薄いラッパ。
//!
//! parse → validate → render のパイプラインを提供し、
//! HTTP ハンドラから呼ばれることを想定している。

use fulgur_chart::{
    font::DEFAULT_FONT,
    frontend,
    guard::{self, InputLimits},
    ir::ChartSpec,
    raster_direct::{self, PngCompression},
    render,
};

// ---------------------------------------------------------------------------
// 出力フォーマット
// ---------------------------------------------------------------------------

/// チャートの出力フォーマット。
///
/// `"data-uri"` のみ serde で rename する。
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Svg,
    #[default]
    Png,
    Webp,
    #[serde(rename = "data-uri")]
    DataUri,
}

impl OutputFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::DataUri => "data-uri",
        }
    }
}

/// PNG 圧縮プリセット。サイズと速度のトレードオフ（PNG のみ有効）。
///
/// `fast` は最速・最大サイズ、`high` は最小サイズ・最も遅い。
/// 既定は `balanced`（高速のままサイズを大幅削減）。
///
/// クライアントが per-request で選ぶ値ではなく、**サーバ起動時オプション**
/// (`--png-compression` / `FULGUR_PNG_COMPRESSION`) で運用者が一度決める設定。
#[derive(Debug, Clone, Copy, Default, PartialEq, clap::ValueEnum)]
pub enum Compression {
    Fast,
    #[default]
    Balanced,
    High,
}

impl Compression {
    fn to_png(self) -> PngCompression {
        match self {
            Self::Fast => PngCompression::Fast,
            Self::Balanced => PngCompression::Balanced,
            Self::High => PngCompression::High,
        }
    }
}

// ---------------------------------------------------------------------------
// エラー型
// ---------------------------------------------------------------------------

/// レンダリングパイプラインで発生するエラー。
#[derive(Debug)]
pub enum RenderError {
    /// JSON パースまたは DSL 変換の失敗。
    Parse(String),
    /// 入力上限検証の失敗（DoS 対策）。
    Validate(String),
    /// 描画処理の失敗（フォント読み込み失敗・ラスタライズ失敗など）。
    Render(String),
}

impl RenderError {
    /// HTTP レスポンス等で使用するエラーコード文字列。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Parse(_) => "PARSE_ERROR",
            Self::Validate(_) => "VALIDATE_ERROR",
            Self::Render(_) => "RENDER_ERROR",
        }
    }

    /// エラーの詳細メッセージ。
    pub fn message(&self) -> &str {
        match self {
            Self::Parse(m) | Self::Validate(m) | Self::Render(m) => m,
        }
    }
}

// ---------------------------------------------------------------------------
// パブリック API
// ---------------------------------------------------------------------------

/// JSON 文字列を DSL に従ってパースし、入力上限を検証して `ChartSpec` を返す。
///
/// # 引数
/// - `json`: chart.js v4 互換 JSON またはVega-Lite JSON 文字列
/// - `dsl`: `"vegalite"` で Vega-Lite、それ以外は chart.js として解釈
/// - `strict`: strict モードで parse するか
pub fn parse_and_validate(json: &str, dsl: &str, strict: bool) -> Result<ChartSpec, RenderError> {
    let spec = match dsl {
        "vegalite" => frontend::vegalite::parse(json, strict),
        _ => frontend::chartjs::parse(json, strict),
    }
    .map_err(RenderError::Parse)?;

    guard::validate_spec(&spec, &InputLimits::default()).map_err(RenderError::Validate)?;

    Ok(spec)
}

/// `ChartSpec` を指定フォーマットにレンダリングしてバイト列を返す。
///
/// `DataUri` の場合は SVG bytes を返す（data URI への変換は呼び出し元が行う）。
/// `scale` は PNG/WebP のみ有効。SVG では無視される。
/// `compression` は PNG のみ有効（WebP は lossless、SVG はテキストのため無視）。
pub fn render(
    spec: &ChartSpec,
    format: OutputFormat,
    scale: f32,
    compression: Compression,
) -> Result<Vec<u8>, RenderError> {
    match format {
        OutputFormat::Svg | OutputFormat::DataUri => {
            // render_chart は Result を返さない（パニックしない）。
            let svg = render::render_chart(spec);
            Ok(svg.into_bytes())
        }
        OutputFormat::Png => {
            raster_direct::render_chart_to_png_with(spec, scale, DEFAULT_FONT, compression.to_png())
                .map_err(RenderError::Render)
        }
        OutputFormat::Webp => raster_direct::render_chart_to_webp(spec, scale, DEFAULT_FONT)
            .map_err(RenderError::Render),
    }
}
