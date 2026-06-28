use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::HeaderValue,
    routing::{get, post},
};
use tokio::sync::Semaphore;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::{
    compression::{CompressionLayer, predicate::DefaultPredicate},
    cors::CorsLayer,
};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::{
    config::Config,
    handlers::{chart, mcp, meta, openapi::ApiDoc, shortlink, validate},
    state::AppState,
    store::ShortlinkStore,
};

pub fn build_router(cfg: &Config, store: ShortlinkStore) -> Router {
    let semaphore = Arc::new(Semaphore::new(cfg.max_concurrent));
    let state = AppState {
        store,
        semaphore,
        render_timeout_ms: cfg.render_timeout_ms,
    };

    // CORS
    let cors = if cfg.cors_origins == "*" {
        CorsLayer::very_permissive()
    } else {
        let origins: Vec<HeaderValue> = cfg
            .cors_origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        CorsLayer::new().allow_origin(origins)
    };

    // レート制限: N req/分/IP
    // burst = N、1要素あたり 60000/N ms で補充
    let rate_limit = cfg.rate_limit.max(1) as u32;
    let rate_per_ms = (60_000u64 / rate_limit as u64).max(1);
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_millisecond(rate_per_ms)
            .burst_size(rate_limit)
            .finish()
            .expect("invalid governor config: check rate_limit setting"),
    );

    // 圧縮: image/ (PNG, WebP 含む) は DefaultPredicate が除外する
    let compression = CompressionLayer::new().compress_when(DefaultPredicate::new());

    Router::new()
        .route("/health", get(meta::health))
        .route("/llms.txt", get(meta::llms_txt))
        .route("/chart", get(chart::get_chart).post(chart::post_chart))
        .route("/chart/validate", post(validate::post_validate))
        .route("/chart/create", post(shortlink::post_create))
        .route("/chart/s/{id}", get(shortlink::get_shortlink))
        .route("/mcp", post(mcp::mcp_handler))
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .with_state(state)
        .layer(cors)
        .layer(compression)
        .layer(DefaultBodyLimit::max(cfg.max_body_size))
        .layer(GovernorLayer {
            config: governor_conf,
        })
}
