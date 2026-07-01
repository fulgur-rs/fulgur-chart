use crate::{backend::BackendError, render::OutputFormat, state::AppState};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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

fn compute_id(
    json: &str,
    fmt: &str,
    width: Option<u32>,
    height: Option<u32>,
    background_color: Option<&str>,
) -> String {
    // ID はチャート JSON + レンダーパラメータ全体のハッシュ（同スペックで異なるサイズ/フォーマットは別リンク）
    // "_" を番兵として None と Some("") を区別する。
    // ハッシュは 6 bytes（48bit）: 10000件時の誕生日衝突確率 < 0.001%。
    let id_input = format!(
        "{json}\x00{fmt}\x00{}\x00{}\x00{}",
        width.map_or_else(|| "_".to_string(), |v| v.to_string()),
        height.map_or_else(|| "_".to_string(), |v| v.to_string()),
        background_color.unwrap_or("_"),
    );
    let hash = Sha256::digest(id_input.as_bytes());
    hex::encode(&hash[..6])
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
    let id = compute_id(
        &json,
        req.format.as_str(),
        req.width,
        req.height,
        // "_" を番兵として None と Some("") を区別する（Some("") は空文字列をそのまま使用）。
        req.background_color.as_deref(),
    );

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

pub async fn get_shortlink(Path(id): Path<String>, State(state): State<AppState>) -> Response {
    match state.store.get(&id).await {
        Ok(Some(query)) => Redirect::temporary(&format!("/chart?{query}")).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "short link not found",
                "code": "NOT_FOUND"
            })),
        )
            .into_response(),
        // durable backend の一時障害: 503（in-memory では発生しない）。
        Err(err) => {
            eprintln!("Shortlink backend unavailable (resolve): {err}");
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_and_empty_string_background_produce_different_ids() {
        let json = r#"{"type":"bar"}"#;
        let id_none = compute_id(json, "svg", None, None, None);
        let id_empty = compute_id(json, "svg", None, None, Some(""));
        assert_ne!(
            id_none, id_empty,
            "None と Some(\"\") は異なるハッシュになるべき"
        );
    }

    #[test]
    fn same_params_produce_same_id() {
        let json = r#"{"type":"bar"}"#;
        let id1 = compute_id(json, "svg", Some(800), Some(600), Some("white"));
        let id2 = compute_id(json, "svg", Some(800), Some(600), Some("white"));
        assert_eq!(id1, id2);
    }
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

        // 別スペック → 別 id → 新規挿入だが件数上限(1)に達しているため 503。
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
}
