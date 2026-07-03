use std::net::SocketAddr;

use chart_server::{Config, FileShortlinkStore, build_router};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cfg = Config::parse();
    // shortlink dir を作成して durable backend を wire。作成不可なら fail-fast。
    let store = std::sync::Arc::new(
        FileShortlinkStore::new(&cfg.shortlink_dir, cfg.shortlink_entry_bytes)
            .await
            .unwrap_or_else(|e| {
                panic!("failed to open shortlink dir {:?}: {e}", cfg.shortlink_dir)
            }),
    );
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
