use std::sync::Arc;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, Method, header},
    routing::{get, post},
};
use tokio::sync::Semaphore;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use tower_http::{
    compression::{
        CompressionLayer,
        predicate::{NotForContentType, Predicate, SizeAbove},
    },
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
    // max_concurrent=0 は Semaphore::new(0) で恒久 503 になるため最低 1 に補正。
    let semaphore = Arc::new(Semaphore::new(cfg.max_concurrent.max(1)));
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
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
    };

    // 圧縮: PNG/WebP は既に圧縮済みのため除外。SVG は image/svg+xml だが圧縮効果が高い。
    let compression = CompressionLayer::new().compress_when(
        SizeAbove::new(32)
            .and(NotForContentType::const_new("image/png"))
            .and(NotForContentType::const_new("image/webp")),
    );

    let mut router = Router::new()
        .route("/health", get(meta::health))
        .route("/llms.txt", get(meta::llms_txt))
        .route("/chart", get(chart::get_chart).post(chart::post_chart))
        .route("/chart/validate", post(validate::post_validate))
        .route("/chart/create", post(shortlink::post_create))
        .route("/chart/s/{id}", get(shortlink::get_shortlink))
        .route("/mcp", post(mcp::mcp_handler))
        .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()))
        .with_state(state)
        .layer(compression)
        .layer(DefaultBodyLimit::max(cfg.max_body_size));

    // レート制限: FULGUR_RATE_LIMIT=0（デフォルト）で無効。
    // プロキシ背後のデプロイでは peer アドレスが proxy になるため、
    // 使用する場合は信頼できる転送ヘッダーの設定も検討すること。
    if cfg.rate_limit > 0 {
        let rate_limit = cfg.rate_limit as u32;
        let rate_per_ms = (60_000u64 / rate_limit as u64).max(1);
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(rate_per_ms)
                .burst_size(rate_limit)
                .finish()
                .expect("invalid governor config: check rate_limit setting"),
        );
        router = router.layer(GovernorLayer {
            config: governor_conf,
        });
    }

    // CORS は最外層に置く。こうしないと 429 や 413 に CORS ヘッダーが付かず、
    // ブラウザが CORS エラーを報告してしまう。
    router.layer(cors)
}
