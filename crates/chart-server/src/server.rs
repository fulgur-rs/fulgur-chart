use crate::handlers::{chart, meta, validate};
use axum::{
    Router,
    routing::{get, post},
};

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(meta::health))
        .route("/llms.txt", get(meta::llms_txt))
        .route("/chart", get(chart::get_chart).post(chart::post_chart))
        .route("/chart/validate", post(validate::post_validate))
}
