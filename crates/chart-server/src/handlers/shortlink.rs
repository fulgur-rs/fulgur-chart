use crate::{backend::BackendError, render::OutputFormat, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use ulid::Ulid;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateRequest {
    /// Chart.js v4 spec as JSON object
    pub chart: Value,
    /// Width in pixels
    pub width: Option<u32>,
    /// Height in pixels
    pub height: Option<u32>,
    /// Background colour
    #[serde(rename = "backgroundColor")]
    pub background_color: Option<String>,
    /// Output format: `svg`, `png`, `webp`, `data-uri`
    #[serde(default)]
    pub format: OutputFormat,
}

#[utoipa::path(
    post,
    path = "/chart/create",
    request_body = CreateRequest,
    responses(
        (status = 200, description = "Short link created"),
        (status = 413, description = "Chart payload is too large for a short link"),
        (status = 503, description = "Shortlink store is full"),
    ),
    tag = "chart"
)]
pub async fn post_create(
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> Response {
    let json = req.chart.to_string();
    let id = Ulid::new().to_string();

    let query = build_query(
        &json,
        req.width,
        req.height,
        req.background_color.as_deref(),
        req.format,
    );
    let url = format!("/chart/s/{id}");

    match state.store.insert(id, query).await {
        Ok(()) => (StatusCode::OK, Json(json!({"url": url}))).into_response(),
        // 単一ペイロードが per-entry 上限超過: 再送しても無駄なので 413。
        Err(BackendError::TooLarge) => (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "chart payload is too large for a short link",
                "code": "PAYLOAD_TOO_LARGE"
            })),
        )
            .into_response(),
        // 件数 or 集約バイトが満杯: 一時的な拒否なので 503。
        Err(BackendError::Full) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "shortlink store is full",
                "code": "STORE_FULL"
            })),
        )
            .into_response(),
        // durable backend の一時障害: 503（in-memory では発生しない）。
        Err(BackendError::Unavailable(err)) => {
            eprintln!("Shortlink backend unavailable (create): {err}");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "shortlink backend unavailable",
                    "code": "BACKEND_UNAVAILABLE"
                })),
            )
                .into_response()
        }
    }
}

/// negative-cache 無効化用の `Cache-Control: no-store`。
/// 404/503 いずれも一時的・可変な状態を表すため、前段 CDN に
/// キャッシュさせてはならない。
fn no_store_cache_control() -> HeaderValue {
    HeaderValue::from_static("no-store")
}

pub async fn get_shortlink(Path(id): Path<String>, State(state): State<AppState>) -> Response {
    match state.store.get(&id).await {
        Ok(Some(query)) => {
            let mut resp = Redirect::temporary(&format!("/chart?{query}")).into_response();
            resp.headers_mut().insert(
                header::CACHE_CONTROL,
                format!("public, max-age={}", state.shortlink_ttl_seconds)
                    .parse()
                    .unwrap(),
            );
            resp
        }
        Ok(None) => {
            let mut resp = (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "short link not found",
                    "code": "NOT_FOUND"
                })),
            )
                .into_response();
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, no_store_cache_control());
            resp
        }
        // durable backend の一時障害: 503（in-memory では発生しない）。
        Err(err) => {
            eprintln!("Shortlink backend unavailable (resolve): {err}");
            let mut resp = (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "error": "shortlink backend unavailable",
                    "code": "BACKEND_UNAVAILABLE"
                })),
            )
                .into_response();
            resp.headers_mut()
                .insert(header::CACHE_CONTROL, no_store_cache_control());
            resp
        }
    }
}

fn build_query(
    json: &str,
    w: Option<u32>,
    h: Option<u32>,
    bkg: Option<&str>,
    fmt: OutputFormat,
) -> String {
    let encoded = urlencoding::encode(json);
    let fmt_encoded = urlencoding::encode(fmt.as_str());
    let mut q = format!("c={encoded}&f={fmt_encoded}");
    if let Some(w) = w {
        q.push_str(&format!("&w={w}"));
    }
    if let Some(h) = h {
        q.push_str(&format!("&h={h}"));
    }
    if let Some(bkg) = bkg {
        q.push_str(&format!("&bkg={}", urlencoding::encode(bkg)));
    }
    q
}

/// HTTP レベルの統合テスト: `/chart/create` の handler→store マッピング
/// （200 / 413 PAYLOAD_TOO_LARGE / 503 STORE_FULL）が配線として壊れないことを保証する。
#[cfg(test)]
mod http_tests {
    use crate::config::Config;
    use crate::render::Compression;
    use crate::server::build_router;
    use crate::store::ShortlinkStore;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        response::Response,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    /// 指定したストア上限で `/chart/create` を叩ける router を組む。
    /// バイト/件数上限のテストのため store だけ呼び出し側で構成する。
    fn router_with_store(store: ShortlinkStore) -> Router {
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 4,
            max_body_size: 102_400,
            render_timeout_ms: 1000,
            shortlink_limit: 100,
            shortlink_max_bytes: 128 * 1024 * 1024,
            shortlink_entry_bytes: 512 * 1024,
            shortlink_ttl_seconds: 86_400,
            cors_origins: "*".into(),
            rate_limit: 0,
            log_level: "info".into(),
            png_compression: Compression::default(),
            webp_enabled: false,
            max_webp_area: fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS,
        };
        build_router(&cfg, Arc::new(store))
    }

    /// shortlink_ttl_seconds を明示的に指定できる router ヘルパー
    /// (config駆動であることをdefault値(86400)と異なる値で検証するため)。
    fn router_with_store_and_ttl(store: ShortlinkStore, ttl: u64) -> Router {
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 4,
            max_body_size: 102_400,
            render_timeout_ms: 1000,
            shortlink_limit: 100,
            shortlink_max_bytes: 128 * 1024 * 1024,
            shortlink_entry_bytes: 512 * 1024,
            shortlink_ttl_seconds: ttl,
            cors_origins: "*".into(),
            rate_limit: 0,
            log_level: "info".into(),
            png_compression: Compression::default(),
            webp_enabled: false,
            max_webp_area: fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS,
        };
        build_router(&cfg, Arc::new(store))
    }

    fn create_request(body: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/chart/create")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn status_and_body(resp: Response) -> (StatusCode, String) {
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// per-entry バイト上限を超える保存は 413 PAYLOAD_TOO_LARGE（DefaultBodyLimit ではなく
    /// store 由来であることをコードで確認）。
    #[tokio::test]
    async fn create_rejects_oversized_entry_with_413() {
        // entry_bytes=10。小さな valid body でも query が 10B を超えるため store が拒否。
        // body は 100KiB の DefaultBodyLimit を下回るので 413 は store 由来。
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 10);
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let resp = router_with_store(store)
            .oneshot(create_request(body))
            .await
            .unwrap();
        let (status, body) = status_and_body(resp).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "body={body}");
        assert!(body.contains("PAYLOAD_TOO_LARGE"), "body={body}");
    }

    /// 件数上限に達したら以後の create は 503 STORE_FULL。
    #[tokio::test]
    async fn create_returns_503_when_store_full() {
        let store = ShortlinkStore::new(1, 128 * 1024 * 1024, 512 * 1024);
        let router = router_with_store(store);

        let body1 = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let (status1, b1) =
            status_and_body(router.clone().oneshot(create_request(body1)).await.unwrap()).await;
        assert_eq!(status1, StatusCode::OK, "body={b1}");

        // ULID は非決定的なので spec に関わらず新規挿入だが、件数上限(1)に達しているため 503。
        let body2 =
            r#"{"chart":{"type":"line","data":{"labels":["B"],"datasets":[{"data":[2]}]}}}"#;
        let (status2, b2) =
            status_and_body(router.oneshot(create_request(body2)).await.unwrap()).await;
        assert_eq!(status2, StatusCode::SERVICE_UNAVAILABLE, "body={b2}");
        assert!(b2.contains("STORE_FULL"), "body={b2}");
    }

    /// 上限内なら 200 で /chart/s/{id} を返す。
    #[tokio::test]
    async fn create_succeeds_within_limits() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let resp = router_with_store(store)
            .oneshot(create_request(body))
            .await
            .unwrap();
        let (status, body) = status_and_body(resp).await;
        assert_eq!(status, StatusCode::OK, "body={body}");
        assert!(body.contains("/chart/s/"), "body={body}");
    }

    /// ULID は非決定的なので、同一 spec を連投しても別エントリ(別 URL)になる
    /// (content-hash 時代の dedup は意図的に失われる — 8tr.5 の受容済みトレードオフ)。
    #[tokio::test]
    async fn create_generates_distinct_ids_for_identical_specs() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let router = router_with_store(store);
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;

        let (status1, b1) =
            status_and_body(router.clone().oneshot(create_request(body)).await.unwrap()).await;
        assert_eq!(status1, StatusCode::OK, "body={b1}");

        let (status2, b2) =
            status_and_body(router.oneshot(create_request(body)).await.unwrap()).await;
        assert_eq!(status2, StatusCode::OK, "body={b2}");

        assert_ne!(
            b1, b2,
            "identical spec should produce distinct shortlink URLs (no dedup)"
        );
    }

    /// 返却される id は ULID の文字列表現: 26 文字の Crockford base32。
    #[tokio::test]
    async fn create_returns_url_with_26_char_ulid_id() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let resp = router_with_store(store)
            .oneshot(create_request(body))
            .await
            .unwrap();
        let (status, body) = status_and_body(resp).await;
        assert_eq!(status, StatusCode::OK, "body={body}");

        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let url = json["url"].as_str().unwrap();
        let id = url
            .strip_prefix("/chart/s/")
            .expect("url should be /chart/s/{id}");
        assert_eq!(
            id.len(),
            26,
            "ULID string repr should be 26 chars, got: {id}"
        );
        assert!(
            id.chars()
                .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c.to_ascii_uppercase())),
            "id should be valid Crockford base32: {id}"
        );
    }

    /// resolve成功時は Cache-Control: public, max-age=<config値> が付く。
    /// default(86400)と異なる値(3600)を使い、handlerがハードコードでなく
    /// config駆動であることを検証する。
    #[tokio::test]
    async fn resolve_success_sets_cache_control_from_config_ttl() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let router = router_with_store_and_ttl(store, 3600);

        let create_body =
            r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let (status, body) = status_and_body(
            router
                .clone()
                .oneshot(create_request(create_body))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "body={body}");
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let url = json["url"].as_str().unwrap().to_string();

        let resp = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(&url)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::TEMPORARY_REDIRECT,
            "headers={:?}",
            resp.headers()
        );
        assert_eq!(
            resp.headers().get("cache-control").unwrap(),
            "public, max-age=3600"
        );
    }

    /// 未検出(404)は Cache-Control: no-store。前段CDNのnegative-cacheが
    /// LBハズレ由来の一時的な404を永続化させないようにするため。
    #[tokio::test]
    async fn resolve_not_found_sets_no_store_cache_control() {
        let store = ShortlinkStore::new(100, 128 * 1024 * 1024, 512 * 1024);
        let router = router_with_store(store);

        let resp = router
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/chart/s/does-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
    }
}
