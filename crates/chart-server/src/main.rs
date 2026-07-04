use std::net::SocketAddr;

use chart_server::{Config, FileShortlinkStore, build_router};
use clap::Parser;

/// 背景 janitor が期限切れ bucket を sweep する間隔（秒）。
const SWEEP_INTERVAL_SECS: u64 = 60;

#[tokio::main]
async fn main() {
    // 旧集約上限 env（8tr.6 で撤去）を検出して fail-fast。sdp で MAX_BYTES/MAX_ENTRIES を
    // 復活させたため、旧名 FULGUR_SHORTLINK_LIMIT のみ「改名」として弾く。
    if std::env::var_os("FULGUR_SHORTLINK_LIMIT").is_some() {
        panic!(
            "FULGUR_SHORTLINK_LIMIT is renamed: use FULGUR_SHORTLINK_MAX_ENTRIES \
             (and FULGUR_SHORTLINK_MAX_BYTES for the byte budget)."
        );
    }
    let cfg = Config::parse();
    if cfg.shortlink_max_bytes != 0 && cfg.shortlink_max_bytes < cfg.shortlink_entry_bytes as u64 {
        panic!(
            "FULGUR_SHORTLINK_MAX_BYTES ({}) must be >= FULGUR_SHORTLINK_ENTRY_BYTES ({}): \
             a byte budget smaller than a single entry permanently rejects large entries \
             (even on an empty store). Raise the budget or lower the per-entry limit.",
            cfg.shortlink_max_bytes, cfg.shortlink_entry_bytes
        );
    }
    // shortlink dir を作成して durable backend を wire。作成不可なら fail-fast。
    let store = std::sync::Arc::new(
        FileShortlinkStore::new(&cfg.shortlink_dir, cfg.shortlink_entry_bytes)
            .await
            .unwrap_or_else(|e| panic!("failed to open shortlink dir {:?}: {e}", cfg.shortlink_dir))
            .with_ttl_seconds(cfg.shortlink_ttl_seconds)
            .with_capacity(cfg.shortlink_max_bytes, cfg.shortlink_max_entries),
    );
    // 背景 janitor: 一定間隔で期限切れ bucket を sweep。concrete Arc を clone して回す
    // （sweep は FileShortlinkStore 固有メソッドで trait には無い）。detached task で、
    // プロセス終了時に drop される（main に graceful shutdown が無いため YAGNI）。
    {
        let store = store.clone();
        tokio::spawn(async move {
            let mut tick =
                tokio::time::interval(std::time::Duration::from_secs(SWEEP_INTERVAL_SECS));
            loop {
                tick.tick().await;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                store.sweep_expired(now).await;
            }
        });
    }
    // Railway は $PORT を inject する。FULGUR_PORT より優先して読む。
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(cfg.port);
    // タプル形式で bind すれば IPv6 アドレス（::1 等）でも正しく動作する。
    let listener = tokio::net::TcpListener::bind((cfg.host.as_str(), port))
        .await
        .unwrap();
    println!("chart-server listening on {}:{}", cfg.host, port);
    axum::serve(
        listener,
        build_router(&cfg, store).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
