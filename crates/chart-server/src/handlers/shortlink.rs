use crate::handlers::chart::{ChartQuery, render_from_query};
use crate::{backend::BackendError, render::OutputFormat, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
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
        // 満杯（件数/集約バイト上限）: 一時的な拒否なので 503。FileShortlinkStore は
        // 容量上限超過時に返す（inline sweep 後もなお満杯なら。次 sweep で自己回復）。
        Err(BackendError::Full) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "shortlink store is full",
                "code": "STORE_FULL"
            })),
        )
            .into_response(),
        // durable backend の一時障害: 503（FileShortlinkStore は I/O 失敗時に返す）。
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

pub async fn get_shortlink(
    Path(id): Path<String>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> Response {
    match state.store.get(&id).await {
        Ok(Some(query)) => {
            // store の query 文字列を GET /chart と同一の意味論で ChartQuery に
            // デコードする。axum の `Query::try_from_uri` は内部で
            // `serde_urlencoded::Deserializer::new(form_urlencoded::parse(..))` を使うので、
            // ここで `serde_urlencoded::from_str` を直接呼ぶのと成功時は等価
            // (axum は serde_path_to_error でエラー文言を richにするだけ)。
            // 重要: `http::Uri` を経由しない。Uri は内部 offset が u16 のため
            // MAX_LEN=65534 の長さ上限を持ち、create の受理域(body 100KiB /
            // entry_bytes 512KiB)まで通る大 spec を resolve で 414/500 に落として
            // しまう。render-by-id は「spec を URL に載せない」のが要件なので、
            // query 文字列を直接 parse して長さ上限を持ち込まない。
            let q = match serde_urlencoded::from_str::<ChartQuery>(&query) {
                Ok(q) => q,
                Err(_) => return internal_error_no_store(),
            };
            let mut resp = render_from_query(q, headers, state.clone()).await;
            // 成功(200)と 304(条件付き再検証)だけを CDN キャッシュ可能にし、
            // max-age を shortlink TTL に結合する(8tr.3)。エラー(400/415/500/503/504)は
            // no-store で、前段 CDN が誤って永続化しないようにする。
            let cc = if resp.status().is_success() || resp.status() == StatusCode::NOT_MODIFIED {
                state.shortlink_cache_control.clone()
            } else {
                no_store_cache_control()
            };
            resp.headers_mut().insert(header::CACHE_CONTROL, cc);
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
        // durable backend の一時障害: 503（FileShortlinkStore は I/O 失敗時に返す）。
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

/// store に入っている query 文字列が壊れていて parse 不能な場合の 500。
/// 自前 write なので通常到達しないが、防御的に no-store で返す。
fn internal_error_no_store() -> Response {
    let mut resp = (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "error": "corrupt short link entry",
            "code": "INTERNAL"
        })),
    )
        .into_response();
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, no_store_cache_control());
    resp
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
/// （200 / 413 PAYLOAD_TOO_LARGE）が配線として壊れないことを保証する。
/// 503 BACKEND_UNAVAILABLE 経路は `tests/public_api.rs` がスタック backend で網羅する。
#[cfg(test)]
mod http_tests {
    use crate::config::Config;
    use crate::file_store::FileShortlinkStore;
    use crate::render::Compression;
    use crate::server::build_router;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        response::Response,
    };
    use std::sync::Arc;
    use tower::ServiceExt;

    /// 指定した entry_bytes で tempdir-backed FileShortlinkStore を組んだ router。
    /// per-entry バイト上限のテストのため entry_bytes を呼び出し側で指定する。
    async fn router_with_entry_bytes(entry_bytes: usize) -> Router {
        router_with_entry_bytes_and_ttl(entry_bytes, 86_400).await
    }

    /// entry_bytes と shortlink_ttl_seconds を明示指定できる router ヘルパー
    /// (config駆動であることをdefault値(86400)と異なる値で検証するため)。
    async fn router_with_entry_bytes_and_ttl(entry_bytes: usize, ttl: u64) -> Router {
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let store = FileShortlinkStore::new(dir.path(), entry_bytes)
            .await
            .unwrap();
        let cfg = Config {
            host: "0.0.0.0".into(),
            port: 3000,
            max_concurrent: 4,
            max_body_size: 102_400,
            render_timeout_ms: 1000,
            shortlink_entry_bytes: entry_bytes,
            shortlink_dir: "unused".into(),
            shortlink_ttl_seconds: ttl,
            shortlink_max_bytes: 0, // テストは無制限（容量テストは file_store 側で実施）
            shortlink_max_entries: 0,
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
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let resp = router_with_entry_bytes(10)
            .await
            .oneshot(create_request(body))
            .await
            .unwrap();
        let (status, body) = status_and_body(resp).await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "body={body}");
        assert!(body.contains("PAYLOAD_TOO_LARGE"), "body={body}");
    }

    /// 上限内なら 200 で /chart/s/{id} を返す。
    #[tokio::test]
    async fn create_succeeds_within_limits() {
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let resp = router_with_entry_bytes(512 * 1024)
            .await
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
        let router = router_with_entry_bytes(512 * 1024).await;
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
        let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
        let resp = router_with_entry_bytes(512 * 1024)
            .await
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
        let router = router_with_entry_bytes_and_ttl(512 * 1024, 3600).await;

        // 明示的に format:svg を要求する。OutputFormat の default は Png のため、
        // resolve のレンダ済み content-type を SVG に固定するには create 時に指定する。
        let create_body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}},"format":"svg"}"#;
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

        // render-by-id: リダイレクトではなく 200 でレンダリング済み SVG を返す。
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "headers={:?}",
            resp.headers()
        );
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "image/svg+xml; charset=utf-8"
        );
        // Cache-Control は shortlink TTL(config 3600)に結合(8tr.3)。
        assert_eq!(
            resp.headers().get("cache-control").unwrap(),
            "public, max-age=3600"
        );
        // ボディは非空のレンダリング済み SVG。
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(!bytes.is_empty(), "resolved SVG body must be non-empty");
        assert!(
            bytes.starts_with(b"<svg") || bytes.starts_with(b"<?xml"),
            "body should be SVG, got: {:?}",
            String::from_utf8_lossy(&bytes[..bytes.len().min(64)])
        );
    }

    /// 未検出(404)は Cache-Control: no-store。前段CDNのnegative-cacheが
    /// LBハズレ由来の一時的な404を永続化させないようにするため。
    #[tokio::test]
    async fn resolve_not_found_sets_no_store_cache_control() {
        let router = router_with_entry_bytes(512 * 1024).await;

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

/// build_query が作る query 文字列を、GET /chart と同一の抽出器
/// (`Query::try_from_uri`)で parse し直したとき、c/f/w/h/bkg が原値復元される
/// ことを固定する。resolve の render-by-id 化はこの往復に依存する(advisor 指摘の盲点)。
#[cfg(test)]
mod roundtrip_tests {
    use super::build_query;
    use crate::handlers::chart::ChartQuery;
    use crate::render::OutputFormat;
    use axum::{extract::Query, http::Uri};

    #[test]
    fn build_query_round_trips_all_fields_and_formats() {
        // 構造文字 { } " : , / 空白 / 非ASCII(日本語) / + と & を値に含む。
        // + と & は form-encoding の古典的な罠(percent-encode されていれば復元される)。
        let spec = r#"{"type":"bar","data":{"labels":["a b","日本語","x+y&z"],"datasets":[{"data":[1,2,3]}]}}"#;

        for fmt in [
            OutputFormat::Svg,
            OutputFormat::Png,
            OutputFormat::Webp,
            OutputFormat::DataUri,
        ] {
            let q = build_query(spec, Some(640), Some(360), Some("hot+pink & white"), fmt);
            let uri: Uri = format!("/chart?{q}")
                .parse()
                .expect("query must form a valid URI");
            let Query(parsed) = Query::<ChartQuery>::try_from_uri(&uri)
                .expect("stored query must parse as ChartQuery");

            assert_eq!(
                parsed.c.as_deref(),
                Some(spec),
                "spec round-trip failed for {fmt:?}"
            );
            assert_eq!(parsed.w, Some(640), "w round-trip failed for {fmt:?}");
            assert_eq!(parsed.h, Some(360), "h round-trip failed for {fmt:?}");
            assert_eq!(
                parsed.bkg.as_deref(),
                Some("hot+pink & white"),
                "bkg round-trip failed for {fmt:?}"
            );
            assert_eq!(parsed.f, fmt, "format round-trip failed for {fmt:?}");
        }
    }

    /// None の w/h/bkg は query に出ず、parse 後も None のまま(Some("") にならない)。
    #[test]
    fn build_query_omits_absent_optionals() {
        let spec = r#"{"type":"bar"}"#;
        let q = build_query(spec, None, None, None, OutputFormat::Svg);
        let uri: Uri = format!("/chart?{q}").parse().unwrap();
        let Query(parsed) = Query::<ChartQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(parsed.c.as_deref(), Some(spec));
        assert_eq!(parsed.w, None);
        assert_eq!(parsed.h, None);
        assert_eq!(parsed.bkg, None);
        assert_eq!(parsed.f, OutputFormat::Svg);
    }
}
