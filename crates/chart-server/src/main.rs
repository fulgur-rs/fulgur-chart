mod config;
mod handlers;
mod render;
mod response;
mod server;
mod state;
mod store;

use std::net::SocketAddr;

use clap::Parser;

#[tokio::main]
async fn main() {
    let cfg = config::Config::parse();
    let store = store::ShortlinkStore::new(cfg.shortlink_limit);
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
        server::build_router(&cfg, store).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
