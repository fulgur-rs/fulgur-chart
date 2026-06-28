use axum::{Router, routing::get};

pub fn build_router() -> Router {
    Router::new().route("/health", get(|| async { "ok" }))
}
