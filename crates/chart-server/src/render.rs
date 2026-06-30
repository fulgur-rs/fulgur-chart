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

/// WebP 出力のサーバ側ポリシー（起動時設定）。
///
/// WebP はロスレスエンコードで pixmap + 入力複製 + 出力 Vec の最大 3 フレーム分の
/// メモリを要し、untrusted spec を受けるサーバでは OOM 経路になりうる。そのため
/// 既定では **無効**（opt-in）とし、有効化時も面積予算 `max_area` で運用者が
/// メモリ上限を絞れるようにする。
#[derive(Debug, Clone, Copy)]
pub struct WebpPolicy {
    /// WebP 出力を許可するか。`false` なら format=webp を拒否する。
    pub enabled: bool,
    /// scale 適用後の最大ピクセル面積。ピークメモリ ≈ 面積 × 4B × 3。
    /// ライブラリの `MAX_WEBP_AREA_PIXELS` が hard backstop で、ここはそれ以下に
    /// 絞るための運用ノブ。
    pub max_area: u64,
}

/// レンダリングパイプラインで発生するエラー。
#[derive(Debug)]
pub enum RenderError {
    /// JSON パースまたは DSL 変換の失敗。
    Parse(String),
    /// 入力上限検証の失敗（DoS 対策）。
    Validate(String),
    /// 描画処理の失敗（フォント読み込み失敗・ラスタライズ失敗など）。
    Render(String),
    /// 要求されたフォーマットがサーバ設定で無効（例: WebP が opt-in 未許可）。
    Unsupported(String),
}

impl RenderError {
    /// HTTP レスポンス等で使用するエラーコード文字列。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Parse(_) => "PARSE_ERROR",
            Self::Validate(_) => "VALIDATE_ERROR",
            Self::Render(_) => "RENDER_ERROR",
            Self::Unsupported(_) => "UNSUPPORTED_FORMAT",
        }
    }

    /// エラーの詳細メッセージ。
    pub fn message(&self) -> &str {
        match self {
            Self::Parse(m) | Self::Validate(m) | Self::Render(m) | Self::Unsupported(m) => m,
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
/// `webp` は WebP のみ有効。無効時は format=webp を `Unsupported` で拒否し、
/// 有効時も面積予算超過を `Validate` で pixmap 確保前に拒否する（OOM 対策）。
pub fn render(
    spec: &ChartSpec,
    format: OutputFormat,
    scale: f32,
    compression: Compression,
    webp: WebpPolicy,
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
        OutputFormat::Webp => {
            if !webp.enabled {
                return Err(RenderError::Unsupported(
                    "WebP output is disabled on this server (set FULGUR_WEBP_ENABLED=true to enable)"
                        .to_string(),
                ));
            }
            // pixmap(256MB 級)を確保する前に、サーバの面積予算で弾く。
            // ライブラリの MAX_WEBP_AREA_PIXELS も hard backstop として効く。
            let area = webp_output_area(spec, scale);
            if area > webp.max_area {
                return Err(RenderError::Validate(format!(
                    "WebP output area {area} px exceeds the server limit of {} px",
                    webp.max_area
                )));
            }
            raster_direct::render_chart_to_webp(spec, scale, DEFAULT_FONT)
                .map_err(RenderError::Render)
        }
    }
}

/// scale 適用後の WebP 出力ピクセル面積を求める（面積予算チェック用）。
///
/// ライブラリ `raster_direct` の寸法計算（scale ≤ 0/非有限は 1.0、最低 1 px、
/// u32 飽和）と一致させ、予算判定が実際の確保サイズとずれないようにする。
fn webp_output_area(spec: &ChartSpec, scale: f32) -> u64 {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let w = (spec.width as f32 * scale).round().max(1.0) as u32;
    let h = (spec.height as f32 * scale).round().max(1.0) as u32;
    w as u64 * h as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar_spec() -> ChartSpec {
        parse_and_validate(
            r#"{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}"#,
            "chartjs",
            false,
        )
        .unwrap()
    }

    fn webp_on(max_area: u64) -> WebpPolicy {
        WebpPolicy {
            enabled: true,
            max_area,
        }
    }

    /// WebP が無効なら format=webp は Unsupported（→ 415）で拒否される。
    #[test]
    fn webp_disabled_is_unsupported() {
        let policy = WebpPolicy {
            enabled: false,
            max_area: u64::MAX,
        };
        let err = render(
            &bar_spec(),
            OutputFormat::Webp,
            1.0,
            Compression::default(),
            policy,
        )
        .unwrap_err();
        assert!(matches!(err, RenderError::Unsupported(_)));
        assert_eq!(err.code(), "UNSUPPORTED_FORMAT");
    }

    /// WebP 有効・面積予算内なら通常どおり WebP バイト列を返す。
    #[test]
    fn webp_enabled_within_budget_renders() {
        let bytes = render(
            &bar_spec(),
            OutputFormat::Webp,
            1.0,
            Compression::default(),
            webp_on(fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS),
        )
        .unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    /// サーバ面積予算を超える要求は pixmap 確保前に Validate（→ 400）で拒否する。
    #[test]
    fn webp_over_server_budget_is_validate_error() {
        // 予算をわざと極小(100px)にすると通常チャートでも超過する。
        let err = render(
            &bar_spec(),
            OutputFormat::Webp,
            1.0,
            Compression::default(),
            webp_on(100),
        )
        .unwrap_err();
        assert!(matches!(err, RenderError::Validate(_)));
        assert!(err.message().contains("server limit"));
    }

    /// WebP ポリシーは PNG に影響しない（WebP 無効でも PNG は描画できる）。
    #[test]
    fn png_ignores_webp_policy() {
        let policy = WebpPolicy {
            enabled: false,
            max_area: 0,
        };
        let bytes = render(
            &bar_spec(),
            OutputFormat::Png,
            1.0,
            Compression::default(),
            policy,
        )
        .unwrap();
        assert_eq!(&bytes[0..4], &[0x89, b'P', b'N', b'G']);
    }
}
