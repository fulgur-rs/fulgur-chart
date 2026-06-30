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
        png_compression: cfg.png_compression,
        webp: crate::render::WebpPolicy {
            enabled: cfg.webp_enabled,
            max_area: cfg.max_webp_area,
        },
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

    use crate::render::Compression;

    fn restricted_cors_router() -> Router {
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 1,
            max_body_size: 102_400,
            render_timeout_ms: 1000,
            shortlink_limit: 100,
            shortlink_max_bytes: 128 * 1024 * 1024,
            shortlink_entry_bytes: 512 * 1024,
            cors_origins: "https://example.com".into(),
            rate_limit: 0,
            log_level: "info".into(),
            png_compression: Compression::default(),
            webp_enabled: false,
            max_webp_area: fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS,
        };
        build_router(
            &cfg,
            ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024),
        )
    }

    fn router_with_compression(compression: Compression) -> Router {
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 4,
            max_body_size: 102_400,
            render_timeout_ms: 5000,
            shortlink_limit: 100,
            shortlink_max_bytes: 128 * 1024 * 1024,
            shortlink_entry_bytes: 512 * 1024,
            cors_origins: "*".into(),
            rate_limit: 0,
            log_level: "info".into(),
            png_compression: compression,
            webp_enabled: false,
            max_webp_area: fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS,
        };
        build_router(
            &cfg,
            ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024),
        )
    }

    /// 指定した起動時設定 `png_compression` の下で PNG をレンダーし、バイト長を返す。
    /// 圧縮は per-request ではなくサーバ設定なので、リクエスト body には compression を付けない。
    async fn png_len_for_config(compression: Compression) -> usize {
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A","B","C","D"],"datasets":[{"data":[12,19,3,5]}]}},"format":"png"}"#;
        let resp = router_with_compression(compression)
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
        assert_eq!(resp.status(), 200, "{compression:?} は 200 を返すべき");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(&bytes[0..4], &[0x89, b'P', b'N', b'G'], "PNG を返すべき");
        bytes.len()
    }

    /// 起動時設定 png_compression がエンドツーエンドで効くこと。
    /// High は Fast より小さい PNG を返さなければならない(本機能の目的)。
    #[tokio::test]
    async fn compression_config_high_yields_smaller_png_than_fast() {
        let fast = png_len_for_config(Compression::Fast).await;
        let high = png_len_for_config(Compression::High).await;
        assert!(
            high < fast,
            "High ({high}B) は Fast ({fast}B) より小さい PNG を返すべき"
        );
    }

    /// 既定の起動時設定は Balanced であり、Fast 以下のサイズになること。
    /// (`#[serde(default)]` ではなく clap の既定値 + enum 既定が真実源)
    #[tokio::test]
    async fn default_compression_config_is_balanced() {
        assert_eq!(Compression::default(), Compression::Balanced);
        let fast = png_len_for_config(Compression::Fast).await;
        let default = png_len_for_config(Compression::default()).await;
        assert!(default <= fast, "既定(Balanced) は Fast 以下のサイズ");
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

    /// WebP 無効サーバでは、ETag にマッチする条件付き要求でも 304 ではなく 415 を返す
    /// （フォーマット可用性は描画を伴わない 304 短絡より優先。Codex review 対応）。
    /// 同条件の PNG は precheck 対象外で 304 のままになることも確認する。
    #[tokio::test]
    async fn webp_disabled_returns_415_even_with_matching_etag() {
        let mk = |fmt: &str| {
            let body = format!(
                r#"{{"chart":{{"type":"bar","data":{{"labels":["A"],"datasets":[{{"data":[1]}}]}}}},"format":"{fmt}"}}"#
            );
            Request::builder()
                .method("POST")
                .uri("/chart")
                .header("content-type", "application/json")
                .header("if-none-match", "*") // どの ETag にもマッチ → 304 経路を強制
                .body(Body::from(body))
                .unwrap()
        };

        // router_with_compression は webp_enabled=false（既定 disable）。
        let webp = router_with_compression(Compression::default())
            .oneshot(mk("webp"))
            .await
            .unwrap();
        assert_eq!(
            webp.status(),
            415,
            "WebP 無効時は 304 でなく 415 を返すべき"
        );

        // PNG は precheck 対象外なので If-None-Match:* で従来どおり 304。
        let png = router_with_compression(Compression::default())
            .oneshot(mk("png"))
            .await
            .unwrap();
        assert_eq!(
            png.status(),
            304,
            "PNG は If-None-Match:* で 304 を返すべき"
        );
    }
}
