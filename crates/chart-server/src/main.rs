mod config;
mod handlers;
mod render;
mod server;
use clap::Parser;

#[tokio::main]
async fn main() {
    let cfg = config::Config::parse();
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("chart-server listening on {addr}");
    axum::serve(listener, server::build_router()).await.unwrap();
}
