use crate::{
    handlers::{chart, meta, shortlink, validate},
    store::ShortlinkStore,
};
use axum::{
    Router,
    routing::{get, post},
};

pub fn build_router(store: ShortlinkStore) -> Router {
    Router::new()
        .route("/health", get(meta::health))
        .route("/llms.txt", get(meta::llms_txt))
        .route("/chart", get(chart::get_chart).post(chart::post_chart))
        .route("/chart/validate", post(validate::post_validate))
        .route("/chart/create", post(shortlink::post_create))
        .route("/chart/s/{id}", get(shortlink::get_shortlink))
        .with_state(store)
}
