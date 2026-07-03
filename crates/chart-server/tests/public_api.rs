//! 公開 API が外部 crate と同じ経路(`chart_server::...` の再エクスポートのみ)で
//! 使えること、特に `ShortlinkBackend` を実装した任意の backend を
//! `build_router` に inject できることを検証する。OSS デフォルトの file backend
//! (`FileShortlinkStore`)では通常発生しない `Unavailable`/`Full` 経路(→ 503)も、
//! スタブ backend を注入して HTTP レベルで実際に網羅する。

use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    response::Response,
};
use chart_server::{BackendError, Config, FileShortlinkStore, ShortlinkBackend, build_router};
use clap::Parser;
use tower::ServiceExt;

/// 外部 crate 側で定義しうる最小の backend 実装(常に成功・空)。
struct NoopBackend;

#[async_trait]
impl ShortlinkBackend for NoopBackend {
    async fn insert(&self, _id: String, _query: String) -> Result<(), BackendError> {
        Ok(())
    }
    async fn get(&self, _id: &str) -> Result<Option<String>, BackendError> {
        Ok(None)
    }
}

/// 常に `Unavailable` を返す durable backend のスタブ(I/O 障害を模擬)。
struct UnavailableBackend;

#[async_trait]
impl ShortlinkBackend for UnavailableBackend {
    async fn insert(&self, _id: String, _query: String) -> Result<(), BackendError> {
        Err(BackendError::Unavailable("backend down".into()))
    }
    async fn get(&self, _id: &str) -> Result<Option<String>, BackendError> {
        Err(BackendError::Unavailable("backend down".into()))
    }
}

/// 常に `Full` を返す backend のスタブ（満杯状態を模擬）。
struct FullBackend;

#[async_trait]
impl ShortlinkBackend for FullBackend {
    async fn insert(&self, _id: String, _query: String) -> Result<(), BackendError> {
        Err(BackendError::Full)
    }
    async fn get(&self, _id: &str) -> Result<Option<String>, BackendError> {
        Ok(None)
    }
}

/// clap のデフォルト値で Config を構築(引数なし起動相当)。
fn default_config() -> Config {
    Config::parse_from(["chart-server"])
}

async fn status_and_body(resp: Response) -> (StatusCode, String) {
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// 受け入れ基準: 外部実装の backend も OSS デフォルトの file backend
/// (`FileShortlinkStore`)も同じ `build_router` シームに渡せる
/// (コンパイルが通ること自体が実証)。
#[tokio::test]
async fn external_backend_can_be_injected_into_build_router() {
    let cfg = default_config();
    let _router = build_router(&cfg, Arc::new(NoopBackend));
    // TempDir を変数に束縛して router 構築後も dir を生存させる（直接 .path() を渡すと
    // 一時 TempDir が文末で drop され dir が消える。store ヘルパの established pattern に揃える）。
    let dir = tempfile::tempdir().unwrap();
    let _router2 = build_router(
        &cfg,
        Arc::new(FileShortlinkStore::new(dir.path(), 256).await.unwrap()),
    );
}

/// backend が `Full` を返すと `POST /chart/create` は 503 STORE_FULL。
/// （このテストはスタブ backend で 503 経路を検証する。FileShortlinkStore も容量上限
/// 超過時に `Full` を返す。）
#[tokio::test]
async fn create_returns_503_store_full_when_backend_full() {
    let cfg = default_config();
    let router = build_router(&cfg, Arc::new(FullBackend));
    let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/chart/create")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, body) = status_and_body(router.oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert!(body.contains("STORE_FULL"), "body={body}");
}

/// backend が `Unavailable` を返すと `POST /chart/create` は 503 BACKEND_UNAVAILABLE。
#[tokio::test]
async fn create_returns_503_backend_unavailable_when_backend_errors() {
    let cfg = default_config();
    let router = build_router(&cfg, Arc::new(UnavailableBackend));
    let body = r#"{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}"#;
    let req = Request::builder()
        .method("POST")
        .uri("/chart/create")
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap();
    let (status, body) = status_and_body(router.oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert!(body.contains("BACKEND_UNAVAILABLE"), "body={body}");
}

/// backend が `Unavailable` を返すと `GET /chart/s/{id}` は 503 BACKEND_UNAVAILABLE。
#[tokio::test]
async fn resolve_returns_503_backend_unavailable_when_backend_errors() {
    let cfg = default_config();
    let router = build_router(&cfg, Arc::new(UnavailableBackend));
    let req = Request::builder()
        .method("GET")
        .uri("/chart/s/deadbeef")
        .body(Body::empty())
        .unwrap();
    let (status, body) = status_and_body(router.oneshot(req).await.unwrap()).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body={body}");
    assert!(body.contains("BACKEND_UNAVAILABLE"), "body={body}");
}

/// backend が `Unavailable` を返すときの 503 応答は Cache-Control: no-store
/// (一時障害を前段CDNに誤ってキャッシュさせないため)。
#[tokio::test]
async fn resolve_returns_no_store_cache_control_when_backend_unavailable() {
    let cfg = default_config();
    let router = build_router(&cfg, Arc::new(UnavailableBackend));
    let req = Request::builder()
        .method("GET")
        .uri("/chart/s/deadbeef")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
}
