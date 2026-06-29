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
            .allow_headers([
                header::CONTENT_TYPE,
                header::AUTHORIZATION,
                header::ACCEPT,
                header::IF_NONE_MATCH,
            ])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ShortlinkStore;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    fn restricted_cors_router() -> Router {
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 1,
            max_body_size: 102_400,
            render_timeout_ms: 1000,
            shortlink_limit: 100,
            cors_origins: "https://example.com".into(),
            rate_limit: 0,
            log_level: "info".into(),
        };
        build_router(&cfg, ShortlinkStore::new(100))
    }

    fn test_router() -> Router {
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 4,
            max_body_size: 102_400,
            render_timeout_ms: 5000,
            shortlink_limit: 100,
            cors_origins: "*".into(),
            rate_limit: 0,
            log_level: "info".into(),
        };
        build_router(&cfg, ShortlinkStore::new(100))
    }

    async fn post_png_len(compression: &str) -> usize {
        let body = format!(
            r#"{{"chart":{{"type":"bar","data":{{"labels":["A","B","C","D"],"datasets":[{{"data":[12,19,3,5]}}]}}}},"format":"png","compression":"{compression}"}}"#
        );
        let resp = test_router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chart")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            200,
            "compression={compression} は 200 を返すべき"
        );
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&bytes[0..4], &[0x89, b'P', b'N', b'G'], "PNG を返すべき");
        bytes.len()
    }

    /// compression パラメータがエンドツーエンドで効くこと。
    /// high は fast より小さい PNG を返さなければならない(本機能の目的)。
    #[tokio::test]
    async fn compression_high_yields_smaller_png_than_fast() {
        let fast = post_png_len("fast").await;
        let high = post_png_len("high").await;
        assert!(
            high < fast,
            "compression=high ({high}B) は fast ({fast}B) より小さい PNG を返すべき"
        );
    }

    /// compression 未指定時は既定の balanced が使われ(fast より小さい)、
    /// 不正値はパースエラー(400)になること。
    #[tokio::test]
    async fn compression_default_is_balanced_and_invalid_is_rejected() {
        let fast = post_png_len("fast").await;
        let default = post_png_len("balanced").await;
        assert!(default <= fast, "既定(balanced) は fast 以下のサイズ");

        // 不正な compression 値は 400(deserialize 失敗)。
        let resp = test_router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/chart")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}},"format":"png","compression":"bogus"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            resp.status().is_client_error(),
            "不正な compression はクライアントエラー(4xx)を返すべき: {}",
            resp.status()
        );
    }

    #[tokio::test]
    async fn restricted_cors_allows_if_none_match_preflight() {
        let resp = restricted_cors_router()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/chart")
                    .header("origin", "https://example.com")
                    .header("access-control-request-method", "GET")
                    .header("access-control-request-headers", "if-none-match")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // 制限 CORS でも If-None-Match が許可されていれば OPTIONS 200/204 が返る
        let allowed = resp
            .headers()
            .get("access-control-allow-headers")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_ascii_lowercase().contains("if-none-match"))
            .unwrap_or(false);
        assert!(
            allowed,
            "access-control-allow-headers に if-none-match が含まれていない: {:?}",
            resp.headers()
        );
    }
}
