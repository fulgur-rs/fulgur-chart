use clap::Parser;

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

    #[arg(long, env = "FULGUR_RATE_LIMIT", default_value_t = 60)]
    pub rate_limit: u64,

    #[arg(long, env = "FULGUR_LOG_LEVEL", default_value = "info")]
    pub log_level: String,
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
