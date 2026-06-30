use clap::Parser;

use crate::render::Compression;

#[derive(Parser, Debug, Clone)]
#[command(name = "chart-server", about = "fulgur-chart HTTP rendering server")]
pub struct Config {
    #[arg(long, env = "FULGUR_HOST", default_value = "0.0.0.0")]
    pub host: String,

    #[arg(long, env = "FULGUR_PORT", default_value_t = 3000)]
    pub port: u16,

    #[arg(long, env = "FULGUR_MAX_CONCURRENT", default_value_t = num_cpus())]
    pub max_concurrent: usize,

    #[arg(long, env = "FULGUR_MAX_BODY_SIZE", default_value_t = 102_400)]
    pub max_body_size: usize,

    #[arg(long, env = "FULGUR_RENDER_TIMEOUT_MS", default_value_t = 1000)]
    pub render_timeout_ms: u64,

    #[arg(long, env = "FULGUR_SHORTLINK_LIMIT", default_value_t = 10_000)]
    pub shortlink_limit: usize,

    #[arg(long, env = "FULGUR_CORS_ORIGINS", default_value = "*")]
    pub cors_origins: String,

    /// レート制限（リクエスト/分/IP）。0 で無効。
    #[arg(long, env = "FULGUR_RATE_LIMIT", default_value_t = 0)]
    pub rate_limit: u64,

    #[arg(long, env = "FULGUR_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// PNG 圧縮プリセット（PNG 出力のみ。`fast`=最速/最大, `balanced`=既定, `high`=最小/最遅）。
    /// クライアント指定ではなくサーバ全体に適用する起動時設定。
    #[arg(
        long,
        env = "FULGUR_PNG_COMPRESSION",
        value_enum,
        default_value = "balanced"
    )]
    pub png_compression: Compression,

    /// WebP 出力を許可するか。既定は **無効**（opt-in）。
    ///
    /// WebP ロスレスは pixmap + 入力複製 + 出力 Vec で最大 3 フレーム分のメモリを要し、
    /// untrusted spec を受けるサーバでは OOM 経路になりうるため、明示的に有効化した
    /// 場合のみ受け付ける。無効時は format=webp を 415 で拒否する。
    #[arg(long, env = "FULGUR_WEBP_ENABLED", default_value_t = false)]
    pub webp_enabled: bool,

    /// WebP 出力の最大ピクセル面積（scale 適用後）。既定はライブラリの hard backstop
    /// と同値。ピークメモリ ≈ 面積 × 4B × 3。メモリの厳しい環境ではこれを下げて
    /// WebP のピークを絞れる（上げてもライブラリ上限で頭打ち）。
    #[arg(
        long,
        env = "FULGUR_MAX_WEBP_AREA",
        default_value_t = fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS
    )]
    pub max_webp_area: u64,
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
