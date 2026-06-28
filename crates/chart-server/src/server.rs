use crate::handlers::meta;
use axum::{Router, routing::get};

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(meta::health))
        .route("/llms.txt", get(meta::llms_txt))
}
